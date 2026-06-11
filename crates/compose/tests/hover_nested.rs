//! 嵌套悬停语义回归（对齐 Flutter MouseRegion）：
//! 进入父悬停区域的**子区域**不触发父区域 leave；从子区域直接离开父区域时，
//! 父区域的 leave 要补发。此前「最近命中者赢」导致工具条内的按钮一悬停，
//! 外层 AutoHideToolbar 就误触 leave → 展开态被 2s 收回。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Arrangement, Box as LayoutBox, Column, CrossAxisAlignment, Modifier, NodeId,
    create_root_node, dispatch_hover_at, hoverable, perform_layout,
};
use compose::runtime::{Composition, Recomposer, with_rt_pub};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Default)]
struct Counters {
    p_enter: Cell<u32>,
    p_leave: Cell<u32>,
    c_enter: Cell<u32>,
    c_leave: Cell<u32>,
}

/// 组合父(0..60, 含子区 0..20) + 母外区(60..100)；返回 (根, 计数器, 父节点, 子节点)。
fn boot() -> (NodeId, Rc<Counters>, NodeId, NodeId) {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let cnt: Rc<Counters> = Rc::default();
    // 单一 Rc 句柄贯穿所有闭包（字段经 Deref 访问，避开字段级 move 捕获）。
    let c0 = cnt.clone();
    comp.set_content(move || {
        let c = c0.clone();
        LayoutBox(Modifier::new().fill_max_size(), Alignment::TopStart, move || {
            let ch = c.clone(); // on_hover 闭包句柄
            let cc = c.clone(); // content 闭包句柄
            hoverable(
                Modifier::new().fill_max_width().height(60.0),
                move |enter| {
                    if enter {
                        ch.p_enter.set(ch.p_enter.get() + 1);
                    } else {
                        ch.p_leave.set(ch.p_leave.get() + 1);
                    }
                },
                move || {
                    let ch2 = cc.clone();
                    let cc2 = cc.clone();
                    Column(
                        Modifier::new().fill_max_size(),
                        Arrangement::Start,
                        CrossAxisAlignment::Stretch,
                        move || {
                            let ch3 = ch2.clone();
                            let cc3 = ch2.clone();
                            hoverable(
                                Modifier::new().fill_max_width().height(20.0),
                                move |enter| {
                                    if enter {
                                        ch3.c_enter.set(ch3.c_enter.get() + 1);
                                    } else {
                                        ch3.c_leave.set(ch3.c_leave.get() + 1);
                                    }
                                },
                                move || {
                                    LayoutBox(
                                        Modifier::new().fill_max_size(),
                                        Alignment::TopStart,
                                        || {},
                                    );
                                },
                            );
                            LayoutBox(
                                Modifier::new().fill_max_width().height(40.0),
                                Alignment::TopStart,
                                || {},
                            );
                            let _ = cc3;
                        },
                    );
                },
            );
            LayoutBox(
                Modifier::new().fill_max_width().height(40.0),
                Alignment::BottomStart,
                || {},
            );
        });
    });
    perform_layout(root, Constraints::tight(100.0, 100.0));
    // 按注册顺序找带 hover_handler 的节点：第一个 = 父，第二个 = 子。
    let mut parent = None;
    let mut child = None;
    with_rt_pub(|rt| {
        for (i, n) in rt.nodes.iter().enumerate() {
            if n.hover_handler.is_some() {
                if parent.is_none() {
                    parent = Some(NodeId(i as u32));
                } else if child.is_none() {
                    child = Some(NodeId(i as u32));
                }
            }
        }
    });
    (root, cnt, parent.expect("父悬停区应存在"), child.expect("子悬停区应存在"))
}

fn hover(target: NodeId, cnt: &Rc<Counters>, tag: &str) {
    let fired = dispatch_hover_at(target);
    eprintln!("[dbg] {tag}: dispatch={fired} p_enter={} p_leave={} c_enter={} c_leave={}",
        cnt.p_enter.get(), cnt.p_leave.get(), cnt.c_enter.get(), cnt.c_leave.get());
}

#[test]
fn entering_child_region_keeps_parent_entered() {
    let (root, c, p, ch) = boot();
    let _ = root;

    hover(p, &c, "进入父区域");
    assert_eq!(c.p_enter.get(), 1, "父区域应 enter");
    hover(ch, &c, "进入子区域");
    assert_eq!(c.c_enter.get(), 1, "子区域应 enter");
    assert_eq!(c.p_leave.get(), 0, "进入子区域不应触发父区域 leave");

    hover(p, &c, "回到父区域其它部分");
    assert_eq!(c.c_leave.get(), 1, "子区域应 leave");
    assert_eq!(c.p_leave.get(), 0, "仍在父区域内，不应 leave");
}

#[test]
fn leaving_child_region_directly_fires_parent_leave() {
    let (root, c, p, ch) = boot();
    let _ = root;

    hover(ch, &c, "进入子区域");
    hover(p, &c, "进入父区域（建立悬停）");
    // 直接跳到无悬停区域（根节点自身无 handler）→ 子区域 leave + 父区域 leave 补发。
    hover(root, &c, "离开到无悬停区");
    assert_eq!(c.c_leave.get(), 1, "子区域应 leave");
    assert_eq!(c.p_leave.get(), 1, "父区域 leave 应补发（不能卡在 entered）");
}
