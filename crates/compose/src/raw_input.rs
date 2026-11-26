//! 原始窗口输入「分流」钩子（附加能力，默认无操作）。
//!
//! 框架默认把鼠标 / 滚轮 / 键盘事件喂给内核做本地 UI 手势与文本编辑（见
//! [`crate::render::host`] 的 `window_event`）。远程桌面类应用需要拿到**未被裁剪的**
//! 原始事件（区分左右中键、含按下与抬起、滚轮 x/y、键盘含抬起与修饰键），据此经
//! DataChannel 转发到远端主机。本模块提供一个**按窗口注册**的可空回调：宿主在
//! 默认路由之前先把事件交给已注册的处理器，处理器返回 `true` 表示「已消费、跳过
//! 默认路由」，返回 `false` 则照常走本地 UI（例如工具栏按钮）。未注册时全部为
//! no-op，不改变既有行为。

use crate::render::host::WindowHandle;
use std::cell::RefCell;

/// 一次原始按键事件（`logical_key` 已字符串化，供上层映射为远端虚拟键码）。
#[derive(Clone, Debug)]
pub struct RawKeyEvent {
    /// `true` = 按下，`false` = 抬起。
    pub press: bool,
    /// winit `Key` 的可读字符串：字符键给出该字符（Shift 反映为大小写），
    /// 命名键给出名称（如 `"Enter"` / `"ArrowUp"` / `"Backspace"` / `"F1"` / `" "`）。
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// 原始窗口输入事件（坐标为物理像素，与 `inner_size` 同一坐标空间）。
#[derive(Clone, Debug)]
pub enum RawEvent {
    Move { x: f32, y: f32 },
    /// `button`：0=左，1=中，2=右，3=其他。
    Press { x: f32, y: f32, button: u8 },
    Release { x: f32, y: f32, button: u8 },
    Wheel { x: f32, y: f32, dx: f32, dy: f32 },
    Key(RawKeyEvent),
}

/// 处理器：`(事件, 窗口内容宽, 窗口内容高) -> 是否已消费`。
pub type RawHandler = Box<dyn FnMut(&RawEvent, f32, f32) -> bool>;

thread_local! {
    static HANDLERS: RefCell<Vec<(WindowHandle, RawHandler)>> = const { RefCell::new(Vec::new()) };
}

/// 为某窗口登记原始输入处理器（同窗口重复登记覆盖）。仅主线程。
pub fn install(handle: WindowHandle, handler: impl FnMut(&RawEvent, f32, f32) -> bool + 'static) {
    let boxed: RawHandler = Box::new(handler);
    HANDLERS.with(|reg| {
        let mut reg = reg.borrow_mut();
        if let Some(i) = reg.iter().position(|(id, _)| id == &handle) {
            reg[i].1 = boxed;
        } else {
            reg.push((handle, boxed));
        }
    });
}

/// 注销某窗口的处理器（关窗时调用，避免句柄复用后串窗）。
pub fn uninstall(handle: WindowHandle) {
    HANDLERS.with(|reg| reg.borrow_mut().retain(|(id, _)| id != &handle));
}

/// 宿主在默认路由前调用：命中处理器且其返回 `true` → 事件已被消费。
pub(crate) fn dispatch(handle: &WindowHandle, ev: &RawEvent, w: f32, h: f32) -> bool {
    // take-put 防重入（处理器内部可能再 install / uninstall）。
    let mut slot: Option<RawHandler> = HANDLERS.with(|reg| {
        let mut reg = reg.borrow_mut();
        let i = reg.iter().position(|(id, _)| id == handle)?;
        Some(std::mem::replace(&mut reg[i].1, Box::new(|_, _, _| false)))
    });
    let Some(mut f) = slot.take() else {
        return false;
    };
    let consumed = f(ev, w, h);
    HANDLERS.with(|reg| {
        let mut reg = reg.borrow_mut();
        if let Some(i) = reg.iter().position(|(id, _)| id == handle) {
            reg[i].1 = f;
        }
    });
    consumed
}
