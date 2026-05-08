//! 各平台后端，按编译目标条件编译（目录分发）。
//!
//! - `desktop` / `web` / `android`：winit / android-activity 事件循环。
//! - `ios` / `ohos`：宿主驱动 C ABI（UIKit / ArkTS 调用）。
//! - `camera` / `bluetooth` / `Dialog`：平台一级能力接缝（camera / Dialog）。
//! - `services`：统一能力层（clipboard / permission / file_dialog / lifecycle / channel）。

// 桌面 / Web 共用 winit 事件循环（由 crate::render::host 提供）。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
pub mod desktop;
// Web (wasm32-unknown-unknown + wgpu WebGPU/WebGL)。
#[cfg(target_arch = "wasm32")]
pub mod web;
// Android (android-activity 直接驱动事件循环，不走 winit android 后端)。
#[cfg(target_os = "android")]
pub mod android;
// iOS（UIKit 宿主驱动，C ABI 供 Swift 调用）。
#[cfg(target_os = "ios")]
pub mod ios;
// HarmonyOS Next（OHOS NDK FFI，C ABI 供 ArkTS 调用）。
#[cfg(target_env = "ohos")]
pub mod ohos;

// 平台一级能力接缝（相机 / 蓝牙 / 文件对话框入口）。
pub mod bluetooth;
pub mod camera;
pub mod dialog;

// 跨平台平台服务层（clipboard / permission / file_dialog / lifecycle / channel）。
pub mod services;

// ── 顶层便利重导出（platform 对外暴露方式）────────────────────────────
pub use camera::CameraController;
pub use services::clipboard;
pub use services::lifecycle;
pub use services::permission;
