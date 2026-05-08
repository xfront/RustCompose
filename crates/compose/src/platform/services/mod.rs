//! 跨平台平台服务层 — 对齐 Flutter `services` 绑定（`ServicesBinding`）。
//!
//! 把各后端的平台能力收敛为**统一 Rust API**，业务侧无需关心目标平台：
//!
//! | 能力 | 模块 | 对齐 Flutter |
//! |------|------|---------------------|
//! | 平台通道（Rust ↔ 宿主） | [`channel`] | `MethodChannel` |
//! | 剪贴板 | [`clipboard`] | `Clipboard` |
//! | 权限 | [`permission`] | `permission_handler` |
//! | 文件对话框 | [`file_dialog`] | （必备能力，桌面 rfd） |
//! | 应用生命周期 | [`lifecycle`] | `AppLifecycleState` |
//! | 统一错误 | [`error`] | `PlatformException` |
//!
//! # 平台覆盖（接缝策略）
//! - **Desktop**：剪贴板/文件对话框真实后端需 `arboard`/`rfd`（待接入，统一经
//!   `channel` 由宿主或桌面 hanlder 提供）；权限视为已授权；生命周期由事件循环驱动。
//! - **Web**：经 `channel` 宿主桥接（剪贴板异步读取受限，返回 unavailable）。
//! - **Android / iOS / OHOS**：经 `channel` 交给各自原生宿主实现。
//!
//! ```rust
//! use compose::platform::services::{clipboard, channel, lifecycle, file_dialog, permission};
//!
//! clipboard::set_text("hello").ok();
//! channel::set_method_call_handler(Box::new(|m, _a| {
//!     if m == "clipboardGet" { Some("hello".into()) } else { None }
//! }));
//! lifecycle::add_observer(Box::new(|s| eprintln!("lifecycle {:?}", s)));
//! let _ = file_dialog::open_file("Open", &[]);
//! let _ = permission::request(permission::Permission::Camera);
//! ```

pub mod channel;
pub mod clipboard;
pub mod error;
pub mod file_dialog;
pub mod lifecycle;
pub mod permission;
