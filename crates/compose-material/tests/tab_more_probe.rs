//! TabRow 溢出回归测试：
//! 1) 多标签时「⋮」按钮在不同宽度下都应显示；
//! 2) 从下拉菜单选中的标签应滚入视口可见（曾因 sticky 指向组合期瞬时节点而失效）。

use compose::foundation::Constraints;
use compose::layout::{
    Modifier, NodeId, create_root_node, dispatch_click_at, hit_test, node_abs_rect, perform_layout,
    set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, MutableState, Recomposer, advance_frame, mutableStateOf, with_rt_pub};
use compose_material::{OverlayHost, TabRow};

const ITEM_H: f32 = 48.0;

fn settle(root: NodeId, w: f32, h: f32, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, h));
    }
}

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

fn more_button(scroll: NodeId) -> Option<NodeId> {
    with_rt_pub(|rt| {
        let row = rt.nodes[scroll.index()].parent?;
        rt.nodes[row.index()]
            .children
            .iter()
            .copied()
            .find(|c| *c != scroll && ((rt.nodes[c.index()].width - 40.0).abs() < 1.0))
    })
}

fn boot(w: f32) -> (NodeId, MutableState<usize>) {
    set_viewport(w, 800.0);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);
    let tab: MutableState<usize> = mutableStateOf(0usize);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let many: Vec<String> = (0..12).map(|i| format!("Tab {}", i + 1)).collect();
                TabRow(&many, tab, Modifier::new().fill_max_width());
            });
        });
    });
    perform_layout(root, Constraints::tight(w, 800.0));
    std::mem::forget(comp); // 保活组合，避免 drop 取消调度
    (root, tab)
}

#[test]
fn probe_more_button_at_real_widths() {
    // 模拟 story 内容区宽度（去掉侧栏后大致宽度）
    for &w in &[400.0f32, 600.0, 700.0, 800.0] {
        let (root, _tab) = boot(w);
        settle(root, w, 800.0, 90);
        let (scroll, tabs, _off, vp) = strip_geometry(12);
        let total: f32 = tabs.iter().map(|(a, b)| b - a).sum();
        let more = more_button(scroll).is_some();
        println!("[probe] w={w} vp={vp:.0} 标签总宽={total:.0} 溢出={} ⋮显示={}", total > vp, more);
        if total > vp {
            assert!(more, "w={w} 有溢出但 ⋮ 未显示");
        }
    }
}

#[test]
fn probe_dropdown_select_scrolls_into_view() {
    let w = 400.0f32;
    let (root, tab) = boot(w);
    settle(root, w, 800.0, 90);
    let (scroll, _tabs, _off, _vp) = strip_geometry(12);
    let more = more_button(scroll).expect("应有 ⋮ 按钮");
    let (mx, my, mw, mh) = node_abs_rect(more);
    let (t, _, _) = hit_test(root, mx + mw / 2.0, my + mh / 2.0).expect("命中 ⋮");
    assert!(dispatch_click_at(t, mx + mw / 2.0, my + mh / 2.0));
    for _ in 0..30 {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(w, 800.0));
    }

    // 取溢出菜单首个隐藏标签项并点击。
    let (_sc, tabs, off, vp) = strip_geometry(12);
    let vis_r = off + vp;
    let hidden: Vec<usize> = (0..tabs.len()).filter(|&i| tabs[i].0 >= vis_r - 0.5).collect();
    assert!(!hidden.is_empty(), "应有右溢出隐藏标签");
    let want = hidden[0];

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
    let (it, _, _) = hit_test(root, ix + iw / 2.0, iy + ih / 2.0).expect("命中菜单项");
    dispatch_click_at(it, ix + iw / 2.0, iy + ih / 2.0);

    // 单步：重组（应设 sticky）→ 读 sticky → 布局 → 读 offset
    Recomposer::recompose_pending();
    let (sticky_after_comp, off_after_comp) = with_rt_pub(|rt| {
        let sc = (0..rt.nodes.len())
            .find(|&i| {
                let n = &rt.nodes[i];
                n.scroll.as_ref().map(|s| !s.vertical).unwrap_or(false) && n.children.len() == 12
            })
            .unwrap();
        let s = rt.nodes[sc].scroll.as_ref().unwrap();
        (s.sticky, s.offset)
    });
    println!("[probe] 重组后 sticky={sticky_after_comp:?} off={off_after_comp}");
    perform_layout(root, Constraints::tight(w, 800.0));
    let (_sc, _t, off_laid, _vp) = strip_geometry(12);
    println!("[probe] 布局后 off={off_laid}");
    settle(root, w, 800.0, 30);

    println!("[probe] 选中标签 tab.get()={}", tab.get());
    assert_eq!(tab.get(), want, "点溢出菜单首项应选中对应隐藏标签");

    // 关键：选中后该标签是否已滚入视口可见。
    let (_sc2, tabs2, off2, vp2) = strip_geometry(12);
    let (x0, x1) = tabs2[want];
    let vis_l = off2;
    let vis_r = off2 + vp2;
    let visible = x0 < vis_r && x1 > vis_l;
    println!(
        "[probe] want={want} x=[{x0:.0},{x1:.0}] 视口=[{vis_l:.0},{vis_r:.0}] 滚入可见={visible}"
    );
    assert!(visible, "从下拉选中的隐藏标签应滚入视口显示");
}
