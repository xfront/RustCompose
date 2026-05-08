//! 保留模式分块回归（`draw_tree_retained`）。
//!
//! 分块键是**内容**而不是 arena 下标，且块序即 z 序：
//! ① 定向重组会让同一子树换到新的节点下标，按 id 比对会把内容根本没变的整棵子树误判成
//!    动态（story 动画页实测动态集 244/893，valo 每帧重编码全花在这里）；
//! ② 静态段只留句柄，命令仍躺在节点图层缓存里，平台侧按块键复用 DisplayList；
//! ③ 周期动画（光标闪烁、无限 tween）每帧内容都变，不能被误缓存成静态；
//! ④ 拼回的一帧必须与平铺遍历逐命令一致 —— 旧的两层贴图方案把动态段整段画在静态层之上，
//!    树中部的动画会把后续静态内容盖掉（实测：表格/侧栏文字消失）。

use compose::foundation::{Color, Constraints};
use compose::layout::{
    add_node, create_root_node, draw_tree, draw_tree_retained, draw_tree_retained_into,
    perform_layout, Alignment, Chunk, DrawBuilder, DrawCmd, FrameDraw, Modifier, NodeId, Policy,
    RetainedState,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{mutableStateOf, with_rt_pub, Composition, Recomposer};

const VP: f32 = 400.0;

fn vp() -> Constraints {
    Constraints::new(0.0, VP, 0.0, VP)
}

/// 两个纵向堆叠的 Leaf：`a` 恒白，`b` 的取色由状态 `v` 决定。
/// 另有一个只被组合期读取、不参与取色的状态 `u`，用来制造「id 换代但输出不变」。
struct Scene {
    root: NodeId,
    state: RetainedState,
    v: compose::runtime::MutableState<i32>,
    u: compose::runtime::MutableState<i32>,
}

fn build() -> Scene {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let v = mutableStateOf(0i32);
    let u = mutableStateOf(0i32);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let n = v.get();
            let _ = u.get(); // 依赖它：set(u) 会重组本作用域并重建两个孩子
            let id_a = add_node(Policy::Leaf, Modifier::new().size(100.0, 40.0));
            with_rt_pub(|rt| {
                rt.nodes[id_a.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                    b.fill_rect(x, y, w, h, Color::WHITE);
                }));
            });
            let id_b = add_node(Policy::Leaf, Modifier::new().size(100.0, 40.0));
            with_rt_pub(|rt| {
                rt.nodes[id_b.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                    let color = if n == 0 { Color::BLACK } else { Color::rgb(0.0, 0.0, 1.0) };
                    b.fill_rect(x, y, w, h, color);
                }));
            });
        });
    });
    Scene { root, state: RetainedState::default(), v, u }
}

/// 一帧：布局 + 保留遍历。
fn frame(s: &mut Scene, force: bool) -> FrameDraw {
    perform_layout(s.root, vp());
    draw_tree_retained(s.root, &mut s.state, force)
}

/// 命令流里 `FillRect` 的 (y, 颜色)：用于数段、辨内容。
fn rects(cmds: &[DrawCmd]) -> Vec<(f32, Color)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::FillRect { y, color, .. } => Some((*y, *color)),
            _ => None,
        })
        .collect()
}

/// 块计划 → `(是否静态, 该块命令本体)`：与内核拼帧读到的完全一样。
fn chunks(f: &FrameDraw) -> Vec<(bool, Vec<DrawCmd>)> {
    f.plan
        .iter()
        .map(|c| match c {
            Chunk::Static { from, to, .. } => (true, f.static_chunk_cmds(*from, *to)),
            Chunk::Live { start, len } => (false, f.dynamic_cmds[*start..*start + *len].to_vec()),
        })
        .collect()
}

/// 拼回整帧（块序串接）——z 序正确性的判据。
fn stitched(f: &FrameDraw) -> Vec<DrawCmd> {
    chunks(f).into_iter().flat_map(|(_, c)| c).collect()
}

#[test]
fn stable_scene_records_static_chunks_without_copying_them() {
    let mut s = build();
    // 首帧：无历史 → 全部物化成现场块（静态集合仍为空 ⇒ 键未变）。
    let f = frame(&mut s, false);
    assert_eq!(rects(&f.dynamic_cmds).len(), 2, "首帧两段都应物化为现场命令");
    assert!(!f.static_changed);

    // 第二帧：内容逐段相同 → 全静态，静态块首次出现 ⇒ 需要录一次。
    let f = frame(&mut s, false);
    assert!(f.static_changed);
    assert!(f.dynamic_cmds.is_empty(), "内容未变不应留下现场命令");
    assert_eq!(chunks(&f).len(), 1, "相邻静态段应并成一块");
    assert_eq!(rects(&chunks(&f)[0].1).len(), 2);
    assert_eq!(f.cmd_count, 2);

    // 第三帧：仍未变 → 同键复用，现场命令一条都不复制（动画期省下的正是这段）。
    let f = frame(&mut s, false);
    assert!(!f.static_changed);
    assert!(f.dynamic_cmds.is_empty());
    assert_eq!(rects(&chunks(&f)[0].1).len(), 2, "静态块本体仍可从图层缓存取回");

    // 第四帧：第二个叶子变化 → 只有它进现场块，静态块收缩并重算键。
    s.v.set(1);
    Recomposer::recompose_pending();
    let f = frame(&mut s, false);
    assert!(f.static_changed);
    let c = chunks(&f);
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].0, true, "未变的第一个叶子在前");
    assert_eq!(c[1].0, false, "变化的叶子在后（z 序不变）");
    assert_eq!(rects(&c[0].1).len(), 1);
    assert_eq!(rects(&c[1].1).len(), 1);
}

#[test]
fn id_churn_does_not_invalidate_static_chunks() {
    let mut s = build();
    frame(&mut s, false);
    frame(&mut s, false);
    assert!(!frame(&mut s, false).static_changed, "基线：稳定画面复用静态块");

    // 定向重组换掉节点下标，但产出的命令逐字节相同 → 静态块不该重录。
    s.u.set(1);
    Recomposer::recompose_pending();
    let f = frame(&mut s, false);
    assert!(!f.static_changed, "内容逐命令相同 ⇒ id 换代不影响静态块");
    assert!(f.dynamic_cmds.is_empty(), "id 换代不应把内容推到现场块");
}

#[test]
fn oscillating_content_stays_dynamic() {
    let mut s = build();
    frame(&mut s, false);
    frame(&mut s, false);

    // 周期动画：输出在两种颜色间振荡。判据只看「上一帧」，故振荡内容永远留在现场块。
    for i in 0..4 {
        s.v.set(if i % 2 == 0 { 1 } else { 0 });
        Recomposer::recompose_pending();
        let f = frame(&mut s, false);
        assert_eq!(
            rects(&f.dynamic_cmds).len(),
            1,
            "第 {i} 帧的振荡内容必须留在现场块"
        );
    }
}

#[test]
fn dynamic_churn_does_not_touch_static_chunks() {
    let mut s = build();
    frame(&mut s, false);
    frame(&mut s, false);
    assert!(!frame(&mut s, false).static_changed);

    // 只有第二段在变：静态块序列（只剩第一段）第一次收缩需重算，之后再变就不用了。
    // 取值交替 1/0，保证每一步的输出都确实不同于上一帧（否则整树都该判静态）。
    for i in 0..2 {
        s.v.set(1 - i);
        Recomposer::recompose_pending();
        let f = frame(&mut s, false);
        assert_eq!(rects(&f.dynamic_cmds).len(), 1);
        let static_rects: Vec<_> = chunks(&f)
            .into_iter()
            .filter(|(s, _)| *s)
            .flat_map(|(_, c)| rects(&c))
            .collect();
        assert_eq!(static_rects.len(), 1, "静态块只剩第一个叶子");
        if i == 0 {
            assert!(f.static_changed, "静态块集合收缩 → 需重录");
        } else {
            assert!(!f.static_changed, "只有现场块变化时不应重录静态块");
        }
    }
}

#[test]
fn force_static_rerecords_even_when_content_is_stable() {
    let mut s = build();
    frame(&mut s, false);
    frame(&mut s, false);
    assert!(!frame(&mut s, false).static_changed);

    // 主题底色不在命令流里，靠 `force_static` 兜住，否则静态块会留着旧底色。
    let f = frame(&mut s, true);
    assert!(f.static_changed);
    assert_eq!(rects(&chunks(&f)[0].1).len(), 2);
}

/// 裁剪容器（前缀 `ClipPush`、后缀 `ClipPop` + 描边）套一个恒定的孩子。
fn build_clipped() -> Scene {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let v = mutableStateOf(0i32);
    let u = mutableStateOf(0i32);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let n = v.get();
            let _ = u.get();
            compose::layout::Box(
                Modifier::new()
                    .size(120.0, 80.0)
                    .background(Color::WHITE, 0.0)
                    .clip(6.0)
                    .border(2.0, if n == 0 { Color::BLACK } else { Color::rgb(0.0, 0.0, 1.0) }, 6.0),
                Alignment::TopStart,
                move || {
                    let id = add_node(Policy::Leaf, Modifier::new().size(40.0, 20.0));
                    with_rt_pub(|rt| {
                        rt.nodes[id.index()].draw =
                            Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                                b.fill_rect(x, y, w, h, Color::rgb(1.0, 0.0, 0.0));
                            }));
                    });
                },
            );
        });
    });
    Scene { root, state: RetainedState::default(), v, u }
}

/// 回归：裁剪指令永远落在现场块里，静态块只装墨迹 —— 于是裁剪区里的静态孩子能独立成块。
///
/// 旧规则要求「块边界只能落在裁剪栈清空处」，一个铺满内容的裁剪容器就把整棵子树并成
/// 一个巨型现场块（story 动画页实测 890/893 条每帧重录，静态复用等于没有）。
/// 现在 `ClipPush`/`ClipPop` 各自成原子段且永远现场录制，外层构建器的裁剪栈与平铺流
/// 逐步同步，静态块嵌回原位置即继承这层裁剪。
#[test]
fn clip_ops_stay_live_and_unlock_static_runs_inside_clip() {
    let mut s = build_clipped();
    frame(&mut s, false);
    let f = frame(&mut s, false);
    for (is_static, cmds) in chunks(&f) {
        if is_static {
            assert!(
                !cmds.iter().any(|c| matches!(c, DrawCmd::ClipPush { .. } | DrawCmd::ClipPop)),
                "静态块里混进了裁剪指令 → 嵌回后外层栈与块内栈会错位"
            );
        }
    }
    assert_eq!(stitched(&f), draw_tree(s.root), "拼回的一帧须与平铺流逐命令一致");

    // 只改容器描边色：变化全在后缀的墨迹 run，被裁的孩子必须留在自己的静态块里。
    s.v.set(1);
    Recomposer::recompose_pending();
    let f = frame(&mut s, false);
    let c = chunks(&f);
    let kid = c
        .iter()
        .find(|(_, cmds)| rects(cmds).iter().any(|&(_, col)| col == Color::rgb(1.0, 0.0, 0.0)))
        .expect("孩子红块要在计划里");
    assert!(kid.0, "容器变了不该把孩子拖出现静块");
    assert_eq!(stitched(&f), draw_tree(s.root));
}

/// 复用入口（内核跨帧持有同一个 `FrameDraw`）必须与「每帧新建」逐命令等价：
/// 清空只回收内容、保留容量，不得残留上一帧的计划或现场命令。
#[test]
fn reused_frame_draw_equals_flat_stream_across_steps() {
    let mut s = build();
    let mut fd = FrameDraw::default();
    for step in 0..6 {
        if step == 2 {
            s.v.set(1);
            Recomposer::recompose_pending();
        }
        if step == 4 {
            s.u.set(5); // 定向重组：节点下标换代，输出不变
            Recomposer::recompose_pending();
        }
        perform_layout(s.root, vp());
        draw_tree_retained_into(s.root, &mut s.state, &mut fd, false);
        assert_eq!(
            stitched(&fd),
            draw_tree(s.root),
            "第 {step} 步：复用缓冲拼回的一帧须与平铺遍历逐命令一致"
        );
        assert!(fd.cmd_count >= 2, "第 {step} 步：计划不应为空");
    }
    // 内容稳定后再走一帧：现场命令必须为空（复用缓冲不能把上一帧的现场命令带过来）。
    perform_layout(s.root, vp());
    draw_tree_retained_into(s.root, &mut s.state, &mut fd, false);
    assert!(fd.dynamic_cmds.is_empty(), "稳定画面不应残留现场命令");
    assert_eq!(stitched(&fd), draw_tree(s.root));
}

/// `FrameDraw::clear` 只清内容：计划 / 现场命令 / 计数归零。
#[test]
fn frame_draw_clear_resets_previous_plan() {
    let mut s = build();
    let mut fd = frame(&mut s, false);
    assert!(fd.cmd_count > 0 && !fd.plan.is_empty());
    fd.clear();
    assert_eq!(fd.cmd_count, 0);
    assert!(fd.plan.is_empty() && fd.dynamic_cmds.is_empty());
}

/// 核心不变式：把块按顺序拼回去，必须与平铺遍历逐命令相同。
///
/// 这条断言直接封死「动态层盖在后续静态内容之上」那一类分层错误（旧的两层贴图方案
/// 就是在这里丢掉了 story 首页的表格文字）。
#[test]
fn chunked_reassembly_equals_flat_stream() {
    for mut s in [build(), build_clipped()] {
        for step in 0..4 {
            if step == 2 {
                s.v.set(1);
                Recomposer::recompose_pending();
            }
            if step == 3 {
                s.u.set(7); // 定向重组：换节点下标，内容不变
                Recomposer::recompose_pending();
            }
            perform_layout(s.root, vp());
            let f = draw_tree_retained(s.root, &mut s.state, false);
            let flat = draw_tree(s.root);
            assert_eq!(
                stitched(&f),
                flat,
                "第 {step} 步：分块拼回应与平铺遍历逐命令一致"
            );
        }
    }
}
