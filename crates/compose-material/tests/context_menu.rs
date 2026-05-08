//! 回归：`ContextMenu` 支持鼠标右键弹出菜单。
//!
//! 引擎链路 winit `MouseButton::Right` → `Kernel::pointer_secondary_click`
//! → `dispatch_secondary_click_at` → `Node.on_secondary_click`。
//! 右键与 `on_click` 严格分离：不进按压/波纹链路，也不结算成普通左键点击。

use std::cell::Cell;

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Modifier, NodeId, Policy, add_node, create_root_node, dispatch_click_at,
    dispatch_secondary_click_at, hit_test, perform_layout, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, advance_frame, with_rt_pub};
use compose_material::{ContextMenu, OverlayHost};
use compose_material::widgets::button_label;

thread_local! {
    static PRIMARY: Cell<usize> = const { Cell::new(0) };
    static SECONDARY: Cell<usize> = const { Cell::new(0) };
    static SELECTED: Cell<Option<usize>> = const { Cell::new(None) };
}

fn bump_primary() {
    PRIMARY.with(|c| c.set(c.get() + 1));
}
fn bump_secondary() {
    SECONDARY.with(|c| c.set(c.get() + 1));
}
fn record_sel(i: usize) {
    SELECTED.with(|c| c.set(Some(i)));
}
fn reset_counters() {
    PRIMARY.with(|c| c.set(0));
    SECONDARY.with(|c| c.set(0));
    SELECTED.with(|c| c.set(None));
}

fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
}

fn alive_count() -> usize {
    with_rt_pub(|rt| rt.nodes.iter().filter(|n| n.alive).count())
}

/// 节点绝对矩形（累加父链局部坐标）。
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
        for n in chain {
            let node = &rt.nodes[n.index()];
            x += node.x;
            y += node.y;
        }
        let n = &rt.nodes[id.index()];
        (x, y, n.width, n.height)
    })
}

fn node_with_secondary() -> NodeId {
    with_rt_pub(|rt| {
        NodeId(
            rt.nodes
                .iter()
                .position(|n| n.alive && n.on_secondary_click.is_some())
                .expect("应有带 on_secondary_click 的节点") as u32,
        )
    })
}

/// 引擎层：一个普通节点同时挂左/右键处理器。
fn boot_plain() -> NodeId {
    reset_counters();
    let (w, h) = (300.0, 200.0);
    set_viewport(w, h);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let id = add_node(
            Policy::Box(Alignment::TopStart),
            Modifier::new().size(120.0, 48.0),
        );
        with_rt_pub(|rt| {
            let n = &mut rt.nodes[id.index()];
            n.on_click = Some(Box::new(bump_primary));
            n.on_secondary_click = Some(Box::new(bump_secondary));
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 3);
    std::mem::forget(comp);
    root
}

/// 控件层：ContextMenu + OverlayHost。返回 (root,w,h,锚点中心 x/y,锚点底边)。
fn boot_context_menu() -> (NodeId, f32, f32, f32, f32, f32) {
    reset_counters();
    let (w, h) = (400.0, 300.0);
    set_viewport(w, h);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                ContextMenu(
                    &["Cut".to_string(), "Copy".to_string(), "Paste".to_string()],
                    Modifier::new().fill_max_width().height(60.0),
                    record_sel,
                    || button_label("Right-click here"),
                );
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 6);
    std::mem::forget(comp);
    let (ax, ay, aw, ah) = abs_rect(node_with_secondary());
    (root, w, h, ax + aw / 2.0, ay + ah / 2.0, ay + ah)
}

#[test]
fn secondary_click_only_fires_secondary_handler() {
    boot_plain();
    let id = node_with_secondary();
    assert!(
        dispatch_secondary_click_at(id, 0.0, 0.0),
        "右键应命中 on_secondary_click"
    );
    assert_eq!(SECONDARY.with(|c| c.get()), 1);
    assert_eq!(PRIMARY.with(|c| c.get()), 0, "右键不得触发 on_click");
    assert!(dispatch_click_at(id, 0.0, 0.0), "左键仍照常工作");
    assert_eq!(PRIMARY.with(|c| c.get()), 1);
    assert_eq!(SECONDARY.with(|c| c.get()), 1, "左键不得触发右键处理器");
}

/// 右键命中内容子节点时，应沿祖先链找到 ContextMenu 锚点并弹出浮层。
#[test]
fn context_menu_opens_on_right_click() {
    let (root, w, h, cx, cy, _) = boot_context_menu();
    let before = alive_count();
    let (target, lx, ly) = hit_test(root, cx, cy).expect("内容区应命中");
    assert!(
        dispatch_secondary_click_at(target, lx, ly),
        "右键应被 ContextMenu 消费"
    );
    settle(root, w, h, 6);
    let after = alive_count();
    assert!(
        after > before,
        "右键后菜单浮层应被组合：{before} → {after}"
    );
}

/// 菜单弹出后可点菜单项并回调 `on_selected`（端到端可见性验证）。
#[test]
fn context_menu_item_is_clickable_after_right_click() {
    let (root, w, h, cx, cy, anchor_bottom) = boot_context_menu();
    let (target, lx, ly) = hit_test(root, cx, cy).expect("内容区应命中");
    assert!(dispatch_secondary_click_at(target, lx, ly));
    settle(root, w, h, 6);

    // 收集可点节点后在外层算几何（避免嵌套 runtime 借用）。
    let clickable: Vec<NodeId> = with_rt_pub(|rt| {
        rt.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.alive && n.on_click.is_some())
            .map(|(i, _)| NodeId(i as u32))
            .collect()
    });
    let items: Vec<NodeId> = clickable
        .into_iter()
        .filter(|id| {
            let (_, y, ww, hh) = abs_rect(*id);
            // 排除全屏遮罩（宽≈视口）与锚点自身（在 anchor_bottom 之上）。
            y >= anchor_bottom && ww < w - 1.0 && hh < 100.0
        })
        .collect();
    assert!(!items.is_empty(), "右键后应出现可点的菜单项");
    assert!(dispatch_click_at(items[0], 0.0, 0.0), "菜单项应可点击");
    assert!(
        SELECTED.with(|c| c.get()).is_some(),
        "点菜单项应触发 on_selected"
    );
}

/// host 实际调用的 `Kernel::pointer_secondary_click` 入口（含 hit_test + 指针坐标写入）。
#[test]
fn kernel_secondary_click_entry_opens_menu() {
    let (w, h) = (400.0, 300.0);
    set_viewport(w, h);
    let kernel = compose::render::kernel::Kernel::new(ColorScheme::light(), move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                ContextMenu(
                    &["Cut".to_string(), "Copy".to_string()],
                    Modifier::new().fill_max_width().height(60.0),
                    |_| {},
                    || button_label("Right-click here"),
                );
            });
        });
    });
    let root = with_rt_pub(|rt| {
        NodeId(
            rt.nodes
                .iter()
                .position(|n| n.alive && n.parent.is_none())
                .expect("应有组合根节点") as u32,
        )
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 6);
    let (ax, ay, aw, ah) = abs_rect(node_with_secondary());
    let (px, py) = (ax + aw / 2.0, ay + ah / 2.0);
    let before = alive_count();
    assert!(
        kernel.pointer_secondary_click(px, py),
        "内核右键入口应被 ContextMenu 消费"
    );
    settle(root, w, h, 6);
    assert!(alive_count() > before, "菜单浮层应出现");
    assert_eq!(
        compose::layout::last_pointer_global(),
        (px, py),
        "右键坐标应写入全局指针"
    );
}

/// 左键兼容入口仍然可用（不破坏既有调用方）。
#[test]
fn context_menu_still_opens_on_left_click() {
    let (root, w, h, cx, cy, _) = boot_context_menu();
    let before = alive_count();
    let (target, lx, ly) = hit_test(root, cx, cy).expect("内容区应命中");
    assert!(dispatch_click_at(target, lx, ly));
    settle(root, w, h, 6);
    assert!(alive_count() > before, "左键仍应弹出菜单");
    assert_eq!(SECONDARY.with(|c| c.get()), 0, "左键不应触发右键处理器");
}
