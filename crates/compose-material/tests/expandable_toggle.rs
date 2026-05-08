//! `Expandable` 表头点击应切换展开态（曾缺失点击绑定，导致无法展开/折叠）。
use compose::foundation::Constraints;
use compose::layout::{
    create_root_node, dispatch_click_at, hit_test, perform_layout, Alignment, Modifier, TextStyle,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{mutableStateOf, Composition, Recomposer};
use compose_material::widgets::layout::Expandable;
use compose_material::Text;

fn layout(root: compose::layout::NodeId) {
    perform_layout(root, Constraints::tight(800.0, 600.0));
}

fn build() -> (compose::layout::NodeId, compose::runtime::MutableState<bool>) {
    let root = create_root_node(Alignment::TopStart);
    let open = mutableStateOf(false);
    let comp = Composition::new(root);
    let st = open;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let st2 = st;
            Expandable(
                Modifier::new().fill_max_width(),
                st2,
                60.0,
                move || {
                    Text("Header".to_string(), TextStyle::new(), Modifier::new());
                },
                move || {
                    Text("Body".to_string(), TextStyle::new(), Modifier::new());
                },
            );
        });
    });
    layout(root);
    (root, open)
}

/// 头部中心（LIST_ONE_LINE=56 高）。
const HX: f32 = 400.0;
const HY: f32 = 28.0;

#[test]
fn expandable_header_click_toggles() {
    let (root, open) = build();
    assert!(!open.get(), "初始应为折叠态");

    // 首次点击 → 展开
    let (h, lx, ly) = hit_test(root, HX, HY).expect("应命中表头");
    assert!(dispatch_click_at(h, lx, ly), "表头点击应被消费");
    Recomposer::recompose_pending();
    layout(root);
    assert!(open.get(), "首次点击应展开");

    // 再次点击 → 折叠
    let (h2, lx2, ly2) = hit_test(root, HX, HY).expect("应命中表头");
    assert!(dispatch_click_at(h2, lx2, ly2), "表头点击应被消费");
    Recomposer::recompose_pending();
    layout(root);
    assert!(!open.get(), "再次点击应折叠");
}
