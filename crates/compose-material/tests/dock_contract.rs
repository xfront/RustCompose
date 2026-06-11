//! 停靠契约层对齐回归 —— 对齐 compose-docking 的 `model/Enums.kt`、`model/Ids.kt`、
//! `model/DockableOptions.kt`、`model/DockNode.kt`、`state/DockTarget.kt`、
//! `state/DockState.kt`。
//!
//! 覆盖：节点四种形态与稳定 id、`DockingStyle` 双向校验、`DockTarget` 解析、
//! `ensure_anchor` 占位还原、按 **节点 id** 的寻址操作。

use compose_material::{
    AnchorId, DOCK_RATIO_MAX, DOCK_RATIO_MIN, DockDirection, DockModel, DockNode, DockNodeId,
    DockRegion, DockState, DockTarget, DockableOptions, DockingStyle, SplitOrientation,
    WINDOW_MAIN, WindowBounds, dock_panel_info, dock_state, register_dock_panel,
};
use compose_material::{DockablePanel, PanelId};

fn setup(n: usize) -> (Vec<PanelId>, DockState) {
    compose_material::clear_dock_panels();
    compose_material::drain_dock_events();
    let ids: Vec<PanelId> = (0..n)
        .map(|i| register_dock_panel(DockablePanel::new(format!("P{i}")), || {}))
        .collect();
    let st = dock_state(DockModel::new(DockNode::tabs(&ids)));
    (ids, st)
}

// ── 节点形态（KT DockNode.kt） ──

#[test]
fn node_has_four_kotlin_shapes() {
    let leaf = DockNode::leaf(7);
    let tabs = DockNode::tabs(&[7, 8]);
    let split = DockNode::split(SplitOrientation::Horizontal, 0.4, leaf.clone(), tabs.clone());
    let anchor = DockNode::anchor(3);

    assert!(matches!(leaf, DockNode::Leaf { .. }));
    assert!(matches!(tabs, DockNode::Tabs { .. }));
    assert!(matches!(split, DockNode::Split { .. }));
    assert!(matches!(anchor, DockNode::Anchor { .. }));

    // 标签组成员恒为叶子，选中下标指向成员
    let DockNode::Tabs {
        tabs: members,
        selected_index,
        ..
    } = &tabs
    else {
        panic!("应为 Tabs");
    };
    assert_eq!(members.len(), 2);
    assert!(members.iter().all(|m| matches!(m, DockNode::Leaf { .. })));
    assert_eq!(*selected_index, 0);
    assert_eq!(tabs.selected_tab().unwrap().first_id(), Some(7));

    // 锚点节点不携带面板
    assert!(anchor.collect_ids().is_empty());
    assert!(anchor.contains_anchor(3));
    assert!(!anchor.contains_anchor(4));
}

#[test]
fn node_ids_are_stable_and_addressable() {
    let leaf = DockNode::leaf(1);
    let tabs = DockNode::tabs(&[2]);
    let split = DockNode::split(SplitOrientation::Vertical, 0.3, leaf.clone(), tabs.clone());

    assert_ne!(leaf.id(), tabs.id());
    assert_ne!(split.id(), leaf.id());
    assert_eq!(split.find_node(leaf.id()), Some(&leaf));
    assert_eq!(split.find_node(tabs.id()), Some(&tabs));
    assert!(split.find_node(DockNodeId::MAX).is_none());
    let leaf2 = split.find_leaf(2).expect("应找到面板 2 的叶子");
    assert!(matches!(leaf2, DockNode::Leaf { .. }));
    assert_eq!(leaf2.first_id(), Some(2));
}

#[test]
fn dock_region_keeps_legacy_alias() {
    let r: DockDirection = DockRegion::Center;
    assert_eq!(r, DockRegion::Center);
    assert!(!r.is_edge());
    assert!(DockRegion::North.is_edge());
    assert_eq!(DockRegion::North.index(), 0);
    assert_eq!(DockRegion::Center.index(), 4);
    assert_eq!(DockRegion::from_index(0), Some(DockRegion::North));
}

// ── DockingStyle 双向校验（KT Enums.kt） ──

#[test]
fn docking_style_is_checked_bidirectionally() {
    use compose_material::region_is_allowed;
    assert!(DockingStyle::Horizontal.allows_region(DockRegion::North));
    assert!(!DockingStyle::Horizontal.allows_region(DockRegion::East));
    assert!(DockingStyle::Vertical.allows_region(DockRegion::West));
    assert!(!DockingStyle::Vertical.allows_region(DockRegion::North));
    assert!(DockingStyle::CenterOnly.allows_region(DockRegion::Center));
    assert!(!DockingStyle::CenterOnly.allows_region(DockRegion::South));
    assert!(DockingStyle::Both.allows_region(DockRegion::East));

    assert!(region_is_allowed(
        DockRegion::North,
        DockingStyle::Both,
        DockingStyle::Horizontal
    ));
    assert!(!region_is_allowed(
        DockRegion::East,
        DockingStyle::Both,
        DockingStyle::Horizontal
    ));
    assert!(!region_is_allowed(
        DockRegion::North,
        DockingStyle::Vertical,
        DockingStyle::Both
    ));
}

// ── DockTarget + DockState 操作（KT DockState.kt） ──

#[test]
fn dock_to_target_and_region() {
    let (ids, st) = setup(3);
    st.reset(DockNode::tabs(&[ids[0]]));

    assert!(st.dock_to(
        ids[1],
        DockTarget::OnDockable(ids[0]),
        DockRegion::East,
        Some(0.3)
    ));
    assert!(st.is_docked(ids[1]));

    assert!(st.dock_to(ids[2], DockTarget::root(), DockRegion::South, None));
    let root = st.model().root.unwrap();
    assert_eq!(root.collect_ids().len(), 3);
}

#[test]
fn can_dock_respects_docking_style() {
    compose_material::clear_dock_panels();
    let opts = DockableOptions {
        docking_style: DockingStyle::CenterOnly,
        ..Default::default()
    };
    let a = register_dock_panel(DockablePanel::new("A"), || {});
    let b = register_dock_panel(DockablePanel::with_options("B", opts), || {});
    let st = dock_state(DockModel::new(DockNode::tabs(&[a])));

    assert!(!st.can_dock(b, DockTarget::OnDockable(a), DockRegion::East));
    assert!(st.can_dock(b, DockTarget::OnDockable(a), DockRegion::Center));
    // 目标侧风格也参与校验：b 是 CenterOnly → 边缘区域被拒，中心允许
    assert!(!st.can_dock(a, DockTarget::OnDockable(b), DockRegion::East));
    assert!(st.can_dock(a, DockTarget::OnDockable(b), DockRegion::Center));
}

#[test]
fn limited_to_window_blocks_cross_window_move() {
    compose_material::clear_dock_panels();
    let opts = DockableOptions {
        limited_to_window: true,
        ..Default::default()
    };
    let a = register_dock_panel(DockablePanel::new("A"), || {});
    let b = register_dock_panel(DockablePanel::with_options("B", opts), || {});
    let st = dock_state(DockModel::new(DockNode::tabs(&[a, b])));
    assert!(st.can_dock(b, DockTarget::OnDockable(a), DockRegion::East));
    // 跨窗口被拒（b 有当前窗口 = 主窗口）
    assert!(!st.can_dock(
        b,
        DockTarget::Root {
            window_id: 99
        },
        DockRegion::East
    ));
}

#[test]
fn ensure_anchor_creates_placeholder_once() {
    let (_ids, st) = setup(1);
    let anchor_id: AnchorId = 42;

    assert!(st.ensure_anchor(anchor_id, DockRegion::East, 0.25));
    assert!(st.model().root.unwrap().contains_anchor(anchor_id));

    // 幂等
    assert!(!st.ensure_anchor(anchor_id, DockRegion::East, 0.25));
    assert_eq!(st.model().root.unwrap().collect_ids().len(), 1);
}

#[test]
fn node_id_addressed_operations() {
    let (ids, st) = setup(3);
    st.reset(DockNode::tabs(&ids));

    let tabs_id = st.model().root.unwrap().id();

    assert!(st.select_tab(tabs_id, 1));
    assert_eq!(
        st.model()
            .root
            .unwrap()
            .selected_tab()
            .unwrap()
            .first_id(),
        Some(ids[1])
    );

    assert!(st.move_tab(tabs_id, 0, 2));
    let order = st.model().root.unwrap().group_panels();
    assert_eq!(order, vec![ids[1], ids[2], ids[0]]);

    assert!(!st.select_tab(tabs_id, 99));
    assert!(!st.move_tab(tabs_id, 0, 99));
}

#[test]
fn split_proportion_by_node_id() {
    let (ids, st) = setup(2);
    st.reset(DockNode::split(
        SplitOrientation::Horizontal,
        0.4,
        DockNode::tabs(&[ids[0]]),
        DockNode::tabs(&[ids[1]]),
    ));
    let split_id = st.model().root.unwrap().id();

    assert!(st.set_split_proportion(split_id, 0.7));
    let DockNode::Split { proportion, .. } = st.model().root.unwrap() else {
        panic!("应为 Split");
    };
    assert!((proportion - 0.7).abs() < 1e-6);

    assert!(st.reset_split_proportion(split_id));
    let DockNode::Split { proportion, .. } = st.model().root.unwrap() else {
        panic!("应为 Split");
    };
    assert!((proportion - 0.5).abs() < 1e-6);

    assert!(st.set_split_proportion(split_id, 5.0));
    let DockNode::Split { proportion, .. } = st.model().root.unwrap() else {
        panic!("应为 Split");
    };
    assert!((proportion - DOCK_RATIO_MAX).abs() < 1e-6);

    assert!(st.set_split_proportion(split_id, -1.0));
    let DockNode::Split { proportion, .. } = st.model().root.unwrap() else {
        panic!("应为 Split");
    };
    assert!((proportion - DOCK_RATIO_MIN).abs() < 1e-6);
}

#[test]
fn window_of_and_is_open() {
    let (ids, st) = setup(1);
    assert_eq!(st.window_of(ids[0]), Some(WINDOW_MAIN));
    assert!(st.is_open(ids[0]));
    assert!(!st.is_open(9999));

    st.float_panel(ids[0]);
    let wid = st.window_of(ids[0]).unwrap();
    assert_ne!(wid, WINDOW_MAIN);
    assert!(st.is_open(ids[0]));
}

#[test]
fn move_to_new_window_creates_floating_window() {
    let (ids, st) = setup(2);
    st.move_to_new_window(
        ids[0],
        Some(WindowBounds {
            x: 5.0,
            y: 6.0,
            width: 320.0,
            height: 240.0,
        }),
    );
    assert!(st.is_floating(ids[0]));
    let model = st.model();
    assert_eq!(model.floating.len(), 1);
    assert_eq!(model.floating[0].x, 5.0);
    assert_eq!(model.floating[0].w, 320.0);

    let ev = compose_material::drain_dock_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, compose_material::DockingEvent::WindowOpened(_))),
        "应派发 WindowOpened: {ev:?}"
    );
}

#[test]
fn undock_removes_from_layout() {
    let (ids, st) = setup(2);
    st.undock(ids[0]);
    assert!(!st.is_docked(ids[0]));
    let ev = compose_material::drain_dock_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, compose_material::DockingEvent::Undocked(_, _))),
        "应派发 Undocked: {ev:?}"
    );
}

#[test]
fn panel_options_default_to_permissive() {
    let (ids, _st) = setup(1);
    let info = dock_panel_info(ids[0]).unwrap();
    assert_eq!(info.options.docking_style, DockingStyle::Both);
    assert!(info.options.closable);
    assert!(info.options.floatable);
    assert!(!info.options.limited_to_window);
    assert_eq!(info.options.tab_preference, compose_material::TabPreference::Default);
    assert_eq!(info.options.anchor, None);
}
