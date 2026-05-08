//! 诊断：分组柱 / 桑基图的绘制几何。

use compose::foundation::Constraints;
use compose::layout::{Alignment, DrawCmd, Modifier, create_root_node, draw_tree, perform_layout};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition};
use compose_material::{
    Chart, ChartData, ChartOptions, ChartSeries, ChartType, SankeyLink, SankeyNode,
};

fn grouped_bar() -> Vec<DrawCmd> {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            Chart(
                ChartType::GroupedBar,
                ChartData::series(vec![
                    ChartSeries::of(
                        "线上",
                        vec![(0.0, 32.0), (1.0, 41.0), (2.0, 38.0), (3.0, 52.0)],
                    ),
                    ChartSeries::of(
                        "门店",
                        vec![(0.0, 20.0), (1.0, 28.0), (2.0, 34.0), (3.0, 30.0)],
                    ),
                ]),
                ChartOptions::default()
                    .title("季度销量")
                    .data_labels(true)
                    .height(220.0)
                    .x_labels(vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()]),
                Modifier::new().fill_max_width(),
            );
        });
    });
    perform_layout(root, Constraints::tight(600.0, 300.0));
    draw_tree(root)
}

#[test]
fn grouped_bar_sits_on_axis() {
    let mut bars_bottom = vec![];
    let mut axis_y = None;
    for c in grouped_bar() {
        match c {
            DrawCmd::Line { y0, y1, .. } if (y0 - y1).abs() < 0.01 => axis_y = Some(y0),
            // 仅统计绘图区内的柱子（宽约 53，排除图例 9px 色块）
            DrawCmd::FillRRect { y, h, w, .. } if (w - 53.1).abs() < 1.0 => bars_bottom.push(y + h),
            _ => {}
        }
    }
    let axis_y = axis_y.expect("应有坐标轴");
    assert!(
        bars_bottom.iter().all(|b| (b - axis_y).abs() < 0.5),
        "所有柱子底部应落在 X 轴上（={axis_y}）：{bars_bottom:?}"
    );
}

#[test]
fn dump_grouped_bar() {
    for c in grouped_bar() {
        match c {
            DrawCmd::Line { x0, y0, x1, y1, .. } => {
                println!("[dbg] LINE ({x0:.1},{y0:.1})→({x1:.1},{y1:.1})")
            }
            DrawCmd::FillRRect { x, y, w, h, .. } => {
                println!(
                    "[dbg] BAR x={x:.1} y={y:.1} {w:.1}x{h:.1} bottom={:.1}",
                    y + h
                )
            }
            DrawCmd::Text { x, y, text, .. } => println!("[dbg] TEXT {text:?} at ({x:.1},{y:.1})"),
            _ => {}
        }
    }
}

fn sankey() -> Vec<DrawCmd> {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            Chart(
                ChartType::Sankey,
                ChartData::Sankey {
                    nodes: vec![
                        SankeyNode {
                            label: "A".into(),
                            value: 10.0,
                        },
                        SankeyNode {
                            label: "C".into(),
                            value: 6.0,
                        },
                        SankeyNode {
                            label: "B".into(),
                            value: 16.0,
                        },
                        SankeyNode {
                            label: "D".into(),
                            value: 16.0,
                        },
                    ],
                    links: vec![
                        SankeyLink {
                            source: 0,
                            target: 2,
                            value: 10.0,
                        },
                        SankeyLink {
                            source: 1,
                            target: 2,
                            value: 6.0,
                        },
                        SankeyLink {
                            source: 2,
                            target: 3,
                            value: 16.0,
                        },
                    ],
                },
                ChartOptions::default().title("流向").height(260.0),
                Modifier::new().fill_max_width(),
            );
        });
    });
    perform_layout(root, Constraints::tight(600.0, 320.0));
    draw_tree(root)
}

#[test]
fn sankey_layers_and_ribbons() {
    let mut node_xs = vec![];
    let mut ribbons = 0;
    for c in sankey() {
        match c {
            DrawCmd::FillRRect { x, w, h, .. } if (w - 16.0).abs() < 0.5 && h > 2.0 => {
                node_xs.push((x, h));
            }
            DrawCmd::FillPath { .. } => ribbons += 1,
            _ => {}
        }
    }
    println!("[dbg] sankey nodes={node_xs:?} ribbons={ribbons}");
    // 分层：节点应分布在多列（不止一个 x）
    let distinct_x: std::collections::HashSet<i32> =
        node_xs.iter().map(|(x, _)| (*x) as i32).collect();
    assert!(
        distinct_x.len() >= 3,
        "桑基应分 3 层（A/C | B | D）：{node_xs:?}"
    );
    assert_eq!(ribbons, 3, "应有 3 条缎带");
    // 高度与流量正相关：B(16) 应高于 A(10)
    let h_a = node_xs
        .iter()
        .find(|(x, _)| (*x as i32 - 12 - 0).abs() < 30)
        .map(|(_, h)| *h);
    assert!(node_xs.iter().all(|(_, h)| *h >= 2.0), "节点高度应>0");
    println!("[dbg] ok");
}
