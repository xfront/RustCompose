//! Docking 回归：布局树变换 / 5 区判定 / 几何 / 序列化 / 状态机 / 事件 / 组合渲染。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, TextStyle, create_root_node, dispatch_press_down,
    dispatch_press_up, dispatch_secondary_click_at, drag_2d_end_global, drag_2d_move_global,
    draw_tree, hit_test, perform_layout, set_last_pointer_global, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer};

use compose_material::{clear_dock_panels, default_tab_preference, dock_direction_in_rect, dock_guide_hit, dock_outer_guides, dock_preview_rect, dock_split_bounds, dock_state, drain_dock_events, parse_dock_layout, register_dock_panel, serialize_dock_node, set_default_tab_preference, DockDirection, DockEdge, DockLayout, DockModel, DockNode, DockRegion, DockState, DockTarget, DockableOptions, DockablePanel, DockingEvent, PanelId, SetLiquidGlass, SplitOrientation, TabPreference, Text, DOCK_SPLITTER_PX};

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
            register_dock_panel(DockablePanel::new(format!("P{i}")), move || {
                Text(format!("P{i}_BODY"), TextStyle::new(), Modifier::new());
            })
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
    // 分割条 path 寻址分割节点自身：根 = []，内层（根的 second）= [1]。
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
    // 拖内层（下方）分割条：只改内层，外层不动。
    st.set_split_ratio(&[1], 0.7);
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
    // 拖根（右侧）分割条：只改外层，内层保持 0.7。
    st.set_split_ratio(&[], 0.3);
    let root = st.model().root.unwrap();
    match root {
        DockNode::Split { proportion, second, .. } => {
            assert!((proportion - 0.3).abs() < 1e-6, "根比例应更新为 0.3");
            match *second {
                DockNode::Split { proportion, .. } => {
                    assert!((proportion - 0.7).abs() < 1e-6, "内层比例不应被根拖拽改动");
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

/// VS 双十字：外层大菱形以主窗口中心为基准、五向齐全且明显大于目标内层小罗盘。
#[test]
fn outer_edge_guides_hug_container_sides() {
    let root = (0.0, 0.0, 800.0, 600.0);
    let outer = dock_outer_guides(root);
    assert_eq!(outer.len(), 4, "外层十字应为容器四边的 4 向指南");
    let dirs: Vec<DockDirection> = outer.iter().map(|(d, _)| *d).collect();
    assert_eq!(
        dirs,
        vec![
            DockDirection::West,
            DockDirection::East,
            DockDirection::North,
            DockDirection::South,
        ],
        "外层方向顺序应为 [W, E, N, S]: {dirs:?}"
    );

    let rect_of = |d| outer.iter().find(|(k, _)| *k == d).unwrap().1;
    let w = rect_of(DockDirection::West);
    let e = rect_of(DockDirection::East);
    let n = rect_of(DockDirection::North);
    let s = rect_of(DockDirection::South);
    // 左右格贴容器左右边，纵向居中于容器。
    assert!(w.0 + w.2 / 2.0 < 40.0, "西格应贴容器左边: {w:?}");
    assert!((w.1 + w.3 / 2.0 - 300.0).abs() < 1.0, "西格应纵向居中: {w:?}");
    assert!(e.0 + e.2 / 2.0 > 760.0, "东格应贴容器右边: {e:?}");
    // 上下格贴容器上下边，横向居中于容器。
    assert!(n.1 + n.3 / 2.0 < 40.0, "北格应贴容器上边: {n:?}");
    assert!((n.0 + n.2 / 2.0 - 400.0).abs() < 1.0, "北格应横向居中: {n:?}");
    assert!(s.1 + s.3 / 2.0 > 560.0, "南格应贴容器下边: {s:?}");

    // 命中四边格应解析为对应方向。
    assert_eq!(
        dock_guide_hit(&outer, e.0 + e.2 / 2.0, e.1 + e.3 / 2.0),
        Some(DockDirection::East)
    );
    assert_eq!(
        dock_guide_hit(&outer, s.0 + s.2 / 2.0, s.1 + s.3 / 2.0),
        Some(DockDirection::South)
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

/// 两个面板是否同处一个 Horizontal split 的左右直接两侧（允许中间隔着更深的 Split）。
fn has_horizontal_pair(node: &DockNode, left: PanelId, right: PanelId) -> bool {
    match node {
        DockNode::Split {
            orientation: SplitOrientation::Horizontal,
            first,
            second,
            ..
        } => {
            (first.collect_ids().contains(&left) && second.collect_ids().contains(&right))
                || has_horizontal_pair(first, left, right)
                || has_horizontal_pair(second, left, right)
        }
        DockNode::Split { first, second, .. } => {
            has_horizontal_pair(first, left, right) || has_horizontal_pair(second, left, right)
        }
        _ => false,
    }
}

#[test]
fn hide_restore_nested_right_panel_returns_to_original_side() {
    let (ids, st) = setup(6);
    // VS2022 工作区：左导航 / 中文档+输出 / 右属性。属性在最内层 Horizontal split 的 second。
    st.reset(DockNode::split(
        SplitOrientation::Horizontal,
        0.20,
        DockNode::tabs(&[ids[0], ids[1]]),
        DockNode::split(
            SplitOrientation::Horizontal,
            0.78,
            DockNode::split(
                SplitOrientation::Vertical,
                0.72,
                DockNode::tabs(&[ids[2]]),
                DockNode::tabs(&[ids[3], ids[4]]),
            ),
            DockNode::tabs(&[ids[5]]),
        ),
    ));
    st.hide_to_edge(ids[5], DockEdge::Right);
    assert!(st.is_hidden(ids[5]));
    st.restore_from_edge(ids[5]);
    let after = st.model().root.expect("恢复后根不应为空");
    match &after {
        DockNode::Split { first, second, .. } => {
            assert!(
                second.collect_ids().contains(&ids[5]),
                "最右面板应回到根右侧区域，实际树: {after:?}"
            );
            assert!(
                !first.collect_ids().contains(&ids[5]),
                "最右面板不应被塞进左侧导航组，实际树: {after:?}"
            );
        }
        _ => panic!("根应为 split，实际: {after:?}"),
    }
    assert!(
        has_horizontal_pair(&after, ids[2], ids[5]),
        "属性面板应重新回到文档区右侧（原位置），实际树: {after:?}"
    );
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

/// 外层十字贴在**容器四条边中点**：拖到容器西边格应整窗西向停靠（被拖面板在最左），
/// 而不是并入某个目标标签组。同时验证该边格真的画在容器边缘。
#[test]
fn dragging_to_container_edge_docks_at_root() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("OL-A"), || {});
    let b = register_dock_panel(DockablePanel::new("OL-B"), || {});
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

    // 右半区标签头（复用已验证能命中的坐标）。
    let (sx, sy) = (225.0, 15.0);
    let hit = hit_test(root, sx, sy).expect("应命中右侧标签头");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy), "按下应起手势");

    // 外层十字格贴在容器西边中点（远离顶部标签条 / 目标组中心）。
    let edges = dock_outer_guides((0.0, 0.0, 400.0, 300.0));
    let west = edges.iter().find(|(d, _)| *d == DockDirection::West).unwrap().1;
    let (dx, dy) = (west.0 + west.2 / 2.0, west.1 + west.3 / 2.0);
    assert!(drag_2d_move_global(dx, dy), "移动应由二维拖拽会话消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    // 外层边格必须真的画在容器边缘（组合期若读未 place 的 ROOT_NODE 会整组漏画）。
    let cmds = draw_tree(root);
    let west_drawn = cmds.iter().any(|c| match c {
        DrawCmd::FillRRect { x, y, w, h, radius, .. } => {
            (x - west.0).abs() < 1.0
                && (y - west.1).abs() < 1.0
                && (w - west.2).abs() < 1.0
                && (h - west.3).abs() < 1.0
                && (radius - 5.0).abs() < 0.1
        }
        _ => false,
    });
    assert!(west_drawn, "拖拽时应绘制容器西边的外层指南格 @ {west:?}");

    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global(), "抬起应提交拖放");
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let r = state.model().root.expect("树不应为空");
    // 整窗西向分割：被拖面板 b 成为根 first（最左），且不与 a 同组。
    match &r {
        DockNode::Split {
            orientation: SplitOrientation::Horizontal,
            first,
            ..
        } => assert!(
            first.collect_ids().contains(&b),
            "容器西边格应把面板停到根级最左，实际树: {r:?}"
        ),
        _ => panic!("容器西向停靠应产生根级水平分割，实际: {r:?}"),
    }
    assert!(
        group_items(&r, b).map(|g| g.len()).unwrap_or(0) == 1,
        "容器西边格是整窗停靠，不应与 a 合并成一个标签组: {r:?}"
    );
    std::mem::forget(comp);
}

/// 外层十字贴在容器边缘（非目标组中心）：容器东边格应把面板整窗东向停靠到最右，
/// 且该格的位置确实靠近容器右边界。
#[test]
fn outer_edge_guide_hugs_container_and_docks_east() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("OW-A"), || {});
    let b = register_dock_panel(DockablePanel::new("OW-B"), || {});
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

    let edges = dock_outer_guides((0.0, 0.0, 400.0, 300.0));
    let east = edges.iter().find(|(d, _)| *d == DockDirection::East).unwrap().1;
    // 东边格应贴容器右边界（中心 x 接近 400），而非目标组中心。
    assert!(
        east.0 + east.2 / 2.0 > 360.0,
        "外层东格应贴容器右边缘: {east:?}"
    );

    let (sx, sy) = (225.0, 15.0); // 复用已验证能命中的右侧标签头，源为右半组 b
    let hit = hit_test(root, sx, sy).expect("应命中右侧标签头");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy), "按下应起手势");
    let (dx, dy) = (east.0 + east.2 / 2.0, east.1 + east.3 / 2.0);
    assert!(drag_2d_move_global(dx, dy), "移动应由二维拖拽会话消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global(), "抬起应提交拖放");
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let r = state.model().root.expect("树不应为空");
    match &r {
        DockNode::Split {
            orientation: SplitOrientation::Horizontal,
            second,
            ..
        } => assert!(
            second.collect_ids().contains(&b),
            "容器东边格应把面板停到根级最右，实际: {r:?}"
        ),
        _ => panic!("容器东向停靠应产生根级水平分割，实际: {r:?}"),
    }
    std::mem::forget(comp);
}

/// 外层十字必须画在**停靠容器（DockLayout 内容区）**的边，而不是整个视口/窗口。
/// 把 DockLayout 整体下移 60px 模拟内容区相对视口偏移：外层西格应落在内容区中线
/// （y≈210），而非视口中线（y≈150）。这正是修复前"外十字出界到标题栏"的成因。
#[test]
fn outer_edge_guides_use_container_rect_not_viewport() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("CT-A"), || {});
    let b = register_dock_panel(DockablePanel::new("CT-B"), || {});
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
            DockLayout(state, Modifier::new().fill_max_size().offset(0.0, 60.0));
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));

    // DockLayout 下移 60：内容区 = (0,60,400,300)；其外层西格中心 y = 60+150 = 210，
    // 而视口西格中心 y = 150 —— 两者可区分。
    let container = (0.0, 60.0, 400.0, 300.0);
    let edges = dock_outer_guides(container);
    let west = edges.iter().find(|(d, _)| *d == DockDirection::West).unwrap().1;
    let viewport_west = dock_outer_guides((0.0, 0.0, 400.0, 300.0))
        .iter().find(|(d, _)| *d == DockDirection::West).unwrap().1;
    assert!(
        west.1 - viewport_west.1 > 50.0,
        "内容区西格应明显比视口西格靠下"
    );

    // 右半标签头随 DockLayout 下移：原 (225,15) → (225,75)。
    let (sx, sy) = (225.0, 75.0);
    let hit = hit_test(root, sx, sy).expect("应命中右侧标签头");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy), "按下应起手势");
    let (dx, dy) = (west.0 + west.2 / 2.0, west.1 + west.3 / 2.0);
    assert!(drag_2d_move_global(dx, dy), "移动应由二维拖拽会话消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    // 绘制出的外层西格应位于内容区中线（≈west），而不是视口中线（≈viewport_west）。
    let cmds = draw_tree(root);
    let drawn = cmds.iter().any(|c| match c {
        DrawCmd::FillRRect { x, y, w, h, radius, .. } => {
            (x - west.0).abs() < 1.0 && (y - west.1).abs() < 1.0
                && (w - west.2).abs() < 1.0 && (h - west.3).abs() < 1.0
                && (radius - 5.0).abs() < 0.1
        }
        _ => false,
    });
    let drawn_at_viewport = cmds.iter().any(|c| match c {
        DrawCmd::FillRRect { x, y, w, h, radius, .. } => {
            (x - viewport_west.0).abs() < 1.0 && (y - viewport_west.1).abs() < 1.0
                && (w - viewport_west.2).abs() < 1.0 && (h - viewport_west.3).abs() < 1.0
                && (radius - 5.0).abs() < 0.1
        }
        _ => false,
    });
    assert!(drawn, "外层西格应画在内容区中线 @ {west:?}");
    assert!(!drawn_at_viewport, "外层西格不应画在视口中线 @ {viewport_west:?}");

    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global(), "抬起应提交拖放");
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
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
        panels.iter().any(|p| (p.2 - 190.0).abs() < 1.0),
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
    // peek 贴边整条 + 0.18s 滑入动画：泵帧到动画结束。首帧根节点尚未测量
    // （dock_layout_rect 暂为 0），布局后 peek 作用域随动画重读即得正确尺寸。
    let mut cmds = settle();
    for _ in 0..24 {
        compose::runtime::advance_frame(1.0 / 60.0);
        perform_layout(root, Constraints::tight(1000.0, 600.0));
        cmds = draw_tree(root);
    }
    let panels = glass_panels(&cmds);
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
    // 主面板组为全尺寸玻璃表面。
    assert!(
        panels
            .iter()
            .any(|p| (p.2 - 1000.0).abs() < 1.0 && (p.3 - 600.0).abs() < 1.0),
        "主面板组应是全尺寸玻璃: {panels:?}"
    );
    // VS 滑出预览：贴边整条（顶边 = 全宽 × clamp(高*0.32,180,360)=192），滑入到位 y≈0。
    assert!(
        panels
            .iter()
            .any(|p| (p.2 - 1000.0).abs() < 1.0 && (p.3 - 192.0).abs() < 1.0 && p.1.abs() < 1.0),
        "peek 应为贴边整条玻璃并滑入到位: {panels:?}"
    );
    assert!(panels.iter().all(|p| p.4 > 0.0), "所有玻璃表面都应带背景模糊");

    SetLiquidGlass(false);
    assert!(
        glass_panels(&settle()).is_empty(),
        "关闭后浮层与边条应恢复普通底色"
    );
    std::mem::forget(comp);
}

// ── 中键关闭 / Esc 取消 / 光标反馈 ──

/// 中键点击标签头 = 关闭该面板（VS 语义），不可关闭面板除外。
#[test]
fn middle_click_closes_tab() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));
    // 右半区标签头。
    let (sx, sy) = (225.0, 15.0);
    let hit = hit_test(root, sx, sy).expect("应命中右侧标签头");
    set_last_pointer_global(sx, sy);
    assert!(
        compose::layout::dispatch_middle_click_at(hit.0),
        "中键应命中标签的 on_middle_click"
    );
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let r = state.model().root.expect("树不应为空");
    assert_eq!(r.collect_ids(), vec![a], "中键应关闭 Beta，只剩 Alpha: {r:?}");
    std::mem::forget(comp);
}

/// Esc 取消分割条拖拽：恢复拖拽前比例，光标回 Default（不提交 on_end）。
#[test]
fn esc_cancels_splitter_drag_and_restores_ratio() {
    compose_material::install_router();
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    set_viewport(400.0, 300.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 300.0));
    // 根分割条（垂直缝；0.4 比例 → x≈158.4..162.4）。
    let (sx, sy) = (160.0, 150.0);
    let hit = hit_test(root, sx, sy).expect("应命中分割条");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy), "按下应起分割拖拽");
    assert_eq!(
        compose::cursor::current_cursor(),
        compose::cursor::Cursor::ResizeEw,
        "水平分割拖拽应为左右双箭头光标"
    );
    // 拖到 x=300 → 比例 ~0.75。
    assert!(drag_2d_move_global(300.0, 150.0));
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let dragged = match state.model().root.unwrap() {
        DockNode::Split { proportion, .. } => proportion,
        _ => panic!("根应为 Split"),
    };
    assert!(dragged > 0.6, "拖拽应已改比例: {dragged}");
    // Esc → 取消并恢复 0.4（two_pane_tree 初始比例）。
    compose::input_router::escape();
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let restored = match state.model().root.unwrap() {
        DockNode::Split { proportion, .. } => proportion,
        _ => panic!("根应为 Split"),
    };
    assert!(
        (restored - 0.4).abs() < 1e-6,
        "Esc 应恢复拖拽前比例 0.4: {restored}"
    );
    assert_eq!(
        compose::cursor::current_cursor(),
        compose::cursor::Cursor::Default,
        "取消后光标应回 Default"
    );
    std::mem::forget(comp);
}

// ── 底部面板自动隐藏应落在底部边缘 ──

#[test]
fn bottom_panel_auto_hide_uses_bottom_edge() {
    clear_dock_panels();
    let ids: Vec<u64> = (0..3)
        .map(|i| register_dock_panel(DockablePanel::new(format!("Edge{i}")), || {}))
        .collect();
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Vertical,
        0.7,
        DockNode::tabs(&[ids[0]]),
        DockNode::tabs(&[ids[1], ids[2]]),
    )));
    set_viewport(800.0, 600.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let st = state;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(st, Modifier::new().fill_max_size());
        });
    });
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(800.0, 600.0));
    // GROUP_RECTS 已由上面这帧组合填充；与真实点击回调处于同一上下文。
    state.toggle_auto_hide(ids[1]);
    let hidden = state.model().hidden;
    assert_eq!(hidden.len(), 1, "应有一个隐藏组");
    assert_eq!(
        hidden[0].edge,
        DockEdge::Bottom,
        "底部面板应隐藏到底部边缘，实际 {:?}",
        hidden[0].edge
    );
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(800.0, 600.0));
    let strips: Vec<(f32, f32, f32, f32)> = draw_tree(root)
        .iter()
        .filter_map(|c| match c {
            DrawCmd::FillRect { x, y, w, h, .. } if *w > 700.0 && *h < 60.0 => {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .collect();
    assert!(!strips.is_empty(), "应绘制横向边条，实际: {strips:?}");
    assert!(
        strips.iter().any(|s| s.1 > 500.0),
        "底部边条应贴在窗口底部（另一个 y=0 的是主面板标签条），实际: {strips:?}"
    );
    std::mem::forget(comp);
}

#[test]
fn auto_hide_before_layout_uses_pointer_instead_of_root_center() {
    clear_dock_panels();
    let ids: Vec<u64> = (0..2)
        .map(|i| register_dock_panel(DockablePanel::new(format!("Pre{i}")), || {}))
        .collect();
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Vertical,
        0.7,
        DockNode::tabs(&[ids[0]]),
        DockNode::tabs(&[ids[1]]),
    )));
    set_viewport(1000.0, 600.0);
    // 尚未组合：GROUP_RECTS 为空，不能回退到 root 中心（宽窗口会判成 Top/Left）。
    set_last_pointer_global(500.0, 550.0);
    state.toggle_auto_hide(ids[1]);
    let hidden = state.model().hidden;
    assert_eq!(hidden.len(), 1);
    assert_eq!(
        hidden[0].edge,
        DockEdge::Bottom,
        "几何表缺失时应按指针位置判定为底部，实际 {:?}",
        hidden[0].edge
    );
}

// ── 边条全高 + 中央内容区自动缩小 ──

/// 有自动隐藏边条时：边条保持全高，中央内容区向内让出边条厚度（不被覆盖）。
#[test]
fn side_strip_full_height_and_content_shrinks() {
    use compose::foundation::Constraints;
    use compose::runtime::{Composition, Recomposer, advance_frame};

    compose_material::clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Doc"), || {
        Text("doc-body", TextStyle::new(), Modifier::new());
    });
    let x = register_dock_panel(DockablePanel::new("Center"), || {
        Text("center-body", TextStyle::new(), Modifier::new());
    });
    let b = register_dock_panel(DockablePanel::new("Errors"), || {
        Text("err-body", TextStyle::new(), Modifier::new());
    });
    // 中心 + 底部（South 分割）+ 左侧独立组（随后自动隐藏到左边条）。
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Vertical,
        0.7,
        DockNode::tabs(&[x]),
        DockNode::tabs(&[b]),
    )));
    state.dock(a, Some(x), DockRegion::West, 0.25);
    state.hide_to_edge(a, DockEdge::Left);

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
    let cmds = draw_tree(root);

    // 1) 边条全高（30×300，不因底部停靠面板而缩短）。
    let strips: Vec<(f32, f32, f32, f32)> = cmds
        .iter()
        .filter_map(|c| match c {
            DrawCmd::FillRect { x, y, w, h, .. } if (*w - 30.0).abs() < 1.0 => {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .collect();
    assert!(
        strips.iter().any(|&(x, y, _, h)| x < 1.0 && y < 1.0 && h > 299.0),
        "左边条应全高渲染: {strips:?}"
    );

    // 2) 中央内容区左缘让出边条厚度：center-body 文本 x ≥ 30。
    let cx = cmds.iter().find_map(|c| match c {
        DrawCmd::Text { x, text, .. } if text == "center-body" => Some(*x),
        _ => None,
    });
    assert!(
        cx.is_some_and(|x| x >= 30.0),
        "中央内容应向右让出边条厚度 30: x={cx:?}"
    );

    // 3) 底部面板（错误列表）不被边条覆盖：err-body 正常渲染在底部区域。
    let (ex, ey) = cmds
        .iter()
        .find_map(|c| match c {
            DrawCmd::Text { x, y, text, .. } if text == "err-body" => Some((*x, *y)),
            _ => None,
        })
        .expect("底部面板内容应渲染");
    assert!(ey > 210.0, "err-body 应在底部区域: ({ex},{ey})");
}

// ── 标签位置跟随当前停靠边（偏好仅兜底） ──

/// 同一面板从顶边挪到底边后，标签条必须从内容上方换到下方；
/// 反之默认偏好的面板挪到底边也要换到下方（不再由初始 tab_preference 决定）。
#[test]
fn tab_position_follows_current_dock_edge() {
    clear_dock_panels();
    drain_dock_events();
    let bottom_pref = register_dock_panel(
        DockablePanel::with_options(
            "BOTTOMPREF",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {
            Text("BOTTOMPREF_BODY".to_string(), TextStyle::new(), Modifier::new());
        },
    );
    let default_pref = register_dock_panel(DockablePanel::new("DEFAULTPREF"), || {
        Text("DEFAULTPREF_BODY".to_string(), TextStyle::new(), Modifier::new());
    });
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Vertical,
        0.5,
        DockNode::tabs(&[bottom_pref]),
        DockNode::tabs(&[default_pref]),
    )));
    set_viewport(400.0, 300.0);
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
        perform_layout(root, Constraints::tight(400.0, 300.0));
        text_values(&draw_tree(root))
    };
    let tab_is_below = |texts: &[String], title: &str, body: &str| {
        let t = texts.iter().position(|s| s == title).expect("标签文本应绘制");
        let b = texts.iter().position(|s| s == body).expect("内容文本应绘制");
        b < t
    };

    // bottom_pref 在上边 → 尽管偏好 Bottom，标签仍应在内容上方；
    // default_pref 在下边 → 尽管默认偏好 Top，标签仍应在内容下方。
    let texts = settle();
    assert!(
        !tab_is_below(&texts, "BOTTOMPREF", "BOTTOMPREF_BODY"),
        "当前停在上边 ⇒ 标签应在上（忽略 BottomAlways）: {texts:?}"
    );
    assert!(
        tab_is_below(&texts, "DEFAULTPREF", "DEFAULTPREF_BODY"),
        "当前停在下边 ⇒ 标签应在下（忽略默认 Top）: {texts:?}"
    );

    // 树中上下互换 → 两者标签位置跟着翻转。
    state.reset(DockNode::split(
        SplitOrientation::Vertical,
        0.5,
        DockNode::tabs(&[default_pref]),
        DockNode::tabs(&[bottom_pref]),
    ));
    let texts = settle();
    assert!(
        tab_is_below(&texts, "BOTTOMPREF", "BOTTOMPREF_BODY"),
        "挪到下边 ⇒ 标签应在下: {texts:?}"
    );
    assert!(
        !tab_is_below(&texts, "DEFAULTPREF", "DEFAULTPREF_BODY"),
        "挪到上边 ⇒ 标签应在上: {texts:?}"
    );
}

// ── 底部标签拖到侧边（回归：曾把底部条二分左右两块） ──

/// 从底部「输出/错误列表」标签拖到窗口**左缘但仍在底部高度**：
/// 应命中整窗外缘带 → 全高左侧面板；不应命中底部组的内层西方位把底边条一分为二。
#[test]
fn bottom_tab_drag_to_left_edge_docks_full_height_panel() {
    let (ids, state) = setup(3);
    state.reset(DockNode::split(
        SplitOrientation::Vertical,
        0.7,
        DockNode::tabs(&[ids[0]]),
        DockNode::tabs(&[ids[1], ids[2]]),
    ));
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
    let (tx, ty) = draw_tree(root)
        .iter()
        .find_map(|c| match c {
            DrawCmd::Text { text, x, y, .. } if text == "P1" => Some((x + 8.0, y + 8.0)),
            _ => None,
        })
        .expect("底部标签应绘制");
    let (target, lx, ly) = hit_test(root, tx, ty).expect("底部标签应可命中");
    set_last_pointer_global(tx, ty);
    assert!(dispatch_press_down(target, lx, ly), "按下底部标签应起手势");

    // 释放点贴左缘、但 y 仍在底部面板范围内：旧逻辑会先命中底部组内罗盘 West。
    let (dx, dy) = (4.0, ty);
    assert!(drag_2d_move_global(dx, dy), "移动应被拖拽会话消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global(), "抬起应提交");
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let tree = state.model().root.expect("布局应存在");
    let (first, second) = match &tree {
        DockNode::Split {
            orientation,
            first,
            second,
            ..
        } => {
            assert_eq!(
                *orientation,
                SplitOrientation::Horizontal,
                "整窗左停靠应为左右分割: {tree:?}"
            );
            (first, second)
        }
        _ => panic!("应为 Split: {tree:?}"),
    };
    assert!(
        first.collect_ids().contains(&ids[1]),
        "被拖面板应独占左侧全高面板: {tree:?}"
    );
    assert!(
        second.collect_ids().contains(&ids[0]) && second.collect_ids().contains(&ids[2]),
        "其余两个面板应仍在右侧原布局中（顶部 + 底部）：{tree:?}"
    );
}

/// 图1 → 图2：底部「错误列表/输出」组的标签拖到文档组内层西方指南，
/// 应切成左右两个面板留在上半区；两子组不再贴底边，标签自动回到内容上方。
#[test]
fn bottom_tabs_split_side_by_side_with_tabs_on_top() {
    clear_dock_panels();
    drain_dock_events();
    let doc = register_dock_panel(DockablePanel::new("DOC"), || {
        Text("DOC_BODY".to_string(), TextStyle::new(), Modifier::new());
    });
    let bottom = |name: &str| {
        register_dock_panel(
            DockablePanel::with_options(
                name,
                DockableOptions {
                    tab_preference: TabPreference::BottomAlways,
                    ..Default::default()
                },
            ),
            || {},
        )
    };
    let p1 = bottom("P1");
    let p2 = bottom("P2");
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Vertical,
        0.7,
        DockNode::tabs(&[doc]),
        DockNode::tabs(&[p1, p2]),
    )));
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
    // 文档组几何 = 根的上 70%；其内层 West 指南在上半区。
    let inner = compose_material::dock_inner_guides((0.0, 0.0, 400.0, 210.0));
    let west = inner
        .iter()
        .find(|(dir, _)| *dir == DockDirection::West)
        .unwrap()
        .1;
    let (dx, dy) = (west.0 + west.2 / 2.0, west.1 + west.3 / 2.0);

    // 源标签「P1」在底边条上（BottomAlways）。
    let (tx, ty) = draw_tree(root)
        .iter()
        .find_map(|c| match c {
            DrawCmd::Text { text, x, y, .. } if text == "P1" => Some((x + 8.0, y + 8.0)),
            _ => None,
        })
        .expect("P1 标签应绘制在底边");
    assert!(ty > 210.0, "P1 标签应在底边: {ty}");
    let (target, lx, ly) = hit_test(root, tx, ty).expect("标签应可命中");
    set_last_pointer_global(tx, ty);
    assert!(dispatch_press_down(target, lx, ly));
    assert!(drag_2d_move_global(dx, dy));
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global());
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let tree = state.model().root.expect("布局应存在");
    // 根仍为上/下；上半区应变成左右两个面板。
    let top = match &tree {
        DockNode::Split { first, .. } => first,
        _ => panic!("根应为上下 Split: {tree:?}"),
    };
    let (left_ids, right_ids) = match &**top {
        DockNode::Split { orientation, first, second, .. } => {
            assert_eq!(*orientation, SplitOrientation::Horizontal, "上半区应切成左右: {tree:?}");
            (first.collect_ids(), second.collect_ids())
        }
        _ => panic!("上半区应为左右 Split: {tree:?}"),
    };
    assert!(left_ids.contains(&p1), "P1 应拖到左侧: {tree:?}");
    assert!(right_ids.contains(&doc), "DOC 应仍在右侧: {tree:?}");
    // P2 留在底边；被拖出的 P1 标签应回到上方。
    let cmds = draw_tree(root);
    let texts = text_values(&cmds);
    assert!(texts.iter().any(|t| t == "P2"), "底边组应保留 P2");
    let p1_y = cmds.iter().find_map(|c| match c {
        DrawCmd::Text { text, y, .. } if text == "P1" => Some(*y),
        _ => None,
    }).expect("P1 标签应绘制");
    assert!(p1_y < 100.0, "P1 不再贴底边，标签应在上方: {p1_y}");
}

/// 拖动标签条空白区（红框区域）：整组标签一起作为单元投放，不是只拖活动标签。
#[test]
fn dragging_tab_bar_blank_area_moves_whole_group() {
    clear_dock_panels();
    drain_dock_events();
    let doc = register_dock_panel(DockablePanel::new("DOC"), || {});
    let p1 = register_dock_panel(
        DockablePanel::with_options(
            "P1",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {},
    );
    let p2 = register_dock_panel(
        DockablePanel::with_options(
            "P2",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {},
    );
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Vertical,
        0.7,
        DockNode::tabs(&[doc]),
        DockNode::tabs(&[p1, p2]),
    )));
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
    // 空白区：底边标签条上、标签右侧（避开箭头/图钉按钮）。
    let (tx, ty) = (240.0, 285.0);
    let (target, lx, ly) = hit_test(root, tx, ty).expect("标签条空白区应可命中");
    set_last_pointer_global(tx, ty);
    assert!(dispatch_press_down(target, lx, ly), "空白区按下应起整组拖拽");

    let inner = compose_material::dock_inner_guides((0.0, 0.0, 400.0, 210.0));
    let west = inner
        .iter()
        .find(|(dir, _)| *dir == DockDirection::West)
        .unwrap()
        .1;
    let (dx, dy) = (west.0 + west.2 / 2.0, west.1 + west.3 / 2.0);
    assert!(drag_2d_move_global(dx, dy));
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global());
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let tree = state.model().root.expect("布局应存在");
    let (left_ids, right_ids) = match &tree {
        DockNode::Split { orientation, first, second, .. } => {
            assert_eq!(*orientation, SplitOrientation::Horizontal, "应变成左右 Split: {tree:?}");
            (first.collect_ids(), second.collect_ids())
        }
        _ => panic!("根应为左右 Split: {tree:?}"),
    };
    assert!(
        left_ids.contains(&p1) && left_ids.contains(&p2),
        "P1/P2 应作为整组一起移动: {tree:?}"
    );
    assert!(right_ids.contains(&doc), "DOC 应在右侧: {tree:?}");
}

/// 拖拽入口只在标签条：面板内容区按下不应启动 drag & drop。
#[test]
fn dragging_panel_body_does_not_start_drag() {
    clear_dock_panels();
    drain_dock_events();
    let doc = register_dock_panel(DockablePanel::new("DOC"), || {});
    let p1 = register_dock_panel(
        DockablePanel::with_options(
            "P1",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {},
    );
    let p2 = register_dock_panel(
        DockablePanel::with_options(
            "P2",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {},
    );
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Vertical,
        0.7,
        DockNode::tabs(&[doc]),
        DockNode::tabs(&[p1, p2]),
    )));
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

    // 底部面板主体内容区空白（标签条在底边，内容区在其上方）。
    let (tx, ty) = (240.0, 250.0);
    let (target, lx, ly) = hit_test(root, tx, ty).expect("面板主体空白应可命中");
    set_last_pointer_global(tx, ty);
    // 内容区没有 on_click/on_press：dispatch 不会登记为可拖拽按压。
    let pressed = dispatch_press_down(target, lx, ly);
    assert!(!pressed || state.drag().source.is_none(), "内容区按下不应启动拖拽");

    // 即便后续移动，也不应出现 drop 指南或改变布局。
    let inner = compose_material::dock_inner_guides((0.0, 0.0, 400.0, 210.0));
    let west = inner
        .iter()
        .find(|(dir, _)| *dir == DockDirection::West)
        .unwrap()
        .1;
    let (dx, dy) = (west.0 + west.2 / 2.0, west.1 + west.3 / 2.0);
    drag_2d_move_global(dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    assert!(state.drag().source.is_none(), "内容区拖动不应起拖拽源");
    assert!(state.drag().target.is_none(), "内容区拖动不应显示 drop target");
}

/// 图1 → 图2（同组拆分）：两个标签在同一 dockpanel 时，把其中一个拖到
/// 本标签条右端应出现 drop target（Inner East），拆成左右两个面板；标签到上方。
#[test]
fn same_group_tab_drag_to_bar_end_splits_side_by_side() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(
        DockablePanel::with_options(
            "P1",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {},
    );
    let b = register_dock_panel(
        DockablePanel::with_options(
            "P2",
            DockableOptions {
                tab_preference: TabPreference::BottomAlways,
                ..Default::default()
            },
        ),
        || {},
    );
    let state = dock_state(DockModel::new(DockNode::tabs(&[a, b])));
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
    let (sx, sy) = draw_tree(root)
        .iter()
        .find_map(|c| match c {
            DrawCmd::Text { text, x, y, .. } if text == "P1" => Some((x + 8.0, y + 8.0)),
            _ => None,
        })
        .expect("P1 标签应绘制");
    let (target, lx, ly) = hit_test(root, sx, sy).expect("标签应可命中");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(target, lx, ly));

    // 同组标签条右端（远离箭头/按钮区，落在 bar 右侧 20% 内）。
    let (dx, dy) = (350.0, sy);
    assert!(drag_2d_move_global(dx, dy));
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let guide = state.drag().guide;
    assert!(
        matches!(guide, Some(compose_material::DockGuide::Inner { dir: DockDirection::East, .. })),
        "拖到本标签条右端应出现 Inner East drop target: {guide:?}"
    );
    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global());
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let tree = state.model().root.expect("布局应存在");
    let (l, r) = match &tree {
        DockNode::Split { orientation, first, second, .. } => {
            assert_eq!(*orientation, SplitOrientation::Horizontal, "应拆成左右: {tree:?}");
            (first.collect_ids(), second.collect_ids())
        }
        _ => panic!("应为左右 Split: {tree:?}"),
    };
    assert_eq!(l.len(), 1);
    assert_eq!(r.len(), 1);
    assert!(l.contains(&b) && r.contains(&a), "P1 应拆到右侧、P2 留左侧: {tree:?}");
    // 拆开后两组都离开底边 ⇒ 标签回到上方。
    let cmds = draw_tree(root);
    let p1_y = cmds.iter().find_map(|c| match c {
        DrawCmd::Text { text, y, .. } if text == "P1" => Some(*y),
        _ => None,
    }).expect("P1 标签应绘制");
    assert!(p1_y < 100.0, "拆出后标签应在上方: {p1_y}");
}

/// story 真实嵌套：左栏 + 上文档 + 底部输出组；从底部组标签拖到本标签条右端。
#[test]
fn nested_bottom_group_tab_splits_in_place() {
    clear_dock_panels();
    drain_dock_events();
    let mk = |name: &str, bottom: bool| {
        if bottom {
            register_dock_panel(
                DockablePanel::with_options(
                    name,
                    DockableOptions {
                        tab_preference: TabPreference::BottomAlways,
                        ..Default::default()
                    },
                ),
                || {},
            )
        } else {
            register_dock_panel(DockablePanel::new(name), || {})
        }
    };
    let side = mk("SIDE", false);
    let doc = mk("DOC", false);
    let p1 = mk("P1", true);
    let p2 = mk("P2", true);
    let state = dock_state(DockModel::new(DockNode::split(
        SplitOrientation::Horizontal,
        0.2,
        DockNode::tabs(&[side]),
        DockNode::split(
            SplitOrientation::Vertical,
            0.7,
            DockNode::tabs(&[doc]),
            DockNode::tabs(&[p1, p2]),
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
    perform_layout(root, Constraints::tight(1000.0, 600.0));
    let (sx, sy) = draw_tree(root)
        .iter()
        .find_map(|c| match c {
            DrawCmd::Text { text, x, y, .. } if text == "P1" => Some((x + 8.0, y + 8.0)),
            _ => None,
        })
        .expect("P1 标签应绘制");
    let (target, lx, ly) = hit_test(root, sx, sy).expect("标签应可命中");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(target, lx, ly));
    let (dx, dy) = (900.0, sy);
    assert!(drag_2d_move_global(dx, dy));
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(1000.0, 600.0));
    let guide = state.drag().guide;
    assert!(
        matches!(guide, Some(compose_material::DockGuide::Inner { dir: DockDirection::East, .. })),
        "嵌套场景下也应出现 East drop target: {guide:?}"
    );

    set_last_pointer_global(dx, dy);
    assert!(drag_2d_end_global());
    dispatch_press_up(hit_test(root, dx, dy), dx, dy);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(1000.0, 600.0));

    let p1_y = draw_tree(root)
        .iter()
        .find_map(|c| match c {
            DrawCmd::Text { text, y, .. } if text == "P1" => Some(*y),
            _ => None,
        })
        .expect("拆分后 P1 标签仍应绘制");
    assert!(p1_y > 420.0, "底部区内原地拆分后标签仍应在下方: {p1_y}");
}
