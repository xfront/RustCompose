//! Web (wasm32-unknown-unknown) 平台入口 — 复用 winit web 后端与共享内核。
//!
//! 非阻塞：`run_app` 内部走 winit `spawn_app`，main 立即返回，浏览器 rAF 保活。

#[cfg(target_arch = "wasm32")]
pub use crate::render::{App, init_text_context, run_app};

/// 非 wasm 目标的编译兜底（本模块只服务于 wasm32）。
#[cfg(not(target_arch = "wasm32"))]
pub fn run_app(_app: impl crate::render::App + 'static) -> ! {
    panic!("compose-platform/web: 请用 `--target wasm32-unknown-unknown` 构建")
}
