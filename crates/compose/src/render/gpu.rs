//! GPU 渲染器 — wgpu 30 + valo 0.18（共享后端，经验证的管线）。
//!
//! ```Text
//!   wgpu::Instance → Adapter → Device + Queue
//!                              │
//!                    valo::Context (GPU 渲染引擎)
//!                              │
//!                    valo::Surface (窗口表面，按平台创建)
//!                              │
//!                    render_frame() → acquire → render → present
//! ```
//!
//! Surface 创建按平台拆分（见各 `create_surface_from_*`），其余渲染管线完全共用。

use std::sync::Arc;

use compose::prelude::Color;
use winit::window::Window;

pub struct GpuRenderer {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    context: valo::Context,
    surface: Option<valo::Surface>,
    width: u32,
    height: u32,
}

impl GpuRenderer {
    /// 创建渲染器（pollster 阻塞初始化，须在创建 Surface 之前调用）。
    pub fn new() -> Self {
        // Android：单后端实例，规避 wgpu #2384 的 EGL 窗口重复连接（实践经验）。
        #[cfg(target_os = "android")]
        let backends = Self::pick_android_backend();
        #[cfg(not(target_os = "android"))]
        let backends = wgpu::Backends::all();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("Failed to find a suitable GPU adapter");

        eprintln!("[GpuRenderer] Adapter: {:?}", adapter.get_info().name);

        // 按适配器实际上限请求设备（跨后端稳健）
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("compose GPU device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                ..Default::default()
            },
        ))
        .expect("Failed to create wgpu device");

        let context = valo::Context::new(device, queue);
        Self {
            instance,
            adapter,
            context,
            surface: None,
            width: 0,
            height: 0,
        }
    }

    /// 探测阶段决定 Android 用哪个后端：真机优先 Vulkan，模拟器无可用 Vulkan 时退回 GLES。
    #[cfg(target_os = "android")]
    fn pick_android_backend() -> wgpu::Backends {
        let probe = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = pollster::block_on(
            probe.enumerate_adapters(wgpu::Backends::VULKAN | wgpu::Backends::GL),
        );
        let is_emulator_vulkan = |a: &wgpu::Adapter| -> bool {
            let info = a.get_info();
            if info.backend != wgpu::Backend::Vulkan {
                return false;
            }
            let sig = format!("{} {}", info.name, info.device).to_lowercase();
            ["goldfish", "ranchu", "gfxstream", "emulator", "crosvm", "virgl", "qemu", "cuttlefish", "swiftshader"]
                .iter()
                .any(|k| sig.contains(k))
        };
        let has_real_vulkan = adapters
            .iter()
            .any(|a| a.get_info().backend == wgpu::Backend::Vulkan && !is_emulator_vulkan(a));
        let chosen = if has_real_vulkan {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::GL
        };
        eprintln!("[GpuRenderer] Android backend pick: {:?}", chosen);
        drop(probe);
        chosen
    }

    /// 从 winit 窗口创建 valo Surface（桌面端）。
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android"), not(target_env = "ohos")))]
    pub fn create_surface_from_window(&mut self, window: Arc<Window>) {
        let size = window.inner_size();
        self.width = size.width;
        self.height = size.height;
        let surface = valo::Surface::new(
            &self.instance,
            &self.adapter,
            self.context.device(),
            window,
            [self.width, self.height],
        )
        .expect("Failed to create valo Surface from window");
        self.surface = Some(surface);
    }

    /// 从 winit 窗口创建 valo Surface（Web / wasm32）。
    #[cfg(target_arch = "wasm32")]
    pub fn create_surface_from_window(&mut self, window: Arc<Window>) {
        let size = window.inner_size();
        self.width = size.width;
        self.height = size.height;
        let surface_target = unsafe {
            wgpu::SurfaceTargetUnsafe::from_window(&window)
                .expect("Failed to get raw window handle from winit Window (wasm32)")
        };
        let surface = unsafe {
            valo::Surface::new_unsafe(
                &self.instance,
                &self.adapter,
                self.context.device(),
                surface_target,
                [self.width, self.height],
            )
            .expect("Failed to create valo Surface from winit Window (wasm32)")
        };
        self.surface = Some(surface);
    }

    /// 从 Android NativeWindow 创建 valo Surface。
    #[cfg(target_os = "android")]
    pub fn create_surface_from_native_window(&mut self, native_window: &ndk::native_window::NativeWindow) {
        let width = native_window.width() as u32;
        let height = native_window.height() as u32;
        self.width = width;
        self.height = height;

        let window_handle = raw_window_handle::RawWindowHandle::AndroidNdk(
            raw_window_handle::AndroidNdkWindowHandle::new(native_window.ptr().cast()),
        );
        let display_handle = raw_window_handle::RawDisplayHandle::Android(
            raw_window_handle::AndroidDisplayHandle::new(),
        );
        let surface_target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_window_handle: window_handle,
            raw_display_handle: Some(display_handle),
        };

        let surface = unsafe {
            valo::Surface::new_unsafe(
                &self.instance,
                &self.adapter,
                self.context.device(),
                surface_target,
                [width, height],
            )
            .expect("Failed to create valo Surface from Android NativeWindow")
        };
        self.surface = Some(surface);
    }

    /// 从 iOS `UIView` 创建 valo Surface（iOS 嵌入模式）。
    #[cfg(target_os = "ios")]
    pub fn create_surface_from_metal_layer(&mut self, view: *mut std::ffi::c_void, width: u32, height: u32) {
        assert!(!view.is_null(), "UIView pointer is null");
        self.width = width;
        self.height = height;

        let window_handle = raw_window_handle::RawWindowHandle::UiKit(
            raw_window_handle::UiKitWindowHandle::new(
                std::ptr::NonNull::new(view).expect("UIView pointer is null"),
            ),
        );
        let display_handle =
            raw_window_handle::RawDisplayHandle::UiKit(raw_window_handle::UiKitDisplayHandle::new());
        let surface_target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_window_handle: window_handle,
            raw_display_handle: Some(display_handle),
        };

        let surface = unsafe {
            valo::Surface::new_unsafe(
                &self.instance,
                &self.adapter,
                self.context.device(),
                surface_target,
                [width, height],
            )
            .expect("Failed to create valo Surface from UIView")
        };
        self.surface = Some(surface);
        eprintln!("[GpuRenderer] iOS Metal Surface created {}x{}", width, height);
    }

    /// 从 HarmonyOS OHNativeWindow 创建 valo Surface。
    ///
    /// OHNativeWindow 与 ANativeWindow 二进制兼容，复用 `AndroidWindowHandle`。
    /// 窗口尺寸由调用方（compose-ohos）经 NDK FFI 查询后传入。
    #[cfg(target_env = "ohos")]
    pub fn create_surface_from_ohos_window(&mut self, native_window: *mut std::ffi::c_void, width: u32, height: u32) {
        assert!(!native_window.is_null(), "OHNativeWindow pointer is null");
        self.width = width;
        self.height = height;

        let window_handle = raw_window_handle::RawWindowHandle::Android(
            raw_window_handle::AndroidWindowHandle::new(
                std::ptr::NonNull::new(native_window).expect("OHNativeWindow pointer is null"),
            ),
        );
        let display_handle = raw_window_handle::RawDisplayHandle::Android(
            raw_window_handle::AndroidDisplayHandle::new(),
        );
        let surface_target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_window_handle: window_handle,
            raw_display_handle: Some(display_handle),
        };

        let surface = unsafe {
            valo::Surface::new_unsafe(
                &self.instance,
                &self.adapter,
                self.context.device(),
                surface_target,
                [width, height],
            )
            .expect("Failed to create valo Surface from OHNativeWindow")
        };
        self.surface = Some(surface);
        eprintln!("[GpuRenderer] OHOS Surface created {}x{}", width, height);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.width = width;
        self.height = height;
        if let Some(surface) = self.surface.as_mut() {
            surface.resize([width, height]);
        }
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }

    /// 显式丢弃当前 Surface（Android/OHOS 重建窗口表面前必须先调用，断开 EGL 连接）。
    pub fn discard_surface(&mut self) {
        self.surface = None;
    }

    /// 渲染一帧：直渲快路径（本框架不含 backdrop 类离屏命令）。
    pub fn render_frame(
        &mut self,
        display_list: std::sync::Arc<valo::DisplayList>,
        bg_color: Color,
    ) {
        let surface = match self.surface.as_mut() {
            Some(s) => s,
            None => return,
        };
        let t0 = std::time::Instant::now();
        let frame = match surface.acquire() {
            Ok(f) => f,
            Err(refused) => {
                eprintln!("[GpuRenderer] Surface acquire failed: {:?}", refused);
                return;
            }
        };
        let t1 = std::time::Instant::now();
        let target = frame.target(Some(valo::Color::rgba(bg_color.r, bg_color.g, bg_color.b, bg_color.a)));
        self.context.render(&display_list, &target);
        let t2 = std::time::Instant::now();
        self.context.present(frame);
        let t3 = std::time::Instant::now();
        gpu_split(
            t1.duration_since(t0).as_secs_f32() * 1e3,
            t2.duration_since(t1).as_secs_f32() * 1e3,
            t3.duration_since(t2).as_secs_f32() * 1e3,
        );
    }

    /// 把一段 RGBA8 像素上传为 GPU 纹理，返回可按 `image_id` 复用的 `valo::Image` 句柄。
    ///
    /// 像素必须是 `4 * w * h` 字节（行优先）。失败（尺寸/格式不符）由 valo 内部 panic，
    /// 调用方需保证像素来自已成功解码的位图。
    pub fn upload_image(&mut self, _id: u64, pixels: &[u8], w: u32, h: u32) -> valo::Image {
        let desc = valo::ImageDesc {
            size: [w, h],
            premultiplied: false,
            mips: true,
        };
        self.context.upload_image(desc, pixels)
    }
}

/// 临时诊断：把 acquire / plan+encode / present 三段耗时按 60 帧窗口打到 stderr。
fn gpu_split(acq: f32, render: f32, present: f32) {
    if std::env::var("COMPOSE_TRACE_FRAMES").is_err() {
        return;
    }
    struct A {
        n: u32,
        s: [f32; 3],
        clock: std::time::Instant,
    }
    thread_local! {
        static ACC: std::cell::RefCell<Option<A>> = const { std::cell::RefCell::new(None) };
    }
    ACC.with(|cell| {
        let mut b = cell.borrow_mut();
        let acc = b.get_or_insert(A { n: 0, s: [0.0; 3], clock: std::time::Instant::now() });
        acc.n += 1;
        acc.s[0] += acq;
        acc.s[1] += render;
        acc.s[2] += present;
        if acc.n < 60 {
            return;
        }
        let secs = acc.clock.elapsed().as_secs_f32().max(1e-6);
        let n = acc.n as f32;
        eprintln!(
            "[gpu] {:5.1} fps | ms/frame acq={:.2} render={:.2} present={:.2} | wall={:.2}%",
            n / secs,
            acc.s[0] / n,
            acc.s[1] / n,
            acc.s[2] / n,
            (acc.s.iter().sum::<f32>() / n) * (n / secs) / 10.0,
        );
        *acc = A { n: 0, s: [0.0; 3], clock: std::time::Instant::now() };
    });
}
