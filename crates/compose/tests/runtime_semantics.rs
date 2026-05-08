//! runtime 语义无头测试 — 定向重组、remember 槽位、派生状态、作用域复用。

use compose::layout::{
    Column, create_root_node, dispatch_click, dispatch_click_at, hit_test, Alignment, Arrangement,
    CrossAxisAlignment, Modifier, NodeId, TextStyle,
};
use compose::prelude::*;
use compose_material::{Button, Text};
use compose::runtime::{state_id, with_runtime, Composition, Composer, RuntimeData};
use std::cell::{Cell, RefCell};

thread_local! {
    static RUN_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(msg: &str) {
    RUN_LOG.with(|l| l.borrow_mut().push(msg.to_string()));
}

fn logged() -> Vec<String> {
    RUN_LOG.with(|l| l.borrow().clone())
}

fn reset() {
    RUN_LOG.with(|l| l.borrow_mut().clear());
}

fn find_clickables(rt: &RuntimeData, id: NodeId, out: &mut Vec<NodeId>) {
    if rt.nodes[id.index()].on_click.is_some() {
        out.push(id);
    }
    let ch = rt.nodes[id.index()].children.clone();
    for c in ch {
        find_clickables(rt, c, out);
    }
}

fn clickables(root: NodeId) -> Vec<NodeId> {
    with_runtime(|rt| {
        let mut v = Vec::new();
        find_clickables(rt, root, &mut v);
        v
    })
}

/// 计数器：点击按钮 → 只有读状态的文本作用域重组（定向重组验证）。
#[test]
fn targeted_recomposition_and_remember_stability() {
    reset();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);

    let count = mutableStateOf(0i32);
    let count2 = count;
    comp.set_content(move || {
        let n = count.get();
        log(&format!("label:{n}"));
        MaterialTheme(ColorScheme::light(), move || {
            Column(Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Start,
                move || {
                    Text(format!("Count: {n}"), TextStyle::new(), Modifier::new());
                    let count2 = count2;
                    Button(Modifier::new(),
                        move || {
                            count2.set(count2.get() + 1);
                        },
                        || {
                            Text("Increment", TextStyle::new(), Modifier::new());
                        },
                    );
                },
            );
        });
    });

    assert_eq!(logged(), vec!["label:0"]);

    // 点击 → 定向重组：只有 label 作用域重跑
    let btns = clickables(root);
    assert_eq!(btns.len(), 1);
    dispatch_click(btns[0]);
    Recomposer::recompose_pending();
    assert_eq!(logged(), vec!["label:0", "label:1"]);

    // remember 身份：节点句柄跨重组稳定（这里通过再次点击仍工作验证）
    let btns2 = clickables(root);
    assert_eq!(btns2.len(), 1);
    dispatch_click(btns2[0]);
    Recomposer::recompose_pending();
    assert_eq!(logged().last().unwrap(), "label:2");
}

/// remember 槽位身份：重组后返回同一 state。
#[test]
fn remember_slot_identity_across_recompositions() {
    reset();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);

    let trigger = mutableStateOf(0u32);
    let trigger2 = trigger;

    // 借用受限：用 thread_local 收集
    thread_local! {
        static SEEN: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    }
    SEEN.with(|s| s.borrow_mut().clear());

    comp.set_content(move || {
        let st = remember(|| mutableStateOf(0u32));
        SEEN.with(|s| s.borrow_mut().push(state_id(st)));
        trigger2.get();
    });

    let first = SEEN.with(|s| s.borrow().clone());
    assert_eq!(first.len(), 1);

    trigger.set(1);
    Recomposer::recompose_pending();
    let second = SEEN.with(|s| s.borrow().clone());
    assert_eq!(second.len(), 2);
    assert_eq!(first[0], second[0], "remember must return the same state across recompositions");
}

/// 派生状态：来源变化 → 派生重算 → 下游作用域重组。
#[test]
fn derived_state_recomputes() {
    reset();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);

    let a = mutableStateOf(2i32);
    let b = mutableStateOf(3i32);
    let a2 = a;
    let b2 = b;

    thread_local! {
        static LABEL: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    }
    LABEL.with(|l| l.borrow_mut().clear());

    comp.set_content(move || {
        let sum = derivedStateOf(move || a2.get() + b2.get());
        LABEL.with(|l| l.borrow_mut().push(sum.get()));
    });

    assert_eq!(LABEL.with(|l| l.borrow().clone()), vec![5]);

    // 改 a → 派生重算 → 作用域重组
    a.set(10);
    Recomposer::recompose_pending();
    assert_eq!(LABEL.with(|l| l.borrow().clone()), vec![5, 13]);
}

/// 命中测试：嵌套节点的坐标换算与最深目标。
#[test]
fn hit_test_and_click_dispatch() {
    reset();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);

    thread_local! {
        static CLICKED: Cell<u32> = const { Cell::new(0) };
    }

    comp.set_content(|| {
        MaterialTheme(ColorScheme::light(), || {
            Column(Modifier::new().size(300.0, 300.0),
                Arrangement::Start,
                CrossAxisAlignment::Start,
                || {
                    let clicked = || CLICKED.with(|k| k.set(k.get() + 1));
                    Button(Modifier::new().size(40.0, 40.0), clicked, || {
                        Text("OK", TextStyle::new(), Modifier::new());
                    });
                },
            );
        });
    });

    // 布局：先 measure（窗口 300x300）
    compose::layout::perform_layout(root, Constraints::tight(300.0, 300.0));

    // 点击 (10, 10)（在按钮区域内）
    let hit = hit_test(root, 10.0, 10.0);
    assert!(hit.is_some(), "should hit something at (10,10)");
    let (target, lx, ly) = hit.unwrap();
    dispatch_click_at(target, lx, ly);
    assert_eq!(CLICKED.with(|k| k.get()), 1);

    // 点击 (100, 10)（按钮外）不触发
    let hit2 = hit_test(root, 100.0, 10.0);
    if let Some((t2, lx2, ly2)) = hit2 {
        dispatch_click_at(t2, lx2, ly2);
    }
    assert_eq!(CLICKED.with(|k| k.get()), 1, "click outside Button must not trigger");
}

/// 运行时主题切换：根作用域读 dark 状态 → 切换后 MaterialTheme 重提供新配色，
/// 子树闭包重跑并读到新值（remember 槽位保留）。
#[test]
fn theme_switch_propagates_to_subtree() {
    reset();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);

    let dark = mutableStateOf(false);
    let dark2 = dark;
    comp.set_content(move || {
        MaterialTheme(if dark2.get() { ColorScheme::dark() } else { ColorScheme::light() },
            move || {
                let colors = theme_colors();
                log(&format!("theme:{}", if colors.Surface == ColorScheme::dark().Surface { "dark" } else { "light" }));
            },
        );
    });
    Recomposer::recompose_pending();
    assert_eq!(logged(), vec!["theme:light"]);

    dark.set(true);
    Recomposer::recompose_pending();
    assert_eq!(logged(), vec!["theme:light", "theme:dark"]);
}

/// 回归：定向重组的作用域读取 CompositionLocal（theme_colors）不 panic——
/// run_scope 需从节点父链重建祖先作用域链。
#[test]
fn targeted_recomposition_reads_composition_local() {
    reset();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);

    let page = mutableStateOf(0usize);
    let page2 = page;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            Column(Modifier::new(), Arrangement::Start, CrossAxisAlignment::Start, move || {
                let pg = page2;
                // 独立子作用域：读 page + 读主题
                compose::layout::Clickable(Modifier::new(), move || pg.set(1), move || {
                    let colors = theme_colors();
                    log(&format!("item:{}:{}", page2.get(), colors.Surface == ColorScheme::light().Surface));
                    Text("label", TextStyle::new(), Modifier::new());
                });
            });
        });
    });
    Recomposer::recompose_pending();
    assert_eq!(logged(), vec!["item:0:true"]);

    // 只失效子作用域（读 page 的那个）——修复前此处 panic: CompositionLocal not provided
    page.set(1);
    Recomposer::recompose_pending();
    assert_eq!(logged(), vec!["item:0:true", "item:1:true"]);
}
