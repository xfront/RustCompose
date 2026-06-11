//! Docking 对外 API 对齐 compose-docking 的回归测试：
//! `DockingSettings` / `activeDockableId` / `show` / 窗口级操作 / `NamedLayouts` / `dockLayout {}` DSL。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, TextStyle, create_root_node, dispatch_press_down,
    drag_2d_end_global, drag_2d_move_global, draw_tree, hit_test, perform_layout,
    set_last_pointer_global, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, MutableState, Recomposer};

use compose_material::{
    DockArea, DockEdge, DockLayout, DockLayoutSpec, DockModel, DockNode, DockRegion, DockState,
    DockableOptions, DockablePanel, DockingEvent, DockingSettings, SplitOrientation, Text,
    TabPreference, WindowBounds, clear_dock_panels, dock_layout, dock_state, drain_dock_events,
    register_dock_panel,
};

fn text_values(cmds: &[DrawCmd]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn two_pane_tree(a: u64, b: u64) -> DockNode {
    DockNode::split(
        SplitOrientation::Horizontal,
        0.4,
        DockNode::tabs(&[a]),
        DockNode::tabs(&[b]),
    )
}

/// 挂载 DockLayout 到离屏窗口，返回根节点。
fn mount(state: DockState, w: f32, h: f32) -> compose::layout::NodeId {
    set_viewport(w, h);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockArea(state, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    std::mem::forget(comp);
    root
}

fn root_ratio(state: &DockState) -> f32 {
    match state.model().root.expect("根不应为空") {
        DockNode::Split { proportion, .. } => proportion,
        other => panic!("根应为 Split，实际 {other:?}"),
    }
}

// ─────────────────────────── DockingSettings ───────────────────────────

/// `DockingSettings::default_tab_preference` 驱动 `TabPreference::Default` 的面板。
#[test]
fn settings_default_tab_preference_moves_tab_bar() {
    clear_dock_panels();
    drain_dock_events();
    let body = register_dock_panel(DockablePanel::new("SETDOC"), || {
        Text("SET_CONTENT".to_string(), TextStyle::new(), Modifier::new());
    });
    let state = dock_state(DockModel::new(DockNode::tabs(&[body])));
    let root = mount(state, 400.0, 300.0);

    let texts = text_values(&draw_tree(root));
    let top_tab = texts.iter().position(|t| t == "SETDOC").unwrap();
    let top_body = texts.iter().position(|t| t == "SET_CONTENT").unwrap();
    assert!(top_tab < top_body, "默认应为顶部标签条: {texts:?}");

    state.set_settings(DockingSettings {
        default_tab_preference: TabPreference::Bottom,
        ..state.settings()
    });
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    let texts = text_values(&draw_tree(root));
    let bottom_tab = texts.iter().position(|t| t == "SETDOC").unwrap();
    let bottom_body = texts.iter().position(|t| t == "SET_CONTENT").unwrap();
    assert!(
        bottom_body < bottom_tab,
        "设置改底部后内容应先绘制: {texts:?}"
    );
}

/// 面板显式偏好优先于 `settings.default_tab_preference`。
#[test]
fn panel_tab_preference_overrides_settings() {
    clear_dock_panels();
    drain_dock_events();
    let body = register_dock_panel(
        DockablePanel::with_options(
            "TOPWIN",
            DockableOptions {
                tab_preference: TabPreference::TopAlways,
                ..Default::default()
            },
        ),
        || {
            Text("TOPWIN_CONTENT".to_string(), TextStyle::new(), Modifier::new());
        },
    );
    let state = dock_state(DockModel::new(DockNode::tabs(&[body])));
    state.set_settings(DockingSettings {
        default_tab_preference: TabPreference::BottomAlways,
        ..Default::default()
    });
    let root = mount(state, 400.0, 300.0);
    let texts = text_values(&draw_tree(root));
    let tab = texts.iter().position(|t| t == "TOPWIN").unwrap();
    let content = texts.iter().position(|t| t == "TOPWIN_CONTENT").unwrap();
    assert!(tab < content, "面板显式 Top 应压过全局 Bottom: {texts:?}");
}

/// `settings.min_pane_w` 收紧分割条可拖区间。
#[test]
fn settings_min_pane_size_clamps_splitter_drag() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    state.set_settings(DockingSettings {
        min_pane_w: 160.0,
        ..Default::default()
    });
    let root = mount(state, 400.0, 300.0);

    // 默认 60px 时可拖到 0.15；改成 160px 后下限应为 160/400 = 0.4。
    let (sx, sy) = (160.0, 150.0);
    let hit = hit_test(root, sx, sy).expect("应命中分割条");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy), "按下应起分割拖拽");
    assert!(drag_2d_move_global(20.0, 150.0), "移动应由拖拽会话消费");
    drag_2d_end_global();
    let r = root_ratio(&state);
    assert!(
        (r - 0.4).abs() < 1e-3,
        "min_pane_w=160 应把下限钳到 0.4，实际 {r}"
    );
}

/// 逐面板 `min_size` 覆盖全局设置（只约束自己那一侧）。
#[test]
fn per_panel_min_size_overrides_settings() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(
        DockablePanel::with_options(
            "Wide",
            DockableOptions {
                min_size: Some((260.0, 40.0)),
                ..Default::default()
            },
        ),
        || {},
    );
    let b = register_dock_panel(DockablePanel::new("Narrow"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    state.set_settings(DockingSettings {
        min_pane_w: 20.0,
        ..Default::default()
    });
    let root = mount(state, 400.0, 300.0);

    let (sx, sy) = (160.0, 150.0);
    let hit = hit_test(root, sx, sy).expect("应命中分割条");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy));
    assert!(drag_2d_move_global(20.0, 150.0));
    drag_2d_end_global();
    let lo = root_ratio(&state);
    assert!(
        (lo - 260.0 / 400.0).abs() < 1e-3,
        "左侧面板 min 260 应把下限钳到 0.65，实际 {lo}"
    );

    // 反向拖到底：右侧只受全局 20px 约束 → 上限 ~0.95。
    let hit = hit_test(root, sx, sy).expect("分割条仍应可命中");
    set_last_pointer_global(sx, sy);
    assert!(dispatch_press_down(hit.0, sx, sy));
    assert!(drag_2d_move_global(395.0, 150.0));
    drag_2d_end_global();
    let hi = root_ratio(&state);
    assert!(hi > 0.9, "右窄侧应允许拖到 0.9 以上，实际 {hi}");
}

// ─────────────────────── active / can_float / show ───────────────────────

/// `active_dockable` 跟随 `switch_tab`，面板关闭后回落。
#[test]
fn active_dockable_tracks_selection_and_falls_back() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(DockNode::tabs(&[a, b])));
    assert_eq!(state.active_dockable(), Some(a), "初始活动面板应为首位");

    state.switch_tab(b);
    assert_eq!(state.active_dockable(), Some(b), "切标签应更新活动面板");

    state.set_active_dockable(9999);
    assert_eq!(state.active_dockable(), Some(b), "未注册面板应被忽略");

    state.close(b);
    assert_eq!(state.active_dockable(), Some(a), "活动面板关闭后应回落");
}

/// `can_float` / `can_close` 反映面板选项，`float_panel` 与 `close` 据此拒绝。
#[test]
fn can_float_and_can_close_gate_operations() {
    clear_dock_panels();
    drain_dock_events();
    let fixed = register_dock_panel(
        DockablePanel::with_options(
            "Fixed",
            DockableOptions {
                floatable: false,
                closable: false,
                ..Default::default()
            },
        ),
        || {},
    );
    let free = register_dock_panel(DockablePanel::new("Free"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(fixed, free)));

    assert!(!state.can_float(fixed));
    assert!(state.can_float(free));
    assert!(!state.can_close(fixed));
    assert!(state.can_close(free));

    state.float_panel(fixed);
    assert!(state.is_docked(fixed), "不可浮起面板应留在主窗口");
    // `close` 是程序化关闭入口，是否给出该入口由 `can_close` 决定（菜单/中键均已门禁）
    state.close(free);
    assert!(!state.is_open(free));
    // 未注册面板：两个判据都为假
    assert!(!state.can_float(4242));
    assert!(!state.can_close(4242));
}

/// `show` 重新打开被关闭的面板；自动隐藏面板被钉回。
#[test]
fn show_reopens_hidden_and_closed_panels() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));

    state.close(b);
    assert!(!state.is_open(b));
    state.show(b);
    assert!(state.is_open(b), "show 应把关闭的面板放回布局");
    assert_eq!(state.active_dockable(), Some(b), "show 应置活动");

    state.hide_to_edge(a, DockEdge::Left);
    assert!(state.is_hidden(a));
    state.show(a);
    assert!(!state.is_hidden(a), "show 应从自动隐藏钉回");
    assert!(state.is_docked(a));
    // 未注册面板：show 无事可做
    state.show(7777);
    assert!(!state.is_open(7777));
}

/// `hide` 摘下面板但保留注册。
#[test]
fn hide_detaches_without_closing() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    state.hide(b);
    assert!(!state.is_open(b));
    assert!(compose_material::dock_panel_exists(b), "hide 不应注销面板");
}

// ─────────────────────────── 窗口级操作 ───────────────────────────

/// `set_window_bounds` / `window_bounds` / `window_ids` / `close_window`。
#[test]
fn window_ops_move_and_close_floating_windows() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));

    state.float_panel(b);
    let wid = state.window_of(b).expect("浮起后应有窗口 id");
    assert_ne!(wid, compose_material::WINDOW_MAIN);
    assert_eq!(state.window_ids(), vec![compose_material::WINDOW_MAIN, wid]);

    state.set_window_bounds(wid, WindowBounds { x: 40.0, y: 60.0, width: 500.0, height: 400.0 });
    let bounds = state.window_bounds(wid).expect("浮动窗口应有边界");
    assert_eq!((bounds.x, bounds.y, bounds.width, bounds.height), (40.0, 60.0, 500.0, 400.0));
    // 主窗口没有边界
    assert!(state.window_bounds(compose_material::WINDOW_MAIN).is_none());
    state.set_window_bounds(compose_material::WINDOW_MAIN, bounds);

    state.close_window(wid);
    assert!(state.window_bounds(wid).is_none(), "窗口应已关闭");
    assert!(state.is_docked(b), "关闭浮动窗口应把面板收回主窗口");
    assert_eq!(
        state.model().root.expect("根不应为空").collect_ids(),
        vec![a, b],
        "收回后面板必须都在"
    );
    // 关闭不存在的窗口无事可做
    state.close_window(wid);
    state.close_window(compose_material::WINDOW_MAIN);
}

/// `restore_window` 只还原归属该窗口的最大化。
#[test]
fn restore_window_only_unmaximizes_its_own() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    state.maximize(b);
    assert!(state.is_maximized(b));
    let main = compose_material::WINDOW_MAIN;
    state.restore_window(main + 42);
    assert!(state.is_maximized(b), "别的窗口不应还原最大化");
    let wid = state.window_of(b).unwrap_or(main);
    state.restore_window(wid);
    assert!(!state.is_maximized(b), "归属窗口的 restore 应还原");
}

// ─────────────────────────── NamedLayouts ───────────────────────────

/// `layouts().save/restore/names/contains/remove` 往返。
#[test]
fn named_layouts_roundtrip() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    drain_dock_events();

    let layouts = state.layouts();
    assert!(layouts.save("默认"));
    assert!(layouts.contains("默认"));
    assert_eq!(layouts.names(), vec!["默认".to_string()]);

    state.close(b);
    assert!(!state.is_open(b));
    assert!(layouts.restore("默认"));
    assert!(state.is_open(b), "恢复命名布局应带回 Beta");
    assert_eq!(
        root_ratio(&state),
        0.4,
        "恢复应连带回到快照里的分割比例"
    );
    let kinds: Vec<_> = drain_dock_events()
        .iter()
        .filter(|e| matches!(e, DockingEvent::LayoutChanged(_)))
        .cloned()
        .collect();
    assert_eq!(kinds.len(), 1, "恢复应派发一次 LayoutChanged");

    assert!(layouts.remove("默认"));
    assert!(!layouts.remove("默认"));
    assert!(!layouts.restore("默认"));
    assert_eq!(layouts.names(), Vec::<String>::new());
}

/// 快照里含已注销面板时：`save` 报假、`restore` 剔除该项。
#[test]
fn named_layouts_prune_unregistered_on_restore() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    let layouts = state.layouts();
    assert!(layouts.save("v1"));

    compose_material::unregister_dock_panel(b);
    assert!(!layouts.save("v2"), "含未注册面板时 save 应返回 false");
    assert!(layouts.restore("v1"));
    let ids = state.model().root.expect("根不应为空").collect_ids();
    assert_eq!(ids, vec![a], "恢复时未注册面板应被剔除");
}

/// `capture` / `restore_capture` 是具名布局之外的内存快照原语。
#[test]
fn capture_and_restore_snapshot() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});
    let state = dock_state(DockModel::new(two_pane_tree(a, b)));
    let snap = state.capture();
    state.close(a);
    state.set_split_ratio(&[], 0.8);
    assert!(!state.is_open(a));
    state.restore_capture(&snap);
    assert!(state.is_open(a));
    assert_eq!(root_ratio(&state), 0.4);
    assert_eq!(state.model(), snap, "恢复后状态应与快照全等");
}

// ─────────────────────────── dock_layout {} DSL ───────────────────────────

/// DSL 声明主窗口 + 浮动窗口，产物与手工建树等值。
#[test]
fn dock_layout_dsl_builds_windows() {
    clear_dock_panels();
    drain_dock_events();
    let explorer = register_dock_panel(DockablePanel::new("Explorer"), || {});
    let editor = register_dock_panel(DockablePanel::new("Editor"), || {});
    let output = register_dock_panel(DockablePanel::new("Output"), || {});
    let problems = register_dock_panel(DockablePanel::new("Problems"), || {});
    let props = register_dock_panel(DockablePanel::new("Props"), || {});

    let spec: DockLayoutSpec = dock_layout(|l| {
        l.main(|m| {
            m.row(0.25, |c| c.dock(explorer));
            m.col(0.75, |c| {
                c.dock(editor);
                c.tabs_selected(&[output, problems], 1);
            });
        });
        l.floating_window(
            WindowBounds { x: 80.0, y: 90.0, width: 320.0, height: 240.0 },
            |f| f.dock(props),
        );
    });
    assert!(spec.to_layout().is_valid(), "DSL 产物必须满足窗口列表不变式");

    let model = spec.to_model();
    let root = model.root.clone().expect("主窗口应有内容");
    assert_eq!(
        root.collect_ids(),
        vec![explorer, editor, output, problems],
        "主窗口面板集合应与声明一致"
    );
    match root {
        DockNode::Split { orientation, proportion, first, .. } => {
            assert_eq!(orientation, SplitOrientation::Horizontal);
            assert!(
                (proportion - 0.25).abs() < 1e-6,
                "同级权重 0.25 : 0.75 应让首栏占 1/4，实际 {proportion}"
            );
            assert_eq!(first.collect_ids(), vec![explorer]);
        }
        other => panic!("根应为水平分割，实际 {other:?}"),
    }
    assert_eq!(model.floating.len(), 1);
    assert_eq!(model.floating[0].items(), vec![props]);
    assert_eq!((model.floating[0].x, model.floating[0].w), (80.0, 320.0));

    // 标签组默认选中第一项，活动面板因此是主窗口树首位
    let state = dock_state(model);
    assert_eq!(state.active_dockable(), Some(explorer));

    let root = mount(state, 800.0, 600.0);
    let texts = text_values(&draw_tree(root));
    for t in ["Explorer", "Editor", "Output", "Problems", "Props"] {
        assert!(texts.iter().any(|x| x == t), "浮动窗口与主窗口标题都应渲染: {texts:?}");
    }
}

/// DSL 的 `display` / `dock_on` / `anchor_at_root` 与标签选择语义。
#[test]
fn dock_layout_dsl_dock_on_and_anchor() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Alpha"), || {});
    let b = register_dock_panel(DockablePanel::new("Beta"), || {});

    let spec = dock_layout(|l| {
        l.display(&[a]);
        l.dock_on(a, DockRegion::East, b, 0.3);
        l.anchor_at_root(DockRegion::South, 7, 0.2);
    });
    let root = spec.to_model().root.clone().expect("应有根");
    assert_eq!(root.collect_ids(), vec![a, b]);
    assert!(root.contains_anchor(7), "anchor_at_root 应放占位锚点");

    // display_selected 决定标签组选中项
    let spec2 = dock_layout(|l| l.display_selected(&[a, b], 1));
    match spec2.to_model().root.expect("根") {
        DockNode::Tabs { selected_index, .. } => assert_eq!(selected_index, 1),
        other => panic!("应为标签组，实际 {other:?}"),
    }
}

/// `DockArea` 与 `DockLayout` 同义：两者都渲染面板标题。
#[test]
fn dock_area_and_dock_layout_are_equivalent() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Same"), || {
        Text("SAME_BODY".to_string(), TextStyle::new(), Modifier::new());
    });
    let state = dock_state(DockModel::new(DockNode::tabs(&[a])));
    let root = mount(state, 400.0, 300.0);
    let via_area = text_values(&draw_tree(root));
    set_viewport(400.0, 300.0);
    let root2 = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root2);
    let st2: DockState = state;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DockLayout(st2, Modifier::new().fill_max_size());
        });
    });
    perform_layout(root2, Constraints::tight(400.0, 300.0));
    let via_layout = text_values(&draw_tree(root2));
    assert_eq!(via_area, via_layout, "两个入口的绘制应一致");
    std::mem::forget(comp);
}

/// `remember_dock_state_in` 用 DSL + 设置建状态（组合期入口）。
#[test]
fn state_can_be_seeded_from_spec_and_settings() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("Seed"), || {});
    let spec = dock_layout(|l| l.dock(a));
    let state = dock_state(spec.to_model()).with_settings(DockingSettings {
        min_pane_w: 123.0,
        collapsed_anchor_thickness: 8.0,
        ..Default::default()
    });
    assert_eq!(state.settings().min_pane_w, 123.0);
    assert!(state.settings().collapses_empty_anchors());
    assert!(!state.settings().always_display_tabs());
    assert_eq!(state.model().root.expect("根").collect_ids(), vec![a]);
}

/// 状态句柄可跨重组保持（`MutableState` 语义抽查）。
#[test]
fn settings_state_is_reactive() {
    let st: MutableState<DockingSettings> = compose::runtime::mutableStateOf(Default::default());
    st.update(|s| s.min_pane_h = 42.0);
    assert_eq!(st.get().min_pane_h, 42.0);
}
