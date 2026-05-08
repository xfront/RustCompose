//! # compose-platform/OHOS
//!
//! HarmonyOS Next（API 12+）平台入口 — 宿主驱动 NAPI 模型。
//!
//! ArkTS 宿主拥有 Ability 生命周期与 XComponent，本模块 **不**自建事件循环，
//! 而是导出 C ABI 供 ArkTS/NAPI 侧驱动：
//!
//! ```Text
//!   OHOS NDK (C API)                         Rust (compose-platform OHOS 后端)
//!   ├─ OHNativeWindow (XComponent) ─ptr────▶ GpuRenderer::create_surface_from_ohos_window
//!   ├─ PointerEvent / KeyEvent ────────────▶ compose_ohos_pointer / _key / _char
//!   ├─ Ability 生命周期 ────────────────────▶ compose_ohos_on_foreground / background
//!   └─ vsync (XComponent callback) ────────▶ compose_ohos_render_frame()
//! ```
//!
//! 渲染：OHOS 设备普遍支持 Vulkan 1.1+，wgpu 经 raw-window-handle 的
//! AndroidWindowHandle 兼容路径从 `OHNativeWindow` 创建 Vulkan Surface。
//!
//! ## 导出符号（ArkTS 经 napi / XComponent callback 调用）
//!
//! | 符号 | 时机 |
//! |---|---|
//! | `compose_ohos_set_app_factory(factory)` | Ability onCreate 最早时机 |
//! | `compose_ohos_attach_native_window(win, w, h)` | XComponent OnSurfaceCreated |
//! | `compose_ohos_change(w, h)` | OnSurfaceChanged |
//! | `compose_ohos_destroy_surface()` | OnSurfaceDestroyed |
//! | `compose_ohos_render_frame()` | 每帧（vsync 回调） |
//! | `compose_ohos_pointer(action, x, y)` | 触摸事件（0=down 1=move 2=up） |
//! | `compose_ohos_key(code)` / `_char(code)` / `_backspace()` | 物理键盘/软键盘 |
//! | `compose_ohos_on_foreground / background / terminate` | Ability 生命周期 |

use std::ffi::c_void;
use std::sync::Mutex;

use crate::render::{App, GpuRenderer, Kernel, default_font_collection};

type Factory = fn() -> Box<dyn App + 'static>;

static FACTORY: Mutex<Option<Factory>> = Mutex::new(None);

/// 共享宿主状态（UI 主线程单线程访问；Mutex 仅满足 static 同步要求）。
struct Host {
    kernel: Kernel,
    renderer: GpuRenderer,
    width: f32,
    height: f32,
}

static HOST: Mutex<Option<Host>> = Mutex::new(None);

/// 注册应用工厂（ArkTS Ability onCreate 最早时机调用）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_set_app_factory(factory: Option<Factory>) {
    *FACTORY.lock().unwrap() = factory;
}

/// 附着原生窗口（XComponent OnSurfaceCreated）。`native_window` = `OHNativeWindow*`。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_attach_native_window(
    native_window: *mut c_void,
    width: u32,
    height: u32,
) {
    let mut host = HOST.lock().unwrap();
    if let Some(h) = host.as_mut() {
        // Surface 重建：先断开旧连接（EGL/Vulkan swapchain 失效）。
        h.renderer.discard_surface();
        attach(&mut h.renderer, native_window, width, height);
        h.width = width as f32;
        h.height = height as f32;
        return;
    }
    let mut renderer = GpuRenderer::new();
    attach(&mut renderer, native_window, width, height);

    let factory = FACTORY
        .lock()
        .unwrap()
        .expect("compose-ohos: 未调用 set_app_factory");
    let app = factory();
    let theme = app.theme();
    crate::render::init_text_context(default_font_collection()); // TODO: 读 RawDir 字体
    let kernel = Kernel::new(theme, move || app.content());
    *host = Some(Host {
        kernel,
        renderer,
        width: width as f32,
        height: height as f32,
    });
}

#[cfg(target_env = "ohos")]
fn attach(renderer: &mut GpuRenderer, native_window: *mut c_void, width: u32, height: u32) {
    renderer.create_surface_from_ohos_window(native_window, width, height);
}

#[cfg(not(target_env = "ohos"))]
fn attach(_renderer: &mut GpuRenderer, _native_window: *mut c_void, _w: u32, _h: u32) {}

/// 尺寸变化（XComponent OnSurfaceChanged）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_change(width: u32, height: u32) {
    if let Some(h) = HOST.lock().unwrap().as_mut() {
        h.renderer.resize(width, height);
        h.width = width as f32;
        h.height = height as f32;
    }
}

/// 销毁 Surface（XComponent OnSurfaceDestroyed）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_destroy_surface() {
    if let Some(h) = HOST.lock().unwrap().as_mut() {
        h.renderer.discard_surface();
    }
}

/// 渲染一帧（vsync 回调）。返回 1 = 有动画应尽快续帧。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_render_frame() -> i32 {
    let mut host = HOST.lock().unwrap();
    match host.as_mut() {
        Some(h) => h.kernel.frame(&mut h.renderer, h.width, h.height) as i32,
        None => 0,
    }
}

/// 触摸派发：action 0=down 1=move 2=up（坐标 = 物理像素）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_pointer(action: u8, x: f32, y: f32) {
    let host = HOST.lock().unwrap();
    if let Some(h) = host.as_ref() {
        match action {
            0 => {
                h.kernel.pointer_move(x, y);
                h.kernel.pointer_down(x, y);
            }
            1 => {
                h.kernel.pointer_move(x, y);
            }
            _ => {
                h.kernel.pointer_up(x, y);
            }
        }
    }
}

/// 键盘导航键：0=Up 1=Down 2=PageUp 3=PageDown。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_key(code: u8) {
    use compose::runtime::NavKey;
    let key = match code {
        0 => NavKey::Up,
        1 => NavKey::Down,
        2 => NavKey::PageUp,
        _ => NavKey::PageDown,
    };
    let host = HOST.lock().unwrap();
    if let Some(h) = host.as_ref() {
        h.kernel.key(key);
    }
}

/// 字符输入（Unicode 标量）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_char(code: u32) {
    if let Some(ch) = char::from_u32(code) {
        let host = HOST.lock().unwrap();
        if let Some(h) = host.as_ref() {
            h.kernel.char(ch);
        }
    }
}

/// 退格。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_backspace() {
    let host = HOST.lock().unwrap();
    if let Some(h) = host.as_ref() {
        h.kernel.backspace();
    }
}

/// 前台（Ability onForeground）。TODO: 恢复渲染泵。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_on_foreground() {}

/// 后台（Ability onBackground）。TODO: 停 vsync、保存状态。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_on_background() {}

/// 终止（Ability onDestroy）。TODO: 落盘状态、清理 arena。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ohos_on_terminate() {}
