//! 回归用例：SearchBar 尾部图标（清空按钮）应位于内容区最右，输入框占满剩余宽度。

use compose::foundation::Constraints;
use compose::layout::{Alignment, Modifier, create_root_node, perform_layout};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition, mutableStateOf, with_rt_pub};
use compose_material::SearchBar;

#[test]
fn searchbar_trailing_icon_at_far_right() {
    let root = create_root_node(Alignment::TopStart);
    let q = mutableStateOf(String::new());
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            SearchBar(q, "Search…", Modifier::new().fill_max_width());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 600.0));
    let (row_w, kids) = with_rt_pub(|rt| {
        // root → 包装节点 → SearchBar Row
        let row = rt.nodes[root.index()].children[0];
        let row = rt.nodes[row.index()].children[0];
        let n = &rt.nodes[row.index()];
        (n.width, n.children.clone())
    });
    assert_eq!(kids.len(), 4, "图标 / 间隔 / 输入框 / 清空按钮");
    let (field_w, btn) = with_rt_pub(|rt| {
        let f = &rt.nodes[kids[2].index()];
        let b = &rt.nodes[kids[3].index()];
        (f.width, (b.x, b.width))
    });
    // 输入框占满剩余宽度（不能塌缩为 0，否则按钮贴到左侧）
    assert!(field_w > 100.0, "输入框应展开，实际宽 {field_w}");
    // 清空按钮右缘 = 节点右缘 − 右 padding（内容区最右）
    let (btn_r, row_r) = with_rt_pub(|rt| {
        let b = &rt.nodes[kids[3].index()];
        (b.x + b.width, row_w - 12.0)
    });
    let _ = btn;
    assert!(
        (btn_r - row_r).abs() < 0.5,
        "清空按钮应贴最右：btn_r={btn_r} row_r={row_r}"
    );
}
