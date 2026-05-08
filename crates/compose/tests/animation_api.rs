//! 回归用例：动画控件 API 接线正确（组合期不 panic、内容始终进入绘制树）。

use compose::foundation::Constraints;
use compose::layout::{Box, create_root_node, draw_tree, perform_layout, Alignment, DrawCmd, Modifier};
use compose::material::{MaterialTheme, ColorScheme};
use compose::runtime::{mutableStateOf, remember, Composer, Composition};
use compose_material::{
    AnimatedVisibility, OpenContainer,
};

fn text_names(cmds: &[DrawCmd]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn open_container_wires_both_states() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OpenContainer(false,
                Modifier::new().fill_max_width().height(64.0),
                move || {
                    Box(Modifier::new().fill_max_width().height(64.0),
                        Alignment::TopStart,
                        move || Text_placeholder("CLOSED"),
                    );
                },
                move || {
                    Box(Modifier::new().fill_max_width().height(64.0),
                        Alignment::TopStart,
                        move || Text_placeholder("OPENED"),
                    );
                },
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));
    let names = text_names(&draw_tree(root));
    assert!(names.contains(&"CLOSED".to_string()), "closed 内容应进入绘制树: {names:?}");
    assert!(names.contains(&"OPENED".to_string()), "opened 内容应进入绘制树: {names:?}");
}

#[test]
fn animated_visibility_wires_content() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let vis = remember(|| mutableStateOf(true));
            AnimatedVisibility(vis.get(),
                56.0,
                Modifier::new(),
                move || Text_placeholder("AV"),
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));
    let names = text_names(&draw_tree(root));
    assert!(names.contains(&"AV".to_string()), "可见内容应进入绘制树: {names:?}");
}

// 测试内联文本：复用 compose_material 的 Text
fn Text_placeholder(s: &str) {
    use compose::layout::TextStyle;
    compose_material::Text(s.to_string(), TextStyle::new(), Modifier::new());
}
