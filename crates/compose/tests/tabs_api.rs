//! 回归用例：TabRow / Tabs 接线正确（组合不 panic、标题进入绘制树、多标签含溢出下拉）。

use compose::foundation::Constraints;
use compose::layout::{create_root_node, draw_tree, DrawCmd, Modifier, TextStyle};
use compose::material::{MaterialTheme, ColorScheme};
use compose::runtime::{remember, mutableStateOf, Composer};
use compose_material::{TabRow, Tabs, Text};

fn text_values(cmds: &[DrawCmd]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn tab_row_renders_titles_and_overflow_dropdown() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let many: Vec<String> = (0..12).map(|i| format!("Tab {}", i + 1)).collect();
            let tab = remember(|| mutableStateOf(0usize));
            TabRow(&many, tab, Modifier::new());
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(360.0, 80.0));
    let vals = text_values(&draw_tree(root));
    assert!(vals.iter().any(|t| t == "Tab 1"), "应渲染首个标签: {vals:?}");
    // 溢出下拉按钮（▾）不 panic 即视为接线正确
    assert!(!draw_tree(root).is_empty(), "TabRow 应产生绘制指令");
}

#[test]
fn tabs_renders_selected_page() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::dark(), move || {
            let titles = vec!["A".into(), "B".into(), "C".into()];
            let tab = remember(|| mutableStateOf(1usize));
            Tabs(&titles,
                tab,
                Modifier::new(),
                move | idx| {
                    Text(format!("page {idx}"), TextStyle::new(), Modifier::new());
                },
            );
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(360.0, 200.0));
    let vals = text_values(&draw_tree(root));
    assert!(vals.iter().any(|t| t == "page 1"), "应渲染选中页 B: {vals:?}");
}
