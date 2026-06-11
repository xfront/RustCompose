//! 回归：靠右的浮层（TabRow 溢出「⋮」按内容自然宽展开）应被智能重定位，
//! 完整落在视口内，而不是伸出窗口右边界被 OS 裁切。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Modifier, Policy, add_node, create_root_node, node_abs_rect, perform_layout,
    set_viewport, viewport_size,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, advance_frame, mutableStateOf, with_rt_pub};
use compose_material::widgets::input::DropdownMenu;
use compose_material::OverlayHost;

const KEY: u64 = 0x5C0A_2200;

#[test]
fn right_edge_dropdown_is_shifted_into_view() {
    set_viewport(900.0, 600.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                // 锚点贴右上角，宽 60 → 绝对 x ≈ 840。
                let anchor = add_node(
                    Policy::Leaf,
                    Modifier::new().size(60.0, 40.0).align(Alignment::TopEnd),
                );
                let open = mutableStateOf(true);
                // 长文字 → 自然宽明显大于锚点宽，若左对齐锚点必然伸出右边界。
                let items: Vec<String> =
                    vec!["A_LONG_MENU_ITEM_LABEL_TEXT".to_string(); 3];
                DropdownMenu(anchor, KEY, &items, open, false, |_| {});
            });
        });
    });
    perform_layout(root, Constraints::tight(900.0, 600.0));
    for _ in 0..40 {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(900.0, 600.0));
    }

    let vw = viewport_size().0;
    // 找到浮层面板（滚动容器）节点，取其绝对矩形。
    let panel = with_rt_pub(|rt| {
        rt.nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.scroll.as_ref().map(|s| s.vertical).unwrap_or(false))
            .map(|(i, _)| compose::layout::NodeId(i as u32))
    })
    .expect("应存在滚动面板节点");

    let (px, _py, pw, _ph) = node_abs_rect(panel);
    println!("[panel] abs_x={px} width={pw} viewport_w={vw}");

    // 面板必须完整落在视口内：右缘不越过窗口，且确实被左移（非停在锚点 x=840）。
    assert!(
        px + pw <= vw + 0.5,
        "面板右缘 {} 伸出视口宽 {vw}",
        px + pw
    );
    assert!(px >= 0.0, "面板左缘不应为负");
    assert!(
        pw > 60.0,
        "自然宽面板应明显宽于锚点（否则未测到溢出场景），实际 {pw}"
    );
    assert!(
        px < 840.0,
        "面板应从锚点 x=840 向左智能重定位，实际停在 {px}"
    );
}
