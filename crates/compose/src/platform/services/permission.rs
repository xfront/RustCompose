//! 权限请求 — 对齐 Flutter `permission_handler` 的轻量跨平台对应物。
//!
//! 跨平台策略：
//! - **Desktop**：无权限框架，统一视为已授权（返回 `Ok(true)`）。
//! - **Android / Web / iOS / OHOS**：经 [`channel`] 的 `requestPermission` 交给宿主原生
//!   实现，参数即权限码（`camera`/`storage`/`location`/`microphone`/`photos`），
//!   宿主回传 `granted` / `denied` / `limited` / `restricted` / `permanentlyDenied` / `unknown`。

use super::channel;
use super::error::Result;

/// 受控权限类型（对齐 Flutter `Permission`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// 相机（Android `CAMERA`）。
    Camera,
    /// 存储读写。
    Storage,
    /// 定位。
    Location,
    /// 麦克风。
    Microphone,
    /// 相册（iOS `PHPhotoLibrary` / Android `READ_MEDIA_IMAGES`）。
    Photos,
}

impl Permission {
    /// 通道参数码。
    pub fn code(self) -> &'static str {
        match self {
            Permission::Camera => "camera",
            Permission::Storage => "storage",
            Permission::Location => "location",
            Permission::Microphone => "microphone",
            Permission::Photos => "photos",
        }
    }
}

/// 解析宿主返回的状态串为是否授权。
fn parse_status(s: &str) -> bool {
    matches!(s, "granted" | "limited")
}

/// 请求权限。返回是否授权；宿主未实现 / 平台未接通时返回 `Err`。
///
/// ```rust
/// use compose::platform::services::permission::{request, Permission};
/// let ok = request(Permission::Camera).unwrap_or(false);
/// ```
pub fn request(perm: Permission) -> Result<bool> {
    // 桌面无权限框架，视为已授权。
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(target_os = "android"),
        not(target_os = "ios"),
        not(target_env = "ohos")
    ))]
    {
        let _ = perm;
        return Ok(true);
    }
    #[cfg(any(
        target_arch = "wasm32",
        target_os = "android",
        target_os = "ios",
        target_env = "ohos"
    ))]
    {
        channel::invoke_method("requestPermission", perm.code())
            .as_deref()
            .map(parse_status)
            .ok_or_else(|| super::error::ServiceError {
                kind: super::error::ErrorKind::Unavailable,
                message: format!("requestPermission({}): 宿主未实现通道", perm.code()),
            })
    }
}
