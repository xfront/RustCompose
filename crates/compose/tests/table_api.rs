//! 回归用例：DataTable API 接线正确（组合不 panic、表头与数据进入绘制树）。

use compose::foundation::Constraints;
use compose::layout::{create_root_node, draw_tree, DrawCmd, Modifier};
use compose::material::{MaterialTheme, ColorScheme};
use compose::runtime::Composer;
use compose_material::DataTable;

fn text_values(cmds: &[DrawCmd]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn data_table_renders_header_and_rows() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            DataTable(&["ID".into(), "Name".into(), "Qty".into()],
                &[
                    vec!["1".into(), "Apple".into(), "10".into()],
                    vec!["2".into(), "Banana".into(), "20".into()],
                ],
                true,
                true,
                Modifier::new(),
            );
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(300.0, 200.0));
    let vals = text_values(&draw_tree(root));
    assert!(vals.iter().any(|t| t == "ID"), "表头应含 ID: {vals:?}");
    assert!(vals.iter().any(|t| t == "Apple"), "数据应含 Apple: {vals:?}");
    assert!(vals.iter().any(|t| t == "Banana"), "数据应含 Banana: {vals:?}");
}

#[test]
fn data_table_numbers_right_align_no_panic() {
    // 数字列（整列可解析 f64）不应导致 panic；组合期即可验证。
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::dark(), move || {
            DataTable(&["Name".into(), "Math".into(), "Physics".into()],
                &[
                    vec!["Alice".into(), "90".into(), "88".into()],
                    vec!["Bob".into(), "85".into(), "92".into()],
                ],
                true,
                true,
                Modifier::new(),
            );
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(300.0, 200.0));
    assert!(!draw_tree(root).is_empty(), "DataTable 应产生绘制指令");
}
