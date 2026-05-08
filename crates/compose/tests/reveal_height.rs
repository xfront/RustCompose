//! 回归用例：`.reveal_height(frac)`（Compose `expandVertically` 的测量语义）。
//!
//! 关键点：内容必须按**自然高度**测量 —— 用 `max_height` 动画会把约束传给内容，
//! 文字/行被压扁；`reveal_height` 只折算容器自身高度，配合 `.clip` 裁掉溢出。

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, Arrangement, CrossAxisAlignment, DrawCmd, Modifier, NodeId, Policy, add_node,
    draw_tree, perform_layout,
};
use compose::runtime::Composition;

fn bar_color(i: usize) -> Color {
    // 三条互不相同、也不与容器底色相同的纯色。
    match i {
        0 => Color::rgba(0.9, 0.1, 0.1, 1.0),
        1 => Color::rgba(0.1, 0.9, 0.1, 1.0),
        _ => Color::rgba(0.1, 0.1, 0.9, 1.0),
    }
}

const WRAP: Color = Color::rgba(0.5, 0.5, 0.5, 1.0);
const BAR_H: f32 = 20.0;

/// 容器底色矩形（radius=0 的 `background` 走 `FillRect`）的 (y, h)。
fn wrap_rect(cmds: &[DrawCmd]) -> (f32, f32) {
    rects(cmds).into_iter().find(|c| c.2 == WRAP).map(|c| (c.0, c.1)).expect("容器底色应被绘制")
}

/// 各内容条的 (y, h)，按颜色查找。
fn bar_rects(cmds: &[DrawCmd]) -> Vec<(f32, f32)> {
    let all = rects(cmds);
    (0..3)
        .map(|i| {
            let c = bar_color(i);
            all.iter()
                .find(|r| r.2 == c)
                .map(|r| (r.0, r.1))
                .unwrap_or_else(|| panic!("第 {i} 条内容应被绘制"))
        })
        .collect()
}

fn rects(cmds: &[DrawCmd]) -> Vec<(f32, f32, Color)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::FillRect { y, h, color, .. } => Some((*y, *h, *color)),
            _ => None,
        })
        .collect()
}

/// 一棵「容器底色 + 3×20dp 内容条」的树，容器高度按 `frac` 展开。
fn one_reveal(frac: f32) -> NodeId {
    let root = compose::layout::create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        compose::layout::Column(
            Modifier::new().fill_max_width(),
            Arrangement::Start,
            CrossAxisAlignment::Stretch,
            move || {
                let wid = add_node(
                    Policy::Column {
                        arrangement: Arrangement::Start,
                        cross: CrossAxisAlignment::Stretch,
                    },
                    Modifier::new()
                        .fill_max_width()
                        .background(WRAP, 0.0)
                        .reveal_height(frac)
                        .clip(0.0),
                );
                compose::layout::with_content_scope(wid, move || {
                    for i in 0..3 {
                        add_node(
                            Policy::Leaf,
                            Modifier::new()
                                .fill_max_width()
                                .height(BAR_H)
                                .background(bar_color(i), 0.0),
                        );
                    }
                });
            },
        );
    });
    perform_layout(root, Constraints::new(0.0, 300.0, 0.0, 300.0));
    root
}

#[test]
fn reveal_half_shrinks_container_only() {
    let root = one_reveal(0.5);
    let cmds = draw_tree(root);
    let (wy, wh) = wrap_rect(&cmds);
    assert!(
        (wh - 30.0).abs() < 0.01,
        "容器高度应 = 0.5 × 自然 60 = 30，实得 {wh}"
    );
    assert!((wy - 0.0).abs() < 0.01, "容器仍从顶部开始落位");
    // 关键断言：内容条没被压扁（`max_height` 会把它们一起压成 10 高）。
    for (i, (by, bh)) in bar_rects(&cmds).into_iter().enumerate() {
        assert!(
            (bh - BAR_H).abs() < 0.01,
            "第 {i} 条内容应保持自然高度 {BAR_H}，实得 {bh}"
        );
        assert!((by - i as f32 * BAR_H).abs() < 0.01, "内容按自然位置依次排布");
    }
}

#[test]
fn reveal_full_and_zero_match_natural_and_collapsed() {
    let open = one_reveal(1.0);
    let (_, h) = wrap_rect(&draw_tree(open));
    assert!((h - 60.0).abs() < 0.01, "frac=1 即自然高度 60，实得 {h}");

    let shut = one_reveal(0.0);
    let (_, h0) = wrap_rect(&draw_tree(shut));
    assert!(h0.abs() < 0.01, "frac=0 容器收为 0 高（内容不再占位），实得 {h0}");
}
