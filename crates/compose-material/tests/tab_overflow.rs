//! 回归：TabRow 溢出下拉只列出「不在视口内」的标签（右溢出在上、左溢出在下、中间分隔条），
//! 且标签全部装得下时连「⋮」按钮都不出现。

use compose::foundation::Constraints;
use compose::layout::{
    Modifier, NodeId, create_root_node, dispatch_click_at, hit_test, node_abs_rect, perform_layout,
    set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, MutableState, Recomposer, advance_frame, mutableStateOf, with_rt_pub};
use compose_material::{OverlayHost, TabRow};

const SEP_H: f32 = 9.0;
const ITEM_H: f32 = 48.0;

/// 泵若干帧：推进帧时钟（跑一次性溢出帧驱动）→ 重组 → 布局，直到溢出判定稳定。
fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
}

/// 找到横向标签滚动容器（孩子数 == `n`），返回 (滚动节点, 各标签 [x, x+width], 偏移, 视口宽)。
fn strip_geometry(n: usize) -> (NodeId, Vec<(f32, f32)>, f32, f32) {
    with_rt_pub(|rt| {
        let sc = (0..rt.nodes.len())
            .find(|&i| {
                let node = &rt.nodes[i];
                node.scroll.as_ref().map(|s| !s.vertical).unwrap_or(false)
                    && node.children.len() == n
            })
            .expect("应有横向标签滚动容器");
        let sc_id = NodeId(sc as u32);
        let sn = &rt.nodes[sc];
        let off = sn.scroll.as_ref().unwrap().offset;
        let tabs: Vec<(f32, f32)> = sn
            .children
            .iter()
            .map(|c| {
                let t = &rt.nodes[c.index()];
                (t.x, t.x + t.width)
            })
            .collect();
        (sc_id, tabs, off, sn.width)
    })
}

/// 把横向滚动容器的偏移设为 `off`（模拟用户横向滚动，使视口两侧都可能出现溢出）。
fn set_offset(sc: NodeId, off: f32) {
    with_rt_pub(|rt| {
        if let Some(s) = rt.nodes[sc.index()].scroll.as_mut() {
            s.offset = off;
        }
    });
}

/// 标签条 Row 上除滚动容器外的兄弟（即「⋮」按钮）——存在则返回其节点。
fn more_button(scroll: NodeId) -> Option<NodeId> {
    with_rt_pub(|rt| {
        let row = rt.nodes[scroll.index()].parent?;
        rt.nodes[row.index()]
            .children
            .iter()
            .copied()
            .find(|c| {
                *c != scroll && ((rt.nodes[c.index()].width - 40.0).abs() < 1.0)
            })
    })
}

/// 打开后的浮层面板：竖向滚动容器，统计其 Item(高 48) / Sep(高 9) 行数。
fn open_menu_rows() -> (usize, usize) {
    with_rt_pub(|rt| {
        let panel = (0..rt.nodes.len())
            .find(|&i| {
                let node = &rt.nodes[i];
                if let Some(s) = &node.scroll {
                    if !s.vertical {
                        return false;
                    }
                    // 竖向滚动容器，孩子里既有菜单项又有（可能）分隔条。
                    return node.children.iter().any(|c| {
                        (rt.nodes[c.index()].height - ITEM_H).abs() < 1.0
                    });
                }
                false
            })
            .map(|i| NodeId(i as u32));
        let panel = match panel {
            Some(p) => p,
            None => return (0, 0),
        };
        let mut items = 0usize;
        let mut seps = 0usize;
        for c in &rt.nodes[panel.index()].children {
            let h = rt.nodes[c.index()].height;
            if (h - ITEM_H).abs() < 1.0 {
                items += 1;
            } else if (h - SEP_H).abs() < 1.0 {
                seps += 1;
            }
        }
        (items, seps)
    })
}

#[test]
fn overflow_dropdown_lists_only_hidden_tabs_with_separator() {
    let (w, h) = (300.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let tab: MutableState<usize> = mutableStateOf(6usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let many: Vec<String> = (0..12).map(|i| format!("Tab {:02}", i)).collect();
                TabRow(&many, tab, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 60);

    // 横向滚动到中段，使视口左侧（已滚过）与右侧（未滚到）同时出现溢出。
    let (scroll, tabs0, _off0, vp0) = strip_geometry(12);
    set_offset(scroll, tabs0[6].0 - vp0 / 2.0);
    settle(root, w, h, 60);

    // 「⋮」按钮应出现（有溢出）。
    let (scroll, tabs, off, vp) = strip_geometry(12);
    let more = more_button(scroll).expect("标签装不下时应有 ⋮ 溢出按钮");

    // 点 ⋮ 打开下拉。
    let (mx, my, mw, mh) = node_abs_rect(more);
    let (target, _, _) = hit_test(root, mx + mw / 2.0, my + mh / 2.0).expect("应命中 ⋮");
    assert!(dispatch_click_at(target, mx + mw / 2.0, my + mh / 2.0));
    settle(root, w, h, 30);

    // 溢出（完全在视口 [off, off+vp] 之外）的标签数；左右各自存在。
    let vis_l = off;
    let vis_r = off + vp;
    let right = tabs.iter().filter(|(x0, _)| *x0 >= vis_r - 0.5).count();
    let left = tabs.iter().filter(|(_, x1)| *x1 <= vis_l + 0.5).count();
    let hidden = right + left;
    let (items, seps) = open_menu_rows();
    assert!(right > 0 && left > 0, "此布局应左右都溢出：right={right} left={left}");
    assert_eq!(items, hidden, "应只列出不在视口内的标签：items={items} hidden={hidden}");
    assert_eq!(seps, 1, "左右两段之间应恰有一条分隔条");
}

#[test]
fn clicking_overflow_item_jumps_to_a_hidden_tab() {
    let (w, h) = (300.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let tab: MutableState<usize> = mutableStateOf(6usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let many: Vec<String> = (0..12).map(|i| format!("Tab {:02}", i)).collect();
                TabRow(&many, tab, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 60);
    let (scroll, tabs0, _o0, vp0) = strip_geometry(12);
    set_offset(scroll, tabs0[6].0 - vp0 / 2.0);
    settle(root, w, h, 60);

    let (scroll, _tabs, _off, _vp) = strip_geometry(12);
    let more = more_button(scroll).expect("应有 ⋮ 按钮");
    let (mx, my, mw, mh) = node_abs_rect(more);
    let (t, _, _) = hit_test(root, mx + mw / 2.0, my + mh / 2.0).unwrap();
    dispatch_click_at(t, mx + mw / 2.0, my + mh / 2.0);
    settle(root, w, h, 30);

    // 当前隐藏标签索引集合（右段在前，顶项 = 右溢出第一个）。
    let (_sc, tabs, off, vp) = strip_geometry(12);
    let (_vis_l, vis_r) = (off, off + vp);
    let right: Vec<usize> = (0..tabs.len())
        .filter(|&i| tabs[i].0 >= vis_r - 0.5)
        .collect();
    assert!(!right.is_empty(), "应有右溢出标签");
    let want = right[0];

    // 命中菜单面板首个 item（在 with_rt 借用外取绝对矩形，避免 node_abs_rect 重入借用）。
    let first_item = with_rt_pub(|rt| {
        let panel = (0..rt.nodes.len())
            .find(|&i| {
                let node = &rt.nodes[i];
                node.scroll.as_ref().map(|s| s.vertical).unwrap_or(false)
                    && node
                        .children
                        .iter()
                        .any(|c| (rt.nodes[c.index()].height - ITEM_H).abs() < 1.0)
            })
            .map(|i| NodeId(i as u32))
            .expect("应有竖向菜单面板");
        rt.nodes[panel.index()]
            .children
            .iter()
            .copied()
            .find(|c| (rt.nodes[c.index()].height - ITEM_H).abs() < 1.0)
            .unwrap()
    });
    let (ix, iy, iw, ih) = node_abs_rect(first_item);
    let (it, _, _) = hit_test(root, ix + iw / 2.0, iy + ih / 2.0).expect("应命中菜单项");
    dispatch_click_at(it, ix + iw / 2.0, iy + ih / 2.0);
    Recomposer::recompose_pending();

    // 顶项 = 右溢出第一个隐藏标签 → 选中应跳到它。
    assert_eq!(tab.get(), want, "点溢出菜单顶项应选中对应的隐藏标签");
}

#[test]
fn no_overflow_hides_more_button() {
    let (w, h) = (800.0, 600.0);
    set_viewport(w, h);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let tab: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let few: Vec<String> = vec!["Alpha".into(), "Beta".into(), "Gamma".into()];
                TabRow(&few, tab, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, h));
    settle(root, w, h, 60);

    // 全部标签都装得下 → 不应有 ⋮ 按钮。
    let (scroll, _tabs, _off, _vp) = strip_geometry(3);
    assert!(
        more_button(scroll).is_none(),
        "无溢出时不应出现 ⋮ 按钮"
    );
}
