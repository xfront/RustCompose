//! 输入路由反转表 — 解开「render（帧内核 / 宿主）↔ material（文本控件）」的循环依赖。
//!
//! 合并进 `compose` 后，`crate::render::{kernel,host}` 需要把键盘 / 字符 / IME /
//! 剪贴板 / 滚动等输入事件转发给当前聚焦的文本控件（这些处理函数住在
//! `compose-material`）。但核心不能编译期依赖 material（否则成环），故此处以
//! **函数指针槽**登记一组 handler：material 在 app bootstrap 调
//! `compose_material::install_router()` 把自己的实现塞进来，核心只经本模块转发。
//!
//! 约定（对齐全库「零 Rc/Arc/unsafe」）：handler 全为无状态 `fn` 指针，注册表是
//! 线程局部 `Option<Handlers>`；**未注册时所有转发安全退化为 no-op**（`focused()`
//! 返回 `false`），纯渲染 / 无文本控件的场景不因此改变行为。

use crate::runtime::NavKey;

/// 一组输入转发函数指针（由持有文本控件实现的 crate 提供）。
#[derive(Clone, Copy)]
pub struct Handlers {
    pub focused: fn() -> bool,
    pub char: fn(char),
    pub backspace: fn(),
    pub key: fn(NavKey),
    pub commit: fn(&str),
    pub preedit: fn(&str),
    pub enter: fn(),
    pub copy: fn(),
    pub cut: fn(),
    pub paste: fn(),
    pub select_all: fn(),
    pub shift: fn(bool),
    pub scroll: fn(f32),
    /// 撤销（Ctrl+Z）。
    pub undo: fn(),
    /// 重做（Ctrl+Y / Ctrl+Shift+Z）。
    pub redo: fn(),
    /// 查找下一个匹配（F3）。
    pub find_next: fn(),
}

thread_local! {
    static HANDLERS: core::cell::Cell<Option<&'static Handlers>> = const { core::cell::Cell::new(None) };
}

/// 登记实现（`'static` 借用：调用方应以 `Box::leak` 或 `static` 提供长生命周期 `Handlers`）。
pub fn install(h: &'static Handlers) {
    HANDLERS.with(|c| c.set(Some(h)));
}

fn with<R>(f: impl Fn(&Handlers) -> R, none: R) -> R {
    match HANDLERS.with(|c| c.get()) {
        Some(h) => f(h),
        None => none,
    }
}

/// 当前是否有聚焦文本控件（未登记 → `false`）。
pub fn focused() -> bool {
    with(|h| (h.focused)(), false)
}

pub fn char(ch: char) {
    with(|h| (h.char)(ch), ())
}

pub fn backspace() {
    with(|h| (h.backspace)(), ())
}

pub fn key(k: NavKey) {
    with(|h| (h.key)(k), ())
}

pub fn commit(s: &str) {
    with(|h| (h.commit)(s), ())
}

pub fn preedit(s: &str) {
    with(|h| (h.preedit)(s), ())
}

pub fn enter() {
    with(|h| (h.enter)(), ())
}

pub fn copy() {
    with(|h| (h.copy)(), ())
}

pub fn cut() {
    with(|h| (h.cut)(), ())
}

pub fn paste() {
    with(|h| (h.paste)(), ())
}

pub fn select_all() {
    with(|h| (h.select_all)(), ())
}

pub fn shift(on: bool) {
    with(|h| (h.shift)(on), ())
}

pub fn scroll(dy: f32) {
    with(|h| (h.scroll)(dy), ())
}

/// 撤销（未登记 → no-op）。
pub fn undo() {
    with(|h| (h.undo)(), ())
}

/// 重做（未登记 → no-op）。
pub fn redo() {
    with(|h| (h.redo)(), ())
}

/// 查找下一个匹配（未登记 → no-op）。
pub fn find_next() {
    with(|h| (h.find_next)(), ())
}
