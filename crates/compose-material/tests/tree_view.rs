//! 回归：`TreeView` 的节点必须竖向排布、互不重叠。
//!
//! 早期实现把 `tree_node` 直接挂在 `focusable_full`（Box 语义，子树重叠）下，
//! 导致所有根节点叠在一起。修复后根节点收进 `Column`，与 ListView/List 一致。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Modifier, NodeId, create_root_node, perform_layout, set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, MutableState, Recomposer, advance_frame, mutableStateOf, with_rt_pub};
use compose_material::{OverlayHost, TreeView, TreeNode};

fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
}

fn abs_rect(id: NodeId) -> (f32, f32, f32, f32) {
    with_rt_pub(|rt| {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(n) = cur {
            chain.push(n);
            cur = rt.nodes[n.index()].parent;
        }
        chain.reverse();
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        for i in 0..chain.len() {
            if i > 0 {
                let p = &rt.nodes[chain[i - 1].index()];
                if let Some(s) = p.scroll.as_ref() {
                    if s.vertical {
                        y -= s.offset;
                    } else {
                        x -= s.offset;
                    }
                }
            }
            let n = &rt.nodes[chain[i].index()];
            x += n.x;
            y += n.y;
        }
        let n = &rt.nodes[id.index()];
        (x, y, n.width, n.height)
    })
}

/// 行节点：带 `on_click`（点击选中/展开）但**不是** focusable 容器（无 `key_handler`）。
fn tree_rows() -> Vec<NodeId> {
    with_rt_pub(|rt| {
        let mut out = Vec::new();
        for (i, n) in rt.nodes.iter().enumerate() {
            if n.on_click.is_some() && n.key_handler.is_none() {
                out.push(NodeId(i as u32));
            }
        }
        out
    })
}

fn make_tree() -> Vec<TreeNode> {
    // A[B[D],C]  +  E[F]  —— key: A, A/B, A/B/D, A/C, E, E/F
    vec![
        TreeNode::with_children(
            "A",
            vec![
                TreeNode::with_children("B", vec![TreeNode::new("D")]),
                TreeNode::new("C"),
            ],
        ),
        TreeNode::with_children("E", vec![TreeNode::new("F")]),
    ]
}

fn boot_tree(expanded: Vec<String>) -> NodeId {
    let (w, h) = (400.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let exp: MutableState<Vec<String>> = mutableStateOf(expanded);
    let sel: MutableState<String> = mutableStateOf(String::new());
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                // 树在闭包内重建（与 list_row_click 的 boot 同构），避免跨层 move。
                let nodes = make_tree();
                let e = exp;
                let s = sel;
                TreeView(&nodes, e, s, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 6);
    std::mem::forget(comp);
    root
}

/// 断言给定布局下所有可见行节点互不重叠（y 严格递增且下一行起点 ≥ 本行底边）。
fn assert_no_overlap(expected: usize) {
    let mut rows = tree_rows();
    assert_eq!(rows.len(), expected, "可见行数量");
    rows.sort_by(|a, b| {
        let (_, ya, _, _) = abs_rect(*a);
        let (_, yb, _, _) = abs_rect(*b);
        ya.partial_cmp(&yb).unwrap()
    });
    for w in rows.windows(2) {
        let (_, y0, _, h0) = abs_rect(w[0]);
        let (_, y1, _, _) = abs_rect(w[1]);
        assert!(
            y1 >= y0 + h0 - 0.5,
            "TreeView 行不应重叠：上一行底 {:.1} 下一行顶 {:.1}",
            y0 + h0,
            y1
        );
    }
}

#[test]
fn treeview_expanded_items_do_not_overlap() {
    // 展开全部父节点（key 含路径前缀）→ 6 行全部可见，竖向不重叠。
    boot_tree(vec!["A".into(), "A/B".into(), "E".into()]);
    assert_no_overlap(6);
}

#[test]
fn treeview_collapsed_roots_do_not_overlap() {
    // 全部折叠 → 仅 2 个根节点可见，仍然不重叠。
    boot_tree(vec![]);
    assert_no_overlap(2);
}
