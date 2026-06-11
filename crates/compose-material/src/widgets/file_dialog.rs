//! 系统文件对话框（第一阶段仅 desktop，基于 `rfd` 原生对话框）。
//!
//! 对齐 rui 的做法：原生对话框（`rfd::FileDialog::pick_file()`）是**阻塞**调用，故在后台线程
//! 执行，结果经「主线程任务队列」(`compose::runtime::post_to_main`) 回传主线程，再调用调用方
//! 提供的回调——等价于 rui `tokio::task::spawn_blocking` 后把结果送回 GUI 线程。
//! 非桌面平台（`wasm/android/ios/ohos`）暂未实现：回调直接收到 `None`，后续经平台通道对接宿主。

use std::path::PathBuf;

/// 文件过滤器（跨平台共用）。
#[derive(Debug, Clone, PartialEq)]
pub struct FileFilter {
    /// 过滤器显示名（如 `"图片"`）。
    pub name: String,
    /// 扩展名列表（不含点，如 `["png", "jpg"]`）。
    pub extensions: Vec<String>,
}

impl FileFilter {
    /// 构造过滤器。
    pub fn new(name: &str, extensions: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// 桌面端阻塞式取文件（运行于后台线程，不阻塞 GUI 事件循环）。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(target_env = "ohos")
))]
fn pick_file_blocking(title: &str, filters: &[FileFilter]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    for f in filters {
        let exts: Vec<&str> = f.extensions.iter().map(|s| s.as_str()).collect();
        dialog = dialog.add_filter(&f.name, &exts);
    }
    dialog.pick_file()
}

/// 打开单个文件（非阻塞）：在后台线程弹出原生对话框，结果通过 `on_result` 回调回传**主线程**。
///
/// `on_result` 收到的 `Option<PathBuf>`：选中为 `Some(path)`，取消 / 未实现为 `None`。
/// 回调内可安全调用 `MutableState::set`（已确保运行在主线程运行时里）。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(target_env = "ohos")
))]
pub fn open_file_dialog(
    title: &str,
    filters: &[FileFilter],
    on_result: impl FnOnce(Option<PathBuf>) + Send + 'static,
) {
    let title = title.to_string();
    let filters: Vec<FileFilter> = filters.to_vec();
    std::thread::spawn(move || {
        let result = pick_file_blocking(&title, &filters);
        // 结果回传主线程（避免跨线程写运行时状态）。
        compose::runtime::post_to_main(move || on_result(result));
    });
}

/// 非桌面平台占位：当前未实现，直接在主线程回调 `None`（不弹窗）。
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_env = "ohos"
))]
pub fn open_file_dialog(
    _title: &str,
    _filters: &[FileFilter],
    on_result: impl FnOnce(Option<PathBuf>) + Send + 'static,
) {
    compose::runtime::post_to_main(move || on_result(None));
}

/// 便捷封装：打开「图片」文件对话框（png/jpg/jpeg/gif/bmp/webp），结果经 `on_result` 回传主线程。
pub fn open_image_dialog(on_result: impl FnOnce(Option<PathBuf>) + Send + 'static) {
    open_file_dialog(
        "打开图片",
        &[FileFilter::new(
            "图片",
            &["png", "jpg", "jpeg", "gif", "bmp", "webp"],
        )],
        on_result,
    )
}
