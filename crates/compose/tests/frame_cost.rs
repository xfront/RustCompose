//! 每帧成本基准（无头、无 GPU）：把「布局 / 绘制遍历 / DisplayList 构建」三段耗时量化。
//!
//! 运行：`cargo test -p compose --test frame_cost -- --ignored --nocapture`
//!
//! 场景规模对齐 story 动画页（约 200 节点 / 900 条命令，含文本）。
//! 两个用例分别回答：
//! ① 只有一个叶子在动（纯绘制失效）时，每帧仍要付多少？
//! ② 动画驱动重组（脏区传到根）时，布局要多付多少？

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, DrawBuilder, Modifier, NodeId, Policy, TextStyle, add_node, create_root_node,
    draw_tree_tagged, invalidate_draw_node, perform_layout, with_content_scope,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, mutableStateOf, with_rt_pub};
use compose::render::{TextContext, build_display_list};

const VP_W: f32 = 1080.0;
const VP_H: f32 = 800.0;
const GROUPS: usize = 40;
const PER_GROUP: usize = 5;

fn vp() -> Constraints {
    Constraints::new(0.0, VP_W, 0.0, VP_H)
}

struct Scene {
    root: NodeId,
    /// 每帧改变颜色的那个叶子
    hot: NodeId,
    tick: Rc<Cell<u32>>,
    v: compose::runtime::MutableState<i32>,
}

/// 组 → 叶子（矩形 + 文本）的两层树；`hot` 叶子的填充色由 `tick` 决定。
fn build() -> Scene {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let tick = Rc::new(Cell::new(0u32));
    let hot = Rc::new(RefCell::new(None::<NodeId>));
    let v = mutableStateOf(0i32);
    let (tick_c, hot_c) = (tick.clone(), hot.clone());
    comp.set_content(move || {
        let (tick_c, hot_c) = (tick_c.clone(), hot_c.clone());
        MaterialTheme(ColorScheme::light(), move || {
            let (tick_o, hot_o) = (tick_c.clone(), hot_c.clone());
            let _ = v.get(); // 组合期读取 → set 时定向重组本作用域
            for g in 0..GROUPS {
                let (tick_g, hot_g) = (tick_o.clone(), hot_o.clone());
                let gy = (g as f32) * 18.0;
                let gid = add_node(
                    Policy::Box(Alignment::TopStart),
                    Modifier::new().size(VP_W, 18.0).offset(0.0, gy),
                );
                with_content_scope(gid, move || {
                    let (tick_c, hot_c) = (tick_g.clone(), hot_g.clone());
                        for i in 0..PER_GROUP {
                            let x = (i as f32) * 200.0;
                            let id = add_node(
                                Policy::Leaf,
                                Modifier::new().size(190.0, 16.0).offset(x, 0.0),
                            );
                            if g == 0 && i == 0 {
                                hot_c.replace(Some(id));
                            }
                            let is_hot = g == 0 && i == 0;
                            let t = tick_c.clone();
                            let _ = &hot_c;
                            with_rt_pub(move |rt| {
                                rt.nodes[id.index()].draw =
                                    Some(Box::new(move |b: &mut DrawBuilder, px, py, w, h| {
                                        let n = t.get();
                                        let color = if is_hot && n % 2 == 0 {
                                            Color::rgb(1.0, 0.0, 0.0)
                                        } else {
                                            Color::rgb(0.9, 0.9, 0.95)
                                        };
                                        b.fill_rect(px, py, w, h, color);
                                        b.draw_text(
                                            px + 4.0,
                                            py + 2.0,
                                            &format!("group {g} item {i} frame {n}"),
                                            &TextStyle::new().size(12.0),
                                            f32::INFINITY,
                                        );
                                        b.fill_circle(px + w - 8.0, py + h / 2.0, 5.0, color);
                                    }));
                            });
                        }
                    },
                );
            }
        });
    });
    let _ = perform_layout(root, vp());
    let _ = draw_tree_tagged(root);
    let _ = perform_layout(root, vp());
    let _ = draw_tree_tagged(root);
    Scene { root, hot: hot.borrow().unwrap(), tick, v }
}

fn text_ctx() -> TextContext {
    TextContext::new(valo::FontCollection::default())
}

/// ① 纯绘制失效：只有一个叶子在动。
#[test]
#[ignore = "性能基准，手动运行"]
fn bench_draw_only() {
    let s = build();
    let mut tc = text_ctx();
    let mut lay = 0.0;
    let mut draw = 0.0;
    let mut build_ms = 0.0;
    let mut cmds = 0usize;
    for f in 0..200 {
        s.tick.set(f);
        invalidate_draw_node(s.hot);
        let t0 = Instant::now();
        let _ = perform_layout(s.root, vp());
        let t1 = Instant::now();
        let (c, _o) = draw_tree_tagged(s.root);
        let t2 = Instant::now();
        let dl = build_display_list(&c, Color::WHITE, &mut tc);
        let t3 = Instant::now();
        if f > 20 {
            lay += t1.duration_since(t0).as_secs_f32() * 1e3;
            draw += t2.duration_since(t1).as_secs_f32() * 1e3;
            build_ms += t3.duration_since(t2).as_secs_f32() * 1e3;
        }
        cmds = c.len();
        let _ = dl;
    }
    let n = 180.0f32;
    println!(
        "[draw-only] cmds={cmds} | ms/frame lay={:.2} draw={:.2} build={:.2} total={:.2}",
        lay / n,
        draw / n,
        build_ms / n,
        (lay + draw + build_ms) / n
    );
}

/// ③ 全链路（含 valo GPU 编码）：把「命令 → DisplayList → wgpu 提交」的成本量化。
#[test]
#[ignore = "性能基准，手动运行"]
fn bench_gpu_encode() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("SKIP bench_gpu_encode: no GPU adapter");
        return;
    };
    let mut context = valo::Context::new(device.clone(), queue.clone());
    let off = valo::Offscreen::new(&device, [VP_W as u32, VP_H as u32]);
    let s = build();
    let mut tc = TextContext::new(compose::render::default_font_collection());
    let (mut draw_ms, mut build_ms, mut gpu_ms) = (0.0f32, 0.0f32, 0.0f32);
    for f in 0..200 {
        s.tick.set(f);
        invalidate_draw_node(s.hot);
        let _ = perform_layout(s.root, vp());
        let t0 = Instant::now();
        let (c, _o) = draw_tree_tagged(s.root);
        let t1 = Instant::now();
        let dl = std::sync::Arc::new(build_display_list(&c, Color::WHITE, &mut tc));
        let t2 = Instant::now();
        context.render(&dl, &off.target(Some(valo::Color::WHITE)));
        let t3 = Instant::now();
        if f > 20 {
            draw_ms += t1.duration_since(t0).as_secs_f32() * 1e3;
            build_ms += t2.duration_since(t1).as_secs_f32() * 1e3;
            gpu_ms += t3.duration_since(t2).as_secs_f32() * 1e3;
        }
    }
    let n = 180.0f32;
    println!(
        "[full-chain] ms/frame draw={:.2} build={:.2} valo_render={:.2} total={:.2}",
        draw_ms / n,
        build_ms / n,
        gpu_ms / n,
        (draw_ms + build_ms + gpu_ms) / n
    );
}

/// ④ 嵌套 DisplayList 是否让 valo 跳过未变化内容的重复编码？
#[test]
#[ignore = "性能基准，手动运行"]
fn bench_nesting() {
    use compose::layout::DrawCmd;
    use std::collections::HashMap;
    let Some((device, queue)) = headless_device() else {
        eprintln!("SKIP bench_nesting: no GPU adapter");
        return;
    };
    let mut context = valo::Context::new(device.clone(), queue.clone());
    let off = valo::Offscreen::new(&device, [VP_W as u32, VP_H as u32]);
    let s = build();
    let mut tc = TextContext::new(compose::render::default_font_collection());

    // 上一帧每个分块的命令，用于判断该块是否需要重建
    let mut last_chunk: HashMap<usize, Vec<DrawCmd>> = HashMap::new();
    let mut cached: HashMap<usize, std::sync::Arc<valo::DisplayList>> = HashMap::new();
    let (mut flat, mut nest_new, mut nest_same, mut parts) = (0.0f32, 0.0f32, 0.0f32, 0usize);

    for f in 0..150 {
        s.tick.set(f);
        invalidate_draw_node(s.hot);
        let _ = perform_layout(s.root, vp());
        let (c, owners) = draw_tree_tagged(s.root);
        // 按归属节点分块（同属一节点的命令保持相邻 → 绘制顺序不变）
        let mut chunks: Vec<(u32, Vec<DrawCmd>)> = Vec::new();
        for (i, cmd) in c.iter().enumerate() {
            let o = owners[i];
            if chunks.last().is_none_or(|&(last, _)| last != o) {
                chunks.push((o, Vec::new()));
            }
            chunks.last_mut().unwrap().1.push(cmd.clone());
        }
        parts = chunks.len();

        let t0 = Instant::now();
        let dl = std::sync::Arc::new(build_display_list(&c, Color::WHITE, &mut tc));
        context.render(&dl, &off.target(Some(valo::Color::WHITE)));
        let t1 = Instant::now();

        // 嵌套：每块一个子 DisplayList（本帧全部重建）
        let mut b = valo::DisplayListBuilder::new();
        for (k, (_o, cmds)) in chunks.iter().enumerate() {
            let sub = std::sync::Arc::new(build_display_list(cmds, Color::WHITE, &mut tc));
            b.draw_display_list(&sub);
            cached.insert(k, sub);
        }
        let list = std::sync::Arc::new(b.build());
        context.render(&list, &off.target(Some(valo::Color::WHITE)));
        let t2 = Instant::now();

        // 嵌套 + 复用：命令未变的块直接用上一帧的 Arc（valo 缓存候选）
        let mut b = valo::DisplayListBuilder::new();
        for (k, (_o, cmds)) in chunks.iter().enumerate() {
            let same = last_chunk.get(&k).is_some_and(|p| p == cmds);
            let sub = if same {
                cached[&k].clone()
            } else {
                let sub = std::sync::Arc::new(build_display_list(cmds, Color::WHITE, &mut tc));
                cached.insert(k, sub.clone());
                last_chunk.insert(k, cmds.clone());
                sub
            };
            b.draw_display_list_cached(&sub);
        }
        let list = std::sync::Arc::new(b.build());
        context.render(&list, &off.target(Some(valo::Color::WHITE)));
        let t3 = Instant::now();

        if f > 20 {
            flat += t1.duration_since(t0).as_secs_f32() * 1e3;
            nest_new += t2.duration_since(t1).as_secs_f32() * 1e3;
            nest_same += t3.duration_since(t2).as_secs_f32() * 1e3;
        }
    }
    let n = 130.0f32;
    println!(
        "[nesting] chunks={parts} | ms/frame flat={:.2} nested_rebuild={:.2} nested_reuse={:.2}",
        flat / n,
        nest_new / n,
        nest_same / n
    );
}

fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    let limits = adapter.limits();
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("bench"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .ok()
}

/// ② 重组驱动：状态变化 → 定向重组 → 脏区传到根 → 全树重布局。
#[test]
#[ignore = "性能基准，手动运行"]
fn bench_recompose() {
    let s = build();
    let mut tc = text_ctx();
    let mut adv = 0.0;
    let mut lay = 0.0;
    let mut draw = 0.0;
    let mut build_ms = 0.0;
    for f in 0..200 {
        let t0 = Instant::now();
        s.v.set(f);
        Recomposer::recompose_pending();
        let t1 = Instant::now();
        let _ = perform_layout(s.root, vp());
        let t2 = Instant::now();
        let (c, _o) = draw_tree_tagged(s.root);
        let t3 = Instant::now();
        let dl = build_display_list(&c, Color::WHITE, &mut tc);
        let t4 = Instant::now();
        if f > 20 {
            adv += t1.duration_since(t0).as_secs_f32() * 1e3;
            lay += t2.duration_since(t1).as_secs_f32() * 1e3;
            draw += t3.duration_since(t2).as_secs_f32() * 1e3;
            build_ms += t4.duration_since(t3).as_secs_f32() * 1e3;
        }
        let _ = dl;
    }
    let n = 180.0f32;
    println!(
        "[recompose] | ms/frame adv={:.2} lay={:.2} draw={:.2} build={:.2} total={:.2}",
        adv / n,
        lay / n,
        draw / n,
        build_ms / n,
        (adv + lay + draw + build_ms) / n
    );
}
