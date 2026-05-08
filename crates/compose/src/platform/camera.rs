//! 相机能力接入（JNI → Java Camera2 / AVFoundation /
//! CameraX 的 Rust 封装接缝）。
//!
//! 架构：
//!
//! ```Text
//! CameraController (Rust)
//!   → channel / JNI → 宿主相机 API
//!       ├→ open_camera(id)   打开相机（异步）
//!       ├→ is_opened()      查询状态
//!       └→ close_camera()   释放资源
//! ```
//!
//! # 当前状态
//! Rust 侧为占位实现：Android 记录状态（真实 JNI 待接），其余平台 `open_camera`
//! 直接返回 `false`（不可用）。业务侧统一用 [`CameraController::is_opened`] 判能力。
//!
//! # 权限
//! Android 需 `android.permission.CAMERA`（运行时权限由宿主 Activity 胶水请求，
//! NativeActivity 无法直接弹窗）。

use std::cell::Cell;

/// 相机控制器（封装宿主相机 API；当前为占位实现，JNI/AVFoundation 待接）。
pub struct CameraController {
    /// 相机是否已打开。
    opened: Cell<bool>,
    /// 相机 ID（0 = 后置，1 = 前置）。
    camera_id: Cell<u32>,
}

impl CameraController {
    /// 创建相机控制器。
    pub fn new() -> Self {
        Self {
            opened: Cell::new(false),
            camera_id: Cell::new(0),
        }
    }

    /// 打开相机（异步）。
    ///
    /// - `camera_id`: 0 = 后置，1 = 前置。
    /// - 返回 `true` = 已开始打开（异步），`false` = 失败（权限不足 / 无相机 / 非 Android）。
    #[cfg(target_os = "android")]
    pub fn open_camera(&self, camera_id: u32) -> bool {
        self.camera_id.set(camera_id);
        eprintln!(
            "[compose camera] open_camera({}) called (Android: JNI Camera2 待接)",
            camera_id
        );
        // TODO: 经 channel::invoke_method("cameraOpen", &id.to_string()) 桥接 JNI。
        self.opened.set(true);
        true
    }

    /// 非 Android：相机不可用（占位）。
    #[cfg(not(target_os = "android"))]
    pub fn open_camera(&self, camera_id: u32) -> bool {
        self.camera_id.set(camera_id);
        eprintln!(
            "[compose camera] open_camera({}) called (非 Android stub: 相机不可用)",
            camera_id
        );
        false
    }

    /// 关闭相机。
    pub fn close_camera(&self) {
        if !self.opened.get() {
            return;
        }
        eprintln!("[compose camera] close_camera()");
        self.opened.set(false);
    }

    /// 相机是否已打开。
    pub fn is_opened(&self) -> bool {
        self.opened.get()
    }

    /// 当前相机 ID。
    pub fn camera_id(&self) -> u32 {
        self.camera_id.get()
    }
}

impl Default for CameraController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_controller_new() {
        let c = CameraController::new();
        assert!(!c.is_opened());
        assert_eq!(c.camera_id(), 0);
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn test_open_camera_desktop() {
        let c = CameraController::new();
        assert!(!c.open_camera(0));
        assert!(!c.is_opened());
    }
}
