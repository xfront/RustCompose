//! 回归用例：按压指示（Flutter `InkWell` / M3 ripple 语义）。
//!
//! 1) 点击在**抬起**时结算，按下只起波纹 —— 否则在控件上按下、滑出去抬起会误触。
//! 2) 抬起点偏离按压点超过 tap slop（8px）或落在别的可点击链上 → 取消点击。
//! 3) 波纹画在归属节点上（裁到节点形状），抬手淡出窗口结束后被剪掉。
//! 4) `Modifier::no_indication()` 的整屏遮罩不画波纹。

use std::cell::Cell;
use std::time::{Duration, Instant};

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, Box, Clickable, DrawCmd, Modifier, NodeId, create_root_node, dispatch_press_down,
    dispatch_press_up, draw_tree, hit_test, perform_layout, press_active,
};
use compose::runtime::{Composition, advance_frame};

// 计数器走 thread_local：`set_content` 要求 `Fn`，嵌套 `move` 闭包无法共享 `Rc` 所有权。
thread_local! {
    static HITS: Cell<usize> = const { Cell::new(0) };
}

fn bump() {
    HITS.with(|h| h.set(h.get() + 1));
}
fn hits() -> usize {
    HITS.with(|h| h.get())
}

fn win() -> Constraints {
    Constraints::new(0.0, 200.0, 0.0, 100.0)
}

/// 一棵只有一个 80×40 可点击块的树（块位于 (0,0)，故 (20,20) 命中其内部）。
fn one_clickable() -> NodeId {
    HITS.with(|h| h.set(0));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        Box(Modifier::new().size(200.0, 100.0), Alignment::TopStart, || {
            Clickable(
                Modifier::new()
                    .size(80.0, 40.0)
                    .background(Color::rgba(0.4, 0.4, 0.9, 1.0), 8.0),
                bump,
                || {},
            );
        });
    });
    perform_layout(root, win());
    root
}

/// 整屏遮罩（点一下就关的那类浮层：不该糊满一屏波纹）。
fn full_scrim() -> NodeId {
    HITS.with(|h| h.set(0));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        Clickable(
            Modifier::new().fill_max_size().no_indication(),
            bump,
            || {},
        );
    });
    perform_layout(root, win());
    root
}

fn circles(root: NodeId) -> Vec<(f32, f32, f32)> {
    perform_layout(root, win());
    draw_tree(root)
        .iter()
        .filter_map(|c| match c {
            DrawCmd::FillCircle { cx, cy, r, .. } => Some((*cx, *cy, *r)),
            _ => None,
        })
        .collect()
}

#[test]
fn click_fires_on_release_not_on_press() {
    let root = one_clickable();
    let (target, _, _) = hit_test(root, 20.0, 20.0).expect("按下点应命中可点击块");

    dispatch_press_down(target, 20.0, 20.0);
    assert_eq!(
        hits(),
        0,
        "按下即触发点击 → 滑出抬起会误触，且与 Flutter/Compose 相反"
    );

    let up = hit_test(root, 23.0, 21.0);
    assert!(dispatch_press_up(up, 23.0, 21.0), "slop 内抬起应结算为点击");
    assert_eq!(hits(), 1);
}

#[test]
fn release_beyond_tap_slop_cancels_click() {
    let root = one_clickable();
    let (target, _, _) = hit_test(root, 20.0, 20.0).expect("按下点应命中可点击块");

    dispatch_press_down(target, 20.0, 20.0);
    // 同一次按压里滑到 (60,20)：仍在块内，但偏离按下点 40px > slop ⇒ 视为滑出。
    let up = hit_test(root, 60.0, 20.0);
    assert!(!dispatch_press_up(up, 60.0, 20.0));
    assert_eq!(hits(), 0, "超过 tap slop 的位移不该触发点击");
}

#[test]
fn release_outside_any_clickable_cancels_click() {
    let root = one_clickable();
    let (target, _, _) = hit_test(root, 20.0, 20.0).expect("按下点应命中可点击块");

    dispatch_press_down(target, 20.0, 20.0);
    // 块只到 (80,40)：(150,90) 的命中链上没有带 on_click 的祖先。
    let up = hit_test(root, 150.0, 90.0);
    assert!(!dispatch_press_up(up, 150.0, 90.0));
    assert_eq!(hits(), 0);
}

#[test]
fn press_paints_ripple_and_prunes_after_fade() {
    let root = one_clickable();
    let (target, _, _) = hit_test(root, 20.0, 20.0).expect("按下点应命中可点击块");

    assert!(circles(root).is_empty(), "未按下不该有波纹");
    dispatch_press_down(target, 20.0, 20.0);
    let held = circles(root);
    assert_eq!(held.len(), 1, "按下一帧应画出波纹");
    assert_eq!((held[0].0, held[0].1), (20.0, 20.0), "波纹圆心 = 按压点");
    assert!(press_active());

    dispatch_press_up(hit_test(root, 20.0, 20.0), 20.0, 20.0);
    assert_eq!(hits(), 1);
    advance_frame(1.0 / 60.0);
    assert_eq!(circles(root).len(), 1, "抬手后波纹仍在淡出窗口内");

    // 淡出窗口（0.35s）过后剪掉：波纹不能永远挂在树上。
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(420) {
        advance_frame(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(circles(root).is_empty(), "淡出结束后波纹必须被剪掉");
    assert!(!press_active(), "无在飞按压/波纹时不该再请求续帧");
}

#[test]
fn no_indication_suppresses_ripple() {
    let root = full_scrim();
    let (target, _, _) = hit_test(root, 120.0, 70.0).expect("遮罩应铺满窗口");

    dispatch_press_down(target, 120.0, 70.0);
    assert!(circles(root).is_empty(), "no_indication 节点不该有波纹");
    assert!(dispatch_press_up(hit_test(root, 120.0, 70.0), 120.0, 70.0));
    assert_eq!(hits(), 1, "关闭波纹不影响点击本身");
}
