//! Docking Batch 2 对齐 compose-docking 的回归测试：
//! `DockingTheme` / `LocalDockingTheme`、`LayoutStorage` + `rememberAutoPersist`、
//! `DockCapabilities`、`DockableResolver`、`mergeFloatingIntoMain`。

use std::path::PathBuf;

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, DrawCmd as DC, Modifier, TextStyle, create_root_node, draw_tree,
    perform_layout, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, advance_frame, frame_driver_count};

use compose_material::{
    DOCK_GUIDE_PAD, DOCK_HEADER_H, DOCK_SPLITTER_PX, DockArea, DockCapabilities, DockModel,
    DockNode, DockState, DockableOptions, DockablePanel, DockingEvent, DockingSettings,
    DockingTheme, FileLayoutStorage, LayoutStorage, MemoryLayoutStorage, ProvideDockingTheme,
    SplitOrientation, Text, clear_dock_panels, decode_named_layouts, docking_theme, dock_layout,
    dock_split_bounds, dock_state, drain_dock_events, encode_named_layouts, register_dock_panel,
    remember_auto_persist, unregister_dock_panel,
};

fn mount(state: DockState, w: f32, h: f32) -> compose::layout::NodeId {
    mount_with(w, h, move || {
        DockArea(state, Modifier::new().fill_max_size());
    })
}

/// 用任意组合内容挂载（主题 Provider / 自动持久化需要在组合期调用）。
fn mount_with(w: f32, h: f32, content: impl Fn() + 'static) -> compose::layout::NodeId {
    set_viewport(w, h);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    // MaterialTheme 按值消费 content；用 Rc 克隆让外层闭包仍满足 Fn。
    let content = std::rc::Rc::new(content);
    comp.set_content(move || {
        let c = content.clone();
        MaterialTheme(ColorScheme::light(), move || c())
    });
    perform_layout(root, Constraints::tight(w, h));
    std::mem::forget(comp);
    root
}

fn two_pane(a: u64, b: u64) -> DockNode {
    DockNode::split(
        SplitOrientation::Horizontal,
        0.4,
        DockNode::tabs(&[a]),
        DockNode::tabs(&[b]),
    )
}

fn rects(cmds: &[DrawCmd]) -> Vec<(f32, f32, f32, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            DC::FillRect { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect()
}

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("compose-rs_dock_{tag}_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&p);
    p
}

// ─────────────────────────── DockingTheme ───────────────────────────

/// 默认主题的每个度量都等于旧的公开常量——接线到主题后不应有任何视觉回归。
#[test]
fn theme_defaults_match_legacy_constants() {
    let t = DockingTheme::default();
    assert_eq!(t.header_height, DOCK_HEADER_H);
    assert_eq!(t.splitter_width, DOCK_SPLITTER_PX);
    assert!(t.accent.is_none() && t.accent_container.is_none());
    assert_eq!(t, DockingTheme::standard());
    assert_eq!(docking_theme(), DockingTheme::standard());
}

/// 主题度量真的被命中/几何函数读取（不只是存着）。
#[test]
fn theme_metrics_drive_geometry_helpers() {
    let th = DockingTheme::standard()
        .with_splitter_width(8.0)
        .with_guides(20.0, 2.0, 9.0);
    ProvideDockingTheme(th, || {});
    assert_eq!(docking_theme().splitter_width, 8.0);

    // 分割条宽度进入子区边界：100 宽、比例 0.5 ⇒ 两侧各 (100-8)/2 = 46。
    let (a, b) = dock_split_bounds(
        SplitOrientation::Horizontal,
        0.5,
        (0.0, 0.0, 100.0, 50.0),
    );
    assert_eq!(a.2, 46.0);
    assert_eq!(b.0, 54.0);

    // 指南尺寸与容差。
    let guides = compose_material::dock_inner_guides((0.0, 0.0, 400.0, 300.0));
    let center = guides
        .iter()
        .find(|(d, _)| *d == compose_material::DockDirection::Center)
        .unwrap()
        .1;
    assert_eq!(center.2, 20.0);
    // 命中容差 9：在中心格右侧 20/2+9 = 19 处仍算命中，20 处不算。
    assert!(
        compose_material::dock_guide_hit(&guides, 200.0 + 18.0, 150.0).is_some(),
        "容差应放大命中区"
    );
    // 200+25 虽超出中心格容差，却落进了相邻 East 格的命中区；取更远的点验证失配。
    assert!(
        compose_material::dock_guide_hit(&guides, 200.0 + 60.0, 150.0).is_none(),
        "超出容差应失配"
    );
    assert!(DOCK_GUIDE_PAD > 0.0);
}

/// 紧凑主题把头高落到实际绘制的标签条矩形上。
#[test]
fn theme_header_height_changes_drawn_bar() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("HEADDOC"), || {});
    let state = dock_state(DockModel::new(DockNode::tabs(&[a])));

    let root = mount_with(400.0, 300.0, move || {
        ProvideDockingTheme(DockingTheme::compact(), move || {
            DockArea(state, Modifier::new().fill_max_size());
        });
    });
    let hs: Vec<f32> = rects(&draw_tree(root))
        .into_iter()
        .filter(|(_, _, w, _)| *w >= 399.0)
        .map(|(_, _, _, h)| h)
        .collect();
    assert!(
        hs.iter().any(|h| *h == 26.0),
        "compact 主题应把整宽头栏画成 26 高: {hs:?}"
    );

    // 默认主题同场景应回到 30 高。Provider 设置的是全局主题镜像，需显式还原。
    ProvideDockingTheme(DockingTheme::standard(), || {});
    clear_dock_panels();
    drain_dock_events();
    let b = register_dock_panel(DockablePanel::new("STDDOC"), || {});
    let st2 = dock_state(DockModel::new(DockNode::tabs(&[b])));
    let root2 = mount(st2, 400.0, 300.0);
    let hs2: Vec<f32> = rects(&draw_tree(root2))
        .into_iter()
        .filter(|(_, _, w, _)| *w >= 399.0)
        .map(|(_, _, _, h)| h)
        .collect();
    assert!(
        hs2.iter().any(|h| *h == DOCK_HEADER_H),
        "默认主题头栏仍是 30 高: {hs2:?}"
    );
}

// ─────────────────────────── LayoutStorage ───────────────────────────

/// 内存存储：保存 → 新状态恢复，比例与浮动窗都回来。
#[test]
fn memory_storage_roundtrip() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("M-A"), || {});
    let b = register_dock_panel(DockablePanel::new("M-B"), || {});
    MemoryLayoutStorage::clear();

    let state = dock_state(DockModel::new(two_pane(a, b)));
    state.float_panel(b);
    state.save_layout_to(MemoryLayoutStorage::shared(), "main");
    let saved = state.model().clone();

    let other = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(other.restore_layout_from(MemoryLayoutStorage::shared(), "main"));
    let got = other.model();
    assert_eq!(got.floating.len(), saved.floating.len());
    assert!(!got.floating.is_empty(), "浮动窗应随存储恢复");
    assert!(got.root.is_some());

    // 事件：Persisted + Restored 各一次。
    let kinds: Vec<DockingEvent> = drain_dock_events();
    assert!(
        kinds
            .iter()
            .any(|e| matches!(e, DockingEvent::LayoutChanged(k) if format!("{k:?}") == *"Persisted")),
        "保存应发 Persisted: {kinds:?}"
    );
    assert!(
        kinds
            .iter()
            .any(|e| matches!(e, DockingEvent::LayoutChanged(k) if format!("{k:?}") == *"Restored")),
        "恢复应发 Restored: {kinds:?}"
    );
}

/// 不存在的键：恢复返回 `false` 且状态不动。
#[test]
fn storage_missing_key_is_noop() {
    clear_dock_panels();
    MemoryLayoutStorage::clear();
    let a = register_dock_panel(DockablePanel::new("K-A"), || {});
    let state = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(!state.restore_layout_from(MemoryLayoutStorage::shared(), "nope"));
    assert_eq!(state.model().root.unwrap().collect_ids().len(), 1);
}

/// 文件存储：往返 + 键名净化（用户输入不能越出目录）。
#[test]
fn file_storage_roundtrip_and_key_sanitizing() {
    clear_dock_panels();
    let dir = tmp_dir("file");
    let store = FileLayoutStorage::new(dir.clone());
    let a = register_dock_panel(DockablePanel::new("F-A"), || {});
    let b = register_dock_panel(DockablePanel::new("F-B"), || {});
    let state = dock_state(DockModel::new(two_pane(a, b)));

    let evil = "../../evil/键 名";
    state.save_layout_to(&store, evil);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(names.len(), 1, "只应在本目录写一个文件: {names:?}");
    assert!(
        !names[0].contains('/')
            && !names[0].contains('\\')
            && !names[0].contains(".dl.dl"),
        "文件名应已净化: {:?}",
        names[0]
    );
    assert_eq!(
        store.load_layout(evil).as_deref(),
        Some(state.save_full_state().as_str())
    );

    let other = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(other.restore_layout_from(&store, evil));
    assert_eq!(other.model().root.unwrap().collect_ids().len(), 2);
    store.remove_layout(evil);
    assert_eq!(store.load_layout(evil), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 命名集合的文本编码：正文含换行、名字含非 ASCII 都要能原样回来。
#[test]
fn named_layouts_text_codec() {
    let mut map = std::collections::HashMap::new();
    map.insert("默认 布局".to_string(), "mdl2\ntree=S|1,2\nmax=-\n".to_string());
    map.insert("two".to_string(), "mdl2\ntree=S|3\nfloat=7;0,0,10,10;-;-;0.3;L|4\n".to_string());
    let text = encode_named_layouts(&map);
    let back = decode_named_layouts(&text).expect("应能解码");
    assert_eq!(back, map);
    assert!(decode_named_layouts("bogus\n").is_none());
}

/// 命名布局集合写入存储后，另一个状态能读回并应用。
#[test]
fn named_layouts_persist_roundtrip() {
    clear_dock_panels();
    MemoryLayoutStorage::clear();
    let a = register_dock_panel(DockablePanel::new("N-A"), || {});
    let b = register_dock_panel(DockablePanel::new("N-B"), || {});
    let state = dock_state(DockModel::new(two_pane(a, b)));
    assert!(state.layouts().save("two-pane"));
    state.save_named_layouts_to(MemoryLayoutStorage::shared(), "group");

    let other = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(other.restore_named_layouts_from(
        MemoryLayoutStorage::shared(),
        "group"
    ));
    assert!(other.layouts().contains("two-pane"));
    assert!(other.layouts().restore("two-pane"));
    match other.model().root.unwrap() {
        DockNode::Split { proportion, .. } => assert!((proportion - 0.4).abs() < 1e-3),
        other => panic!("应恢复成左右分割: {other:?}"),
    }
    // 集合里不存在的键读取即 false。
    assert!(!other.restore_named_layouts_from(MemoryLayoutStorage::shared(), "none"));
}

// ─────────────────────────── 自动持久化 ───────────────────────────

/// 首次组合只记基线；布局变化后（超过最小间隔）才写盘，且不注册帧驱动。
#[test]
fn auto_persist_seeds_then_writes_on_change() {
    clear_dock_panels();
    drain_dock_events();
    MemoryLayoutStorage::clear();
    let a = register_dock_panel(DockablePanel::new("P-A"), || {});
    let b = register_dock_panel(DockablePanel::new("P-B"), || {});
    let state = dock_state(DockModel::new(two_pane(a, b)));

    let root = mount_with(600.0, 400.0, move || {
        remember_auto_persist(state, MemoryLayoutStorage::shared(), "auto", 1);
        DockArea(state, Modifier::new().fill_max_size());
    });
    let store = MemoryLayoutStorage::shared();
    assert_eq!(
        store.load_layout("auto"),
        None,
        "首趟组合不该把未恢复的初始布局覆盖进存储"
    );
    // DockArea 自身会注册少量基础驱动，只看 auto_persist 引入的增量。
    let base_drivers = frame_driver_count();

    // 布局变化触发重组 → 到点路径写盘（间隔 1ms 下可能先挂一条驱动，下一帧到点补写）。
    state.float_panel(b);
    advance_frame(1.0 / 60.0);
    advance_frame(1.0);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(600.0, 400.0));
    let text = store.load_layout("auto").expect("变化后应写盘");
    assert!(text.contains("float="), "写入的应是完整状态: {text}");
    assert_eq!(
        frame_driver_count(),
        base_drivers,
        "写盘后不该留下常驻驱动"
    );
}

/// 未到最小间隔时不写盘，改为登记一条定点唤醒驱动。
#[test]
fn auto_persist_debounces_via_timed_wake() {
    clear_dock_panels();
    MemoryLayoutStorage::clear();
    let a = register_dock_panel(DockablePanel::new("D-A"), || {});
    let b = register_dock_panel(DockablePanel::new("D-B"), || {});
    let state = dock_state(DockModel::new(two_pane(a, b)));
    mount_with(600.0, 400.0, move || {
        remember_auto_persist(state, MemoryLayoutStorage::shared(), "deb", 600_000);
        DockArea(state, Modifier::new().fill_max_size());
    });
    let base_drivers = frame_driver_count();
    state.float_panel(b);
    advance_frame(1.0 / 60.0);
    Recomposer::recompose_pending();
    assert_eq!(
        MemoryLayoutStorage::shared().load_layout("deb"),
        None,
        "间隔未到不该写盘"
    );
    assert_eq!(
        frame_driver_count(),
        base_drivers + 1,
        "待写入应在基线上多挂一条定点唤醒驱动"
    );
}

// ─────────────────────────── 能力与解析器 ───────────────────────────

/// `DockCapabilities` 汇总注册表约束；未注册面板一律禁止。
#[test]
fn capabilities_mirror_options() {
    clear_dock_panels();
    let open = register_dock_panel(DockablePanel::new("C-OPEN"), || {});
    let fixed = register_dock_panel(
        DockablePanel {
            auto_hide_allowed: false,
            min_max_allowed: false,
            ..DockablePanel::with_options(
                "C-FIXED",
                DockableOptions {
                    closable: false,
                    floatable: false,
                    maximizable: false,
                    ..Default::default()
                },
            )
        },
        || {},
    );
    fixed_info(&fixed);
    let state = dock_state(DockModel::new(two_pane(open, fixed)));
    assert_eq!(state.capabilities(open), DockCapabilities::all());
    let caps = state.capabilities(fixed);
    assert!(
        !caps.can_close && !caps.can_float && !caps.can_maximize,
        "固定面板应只剩可停靠: {caps:?}"
    );
    assert!(caps.can_dock);
    assert_eq!(state.capabilities(0xD0C0_FFFF), DockCapabilities::none());
}

fn fixed_info(id: &u64) {
    // 断言注册表里确实带上了 options（避免测试因默认值而假绿）。
    let info = compose_material::dock_panel_info(*id).expect("已注册");
    assert!(!info.options.closable && !info.options.floatable);
    assert!(!info.auto_hide_allowed && !info.min_max_allowed);
}

/// 浮动窗合回主窗口（KT `mergeFloatingIntoMain`）。
#[test]
fn merge_floating_into_main_returns_panels() {
    clear_dock_panels();
    let a = register_dock_panel(DockablePanel::new("G-A"), || {});
    let b = register_dock_panel(DockablePanel::new("G-B"), || {});
    let state = dock_state(DockModel::new(two_pane(a, b)));
    state.float_panel(b);
    assert_eq!(state.model().floating.len(), 1);
    state.merge_floating_into_main();
    let m = state.model();
    assert!(m.floating.is_empty(), "合回后不该留浮动窗");
    assert_eq!(m.root.unwrap().collect_ids().len(), 2, "两个面板都应回到主窗");
}

/// 解析器把失配的面板 id 映射到新的注册项，并丢弃无法解析的叶子。
#[test]
fn resolver_remaps_ids_on_restore() {
    clear_dock_panels();
    let a = register_dock_panel(DockablePanel::new("R-A"), || {});
    let b = register_dock_panel(DockablePanel::new("R-B"), || {});
    let state = dock_state(DockModel::new(two_pane(a, b)));
    let text = state.save_full_state();

    // 模拟「升级后 R-B 这个面板没了，改由 R-C 顶上」：注销 B、注册 C。
    unregister_dock_panel(b);
    let c = register_dock_panel(DockablePanel::new("R-C"), || {});

    let plain = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(plain.restore_full_state(&text));
    let ids = plain.model().root.unwrap().collect_ids();
    assert!(!ids.contains(&b), "无解析器时未注册面板被剪掉");
    assert_eq!(ids.len(), 1, "只剩 A: {ids:?}");

    let mapped = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(mapped.restore_full_state_with(&text, &mut |id| if id == b { Some(c) } else { Some(id) }));
    let ids = mapped.model().root.unwrap().collect_ids();
    assert_eq!(ids.len(), 2, "映射后应回到两栏: {ids:?}");
    assert!(ids.contains(&c), "B 应被替换成 C: {ids:?}");

    // 解析成 None 即丢弃，容器随之收敛成单叶。
    let dropped = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(dropped.restore_full_state_with(&text, &mut |id| if id == b { None } else { Some(id) }));
    assert_eq!(dropped.model().root.unwrap().collect_ids().len(), 1);
}

/// 同一批 id 被映射到同一个面板时只保留一次（不恢复出重复标签）。
#[test]
fn resolver_dedupes_collisions() {
    clear_dock_panels();
    let a = register_dock_panel(DockablePanel::new("U-A"), || {});
    let b = register_dock_panel(DockablePanel::new("U-B"), || {});
    let text = dock_state(DockModel::new(two_pane(a, b))).save_full_state();
    let merged = dock_state(DockModel::new(DockNode::tabs(&[a])));
    assert!(merged.restore_full_state_with(&text, &mut |_| Some(a)));
    let ids = merged.model().root.unwrap().collect_ids();
    assert_eq!(ids, vec![a], "两侧都映射到 A 时应收敛成一个: {ids:?}");
}

// ─────────────────────────── 设置与主题共存 ───────────────────────────

/// `dock_layout {}` DSL + `DockingSettings` + 主题可叠加使用（槽位稳定）。
#[test]
fn dsl_settings_and_theme_compose_together() {
    clear_dock_panels();
    drain_dock_events();
    let a = register_dock_panel(DockablePanel::new("S-A"), || {
        Text("S_A".to_string(), TextStyle::new(), Modifier::new());
    });
    let b = register_dock_panel(DockablePanel::new("S-B"), || {});
    let spec = dock_layout(|l| {
        // builder.dock() 语义为「单面板占满」，多次调用只有最后一个生效；
        // 多个面板并到主窗口应用 display()。
        l.display(&[a, b]);
    });
    let state = remember_spec_state(&spec);
    let root = mount_with(500.0, 300.0, move || {
        ProvideDockingTheme(DockingTheme::compact(), move || {
            DockArea(state, Modifier::new().fill_max_size());
        });
    });
    let texts = draw_tree(root)
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|t| t == "S-A"), "DSL 布局应挂上面板: {texts:?}");
    assert!(
        docking_theme().header_height < DOCK_HEADER_H,
        "主题 Provider 应已在组合期生效"
    );
}

/// 由 spec 播种状态（`remember_dock_state_in` 的等价入口）。
fn remember_spec_state(spec: &DockLayoutSpecRef) -> DockState {
    dock_state(spec.to_model())
}

/// 测试侧的 spec 引用别名（避免把内部类型写死）。
pub type DockLayoutSpecRef = compose_material::DockLayoutSpec;

/// 设置里的最小尺寸在主题改变分割条宽度后仍成立。
#[test]
fn min_pane_still_clamps_with_custom_splitter() {
    clear_dock_panels();
    let a = register_dock_panel(DockablePanel::new("Z-A"), || {});
    let b = register_dock_panel(DockablePanel::new("Z-B"), || {});
    let state = dock_state(DockModel::new(two_pane(a, b)));
    state.set_settings(DockingSettings {
        min_pane_w: 200.0,
        ..state.settings()
    });
    ProvideDockingTheme(DockingTheme::standard().with_splitter_width(12.0), || {});
    mount(state, 600.0, 400.0);
    // 600 宽、两侧各需 200 ⇒ 比例区间 [0.344, 0.656]（扣掉 12 宽分割条）。
    let (lo, hi) = compose_material::dock_ratio_bounds_test(&state);
    assert!(lo > 0.33 && lo < 0.36, "下界应随分割条变宽上移: {lo}");
    assert!(hi > 0.64 && hi < 0.67, "上界对应变窄: {hi}");
}
