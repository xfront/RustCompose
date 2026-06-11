//! 跨平台 winit 宿主循环 — 桌面 / Web 共用（iOS / OHOS / Android 由各自平台后端驱动）。
//!
//! 本模块属于「平台层之上」共享宿主：组合/布局/绘制/事件派发公共管线在
//! [`crate::render::kernel::Kernel`](crate::render::kernel::Kernel)（四端共用），此处只负责：
//! - 桌面字体扫描（linux 系统字体目录，简化版）
//! - winit 事件翻译（鼠标/滚轮/键盘 → `Kernel` 派发原语）
//! - 帧调度门控（交互立即渲染；动画连续帧；空闲 Wait）
//!
//! Web（wasm32）用 `EventLoopExtWebSys::spawn_app`（`run_app` 非阻塞，main 立即返回）；
//! Android 走 `android-activity` 事件循环（见 `crate::platform::android`）；
//! iOS / OHOS 由宿主驱动（UIKit CADisplayLink / ArkTS vsync）调 `Kernel::frame`，不跑 winit。

use compose::runtime::{Composer, Recomposer};
use crate::render::gpu::GpuRenderer;
use crate::render::kernel::Kernel;

// 与应用入口同名的 trait/初始化 re-export（业务侧 `use compose::{App, init_text_context}`）。
pub use crate::render::kernel::{init_text_context, App};

// iOS / OHOS / Android 由宿主驱动，不跑 winit 事件循环，故本段整体不编译。
#[cfg(all(
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
mod winit_host {
    use super::*;
    use compose::runtime::NavKey;
    use std::sync::Arc;

    // ── winit 事件处理 ───────────────────────────────────────────────────────

    thread_local! {
        static KMOD_SHIFT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static KMOD_CTRL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// 窗口初始配置（标题 + 逻辑尺寸），由宿主入口注入，库内不写死。
    ///
    /// 尺寸语义对齐 Jetpack Compose for Desktop 的 `WindowState`：
    /// `None` 相当于 `Dp.Unspecified` —— 不强制尺寸，交由平台默认；
    /// `Some(w, h)` 相当于显式 `rememberWindowState(width = …, height = …)`。
    #[derive(Clone, Debug)]
    pub struct WindowConfig {
        pub title: String,
        pub width: Option<f64>,
        pub height: Option<f64>,
    }

    impl Default for WindowConfig {
        fn default() -> Self {
            Self {
                title: "compose — Jetpack Compose style GUI for Rust".into(),
                width: None,
                height: None,
            }
        }
    }

    impl WindowConfig {
        /// 显式指定逻辑尺寸。
        pub fn sized(title: impl Into<String>, width: f64, height: f64) -> Self {
            Self { title: title.into(), width: Some(width), height: Some(height) }
        }
    }

    thread_local! {
        static WINDOW_CONFIG: std::cell::RefCell<Option<WindowConfig>> =
            const { std::cell::RefCell::new(None) };
    }

    /// 带窗口配置的宿主入口（桌面 / Web）。
    pub fn run_app_with(config: WindowConfig, app: impl App + 'static) {
        WINDOW_CONFIG.with(|c| *c.borrow_mut() = Some(config));
        run_app(app);
    }

    struct AppState {
        window: Arc<winit::window::Window>,
        renderer: GpuRenderer,
        kernel: Kernel,
    }

    impl AppState {
        fn frame(&mut self) {
            let size = self.window.inner_size();
            let busy = self
                .kernel
                .frame(&mut self.renderer, size.width as f32, size.height as f32);
            if busy {
                self.window.request_redraw();
            }
        }
    }

    thread_local! {
        static STATE: std::cell::RefCell<Option<AppState>> = const { std::cell::RefCell::new(None) };
        static MOUSE: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
        static CURRENT_APP: std::cell::RefCell<Option<Box<dyn App>>> = const { std::cell::RefCell::new(None) };
    }

    struct Handler {
        /// 下一次「定点唤醒」的墙钟时刻（光标闪烁 / 自动轮播等非连续节奏）。
        next_wake: Option<std::time::Instant>,
    }

    impl winit::application::ApplicationHandler for Handler {
        fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            let cfg = WINDOW_CONFIG.with(|c| c.borrow().clone()).unwrap_or_default();
            let mut attrs = winit::window::Window::default_attributes().with_title(cfg.title);
            // Unspecified（None）→ 平台默认尺寸；Some → 显式逻辑尺寸。
            if let (Some(w), Some(h)) = (cfg.width, cfg.height) {
                attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h));
            }
            let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
            // 开启输入法（IME）：中文等合成输入依赖 Ime 事件，默认窗口未启用。
            window.set_ime_allowed(true);

            let mut renderer = GpuRenderer::new();
            renderer.create_surface_from_window(window.clone());

            init_text_context(crate::render::default_font_collection());

            // 首次 resumed 才组合；后续 resumed 沿用现有内核（桌面一般不会二次 resumed）。
            let existed = STATE.with(|cell| cell.borrow().is_some());
            if !existed {
                let theme = CURRENT_APP.with(|cell| {
                    cell.borrow().as_ref().expect("app not set").theme()
                });
                let kernel = Kernel::new(theme, || {
                    CURRENT_APP.with(|cell| cell.borrow().as_ref().expect("app").content())
                });

                // 状态变化 → 请求重绘
                let w2 = window.clone();
                Recomposer::on_recompose(move || {
                    w2.request_redraw();
                });

                STATE.with(|cell| {
                    *cell.borrow_mut() = Some(AppState {
                        window,
                        renderer,
                        kernel,
                    });
                });
            } else {
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        st.window = window;
                        st.renderer = renderer;
                    }
                });
            }
            STATE.with(|cell| {
                let mut b = cell.borrow_mut();
                if let Some(st) = b.as_mut() {
                    st.frame();
                }
            });
        }

        fn window_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            window_id: winit::window::WindowId,
            event: winit::event::WindowEvent,
        ) {
            let _ = event_loop;
            use winit::event::{ElementState, Ime, MouseButton, WindowEvent as WE};
            STATE.with(|cell| {
                let mut b = cell.borrow_mut();
                let Some(st) = b.as_mut() else {
                    return;
                };
                if window_id != st.window.id() {
                    return;
                }
                let kernel = &st.kernel;
                match event {
                    WE::CloseRequested => {
                        std::process::exit(0);
                    }
                    WE::Resized(size) => {
                        st.renderer.resize(size.width, size.height);
                        st.window.request_redraw();
                    }
                    WE::RedrawRequested => {
                        st.frame();
                    }
                    WE::CursorMoved { position, .. } => {
                        let (px, py) = (position.x as f32, position.y as f32);
                        MOUSE.with(|m| m.set((px, py)));
                        kernel.pointer_move(px, py);
                        st.window.request_redraw();
                    }
                    WE::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    } => {
                        let (mx, my) = MOUSE.with(|m| m.get());
                        kernel.pointer_down(mx, my);
                        st.window.request_redraw();
                    }
                    WE::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Right,
                        ..
                    } => {
                        // 右键按下即弹上下文菜单；抬起不派发（会命中刚弹出的遮罩而关掉菜单）。
                        let (mx, my) = MOUSE.with(|m| m.get());
                        kernel.pointer_secondary_click(mx, my);
                        st.window.request_redraw();
                    }
                    WE::MouseInput {
                        state: ElementState::Released,
                        button: MouseButton::Left,
                        ..
                    } => {
                        // 点击在抬起时结算（Flutter `onTap` / Compose `clickable` 语义）。
                        let (mx, my) = MOUSE.with(|m| m.get());
                        kernel.pointer_up(mx, my);
                        st.window.request_redraw();
                    }
                    WE::MouseWheel { delta, .. } => {
                        let (mx, my) = MOUSE.with(|m| m.get());
                        let dy = match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, dy) => dy * 40.0,
                            winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                        };
                        crate::input_router::scroll(dy);
                        kernel.scroll_at(mx, my, -dy);
                        st.window.request_redraw();
                    }
                    WE::ModifiersChanged(mods) => {
                        let ms = mods.state();
                        KMOD_SHIFT.with(|s| s.set(ms.shift_key()));
                        KMOD_CTRL.with(|c| c.set(ms.control_key()));
                    }
                    WE::KeyboardInput { event, .. } => {
                        if event.state == ElementState::Pressed {
                            let ctrl = KMOD_CTRL.with(|c| c.get());
                            let shift = KMOD_SHIFT.with(|s| s.get());
                            // 回报 Shift 状态给富文本选区逻辑
                            crate::input_router::shift(shift);
                            // Ctrl 组合键优先于普通字符（复制/剪切/粘贴/全选/撤销/重做）
                            if ctrl {
                                if let winit::keyboard::Key::Character(ch) = &event.logical_key {
                                    match ch.to_lowercase().as_str() {
                                        "c" => {
                                            crate::input_router::copy();
                                            st.window.request_redraw();
                                        }
                                        "x" => {
                                            crate::input_router::cut();
                                            st.window.request_redraw();
                                        }
                                        "v" => {
                                            crate::input_router::paste();
                                            st.window.request_redraw();
                                        }
                                        "a" => {
                                            crate::input_router::select_all();
                                            st.window.request_redraw();
                                        }
                                        "z" => {
                                            if shift {
                                                crate::input_router::redo();
                                            } else {
                                                crate::input_router::undo();
                                            }
                                            st.window.request_redraw();
                                        }
                                        "y" => {
                                            crate::input_router::redo();
                                            st.window.request_redraw();
                                        }
                                        _ => {}
                                    }
                                }
                            } else {
                                match &event.logical_key {
                                    winit::keyboard::Key::Character(ch) => {
                                        for c in ch.to_string().chars() {
                                            kernel.char(c);
                                        }
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Backspace) => {
                                        kernel.backspace()
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter) => {
                                        crate::input_router::enter()
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowUp) => {
                                        kernel.key(NavKey::Up);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowDown) => {
                                        kernel.key(NavKey::Down);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowLeft) => {
                                        kernel.key(NavKey::Left);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowRight) => {
                                        kernel.key(NavKey::Right);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::PageUp) => {
                                        kernel.key(NavKey::PageUp);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::PageDown) => {
                                        kernel.key(NavKey::PageDown);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Home) => {
                                        kernel.key(NavKey::Home);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::End) => {
                                        kernel.key(NavKey::End);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Delete) => {
                                        kernel.key(NavKey::Delete);
                                    }
                                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::F3) => {
                                        crate::input_router::find_next();
                                        st.window.request_redraw();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        st.window.request_redraw();
                    }
                    WE::Ime(ime) => {
                        match ime {
                            Ime::Commit(Text) => crate::input_router::commit(&Text),
                            Ime::Preedit(Text, _) => crate::input_router::preedit(&Text),
                            Ime::Disabled => crate::input_router::preedit(""),
                            _ => {}
                        }
                        st.window.request_redraw();
                    }
                    _ => {}
                }
            });
        }

        fn new_events(
            &mut self,
            _event_loop: &winit::event_loop::ActiveEventLoop,
            cause: winit::event::StartCause,
        ) {
            // 定点唤醒到点：驱动一帧（光标翻转 / 轮播切页）。
            if matches!(cause, winit::event::StartCause::ResumeTimeReached { .. }) {
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow().as_ref() {
                        st.window.request_redraw();
                    }
                });
            }
        }

        fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            use winit::event_loop::ControlFlow;
            // 组合期/帧驱动登记的「稍后再来一帧」（读取即清除）。
            if let Some(delay) = compose::animation::take_wake_delay() {
                self.next_wake = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_secs_f32(delay.max(0.0)),
                );
            }
            match self.next_wake {
                Some(t) if std::time::Instant::now() >= t => {
                    self.next_wake = None;
                    STATE.with(|cell| {
                        if let Some(st) = cell.borrow().as_ref() {
                            st.window.request_redraw();
                        }
                    });
                }
                Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
                // 无动画、无定点唤醒：真正休眠（不再为了「聚焦输入框」无条件重绘）。
                None => event_loop.set_control_flow(ControlFlow::Wait),
            }
        }
    }

    /// 桌面入口（linux/macos/windows）：阻塞至窗口关闭。
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(target_os = "android"),
        not(target_env = "ohos"),
        not(target_os = "ios")
    ))]
    pub fn run_app(app: impl App + 'static) {
        CURRENT_APP.with(|cell| *cell.borrow_mut() = Some(Box::new(app)));
        let event_loop = winit::event_loop::EventLoop::new().expect("event loop");
        let mut handler = Handler { next_wake: None };
        event_loop.run_app(&mut handler).expect("run_app");
    }

    /// Web 入口 — `EventLoopExtWebSys::spawn_app`（main 立即返回，浏览器 rAF 保活）。
    #[cfg(target_arch = "wasm32")]
    pub fn run_app(app: impl App + 'static) {
        use winit::platform::web::EventLoopExtWebSys;
        CURRENT_APP.with(|cell| *cell.borrow_mut() = Some(Box::new(app)));
        let event_loop = winit::event_loop::EventLoop::new().expect("event loop");
        let handler = Handler { next_wake: None };
        event_loop.spawn_app(handler);
    }
}

// 桌面 / Web 宿主入口（Android 走 android-activity，iOS / OHOS 走宿主驱动 C ABI）。
#[cfg(all(
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
pub use winit_host::{run_app, run_app_with, WindowConfig};
