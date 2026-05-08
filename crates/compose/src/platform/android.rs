//! Android 平台后端 — `android-activity` NativeActivity 事件循环。
//!
//! Android 平台入口：用 `android-activity` 驱动 `poll_events` 主循环，
//! 从 `AndroidApp::native_window()` 取 `ANativeWindow` 直接喂给
//! [`GpuRenderer::create_surface_from_native_window`]，复用 `crate::render` 的
//! 共享渲染管线与帧内核 [`Kernel`]（四端共用）。iOS / OHOS 走各自 C ABI 宿主驱动。
//!
//! 触摸→鼠标映射：单指 = 左键；多点触控暂按第一根手指处理（TODO 手势识别）。
//! 坐标使用 Android `MotionEvent` 的物理像素，与 [`Kernel::frame`] 的物理像素布局一致。

use std::time::Duration;

use crate::platform::services::lifecycle;
use android_activity::{
    InputStatus, MainEvent, PollEvent,
    input::{Axis, InputEvent, KeyAction, KeyEvent, Keycode, MotionAction, MotionEvent},
};
use compose::runtime::{Composer, NavKey};
use crate::render::{App, GpuRenderer, Kernel, default_font_collection, init_text_context};

/// `AndroidApp` 类型别名（android-activity 提供）。
pub use android_activity::AndroidApp;

type Factory = fn() -> Box<dyn App + 'static>;

static FACTORY: std::sync::OnceLock<Factory> = std::sync::OnceLock::new();

/// 注册应用工厂（备用模式：宿主 android_main 调此 + [`run_app_with_android`]）。
pub fn register_app_factory(f: Factory) -> bool {
    FACTORY.set(f).is_ok()
}

/// 应用入口（examples 的 `#[no_mangle] android_main` 调用）：工厂取 App 并启动事件循环。
pub fn android_entry(factory: Factory, app: AndroidApp) {
    run_app_with_android(factory(), app);
}

/// 启动 Android `android-activity` 事件循环，驱动 `Kernel` 渲染。
///
/// 不依赖 winit android 后端：直接从 `android_app.native_window()` 取 `ANativeWindow`，
/// 规避 winit 0.30 Android `EventLoopBuilder` / raw-window-handle 的 API 漂移。
pub fn run_app_with_android(app: Box<dyn App + 'static>, android_app: AndroidApp) {
    // 字体与组合内核在窗口创建前准备好；内核持有组合状态（remember 槽位），
    // 故只建一次，rotation / 后台切回只重建 GpuRenderer 表面，不丢应用状态。
    // `app`（Box<dyn App + 'static>）整体移入内容闭包，满足 Kernel::new 的 'static 要求。
    init_text_context(default_font_collection());

    let theme = app.theme();
    let content = move || app.content();
    let mut kernel = Kernel::new(theme, content);

    let mut renderer: Option<GpuRenderer> = None;
    let mut needs_redraw = false;

    loop {
        android_app.poll_events(Some(Duration::from_millis(8)), |event| {
            match event {
                PollEvent::Wake | PollEvent::Timeout => {}
                PollEvent::Main(main_event) => match main_event {
                    MainEvent::InitWindow { .. } => {
                        if let Some(window) = android_app.native_window() {
                            // 建新表面前先丢弃旧 renderer（EGL/ANativeWindow 同一时刻
                            // 只能被一个生产者连接，避免二次 eglCreateWindowSurface 报
                            // already connected → EGL_BAD_ALLOC）。
                            if let Some(g) = renderer.as_mut() {
                                g.discard_surface();
                            }
                            let mut r = GpuRenderer::new();
                            r.create_surface_from_native_window(&window);
                            renderer = Some(r);
                            needs_redraw = true;
                            // 窗口就绪 → 前台可交互（通知生命周期观察者）。
                            lifecycle::set_state(lifecycle::AppLifecycleState::Resumed);
                        }
                    }
                    MainEvent::TerminateWindow { .. } => {
                        if let Some(g) = renderer.as_mut() {
                            g.discard_surface();
                        }
                        renderer = None;
                        // 原生窗口销毁 → 进入后台（暂停渲染）。
                        lifecycle::set_state(lifecycle::AppLifecycleState::Paused);
                    }
                    MainEvent::WindowResized { .. } | MainEvent::ContentRectChanged { .. } => {
                        if let Some(g) = renderer.as_mut() {
                            if let Some(win) = android_app.native_window() {
                                let nw = win.width() as u32;
                                let nh = win.height() as u32;
                                if nw != g.width() || nh != g.height() {
                                    g.resize(nw, nh);
                                }
                            }
                        }
                        needs_redraw = true;
                    }
                    MainEvent::RedrawNeeded { .. } => {
                        needs_redraw = true;
                    }
                    MainEvent::InputAvailable => {
                        if let Ok(mut iter) = android_app.input_events_iter() {
                            while iter.next(|input| match input {
                                InputEvent::MotionEvent(motion) => {
                                    handle_motion(&kernel, motion);
                                    InputStatus::Handled
                                }
                                InputEvent::KeyEvent(key) => {
                                    handle_key(&kernel, key);
                                    InputStatus::Handled
                                }
                                _ => InputStatus::Unhandled,
                            }) {}
                        }
                        needs_redraw = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        });

        if needs_redraw {
            if let Some(g) = renderer.as_mut() {
                let w = g.width() as f32;
                let h = g.height() as f32;
                let busy = kernel.frame(g, w, h);
                needs_redraw = busy; // 动画/待重组继续驱动帧
            }
        }
    }
}

/// 触摸事件 → Kernel 指针派发（单指 = 左键）。
fn handle_motion(kernel: &Kernel, motion: &MotionEvent) {
    if motion.pointer_count() == 0 {
        return;
    }
    let p0 = motion.pointer_at_index(0);
    let x = p0.x();
    let y = p0.y();
    match motion.action() {
        MotionAction::Down => {
            kernel.pointer_down(x, y);
        }
        MotionAction::Move => {
            kernel.pointer_move(x, y);
        }
        MotionAction::Up => {
            kernel.pointer_up(x, y);
        }
        MotionAction::Scroll => {
            let v = p0.axis_value(Axis::Vscroll);
            if v != 0.0 {
                // Android Vscroll 正值 = 向下；Kernel::scroll_at 的 dy 与 winit 一致（正=下）。
                kernel.scroll_at(x, y, -(v as f32) * 20.0);
            }
        }
        _ => {}
    }
}

/// 键盘事件 → Kernel 派发（字母/数字/空格 → char；方向/PageUp/Down → NavKey）。
fn handle_key(kernel: &Kernel, key: &KeyEvent) {
    if key.action() != KeyAction::Down {
        return;
    }
    match key.key_code() {
        Keycode::Del | Keycode::ForwardDel => kernel.backspace(),
        Keycode::DpadUp => {
            kernel.key(NavKey::Up);
        }
        Keycode::DpadDown => {
            kernel.key(NavKey::Down);
        }
        Keycode::PageUp => {
            kernel.key(NavKey::PageUp);
        }
        Keycode::PageDown => {
            kernel.key(NavKey::PageDown);
        }
        Keycode::Space => {
            kernel.char(' ');
        }
        _ => {
            let kc = u32::from(key.key_code());
            // AKEYCODE_A=29..=Z=54，AKEYCODE_0=7..=9=16
            if (29..=54).contains(&kc) {
                let ch = (b'a' + (kc - 29) as u8) as char;
                kernel.char(ch);
            } else if (7..=16).contains(&kc) {
                let ch = (b'0' + (kc - 7) as u8) as char;
                kernel.char(ch);
            }
        }
    }
}
