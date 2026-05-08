//! 平台服务统一错误类型。
//!
//! 所有能力层模块（`clipboard`/`permission`/`file_dialog` 等）均用本 `Result<T>`，
//! 失败时给出 `ServiceError`（含 `ErrorKind`，便于上层区分「未实现 / 权限拒绝 /
//! 用户取消」）。纯 Rust，无平台依赖。

use std::fmt;

/// 错误类别（对齐 Flutter `PlatformException` 的 code 语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 平台能力当前不可用 / 宿主未实现（如桌面剪贴板未接入 arboard）。
    Unavailable,
    /// 资源/方法未找到（宿主无对应 handler）。
    NotFound,
    /// 权限被拒绝。
    PermissionDenied,
    /// 用户取消（文件对话框取消等）。
    Cancelled,
    /// 其它未分类错误。
    Other,
}

/// 平台服务错误。
#[derive(Debug, Clone)]
pub struct ServiceError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ServiceError {
    /// 能力不可用 / 宿主未实现。
    pub fn unavailable(msg: &str) -> Self {
        Self {
            kind: ErrorKind::Unavailable,
            message: msg.to_string(),
        }
    }
    /// 未实现（接缝占位）。
    pub fn not_implemented(msg: &str) -> Self {
        Self {
            kind: ErrorKind::Unavailable,
            message: format!("not implemented: {msg}"),
        }
    }
    /// 未找到。
    pub fn not_found(msg: &str) -> Self {
        Self {
            kind: ErrorKind::NotFound,
            message: msg.to_string(),
        }
    }
    /// 权限被拒绝。
    pub fn permission_denied(msg: &str) -> Self {
        Self {
            kind: ErrorKind::PermissionDenied,
            message: msg.to_string(),
        }
    }
    /// 用户取消。
    pub fn cancelled() -> Self {
        Self {
            kind: ErrorKind::Cancelled,
            message: "cancelled by user".to_string(),
        }
    }
    /// 其它错误。
    pub fn other(msg: &str) -> Self {
        Self {
            kind: ErrorKind::Other,
            message: msg.to_string(),
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for ServiceError {}

/// 能力层统一返回类型。
pub type Result<T> = std::result::Result<T, ServiceError>;
