//! 回归：`ListCustom` / `ListViewCustom` 支持自定义 item，且「整行（含自定义内容）点击选中」。
//!
//! 形如 Jetpack Compose：`ListCustom(count, selected, modifier, |i| { 任意 widget })`。
//! 行外壳（整行点击选中 + `secondary_container` 高亮）由 `list_row` 统一提供，
//! 故点击自定义内容（图标/文字/副标题）或左右 padding / 上下留白都归属同一行。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Arrangement, CrossAxisAlignment, Modifier, NodeId, Row, Column, Spacer, TextStyle,
    create_root_node, dispatch_click_at, hit_test, perform_layout, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{
    Composition, MutableState, Recomposer, advance_frame, mutableStateOf, with_rt_pub,
};
use compose_material::{ListCustom, ListViewCustom, OverlayHost, Text};

const N: usize = 6;

fn data() -> Vec<String> {
    (0..N).map(|i| format!("Item {}", i)).collect()
}

/// 自定义 item：图标 + 两行文字 + trailing。点击其中任意位置都应命中行外壳。
fn custom_item(i: usize) {
    Row(
        Modifier::new()
            .fill_max_width()
            .padding_ltrb(16.0, 0.0, 16.0, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                &format!("★{}", i),
                TextStyle::new().size(18.0),
                Modifier::new(),
            );
            Spacer(Modifier::new().width(12.0));
            Column(
                Modifier::new().weight(1.0, false),
                Arrangement::Center,
                CrossAxisAlignment::Start,
                move || {
                    // 在 `Fn + 'static` 闭包内持有 owned 内容，避免借用外层临时变量。
                    let s = data()[i].clone();
                    Text(&s, TextStyle::new().size(16.0), Modifier::new());
                    Text(
                        &format!("sub {}", i),
                        TextStyle::new().size(13.0),
                        Modifier::new(),
                    );
                },
            );
            Text("›", TextStyle::new().size(16.0), Modifier::new());
        },
    );
}

fn boot(with_scroll: bool) -> (NodeId, MutableState<usize>, f32, f32) {
    let (w, h) = (400.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let sel: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            if with_scroll {
                ListViewCustom(
                    N,
                    sel,
                    Modifier::new().fill_max_width().height(200.0),
                    custom_item,
                );
            } else {
                ListCustom(N, sel, Modifier::new().fill_max_width(), custom_item);
            }
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    std::mem::forget(comp);
    (root, sel, w, h)
}

fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
}

fn abs_rect(id: NodeId) -> (f32, f32, f32, f32) {
    with_rt_pub(|rt| {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(n) = cur {
            chain.push(n);
            cur = rt.nodes[n.index()].parent;
        }
        chain.reverse();
        let (mut x, mut y) = (0.0f32, 0.0f32);
        for n in chain {
            x += rt.nodes[n.index()].x;
            y += rt.nodes[n.index()].y;
        }
        let n = &rt.nodes[id.index()];
        (x, y, n.width, n.height)
    })
}

/// 所有行外壳节点（带 on_click 的整行 Box）。
fn rows() -> Vec<NodeId> {
    with_rt_pub(|rt| {
        rt.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.alive && n.on_click.is_some())
            .map(|(i, _)| NodeId(i as u32))
            .collect()
    })
}

fn click_at(root: NodeId, x: f32, y: f32) -> bool {
    let Some((target, lx, ly)) = hit_test(root, x, y) else {
        return false;
    };
    dispatch_click_at(target, lx, ly)
}

#[test]
fn list_custom_item_click_selects_whole_row() {
    let (root, sel, w, h) = boot(false);
    const T: usize = 3;
    // 行内多个采样点：中心文字、左/右 padding、上/下留白。
    let points = [
        ("中心文字", 0.5, 0.5),
        ("左 padding", 0.02, 0.5),
        ("右 padding", 0.98, 0.5),
        ("上留白", 0.5, 0.05),
        ("下留白", 0.5, 0.95),
    ];
    for (name, fx, fy) in points {
        sel.set(0);
        settle(root, w, h, 4);
        let (rx, ry, rw, rh) = abs_rect(rows()[T]);
        let (px, py) = (rx + rw * fx, ry + rh * fy);
        assert!(
            click_at(root, px, py),
            "custom item 的「{name}」应命中点击链"
        );
        settle(root, w, h, 4);
        assert_eq!(
            sel.read_unregistered(),
            T,
            "点 custom item 的「{name}」应选中第 {T} 行"
        );
    }
    let _ = h;
}

#[test]
fn listview_custom_item_click_selects() {
    let (root, sel, w, h) = boot(true);
    const T: usize = 1; // 滚动容器内可见行
    settle(root, w, h, 4);
    let (rx, ry, rw, rh) = abs_rect(rows()[T]);
    let (px, py) = (rx + rw * 0.5, ry + rh * 0.5);
    assert!(
        click_at(root, px, py),
        "ListViewCustom 的 custom item 应命中点击链"
    );
    settle(root, w, h, 4);
    assert_eq!(
        sel.read_unregistered(),
        T,
        "点 ListViewCustom 的 custom item 应选中第 {T} 行"
    );
}

#[test]
fn list_custom_item_count_matches() {
    let (root, _sel, w, h) = boot(false);
    settle(root, w, h, 4);
    let rs = rows();
    for r in &rs {
        let (x, y, ww, hh) = abs_rect(*r);
        let parent = with_rt_pub(|rt| rt.nodes[r.index()].parent.map(|p| p.index()));
        eprintln!(
            "[diag] id={} rect=({x:.1},{y:.1},{ww:.1},{hh:.1}) parent={parent:?}",
            r.index()
        );
    }
    assert_eq!(rs.len(), N, "应有 N 个自定义行");
}
