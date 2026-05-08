//! Desktop 平台入口（winit 事件循环；由 `crate::render::host` 提供 `run_app`）。

pub use crate::render::{App, WindowConfig, init_text_context, run_app, run_app_with};
