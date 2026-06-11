//! 多窗口（多组合根共享同一 arena）的核心不变量。
//!
//! 多窗口 = 每个窗口一个 `Kernel`（自己的根 / 组合 / 视口 / 保留缓存），共享线程局部
//! 运行时。本测试在无 GPU 条件下验证与窗口无关的引擎侧契约：
//!
//! ① 各根组合期读到自己窗口的视口（浮层居中依赖 `viewport_size()`）；
//! ② 各根的布局互不干扰（同一 arena 里两个根按各自约束测量）；
//! ③ 绘制脏区按根隔离（A 窗口 resize/失效不能把 B 窗口拖进全量重绘）；
//! ④ 关窗（`Kernel::dispose`）只回收本根作用域与节点，B 窗口继续可用。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::{Color, Constraints};
use compose::layout::{
    add_node, draw_tree_tagged, perform_layout, scene_is_clean, set_current_root, viewport_size,
    Modifier, NodeId, Policy,
};
use compose::material::ColorScheme;
use compose::render::Kernel;
use compose::runtime::{mutableStateOf, with_rt_pub, Recomposer};

struct Win {
    kernel: Kernel,
    vp_seen: Rc<Cell<(f32, f32)>>,
    leaf: Rc<Cell<Option<NodeId>>>,
}

/// 建一个「窗口」：组合根 + 一个固定尺寸的叶子节点，并记下组合期读到的视口。
fn window(w: f32, h: f32, leaf_size: (f32, f32)) -> Win {
    let vp_seen = Rc::new(Cell::new((0.0, 0.0)));
    let leaf = Rc::new(Cell::new(None::<NodeId>));
    let (vp2, leaf2) = (vp_seen.clone(), leaf.clone());
    let kernel = Kernel::new_sized(ColorScheme::light(), w, h, move || {
        // 组合期读视口：多窗口下必须是自己窗口的尺寸（不同窗口不同）。
        vp2.set(viewport_size());
        let id = add_node(
            Policy::Leaf,
            Modifier::new().size(leaf_size.0, leaf_size.1).background(Color::WHITE, 0.0),
        );
        leaf2.set(Some(id));
    });
    Win { kernel, vp_seen, leaf }
}

/// 在「本窗口」的根上下文里布局（生产路径由 `Kernel::draw_frame` 的 `RootCtx` 完成）。
fn layout(win: &Win, w: f32, h: f32) {
    set_current_root(Some(win.kernel.root));
    perform_layout(win.kernel.root, Constraints::new(0.0, w, 0.0, h));
    set_current_root(None);
}

/// 驱动到 settled：两轮布局 + 绘制（首帧几何全变会重新标脏，第二轮才清）。
fn settle(win: &Win) {
    let (w, h) = win.vp_seen.get();
    layout(win, w, h);
    let _ = draw_tree_tagged(win.kernel.root);
    let _ = draw_tree_tagged(win.kernel.root);
    layout(win, w, h);
    let _ = draw_tree_tagged(win.kernel.root);
}

#[test]
fn each_root_sees_its_own_viewport() {
    let a = window(320.0, 240.0, (100.0, 50.0));
    let b = window(1024.0, 768.0, (100.0, 50.0));
    assert_eq!(a.vp_seen.get(), (320.0, 240.0), "A 窗口应读到自己注入的视口");
    assert_eq!(b.vp_seen.get(), (1024.0, 768.0), "B 窗口应读到自己注入的视口");
    assert_ne!(a.kernel.root, b.kernel.root, "每个窗口应有独立的组合根");
}

#[test]
fn layout_of_one_root_does_not_touch_the_other() {
    let a = window(320.0, 240.0, (100.0, 50.0));
    let b = window(640.0, 480.0, (100.0, 50.0));
    settle(&a);
    settle(&b);
    assert!(scene_is_clean(a.kernel.root), "A 应已 settled");
    assert!(scene_is_clean(b.kernel.root), "B 应已 settled");

    // 只给 A 换约束（模拟 A 窗口 resize）：A 变脏，B 必须仍然干净。
    layout(&a, 500.0, 400.0);
    assert!(!scene_is_clean(a.kernel.root), "A 换约束后应判脏");
    assert!(scene_is_clean(b.kernel.root), "A 的 resize 不应污染 B 的脏区");

    // B 的绘制仍应回放缓存（命令流非空，且无需重新遍历整树即可稳定）。
    let (cmds, _) = draw_tree_tagged(b.kernel.root);
    assert!(!cmds.is_empty(), "B 窗口应仍有内容");
}

#[test]
fn direct_invalidation_is_root_scoped() {
    let a = window(320.0, 240.0, (100.0, 50.0));
    let b = window(640.0, 480.0, (100.0, 50.0));
    settle(&a);
    settle(&b);
    let leaf_a = a.leaf.get().expect("A 叶子已组合");

    // 时间相关绘制（光标闪烁等）只失效 A 的节点：经父链定位所属根。
    compose::layout::invalidate_draw_node(leaf_a);
    assert!(!scene_is_clean(a.kernel.root), "被失效节点所属根应判脏");
    assert!(scene_is_clean(b.kernel.root), "另一窗口不受影响");
}

#[test]
fn pending_recomposition_is_root_scoped() {
    // 两个窗口各读一个独立状态：只失效受影响的那个窗口（宿主据此只重绘它）。
    let sa = mutableStateOf(0i32);
    let sb = mutableStateOf(0i32);
    let ka = Kernel::new_sized(ColorScheme::light(), 320.0, 240.0, move || {
        let _ = sa.get();
        add_node(Policy::Leaf, Modifier::new().size(10.0, 10.0));
    });
    let kb = Kernel::new_sized(ColorScheme::light(), 320.0, 240.0, move || {
        let _ = sb.get();
        add_node(Policy::Leaf, Modifier::new().size(10.0, 10.0));
    });
    assert!(!Recomposer::has_pending(), "首次组合后不应有挂起重重组");

    sa.set(1);
    assert!(Recomposer::pending_for_root(ka.root), "A 状态变更应归到 A 根");
    assert!(!Recomposer::pending_for_root(kb.root), "B 窗口不应被视为待重组");

    Recomposer::recompose_pending();
    assert!(!Recomposer::has_pending(), "重组后应排空");
}

#[test]
fn disposing_one_window_keeps_the_other_alive() {
    let mut a = window(320.0, 240.0, (100.0, 50.0));
    let b = window(640.0, 480.0, (100.0, 50.0));
    settle(&a);
    settle(&b);
    let (root_a, leaf_a) = (a.kernel.root, a.leaf.get().expect("A 叶子已组合"));
    let leaf_b = b.leaf.get().expect("B 叶子已组合");

    // 关窗：回收 A 的根作用域 / 节点 / 视口 / 布局约束。
    a.kernel.dispose();
    assert!(
        !with_rt_pub(|rt| rt.nodes[root_a.index()].alive),
        "关窗后 A 的根节点应被释放"
    );
    assert!(
        !with_rt_pub(|rt| rt.nodes[leaf_a.index()].alive),
        "关窗后 A 的子树应被回收"
    );
    assert!(
        with_rt_pub(|rt| rt.nodes[leaf_b.index()].alive),
        "B 窗口的节点不应被误回收"
    );

    // B 继续可用：重新布局 + 绘制仍有输出，且能再次 settled。
    settle(&b);
    assert!(scene_is_clean(b.kernel.root), "关掉 A 后 B 仍应能稳定");
    assert!(!draw_tree_tagged(b.kernel.root).0.is_empty());
}
