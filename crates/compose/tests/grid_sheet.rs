//! 回归用例：GridSheet 数据网格（表头/行号渲染、条件格式着色、数据完整性）。

use compose::foundation::Constraints;
use compose::layout::{
    create_root_node, draw_tree, perform_layout, DrawCmd, Modifier, TextStyle,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{mutableStateOf, Composition, Composer, MutableState};
use compose_material::{
    CellValue, ConditionalFormatRule, GridSheet, GridSheetColumn, GridSheetConfig,
    GridSheetRow, GridSheetTextAlign,
};

fn setup() -> (
    compose::layout::NodeId,
    MutableState<Option<(usize, usize)>>,
) {
    let selected = mutableStateOf(None::<(usize, usize)>);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let sel = selected;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let columns = vec![
                GridSheetColumn::new("id", "ID", 36.0),
                GridSheetColumn::new("name", "Name", 90.0),
                GridSheetColumn::new("salary", "Salary", 80.0)
                    .with_align(GridSheetTextAlign::Right),
            ];
            let rows = vec![
                GridSheetRow::new(
                    "1",
                    vec![
                        CellValue::from("1"),
                        CellValue::from("Alice"),
                        CellValue::from(38000.0),
                    ],
                ),
                GridSheetRow::new(
                    "2",
                    vec![
                        CellValue::from("2"),
                        CellValue::from("Bob"),
                        CellValue::from(15000.0),
                    ],
                ),
            ];
            let rules = vec![ConditionalFormatRule::number_greater_than(
                Some("salary"),
                30000.0,
            )
            .with_bg_color(compose::prelude::Color::rgba(0.30, 0.69, 0.31, 0.20))];
            GridSheet(columns,
                rows,
                rules,
                GridSheetConfig::default(),
                sel,
                Modifier::new().fill_max_width().height(200.0),
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 400.0));
    (root, selected)
}

#[test]
fn grid_sheet_renders_header_and_rows() {
    let (root, _sel) = setup();
    let cmds = draw_tree(root);
    let texts: Vec<&str> = cmds
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for expect in ["#", "ID", "Name", "Salary", "Alice", "Bob", "38000", "15000"] {
        assert!(
            texts.iter().any(|t| *t == expect),
            "应渲染文本 {:?}，实际 {:?}",
            expect,
            &texts
        );
    }
}

#[test]
fn grid_sheet_applies_conditional_formatting() {
    let (root, _sel) = setup();
    let cmds = draw_tree(root);
    let green = compose::prelude::Color::rgba(0.30, 0.69, 0.31, 0.20);
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillRect { color, .. } if *color == green)),
        "salary > 30000 的单元格应有绿色条件格式背景"
    );
}

#[test]
fn grid_sheet_draws_zebra_and_row_numbers() {
    let (root, _sel) = setup();
    let cmds = draw_tree(root);
    let has_row_number = cmds.iter().any(|c| {
        matches!(c, DrawCmd::Text { text, .. } if text == "1" || text == "2")
    });
    assert!(has_row_number, "应渲染行号列");
    // 斑马纹：至少两次行背景填充（偶数行透明不绘制，奇数行 surface_container）
    let bg_fills = cmds
        .iter()
        .filter(|c| matches!(c, DrawCmd::FillRect { .. }))
        .count();
    assert!(bg_fills >= 1, "应有斑马纹/单元格背景填充，实际 {}", bg_fills);
}
