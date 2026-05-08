//! `TabRow` 回归测试：溢出 ⋮ 按钮出现 + 完整下拉交互（打开→点击菜单项→选中更新）。
//!
//! 覆盖用户报的两点：
//! 1) 多项时 `TabRow` 应出现溢出 `⋮` 按钮（`tab_overflow` 帧驱动）；
//! 2) 从下拉菜单选中项应改变 `selected`（下拉项可命中、`fire_menu_cb` 真正触发）。
use std::rc::Rc;
use compose::foundation::Constraints;
use compose::layout::{
    create_root_node, dispatch_click_at, hit_test, perform_layout, Alignment, Modifier, NodeId,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{mutableStateOf, Composition, advance_frame, with_rt_pub};
use compose_material::widgets::navigation::TabRow;
use compose_material::widgets::overlay::OverlayHost;

/// 计算节点在树中的绝对坐标（子节点的 x/y 是相对父的）。
fn abs_pos(id: NodeId) -> (f32, f32) {
    with_rt_pub(|rt| {
        let mut acc = (0.0f32, 0.0f32);
        let mut cur = Some(id);
        while let Some(c) = cur {
            let n = &rt.nodes[c.index()];
            acc.0 += n.x;
            acc.1 += n.y;
            cur = n.parent;
        }
        acc
    })
}

fn center(id: NodeId) -> (f32, f32) {
    let (ax, ay) = abs_pos(id);
    let (w, h) = with_rt_pub(|rt| {
        let n = &rt.nodes[id.index()];
        (n.width, n.height)
    });
    (ax + w / 2.0, ay + h / 2.0)
}

fn boot() -> (NodeId, compose::runtime::MutableState<usize>, &'static [String]) {
    let root = create_root_node(Alignment::TopStart);
    let sel = mutableStateOf(0usize);
    let many: &'static Vec<String> =
        Box::leak(Box::new((0..12).map(|i| format!("Tab {}", i + 1)).collect()));
    let comp = Composition::new(root);
    let s = sel;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                TabRow(many.as_slice(), s, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(800.0, 600.0));
    (root, sel, many.as_slice())
}

fn pump(root: NodeId, n: usize) {
    for _ in 0..n {
        advance_frame(16.0);
        perform_layout(root, Constraints::tight(800.0, 600.0));
    }
}

#[test]
fn tabrow_overflow_dropdown_select_updates() {
    let (root, sel, _titles) = boot();
    pump(root, 15);

    // 1) ⋮ 按钮（宽 40 的 MoreVert）应出现
    let more = with_rt_pub(|rt| {
        rt.nodes
            .iter()
            .position(|n| n.on_click.is_some() && (n.width - 40.0).abs() < 1.0 && n.height > 40.0)
    });
    assert!(more.is_some(), "12 个标签应出现溢出 ⋮ 按钮");

    // 2) 点击 ⋮ 打开下拉
    let (mx, my) = center(NodeId(more.unwrap() as u32));
    let (t, lx, ly) = hit_test(root, mx, my).expect("应命中 ⋮ 按钮");
    assert!(dispatch_click_at(t, lx, ly), "⋮ 点击应被消费");
    pump(root, 15);

    // 3) 下拉应出现菜单项（窄于视口、高度 ~48、y 在弹出区），且用绝对坐标命中
    let item = with_rt_pub(|rt| {
        let mut best: Option<(usize, f32)> = None;
        for i in 0..rt.nodes.len() {
            let n = &rt.nodes[i];
            if n.on_click.is_some()
                && n.width > 50.0
                && n.width < 300.0
                && (n.height - 48.0).abs() < 1.0
                && n.y > 10.0
                && n.y < 300.0
            {
                // 取最靠上的菜单项
                if best.map(|(_, b)| n.y < b).unwrap_or(true) {
                    best = Some((i, n.y));
                }
            }
        }
        best.map(|(i, _)| i)
    });
    let item = item.expect("下拉应出现可点击菜单项");

    let (ix, iy) = center(NodeId(item as u32));
    let before = sel.get();
    let (t2, lx2, ly2) = hit_test(root, ix, iy).expect("应命中菜单项");
    assert!(dispatch_click_at(t2, lx2, ly2), "菜单项点击应被消费");
    pump(root, 15);

    assert_ne!(sel.get(), before, "点击菜单项应改变选中");
    assert!(sel.get() >= 9, "选中项应落在溢出段（>=9），实际 {}", sel.get());

    // 4) 选中后菜单应关闭
    let still_open = with_rt_pub(|rt| {
        rt.nodes
            .iter()
            .any(|n| n.on_click.is_some() && n.width > 50.0 && n.width < 300.0 && n.y > 10.0)
    });
    assert!(!still_open, "选择后下拉应关闭");
}
