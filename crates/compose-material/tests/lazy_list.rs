//! 虚拟列表回归：只组合可见区间、滚动切换区间、总尺寸恒定、keyed 状态隔离。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, create_root_node, dispatch_click_at, dispatch_scroll, draw_tree,
    perform_layout, scroll_metrics, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, MutableState, Recomposer, advance_frame, mutableStateOf, remember};
use compose_material::{LazyColumn, LazyRow};

fn boot(
    count: usize,
    extent: f32,
    vertical: bool,
) -> (compose::layout::NodeId, Rc<Cell<usize>>, MutableState<usize>) {
    set_viewport(400.0, 200.0);
    let root = create_root_node(Alignment::TopStart);
    let composed = Rc::new(Cell::new(0usize));
    let c = composed.clone();
    let c1 = c.clone();
    let root_c = root;
    let cap: Rc<Cell<Option<MutableState<usize>>>> = Rc::new(Cell::new(None));
    let cap2 = cap.clone();
    let comp = Composition::new(root);
    comp.set_content(move || {
        let c = c1.clone();
        let cap_c = cap2.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let m = Modifier::new().width(300.0).height(200.0);
            let c = c.clone();
            let selected = remember(|| mutableStateOf(0usize));
            cap_c.set(Some(selected));
            let item = move |i: usize| {
                c.set(c.get() + 1);
                compose_material::Text(format!("I{i}"), compose::layout::TextStyle::new(), Modifier::new());
            };
            if vertical {
                LazyColumn(m, count, selected, extent, item);
            } else {
                LazyRow(m, count, selected, extent, item);
            }
        });
    });
    perform_layout(root_c, Constraints::tight(400.0, 300.0));
    advance_frame(1.0 / 60.0);
    Recomposer::recompose_pending();
    perform_layout(root_c, Constraints::tight(400.0, 300.0));
    let selected = cap.get().expect("应捕获 selected 状态");
    (root_c, composed, selected)
}

fn visible_items(root: compose::layout::NodeId) -> Vec<usize> {
    draw_tree(root)
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } if text.starts_with('I') => {
                text[1..].parse::<usize>().ok()
            }
            _ => None,
        })
        .collect()
}

#[test]
fn lazy_column_only_composes_visible_items() {
    let count = 1000;
    let (root, composed, _selected) = boot(count, 40.0, true);
    let items = visible_items(root);
    assert!(!items.is_empty(), "应组合可见 item");
    assert!(items.len() < 20, "1000 项不应全量组合，实际绘制 {} 项", items.len());
    // 帧驱动只组合可见区间（含 overscan），累计组合数远小于 count。
    assert!(composed.get() < 30, "累计组合 {} 次，未做到按需", composed.get());
}

#[test]
fn lazy_column_scroll_switches_visible_range() {
    let count = 1000;
    let (root, _composed, _selected) = boot(count, 40.0, true);
    let before = visible_items(root);
    let lo = *before.iter().min().unwrap();
    let hi = *before.iter().max().unwrap();

    let list_id = find_scroll_node(root, true);
    assert!(dispatch_scroll(Some(list_id), 400.0), "滚动应被消费");
    advance_frame(1.0 / 60.0);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    let after = visible_items(root);
    let lo2 = *after.iter().min().unwrap();
    assert!(lo2 > lo, "下滚后首项索引应增大: {lo} -> {lo2}");
    assert!(after.iter().all(|i| *i >= lo2 && *i <= lo2 + 15), "可见区间应紧凑");
    let _ = hi;
}

#[test]
fn lazy_column_total_extent_stays_constant() {
    let count = 1000;
    let (root, _composed, _selected) = boot(count, 40.0, true);
    let list_id = find_scroll_node(root, true);
    let (off, content, viewport) = scroll_metrics(list_id).expect("应有滚动指标");
    assert_eq!(off, 0.0);
    assert!((content - count as f32 * 40.0).abs() < 1.0, "总高度应为 count*extent: {content}");
    assert!(viewport > 0.0);
}

#[test]
fn lazy_row_only_composes_visible_items() {
    let (root, composed, _selected) = boot(1000, 60.0, false);
    let items = visible_items(root);
    assert!(!items.is_empty());
    assert!(items.len() < 20, "横向也应只组合可见项，实际 {} 项", items.len());
    assert!(composed.get() < 30);
}

#[test]
fn clicking_item_selects_and_highlights() {
    let (root, _composed, selected) = boot(1000, 40.0, true);
    assert_eq!(selected.get(), 0, "初始选中首项");
    let items = visible_items(root);
    let target_i = items[1]; // 点第二个可见项（避免首项带来的视觉歧义）

    // 找到 I{target_i} 文本的绘制坐标，在其上点击（整行 on_click 沿祖先链命中）。
    let (x, y) = draw_tree(root)
        .iter()
        .find_map(|c| match c {
            DrawCmd::Text { text, x, y, .. }
                if text == &format!("I{target_i}") =>
            {
                Some((x + 4.0, y + 4.0))
            }
            _ => None,
        })
        .expect("目标项应被绘制");
    let hit = compose::layout::hit_test(root, x, y).expect("应命中目标行");
    assert!(dispatch_click_at(hit.0, x, y), "行点击应被消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));

    assert_eq!(selected.get(), target_i, "点击应选中该项");
    // 选中高亮：绘制命令里应出现 secondary_container 背景矩形（非透明）。
    let colors = compose::material::theme_colors();
    let has_highlight = draw_tree(root).iter().any(|c| match c {
        DrawCmd::FillRect { color, .. } | DrawCmd::FillRRect { color, .. } => {
            *color == colors.secondary_container
        }
        _ => false,
    });
    assert!(has_highlight, "选中行应以 secondary_container 高亮");
}

fn find_scroll_node(root: compose::layout::NodeId, vertical: bool) -> compose::layout::NodeId {
    // 从根 DFS 找第一个匹配方向的滚动节点。
    fn dfs(id: compose::layout::NodeId, vertical: bool) -> Option<compose::layout::NodeId> {
        let (kind, kids) = compose::runtime::with_rt_pub(|rt| {
            let n = &rt.nodes[id.index()];
            (n.scroll.as_ref().map(|s| s.vertical), n.children.clone())
        });
        if kind == Some(vertical) {
            return Some(id);
        }
        kids.into_iter().find_map(|c| dfs(c, vertical))
    }
    dfs(root, vertical).expect("应找到滚动节点")
}
