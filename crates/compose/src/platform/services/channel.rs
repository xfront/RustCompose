//! 平台通道（MethodChannel）— 对齐 Flutter `MethodChannel`。
//!
//! 设计目标：所有平台能力（剪贴板 / 权限 / 文件对话框 / 相机等）经此与宿主原生层
//! 通信，业务侧不感知目标平台。无宿主时退化为**进程内注册表**，桌面 / Web 也可
//! 注册 handler 自测，保证纯 Rust 编译与单元可测。
//!
//! ```rust
//! use compose::platform::services::channel;
//!
//! // 宿主侧（Android ComposeActivity / iOS ComposeHost / OHOS ComposeBridge / 桌面自测）注册一次：
//! channel::set_method_call_handler(Box::new(|name, arg| {
//!     match name {
//!         "clipboardGet" => Some("hello".to_string()),
//!         _ => None,
//!     }
//! }));
//!
//! // 能力层调用：
//! let v = channel::invoke_method("clipboardGet", "");
//! ```
//!
//! Android 真机后端可在 `android.rs` 事件循环里把 `invoke_method` 桥接到 JNI
//! （`ComposeActivity.onComposeMethodCall`）；iOS / OHOS 经各自 C ABI 转发。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 方法调用处理器签名。`handler(name, arg) -> Option<String>`；返回 `None` 表示未处理。
pub type MethodHandler = Box<dyn Fn(&str, &str) -> Option<String> + Send + Sync>;

static HANDLER: OnceLock<Mutex<Option<MethodHandler>>> = OnceLock::new();

fn handler_cell() -> &'static Mutex<Option<MethodHandler>> {
    HANDLER.get_or_init(|| Mutex::new(None))
}

/// 注册平台方法调用处理器（宿主侧调用一次；重复调用覆盖）。
pub fn set_method_call_handler(handler: MethodHandler) {
    *handler_cell().lock().unwrap() = Some(handler);
}

/// 是否已注册 handler（host 是否已挂载）。
pub fn has_handler() -> bool {
    handler_cell().lock().unwrap().is_some()
}

/// 调用平台方法，返回宿主响应。未注册 handler 或 host 未处理返回 `None`。
///
/// 返回值约定：宿主以 `Some(String)` 回传结果（可能是 `"granted"` 之类的状态串），
/// `None` 表示该 method 未被处理（能力未就绪 / 宿主未实现）。
pub fn invoke_method(name: &str, arg: &str) -> Option<String> {
    let guard = handler_cell().lock().unwrap();
    guard.as_ref().and_then(|h| h(name, arg))
}

/// 调用并映射为 [`error::Result<String>`]：未注册 handler 或 host 未处理 → `Unavailable`。
pub fn invoke(name: &str, arg: &str) -> crate::platform::services::error::Result<String> {
    use crate::platform::services::error::ServiceError;
    invoke_method(name, arg).ok_or_else(|| {
        ServiceError::unavailable(&format!(
            "channel method '{name}' 未实现 / 宿主未注册 handler"
        ))
    })
}

/// 批量注册一组 (name -> 静态响应) 的便捷方法（桌面 / Web 自测用）。
pub fn register_static(map: &[(&str, &str)]) {
    let owned: HashMap<String, String> = map
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    set_method_call_handler(Box::new(move |name, _arg| owned.get(name).cloned()));
}
