//! 文本折行测量回归 —— 锁死「Leaf 按父级可用宽度上报多行高度」。
//!
//! 之前 `leaf_intrinsic` 探测时闭包拿到的宽度恒为 0，`draw_text` 只能按单行量，
//! 于是卡片/列表里的正文永远是一行高、横向溢出被裁。现在测量阶段把约束宽度喂进
//! `DrawBuilder::probe_max_w`，绘制阶段 `Text` 又把节点宽度喂进 `max_w`，两侧口径一致。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::{Color, Constraints};
use compose::layout::{
    add_node, create_root_node, perform_layout, Alignment, DrawBuilder, Modifier, NodeId, Policy,
    TextStyle,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{with_rt_pub, Composition};

/// 100 个字符，启发式度量下单行宽 = 100 × (14×0.55) = 770px。
fn long_text() -> String {
    "x".repeat(100)
}

fn style() -> TextStyle {
    TextStyle::new().size(14.0)
}

fn build(max_w: f32) -> (NodeId, NodeId) {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let leaf = Rc::new(Cell::new(None::<NodeId>));
    let leaf_c = leaf.clone();
    comp.set_content(move || {
        let leaf_c = leaf_c.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let id = add_node(
                Policy::Leaf,
                Modifier::new().width(max_w).background(Color::WHITE, 0.0),
            );
            with_rt_pub(|rt| {
                rt.nodes[id.index()].draw = Some(Box::new(
                    move |b: &mut DrawBuilder, x, y, w: f32, _h: f32| {
                        b.draw_text(x, y, &long_text(), &style(), w);
                    },
                ));
            });
            leaf_c.set(Some(id));
        });
    });
    (root, leaf.get().unwrap())
}

fn size_of(id: NodeId) -> (f32, f32) {
    with_rt_pub(|rt| {
        let n = &rt.nodes[id.index()];
        (n.width, n.height)
    })
}

#[test]
fn text_wraps_to_multi_line_height_within_available_width() {
    let (root, leaf) = build(200.0);
    perform_layout(root, Constraints::new(0.0, 1000.0, 0.0, 2000.0));
    let (w, h) = size_of(leaf);
    let single = style().line_height.unwrap_or(14.0 * 1.4);
    assert!(w <= 200.0 + f32::EPSILON, "折行后宽度不得超出可用宽度：{w}");
    assert!(h > 2.0 * single, "应报多行高度，实际 {h}（单行 {single}）");
}

#[test]
fn wide_enough_box_stays_single_line() {
    // 100 字符启发式单行宽 770 < 1000 ⇒ 不折行
    let (root, leaf) = build(1000.0);
    perform_layout(root, Constraints::new(0.0, 1000.0, 0.0, 2000.0));
    let (w, h) = size_of(leaf);
    let single = style().line_height.unwrap_or(14.0 * 1.4);
    assert!((h - single).abs() < 1.0, "单行高度不应被撑高：{h} vs {single}");
    assert!(w > 500.0, "单行宽度按字符数估算：{w}");
}
