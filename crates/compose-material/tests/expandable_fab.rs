//! ExpandableFab / FabEx 无头回归测试：
//! 尺寸变体渲染、展开/收起切换、动作项随状态增减。

use compose::foundation::Constraints;
use compose::layout::{Alignment, create_root_node, perform_layout, draw_tree, hit_test, dispatch_click_at, Modifier};
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

// ── rui FabConfig 对齐：disabled / position / children 动画 ──

fn config_fab(
    config: compose_material::FabConfig,
    state: compose::runtime::MutableState<bool>,
) -> compose::layout::NodeId {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content({
        let config = config;
        move || {
            let st = state;
            let value = config.clone();
            MaterialTheme(ColorScheme::light(), move || {
                compose_material::ExpandableFabEx(value.clone(), st, Modifier::new());
            });
        }
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));
    root
}

fn base_actions() -> Vec<compose_material::FabAction> {
    vec![
        compose_material::FabAction::icon(compose::material::IconName::Edit, "编辑"),
        compose_material::FabAction::icon(compose::material::IconName::Copy, "复制"),
        compose_material::FabAction::icon(compose::material::IconName::Share, "分享"),
    ]
}

/// rui `FabConfig::disabled`：禁用态点击不展开。
#[test]
fn fab_config_disabled_blocks_toggle() {
    use compose::layout::{dispatch_click_at, draw_tree, hit_test};
    let exp = mutableStateOf(false);
    let root = config_fab(
        compose_material::FabConfig {
            disabled: true,
            actions: base_actions(),
            ..compose_material::FabConfig::legacy(
                compose::material::IconName::Add,
                FabExpandType::Up,
                vec![],
            )
        },
        exp,
    );
    let cmds_before = draw_tree(root);
    // 扫描命中带 on_click 的 FAB 节点（位置由 Column/Fab 尺寸决定，不硬编码坐标）。
    let mut clicked = false;
    'outer: for y in (0..600).step_by(8) {
        for x in (0..800).step_by(8) {
            if let Some((t, _, _)) = hit_test(root, x as f32, y as f32) {
                let has_click = compose::runtime::with_rt_pub(|rt| {
                    rt.nodes[t.index()].on_click.is_some()
                        || rt.nodes[t.index()]
                            .parent
                            .is_some_and(|p| rt.nodes[p.index()].on_click.is_some())
                });
                if has_click {
                    let _ = dispatch_click_at(t, x as f32, y as f32);
                    clicked = true;
                    break 'outer;
                }
            }
        }
    }
    assert!(clicked, "应找到可点击的 FAB 节点");
    assert!(!exp.get(), "disabled FAB 点击不应展开");
    assert_eq!(draw_tree(root), cmds_before, "禁用态画面不应变化");
}

/// rui `FabPosition` 决定扇面朝向：Right / Left / Center 绘制几何应不同。
#[test]
fn fab_config_position_changes_fan_orientation() {
    use compose::layout::draw_tree;
    let mut cmds = Vec::new();
    for pos in [
        compose_material::FabPosition::Right,
        compose_material::FabPosition::Left,
        compose_material::FabPosition::Center,
    ] {
        let exp = mutableStateOf(true);
        let root = config_fab(
            compose_material::FabConfig {
                expand_type: FabExpandType::Fan,
                position: pos,
                children_anim: compose_material::FabChildrenAnim::Scale,
                actions: base_actions(),
                ..compose_material::FabConfig::legacy(
                    compose::material::IconName::Add,
                    FabExpandType::Fan,
                    vec![],
                )
            },
            exp,
        );
        cmds.push(draw_tree(root));
    }
    assert_ne!(cmds[0], cmds[1], "Right/Left 扇面朝向应不同");
    assert_ne!(cmds[0], cmds[2], "Right/Center 扇面朝向应不同");
}

/// 逐项回调：点击动作项执行 `FabAction::new` 的回调并收起菜单。
#[test]
fn fab_action_callback_fires_and_collapses() {
    use compose::layout::{dispatch_click_at, hit_test};
    let exp = mutableStateOf(false);
    let fired = mutableStateOf(false);
    let f2 = fired;
    let root = config_fab(
        compose_material::FabConfig {
            actions: vec![compose_material::FabAction::new(
                compose::material::IconName::Edit,
                "编辑",
                move || f2.set(true),
            )],
            ..compose_material::FabConfig::legacy(
                compose::material::IconName::Add,
                FabExpandType::Up,
                vec![],
            )
        },
        exp,
    );
    exp.set(true);
    // 泵帧让展开动画收敛，动作项真正滑入裁切盒（否则仍在盒外点不到）。
    for _ in 0..30 {
        compose::runtime::advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::loose(800.0, 600.0));
    }

    // 展开后逐个点击所有 on_click 节点（动作项与 FAB 本体），直到回调触发。
    let mut targets = Vec::new();
    for y in (0..600).step_by(8) {
        for x in (0..800).step_by(8) {
            if let Some((t, _, _)) = hit_test(root, x as f32, y as f32) {
                let clickable = compose::runtime::with_rt_pub(|rt| {
                    let mut cur = Some(t);
                    while let Some(id) = cur {
                        if rt.nodes[id.index()].on_click.is_some() {
                            return true;
                        }
                        cur = rt.nodes[id.index()].parent;
                    }
                    false
                });
                if clickable && !targets.contains(&(x, y)) {
                    targets.push((x, y));
                }
            }
        }
    }
    for (x, y) in targets {
        if let Some((t, _, _)) = hit_test(root, x as f32, y as f32) {
            let _ = dispatch_click_at(t, x as f32, y as f32);
        }
        if fired.get() {
            break;
        }
    }
    assert!(fired.get(), "FabAction::new 的回调应被触发");
    assert!(!exp.get(), "动作点击后应收起菜单");
}

/// rui `FabConfig::on_click` 兜底：无动作项时点击主按钮执行回调而非展开。
#[test]
fn fab_no_actions_click_fallback() {
    use compose::runtime::mutableStateOf;
    let exp = mutableStateOf(false);
    let fired = mutableStateOf(false);
    let f2 = fired;
    let root = config_fab(
        compose_material::FabConfig {
            on_click: Some(std::rc::Rc::new(move || f2.set(true))),
            actions: vec![],
            ..compose_material::FabConfig::legacy(
                compose::material::IconName::Add,
                FabExpandType::Up,
                vec![],
            )
        },
        exp,
    );
    let mut clicked = false;
    'outer: for y in (0..600).step_by(8) {
        for x in (0..800).step_by(8) {
            if let Some((t, _, _)) = hit_test(root, x as f32, y as f32) {
                let has_click = compose::runtime::with_rt_pub(|rt| rt.nodes[t.index()].on_click.is_some());
                if has_click {
                    let _ = dispatch_click_at(t, x as f32, y as f32);
                    clicked = true;
                    break 'outer;
                }
            }
        }
    }
    assert!(clicked, "应找到可点击的主按钮");
    assert!(fired.get(), "无动作项时应执行 on_click 兜底");
    assert!(!exp.get(), "无动作项时不应进入展开态");
}

/// rui `FabSize::Extended` + label：主按钮渲染文字标签。
#[test]
fn fab_config_extended_label_renders() {
    use compose::layout::draw_tree;
    use compose::material::IconName;
    let exp = mutableStateOf(false);
    let root = config_fab(
        compose_material::FabConfig {
            size: compose_material::FabSize::Extended,
            label: "新建".into(),
            actions: base_actions(),
            ..compose_material::FabConfig::legacy(IconName::Add, FabExpandType::Up, vec![])
        },
        exp,
    );
    let cmds = draw_tree(root);
    assert!(
        cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::Text { text, .. } if text == "新建")),
        "Extended FAB 应绘制标签: {cmds:?}"
    );
}

/// rui overlay barrier：展开后点击菜单外区域（全屏命中区）应收起，且收起后屏障不再命中。
#[test]
fn fab_barrier_collapses_on_outside_click() {
    let exp = mutableStateOf(false);
    let root = config_fab(
        compose_material::FabConfig {
            actions: base_actions(),
            ..compose_material::FabConfig::legacy(
                compose::material::IconName::Add,
                FabExpandType::Up,
                vec![],
            )
        },
        exp,
    );
    exp.set(true);
    for _ in 0..30 {
        compose::runtime::advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(800.0, 600.0));
    }
    assert!(exp.get(), "前置：展开态");

    // 点击远离 FAB 的空白区域（barrier 命中区 = 全屏）。
    let (t, _, _) = hit_test(root, 400.0, 40.0).expect("barrier 应命中空白区域");
    assert!(dispatch_click_at(t, 400.0, 40.0), "barrier 点击应被消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(800.0, 600.0));
    assert!(!exp.get(), "点击菜单外应收起");

    // 收起后屏障失效：同一位置点击不再消费（barrier 变为 0 尺寸无命中区）。
    let (t2, _, _) = hit_test(root, 400.0, 40.0).expect("空白区仍可命中节点本身");
    assert!(
        !dispatch_click_at(t2, 400.0, 40.0),
        "收起后 barrier 不应再拦截点击"
    );
}
