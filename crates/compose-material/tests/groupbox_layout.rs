//! 回归用例：GroupBox fieldset 语义 —— 顶边框线从标题文字后方穿过。
//!
//! 几何不变式：标题矩形（含 `Surface` 遮线底色）的**垂直中心**必须与内容盒
//! 顶边框线重合；标题上半在盒外、下半侵入盒内。

use compose::foundation::Constraints;
use compose::layout::{Alignment, TextStyle, create_root_node, perform_layout};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition, with_rt_pub};
use compose::layout::Modifier;
use compose_material::{GroupBox, Text};

fn setup() -> compose::layout::NodeId {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            GroupBox("Personal Info", Modifier::new(), || {
                Text("Name: Rui User", TextStyle::new(), Modifier::new());
            });
        });
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));
    root
}

/// 返回 (标题 y/h/x, 内容盒 y/x, 外层 y)。
fn probe(root: compose::layout::NodeId) -> (f32, f32, f32, f32, f32, f32) {
    with_rt_pub(|rt| {
        // root 下可能存在 MaterialTheme 等单链包装节点：沿单孩子链下探，
        // 直到找到 GroupBox 外层 Box（内容盒 + 标题 ≥2 个孩子）。
        let mut cur = root;
        while rt.nodes[cur.index()].children.len() == 1 {
            cur = rt.nodes[cur.index()].children[0];
        }
        let kids = &rt.nodes[cur.index()].children;
        assert!(kids.len() >= 2, "GroupBox 应有内容盒与标题两个孩子");
        // 组合顺序：先内容盒，后标题（标题覆盖在边框线之上）
        let content_box = &rt.nodes[kids[0].index()];
        let title = &rt.nodes[kids[kids.len() - 1].index()];
        let outer = &rt.nodes[cur.index()];
        (
            title.y,
            title.height,
            title.x,
            content_box.y,
            content_box.x,
            outer.y,
        )
    })
}

#[test]
fn groupbox_title_center_sits_on_top_border() {
    let root = setup();
    let (ty, th, tx, box_y, _box_x, outer_y) = probe(root);
    println!("title=({tx},{ty},{th}) content_box_top={box_y} outer_top={outer_y}");
    let center = ty + th / 2.0;
    assert!(
        (center - box_y).abs() < 0.5,
        "标题垂直中心 ({center}) 应落在顶边框线上 ({box_y})"
    );
    assert!(ty < box_y - 1.0, "标题上半应越出内容盒顶边 ({ty} < {box_y})");
    assert!(
        ty + th > box_y + 1.0,
        "标题下半应侵入内容盒内部（{} > {box_y}）",
        ty + th
    );
}

#[test]
fn groupbox_title_aligns_with_content_padding() {
    let root = setup();
    let (_, _, tx, _, box_x, _) = probe(root);
    // 标题左缘 = 内容盒左缘 + 内容盒水平内边距（SPACE_LG = 16）
    assert!(
        (tx - box_x - 16.0).abs() < 0.5,
        "标题应与内容盒左内边距对齐：title.x={tx} box.x={box_x}"
    );
}

#[test]
fn groupbox_reserves_space_above_for_title() {
    let root = setup();
    // 外层（含 padding_top 半行）应高于内容盒：上沿预留标题上半。
    let (ty, th, _, box_y, box_x, outer_y) = probe(root);
    let (outer_h, box_h) = with_rt_pub(|rt| {
        let mut cur = root;
        while rt.nodes[cur.index()].children.len() == 1 {
            cur = rt.nodes[cur.index()].children[0];
        }
        let kids = &rt.nodes[cur.index()].children;
        let outer = &rt.nodes[cur.index()];
        let cb = &rt.nodes[kids[0].index()];
        (outer.height, cb.height)
    });
    assert!(
        (box_y - outer_y - 10.0).abs() < 0.5,
        "外层应预留半行（10）给标题上半：outer_y={outer_y} box_y={box_y}"
    );
    assert!(
        (outer_h - (box_y - outer_y) - box_h).abs() < 0.5,
        "外层高度 = 预留 + 内容盒高度：outer_h={outer_h} box_h={box_h}"
    );
    let _ = (ty, th, box_x);
}
