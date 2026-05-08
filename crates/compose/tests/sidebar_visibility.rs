//! 回归用例：Sidebar 必须始终显示选中的 item。
//!
//! 1) 含选中项的分组即使处于「折叠」态也强制展开 —— 否则选中项被折叠隐藏。
//! 2) 选中项落在视口之外时，滚动偏移在布局期自动校正，使其进入可视区域。

use compose::foundation::Constraints;
use compose::layout::{
    Box, create_root_node, dispatch_click_at, draw_tree, hit_test, perform_layout, Alignment,
    DrawCmd, Modifier,
};
use compose::material::{MaterialTheme, ColorScheme, IconName};
use compose::runtime::{mutableStateOf, remember, Composer, Composition, Recomposer};
use compose_material::{Sidebar, SidebarSection};
use std::cell::Cell;
use std::rc::Rc;

fn texts(cmds: &[DrawCmd]) -> Vec<(String, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { y, text, .. } => Some((text.clone(), *y)),
            _ => None,
        })
        .collect()
}

/// 分组：A（两条目）+ B（一条目）。
fn sections() -> Vec<SidebarSection> {
    vec![
        SidebarSection::new("A")
            .item(IconName::Home, "A1")
            .item(IconName::Settings, "A2"),
        SidebarSection::new("B").item(IconName::Folder, "B1"),
    ]
}

/// 单分组多条目（用于滚动校正用例）。
fn many_items(n: usize) -> Vec<SidebarSection> {
    let mut s = SidebarSection::new("L");
    for i in 0..n {
        s = s.item(IconName::Home, format!("I{i}"));
    }
    vec![s]
}

// ── 1) 含选中项的分组强制展开 ─────────────────────────────────────────────────

#[test]
fn section_containing_selection_stays_expanded() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            // A 组折叠、B 组展开；选中项在 A 组内
            let sel = remember(|| mutableStateOf(0usize));
            let col = remember(|| mutableStateOf(vec![true, false]));
            let rail = remember(|| mutableStateOf(false));
            Sidebar(&sections(),
                sel,
                col,
                rail,
                Modifier::new().fill_max_width().height(200.0),
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));

    let t = texts(&draw_tree(root));
    let names: Vec<&str> = t.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        names.contains(&"A1"),
        "选中项所在分组应强制展开，否则选中项不可见: {names:?}"
    );
    assert!(
        names.contains(&"A2"),
        "含选中项的分组应完整展开: {names:?}"
    );
}

/// 折叠状态仍作用于**不含**选中项的分组。
#[test]
fn unrelated_section_remains_collapsed() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            // 选中 B1（A 组折叠生效）
            let sel = remember(|| mutableStateOf(2usize));
            let col = remember(|| mutableStateOf(vec![true, false]));
            let rail = remember(|| mutableStateOf(false));
            Sidebar(&sections(),
                sel,
                col,
                rail,
                Modifier::new().fill_max_width().height(200.0),
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));

    let t = texts(&draw_tree(root));
    let names: Vec<&str> = t.iter().map(|(s, _)| s.as_str()).collect();
    assert!(names.contains(&"B1"), "选中项应可见: {names:?}");
    assert!(
        !names.contains(&"A1"),
        "与选中项无关的折叠分组应保持折叠: {names:?}"
    );
}

// ── 2) 选中项滚入视口 ───────────────────────────────────────────────────────

#[test]
fn selected_item_is_scrolled_into_view() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let sel = remember(|| mutableStateOf(19usize)); // 末尾条目
            let col = remember(|| mutableStateOf(Vec::<bool>::new()));
            let rail = remember(|| mutableStateOf(false));
            Box(Modifier::new().size(220.0, 120.0), Alignment::TopStart, move || {
                Sidebar(&many_items(20),
                    sel,
                    col,
                    rail,
                    Modifier::new().fill_max_size(),
                );
            });
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));

    let t = texts(&draw_tree(root));
    let sel_y = t
        .iter()
        .find(|(s, _)| s == "I19")
        .map(|(_, y)| *y)
        .expect("选中条目应被绘制");
    assert!(
        (0.0..120.0).contains(&sel_y),
        "选中条目应被滚动进视口（0..120），实际 y = {sel_y}"
    );
}

// ── 3) 折叠含选中项的分组 → 选中迁移到其它可见条目 ───────────────────────────

#[test]
fn collapsing_selection_section_moves_selection_out() {
    let sel_out: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let so = sel_out.clone();
    comp.set_content(move || {
        let so2 = so.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let sel = remember(|| mutableStateOf(0usize)); // 选中 A1（A 组内）
            let col = remember(|| mutableStateOf(vec![false, false]));
            let rail = remember(|| mutableStateOf(false));
            so2.set(Some(sel.get()));
            Sidebar(&sections(),
                sel,
                col,
                rail,
                Modifier::new().fill_max_width().height(200.0),
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));

    // 点击 A 组标题行（y 0..28）→ 折叠 A 组
    let (target, lx, ly) = hit_test(root, 10.0, 10.0).expect("应命中分组标题行");
    assert!(dispatch_click_at(target, lx, ly));
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(300.0, 300.0));

    assert_eq!(
        sel_out.get(),
        Some(2),
        "折叠含选中项的分组时，选中应迁移到其它可见条目（B1）"
    );
    let t = texts(&draw_tree(root));
    let names: Vec<&str> = t.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        !names.contains(&"A1"),
        "折叠后 A 组条目应隐藏: {names:?}"
    );
    assert!(names.contains(&"B1"), "选中项应仍可见: {names:?}");
}

// ── 4) 图标条（rail）模式：只显示图标、隐藏文字标签 ────────────────────────────

#[test]
fn rail_mode_hides_labels() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let sel = remember(|| mutableStateOf(0usize));
            let col = remember(|| mutableStateOf(Vec::<bool>::new()));
            let rail = remember(|| mutableStateOf(true));
            Sidebar(&sections(),
                sel,
                col,
                rail,
                Modifier::new().fill_max_width().height(200.0),
            );
        });
    });
    perform_layout(root, Constraints::tight(80.0, 200.0));

    let cmds = draw_tree(root);
    let t = texts(&cmds);
    let names: Vec<&str> = t.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        !names.contains(&"A1") && !names.contains(&"B1"),
        "rail 模式不应绘制条目文字标签: {names:?}"
    );
    assert!(
        !names.contains(&"A") && !names.contains(&"B"),
        "rail 模式分组标题应退化为分隔线、不绘制文字: {names:?}"
    );
}
