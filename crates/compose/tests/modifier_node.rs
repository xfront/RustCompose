//! Modifier.Node 集成回归：自定义节点经 Modifier 链进入真实布局测量与绘制。

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, DrawCmd, Modifier, Policy, add_node, create_root_node, draw_tree, perform_layout,
    with_content_scope,
};
use compose::modifier_node::delta_modifier;
use compose::runtime::{Composition, with_rt_pub};

fn node_size(id: compose::layout::NodeId) -> (f32, f32) {
    with_rt_pub(|rt| {
        let n = &rt.nodes[id.index()];
        (n.width, n.height)
    })
}

#[test]
fn custom_measure_node_shrinks_layout_size() {
    let root = create_root_node(Alignment::TopStart);
    let holder = std::rc::Rc::new(std::cell::Cell::new(None));
    let holder2 = holder.clone();
    let comp = Composition::new(root);
    comp.set_content(move || {
        let m = delta_modifier(Modifier::new(), 20.0, 10.0, 20.0, 10.0);
        let id = add_node(Policy::Box(Alignment::TopStart), m);
        with_content_scope(id, || {});
        holder2.set(Some(id));
    });
    perform_layout(root, Constraints::tight(300.0, 200.0));
    let id = holder.get().expect("应组合出节点");
    let (w, h) = node_size(id);
    assert!(w <= 260.0 && h <= 180.0, "自定义 Modifier.Node 应收紧测量: {w}x{h}");
}

#[test]
fn then_and_then_if_compose_in_order() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let blue = Color::rgb(0.0, 0.0, 1.0);
        let red = Color::rgb(1.0, 0.0, 0.0);
        let m = Modifier::new()
            .width(100.0)
            .then(Modifier::new().height(50.0))
            .then_if(true, move || Modifier::new().background(blue, 0.0))
            .then_if(false, move || Modifier::new().background(red, 0.0));
        let id = add_node(Policy::Box(Alignment::TopStart), m);
        with_content_scope(id, || {});
    });
    perform_layout(root, Constraints::tight(300.0, 200.0));
    let cmds = draw_tree(root);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let red = Color::rgb(1.0, 0.0, 0.0);
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillRect { color, .. } if *color == blue)),
        "then_if(true) 的 background 应生效"
    );
    assert!(
        !cmds.iter().any(|c| matches!(c, DrawCmd::FillRect { color, .. } if *color == red)),
        "then_if(false) 的 background 不应生效"
    );
}
