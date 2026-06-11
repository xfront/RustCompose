//! 诊断：菜单项过多时是否显示滚动条（走真实 DropdownMenu → Overlay → scrim 路径）。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, Policy, add_node, create_root_node, draw_tree, perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, advance_frame, mutableStateOf, with_rt_pub};
use compose_material::widgets::input::DropdownMenu;
use compose_material::OverlayHost;

const KEY: u64 = 0x5C0A_1100;

#[test]
fn many_item_dropdown_shows_scrollbar() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let anchor = add_node(Policy::Leaf, Modifier::new().size(160.0, 40.0));
                let open = mutableStateOf(true);
                let items: Vec<String> = (0..30).map(|i| format!("Item {i}")).collect();
                DropdownMenu(anchor, KEY, &items, open, true, |_| {});
            });
        });
    });
    perform_layout(root, Constraints::tight(400.0, 400.0));
    for _ in 0..40 {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(400.0, 400.0));
    }

    with_rt_pub(|rt| {
        for (i, n) in rt.nodes.iter().enumerate() {
            if let Some(s) = n.scroll.as_ref() {
                println!(
                    "[scroll node {i}] {}x{} content={} offset={} vertical={}",
                    n.width, n.height, s.content_size, s.offset, s.vertical
                );
            }
        }
    });

    let cmds = draw_tree(root);
    let thumb = ColorScheme::light().on_surface.with_alpha(0.42);
    let has = cmds
        .iter()
        .any(|c| matches!(c, DrawCmd::FillRRect { color, .. } if *color == thumb));
    println!("[scrollbar] present={}", has);
    assert!(has, "30 项下拉应显示滚动条滑块");
}
