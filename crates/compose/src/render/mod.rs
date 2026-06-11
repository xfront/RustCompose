//! # compose::render
//!
//! 原 `compose-render` crate 并入核心后的渲染层（valo 0.13 + wgpu 30）：平台无关的
//! `DrawCmd → DisplayList → GPU 呈现` 管线、帧内核 [`kernel::Kernel`]，以及跨桌面 / Web /
//! Android 共用的 winit 宿主循环（[`host`]）。
//!
//! 按平台 Surface 创建入口（经验证的路径）：
//!
//! - `GpuRenderer::create_surface_from_window` — 桌面 / Web（winit 窗口）
//! - `GpuRenderer::create_surface_from_native_window` — Android `ANativeWindow`
//! - `GpuRenderer::create_surface_from_metal_layer` — iOS `UIView`
//! - `GpuRenderer::create_surface_from_ohos_window` — HarmonyOS `OHNativeWindow`
//!
//! 各平台原生粘合（iOS / OHOS 的 C ABI 入口、Web/Android 的 cdylib 入口、相机/蓝牙
//! 等平台能力）见 [`crate::platform`]，只负责「取原生窗口 + 事件泵」，复用本后端。

mod display_list;
pub mod image_cache;
mod gpu;
pub mod kernel;
pub mod host;
mod fonts;

pub use display_list::{
    build_cmds_display_list, build_display_list, new_frame_builder, record_cmds, to_valo_color,
    TextContext,
};
pub use fonts::default_font_collection;
pub use gpu::GpuRenderer;
pub use kernel::{App, Kernel, ValoTextMeasurer, init_text_context};

// winit 宿主入口（仅桌面 / Web；Android 走 android-activity，iOS / OHOS 走宿主驱动 C ABI）。
#[cfg(all(
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
pub use host::{WindowConfig, run_app, run_app_with};
