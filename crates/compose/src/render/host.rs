//! 跨平台 winit 宿主循环 — 桌面 / Web 共用（iOS / OHOS / Android 由各自平台后端驱动）。
//!
//! 本模块属于「平台层之上」共享宿主：组合/布局/绘制/事件派发公共管线在
//! [`crate::render::kernel::Kernel`](crate::render::kernel::Kernel)（四端共用），此处只负责：
//! - 桌面字体扫描（linux 系统字体目录，简化版）
//! - winit 事件翻译（鼠标/滚轮/键盘 → `Kernel` 派发原语）
//! - 帧调度门控（交互立即渲染；动画连续帧；空闲 Wait）
//!
//! # 多窗口
//!
//! 每个窗口一个 [`Kernel`]（自己的组合根 / 保留模式缓存 / GPU 表面），共享同一线程的
//! 运行时 arena（节点与作用域 id 全局唯一），对齐 Compose「单 Recomposer + 多 ComposeScene」。
//! 由此本模块额外负责：
//! - 窗口注册表：按 `WindowId` 路由事件到对应 `Kernel`；
//! - 全局帧时钟：每轮事件循环只 `advance_frame` 一次（否则 N 个窗口动画快 N 倍）；
//! - 续帧只给「有动画 / 有待重组」的窗口请求重绘，其余窗口保持休眠；
//! - 开窗 / 关窗请求先进队列，由下一轮事件循环统一执行（组合期不能同步建窗）。
//!
//! Web（wasm32）用 `EventLoopExtWebSys::spawn_app`（`run_app` 非阻塞，main 立即返回）；
//! Android 走 `android-activity` 事件循环（见 `crate::platform::android`）；
//! iOS / OHOS 由宿主驱动（UIKit CADisplayLink / ArkTS vsync）调 `Kernel::frame`，不跑 winit。

use compose::runtime::Recomposer;
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

    // winit 在 wasm32 下用 web_time::Instant（ControlFlow::WaitUntil 的参数类型），
    // 桌面用 std::time::Instant；统一别名避免两套 cfg 分支。
    #[cfg(target_arch = "wasm32")]
    use web_time::{Duration, Instant};
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::{Duration, Instant};

    // ── winit 事件处理 ───────────────────────────────────────────────────────

    thread_local! {
        static KMOD_SHIFT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static KMOD_CTRL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static KMOD_ALT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
        pub decorations: bool,
        /// Border thickness (logical px) that starts an OS resize drag; 0 disables.
        pub resize_border: f32,
    }

    impl Default for WindowConfig {
        fn default() -> Self {
            Self {
                title: "compose — Jetpack Compose style GUI for Rust".into(),
                width: None,
                height: None,
                decorations: true,
                resize_border: 0.0,
            }
        }
    }

    impl WindowConfig {
        /// 显式指定逻辑尺寸。
        pub fn sized(title: impl Into<String>, width: f64, height: f64) -> Self {
            Self {
                title: title.into(),
                width: Some(width),
                height: Some(height),
                decorations: true,
                resize_border: 0.0,
            }
        }
        /// Borderless window: the app draws its own title bar / window controls.
        pub fn undecorated(mut self) -> Self {
            self.decorations = false;
            self
        }
        /// Enable borderless edge/corner resize with a grip band of `px` logical
        /// pixels. `0.0` (default) disables it.
        pub fn resizable(mut self, px: f32) -> Self {
            self.resize_border = px;
            self
        }
    }

    /// 窗口句柄（本线程内唯一；窗口关闭后不再复用）。
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct WindowHandle(u64);

    /// 窗口槽：一个窗口的全部状态。多窗口共享 arena，但渲染资源各自独立。
    struct WindowSlot {
        handle: WindowHandle,
        window: Arc<winit::window::Window>,
        renderer: GpuRenderer,
        kernel: Kernel,
        /// 本窗口的重组监听（关窗时注销；监听器本身持 `Weak<Window>`）。
        listener: compose::runtime::RecomposeListener,
        /// 本窗口内最后一次指针位置（物理像素）：按下/抬起事件本身不带坐标。
        mouse: (f32, f32),
        /// 边缘缩放命中带厚度（逻辑像素）；0 关闭。
        resize_border: f32,
        /// 上次已知的真实最大化状态（在 `WE::Resized` 里比对，变化才触发回调）。
        maximized: bool,
    }

    /// 待建窗口请求（在组合 / 事件回调里登记，事件循环下一轮执行）。
    struct OpenRequest {
        handle: WindowHandle,
        config: WindowConfig,
        app: Box<dyn App>,
    }

    thread_local! {
        /// 主窗口配置 + 应用（`resumed` 时建首个窗口）。
        static MAIN_WINDOW: std::cell::RefCell<Option<(WindowConfig, Box<dyn App>)>> =
            const { std::cell::RefCell::new(None) };
        static SLOTS: std::cell::RefCell<Vec<WindowSlot>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static PENDING_OPEN: std::cell::RefCell<Vec<OpenRequest>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static PENDING_CLOSE: std::cell::RefCell<Vec<WindowHandle>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static NEXT_HANDLE: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
        /// 当前正在组合 / 派发事件的窗口（`current_window()` 的来源）。
        static CURRENT_WINDOW: std::cell::Cell<Option<WindowHandle>> =
            const { std::cell::Cell::new(None) };
        /// 事件循环代理：在组合/回调里开窗关窗后唤醒事件循环排空请求队列。
        static PROXY: std::cell::RefCell<Option<winit::event_loop::EventLoopProxy<()>>> =
            const { std::cell::RefCell::new(None) };
    }

    fn alloc_handle() -> WindowHandle {
        NEXT_HANDLE.with(|c| {
            let h = WindowHandle(c.get());
            c.set(h.0 + 1);
            h
        })
    }

    fn wake() {
        PROXY.with(|p| {
            if let Some(proxy) = p.borrow().as_ref() {
                let _ = proxy.send_event(());
            }
        });
    }

    /// 打开一个新窗口（可在组合闭包或事件回调中调用）。
    ///
    /// 实际建窗发生在下一次事件循环迭代（组合期同步建窗会重入运行时）；返回的句柄
    /// 立即有效，可交给 [`close_window`]。`app` 拥有本窗口的独立内容与主题。
    pub fn open_window(config: WindowConfig, app: impl App + 'static) -> WindowHandle {
        let handle = alloc_handle();
        PENDING_OPEN.with(|q| {
            q.borrow_mut().push(OpenRequest { handle, config, app: Box::new(app) });
        });
        wake();
        handle
    }

    /// 请求关闭指定窗口；最后一个窗口关闭后应用退出。
    pub fn close_window(handle: WindowHandle) {
        PENDING_CLOSE.with(|q| q.borrow_mut().push(handle));
        wake();
    }

    /// 当前窗口（组合期间 / 该窗口的事件派发期间）。
    ///
    /// 点击回调里据此获知「是哪个窗口触发了我」，例如自己开的子窗口里的关闭按钮。
    pub fn current_window() -> Option<WindowHandle> {
        CURRENT_WINDOW.with(|c| c.get())
    }

    /// 当前打开的窗口数量。
    pub fn window_count() -> usize {
        SLOTS.with(|s| s.borrow().len())
    }

    /// 待处理的窗口控制请求（全屏 / 最小化）：组合期 / 事件回调里登记，drain 时统一执行
    /// （事件派发期间该窗口槽位被 `take_slot` 取出，无法即时命中，故排队到下一轮）。
    #[derive(Clone, Copy)]
    enum WinCtl {
        Fullscreen(bool),
        Minimize(bool),
        Maximize(bool),
    }

    thread_local! {
        static PENDING_WINCTL: std::cell::RefCell<Vec<(WindowHandle, WinCtl)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    /// 设置窗口无边框全屏 / 取消全屏（下一轮事件循环生效）。
    pub fn set_fullscreen(handle: WindowHandle, on: bool) {
        PENDING_WINCTL.with(|q| q.borrow_mut().push((handle, WinCtl::Fullscreen(on))));
        wake();
    }

    /// 最小化 / 还原窗口（下一轮事件循环生效）。
    pub fn set_minimized(handle: WindowHandle, on: bool) {
        PENDING_WINCTL.with(|q| q.borrow_mut().push((handle, WinCtl::Minimize(on))));
        wake();
    }

    /// Maximize / restore a window (applied on the next event-loop turn).
    pub fn set_maximized(handle: WindowHandle, on: bool) {
        PENDING_WINCTL.with(|q| q.borrow_mut().push((handle, WinCtl::Maximize(on))));
        wake();
    }

    thread_local! {
        /// Window whose OS-level move should start on this event-loop turn.
        /// Set by a title-bar press handler, drained in `window_event` while the
        /// slot is still in hand (so `drag_window` runs in the same mouse turn).
        static DRAG_REQUEST: std::cell::Cell<Option<WindowHandle>> =
            const { std::cell::Cell::new(None) };
    }

    /// Begin an OS window drag (borderless custom title bar). Call from a
    /// pointer-press handler; executed within the same `window_event` turn.
    pub fn start_window_drag(handle: WindowHandle) {
        DRAG_REQUEST.with(|d| d.set(Some(handle)));
        wake();
    }

    /// Which window border a resize drag should grab (mirrors winit
    /// `ResizeDirection`; kept neutral so winit types stay out of the API).
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum ResizeEdge {
        North,
        South,
        East,
        West,
        NorthEast,
        NorthWest,
        SouthEast,
        SouthWest,
    }

    thread_local! {
        /// Window + border whose OS resize should start this event-loop turn.
        static RESIZE_REQUEST: std::cell::Cell<Option<(WindowHandle, ResizeEdge)>> =
            const { std::cell::Cell::new(None) };
    }

    /// Begin an OS window resize from a border (borderless edge drag). Call from
    /// a pointer-press handler; executed within the same `window_event` turn.
    pub fn start_window_resize(handle: WindowHandle, edge: ResizeEdge) {
        RESIZE_REQUEST.with(|r| r.set(Some((handle, edge))));
        wake();
    }

    thread_local! {
        /// Per-window callbacks fired when the real maximized state flips
        /// (detected in `WE::Resized`, the only spot the slot + `is_maximized()`
        /// are both reachable — composition runs with the slot taken out).
        static MAX_HOOKS: std::cell::RefCell<
            Vec<(WindowHandle, Box<dyn FnMut(bool)>)>,
        > = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// Mirror a window's real maximized state into the app. The callback runs on
    /// the main thread inside `window_event`; register it once, e.g. from the
    /// first composition where `current_window()` is the target window.
    pub fn on_maximized_changed(handle: WindowHandle, f: impl FnMut(bool) + 'static) {
        MAX_HOOKS.with(|v| v.borrow_mut().push((handle, Box::new(f))));
    }

    /// 事件派发 / 组合期间的「当前窗口」守卫，`Drop` 时恢复进入前的值。
    struct WindowCtx(Option<WindowHandle>);

    impl WindowCtx {
        fn enter(handle: WindowHandle) -> Self {
            let prev = CURRENT_WINDOW.with(|c| c.get());
            CURRENT_WINDOW.with(|c| c.set(Some(handle)));
            Self(prev)
        }
    }

    impl Drop for WindowCtx {
        fn drop(&mut self) {
            CURRENT_WINDOW.with(|c| c.set(self.0));
        }
    }

    /// 按窗口 id 取出槽位（派发期间槽位不在注册表里：避免持有 `RefCell` 借用时
    /// 回调再进来开窗/关窗造成重复借用）。
    fn take_slot(id: winit::window::WindowId) -> Option<WindowSlot> {
        SLOTS.with(|s| {
            let mut v = s.borrow_mut();
            let i = v.iter().position(|x| x.window.id() == id)?;
            Some(v.remove(i))
        })
    }

    fn put_slot(slot: WindowSlot) {
        SLOTS.with(|s| s.borrow_mut().push(slot));
    }

    fn request_redraw_all() {
        SLOTS.with(|s| {
            for slot in s.borrow().iter() {
                slot.window.request_redraw();
            }
        });
    }

    /// 创建窗口并完成首帧组合。尺寸取实际内尺寸（平台可能按 DPI/最小尺寸调整）。
    fn create_slot(
        event_loop: &winit::event_loop::ActiveEventLoop,
        req: OpenRequest,
    ) -> Option<WindowSlot> {
        let OpenRequest { handle, config, app } = req;
        let mut attrs = winit::window::Window::default_attributes().with_title(config.title.clone());
        // Unspecified（None）→ 平台默认尺寸；Some → 显式逻辑尺寸。
        if let (Some(w), Some(h)) = (config.width, config.height) {
            attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h));
        }
        if !config.decorations {
            attrs = attrs.with_decorations(false);
        }
        let window = Arc::new(event_loop.create_window(attrs).ok()?);
        // 开启输入法（IME）：中文等合成输入依赖 Ime 事件，默认窗口未启用。
        window.set_ime_allowed(true);

        let mut renderer = GpuRenderer::new();
        renderer.create_surface_from_window(window.clone());

        let size = window.inner_size();
        let (mut w, mut h) = (size.width as f32, size.height as f32);
        if w <= 0.0 || h <= 0.0 {
            // 某些平台在 create_window 当轮拿不到内尺寸：退回配置的逻辑尺寸（默认 800×600）。
            w = config.width.unwrap_or(800.0) as f32;
            h = config.height.unwrap_or(600.0) as f32;
        }
        let theme = app.theme();
        // 组合期也要能读到 current_window()（内容里可能直接引用自己的窗口句柄）。
        let ctx = WindowCtx::enter(handle);
        let mut app = app;
        let kernel = Kernel::new_sized(theme, w, h, move || app.content());
        drop(ctx);

        // 状态变化 → 只请求本窗口重绘（别的窗口保持休眠）。
        // 用 `Weak` 持窗口：关闭后监听器不会阻止窗口销毁（并在关窗时注销）。
        let weak = Arc::downgrade(&window);
        let root = kernel.root;
        let listener = Recomposer::on_recompose(move || {
            if Recomposer::pending_for_root(root) {
                if let Some(w) = weak.upgrade() {
                    w.request_redraw();
                }
            }
        });

        window.request_redraw();
        Some(WindowSlot {
            handle,
            window,
            renderer,
            kernel,
            listener,
            mouse: (0.0, 0.0),
            resize_border: config.resize_border,
            maximized: false,
        })
    }

    struct Handler {
        /// 下一次「定点唤醒」的墙钟时刻（光标闪烁 / 自动轮播等非连续节奏）。
        next_wake: Option<Instant>,
        /// 上一轮 `advance_frame` 的时刻（多窗口共享同一全局帧时钟）。
        last_tick: Instant,
        /// 本轮事件循环是否已推进过全局帧（`new_events` 复位）。
        advanced: bool,
        /// 最近获得指针 / 焦点的窗口：全局 `cursor` 形状变化只应用到它。
        active: Option<WindowHandle>,
        /// `resumed` 已跑过：只有此后「无窗口」才意味着全部关完（启动瞬间不能误退出）。
        started: bool,
    }

    impl Handler {
        fn new() -> Self {
            Self {
                next_wake: None,
                last_tick: Instant::now(),
                advanced: false,
                active: None,
                started: false,
            }
        }

        /// 执行排队的开窗 / 关窗请求；无窗口时退出事件循环。
        fn drain_requests(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            // 先关后开：「关掉最后一个窗口 + 同轮开新窗」不应被误判为退出。
            let closes = PENDING_CLOSE.with(|q| std::mem::take(&mut *q.borrow_mut()));
            for handle in closes {
                let slot = SLOTS.with(|s| {
                    let mut v = s.borrow_mut();
                    let i = v.iter().position(|x| x.handle == handle)?;
                    Some(v.remove(i))
                });
                if let Some(mut slot) = slot {
                    // 回收该根的作用域 / 节点 / 视口 / 布局约束；注销重组监听；
                    // 随后 drop 掉 `Arc<Window>` 即由平台销毁窗口。
                    Recomposer::off_recompose(slot.listener);
                    slot.kernel.dispose();
                }
            }
            let opens = PENDING_OPEN.with(|q| std::mem::take(&mut *q.borrow_mut()));
            for req in opens {
                if let Some(slot) = create_slot(event_loop, req) {
                    put_slot(slot);
                }
            }
            let ctrls = PENDING_WINCTL.with(|q| std::mem::take(&mut *q.borrow_mut()));
            for (handle, ctl) in ctrls {
                SLOTS.with(|s| {
                    if let Some(slot) = s.borrow().iter().find(|x| x.handle == handle) {
                        match ctl {
                            WinCtl::Fullscreen(on) => slot.window.set_fullscreen(
                                if on {
                                    Some(winit::window::Fullscreen::Borderless(None))
                                } else {
                                    None
                                },
                            ),
                            WinCtl::Minimize(on) => slot.window.set_minimized(on),
                            WinCtl::Maximize(on) => slot.window.set_maximized(on),
                        }
                        slot.window.request_redraw();
                    }
                });
            }
            if self.started && SLOTS.with(|s| s.borrow().is_empty()) {
                event_loop.exit();
            }
        }

        /// 绘制一个窗口。
        ///
        /// 全局帧时钟（[`compose::runtime::advance_frame`]）每轮事件循环只推进一次：
        /// 同轮多个窗口共享同一时间戳与同一次动画采样。
        fn pump(&mut self, slot: &mut WindowSlot) {
            if !self.advanced {
                let now = Instant::now();
                let delta = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
                self.last_tick = now;
                compose::runtime::advance_frame(delta);
                self.advanced = true;
            }
            let size = slot.window.inner_size();
            let (w, h) = (size.width as f32, size.height as f32);
            if w <= 0.0 || h <= 0.0 {
                return; // 最小化：没有可绘制表面
            }
            let ctx = WindowCtx::enter(slot.handle);
            let _ = slot.kernel.draw_frame(&mut slot.renderer, w, h);
            drop(ctx);

            // 续帧按根判定：只有本窗口「有动画」或「有待重组」才继续请求帧，
            // 静态窗口不跟着别的窗口一起 60fps 空转。
            let root = slot.kernel.root;
            let busy = Recomposer::pending_for_root(root)
                || compose::animation::running_animation_roots().contains(&root);
            if busy {
                slot.window.request_redraw();
            }

            // 光标形状：控件/手势层登记的期望光标，仅在变化时调 OS API；
            // 只由最近活动的窗口应用（全局单份状态，避免各窗口互相覆盖）。
            if self.active == Some(slot.handle) {
                if let Some(c) = compose::cursor::take_cursor_change() {
                    slot.window.set_cursor(map_cursor(c));
                }
            }
        }
    }

    impl winit::application::ApplicationHandler for Handler {
        fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            init_text_context(crate::render::default_font_collection());
            // 首次 resumed：创建主窗口；已有窗口（桌面不会二次 resumed）只补一帧。
            if SLOTS.with(|s| s.borrow().is_empty()) {
                if let Some((config, app)) = MAIN_WINDOW.with(|m| m.borrow_mut().take()) {
                    let handle = alloc_handle();
                    if let Some(slot) = create_slot(event_loop, OpenRequest { handle, config, app })
                    {
                        put_slot(slot);
                    }
                }
            }
            self.started = true;
            request_redraw_all();
        }

        fn window_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            window_id: winit::window::WindowId,
            event: winit::event::WindowEvent,
        ) {
            use winit::event::{ElementState, Ime, MouseButton, WindowEvent as WE};
            let Some(mut slot) = take_slot(window_id) else {
                return; // 已关闭窗口的残留事件
            };
            let ctx = WindowCtx::enter(slot.handle);
            let handle = slot.handle;
            // ── 原始输入分流（远程桌面转发）：默认路由前交给已登记的窗口处理器。
            // 处理器返回 true = 已消费（跳过本地 UI 路由）；返回 false = 照常走本地。
            {
                use crate::raw_input::{RawEvent, RawKeyEvent};
                let (mx, my) = slot.mouse;
                let isz = slot.window.inner_size();
                let (sw, sh) = (isz.width as f32, isz.height as f32);
                let raw = match &event {
                    WE::CursorMoved { position, .. } => {
                        Some(RawEvent::Move { x: position.x as f32, y: position.y as f32 })
                    }
                    WE::MouseInput { state, button, .. } => {
                        let b = match button {
                            MouseButton::Left => 0u8,
                            MouseButton::Middle => 1u8,
                            MouseButton::Right => 2u8,
                            _ => 3u8,
                        };
                        match state {
                            ElementState::Pressed => {
                                Some(RawEvent::Press { x: mx, y: my, button: b })
                            }
                            ElementState::Released => {
                                Some(RawEvent::Release { x: mx, y: my, button: b })
                            }
                        }
                    }
                    WE::MouseWheel { delta, .. } => {
                        let (dx, dy) = match delta {
                            winit::event::MouseScrollDelta::LineDelta(x, y) => (*x as f32, *y as f32),
                            winit::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                        };
                        Some(RawEvent::Wheel { x: mx, y: my, dx, dy })
                    }
                    WE::KeyboardInput { event: k, .. } => {
                        let shift = KMOD_SHIFT.with(|s| s.get());
                        let ctrl = KMOD_CTRL.with(|c| c.get());
                        let alt = KMOD_ALT.with(|a| a.get());
                        let key = match &k.logical_key {
                            winit::keyboard::Key::Character(c) => c.to_string(),
                            winit::keyboard::Key::Named(n) => format!("{n:?}"),
                            _ => String::new(),
                        };
                        Some(RawEvent::Key(RawKeyEvent {
                            press: k.state == ElementState::Pressed,
                            key,
                            shift,
                            ctrl,
                            alt,
                        }))
                    }
                    _ => None,
                };
                if let Some(r) = raw {
                    if crate::raw_input::dispatch(&handle, &r, sw, sh) {
                        slot.window.request_redraw();
                        drop(ctx);
                        put_slot(slot);
                        return;
                    }
                }
            }
            match event {
                WE::CloseRequested => {
                    Recomposer::off_recompose(slot.listener);
                    slot.kernel.dispose();
                    drop(slot);
                    drop(ctx);
                    if SLOTS.with(|s| s.borrow().is_empty()) {
                        event_loop.exit();
                    }
                    return;
                }
                WE::Resized(size) => {
                    slot.renderer.resize(size.width, size.height);
                    // Maximize / restore always changes the inner size, so this is
                    // where the WM's real state becomes observable. Compare against
                    // the cached edge and notify only on an actual transition.
                    let m = slot.window.is_maximized();
                    if m != slot.maximized {
                        slot.maximized = m;
                        MAX_HOOKS.with(|v| {
                            for (h, f) in v.borrow_mut().iter_mut() {
                                if *h == handle {
                                    f(m);
                                }
                            }
                        });
                    }
                    slot.window.request_redraw();
                }
                WE::RedrawRequested => self.pump(&mut slot),
                WE::Focused(true) => {
                    self.active = Some(handle);
                    slot.window.request_redraw();
                }
                WE::Focused(false) => {
                    // 键盘焦点跟随操作系统：窗口失焦即释放聚焦控件（否则另一窗口的
                    // 方向键/字符输入会继续打到本窗口的旧焦点上）。
                    compose::runtime::clear_focus();
                    slot.kernel.pointer_leave();
                    slot.window.request_redraw();
                }
                WE::CursorLeft { .. } => {
                    // 指针移出窗口（可能移向另一个窗口）：清本窗口的悬停高亮。
                    slot.kernel.pointer_leave();
                    slot.window.request_redraw();
                }
                WE::CursorMoved { position, .. } => {
                    let (px, py) = (position.x as f32, position.y as f32);
                    slot.mouse = (px, py);
                    self.active = Some(handle);
                    slot.kernel.pointer_move(px, py);
                    slot.window.request_redraw();
                }
                WE::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    let (mx, my) = slot.mouse;
                    self.active = Some(handle);
                    if !try_edge_resize(&mut slot, mx, my) {
                        slot.kernel.pointer_down(mx, my);
                    }
                    slot.window.request_redraw();
                }
                WE::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Right,
                    ..
                } => {
                    // 右键按下即弹上下文菜单；抬起不派发（会命中刚弹出的遮罩而关掉菜单）。
                    let (mx, my) = slot.mouse;
                    self.active = Some(handle);
                    slot.kernel.pointer_secondary_click(mx, my);
                    slot.window.request_redraw();
                }
                WE::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Middle,
                    ..
                } => {
                    let (mx, my) = slot.mouse;
                    self.active = Some(handle);
                    slot.kernel.pointer_middle_click(mx, my);
                    slot.window.request_redraw();
                }
                WE::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    // 点击在抬起时结算（Flutter `onTap` / Compose `clickable` 语义）。
                    let (mx, my) = slot.mouse;
                    self.active = Some(handle);
                    slot.kernel.pointer_up(mx, my);
                    slot.window.request_redraw();
                }
                WE::MouseWheel { delta, .. } => {
                    let (mx, my) = slot.mouse;
                    let dy = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, dy) => dy * 40.0,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    crate::input_router::scroll(dy);
                    slot.kernel.scroll_at(mx, my, -dy);
                    slot.window.request_redraw();
                }
                WE::ModifiersChanged(mods) => {
                    let ms = mods.state();
                    KMOD_SHIFT.with(|s| s.set(ms.shift_key()));
                    KMOD_CTRL.with(|c| c.set(ms.control_key()));
                    KMOD_ALT.with(|a| a.set(ms.alt_key()));
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
                                        slot.window.request_redraw();
                                    }
                                    "x" => {
                                        crate::input_router::cut();
                                        slot.window.request_redraw();
                                    }
                                    "v" => {
                                        crate::input_router::paste();
                                        slot.window.request_redraw();
                                    }
                                    "a" => {
                                        crate::input_router::select_all();
                                        slot.window.request_redraw();
                                    }
                                    "z" => {
                                        if shift {
                                            crate::input_router::redo();
                                        } else {
                                            crate::input_router::undo();
                                        }
                                        slot.window.request_redraw();
                                    }
                                    "y" => {
                                        crate::input_router::redo();
                                        slot.window.request_redraw();
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            match &event.logical_key {
                                winit::keyboard::Key::Character(ch) => {
                                    for c in ch.to_string().chars() {
                                        slot.kernel.char(c);
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Backspace) => {
                                    slot.kernel.backspace()
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter) => {
                                    crate::input_router::enter()
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowUp) => {
                                    slot.kernel.key(NavKey::Up);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowDown) => {
                                    slot.kernel.key(NavKey::Down);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowLeft) => {
                                    slot.kernel.key(NavKey::Left);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowRight) => {
                                    slot.kernel.key(NavKey::Right);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::PageUp) => {
                                    slot.kernel.key(NavKey::PageUp);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::PageDown) => {
                                    slot.kernel.key(NavKey::PageDown);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Home) => {
                                    slot.kernel.key(NavKey::Home);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::End) => {
                                    slot.kernel.key(NavKey::End);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Delete) => {
                                    slot.kernel.key(NavKey::Delete);
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F3) => {
                                    crate::input_router::find_next();
                                    slot.window.request_redraw();
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) => {
                                    crate::input_router::escape();
                                    slot.window.request_redraw();
                                }
                                _ => {}
                            }
                        }
                    }
                    slot.window.request_redraw();
                }
                WE::Ime(ime) => {
                    match ime {
                        Ime::Commit(Text) => crate::input_router::commit(&Text),
                        Ime::Preedit(Text, _) => crate::input_router::preedit(&Text),
                        Ime::Disabled => crate::input_router::preedit(""),
                        _ => {}
                    }
                    slot.window.request_redraw();
                }
                _ => {}
            }
            if DRAG_REQUEST.with(|d| d.get()) == Some(handle) {
                DRAG_REQUEST.with(|d| d.set(None));
                let _ = slot.window.drag_window();
            }
            let pending_resize = RESIZE_REQUEST.with(|r| r.get());
            if let Some((rh, edge)) = pending_resize {
                if rh == handle {
                    RESIZE_REQUEST.with(|r| r.set(None));
                    let dir = match edge {
                        ResizeEdge::North => winit::window::ResizeDirection::North,
                        ResizeEdge::South => winit::window::ResizeDirection::South,
                        ResizeEdge::East => winit::window::ResizeDirection::East,
                        ResizeEdge::West => winit::window::ResizeDirection::West,
                        ResizeEdge::NorthEast => winit::window::ResizeDirection::NorthEast,
                        ResizeEdge::NorthWest => winit::window::ResizeDirection::NorthWest,
                        ResizeEdge::SouthEast => winit::window::ResizeDirection::SouthEast,
                        ResizeEdge::SouthWest => winit::window::ResizeDirection::SouthWest,
                    };
                    let _ = slot.window.drag_resize_window(dir);
                }
            }
            drop(ctx);
            put_slot(slot);
        }

        fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, _event: ()) {
            // 组合/回调里登记的开窗、关窗请求：由事件循环统一执行。
            self.drain_requests(event_loop);
            // 后台线程 post_to_main 唤醒：请求重绘以在各窗口 pump 中排空主线程任务队列。
            request_redraw_all();
        }

        fn new_events(
            &mut self,
            _event_loop: &winit::event_loop::ActiveEventLoop,
            cause: winit::event::StartCause,
        ) {
            // 每轮事件循环推进一次全局帧（多窗口共享；`RedrawRequested` 只做本窗口绘制）。
            self.advanced = false;
            // 定点唤醒到点：驱动一帧（光标翻转 / 轮播切页）。
            if matches!(cause, winit::event::StartCause::ResumeTimeReached { .. }) {
                request_redraw_all();
            }
        }

        fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            use winit::event_loop::ControlFlow;
            self.drain_requests(event_loop);
            // 显式续帧请求（自定义帧驱动 `request_anim_frames`）：没有具体归属根时
            // 让所有窗口各走一帧（各自的 settled 快路径会立即跳过绘制）。
            if compose::animation::animations_requested() {
                request_redraw_all();
            }
            // 统一清除：`draw_frame` 用的是非破坏读取，这里才是「读取即清除」的唯一消费者。
            compose::animation::clear_anim_frames_request();
            // 组合期/帧驱动登记的「稍后再来一帧」（读取即清除）。
            if let Some(delay) = compose::animation::take_wake_delay() {
                let t = Instant::now() + Duration::from_secs_f32(delay.max(0.0));
                self.next_wake = Some(match self.next_wake {
                    Some(cur) if cur <= t => cur,
                    _ => t,
                });
            }
            match self.next_wake {
                Some(t) if Instant::now() >= t => {
                    self.next_wake = None;
                    request_redraw_all();
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
                Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
                // 无动画、无定点唤醒：真正休眠（不再为了「聚焦输入框」无条件重绘）。
                None => event_loop.set_control_flow(ControlFlow::Wait),
            }
        }
    }

    /// 若按下点落在窗口外缘 `resize_border` 内，直接发起 OS 缩放拖拽并返回 true
    /// （跳过本地 UI 命中）；不在边缘或平台不支持则返回 false。指针与内尺寸均为
    /// 物理像素，边框厚度按逻辑像素给出，需乘 scale_factor 换算。
    fn try_edge_resize(slot: &mut WindowSlot, mx: f32, my: f32) -> bool {
        let b = slot.resize_border;
        if b <= 0.0 {
            return false;
        }
        let size = slot.window.inner_size();
        let (sw, sh) = (size.width as f32, size.height as f32);
        if sw <= 0.0 || sh <= 0.0 {
            return false;
        }
        let band = b * slot.window.scale_factor() as f32;
        let left = mx <= band;
        let right = mx >= sw - band;
        let top = my <= band;
        let bottom = my >= sh - band;
        use winit::window::ResizeDirection as RD;
        let dir = if top && left {
            RD::NorthWest
        } else if top && right {
            RD::NorthEast
        } else if bottom && left {
            RD::SouthWest
        } else if bottom && right {
            RD::SouthEast
        } else if left {
            RD::West
        } else if right {
            RD::East
        } else if top {
            RD::North
        } else if bottom {
            RD::South
        } else {
            return false;
        };
        slot.window.drag_resize_window(dir).is_ok()
    }

    /// 中性光标 → winit 光标形状。
    fn map_cursor(c: compose::cursor::Cursor) -> winit::window::CursorIcon {
        use compose::cursor::Cursor as C;
        use winit::window::CursorIcon as I;
        match c {
            C::Default => I::Default,
            C::Pointer => I::Pointer,
            C::Text => I::Text,
            C::Move => I::Move,
            C::Grabbing => I::Grabbing,
            C::ResizeEw => I::EwResize,
            C::ResizeNs => I::NsResize,
            C::ResizeNwse => I::NwseResize,
            C::ResizeNesw => I::NeswResize,
            C::NotAllowed => I::NotAllowed,
        }
    }

    /// 带窗口配置的宿主入口（桌面 / Web）——创建主窗口。
    pub fn run_app_with(config: WindowConfig, app: impl App + 'static) {
        MAIN_WINDOW.with(|m| *m.borrow_mut() = Some((config, Box::new(app))));
        let event_loop =
            winit::event_loop::EventLoop::<()>::new().expect("event loop");
        let proxy = event_loop.create_proxy();
        PROXY.with(|p| *p.borrow_mut() = Some(proxy.clone()));
        // 跨线程唤醒：后台线程 post_to_main 后经此代理泵醒事件循环。
        compose::runtime::set_wake_hook(move || {
            let _ = proxy.send_event(());
        });

        // 桌面入口：阻塞至所有窗口关闭。
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut handler = Handler::new();
            event_loop.run_app(&mut handler).expect("run_app");
        }
        // Web 入口：`spawn_app` 非阻塞（main 立即返回，浏览器 rAF 保活）。
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::EventLoopExtWebSys;
            event_loop.spawn_app(Handler::new());
        }
    }

    /// 默认窗口配置的宿主入口。
    pub fn run_app(app: impl App + 'static) {
        run_app_with(WindowConfig::default(), app);
    }
}

// 桌面 / Web 宿主入口（Android 走 android-activity，iOS / OHOS 走宿主驱动 C ABI）。
#[cfg(all(
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
pub use winit_host::{
    close_window, current_window, on_maximized_changed, open_window, run_app, run_app_with,
    set_fullscreen,
    set_maximized, set_minimized, start_window_drag, start_window_resize, window_count,
    ResizeEdge, WindowConfig, WindowHandle,
};
