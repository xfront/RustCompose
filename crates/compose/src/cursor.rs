//! 平台无关的光标形状请求 — 控件/手势层登记期望光标，宿主映射到 OS 光标。
//!
//! 核心不依赖 winit 的光标类型（layout/material 需跨平台），故此处只定义中性枚举 +
//! 线程局部「当前期望光标」。宿主每帧 [`take_cursor_change`] 取出**变化**并调用
//! `Window::set_cursor`；未变化时返回 `None`，避免每帧重复系统调用。
//!
//! 约定：悬停进入设形状、离开回 [`Cursor::Default`]；拖拽开始设形状、结束/取消回 Default。

use std::cell::Cell;

/// 中性光标形状（宿主负责映射到 winit `CursorIcon` / 平台等价物）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Pointer,
    Text,
    /// 移动 / 拖拽整窗。
    Move,
    /// 可抓握（拖拽撕离标签）。
    Grabbing,
    /// 水平双箭头（左右分割 / 浮动窗左右边）。
    ResizeEw,
    /// 垂直双箭头（上下分割 / 浮动窗上下边）。
    ResizeNs,
    /// 主对角双箭头（左上 / 右下角）。
    ResizeNwse,
    /// 副对角双箭头（右上 / 左下角）。
    ResizeNesw,
    NotAllowed,
}

thread_local! {
    static WANT: Cell<Cursor> = const { Cell::new(Cursor::Default) };
    static APPLIED: Cell<Cursor> = const { Cell::new(Cursor::Default) };
}

/// 登记期望光标（悬停 / 手势层调用）。
pub fn set_cursor(c: Cursor) {
    WANT.with(|w| w.set(c));
}

/// 当前期望光标。
pub fn current_cursor() -> Cursor {
    WANT.with(|w| w.get())
}

/// 取出相对上次应用的变化；无变化返回 `None`（宿主据此决定是否调 OS API）。
pub fn take_cursor_change() -> Option<Cursor> {
    let want = WANT.with(|w| w.get());
    let applied = APPLIED.with(|a| a.get());
    if want == applied {
        None
    } else {
        APPLIED.with(|a| a.set(want));
        Some(want)
    }
}