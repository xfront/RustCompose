//! ExpandableFab / FabEx 无头回归测试：
//! 尺寸变体渲染、展开/收起切换、动作项随状态增减。

use compose::foundation::Constraints;
use compose::layout::{Alignment, create_root_node, perform_layout, draw_tree, Modifier};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, mutableStateOf};
use compose_material::{ExpandableFab, ExtendedFab, Fab, FabEx, FabExpandType, FabSize};
use compose::material::IconName;

#[test]
fn fab_size_variants_render() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            FabEx(IconName::Add, FabSize::Small, Modifier::new(), || {});
            Fab(IconName::Add, Modifier::new(), || {});
            FabEx(IconName::Add, FabSize::Large, Modifier::new(), || {});
            ExtendedFab(IconName::Add, "Extended", Modifier::new(), || {});
        });
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));
    let cmds = draw_tree(root);
    assert!(!cmds.is_empty(), "四种 FAB 变体应产生绘制命令");
}

#[test]
fn expandable_fab_toggle_reveals_items() {
    let exp = mutableStateOf(false);
    let exp2 = exp.clone();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let e = exp2.clone();
        MaterialTheme(ColorScheme::light(), move || {
            ExpandableFab(
                IconName::Add,
                &[
                    (IconName::Edit, "编辑".into()),
                    (IconName::Copy, "复制".into()),
                    (IconName::Share, "分享".into()),
                ],
                FabExpandType::Up,
                e.clone(),
                Modifier::new(),
            );
        });
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));
    assert!(!exp.get(), "初始应收起");
    let collapsed_cmds = draw_tree(root);

    exp.set(true);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::loose(800.0, 600.0));
    assert!(exp.get(), "展开后状态应为 true");
    let expanded_cmds = draw_tree(root);
    assert_ne!(
        collapsed_cmds, expanded_cmds,
        "展开后绘制命令应变化（图标 Add→Close + 条目滑入）"
    );

    exp.set(false);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::loose(800.0, 600.0));
    let recollapsed_cmds = draw_tree(root);
    assert_eq!(
        recollapsed_cmds, collapsed_cmds,
        "收起后绘制命令应与初始一致"
    );
}

#[test]
fn expandable_fab_side_and_fan_render() {
    for kind in [FabExpandType::Side, FabExpandType::Fan] {
        let exp = mutableStateOf(false);
        let exp2 = exp.clone();
        let root = create_root_node(Alignment::TopStart);
        let comp = Composition::new(root);
        comp.set_content(move || {
            let e = exp2.clone();
            let k = kind;
            MaterialTheme(ColorScheme::light(), move || {
                ExpandableFab(
                    IconName::MoreVert,
                    &[
                        (IconName::Edit, "编辑".into()),
                        (IconName::Copy, "复制".into()),
                    ],
                    k,
                    e.clone(),
                    Modifier::new(),
                );
            });
        });
        perform_layout(root, Constraints::loose(800.0, 600.0));
        let _ = draw_tree(root);
        exp.set(true);
        perform_layout(root, Constraints::loose(800.0, 600.0));
        let cmds = draw_tree(root);
        assert!(!cmds.is_empty(), "{kind:?} 展开后应产生绘制命令");
    }
}
