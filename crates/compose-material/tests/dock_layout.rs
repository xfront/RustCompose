//! 停靠布局模型对齐回归 —— 对齐 compose-docking 的 `model/DockLayout.kt`。
//!
//! 覆盖：KT 不变式（窗口列表非空 / 首个是主窗口 / 其余是浮动窗口 / id 不重复）、
//! 访问器（`mainWindow` / `floatingWindows` / `window` / `requireWindow` / `replaceWindow`）、
//! `MaximizedState` 的纯值还原语义、`dl1` 文本往返、以及运行时模型 ↔ 窗口布局的投影。

use compose_material::{
    DOCK_LAYOUT_FORMAT, DockLayout, DockModel, DockNode, DockWindow, MaximizedState, PanelId,
    SplitOrientation, WINDOW_MAIN, WindowBounds, WindowKind, dock_state, parse_dock_layout_text,
    serialize_dock_layout,
};

fn win(id: u32, kind: WindowKind, root: Option<DockNode>) -> DockWindow {
    DockWindow {
        id,
        kind,
        root,
        maximized: None,
        bounds: None,
    }
}

/// KT 不变式：空布局只有一个空主窗口。
#[test]
fn empty_layout_has_single_main_window() {
    let l = DockLayout::empty();
    assert_eq!(l.windows.len(), 1);
    assert_eq!(l.main_window().id, WINDOW_MAIN);
    assert_eq!(l.main_window().kind, WindowKind::Main);
    assert!(l.main_window().root.is_none());
    assert!(l.floating_windows().is_empty());
    assert!(l.is_valid());
}

/// KT 不变式：窗口列表非空 / 首个必须主窗口 / 其余只能浮动窗口 / id 不重复。
#[test]
fn layout_invariants_match_kotlin() {
    // 空窗口列表非法
    let empty = DockLayout { windows: Vec::new() };
    assert!(!empty.is_valid());

    // 首个不是主窗口非法
    let bad_first = DockLayout {
        windows: vec![win(1, WindowKind::Floating, None)],
    };
    assert!(!bad_first.is_valid());

    // 中途出现主窗口非法
    let bad_mid = DockLayout {
        windows: vec![
            win(WINDOW_MAIN, WindowKind::Main, None),
            win(2, WindowKind::Floating, None),
            win(3, WindowKind::Main, None),
        ],
    };
    assert!(!bad_mid.is_valid());

    // 窗口 id 重复非法
    let dup = DockLayout {
        windows: vec![
            win(WINDOW_MAIN, WindowKind::Main, None),
            win(1, WindowKind::Floating, None),
            win(1, WindowKind::Floating, None),
        ],
    };
    assert!(!dup.is_valid());

    // 合法：主窗口 + 两个浮动窗口
    let ok = DockLayout {
        windows: vec![
            win(WINDOW_MAIN, WindowKind::Main, None),
            win(1, WindowKind::Floating, None),
            win(2, WindowKind::Floating, None),
        ],
    };
    assert!(ok.is_valid());
}

/// KT 访问器：window / requireWindow / replaceWindow / floatingWindows。
#[test]
fn layout_accessors_match_kotlin() {
    let l = DockLayout {
        windows: vec![
            win(WINDOW_MAIN, WindowKind::Main, None),
            {
                let mut w = win(7, WindowKind::Floating, None);
                w.bounds = Some(WindowBounds {
                    x: 10.0,
                    y: 20.0,
                    width: 300.0,
                    height: 200.0,
                });
                w
            },
        ],
    };
    assert_eq!(l.window(7).map(|w| w.kind), Some(WindowKind::Floating));
    assert!(l.window(99).is_none());
    assert_eq!(l.require_window(7).id, 7);
    assert_eq!(l.floating_windows().len(), 1);
    assert_eq!(l.floating_windows()[0].bounds.unwrap().width, 300.0);

    // replaceWindow：替换同 id 窗口，其余保持不变
    let mut replacement = win(7, WindowKind::Floating, None);
    replacement.bounds = Some(WindowBounds {
        x: 1.0,
        y: 2.0,
        width: 4.0,
        height: 5.0,
    });
    let next = l.replace_window(replacement);
    assert_eq!(next.window(7).unwrap().bounds.unwrap().x, 1.0);
    assert_eq!(next.windows.len(), l.windows.len());
    assert_eq!(next.main_window().id, WINDOW_MAIN);
}

/// `MaximizedState`：还原最大化是**纯值替换**（saved_root 原样换回）。
#[test]
fn maximized_state_restores_saved_root() {
    let saved = DockNode::tabs(&[42]);
    let l = DockLayout {
        windows: vec![DockWindow {
            id: WINDOW_MAIN,
            kind: WindowKind::Main,
            root: Some(DockNode::tabs(&[7])),
            maximized: Some(MaximizedState {
                dockable_id: 7,
                saved_root: Some(saved.clone()),
            }),
            bounds: None,
        }],
    };
    let mx = l.main_window().maximized.as_ref().unwrap();
    assert_eq!(mx.dockable_id, 7);
    assert_eq!(mx.saved_root.as_ref().unwrap(), &saved);
}

/// 运行时模型 → 窗口布局投影：主窗口 + 每个浮动组一个浮动窗口（带边界）。
#[test]
fn model_projects_to_window_layout() {
    let mut m = DockModel::new(DockNode::tabs(&[1, 2]));
    m.floating.push(compose_material::FloatingPanel::from_items(
        1,
        vec![3],
        3,
        10.0,
        20.0,
        300.0,
        200.0,
        None,
    ));
    let l = DockLayout::from_model(&m);
    assert!(l.is_valid());
    assert_eq!(l.windows.len(), 2);
    assert_eq!(l.main_window().kind, WindowKind::Main);
    assert_eq!(
        l.main_window().root.as_ref().unwrap().collect_ids(),
        vec![1, 2]
    );
    let f = &l.floating_windows()[0];
    assert_eq!(f.kind, WindowKind::Floating);
    assert_eq!(f.id, 1);
    let b = f.bounds.unwrap();
    assert_eq!((b.x, b.y, b.width, b.height), (10.0, 20.0, 300.0, 200.0));
    // 面板归属窗口
    assert_eq!(l.window_of(2), Some(WINDOW_MAIN));
    assert_eq!(l.window_of(3), Some(1));
    assert_eq!(l.window_of(99), None);
}

/// 窗口布局 → 运行时模型写回（主树 + 浮动组）。
#[test]
fn window_layout_applies_back_to_model() {
    let l = DockLayout {
        windows: vec![
            win(WINDOW_MAIN, WindowKind::Main, Some(DockNode::tabs(&[5, 6]))),
            {
                let mut w = win(2, WindowKind::Floating, Some(DockNode::tabs(&[9])));
                w.bounds = Some(WindowBounds {
                    x: 5.0,
                    y: 6.0,
                    width: 100.0,
                    height: 200.0,
                });
                w
            },
        ],
    };
    let mut m = DockModel::empty();
    l.apply_to(&mut m);
    assert_eq!(m.root.as_ref().unwrap().collect_ids(), vec![5, 6]);
    assert_eq!(m.floating.len(), 1);
    assert_eq!(m.floating[0].id, 2);
    assert_eq!(m.floating[0].items(), vec![9]);
    assert_eq!((m.floating[0].x, m.floating[0].w), (5.0, 100.0));
}

/// `dl1` 文本往返（窗口列表 + 最大化状态 + 边界）。
#[test]
fn dock_layout_text_round_trips() {
    let l = DockLayout {
        windows: vec![
            DockWindow {
                id: WINDOW_MAIN,
                kind: WindowKind::Main,
                root: Some(DockNode::split(
                    SplitOrientation::Horizontal,
                    0.4,
                    DockNode::tabs(&[1]),
                    DockNode::tabs(&[2]),
                )),
                maximized: Some(MaximizedState {
                    dockable_id: 1,
                    saved_root: Some(DockNode::tabs(&[3, 4])),
                }),
                bounds: None,
            },
            {
                let mut w = win(3, WindowKind::Floating, Some(DockNode::tabs(&[8])));
                w.bounds = Some(WindowBounds {
                    x: 1.5,
                    y: 2.5,
                    width: 300.0,
                    height: 200.0,
                });
                w
            },
        ],
    };
    let text = serialize_dock_layout(&l);
    assert!(text.starts_with(DOCK_LAYOUT_FORMAT));
    let back = parse_dock_layout_text(&text).expect("应能解析");
    assert!(back.is_valid());
    assert_eq!(back, l);

    // 坏格式被拒
    assert!(parse_dock_layout_text("nope").is_none());
    assert!(parse_dock_layout_text("dl1\nbroken").is_none());
}

/// `DockState::layout()` / `set_layout()`：KT 语义的「整份下一份布局一次赋值」。
#[test]
fn dock_state_exposes_window_layout() {
    let ids: Vec<PanelId> = (0..3)
        .map(|_| {
            compose_material::register_dock_panel(compose_material::DockablePanel::new("P"), || {})
        })
        .collect();
    let st = dock_state(DockModel::new(DockNode::tabs(&ids)));

    // 初始：只有主窗口
    let l = st.layout();
    assert_eq!(l.windows.len(), 1);
    assert!(l.is_valid());

    // 浮起一个面板 → 出现浮动窗口
    st.float_panel(ids[0]);
    let l = st.layout();
    assert_eq!(l.windows.len(), 2);
    assert_eq!(l.floating_windows()[0].kind, WindowKind::Floating);
    assert_eq!(l.window_of(ids[0]), Some(l.floating_windows()[0].id));

    // set_layout：整份替换
    let replacement = DockLayout {
        windows: vec![win(
            WINDOW_MAIN,
            WindowKind::Main,
            Some(DockNode::tabs(&[ids[2]])),
        )],
    };
    st.set_layout(&replacement);
    let l = st.layout();
    assert_eq!(l.windows.len(), 1);
    assert_eq!(
        l.main_window().root.as_ref().unwrap().collect_ids(),
        vec![ids[2]]
    );
    assert!(st.is_docked(ids[2]));
    assert!(!st.is_docked(ids[0]));
}
