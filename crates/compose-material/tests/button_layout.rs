//! 回归用例：按钮内容水平 / 垂直居中。

use compose::foundation::Constraints;
use compose::layout::{Alignment, TextStyle, create_root_node, perform_layout};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition, with_rt_pub};
use compose_material::{Button, Text};

/// 组装单个按钮，返回按钮节点 id。
fn setup() -> compose::layout::NodeId {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            Button(
                Modifier::new(),
                || {},
                move || {
                    Text("Button()", TextStyle::new(), Modifier::new());
                },
            );
        });
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));
    root
}

#[test]
fn button_content_is_centered() {
    let root = setup();
    let (bx, by, bw, bh, cx, cy, cw, chh) = with_rt_pub(|rt| {
        // 沿唯一孩子链下探到叶子（文本节点），其父即按钮节点
        let mut cur = root;
        let mut parent = root;
        while let Some(&next) = rt.nodes[cur.index()].children.first() {
            if rt.nodes[next.index()].children.is_empty() {
                parent = cur;
                cur = next;
                break;
            }
            parent = cur;
            cur = next;
        }
        let b = &rt.nodes[parent.index()];
        let t = &rt.nodes[cur.index()];
        (b.x, b.y, b.width, b.height, t.x, t.y, t.width, t.height)
    });
    assert!(
        (cw - 61.6).abs() < 2.0 && (chh - 19.6).abs() < 2.0,
        "应定位到文本叶子节点，实际 {cw}x{chh}"
    );
    // 文本中心应与按钮中心重合（按钮对称 padding 下，整体中心 == 内容区中心）
    let dx = (cx + cw / 2.0) - (bx + bw / 2.0);
    let dy = (cy + chh / 2.0) - (by + bh / 2.0);
    println!("button=({bx},{by},{bw}x{bh}) text=({cx},{cy},{cw}x{chh}) center err dx={dx} dy={dy}");
    assert!(dx.abs() < 0.5, "水平未居中：dx={dx}");
    assert!(dy.abs() < 0.5, "垂直未居中：dy={dy}");
}

use compose::layout::Modifier;
