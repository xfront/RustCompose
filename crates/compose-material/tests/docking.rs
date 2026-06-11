//! Docking 回归：布局树变换 / 5 区判定 / 几何 / 序列化 / 状态机 / 事件 / 组合渲染。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, TextStyle, create_root_node, dispatch_press_down,
    dispatch_press_up, dispatch_secondary_click_at, drag_2d_end_global, drag_2d_move_global,
    draw_tree, hit_test, perform_layout, set_last_pointer_global, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer};

use compose_material::{
    DOCK_SPLITTER_PX, DockDirection, DockEdge, DockLayout, DockModel, DockNode, DockRegion,
    DockState, DockTarget, DockableOptions, DockablePanel, DockingEvent, SetLiquidGlass,
    SplitOrientation, TabPreference, Text, clear_dock_panels, default_tab_preference,
    dock_direction_in_rect, dock_preview_rect, dock_split_bounds, dock_state, drain_dock_events,
    parse_dock_layout, register_dock_panel, serialize_dock_node, set_default_tab_preference,
};

fn text_values(cmds: &[DrawCmd]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// 每个测试跑在独立线程，thread_local 注册表天然隔离。
fn setup(n: usize) -> (Vec<u64>, DockState) {
    clear_dock_panels();
    drain_dock_events();
    let ids: Vec<u64> = (0..n)
        .map(|i| {
            register_dock_panel(DockablePanel::new(format!("P{i}")), || {})
        })
        .collect();
    let st = dock_state(DockModel::new(DockNode::tabs(&ids)));
    (ids, st)
}

fn two_pane_tree(a: u64, b: u64) -> DockNode {
    DockNode::split(
        SplitOrientation::Horizontal,
        0.4,
        DockNode::tabs(&[a]),
        DockNode::tabs(&[b]),
    )
}

fn group_items(node: &DockNode, id: u64) -> Option<Vec<u64>> {
    match node {
        DockNode::Tabs { tabs, .. } => {
            if node.collect_ids().contains(&id) {
                Some(tabs.iter().flat_map(|t| t.collect_ids()).collect())
            } else {
                None
            }
        }
        DockNode::Leaf { dockable_id, .. } => {
            if *dockable_id == id {
                Some(vec![id])
            } else {
                None
            }
        }
        DockNode::Anchor { .. } => None,
        DockNode::Split { first, second, .. } => {
            group_items(first, id).or_else(|| group_items(second, id))
        }
    }
}

// ── 树变换 ──

#[test]
fn center_dock_merges_into_tab_group() {
    let (ids, st) = setup(3);
    st.reset(two_pane_tree(ids[0], ids[1]));
    st.dock(ids[2], Some(ids[0]), DockDirection::Center, 0.5);
    let root = st.model().root.unwrap();
    let g = group_items(&root, ids[0]).expect("目标组仍应存在");
    assert_eq!(g.len(), 2, "中心停靠应合并成 2 个标签");
    assert!(g.contains(&ids[2]));
    assert_eq!(root.collect_ids().len(), 3, "不能重复/丢失面板");
}

#[test]
fn edge_dock_creates_split_with_correct_orientation() {
    let (ids, st) = setup(4);
    st.reset(DockNode::tabs(&[ids[0]]));
    st.dock(ids[1], Some(ids[0]), DockDirection::West, 0.3);
    st.dock(ids[2], Some(ids[0]), DockDirection::North, 0.3);
    st.dock(ids[3], Some(ids[0]), DockDirection::South, 0.3);
    let root = st.model().root.unwrap();
    assert_eq!(root.collect_ids().len(), 4);
    let has = |o: SplitOrientation| {
        fn walk(n: &DockNode, o: SplitOrientation) -> bool {
            match n {
                DockNode::Tabs { .. } | DockNode::Leaf { .. } | DockNode::Anchor { .. } => false,
                DockNode::Split {
                    orientation,
                    first,
                    second,
                    ..
                } => *orientation == o || walk(first, o) || walk(second, o),
            }
        }
        walk(&root, o)
    };
    assert!(has(SplitOrientation::Horizontal), "WEST/EAST 应为左右分割");
    assert!(has(SplitOrientation::Vertical), "NORTH/SOUTH 应为上下分割");
}

#[test]
fn removing_last_panel_collapses_split() {
    let (ids, st) = setup(2);
    st.reset(two_pane_tree(ids[0], ids[1]));
    st.close(ids[1]);
    let root = st.model().root.unwrap();
    assert_eq!(root.collect_ids(), vec![ids[0]]);
    assert!(matches!(root, DockNode::Tabs { .. }), "空侧应提升兄弟，消除多余分割");
}

#[test]
fn set_split_ratio_only_touches_addressed_level() {
    let (ids, st) = setup(3);
    // S(H, r0, T(a), S(V, r1, T(b), T(c)))
    st.reset(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[ids[0]]),
        DockNode::split(
            SplitOrientation::Vertical,
            0.5,
            DockNode::tabs(&[ids[1]]),
            DockNode::tabs(&[ids[2]]),
        ),
    ));
    st.set_split_ratio(&[1, 1], 0.7);
    let root = st.model().root.unwrap();
    match root {
        DockNode::Split { proportion, second, .. } => {
            assert!((proportion - 0.5).abs() < 1e-6, "外层比例不应被改");
            match *second {
                DockNode::Split { proportion, .. } => {
                    assert!((proportion - 0.7).abs() < 1e-6, "内层比例应更新为 0.7");
                }
                _ => panic!("内层应为 Split"),
            }
        }
        _ => panic!("根应为 Split"),
    }
}

#[test]
fn reorder_tab_moves_within_group() {
    let (ids, st) = setup(3);
    st.reset(DockNode::tabs(&ids));
    st.reorder_tab(ids[0], 2);
    let root = st.model().root.unwrap();
    assert_eq!(
        group_items(&root, ids[0]).unwrap(),
        vec![ids[1], ids[2], ids[0]]
    );
}

// ── 几何 ──

#[test]
fn five_zone_direction_detection() {
    let r = (0.0, 0.0, 100.0, 100.0);
    assert_eq!(dock_direction_in_rect(r, 50.0, 10.0), DockDirection::North);
    assert_eq!(dock_direction_in_rect(r, 50.0, 90.0), DockDirection::South);
    assert_eq!(dock_direction_in_rect(r, 10.0, 50.0), DockDirection::West);
    assert_eq!(dock_direction_in_rect(r, 90.0, 50.0), DockDirection::East);
    assert_eq!(dock_direction_in_rect(r, 50.0, 50.0), DockDirection::Center);
    // 角落先判上下
    assert_eq!(dock_direction_in_rect(r, 5.0, 5.0), DockDirection::North);
    assert_eq!(
        dock_preview_rect(r, DockDirection::West),
        (0.0, 0.0, 50.0, 100.0)
    );
}

#[test]
fn split_bounds_are_geometrically_consistent() {
    let (a, b) = dock_split_bounds(SplitOrientation::Horizontal, 0.5, (0.0, 0.0, 100.0, 50.0));
    assert_eq!(a.2 + DOCK_SPLITTER_PX + b.2, 100.0);
    assert_eq!(a.3, 50.0);
    assert_eq!(b.0, a.0 + a.2 + DOCK_SPLITTER_PX);
    let (c, d) = dock_split_bounds(SplitOrientation::Vertical, 0.25, (10.0, 20.0, 80.0, 100.0));
    assert_eq!(c.3 + DOCK_SPLITTER_PX + d.3, 100.0);
    assert_eq!(d.1, c.1 + c.3 + DOCK_SPLITTER_PX);
}

// ── 序列化 ──

#[test]
fn layout_serialize_parse_roundtrip() {
    let node = DockNode::split(
        SplitOrientation::Horizontal,
        0.35,
        DockNode::tabs(&[11, 12]),
        DockNode::split(
            SplitOrientation::Vertical,
            0.6,
            DockNode::tabs(&[13]),
            DockNode::tabs(&[14, 15]),
        ),
    );
    let s = serialize_dock_node(&node);
    // 新格式带节点 id：S(<nodeId>,<H|V>,<proportion>,<child1>,<child2>)
    assert!(s.starts_with("S("), "序列化格式: {s}");
    assert!(s.contains(",H,0.3500,"), "序列化格式: {s}");
    assert!(s.contains("L(") && s.contains("T("), "叶子/标签组标签: {s}");
    let back = parse_dock_layout(&s).expect("应可解析");
    assert_eq!(back, node);
    assert_eq!(back.collect_ids(), vec![11, 12, 13, 14, 15]);
    // 节点 id 随布局持久化（往返后保持不变）
    assert_eq!(back.id(), node.id());
    assert_eq!(serialize_dock_node(&back), s, "往返后文本应完全一致");
}

#[test]
fn invalid_layout_is_rejected() {
    assert!(parse_dock_layout("").is_none());
    assert!(parse_dock_layout("T(||)").is_none());
    assert!(parse_dock_layout("S(H,0.5,T(1|1))").is_none());
    assert!(parse_dock_layout("S(X,0.5,T(1|1),T(2|2))").is_none());
    let (_, st) = setup(1);
    assert!(!st.restore_layout("not a layout"));
}

#[test]
fn restore_layout_prunes_unregistered_panels() {
    let (ids, st) = setup(2);
    let s = format!(
        "S(H,0.5,T({}|{}),T(999999|999999))",
        ids[0], ids[0]
    );
    assert!(st.restore_layout(&s));
    let root = st.model().root.unwrap();
    assert_eq!(root.collect_ids(), vec![ids[0]], "未注册 id 应被剔除，空侧提升兄弟");
    assert!(matches!(root, DockNode::Tabs { .. }));
}

#[test]
fn full_state_roundtrip_keeps_hidden_floating_and_maximized() {
    let (ids, st) = setup(5);
    st.reset(DockNode::split(
        SplitOrientation::Horizontal,
        0.4,
        DockNode::tabs(&[ids[0], ids[1]]),
        DockNode::split(
            SplitOrientation::Vertical,
            0.5,
            DockNode::tabs(&[ids[2]]),
            DockNode::tabs(&[ids[3], ids[4]]),
        ),
    ));
    st.hide_to_edge(ids[4], DockEdge::Bottom);
    st.float_panel(ids[2]);
    st.maximize(ids[0]);
    let text = st.save_full_state();
    assert!(text.starts_with("mdl2\n"), "状态头: {text}");
    assert!(text.contains("hidden="));
    assert!(text.contains("float="));

    let st2 = dock_state(DockModel::empty());
    assert!(st2.restore_full_state(&text));
    assert_eq!(st.model(), st2.model(), "完整状态应逐字段还原");
    assert!(!st2.restore_full_state("garbage"));
}

// ── 状态机 ──

#[test]
fn hide_and_restore_keeps_position() {
    let (ids, st) = setup(2);
    let before = two_pane_tree(ids[0], ids[1]);
    st.reset(before.clone());
    st.hide_to_edge(ids[1], DockEdge::Right);
    assert!(st.is_hidden(ids[1]));
    assert_eq!(st.model().root.unwrap().collect_ids(), vec![ids[0]]);
    st.restore_from_edge(ids[1]);
    assert!(!st.is_hidden(ids[1]));
    assert_eq!(st.model().root.unwrap().collect_ids().len(), 2);
}

#[test]
fn float_and_unfloat_restores_group() {
    let (ids, st) = setup(3);
    st.reset(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[ids[0], ids[1]]),
        DockNode::tabs(&[ids[2]]),
    ));
    st.float_panel(ids[1]);
    assert!(st.is_floating(ids[1]));
    assert!(st.is_floating(ids[0]), "同标签组应作为一个单元一起浮起");
    st.set_floating_rect(ids[1], 150.0, 160.0, 420.0, 320.0);
    st.read(|m| {
        let f = m.floating.iter().find(|f| f.contains(ids[1])).unwrap();
        assert_eq!((f.x, f.y, f.w, f.h), (150.0, 160.0, 420.0, 320.0));
    });
    st.unfloat_panel(ids[1]);
    assert!(!st.is_floating(ids[1]));
    assert_eq!(st.model().root.unwrap().collect_ids().len(), 3);
}

#[test]
fn maximize_then_restore_returns_original_tree() {
    let (ids, st) = setup(3);
    let before = two_pane_tree(ids[0], ids[1]);
    st.reset(before.clone());
    st.maximize(ids[0]);
    assert!(st.is_maximized(ids[0]));
    assert_eq!(st.model().root.unwrap().collect_ids(), vec![ids[0]]);
    st.restore();
    assert_eq!(st.model().root, Some(before));
}

#[test]
fn close_removes_from_tree_and_aux_containers() {
    let (ids, st) = setup(3);
    st.reset(two_pane_tree(ids[0], ids[1]));
    st.hide_to_edge(ids[1], DockEdge::Left);
    st.close(ids[1]);
    assert!(!st.is_hidden(ids[1]));
    assert_eq!(st.model().root.unwrap().collect_ids(), vec![ids[0]]);
    assert!(!st.is_docked(ids[1]));
}

#[test]
fn root_level_dock_and_reorder_events() {
    let (ids, st) = setup(3);
    st.reset(DockNode::tabs(&[ids[0]]));
    drain_dock_events();
    st.dock(ids[1], None, DockDirection::East, 0.5);
    assert_eq!(st.model().root.unwrap().collect_ids(), vec![ids[0], ids[1]]);
    let ev = drain_dock_events();
    assert!(ev.contains(&DockingEvent::Docked(ids[1], 0, true)), "应派发 Docked: {ev:?}");
    assert!(ev.contains(&DockingEvent::Undocked(ids[1], true)), "应派发 Undocked: {ev:?}");

    drain_dock_events();
    st.hide_to_edge(ids[1], DockEdge::Right);
    let ev = drain_dock_events();
    assert!(
        ev.contains(&DockingEvent::AutoHidden(ids[1], DockEdge::Right)),
        "应派发 AutoHidden: {ev:?}"
    );
    assert!(ev.contains(&DockingEvent::Hidden(ids[1])), "应派发 Hidden: {ev:?}");
    drain_dock_events();
    st.restore_from_edge(ids[1]);
    assert!(drain_dock_events().contains(&DockingEvent::Pinned(ids[1])));
}

// ── 组合渲染（无头）──

#[test]
fn dock_layout_composes_and_renders_tree() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {
        Text("alpha-body", TextStyle::new(), Modifier::new());
    });
    let b = register_dock_panel(DockablePanel::new("Beta"), || {
        Text("beta-body", TextStyle::new(), Modifier::new());
    });
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[a]),
        DockNode::tabs(&[b]),
    )));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let cmds = draw_tree(root);
    let vals = text_values(&cmds);
    assert!(!cmds.is_empty(), "应产生绘制命令");
    assert!(vals.iter().any(|t| t.contains("Alpha")), "左标签应渲染: {vals:?}");
    assert!(vals.iter().any(|t| t.contains("Beta")), "右标签应渲染: {vals:?}");
    assert!(
        vals.iter().any(|t| t.contains("alpha-body")),
        "左面板内容应渲染: {vals:?}"
    );
    assert!(
        vals.iter().any(|t| t.contains("beta-body")),
        "右面板内容应渲染: {vals:?}"
    );
    std::mem::forget(comp);
}

#[test]
fn dock_layout_recomposes_after_docking() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("One"), || {
        Text("one-body", TextStyle::new(), Modifier::new());
    });
    let b = register_dock_panel(DockablePanel::new("Two"), || {
        Text("two-body", TextStyle::new(), Modifier::new());
    });
    let state = dock_state(DockModel::new(DockNode::tabs(&[a])));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));
    // 组合外改状态 → 重组 → 新面板应出现在绘制树里。
    state.dock(b, Some(a), DockDirection::Center, 0.5);
    compose::runtime::Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let vals = text_values(&draw_tree(root));
    assert!(vals.iter().any(|t| t.contains("One")), "原标签应保留: {vals:?}");
    assert!(vals.iter().any(|t| t.contains("Two")), "合并后的新标签应出现: {vals:?}");
    std::mem::forget(comp);
}

#[test]
fn dragging_tab_to_panel_center_merges_group() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Left"), || {
        Text("left-body", TextStyle::new(), Modifier::new());
    });
    let b = register_dock_panel(DockablePanel::new("Right"), || {
        Text("right-body", TextStyle::new(), Modifier::new());
    });
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[a]),
        DockNode::tabs(&[b]),
    )));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));

    // 右半区标签头（根 400 宽、0.5 分割 → 右侧从 ~202 开始）。
    let (sx, sy) = (225.0, 15.0);
    let hit = hit_test(root, sx, sy).expect("应命中右侧标签头");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy), "按下应起手势");
    // 拖到左半区中心（中心区 → 合并成标签组）。
    let (dx, dy) = (90.0, 150.0);
    assert!(drag_2d_move_global(dx, dy), "移动应由二维拖拽会话消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global(), "抬起应提交拖放");
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let r = state.model().root.expect("树不应为空");
    assert_eq!(
        group_items(&r, a),
        Some(vec![a, b]),
        "拖到中心应把 Right 合并进 Left 标签组，实际树: {:?}",
        r
    );
    assert_eq!(r.collect_ids().len(), 2, "面板不能丢失或重复");
    let vals = text_values(&draw_tree(root));
    assert!(vals.iter().any(|t| t == "Left"), "合并后仍显示 Left 标签: {vals:?}");
    assert!(vals.iter().any(|t| t == "Right"), "合并后应显示 Right 标签: {vals:?}");
    std::mem::forget(comp);
}

// ── 跨窗口停靠（浮动窗口 = 完整停靠目标） ──

/// 把 ids[0] 所在标签组浮起为单成员浮动窗口的公共前置。
fn float_single(st: &DockState, ids: &[u64], lead: u64) -> u32 {
    st.reset(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[lead]),
        DockNode::tabs(&ids.iter().copied().filter(|&i| i != lead).collect::<Vec<_>>()),
    ));
    st.float_panel(lead);
    st.window_of(lead).expect("浮起后应有窗口 id")
}

#[test]
fn dock_into_floating_window_center_merges() {
    let (ids, st) = setup(4);
    let wid = float_single(&st, &ids, ids[0]);
    assert!(wid > 0, "浮动窗口 id 应非零");

    // 拖 ids[1] 到浮动窗口面板的中心 → 并入浮动窗口标签组
    assert!(
        st.dock_to(ids[1], DockTarget::OnDockable(ids[0]), DockRegion::Center, Some(0.5)),
        "跨窗口中心停靠应成功"
    );
    st.read(|m| {
        let f = m.floating.iter().find(|f| f.id == wid).unwrap();
        assert_eq!(f.items(), vec![ids[0], ids[1]], "中心应并入浮动窗口标签组");
        assert!(matches!(f.root, DockNode::Tabs { .. }));
    });
    assert_eq!(st.window_of(ids[1]), Some(wid), "ids[1] 应归入浮动窗口");
    assert!(
        !st.is_docked(ids[1]),
        "ids[1] 不应再留在主窗口树"
    );
    let evs = drain_dock_events();
    assert!(
        evs.contains(&DockingEvent::Docked(ids[1], wid, true)),
        "应派发带浮动窗口 id 的 Docked 事件: {evs:?}"
    );
}

#[test]
fn dock_into_floating_window_edge_splits() {
    let (ids, st) = setup(4);
    let wid = float_single(&st, &ids, ids[0]);

    // 拖 ids[1] 到浮动窗口面板的南边 → 浮动窗口内部南北分割
    assert!(st.dock_to(
        ids[1],
        DockTarget::OnDockable(ids[0]),
        DockRegion::South,
        Some(0.5)
    ));
    st.read(|m| {
        let f = m.floating.iter().find(|f| f.id == wid).unwrap();
        assert!(
            matches!(f.root, DockNode::Split { orientation: SplitOrientation::Vertical, .. }),
            "南停靠应在浮动窗口内产生上下分割（Vertical = 堆叠），实际: {:?}",
            f.root
        );
        assert_eq!(f.items(), vec![ids[0], ids[1]]);
    });
    assert_eq!(st.window_of(ids[1]), Some(wid));
}

#[test]
fn can_dock_allows_cross_window_when_not_limited() {
    let (ids, st) = setup(3);
    let wid = float_single(&st, &ids, ids[0]);
    // 默认 options.floatable/limited_to_window=false → 跨窗口允许
    assert!(st.can_dock(ids[1], DockTarget::Root { window_id: wid }, DockRegion::South));
    // limited_to_window 面板不得跨窗口
    clear_dock_panels();
    let locked = register_dock_panel(
        DockablePanel::with_options(
            "Locked",
            DockableOptions {
                limited_to_window: true,
                ..Default::default()
            },
        ),
        || {},
    );
    let free = register_dock_panel(DockablePanel::new("Free"), || {});
    let st2 = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[locked]),
        DockNode::tabs(&[free]),
    )));
    // float_panel 浮起整个标签组 → locked 必须与 free 分属两组，才能留在主窗口
    st2.float_panel(free);
    let wid2 = st2.window_of(free).unwrap();
    assert_eq!(st2.window_of(locked), Some(compose_material::WINDOW_MAIN));
    assert!(
        !st2.can_dock(locked, DockTarget::Root { window_id: wid2 }, DockRegion::South),
        "limited_to_window 面板不得跨窗口停靠"
    );
}

#[test]
fn mdl2_roundtrip_preserves_floating_split_tree() {
    let (ids, st) = setup(4);
    let _ = float_single(&st, &ids, ids[0]);
    st.dock_to(ids[1], DockTarget::OnDockable(ids[0]), DockRegion::South, Some(0.5));

    let text = st.save_full_state();
    let st2 = dock_state(DockModel::empty());
    assert!(st2.restore_full_state(&text));
    assert_eq!(st.model(), st2.model(), "mdl2 应逐字段还原（含浮动窗口树）");
    st2.read(|m| {
        let f = &m.floating[0];
        assert!(
            matches!(f.root, DockNode::Split { .. }),
            "浮动窗口内的分割树应被持久化，实际: {:?}",
            f.root
        );
    });
}

#[test]
fn mdl1_float_line_still_parses() {
    let (ids, _st) = setup(4);
    // 旧 mdl1 浮动行：x|y|w|h|savedSib|savedDir|savedRatio|items|active（扁平标签组）
    let text = format!(
        "mdl1\ntree=\nmax=-\nsavedMax=\nfloat=100.0|50.0|400.0|300.0|-|-|0.3|{},{}|{}",
        ids[2], ids[3], ids[3]
    );
    let st2 = dock_state(DockModel::empty());
    assert!(st2.restore_full_state(&text), "旧 mdl1 应兼容读取");
    st2.read(|m| {
        assert_eq!(m.floating.len(), 1);
        let f = &m.floating[0];
        assert_eq!(f.items(), vec![ids[2], ids[3]], "旧格式应还原为扁平标签组");
        assert!(matches!(f.root, DockNode::Tabs { .. }));
        assert_eq!((f.x, f.y, f.w, f.h), (100.0, 50.0, 400.0, 300.0));
    });
}

// ── 标签条位置偏好（VS2022：文档标签恒在顶部，底部工具窗口标签在底部） ──

#[test]
fn tab_preference_resolves_global_default() {
    assert!(!TabPreference::Top.is_bottom());
    assert!(TabPreference::Bottom.is_bottom());
    assert!(TabPreference::BottomAlways.is_bottom());
    assert!(TabPreference::TopAlways.always_display_tabs());
    assert!(!TabPreference::Bottom.always_display_tabs());

    set_default_tab_preference(TabPreference::Bottom);
    assert_eq!(default_tab_preference(), TabPreference::Bottom);
    // 复原，避免污染同线程其它断言（每个测试独立线程，仍保持整洁）
    set_default_tab_preference(TabPreference::Top);
    assert_eq!(default_tab_preference(), TabPreference::Top);
}

/// 底部偏好：内容先于标签条绘制（draw_tree 里内容文本应在标签标题之前）。
#[test]
fn bottom_tab_preference_renders_tabs_below_content() {
    clear_dock_panels();
    drain_dock_events();
    let body = register_dock_panel(
        DockablePanel::with_options(
            "BODY",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {
            Text("BODY_CONTENT".to_string(), TextStyle::new(), Modifier::new());
        },
    );
    let state = dock_state(DockModel::new(DockNode::tabs(&[body])));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let texts = text_values(&draw_tree(root));
    let content_at = texts.iter().position(|t| t == "BODY_CONTENT");
    let tab_at = texts.iter().position(|t| t == "BODY");
    assert!(content_at.is_some() && tab_at.is_some(), "内容与标签都应绘制: {texts:?}");
    assert!(
        content_at.unwrap() < tab_at.unwrap(),
        "底部偏好时内容应先于标签条绘制，实际: {texts:?}"
    );
    std::mem::forget(comp);
}

/// 顶部偏好（默认）：标签条先于内容绘制。
#[test]
fn top_tab_preference_renders_tabs_above_content() {
    clear_dock_panels();
    drain_dock_events();
    let body = register_dock_panel(DockablePanel::new("TOPDOC"), || {
        Text("TOP_CONTENT".to_string(), TextStyle::new(), Modifier::new());
    });
    let state = dock_state(DockModel::new(DockNode::tabs(&[body])));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let texts = text_values(&draw_tree(root));
    let content_at = texts.iter().position(|t| t == "TOP_CONTENT");
    let tab_at = texts.iter().position(|t| t == "TOPDOC");
    assert!(content_at.is_some() && tab_at.is_some(), "内容与标签都应绘制: {texts:?}");
    assert!(
        tab_at.unwrap() < content_at.unwrap(),
        "顶部偏好时标签条应先于内容绘制，实际: {texts:?}"
    );
    std::mem::forget(comp);
}

// ── 标签条「全部面板」下拉（VS2022 / Tabs 控件「更多」语义） ──

#[test]
fn tab_dropdown_lists_all_group_panels_and_switches() {
    clear_dock_panels();
    drain_dock_events();
    let ids: Vec<u64> = (0..3)
        .map(|i| {
            register_dock_panel(DockablePanel::new(format!("Tab{i}")), || {
                Text("body".to_string(), TextStyle::new(), Modifier::new());
            })
        })
        .collect();
    let state = dock_state(DockModel::new(DockNode::tabs(&ids)));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let st = state;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(st, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));

    // 初始无下拉
    assert_eq!(state.tab_dropdown(), None);
    // 打开下拉 → 渲染出全部 3 个面板标题（活动项带 ✓）
    state.open_tab_dropdown(ids[0], 40.0, 40.0);
    assert_eq!(state.tab_dropdown(), Some((ids[0], 40.0, 40.0)));
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let texts = text_values(&draw_tree(root));
    for i in 0..3 {
        assert!(
            texts.iter().any(|t| t.contains(&format!("Tab{i}"))),
            "下拉应列出 Tab{i}: {texts:?}"
        );
    }
    assert!(texts.iter().any(|t| t.starts_with('✓')), "活动项应带 ✓: {texts:?}");

    // 从下拉切换到第 3 个面板 → 活动项变化、下拉关闭
    state.switch_tab(ids[2]);
    state.close_tab_dropdown();
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    assert_eq!(state.tab_dropdown(), None);
    let texts2 = text_values(&draw_tree(root));
    assert!(!texts2.iter().any(|t| t.starts_with('✓')), "关闭后浮层应消失: {texts2:?}");
    assert_eq!(state.model().root.as_ref().unwrap().active_panel(), Some(ids[2]));

    // reset 顺带清下拉
    state.open_tab_dropdown(ids[1], 10.0, 10.0);
    state.reset(DockNode::tabs(&ids));
    assert_eq!(state.tab_dropdown(), None, "reset 应清掉残留下拉");
    std::mem::forget(comp);
}

#[test]
fn tab_dropdown_works_for_edge_regions_and_floating_window() {
    // 四个方向的面板组都走 render_tab_bar：下拉应能解析各组的全部成员。
    let (ids, st) = setup(4);
    st.reset(DockNode::split(
        SplitOrientation::Vertical,
        0.5,
        DockNode::split(
            SplitOrientation::Horizontal,
            0.5,
            DockNode::tabs(&[ids[0]]),
            DockNode::tabs(&[ids[1]]),
        ),
        DockNode::split(
            SplitOrientation::Horizontal,
            0.5,
            DockNode::tabs(&[ids[2]]),
            DockNode::tabs(&[ids[3]]),
        ),
    ));
    for &lead in &ids {
        st.open_tab_dropdown(lead, 20.0, 20.0);
        assert_eq!(st.tab_dropdown(), Some((lead, 20.0, 20.0)));
        st.close_tab_dropdown();
    }
    // 浮动窗口里的标签组同样可被下拉解析
    st.reset(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[ids[0]]),
        DockNode::tabs(&[ids[1], ids[2], ids[3]]),
    ));
    st.move_to_new_window(ids[1], None);
    st.open_tab_dropdown(ids[2], 30.0, 30.0);
    assert_eq!(st.tab_dropdown(), Some((ids[2], 30.0, 30.0)));
    st.close_tab_dropdown();
}

// ── 下拉浮层定位：窗口全局坐标必须换算成 DockLayout 父内偏移 ──

/// 下拉浮层背景：宽 200、高 = 3 * 30 + 8 = 98 的圆角矩形，直接反映浮层绝对几何。
fn dropdown_panel_rect(cmds: &[DrawCmd]) -> (f32, f32, f32, f32) {
    cmds.iter()
        .find_map(|c| match c {
            DrawCmd::FillRRect { x, y, w, h, .. }
                if (w - 200.0).abs() < 1.0 && (h - 98.0).abs() < 1.0 =>
            {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .expect("应渲染 200×98 的下拉浮层背景")
}

#[test]
fn tab_dropdown_anchors_at_global_pointer_not_layout_center() {
    clear_dock_panels();
    drain_dock_events();
    let ids: Vec<u64> = (0..3)
        .map(|i| register_dock_panel(DockablePanel::new(format!("Dock{i}")), || {}))
        .collect();
    let state = dock_state(DockModel::new(DockNode::tabs(&ids)));
    set_viewport(1000.0, 600.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let st = state;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            // 根节点偏离窗口原点：模拟 IDE 外壳菜单栏 / padding 之下的内容区。
            DockLayout(st, Modifier::new().fill_max_size().offset(40.0, 60.0));
        });
    });

    let open_at = |st: DockState, lead: u64, x: f32, y: f32| {
        st.open_tab_dropdown(lead, x, y);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(1000.0, 600.0));
        dropdown_panel_rect(&draw_tree(root))
    };

    // 右上角标签组点「▼」：浮层宽 200，右缘贴视口右侧 → x = 1000-4-200 = 796。
    // 旧的 `x.min(600)` + 局部 offset 会把它丢到 (640, 140)，即布局中间。
    let (tx, ty, _, _) = open_at(state, ids[0], 900.0, 80.0);
    assert!((tx - 796.0).abs() < 4.0, "右侧下拉不应跑到布局中间，实际 x={tx}");
    assert!((ty - 80.0).abs() < 4.0, "右侧下拉应贴住触发点高度，实际 y={ty}");

    // 左侧点「▼」：不得被根节点偏移推走，也不得被硬 clamp 到中间。
    let (lx, ly, _, _) = open_at(state, ids[1], 60.0, 200.0);
    assert!((lx - 60.0).abs() < 4.0, "左侧下拉应贴近触发点，实际 x={lx}");
    assert!((ly - 200.0).abs() < 4.0, "左侧下拉应贴近触发点高度，实际 y={ly}");

    // 底部标签条点「▼」：纵向夹取到 600-4-98 = 498，完整落在视口内。
    let (bx, by, bw, bh) = open_at(state, ids[2], 500.0, 580.0);
    assert!((bx - 500.0).abs() < 4.0, "底部下拉水平位置不应漂移，实际 x={bx}");
    assert!((by - 498.0).abs() < 4.0, "底部下拉应被夹回视口内，实际 y={by}");
    assert!(bx + bw <= 1000.0, "浮层不应被窗口右边裁切");
    assert!(by + bh <= 600.0, "浮层不应被窗口底边裁切");

    std::mem::forget(comp);
}

// ── 液态玻璃：dock 全部表面跟随全局开关 ──

fn glass_panels(cmds: &[DrawCmd]) -> Vec<(f32, f32, f32, f32, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::GlassPanel { x, y, w, h, blur, .. } => Some((*x, *y, *w, *h, *blur)),
            _ => None,
        })
        .collect()
}

#[test]
fn dock_surfaces_follow_liquid_glass_toggle() {
    clear_dock_panels();
    drain_dock_events();
    SetLiquidGlass(false);
    let ids: Vec<u64> = (0..3)
        .map(|i| register_dock_panel(DockablePanel::new(format!("Glass{i}")), || {}))
        .collect();
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[ids[0]]),
        DockNode::tabs(&[ids[1], ids[2]]),
    )));
    set_viewport(1000.0, 600.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let st = state;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(st, Modifier::new().fill_max_size());
        });
    });
    let settle = || {
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(1000.0, 600.0));
        draw_tree(root)
    };

    // 关闭态：不透明底色，无玻璃命令。
    let cmds = settle();
    assert!(glass_panels(&cmds).is_empty(), "默认关闭不应产出 GlassPanel");
    assert!(
        cmds.iter()
            .any(|c| matches!(c, DrawCmd::FillRect { .. } | DrawCmd::FillRRect { .. })),
        "关闭态 dock 仍应绘制普通底色"
    );

    // 开启态：两组停靠面板都切到玻璃材质。
    SetLiquidGlass(true);
    let cmds = settle();
    let panels = glass_panels(&cmds);
    assert!(
        panels.len() >= 2,
        "两组停靠面板都应玻璃化，实际 {} 个",
        panels.len()
    );
    assert!(panels.iter().all(|p| p.4 > 0.0), "玻璃材质应带背景模糊");

    // 标签下拉：玻璃化且 pin_at_global 的定位不受材质切换影响。
    state.open_tab_dropdown(ids[1], 900.0, 80.0);
    let cmds = settle();
    let panels = glass_panels(&cmds);
    // ids[1] 所在标签组只有 2 个面板 → 浮层 200 × (2*30+8) = 200 × 68。
    let popup = panels
        .iter()
        .find(|p| (p.2 - 200.0).abs() < 1.0 && (p.3 - 68.0).abs() < 1.0)
        .expect("标签下拉浮层应为玻璃");
    assert!(
        (popup.0 - 796.0).abs() < 4.0,
        "玻璃态下拉仍应贴右侧触发点，实际 x={}",
        popup.0
    );
    assert!(
        (popup.1 - 80.0).abs() < 4.0,
        "玻璃态下拉仍应贴触发点高度，实际 y={}",
        popup.1
    );
    assert!(
        !cmds.iter().any(|c| matches!(
            c,
            DrawCmd::FillRRect { w, h, .. }
                if (w - 200.0).abs() < 1.0 && (h - 68.0).abs() < 1.0
        )),
        "玻璃态下拉不应再叠加不透明底色"
    );

    // 右键上下文菜单同样玻璃化。
    state.close_tab_dropdown();
    let _ = settle();
    set_last_pointer_global(50.0, 15.0);
    let hit = hit_test(root, 50.0, 15.0).expect("应命中左侧标签头");
    assert!(
        dispatch_secondary_click_at(hit.0, 50.0, 15.0),
        "右键应被 dock 标签头消费"
    );
    let panels = glass_panels(&settle());
    assert!(
        panels.iter().any(|p| (p.2 - 180.0).abs() < 1.0),
        "右键菜单应为玻璃材质: {panels:?}"
    );

    // 关闭后全部回到不透明底色。
    SetLiquidGlass(false);
    assert!(
        glass_panels(&settle()).is_empty(),
        "关闭后 dock 应恢复不透明底色"
    );
    std::mem::forget(comp);
}

#[test]
fn dock_glass_covers_floating_window_edge_strips_and_peek() {
    clear_dock_panels();
    drain_dock_events();
    SetLiquidGlass(true);
    let ids: Vec<u64> = (0..3)
        .map(|i| register_dock_panel(DockablePanel::new(format!("Surface{i}")), || {}))
        .collect();
    // ids[0] 单独一组：浮起它不会把待自动隐藏的 ids[1] 一起带走
    // （`move_to_new_window` 浮的是整个标签组）。
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Horizontal,
        0.5,
        DockNode::tabs(&[ids[0]]),
        DockNode::split(
            SplitOrientation::Vertical,
            0.5,
            DockNode::tabs(&[ids[1]]),
            DockNode::tabs(&[ids[2]]),
        ),
    )));
    set_viewport(1000.0, 600.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let st = state;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(st, Modifier::new().fill_max_size());
        });
    });
    let settle = || {
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(1000.0, 600.0));
        draw_tree(root)
    };

    // 先组合一次让 GROUP_RECTS 有几何（nearest_edge_for 需要）。
    let _ = settle();
    state.move_to_new_window(ids[0], None);
    state.set_floating_rect(ids[0], 500.0, 100.0, 300.0, 200.0);
    state.toggle_auto_hide(ids[1]);
    state.expand_from_edge(ids[1]);
    let panels = glass_panels(&settle());
    assert!(
        panels
            .iter()
            .any(|p| (p.2 - 300.0).abs() < 1.0 && (p.3 - 200.0).abs() < 1.0),
        "浮动窗口外壳应为玻璃: {panels:?}"
    );
    assert!(
        panels
            .iter()
            .any(|p| (p.2 - 30.0).abs() < 1.0 || (p.3 - 30.0).abs() < 1.0),
        "自动隐藏边条应为玻璃: {panels:?}"
    );
    assert!(state.is_floating(ids[0]), "ids[0] 应已浮起");
    assert!(state.is_hidden(ids[1]), "ids[1] 应已进入自动隐藏组");
    assert_eq!(
        state.read(|m| m.expanded),
        Some(ids[1]),
        "peek 应处于展开态"
    );
    assert!(
        panels.len() >= 4,
        "主面板组/浮动窗口/边条/peek 应各自玻璃化，实际 {} 个: {panels:?}",
        panels.len()
    );
    // peek 与主面板组同为全尺寸表面，两者都应产出玻璃材质（共两个全尺寸玻璃面）。
    let full = panels
        .iter()
        .filter(|p| (p.2 - 1000.0).abs() < 1.0 && (p.3 - 600.0).abs() < 1.0)
        .count();
    assert!(full >= 2, "主面板组与 peek 都应是玻璃: {panels:?}");
    assert!(panels.iter().all(|p| p.4 > 0.0), "所有玻璃表面都应带背景模糊");

    SetLiquidGlass(false);
    assert!(
        glass_panels(&settle()).is_empty(),
        "关闭后浮层与边条应恢复普通底色"
    );
    std::mem::forget(comp);
}
