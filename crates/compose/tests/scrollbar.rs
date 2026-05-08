//! 回归用例：非移动平台滚动条（内容溢出时显示 + 拖拽改变滚动位置）。

use compose::foundation::Constraints;
use compose::foundation::Color;
use compose::layout::{
    create_root_node, draw_tree, hit_test, perform_layout, dispatch_scrollbar_down,
    scrollbar_end_global, scrollbar_move_global, Alignment, Arrangement, CrossAxisAlignment,
    DrawCmd, Modifier, NodeId, TextStyle,
};
use compose::material::{ColorScheme, MaterialTheme};

/// 当前主题下滚动条轨道/滑块的预期配色（与渲染层一致：on_surface 的 alpha）。
fn expected() -> (Color, Color) {
    let c = ColorScheme::light().on_surface;
    (c.with_alpha(0.16), c.with_alpha(0.42))
}
use compose::runtime::{Composition, Composer};
use compose_material::{Text, VerticalScroll};

/// 组装一个纵向滚动容器（指定行数与视口高度），返回 root。
fn setup(rows: usize, viewport_h: f32) -> NodeId {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            VerticalScroll(Modifier::new().fill_max_width().height(viewport_h),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    for i in 0..rows {
                        Text(format!("Row {i}"),
                            TextStyle::new(),
                            Modifier::new().height(20.0),
                        );
                    }
                },
            );
        });
    });
    root
}

/// 取滚动条滑块（特定配色 FillRRect）的 y。
fn scrollbar_thumb_y(cmds: &[DrawCmd]) -> Option<f32> {
    let thumb = expected().1;
    cmds.iter().find_map(|c| match c {
        DrawCmd::FillRRect { color, y, .. } if *color == thumb => Some(*y),
        _ => None,
    })
}

#[test]
fn scrollbar_shown_when_content_overflows() {
    let root = setup(20, 100.0); // 20×20=400 > 100 视口 → 溢出
    perform_layout(root, Constraints::tight(300.0, 400.0));
    let cmds = draw_tree(root);
    let (track, thumb) = expected();
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillRRect { color, .. } if *color == track)),
        "溢出时应绘制滚动条轨道"
    );
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillRRect { color, .. } if *color == thumb)),
        "溢出时应绘制滚动条滑块"
    );
}

#[test]
fn no_scrollbar_when_content_fits() {
    let root = setup(2, 400.0); // 2×20=40 < 400 视口 → 不溢出
    perform_layout(root, Constraints::tight(300.0, 400.0));
    let cmds = draw_tree(root);
    let (track, thumb) = expected();
    assert!(
        !cmds.iter().any(|c| matches!(c, DrawCmd::FillRRect { color, .. } if *color == track)),
        "内容未溢出不应绘制滚动条轨道"
    );
    assert!(
        !cmds.iter().any(|c| matches!(c, DrawCmd::FillRRect { color, .. } if *color == thumb)),
        "内容未溢出不应绘制滚动条滑块"
    );
}

#[test]
fn scrollbar_drag_scrolls_content() {
    let root = setup(20, 100.0);
    perform_layout(root, Constraints::tight(300.0, 400.0));
    let ty0 = scrollbar_thumb_y(&draw_tree(root)).expect("溢出时应有滑块");

    // 命中右下缘滚动条区域（视口宽 300，右缘 strip 在 [288,300]）
    let (target, _, _) = hit_test(root, 10.0, 10.0).expect("应命中列表内节点");
    assert!(
        dispatch_scrollbar_down(target, 295.0, 50.0),
        "右缘应命中并开启滚动条拖拽"
    );
    // 向下拖拽 100px：ratio = content/viewport = 400/100 = 4 → offset 400，钳到 300（最大）
    assert!(scrollbar_move_global(295.0, 150.0), "拖拽应被消费");
    assert!(scrollbar_end_global(), "抬起应结束拖拽");

    let ty1 = scrollbar_thumb_y(&draw_tree(root)).expect("溢出时应有滑块");
    assert!(ty1 > ty0, "向下拖拽滑块应下移（ty0={ty0}, ty1={ty1}）");
}

/// 复现 story「无显式高度、由父级约束」的嵌套滚动场景。
#[test]
fn scrollbar_in_nested_bounded_column() {
    use compose::layout::{Box, Column, Row};
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            Column(Modifier::new().fill_max_size(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    // 模拟 story 的外层 Box(weight(1,true)) + 内 Column(fill_max_size) + Row(fill_max_size)
                    Box(Modifier::new().weight(1.0, true),
                        Alignment::TopStart,
                        move || {
                            Column(Modifier::new().fill_max_size(),
                                Arrangement::Start,
                                CrossAxisAlignment::Stretch,
                                move || {
                                    Row(Modifier::new().fill_max_size(),
                                        Arrangement::Start,
                                        CrossAxisAlignment::Stretch,
                                        move || {
                                            Column(Modifier::new().width(220.0).fill_max_height(),
                                                Arrangement::Start,
                                                CrossAxisAlignment::Stretch,
                                                move || {
                                                    Text("sidebar", TextStyle::new(), Modifier::new());
                                                },
                                            );
                                            VerticalScroll(Modifier::new().weight(1.0, true).fill_max_height().padding_hv(16.0, 12.0),
                                                Arrangement::Start,
                                                CrossAxisAlignment::Stretch,
                                                move || {
                                                    for i in 0..40 {
                                                        Text(format!("Row {i}"), TextStyle::new(), Modifier::new().height(30.0));
                                                    }
                                                },
                                            );
                                        },
                                    );
                                },
                            );
                        },
                    );
                },
            );
        });
    });
    perform_layout(root, Constraints::tight(600.0, 500.0));
    let cmds = draw_tree(root);
    let thumb = expected().1;
    let has = cmds.iter().any(|c| matches!(c, DrawCmd::FillRRect { color, .. } if *color == thumb));
    println!("NESTED thumb present = {}", has);
    assert!(has, "嵌套受约束高度下溢出应显示滚动条");
}
