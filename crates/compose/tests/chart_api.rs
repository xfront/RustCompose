//! 回归用例：图表控件 API 接线正确（组合期不 panic、关键文本进入绘制树）。

use compose::foundation::Constraints;
use compose::layout::{create_root_node, draw_tree, DrawCmd, Modifier};
use compose::material::{MaterialTheme, ColorScheme};
use compose::runtime::Composer;
use compose_material::{BarChart, LineChart, PieChart};

fn text_names(cmds: &[DrawCmd]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn bar_chart_labels_values() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let labels = vec!["A".into(), "B".into(), "C".into(), "D".into(), "E".into()];
            BarChart(&[12.0, 28.0, 18.0, 34.0, 22.0], &labels, Modifier::new());
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(300.0, 220.0));
    let names = text_names(&draw_tree(root));
    assert!(names.iter().any(|t| t == "34"), "柱顶应标最大值 34: {names:?}");
    assert!(names.iter().any(|t| t == "A"), "x 轴应含标签 A: {names:?}");
}

#[test]
fn line_chart_composes() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            LineChart(&[4.0, 9.0, 6.0, 14.0, 11.0, 18.0], Modifier::new());
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(300.0, 220.0));
    let cmds = draw_tree(root);
    assert!(!cmds.is_empty(), "折线图应产生绘制指令");
    // 面积填充 → FillPath 指令存在
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillPath { .. })),
        "折线图应含面积填充 FillPath: {cmds:?}"
    );
}

#[test]
fn pie_chart_percent_and_legend() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let pal = vec![
                compose::foundation::Color::hex(0x6750A4),
                compose::foundation::Color::hex(0x625B71),
                compose::foundation::Color::hex(0x7D5260),
                compose::foundation::Color::hex(0x4A4458),
            ];
            PieChart(&[35.0, 25.0, 20.0, 20.0],
                &pal,
                Some(&["Alpha".into(), "Beta".into(), "Gamma".into(), "Delta".into()]),
                true,
                Modifier::new(),
            );
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(200.0, 260.0));
    let names = text_names(&draw_tree(root));
    assert!(names.iter().any(|t| t == "35%"), "饼图应标最大占比 35%: {names:?}");
    assert!(names.iter().any(|t| t == "Alpha"), "饼图应含图例 Alpha: {names:?}");
}
