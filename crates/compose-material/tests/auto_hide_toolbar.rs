//! AutoHideToolbar 回归：展开/隐藏高度、peek 细条、点击展开、内容不被压扁，
//! 以及滚动方向驱动（下滚隐藏 / 上滚显示）。

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, Arrangement, CrossAxisAlignment, DrawCmd, Modifier, NodeId, TextStyle,
    create_root_node, dispatch_click_at, dispatch_hover_at, dispatch_hover_left, dispatch_scroll,
    draw_tree, hit_test, perform_layout, scroll_metrics,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{
    Composition, MutableState, add_frame_driver, advance_frame, disposable_effect, mutableStateOf,
    remember, remove_frame_driver,
};
use compose_material::{AutoHideToolbar, VerticalScroll};
use std::cell::Cell;
use std::rc::Rc;

const TOOLBAR_H: f32 = 48.0;
/// 收缩态可见细条（对齐 cui peekSize = 2）。
const PEEK_H: f32 = 2.0;
/// 收缩态隐形悬停容差（对齐 cui kCollapsedTriggerMargin = 16）。
const HOT_H: f32 = 16.0;
const SHELL_BG: Color = Color::rgb(0.2, 0.2, 0.2);
const CONTENT_BG: Color = Color::rgb(0.9, 0.5, 0.1);
fn accent_color() -> Color {
    ColorScheme::light().primary
}

struct Harness {
    root: compose::layout::NodeId,
    hidden: MutableState<bool>,
}

fn boot() -> Harness {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let slot = std::rc::Rc::new(
        std::cell::RefCell::new(None::<MutableState<bool>>),
    );
    let s = slot.clone();
    comp.set_content(move || {
        let s = s.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let hidden = remember(|| mutableStateOf(false));
            *s.borrow_mut() = Some(hidden);
            AutoHideToolbar(
                hidden,
                TOOLBAR_H,
                Modifier::new().background(SHELL_BG, 0.0),
                || {
                    // 内容固定满高、有背景：若被压扁高度会缩水。
                    compose::layout::Box(
                        Modifier::new().fill_max_size().background(CONTENT_BG, 0.0),
                        Alignment::TopStart,
                        || {},
                    );
                },
            );
        });
    });
    perform_layout(root, Constraints::tight(400.0, 600.0));
    Harness {
        root,
        hidden: slot.borrow().unwrap(),
    }
}

fn pump(h: &Harness, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        perform_layout(h.root, Constraints::tight(400.0, 600.0));
    }
}

fn rects_of(cmds: &[DrawCmd], color: Color) -> Vec<(f32, f32, f32, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::FillRect { x, y, w, h, color: c } if *c == color => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect()
}

fn has_h(rects: &[(f32, f32, f32, f32)], want: f32, tol: f32) -> bool {
    rects.iter().any(|(_, _, _, h)| (*h - want).abs() < tol)
}

#[test]
fn expanded_shows_full_height() {
    let h = boot();
    pump(&h, 30);
    let cmds = draw_tree(h.root);
    let shell = rects_of(&cmds, SHELL_BG);
    assert!(has_h(&shell, TOOLBAR_H, 1.0), "展开态外壳应满高 {TOOLBAR_H}: {shell:?}");
    // 展开态 accent 淡出（2px 细条不应出现）。
    assert!(
        !rects_of(&cmds, accent_color()).iter().any(|(_, _, _, h)| *h <= 2.0),
        "展开态不应画 peek accent: {cmds:?}"
    );
}

#[test]
fn hidden_shrinks_to_peek_and_draws_accent() {
    let h = boot();
    pump(&h, 30);
    h.hidden.set(true);
    pump(&h, 30);
    let cmds = draw_tree(h.root);
    let shell = rects_of(&cmds, SHELL_BG);
    assert!(has_h(&shell, PEEK_H, 1.0), "隐藏态外壳应收缩到 peek {PEEK_H}: {shell:?}");
    let accent = rects_of(&cmds, accent_color());
    assert!(has_h(&accent, 2.0, 0.1), "隐藏态应画 2px accent 细条: {accent:?}");
}

#[test]
fn clicking_peek_expands() {
    let h = boot();
    pump(&h, 30);
    h.hidden.set(true);
    pump(&h, 30);
    assert!(h.hidden.get(), "前置：应处于隐藏态");

    // 点 peek 细条（y ≤ PEEK_H，命中条带 Clickable）→ 展开。
    let (target, _, _) = hit_test(h.root, 20.0, 1.0).expect("peek 细条应可命中");
    assert!(dispatch_click_at(target, 20.0, 1.0), "点击应被消费");
    assert!(!h.hidden.get(), "点击 peek 细条应展开工具条");

    pump(&h, 30);
    let shell = rects_of(&draw_tree(h.root), SHELL_BG);
    assert!(has_h(&shell, TOOLBAR_H, 1.0), "展开后应恢复满高: {shell:?}");
}

/// 收缩态隐形容差区（条带外、热区内）：悬停可展开，但**不吃**点击
/// （cui `MouseRegion(opaque=false)` 语义——身后页面仍可点）。
#[test]
fn tolerance_hot_zone_hoverable_but_click_through() {
    let h = boot();
    pump(&h, 30);
    h.hidden.set(true);
    pump(&h, 30);
    assert!(h.hidden.get(), "前置：应处于隐藏态");

    // 容差区（PEEK_H < y < HOT_H）：可命中、可悬停展开……
    let (target, _, _) = hit_test(h.root, 20.0, 10.0).expect("容差热区应可命中");
    dispatch_hover_at(target);
    assert!(!h.hidden.get(), "悬停容差区应展开工具条");
    h.hidden.set(true);
    pump(&h, 30);

    // ……但点击不被消费（热区无 on_click，不拦截身后页面）。
    let (target, _, _) = hit_test(h.root, 20.0, 10.0).unwrap();
    assert!(!dispatch_click_at(target, 20.0, 10.0), "容差区点击应穿透");
    assert!(h.hidden.get(), "点击容差区不应展开工具条");
}

// ── hover 显示 / 离开 2s 自动收回 ──

/// 悬停 peek 热区：立即展开；离开后 2s 自动收回；离开 2s 内不误收。
#[test]
fn hover_shows_and_leave_2s_auto_hides() {
    let h = boot();
    pump(&h, 5);
    h.hidden.set(true);
    pump(&h, 30);
    assert!(h.hidden.get(), "前置：应处于隐藏态");

    // 悬停 peek 热区 → 立即展开（hover 进入即取消收回延时）
    let (target, _, _) = hit_test(h.root, 20.0, 5.0).expect("peek 热区应可命中");
    dispatch_hover_at(target);
    assert!(!h.hidden.get(), "悬停应立即显示工具条");
    pump(&h, 15); // 展开动画

    // 离开 → 2 秒内保持展开，到期收回
    // （展开动画重组重建了节点，生产中宿主每帧 mouse move 会重派 hover；测试同样补派）
    dispatch_hover_at(hit_test(h.root, 20.0, 5.0).unwrap().0);
    assert!(dispatch_hover_left(), "离开事件应被消费");
    pump(&h, 100); // ~1.67s，未到期
    assert!(!h.hidden.get(), "离开 2s 内不应收回");
    pump(&h, 40); // 累计 ~2.3s，越过 2s 截止
    assert!(h.hidden.get(), "离开 2s 后应自动收回边缘");
}

/// 悬停中重新进入：未到期的收回延时被取消，不会误收。
#[test]
fn rehover_before_deadline_cancels_auto_hide() {
    let h = boot();
    pump(&h, 5);
    h.hidden.set(true);
    pump(&h, 30);

    let (target, _, _) = hit_test(h.root, 20.0, 5.0).unwrap();
    dispatch_hover_at(target);
    pump(&h, 10);

    // 离开 1s 后重新悬停：延时取消，再过 2s 仍应保持展开
    dispatch_hover_left();
    pump(&h, 60); // ~1s
    dispatch_hover_at(target);
    pump(&h, 150); // 越过原 2s 截止
    assert!(!h.hidden.get(), "重新悬停应取消收回延时");
}

#[test]
fn content_not_squashed_when_hidden() {
    let h = boot();
    pump(&h, 30);
    h.hidden.set(true);
    pump(&h, 30); // 收敛到隐藏

    // 内容仍是满高测量（只是被 clip 裁掉），不应随外壳压扁到 peek 高。
    let cmds = draw_tree(h.root);
    let content = rects_of(&cmds, CONTENT_BG);
    assert!(
        has_h(&content, TOOLBAR_H, 1.0),
        "内容应按满高 {TOOLBAR_H} 测量、不随外壳压扁: {content:?}"
    );
    // 外壳已收缩，但内容高度仍大于外壳（被裁剪）。
    let shell = rects_of(&cmds, SHELL_BG);
    assert!(has_h(&shell, PEEK_H, 1.0), "外壳应已收缩: {shell:?}");
}

// ── 滚动方向驱动（story 的 page_auto_hide 采用同一机制和判定阈值）──

/// 与 story 页一致的帧驱动判定：相邻帧 offset 差判方向，下滚隐藏、上滚/到顶显示。
struct ScrollHideHarness {
    root: NodeId,
    hidden: MutableState<bool>,
    auto: MutableState<bool>,
    list_id: Rc<Cell<Option<NodeId>>>,
}

fn boot_scroll_hide() -> ScrollHideHarness {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let slot = Rc::new(std::cell::RefCell::new(None::<(
        MutableState<bool>,
        MutableState<bool>,
        Rc<Cell<Option<NodeId>>>,
    )>));
    let s = slot.clone();
    comp.set_content(move || {
        let s = s.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let hidden = remember(|| mutableStateOf(false));
            let auto = remember(|| mutableStateOf(true));
            let list_id = remember(|| Rc::new(Cell::new(None::<NodeId>)));
            let last_off = remember(|| Rc::new(Cell::new(0.0f32)));
            *s.borrow_mut() = Some((hidden, auto, list_id.clone()));

            let drv = remember(|| {
                let (auto, hidden, list_id, last_off) =
                    (auto, hidden, list_id.clone(), last_off.clone());
                add_frame_driver(move || {
                    if !auto.get() {
                        return;
                    }
                    let Some(id) = list_id.get() else { return };
                    let Some((off, content, viewport)) = scroll_metrics(id) else { return };
                    let prev = last_off.get();
                    last_off.set(off);
                    let dy = off - prev;
                    if dy.abs() < 0.5 {
                        return;
                    }
                    let at_top = viewport <= 0.0 || off <= 0.5;
                    let at_bottom = content > viewport && off + viewport >= content - 0.5;
                    if dy > 0.0 && !at_bottom {
                        hidden.set(true);
                    } else if dy < 0.0 || at_top {
                        hidden.set(false);
                    }
                })
            });
            disposable_effect(0).on_dispose(move || remove_frame_driver(drv));

            let id = VerticalScroll(
                Modifier::new().fill_max_width().height(100.0),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                || {
                    for _ in 0..40 {
                        compose_material::Text("row", TextStyle::new(), Modifier::new());
                    }
                },
            );
            list_id.set(Some(id));
        });
    });
    perform_layout(root, Constraints::tight(400.0, 600.0));
    let (hidden, auto, list_id) = slot.borrow().clone().unwrap();
    ScrollHideHarness { root, hidden, auto, list_id }
}

fn tick(h: &ScrollHideHarness) {
    advance_frame(1.0 / 60.0);
    perform_layout(h.root, Constraints::tight(400.0, 600.0));
}

#[test]
fn scrolling_down_hides_and_up_shows() {
    let h = boot_scroll_hide();
    tick(&h); // 首帧稳定：driver 记录初始 offset=0

    let id = h.list_id.get().expect("滚动容器 id 应已登记");
    assert!(!h.hidden.get(), "初始应显示");

    // 下滚：offset 增大 → 隐藏
    assert!(dispatch_scroll(Some(id), 60.0), "滚动事件应被消费");
    tick(&h);
    assert!(h.hidden.get(), "向下滚动应隐藏工具条");

    // 上滚：offset 减小 → 显示
    assert!(dispatch_scroll(Some(id), -30.0));
    tick(&h);
    assert!(!h.hidden.get(), "向上滚动应显示工具条");
}

#[test]
fn at_top_forces_show_and_auto_toggle_stops_detection() {
    let h = boot_scroll_hide();
    tick(&h);

    let id = h.list_id.get().unwrap();
    dispatch_scroll(Some(id), 60.0);
    tick(&h);
    assert!(h.hidden.get(), "前置：下滚已隐藏");

    // 回到顶部：强制显示
    dispatch_scroll(Some(id), -60.0);
    tick(&h);
    assert!(!h.hidden.get(), "回到顶部应强制显示");

    // 关闭自动检测：下滚不再改状态
    h.auto.set(false);
    dispatch_scroll(Some(id), 60.0);
    tick(&h);
    assert!(!h.hidden.get(), "关闭自动检测后下滚不应隐藏");
}
