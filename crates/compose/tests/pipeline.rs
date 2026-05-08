//! 渲染管线冒烟测试 — 无窗口环境验证 valo DisplayList 构建（GPU 路径需真机验证）。

use compose::layout::{
    Column, create_root_node, draw_tree, perform_layout, Arrangement, CrossAxisAlignment,
    DrawCmd, Modifier, TextStyle,
};
use compose::material::{MaterialTheme, ColorScheme};
use compose::prelude::*;
use compose::runtime::{Composition, Composer};
use compose_material::{Button, Text};

#[test]
fn full_pipeline_produces_draw_commands() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let count = mutableStateOf(0i32);
    let count2 = count;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            Column(Modifier::new().size(400.0, 300.0).background(Color::WHITE, 0.0),
                Arrangement::Center,
                CrossAxisAlignment::Center,
                move || {
                    let n = count.get();
                    Text(format!("Count: {n}"), TextStyle::new().size(24.0), Modifier::new());
                    let count3 = count2;
                    Button(Modifier::new().width(120.0), move || count3.set(count3.get() + 1), || {
                        Text("Increment", TextStyle::new(), Modifier::new());
                    });
                },
            );
        });
    });

    // 布局（400x300 视口）
    let size = perform_layout(root, Constraints::new(0.0, 400.0, 0.0, 300.0));
    assert_eq!(size, Size::new(400.0, 300.0));

    // 绘制 → DrawCmd 序列（含背景、文本、按钮圆角矩形）
    let cmds = draw_tree(root);
    assert!(!cmds.is_empty(), "draw tree must produce commands");
    let has_text = cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::Text { .. }));
    let has_rrect = cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::FillRRect { .. }));
    assert!(has_text, "must contain Text command");
    assert!(has_rrect, "must contain rounded rect (Button)");

    // valo DisplayList 构建在 compose-desktop 的集成测试验证（需 GPU crate）；
    // 此处验证 DrawCmd 契约的平台无关性。
}
