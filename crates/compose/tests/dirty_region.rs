//! 增量脏区绘制回归 —— 锁死「画面稳定即零成本」的门控语义：
//! ① 稳定后重复绘制不重跑绘制闭包，`scene_changed()` 为 false（平台据此跳过重建/上传）；
//! ② 稳定后 `perform_layout`（约束未变 + 根不脏）直接早返回，不再整树测量/落位；
//! ③ 内容变化（重组）必须把脏区信号传到根，否则平台快路径会把新内容冻在旧缓存里；
//! ④ 时间相关绘制（光标闪烁/进度动画）经 `invalidate_draw_node` 失效时，同样要传到根。
//!
//! 计数口径：Leaf 的绘制闭包既在测量期被 probe 执行一次，也在绘制期执行一次，
//! 故 `hits` 同时反映「布局是否递归」与「绘制是否重跑」。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::{Color, Constraints};
use compose::layout::{
    add_node, create_root_node, draw_tree_tagged, invalidate_draw_node, perform_layout,
    scene_changed, scene_is_clean, Alignment, DrawBuilder, Modifier, NodeId, Policy,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{mutableStateOf, with_rt_pub, Composition, Recomposer};

const VP_W: f32 = 400.0;
const VP_H: f32 = 300.0;

fn vp() -> Constraints {
    Constraints::new(0.0, VP_W, 0.0, VP_H)
}

/// 场景：一个 Leaf 节点，填充色由状态 `v` 决定（0 白 / 1 黑）。
struct Scene {
    root: NodeId,
    leaf: Rc<Cell<Option<NodeId>>>,
    hits: Rc<Cell<u32>>,
    v: compose::runtime::MutableState<i32>,
}

fn build() -> Scene {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let leaf = Rc::new(Cell::new(None::<NodeId>));
    let hits = Rc::new(Cell::new(0u32));
    let v = mutableStateOf(0i32);
    let (leaf_c, hits_c) = (leaf.clone(), hits.clone());
    comp.set_content(move || {
        // 嵌套 `move |c|` 不能把外层捕获直接 move 给内层 `Fn`（E0507）：先在外层体内 clone。
        let leaf_c = leaf_c.clone();
        let hits_c = hits_c.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let n = v.get(); // 组合期读取 → 建立依赖，set 时定向重组本作用域
            let id = add_node(Policy::Leaf, Modifier::new().size(200.0, 100.0));
            leaf_c.set(Some(id));
            let h = hits_c.clone();
            with_rt_pub(|rt| {
                rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, hh| {
                    h.set(h.get() + 1);
                    let color = if n == 0 { Color::WHITE } else { Color::BLACK };
                    b.fill_rect(x, y, w, hh, color);
                }));
            });
        });
    });
    Scene { root, leaf, hits, v }
}

/// 把画面驱动到 settled：两轮「布局 + 绘制」。
///
/// 需两轮是因为首帧 `last_geo` 为空，`place_node` 会把全树判为「几何变化」并重新标脏，
/// 第二轮才因几何相等而清脏；此后空闲帧的 `perform_layout` 才走根不脏的早返回。
fn settle(s: &Scene) {
    perform_layout(s.root, vp());
    let _ = draw_tree_tagged(s.root);
    // 首绘的输出对缓存而言是「新增」，会向上传播失效 → 再绘一轮排空绘制标记。
    let _ = draw_tree_tagged(s.root);
    perform_layout(s.root, vp());
    let _ = draw_tree_tagged(s.root);
}

#[test]
fn settled_scene_replays_cache_without_rerunning_draw() {
    let s = build();
    settle(&s);
    let after_settle = s.hits.get();
    assert!(after_settle >= 2, "至少测量 + 绘制各执行一次：{after_settle}");

    // 再绘一轮：子树未脏 → 回放缓存，闭包不重跑，且无内容变化信号。
    let (cmds, _owners) = draw_tree_tagged(s.root);
    assert_eq!(s.hits.get(), after_settle, "稳定画面不应重跑绘制闭包");
    assert!(!scene_changed(), "回放缓存 ⇒ 输出与上一帧逐命令相同");
    assert!(scene_is_clean(s.root), "稳定后根应无失效标记（脏区为空）");
    assert!(!cmds.is_empty(), "缓存回放仍须产出命令流");
}

#[test]
fn settled_layout_short_circuits_before_tree_walk() {
    let s = build();
    settle(&s);
    let before = s.hits.get();
    // 约束未变 + 根不脏 → 早返回：不递归 measure（Leaf probe 不再执行）、不 place。
    let size = perform_layout(s.root, vp());
    assert_eq!(s.hits.get(), before, "已稳定的布局不应再遍历整树");
    // 早返回必须回报上一轮完整布局的产物尺寸（否则调用方拿到零尺寸）。
    let (rw, rh) = with_rt_pub(|rt| {
        let n = &rt.nodes[s.root.index()];
        (n.width, n.height)
    });
    assert_eq!((size.width, size.height), (rw, rh), "早返回须回报既有尺寸");
    assert!(rw > 0.0 && rh > 0.0, "尺寸应已有效测量：{rw}x{rh}");
}

#[test]
fn content_change_propagates_dirty_to_root() {
    let s = build();
    settle(&s);
    assert!(scene_is_clean(s.root));

    // 状态变化 → 定向重组 → 子树 + 祖先链失效（含根）。
    s.v.set(1);
    Recomposer::recompose_pending();
    assert!(!scene_is_clean(s.root), "重组后脏区信号必须传到根，否则会被缓存吞掉");

    perform_layout(s.root, vp());
    let (cmds, _) = draw_tree_tagged(s.root);
    assert!(scene_changed(), "填充色变了 ⇒ 本轮存在真实脏区");
    assert!(
        cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::FillRect { color, .. }
            if *color == Color::BLACK)),
        "新内容应出现在命令流里：{cmds:?}"
    );
}

#[test]
fn direct_node_invalidation_reaches_root() {
    let s = build();
    settle(&s);
    let id = s.leaf.get().expect("leaf 已组合");
    let before = s.hits.get();

    // 时间相关绘制（光标闪烁等）不走重组，只失效自身 + 祖先链。
    invalidate_draw_node(id);
    assert!(!scene_is_clean(s.root), "单节点失效须同步失效祖先");

    let _ = draw_tree_tagged(s.root);
    assert_eq!(s.hits.get(), before + 1, "被失效的节点应重跑绘制闭包");
    // 输出与上一帧相同（颜色未变）→ 无需重建 DisplayList / 上传 GPU。
    assert!(!scene_changed(), "重绘但输出等价时不应触发重建/上传");
}
