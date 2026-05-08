//! 回归用例：gallery 的 Sidebar / 下拉浮层三处布局与交互缺陷。
//!
//! 1) `Modifier.offset` 曾完全不参与落位 → 下拉菜单永远贴最左，不对齐所点菜单标签。
//! 2) 铺满父节点的装饰性 `Canvas`（聚焦外框）会先命中 → 吃掉条目的点击。
//! 3) 点击焦点容器内部的子项会清空焦点（焦点查找被 on_click 提前打断）。
//! 4) 条目内容用 `Box` 承载「图标 + 间隙 + 标签」→ 三者重叠；正确做法是 `Row`。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::Constraints;
use compose::layout::{
    Box, Canvas, Clickable, create_root_node, dispatch_click_at, draw_tree, focusable_full,
    hit_test, perform_layout, Row, Alignment, Arrangement, CrossAxisAlignment, DrawBuilder, DrawCmd,
    Modifier, TextStyle,
};
use compose::runtime::{focused_scope, Composition, NavKey, ScopeId};

fn label(txt: &'static str) {
    Canvas(Modifier::new(), move |b: &mut DrawBuilder, x, y, _w, _h| {
        b.draw_text(x, y, txt, &TextStyle::new().size(14.0), f32::INFINITY);
    });
}

fn text_xs(cmds: &[DrawCmd]) -> Vec<(String, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { x, text, .. } => Some((text.clone(), *x)),
            _ => None,
        })
        .collect()
}

// ── 1) offset 参与落位 ──────────────────────────────────────────────────────

#[test]
fn offset_modifier_is_applied_at_placement() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        Box(Modifier::new().size(400.0, 200.0), Alignment::TopStart, || {
            Box(Modifier::new().size(50.0, 20.0).offset(120.0, 30.0),
                Alignment::TopStart,
                || {
                    label("X");
                },
            );
        });
    });
    perform_layout(root, Constraints::new(0.0, 400.0, 0.0, 200.0));
    let cmds = draw_tree(root);
    let (_, x) = text_xs(&cmds).into_iter().find(|(t, _)| t == "X").expect("Text X");
    assert_eq!(x, 120.0, "offset 未参与落位 → 浮层永远贴最左");
}

/// offset 只移动绘制/命中位置，不改变测量尺寸（Compose 契约）。
#[test]
fn offset_does_not_affect_measurement() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        Box(Modifier::new().size(400.0, 200.0), Alignment::TopStart, || {
            Box(Modifier::new().size(50.0, 20.0).offset(120.0, 30.0),
                Alignment::TopStart,
                || {
                    label("X");
                },
            );
        });
    });
    let size = perform_layout(root, Constraints::new(0.0, 400.0, 0.0, 200.0));
    assert_eq!(size.width, 400.0);
    assert_eq!(size.height, 200.0);
}

// ── 2) 装饰性覆盖层不吃点击 ──────────────────────────────────────────────────

#[test]
fn full_size_overlay_passes_click_through_when_no_hit() {
    let hit = Rc::new(Cell::new(false));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let flag = hit.clone();
    comp.set_content(move || {
        let f = flag.clone();
        Box(Modifier::new().size(400.0, 200.0), Alignment::TopStart, move || {
            let f2 = f.clone();
            Clickable(Modifier::new().size(200.0, 40.0),
                move || f2.set(true),
                move || {
                    label("ITEM");
                },
            );
            // 与 gallery 聚焦外框同构：铺满整栏的纯描边 Canvas
            Canvas(Modifier::new().fill_max_size().no_hit(), |_b, _x, _y, _w, _h| {});
        });
    });
    perform_layout(root, Constraints::new(0.0, 400.0, 0.0, 200.0));

    let (target, lx, ly) = hit_test(root, 10.0, 10.0).expect("应命中条目");
    assert!(dispatch_click_at(target, lx, ly));
    assert!(hit.get(), "覆盖层吃掉了条目的点击");
}

/// 对照组：未标 no_hit 的覆盖层确实会挡住下方点击（证明 no_hit 是必要手段）。
#[test]
fn full_size_overlay_blocks_click_without_no_hit() {
    let hit = Rc::new(Cell::new(false));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let flag = hit.clone();
    comp.set_content(move || {
        let f = flag.clone();
        Box(Modifier::new().size(400.0, 200.0), Alignment::TopStart, move || {
            let f2 = f.clone();
            Clickable(Modifier::new().size(200.0, 40.0),
                move || f2.set(true),
                move || {
                    label("ITEM");
                },
            );
            Canvas(Modifier::new().fill_max_size(), |_b, _x, _y, _w, _h| {});
        });
    });
    perform_layout(root, Constraints::new(0.0, 400.0, 0.0, 200.0));
    let (target, lx, ly) = hit_test(root, 10.0, 10.0).expect("应命中覆盖层");
    let _ = dispatch_click_at(target, lx, ly);
    assert!(!hit.get(), "未标 no_hit 的覆盖层本应挡住点击");
}

// ── 3) 点内部子项不应夺走焦点容器的焦点 ──────────────────────────────────────

#[test]
fn click_inside_focusable_keeps_focus() {
    let clicked = Rc::new(Cell::new(false));
    let sb: Rc<Cell<Option<ScopeId>>> = Rc::new(Cell::new(None));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let (cf, cs) = (clicked.clone(), sb.clone());
    comp.set_content(move || {
        let f = cf.clone();
        let scope_out = cs.clone();
        Box(Modifier::new().size(400.0, 200.0), Alignment::TopStart, move || {
            let f2 = f.clone();
            let sid = focusable_full(Modifier::new().size(300.0, 200.0),
                || {},
                |_k: NavKey| {},
                None::<fn(char)>,
                move || {
                    let f3 = f2.clone();
                    Clickable(Modifier::new().size(200.0, 40.0),
                        move || f3.set(true),
                        move || {
                            label("ITEM");
                        },
                    );
                    Canvas(Modifier::new().fill_max_size().no_hit(), |_b, _x, _y, _w, _h| {});
                },
            );
            scope_out.set(Some(sid));
        });
    });
    perform_layout(root, Constraints::new(0.0, 400.0, 0.0, 200.0));

    let (target, lx, ly) = hit_test(root, 10.0, 10.0).expect("应命中条目");
    assert!(dispatch_click_at(target, lx, ly));
    assert!(clicked.get(), "条目的 on_click 未触发");
    assert_eq!(
        focused_scope(),
        sb.get(),
        "点击焦点容器内部子项后，焦点应保持在该容器上"
    );
}

// ── 4) 条目内容：Row 依次排布 vs Box 重叠 ────────────────────────────────────

#[test]
fn row_places_icon_and_label_side_by_side() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        Row(Modifier::new().size(300.0, 40.0),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            || {
                label("ICON");
                label("LABEL");
            },
        );
    });
    perform_layout(root, Constraints::new(0.0, 300.0, 0.0, 100.0));
    let xs = text_xs(&draw_tree(root));
    assert_eq!(xs.len(), 2);
    assert!(xs[1].1 > xs[0].1, "Row 应把标签排在图标之后，实际 {xs:?}");
}

#[test]
fn box_children_stack_by_design() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        Box(Modifier::new().size(200.0, 40.0), Alignment::TopStart, || {
            label("A");
            label("B");
        });
    });
    perform_layout(root, Constraints::new(0.0, 300.0, 0.0, 100.0));
    let xs = text_xs(&draw_tree(root));
    assert_eq!(xs.len(), 2);
    assert_eq!(xs[0].1, xs[1].1, "Box 语义就是重叠 —— 条目内容不该用 Box");
}
