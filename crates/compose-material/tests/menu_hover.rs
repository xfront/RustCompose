//! 回归用例：浮层菜单项的悬停状态层。
//!
//! 原缺陷：菜单项是 Leaf 节点，其绘制闭包在 `perform_layout` 的 `RUNTIME.borrow_mut()` 内
//! 被 `leaf_intrinsic` 执行；闭包里读 `MutableState::get()` 构成嵌套可变借 → panic
//! `RefCell already borrowed`。现依赖登记上移到组合期，绘制只读快照布尔值。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, Policy, add_node, create_root_node, dispatch_hover_at, draw_tree,
    perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, with_rt_pub};
use compose_material::widgets::input::{menu_panel, set_menu_callback};

const KEY: u64 = 0x4D45_4E55;

fn setup() -> compose::layout::NodeId {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let anchor = add_node(
                Policy::Leaf,
                Modifier::new().fill_max_width().height(40.0),
            );
            menu_panel(KEY, anchor, &["Alpha".to_string(), "Beta".to_string()]);
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));
    Recomposer::recompose_pending();
    root
}

/// 半透明填充 = 状态层（悬停高亮）；实心填充不算。
fn state_layers(root: compose::layout::NodeId) -> usize {
    draw_tree(root)
        .into_iter()
        .filter(|c| match c {
            DrawCmd::FillRect { color, .. } => color.a < 0.5,
            _ => false,
        })
        .count()
}

fn hoverable_nodes() -> Vec<compose::layout::NodeId> {
    with_rt_pub(|rt| {
        rt.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.hover_handler.is_some())
            .map(|(i, _)| compose::layout::NodeId(i as u32))
            .collect()
    })
}

#[test]
fn menu_layout_pass_does_not_reenter_runtime_borrow() {
    let root = setup();
    // 反复布局：Leaf 探测闭包每次都会执行，读 MutableState 即 panic。
    for _ in 0..3 {
        perform_layout(root, Constraints::tight(400.0, 300.0));
        Recomposer::recompose_pending();
    }
    assert_eq!(state_layers(root), 0, "未悬停时不应有状态层");
}

#[test]
fn menu_item_hover_paints_state_layer() {
    set_menu_callback(KEY, |_| {});
    let root = setup();
    let targets = hoverable_nodes();
    assert!(
        !targets.is_empty(),
        "菜单项应挂悬停处理器（共 2 项）"
    );
    let item = targets[0];
    assert!(
        dispatch_hover_at(item),
        "首次悬停应被消费"
    );
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    assert!(
        state_layers(root) > 0,
        "悬停项应画出 8% 状态层"
    );

    // 移开：状态层必须消失（绘制闭包在重组时重新取快照）。
    // 重组会重建节点并清掉 `hovered_node`，故先按当前节点重新登记悬停目标。
    dispatch_hover_at(hoverable_nodes()[0]);
    assert!(compose::layout::dispatch_hover_left());
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    assert_eq!(state_layers(root), 0, "离开后状态层应撤除");
}
