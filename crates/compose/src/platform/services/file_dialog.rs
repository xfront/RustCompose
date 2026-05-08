//! 原生文件对话框（桌面端 rfd，非桌面 unsupported）。
//!
//! 提供纯 Rust API：`FileFilter` + `open_file` / `open_files` / `save_file`
//! / `open_folder`。桌面端用 [`rfd`] 调用系统原生对话框（真实实现）；非桌面平台
//! 返回 `None` / 空 `Vec`（移动端/Web 由宿主 `channel` 提供导入/导出能力）。
//!
//! 这些函数是**阻塞**的，业务侧应经 `crate::render` 的异步桥在异步任务里调用，
//! 避免阻塞 GUI 事件循环。

/// 文件过滤器（定义见本模块，跨平台共用）。
#[derive(Debug, Clone)]
pub struct FileFilter {
    /// 过滤器显示名（如 "图片"）。
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

// ── Desktop（rfd，真实系统对话框）──────────────────────────────────────────
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(target_env = "ohos")
))]
fn build_dialog(title: &str, filters: &[FileFilter]) -> rfd::FileDialog {
    let mut Dialog = rfd::FileDialog::new().set_title(title);
    for f in filters {
        Dialog = Dialog.add_filter(&f.name, &f.extensions);
    }
    Dialog
}

/// 打开单个文件。选中返回路径，取消返回 `None`。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(target_env = "ohos")
))]
pub fn open_file(title: &str, filters: &[FileFilter]) -> Option<String> {
    build_dialog(title, filters)
        .pick_file()
        .map(|p| p.to_string_lossy().into_owned())
}

/// 打开多个文件。返回路径列表，取消返回空 `Vec`。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(target_env = "ohos")
))]
pub fn open_files(title: &str, filters: &[FileFilter]) -> Vec<String> {
    build_dialog(title, filters)
        .pick_files()
        .map(|paths| {
            paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// 保存文件。选中返回路径，取消返回 `None`。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(target_env = "ohos")
))]
pub fn save_file(title: &str, filters: &[FileFilter]) -> Option<String> {
    build_dialog(title, filters)
        .save_file()
        .map(|p| p.to_string_lossy().into_owned())
}

/// 选择文件夹。选中返回路径，取消返回 `None`。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(target_env = "ohos")
))]
pub fn open_folder(title: &str) -> Option<String> {
    rfd::FileDialog::new()
        .set_title(title)
        .pick_folder()
        .map(|p| p.to_string_lossy().into_owned())
}

// ── 非桌面（暂不支持，返回 None / 空）─────────────────────────────────────
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_env = "ohos"
))]
pub fn open_file(_title: &str, _filters: &[FileFilter]) -> Option<String> {
    None
}

#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_env = "ohos"
))]
pub fn open_files(_title: &str, _filters: &[FileFilter]) -> Vec<String> {
    Vec::new()
}

#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_env = "ohos"
))]
pub fn save_file(_title: &str, _filters: &[FileFilter]) -> Option<String> {
    None
}

#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_env = "ohos"
))]
pub fn open_folder(_title: &str) -> Option<String> {
    None
}
