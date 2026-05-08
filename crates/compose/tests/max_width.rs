//! `Modifier::max_width` 回归 —— Compose `widthIn(max = …)` 的宽松上界语义。
//!
//! 聊天气泡改为「容器 + Text 叶子」后依赖它：超过上限折行，不足时保持自然宽度
//! （不能被撑满，否则短消息气泡会横贯整行）。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::{Color, Constraints};
use compose::layout::{
    add_node, create_root_node, perform_layout, Alignment, DrawBuilder, Modifier, NodeId, Policy,
    TextStyle,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{with_rt_pub, Composition};

fn style() -> TextStyle {
    TextStyle::new().size(14.0)
}

fn build(text: String, max_w: f32) -> (NodeId, NodeId) {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let leaf = Rc::new(Cell::new(None::<NodeId>));
    let leaf_c = leaf.clone();
    comp.set_content(move || {
        let leaf_c = leaf_c.clone();
        let text = text.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let text = text.clone();
            let id = add_node(
                Policy::Leaf,
                Modifier::new().max_width(max_w).background(Color::WHITE, 0.0),
            );
            with_rt_pub(|rt| {
                rt.nodes[id.index()].draw =
                    Some(Box::new(move |b: &mut DrawBuilder, x, y, w: f32, _h: f32| {
                        b.draw_text(x, y, &text, &style(), w);
                    }));
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
fn max_width_caps_and_wraps_long_text() {
    let (root, leaf) = build("x".repeat(100), 200.0);
    perform_layout(root, Constraints::new(0.0, 1000.0, 0.0, 2000.0));
    let (w, h) = size_of(leaf);
    let single = style().line_height.unwrap_or(14.0 * 1.4);
    assert!(w <= 200.0 + f32::EPSILON, "宽度必须收在上限内：{w}");
    assert!(h > 2.0 * single, "超宽文本应折行，实际高 {h}");
}

#[test]
fn max_width_keeps_short_text_compact() {
    let (root, leaf) = build("hi".to_string(), 200.0);
    perform_layout(root, Constraints::new(0.0, 1000.0, 0.0, 2000.0));
    let (w, h) = size_of(leaf);
    let single = style().line_height.unwrap_or(14.0 * 1.4);
    assert!(w < 100.0, "短文本不应被撑到上限：{w}");
    assert!((h - single).abs() < 1.0, "短文本保持单行：{h} vs {single}");
}
