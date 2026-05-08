//! 回归：鼠标移到 Button 上立即点击，不应因为 hover 触发的重组而丢失点击。
//!
//! `kernel::pointer_down` 顺序：hit_test → dispatch_hover_at → dispatch_press_down。
//! hover 令 Button 的 `hovered` 状态变化 → 调度重组；`sweep_nodes` 回收旧节点并
//! 把下标放入 `node_free`，重组后同一控件换了 arena 下标。抬起时若只按下标比对，
//! `same=false` 会吞掉这次点击 —— 表现为「要等 hover 过后停一会才能点」。

use std::cell::Cell;

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Modifier, NodeId, create_root_node, dispatch_hover_at, dispatch_press_down,
    dispatch_press_up, hit_test, perform_layout, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, advance_frame, with_rt_pub};
use compose_material::{Button, OverlayHost};
use compose_material::widgets::button_label;

thread_local! {
    static HITS: Cell<usize> = const { Cell::new(0) };
}

fn bump() {
    HITS.with(|h| h.set(h.get() + 1));
}

fn hits() -> usize {
    HITS.with(|h| h.get())
}

fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
}

/// 节点绝对矩形（累加父链局部坐标）。
fn abs_rect(id: NodeId) -> (f32, f32, f32, f32) {
    with_rt_pub(|rt| {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(n) = cur {
            chain.push(n);
            cur = rt.nodes[n.index()].parent;
        }
        chain.reverse();
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        for n in chain {
            let node = &rt.nodes[n.index()];
            x += node.x;
            y += node.y;
        }
        let n = &rt.nodes[id.index()];
        (x, y, n.width, n.height)
    })
}

fn boot() -> (NodeId, f32, f32, f32, f32) {
    HITS.with(|h| h.set(0));
    let (w, h) = (400.0, 300.0);
    set_viewport(w, h);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                Button(Modifier::new(), bump, || {
                    button_label("Click me");
                });
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 6);
    std::mem::forget(comp);
    let clickable = with_rt_pub(|rt| {
        rt.nodes
            .iter()
            .position(|n| n.alive && n.on_click.is_some())
            .expect("应有 Button 的可点击节点") as u32
    });
    let (x, y, bw, bh) = abs_rect(NodeId(clickable));
    (root, w, h, x + bw / 2.0, y + bh / 2.0)
}

/// 模拟 kernel：hover → press_down → 帧推进（重组）→ press_up。
#[test]
fn click_right_after_hover_is_not_swallowed() {
    let (root, w, h, x, y) = boot();
    let (t0, _, _) = hit_test(root, x, y).expect("按钮中心应命中");
    dispatch_hover_at(t0); // hover 进入 → Button 的 hovered 变化 → 调度重组
    assert!(dispatch_press_down(t0, x, y));
    settle(root, w, h, 3); // 重组换槽
    let up = hit_test(root, x, y);
    assert!(dispatch_press_up(up, x, y), "hover 后立即点击应结算 on_click");
    assert_eq!(hits(), 1, "点击次数");
}

/// 对照：hover 稳定后再点（用户「停一会」才能点的那种情形）本来就应成功。
#[test]
fn click_after_hover_settles_works() {
    let (root, w, h, x, y) = boot();
    let (t0, _, _) = hit_test(root, x, y).expect("按钮中心应命中");
    dispatch_hover_at(t0);
    settle(root, w, h, 3);
    let (t1, _, _) = hit_test(root, x, y).expect("重组后仍应命中");
    assert!(dispatch_press_down(t1, x, y));
    settle(root, w, h, 3);
    let up = hit_test(root, x, y);
    assert!(dispatch_press_up(up, x, y));
    assert_eq!(hits(), 1);
}
