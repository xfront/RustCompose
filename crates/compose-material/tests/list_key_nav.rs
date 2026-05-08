//! 回归：ListView / GridView / TreeView / TabRow 的键盘条目切换
//! （↑/↓、PageUp/Down、Home/End；树另有 ←/→ 折叠展开）与选中项 sticky 滚入视口。
//!
//! 焦点模拟走真实点击链（`dispatch_click_at` 沿祖先链聚焦最近 `key_handler` 容器），
//! 顺带端到端验证「点条目不丢容器焦点」。

use compose::foundation::Constraints;
use compose::layout::{
    Modifier, NodeId, create_root_node, dispatch_click, dispatch_key, perform_layout, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{
    Composition, MutableState, NavKey, Recomposer, advance_frame, mutableStateOf, with_rt_pub,
};
use compose_material::{GridView, ListView, OverlayHost, TabRow, TreeNode, TreeView};

/// 泵若干帧：推进帧时钟 → 重组 → 布局（与 `tab_overflow` 测试同款）。
fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
}

/// 组合内的键盘容器节点（挂 `key_handler` 的 focusable 节点，测试页只有一个）。
fn focusable_node() -> NodeId {
    with_rt_pub(|rt| {
        let i = rt
            .nodes
            .iter()
            .position(|n| n.key_handler.is_some())
            .expect("应有 focusable 容器");
        NodeId(i as u32)
    })
}

/// `id` 子树内全部带 `on_click` 的后代（按**文档序**：重组会复用 arena 槽位，
/// 不可按槽位号排序）。
fn clickables_under(id: NodeId) -> Vec<NodeId> {
    with_rt_pub(|rt| {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = rt.nodes[id.index()].children.clone();
        // 深度优先、文档序：逐层把子节点按原序压入（逆序弹栈即正序访问）。
        while let Some(n) = stack.pop() {
            let node = &rt.nodes[n.index()];
            if node.on_click.is_some() {
                out.push(n);
            }
            stack.extend(node.children.iter().rev().copied());
        }
        out
    })
}

/// 竖向滚动容器（恰有 `n` 个带 `on_click` 的直接孩子）→ (滚动节点, 各行)。
fn vscroll_rows(n: usize) -> (NodeId, Vec<NodeId>) {
    with_rt_pub(|rt| {
        for i in 0..rt.nodes.len() {
            let node = &rt.nodes[i];
            if node.scroll.as_ref().map(|s| s.vertical).unwrap_or(false) {
                let rows: Vec<NodeId> = node
                    .children
                    .iter()
                    .copied()
                    .filter(|c| rt.nodes[c.index()].on_click.is_some())
                    .collect();
                if rows.len() == n {
                    return (NodeId(i as u32), rows);
                }
            }
        }
        panic!("应有包含 {n} 个可点击行的竖向滚动容器");
    })
}

/// 读滚动容器 (偏移, 视口高)。
fn scroll_geom(sc: NodeId) -> (f32, f32) {
    with_rt_pub(|rt| {
        let n = &rt.nodes[sc.index()];
        (n.scroll.as_ref().map(|s| s.offset).unwrap_or(0.0), n.height)
    })
}

fn press(key: NavKey) {
    assert!(dispatch_key(key), "dispatch_key({key:?}) 应命中焦点容器");
}

#[test]
fn listview_arrow_keys() {
    let (w, h) = (400.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let sel: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let items: Vec<String> = (0..50).map(|i| format!("Row {i}")).collect();
                ListView(&items, sel, Modifier::new().fill_max_width().height(300.0));
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 4);

    // 点第一行 → 焦点落到 ListView 容器（行本身消费点击，不抢焦点）。
    let (_sc, rows) = vscroll_rows(50);
    assert!(dispatch_click(rows[0]), "点行应命中点击链");
    settle(root, w, h, 4);

    press(NavKey::Down);
    assert_eq!(sel.read_unregistered(), 1);
    press(NavKey::PageDown);
    assert_eq!(sel.read_unregistered(), 9);
    press(NavKey::End);
    assert_eq!(sel.read_unregistered(), 49);
    press(NavKey::Down);
    assert_eq!(sel.read_unregistered(), 49, "末项向下应夹住");
    press(NavKey::Home);
    assert_eq!(sel.read_unregistered(), 0);
    press(NavKey::Up);
    assert_eq!(sel.read_unregistered(), 0, "首项向上应夹住");
    press(NavKey::PageUp);
    assert_eq!(sel.read_unregistered(), 0);
}

#[test]
fn listview_sticky_scroll_into_view() {
    let (w, h) = (400.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let sel: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let items: Vec<String> = (0..50).map(|i| format!("Row {i}")).collect();
                ListView(&items, sel, Modifier::new().fill_max_width().height(200.0));
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 4);
    let (sc, rows) = vscroll_rows(50);
    dispatch_click(rows[0]);
    settle(root, w, h, 4);

    // End → 选中末项：滚动偏移应被 sticky 校正到末项进入视口。
    press(NavKey::End);
    settle(root, w, h, 6);
    assert_eq!(sel.read_unregistered(), 49);
    let (sc, rows) = vscroll_rows(50); // 重组后重取（句柄可能换槽）
    let (off, vp) = scroll_geom(sc);
    let (ry, rh) = with_rt_pub(|rt| {
        let r = &rt.nodes[rows[49].index()];
        (r.y, r.height)
    });
    assert!(off > 0.0, "选中超视口项后偏移应离开顶部：off={off}");
    assert!(
        ry < off + vp && ry + rh > off,
        "末项应滚入视口：y={ry} h={rh} off={off} vp={vp}"
    );
}

#[test]
fn gridview_2d_keys() {
    let (w, h) = (600.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let sel: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let items: Vec<String> = (0..12).map(|i| format!("G{i}")).collect();
                GridView(&items, 3, sel, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 4);
    let cells = clickables_under(focusable_node());
    assert_eq!(cells.len(), 12);
    dispatch_click(cells[0]);
    settle(root, w, h, 4);

    press(NavKey::Down); // 0 → 3（±cols）
    assert_eq!(sel.read_unregistered(), 3);
    press(NavKey::Right); // 3 → 4（±1）
    assert_eq!(sel.read_unregistered(), 4);
    press(NavKey::PageDown); // 4 → 10（±2·cols）
    assert_eq!(sel.read_unregistered(), 10);
    press(NavKey::End);
    assert_eq!(sel.read_unregistered(), 11);
    press(NavKey::Down);
    assert_eq!(sel.read_unregistered(), 11, "末格向下应夹住");
    press(NavKey::Home);
    assert_eq!(sel.read_unregistered(), 0);
    press(NavKey::Left);
    assert_eq!(sel.read_unregistered(), 0, "首格向左应夹住");
}

#[test]
fn treeview_visible_paths_and_keys() {
    let (w, h) = (400.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let expanded: MutableState<Vec<String>> = mutableStateOf(Vec::new());
    let sel: MutableState<String> = mutableStateOf(String::from("Root"));
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let tree = TreeNode::with_children(
                    "Root",
                    vec![
                        TreeNode::with_children("Child A", vec![TreeNode::new("Leaf A1")]),
                        TreeNode::new("Child B"),
                    ],
                );
                TreeView(&[tree], expanded, sel, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 4);

    // 点 Root 行：选中 + 展开。
    let rows = clickables_under(focusable_node());
    dispatch_click(rows[0]);
    settle(root, w, h, 4);
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root");
    assert!(expanded.peek_mut(|v| v.contains(&"Root".to_string())));

    // 可见表 = Root / Root/Child A / Root/Child B：Down 移到 Child A。
    press(NavKey::Down);
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root/Child A");
    // Right：Child A 有子且未展开 → 展开（选中不动）。
    press(NavKey::Right);
    assert!(
        expanded.peek_mut(|v| v.contains(&"Root/Child A".to_string())),
        "Right 应展开 Child A"
    );
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root/Child A");
    settle(root, w, h, 4);
    // 展平表变长：Down 进 Leaf A1。
    press(NavKey::Down);
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root/Child A/Leaf A1");
    // Left：叶子无子 → 跳父。
    press(NavKey::Left);
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root/Child A");
    // Left：已展开 → 折叠并停在原节点。
    press(NavKey::Left);
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root/Child A");
    assert!(
        !expanded.peek_mut(|v| v.contains(&"Root/Child A".to_string())),
        "Left 应折叠 Child A"
    );
    // Home/End 在可见表上跳首尾。
    press(NavKey::End);
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root/Child B");
    press(NavKey::Home);
    assert_eq!(sel.peek_mut(|s| s.clone()), "Root");
}

#[test]
fn tabrow_key_switch() {
    let (w, h) = (300.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let tab: MutableState<usize> = mutableStateOf(2usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let many: Vec<String> = (0..12).map(|i| format!("Tab {i:02}")).collect();
                TabRow(&many, tab, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 60);

    // 点第一个标签取得容器焦点（标签自身消费点击）。
    let tabs = clickables_under(focusable_node());
    dispatch_click(tabs[0]);
    settle(root, w, h, 4);
    assert_eq!(tab.read_unregistered(), 0);

    press(NavKey::Down);
    assert_eq!(tab.read_unregistered(), 1);
    press(NavKey::Right);
    assert_eq!(tab.read_unregistered(), 2);
    press(NavKey::PageDown);
    assert_eq!(tab.read_unregistered(), 7, "PageDown 应 ±5");
    press(NavKey::End);
    assert_eq!(tab.read_unregistered(), 11);
    press(NavKey::Down);
    assert_eq!(tab.read_unregistered(), 11, "末标签应夹住");
    press(NavKey::Home);
    assert_eq!(tab.read_unregistered(), 0);
}
