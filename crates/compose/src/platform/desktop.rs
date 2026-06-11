//! Desktop 平台入口（winit 事件循环；由 `crate::render::host` 提供 `run_app`）。

pub use crate::render::{
    close_window, current_window, init_text_context, open_window, run_app, run_app_with,
    window_count, App, WindowConfig, WindowHandle,
};
