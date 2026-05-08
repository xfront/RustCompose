//! # compose-platform/iOS
//!
//! iOS 平台入口 — 宿主驱动嵌入模型。
//!
//! UIKit 拥有 `UIApplicationMain` 与运行循环，本模块 **不**自建事件循环，
//! 而是导出 C ABI 供 Swift 宿主驱动：
//!
//! ```Text
//!   Swift 宿主 (UIKit)                    Rust (compose-platform iOS 后端)
//!   ┌──────────────────────┐             ┌──────────────────────────────┐
//!   │ UIViewController      │             │ Kernel (共享帧内核)           │
//!   │  └─ UIView            │             │  ├─ 组合/布局/事件派发         │
//!   │      └─ CAMetalLayer ──┼─ptr──────▶ │  └─ GpuRenderer(Metal)       │
//!   │  CADisplayLink ────────┼─▶ compose_ios_render_frame()              │
//!   │  touchesBegan/Moved ───┼─▶ compose_ios_pointer(action, x, y)       │
//!   │  键盘通知 ─────────────┼─▶ compose_ios_key(code) / _char(ch)       │
//!   │  生命周期 ─────────────┼─▶ compose_ios_on_foreground/background    │
//!   └──────────────────────┘             └──────────────────────────────┘
//! ```
//!
//! ## 导出符号（Swift 桥接头）
//!
//! | 符号 | 时机 |
//! |---|---|
//! | `compose_ios_set_app_factory(factory)` | 启动最早（如 `main` 前）；factory 返回 App |
//! | `compose_ios_attach_metal_layer(layer, w, h)` | viewDidAppear：创建/重建 Surface |
//! | `compose_ios_resize(w, h)` | layoutSubviews |
//! | `compose_ios_render_frame()` | CADisplayLink 每帧 |
//! | `compose_ios_pointer(action, x, y)` | touchesBegan/Moved/Ended（0/1/2） |
//! | `compose_ios_key(code)` | 键盘方向键（0=Up 1=Down 2=PageUp 3=PageDown） |
//! | `compose_ios_char(code)` | 键盘字符（Unicode 标量） |
//! | `compose_ios_backspace()` | 键盘退格 |
//! | `compose_ios_on_foreground/background` | AppDelegate 生命周期 |

use std::ffi::c_void;
use std::sync::Mutex;

use crate::render::{App, GpuRenderer, Kernel, default_font_collection};

type Factory = fn() -> Box<dyn App + 'static>;

static FACTORY: Mutex<Option<Factory>> = Mutex::new(None);

/// 共享宿主状态（UIKit 主线程单线程访问；Mutex 仅满足 static 同步要求）。
struct Host {
    kernel: Kernel,
    renderer: GpuRenderer,
    width: f32,
    height: f32,
}

static HOST: Mutex<Option<Host>> = Mutex::new(None);

/// 注册应用工厂（Swift 侧启动最早时机调用；`extern "C"` 便于桥接头声明）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_set_app_factory(factory: Option<Factory>) {
    *FACTORY.lock().unwrap() = factory;
}

/// 附着 Metal 层（viewDidAppear / layer 重建）。`layer` = CAMetalLayer 指针。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_attach_metal_layer(layer: *mut c_void, width: u32, height: u32) {
    let mut host = HOST.lock().unwrap();
    // Surface 重建：先断开旧连接（对齐 android discard 语义）。
    if let Some(h) = host.as_mut() {
        h.renderer.discard_surface();
        attach(&mut h.renderer, layer, width, height);
        h.width = width as f32;
        h.height = height as f32;
        return;
    }
    // 首次：创建渲染器 + 组合内核。
    let mut renderer = GpuRenderer::new();
    attach(&mut renderer, layer, width, height);

    let factory = FACTORY
        .lock()
        .unwrap()
        .expect("compose-ios: 未调用 set_app_factory");
    let app = factory();
    let theme = app.theme();
    crate::render::init_text_context(default_font_collection()); // TODO: 内嵌 CJK 字体
    let kernel = Kernel::new(theme, move || app.content());
    *host = Some(Host {
        kernel,
        renderer,
        width: width as f32,
        height: height as f32,
    });
}

#[cfg(target_os = "ios")]
fn attach(renderer: &mut GpuRenderer, layer: *mut c_void, width: u32, height: u32) {
    renderer.create_surface_from_metal_layer(layer, width, height);
}

#[cfg(not(target_os = "ios"))]
fn attach(_renderer: &mut GpuRenderer, _layer: *mut c_void, _w: u32, _h: u32) {}

/// 视图尺寸变化（layoutSubviews）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_resize(width: u32, height: u32) {
    if let Some(h) = HOST.lock().unwrap().as_mut() {
        h.renderer.resize(width, height);
        h.width = width as f32;
        h.height = height as f32;
    }
}

/// 渲染一帧（CADisplayLink 每帧调用）。返回 1 = 有动画应尽快续帧。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_render_frame() -> i32 {
    let mut host = HOST.lock().unwrap();
    match host.as_mut() {
        Some(h) => h.kernel.frame(&mut h.renderer, h.width, h.height) as i32,
        None => 0,
    }
}

/// 触摸派发：action 0=down 1=move 2=up（坐标 = 物理像素）。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_pointer(action: u8, x: f32, y: f32) {
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
pub extern "C" fn compose_ios_key(code: u8) {
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
pub extern "C" fn compose_ios_char(code: u32) {
    if let Some(ch) = char::from_u32(code) {
        let host = HOST.lock().unwrap();
        if let Some(h) = host.as_ref() {
            h.kernel.char(ch);
        }
    }
}

/// 退格。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_backspace() {
    let host = HOST.lock().unwrap();
    if let Some(h) = host.as_ref() {
        h.kernel.backspace();
    }
}

/// 前台（AppDelegate didBecomeActive）。TODO: 生命周期观察/恢复渲染泵。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_on_foreground() {}

/// 后台（AppDelegate willResignActive）。TODO: 停 CADisplayLink 保存状态。
#[unsafe(no_mangle)]
pub extern "C" fn compose_ios_on_background() {}
