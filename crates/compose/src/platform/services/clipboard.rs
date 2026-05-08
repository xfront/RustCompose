//! 剪贴板 — 对齐 Flutter `Clipboard`。
//!
//! 跨平台统一 API：所有平台经 [`channel`] 与宿主通信（`clipboardSet` / `clipboardGet`）。
//! 桌面端（本仓支持的真实后端）直接用 [`arboard`] 访问系统剪贴板，不走 channel。
//!
//! - **Desktop**（Linux/macOS/Windows）：`arboard` 调用系统剪贴板（真实实现）。
//! - **Web**：`navigator.clipboard`（宿主桥接，本同步接口 `get_text` 受限）。
//! - **Android / iOS / OHOS**：经 `channel` 交给各自原生宿主实现。

use super::error::{Result, ServiceError};

/// 写入系统剪贴板（Flutter: `Clipboard.setData(ClipboardData(Text:))`）。
pub fn set_text(Text: &str) -> Result<()> {
    inner_set(Text)
}

/// 读取系统剪贴板（Flutter: `Clipboard.getData(Clipboard.kTextPlain)`）。
///
/// 返回 `Ok(String)` 或 `Err(Unavailable)`（宿主未实现 / Web 异步不支持 / 桌面 arboard 异常）。
pub fn get_text() -> Result<String> {
    inner_get()
}

// ── Desktop（arboard，真实系统剪贴板）──────────────────────────────────────
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
fn inner_set(Text: &str) -> Result<()> {
    let mut ctx = arboard::Clipboard::new()
        .map_err(|e| ServiceError::unavailable(&format!("clipboard: {}", e)))?;
    ctx.set_text(Text.to_string())
        .map_err(|e| ServiceError::unavailable(&format!("clipboard: {}", e)))
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
fn inner_get() -> Result<String> {
    let mut ctx = arboard::Clipboard::new()
        .map_err(|e| ServiceError::unavailable(&format!("clipboard: {}", e)))?;
    ctx.get_text()
        .map_err(|e| ServiceError::unavailable(&format!("clipboard: {}", e)))
}

// ── Web / 移动端（经 channel 交给宿主原生实现）─────────────────────────────
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_env = "ohos",
    target_os = "ios"
))]
fn inner_set(Text: &str) -> Result<()> {
    match super::channel::invoke_method("clipboardSet", Text) {
        Some(_) => Ok(()),
        None => Err(ServiceError {
            kind: super::error::ErrorKind::Unavailable,
            message: "clipboard: 宿主未实现 onMethodCall(\"clipboardSet\")".to_string(),
        }),
    }
}

#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_env = "ohos",
    target_os = "ios"
))]
fn inner_get() -> Result<String> {
    super::channel::invoke_method("clipboardGet", "").ok_or_else(|| ServiceError {
        kind: super::error::ErrorKind::Unavailable,
        message: "clipboard: 宿主未实现 onMethodCall(\"clipboardGet\")（Web 异步读取受限制，请经 channel 宿主桥接）"
            .to_string(),
    })
}
