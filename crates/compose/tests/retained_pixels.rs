//! 保留模式拼帧的像素回归：`Kernel::build_chunked_frame` 的无头等价物。
//!
//! 生产配方 = 一条 DisplayList：整屏背景 + 静态块（按内容键复用的 DisplayList 嵌进来）
//! 与现场块（逐命令直接录）交替，**顺序即绘制树顺序**。
//!
//! 这里钉住两件事：
//! ① 嵌套的静态块不改变墨迹（段落尤其容易在嵌套/离屏里丢字）；
//! ② 现场块夹在两个静态块中间时，必须画在两者之间 —— 旧方案把静态层整屏贴图、
//!    动态段一律压在其上，实测把 story 首页侧栏 / 表格文字整片盖掉。

use std::sync::Arc;

use compose::layout::{DrawCmd, TextStyle};
use compose::prelude::Color as C;
use compose::render::{
    build_cmds_display_list, build_display_list, default_font_collection, new_frame_builder,
    record_cmds, TextContext,
};
use valo::{Color, Context, DisplayListBuilder, Offscreen, Paint, Rect};

const SIZE: u32 = 64;

fn headless() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    let limits = adapter.limits();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("test"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue))
}

fn rect_dl(x: f32, color: Color) -> Arc<valo::DisplayList> {
    let mut b = DisplayListBuilder::new();
    b.draw_rect(Rect::new(x, 8.0, 24.0, 24.0), &Paint::from_color(color));
    Arc::new(b.build())
}

/// `Offscreen` 像素回读（Rgba8）。
fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, off: &Offscreen) -> Vec<u8> {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(SIZE * 4 * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: off.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range().unwrap().to_vec();
    buf.unmap();
    data
}

fn px(data: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE * 4) + x * 4) as usize;
    [data[i], data[i + 1], data[i + 2], data[i + 3]]
}

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// 一条含真实段落的静态块 DisplayList（白底 + 黑字）。
fn ink_dl(tc: &mut TextContext) -> Arc<valo::DisplayList> {
    let style = TextStyle {
        color: C::rgb(0.0, 0.0, 0.0),
        size: 24.0,
        weight: 700,
        italic: false,
        family: String::new(),
        line_height: None,
        letter_spacing: 0.0,
    };
    let cmds = [DrawCmd::Text {
        x: 2.0,
        y: 16.0,
        text: "HH".to_string(),
        style,
        max_w: 1000.0,
    }];
    Arc::new(build_cmds_display_list(&cmds, tc))
}

/// 深色像素数（文本墨迹）。
fn dark_px(data: &[u8]) -> usize {
    data.chunks_exact(4)
        .filter(|p| p[0] < 128 && p[1] < 128 && p[2] < 128)
        .count()
}

/// 静态块嵌进本帧（生产用的就是这条）。
fn embed(b: &mut DisplayListBuilder, dl: &Arc<valo::DisplayList>) {
    b.draw_display_list(dl);
}

#[test]
fn embedded_static_chunks_keep_paragraph_ink() {
    let Some((device, queue)) = headless() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut context = Context::new(device.clone(), queue.clone());
    let mut tc = TextContext::new(default_font_collection());
    let dl = ink_dl(&mut tc);

    // ① 直绘（= 非保留模式）作基准
    let a = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&dl, &a.target(None));
    // ② 拼帧配方：builder + 嵌套同一条静态块
    let mut b = new_frame_builder(None);
    embed(&mut b, &dl);
    let c = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&b.build(), &c.target(None));
    queue.submit([]);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let (da, db) = (read_back(&device, &queue, &a), read_back(&device, &queue, &c));
    assert!(dark_px(&da) > 20, "基准：直绘段落应有墨迹（无字体则本用例无效）");
    assert_eq!(da, db, "静态块嵌套后段落墨迹发生变化");
}

/// 现场块夹在两个静态块之间 → 必须压在第一个之上、第二个之下。
///
/// 旧的两层贴图方案在这里会把绿色盖在蓝色之上（动态段永远画在静态层贴图之上）。
#[test]
fn live_chunk_between_static_chunks_keeps_z_order() {
    let Some((device, queue)) = headless() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut context = Context::new(device.clone(), queue.clone());
    let mut tc = TextContext::new(default_font_collection());

    let first = rect_dl(8.0, RED);
    let last = rect_dl(8.0, BLUE);
    let mut b = new_frame_builder(Some(compose::prelude::Color::rgb(1.0, 1.0, 1.0)));
    embed(&mut b, &first);
    // 现场块：同一位置的绿色块 + 一段文字（文字画在其后，必须仍可见）
    let live = [DrawCmd::FillRect {
        x: 8.0,
        y: 8.0,
        w: 24.0,
        h: 24.0,
        color: C::rgb(0.0, 1.0, 0.0),
    }];
    record_cmds(&mut b, &live, &mut tc);
    embed(&mut b, &last);
    let ink = ink_dl(&mut tc);
    embed(&mut b, &ink);

    let off = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&b.build(), &off.target(Some(GREEN)));
    queue.submit([]);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let data = read_back(&device, &queue, &off);
    assert_eq!(px(&data, 20, 20)[0..3], [0, 0, 255], "后来的静态块须盖住现场块");
    assert!(dark_px(&data) > 20, "末尾嵌套的段落墨迹没落到目标上");
    assert_eq!(px(&data, 2, 60)[0..3], [255, 255, 255], "整屏背景应打底");
}

/// 逐命令对齐：分块拼帧的像素必须等于「整条流一次录制」（平铺路径）。
#[test]
fn chunked_frame_equals_flat_frame() {
    let Some((device, queue)) = headless() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut context = Context::new(device.clone(), queue.clone());
    let mut tc = TextContext::new(default_font_collection());

    let bg = compose::prelude::Color::rgb(0.5, 0.5, 0.5);
    let style = TextStyle {
        color: C::rgb(0.0, 0.0, 0.0),
        size: 20.0,
        weight: 400,
        italic: false,
        family: String::new(),
        line_height: None,
        letter_spacing: 0.0,
    };
    let all = vec![
        DrawCmd::FillRect { x: 2.0, y: 2.0, w: 40.0, h: 40.0, color: C::rgb(1.0, 0.0, 0.0) },
        DrawCmd::Text {
            x: 4.0,
            y: 24.0,
            text: "HH".to_string(),
            style: style.clone(),
            max_w: 600.0,
        },
        // 现场块（动画）：与后面的静态文字重叠，顺序错了就丢字
        DrawCmd::FillRect { x: 20.0, y: 8.0, w: 20.0, h: 20.0, color: C::rgb(0.0, 1.0, 0.0) },
        DrawCmd::Text {
            x: 22.0,
            y: 12.0,
            text: "II".to_string(),
            style,
            max_w: 600.0,
        },
    ];
    let (static_a, live, static_b) = (&all[0..2], &all[2..3], &all[3..4]);

    // 平铺：整条流一次录制
    let flat = Arc::new(build_display_list(&all, bg, &mut tc));
    let a = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&flat, &a.target(None));

    // 分块：静态块各自成一条可复用的 DisplayList，现场块直接录
    let sa = build_cmds_display_list(static_a, &mut tc);
    let sb = build_cmds_display_list(static_b, &mut tc);
    let (sa, sb) = (Arc::new(sa), Arc::new(sb));
    let mut b = new_frame_builder(Some(bg));
    embed(&mut b, &sa);
    record_cmds(&mut b, live, &mut tc);
    embed(&mut b, &sb);
    let chunked = Arc::new(b.build());
    let c = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&chunked, &c.target(None));
    queue.submit([]);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let (da, dc) = (read_back(&device, &queue, &a), read_back(&device, &queue, &c));
    assert!(dark_px(&da) > 20, "基准无效：平铺路径没画出文字");
    assert_eq!(da, dc, "分块拼帧与平铺逐像素不一致");
}

/// 分块方案的地基：**嵌回原位置的静态块继承外层裁剪栈**。
///
/// 生产里 `ClipPush`/`ClipPop` 永远现场录进外层构建器，被裁的墨迹录在独立的静态块里
/// 再嵌回来。若 valo 的嵌套列表不带回外层裁剪，块内那块「裁剪框外的红」就会显形，
/// 表现为裁剪容器里的内容溢出到容器外。
#[test]
fn embedded_static_chunk_inherits_enclosing_clip() {
    let Some((device, queue)) = headless() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut context = Context::new(device.clone(), queue.clone());
    let mut tc = TextContext::new(default_font_collection());

    let clip = DrawCmd::ClipPush { x: 0.0, y: 0.0, w: 32.0, h: 32.0, radius: 0.0 };
    let inside = DrawCmd::FillRect { x: 8.0, y: 8.0, w: 16.0, h: 16.0, color: C::rgb(1.0, 0.0, 0.0) };
    let outside =
        DrawCmd::FillRect { x: 40.0, y: 8.0, w: 16.0, h: 16.0, color: C::rgb(1.0, 0.0, 0.0) };
    let flat = vec![clip.clone(), inside.clone(), outside.clone(), DrawCmd::ClipPop];

    // 平铺基准
    let a = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&Arc::new(build_display_list(&flat, C::rgb(1.0, 1.0, 1.0), &mut tc)), &a.target(None));

    // 生产分块：裁剪指令现场录，墨迹各自成静态块嵌回原位
    let cached_in = Arc::new(build_cmds_display_list(std::slice::from_ref(&inside), &mut tc));
    let cached_out = Arc::new(build_cmds_display_list(std::slice::from_ref(&outside), &mut tc));
    let mut b = new_frame_builder(Some(compose::prelude::Color::rgb(1.0, 1.0, 1.0)));
    record_cmds(&mut b, &[clip.clone()], &mut tc);
    embed(&mut b, &cached_in);
    embed(&mut b, &cached_out);
    record_cmds(&mut b, &[DrawCmd::ClipPop], &mut tc);
    let c = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&Arc::new(b.build()), &c.target(None));
    queue.submit([]);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let (da, dc) = (read_back(&device, &queue, &a), read_back(&device, &queue, &c));
    assert_eq!(px(&da, 16, 16)[0..2], [255, 0], "基准：裁剪框内的红须在");
    assert_eq!(px(&da, 48, 16)[0..2], [255, 255], "基准：裁剪框外不该有红");
    assert_eq!(da, dc, "静态块没继承外层裁剪（裁剪容器内容溢出）");
}

#[test]
fn persistent_canvas_is_not_needed_ink_lands_directly() {
    let Some((device, queue)) = headless() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut context = Context::new(device.clone(), queue.clone());

    // 单趟：背景 + 嵌套静态块 + 现场段，不再经任何离屏层。
    let mut b = new_frame_builder(Some(compose::prelude::Color::rgb(1.0, 1.0, 1.0)));
    embed(&mut b, &rect_dl(8.0, RED));
    let mut tc = TextContext::new(default_font_collection());
    record_cmds(
        &mut b,
        &[DrawCmd::FillRect { x: 36.0, y: 8.0, w: 20.0, h: 20.0, color: C::rgb(0.0, 0.0, 1.0) }],
        &mut tc,
    );
    let off = Offscreen::new(&device, [SIZE, SIZE]);
    context.render(&b.build(), &off.target(Some(GREEN)));
    queue.submit([]);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let data = read_back(&device, &queue, &off);
    assert_eq!(px(&data, 20, 20)[0..3], [255, 0, 0], "嵌套静态块的墨迹没落到目标上");
    assert_eq!(px(&data, 46, 20)[0..3], [0, 0, 255], "现场段应画在目标上");
    assert_eq!(px(&data, 2, 60)[0..3], [255, 255, 255], "整屏背景应盖住清屏色");
}
