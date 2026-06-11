//! GraphicsLayer 回归：Modifier.graphicsLayer 生成配对图层命令，参与缓存与静态块隔离。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, Policy, add_node, create_root_node, draw_tree, perform_layout,
    with_content_scope,
};
use compose::runtime::Composition;

fn layer_cmds(root: compose::layout::NodeId) -> Vec<DrawCmd> {
    draw_tree(root)
        .into_iter()
        .filter(|c| matches!(c, DrawCmd::SaveLayer { .. } | DrawCmd::Transform { .. } | DrawCmd::Restore))
        .collect()
}

#[test]
fn alpha_generates_save_layer_and_restore() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        let id = add_node(Policy::Leaf, Modifier::new().size(50.0, 50.0).alpha(0.5));
        with_content_scope(id, || {});
    });
    perform_layout(root, Constraints::tight(100.0, 100.0));
    let cmds = layer_cmds(root);
    assert!(cmds.iter().any(|c| matches!(c, DrawCmd::SaveLayer { alpha, .. } if (*alpha - 0.5).abs() < 1e-5)), "应生成 alpha=0.5 的 SaveLayer: {cmds:?}");
    assert_eq!(cmds.iter().filter(|c| matches!(c, DrawCmd::Restore)).count(), 1, "Restore 应配平");
}

#[test]
fn transform_generates_transform_command() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        let id = add_node(
            Policy::Leaf,
            Modifier::new()
                .size(50.0, 50.0)
                .graphics_layer(1.0, 2.0, 2.0, 90.0, 5.0, 6.0, 0.5, 0.5),
        );
        with_content_scope(id, || {});
    });
    perform_layout(root, Constraints::tight(100.0, 100.0));
    let cmds = layer_cmds(root);
    assert!(cmds.iter().any(|c| matches!(c, DrawCmd::Transform { scale_x, rotation, .. } if *scale_x == 2.0 && *rotation == 90.0)), "应生成变换命令: {cmds:?}");
    // alpha=1 且不透明 → 无 SaveLayer，但 Transform 也需 Restore 配平。
    assert_eq!(cmds.iter().filter(|c| matches!(c, DrawCmd::Restore)).count(), 1);
}

#[test]
fn plain_modifier_has_no_layer_commands() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        let id = add_node(Policy::Leaf, Modifier::new().size(50.0, 50.0).alpha(1.0));
        with_content_scope(id, || {});
    });
    perform_layout(root, Constraints::tight(100.0, 100.0));
    assert!(layer_cmds(root).is_empty(), "alpha=1 无变换不应产生图层命令");
}
