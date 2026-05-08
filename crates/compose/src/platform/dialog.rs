//! 原生平台对话框入口（重导出 `services::file_dialog`）。
//!
//! 文件打开/保存/文件夹选择统一经 [`services::file_dialog`] 提供；本模块仅做
//! 顶层重导出，业务侧可直接 `use compose::platform::Dialog::*`。
//!
//! 这些函数是**阻塞**的，应通过 `crate::render` 的异步桥在异步任务中调用，
//! 避免阻塞 GUI 事件循环。

pub use crate::platform::services::file_dialog::{
    FileFilter, open_file, open_files, open_folder, save_file,
};
