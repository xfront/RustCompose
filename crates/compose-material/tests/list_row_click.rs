//! 回归：`List` / `ListView` 的「整行点击选择」。
//!
//! 与 `list_key_nav.rs` 不同：这里走**真实命中链** `hit_test → dispatch_click_at`，
//! 而不是直接 `dispatch_click(row)`。后者绕过了命中测试，无法验证「点整行任意
//! 位置（中间文字、左右 padding、上下留白）都能选中该行」；且旧测试只点第 0 行
//! （初始选中本就是 0），即便点击完全不生效也会通过。

use compose::foundation::Constraints;
use compose::layout::{
    Modifier, NodeId, create_root_node, dispatch_click_at, dispatch_key, hit_test, perform_layout,
    set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{
    Composition, MutableState, NavKey, Recomposer, advance_frame, mutableStateOf, with_rt_pub,
};
use compose_material::{List, ListView, OverlayHost};

const N: usize = 8;

fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
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

/// `List`（无滚动）：focusable 容器子树内带 `on_click` 的节点即行，按文档序。
fn list_rows(n: usize) -> Vec<NodeId> {
    with_rt_pub(|rt| {
        let f = rt
            .nodes
            .iter()
            .position(|x| x.key_handler.is_some())
            .expect("应有 focusable 容器");
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = rt.nodes[f].children.clone();
        while let Some(id) = stack.pop() {
            let node = &rt.nodes[id.index()];
            if node.on_click.is_some() {
                out.push(id);
            }
            stack.extend(node.children.iter().rev().copied());
        }
        assert_eq!(out.len(), n, "应有 {n} 个可点击的整行");
        out
    })
}

/// 行子树内的全部叶子节点（文字、图标等）：用来精确验证「点中中间文字所在」。
fn leaves_under(row: NodeId) -> Vec<NodeId> {
    with_rt_pub(|rt| {
        let mut out = Vec::new();
        let mut stack = vec![row];
        while let Some(id) = stack.pop() {
            let node = &rt.nodes[id.index()];
            if node.children.is_empty() {
                out.push(id);
            } else {
                stack.extend(node.children.iter().rev().copied());
            }
        }
        out
    })
}

/// 节点的窗口绝对矩形 `(x, y, w, h)`：沿父链累加局部坐标，并减去父滚动偏移。
fn abs_rect(id: NodeId) -> (f32, f32, f32, f32) {
    with_rt_pub(|rt| {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(n) = cur {
            chain.push(n);
            cur = rt.nodes[n.index()].parent;
        }
        chain.reverse();
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        for i in 0..chain.len() {
            if i > 0 {
                let p = &rt.nodes[chain[i - 1].index()];
                if let Some(s) = p.scroll.as_ref() {
                    if s.vertical {
                        y -= s.offset;
                    } else {
                        x -= s.offset;
                    }
                }
            }
            let n = &rt.nodes[chain[i].index()];
            x += n.x;
            y += n.y;
        }
        let n = &rt.nodes[id.index()];
        (x, y, n.width, n.height)
    })
}

fn click_at(root: NodeId, x: f32, y: f32) -> bool {
    let Some((t, lx, ly)) = hit_test(root, x, y) else {
        return false;
    };
    dispatch_click_at(t, lx, ly)
}

/// 建一个所有行都可见的列表。`with_scroll=true` 用 `ListView`，否则用新的 `List`。
fn boot(with_scroll: bool) -> (NodeId, MutableState<usize>, f32, f32) {
    let (w, h) = (400.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let sel: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let items: Vec<String> = (0..N).map(|i| format!("Row {i}")).collect();
                if with_scroll {
                    ListView(&items, sel, Modifier::new().fill_max_width().height(500.0));
                } else {
                    List(&items, sel, Modifier::new().fill_max_width());
                }
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 6);
    std::mem::forget(comp);
    (root, sel, w, h)
}

/// 行内不同位置的采样点：中心（文字所在）、左/右内边距、上/下留白。
fn sample_points(row: NodeId) -> Vec<(&'static str, f32, f32)> {
    let (x, y, w, h) = abs_rect(row);
    vec![
        ("中心文字", x + w / 2.0, y + h / 2.0),
        ("左内边距", x + 2.0, y + h / 2.0),
        ("右内边距", x + w - 2.0, y + h / 2.0),
        ("上留白", x + w / 2.0, y + 2.0),
        ("下留白", x + w / 2.0, y + h - 2.0),
    ]
}

fn rows_of(with_scroll: bool, n: usize, target: usize) -> NodeId {
    if with_scroll {
        vscroll_rows(n).1[target]
    } else {
        list_rows(n)[target]
    }
}

/// 行内任意位置（含 padding / 留白）都应选中该行。
fn check_any_point(with_scroll: bool) {
    let (root, sel, w, h) = boot(with_scroll);
    const TARGET: usize = 3;

    for (name, _, _) in sample_points(rows_of(with_scroll, N, TARGET)) {
        // 每轮重置选中并重新布局，保证各采样点互相独立；重组会换节点槽位，
        // 因此坐标按当前代的同一行重新计算。
        sel.set(0);
        settle(root, w, h, 4);
        let (rx, ry, rw, rh) = abs_rect(rows_of(with_scroll, N, TARGET));
        let (px, py) = match name {
            "中心文字" => (rx + rw / 2.0, ry + rh / 2.0),
            "左内边距" => (rx + 2.0, ry + rh / 2.0),
            "右内边距" => (rx + rw - 2.0, ry + rh / 2.0),
            "上留白" => (rx + rw / 2.0, ry + 2.0),
            _ => (rx + rw / 2.0, ry + rh - 2.0),
        };
        assert!(click_at(root, px, py), "整行「{name}」应命中点击链");
        settle(root, w, h, 4);
        assert_eq!(
            sel.read_unregistered(),
            TARGET,
            "点击第 {TARGET} 行的「{name}」应选中该行"
        );
    }
}

/// 「整行」必须覆盖 padding：行节点撑满可用宽度，文字从左右内边距之后开始。
#[test]
fn list_row_covers_padding_area() {
    let (_root, _sel, w, _h) = boot(false);
    let rows = list_rows(N);
    let (rx, _ry, rw, rh) = abs_rect(rows[0]);
    assert!(
        rw >= w - 1.0,
        "行节点应撑满整宽（含左右 padding）：rw={rw} viewport={w}"
    );
    assert!(rh >= 50.0, "行应保留 56dp 最小高度（含上下留白）：rh={rh}");
    let (lx, _, _, _) = abs_rect(leaves_under(rows[0])[0]);
    assert!(lx > rx, "文字应在左 padding 之后开始：lx={lx} rx={rx}");
}

#[test]
fn list_click_any_point_of_row_selects_it() {
    check_any_point(false);
}

#[test]
fn listview_click_any_point_of_row_selects_it() {
    check_any_point(true);
}

/// 精确点中「中间文字所在」的叶子节点，也应沿祖先链归属整行并选中。
#[test]
fn list_click_text_leaves_select_their_row() {
    let (root, sel, w, h) = boot(false);
    const TARGET: usize = 2;
    let leaves = leaves_under(list_rows(N)[TARGET]);
    assert!(!leaves.is_empty(), "行内应有文字叶子节点");

    for li in 0..leaves.len() {
        sel.set(0);
        settle(root, w, h, 4);
        let leaves = leaves_under(list_rows(N)[TARGET]);
        let (x, y, lw, lh) = abs_rect(leaves[li]);
        let (px, py) = (x + lw / 2.0, y + lh / 2.0);
        let hit = hit_test(root, px, py);
        eprintln!("[textleaf] leaf#{li} rect=({x:.0},{y:.0},{lw:.0},{lh:.0}) hit={hit:?}");
        assert!(click_at(root, px, py), "点中文字/图标叶子应命中点击链");
        settle(root, w, h, 4);
        assert_eq!(
            sel.read_unregistered(),
            TARGET,
            "点中第 {TARGET} 行内叶子 #{li} 应选中该行"
        );
    }
}

/// 每一行都能通过点击选中自己。
#[test]
fn list_click_each_row_selects_that_row() {
    let (root, sel, w, h) = boot(false);
    for target in 0..N {
        let (x, y, rw, rh) = abs_rect(list_rows(N)[target]);
        assert!(
            click_at(root, x + rw / 2.0, y + rh / 2.0),
            "第 {target} 行中心应命中"
        );
        settle(root, w, h, 4);
        assert_eq!(sel.read_unregistered(), target, "点第 {target} 行应选中它");
    }
}

/// 开箱即用：调用方不给 `fill_max_width` 时，行也应铺满父宽（含 padding）。
#[test]
fn list_default_modifier_still_covers_full_width() {
    let (w, h) = (400.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let sel: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let items: Vec<String> = (0..N).map(|i| format!("Row {i}")).collect();
                List(&items, sel, Modifier::new());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 6);
    std::mem::forget(comp);
    let (_, _, rw, _) = abs_rect(list_rows(N)[0]);
    eprintln!("[defmod] 行宽 rw={rw} viewport={w}");
    assert!(
        rw >= w - 1.0,
        "不指定宽度时整行也应铺满父宽（含 padding）：rw={rw}"
    );
}

/// 点击取得焦点后，键盘也能切换选中（与 ListView 同语义）。
#[test]
fn list_keyboard_switch_after_click() {
    let (root, sel, w, h) = boot(false);
    let (x, y, rw, rh) = abs_rect(list_rows(N)[0]);
    assert!(click_at(root, x + rw / 2.0, y + rh / 2.0));
    settle(root, w, h, 4);
    assert!(dispatch_key(NavKey::Down));
    assert_eq!(sel.read_unregistered(), 1);
    assert!(dispatch_key(NavKey::End));
    assert_eq!(sel.read_unregistered(), N - 1);
    assert!(dispatch_key(NavKey::Home));
    assert_eq!(sel.read_unregistered(), 0);
}
