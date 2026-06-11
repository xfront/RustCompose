//! `VideoPlayerWidget` 回归：控制条**叠在画面底部**（不是摆在画面下方）、静止 2s 自动
//! 隐藏且隐藏后点击穿透到画面、鼠标移入重新显形 / 移出立刻收起、`auto_hide=false` 常显、
//! `controls_builder` 可整体替换内置控件、换源后重新显形。
//!
//! 对齐规格：`/work/study/cui/rust/rui_extra/src/video.rs` 的 `VideoPlayerWidget`
//! （Stack + Positioned(bottom) + MouseRegion + AnimatedOpacity + IgnorePointer）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, Clickable, DrawCmd, Modifier, NodeId, TextStyle, create_root_node,
    dispatch_click_at, dispatch_hover_at, dispatch_hover_left, draw_tree, hit_test, perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, MutableState, advance_frame, mutableStateOf, remember};
use compose_material::{
    MockVideoBackend, Text, VideoControlsBuilder, VideoPlayback, VideoPlayerController,
    VideoPlayerWidget,
};

/// 舞台高（与页面 `Modifier::fill_max_width().height(STAGE_H)` 一致）。
const STAGE_W: f32 = 400.0;
const STAGE_H: f32 = 270.0;
/// 自定义控件条的标志性底色（用它定位控制条矩形）。
const BAR_BG: Color = Color::rgb(0.92, 0.31, 0.12);
/// 内置叠放控件条的半透明深底。
const OVERLAY_BG: Color = Color::rgba(0.0, 0.0, 0.0, 0.55);

fn mock_controller() -> VideoPlayerController {
    let pb = VideoPlayback::with_backend(Box::new(MockVideoBackend::new(64, 36, 2_000_000, 8)));
    let c = VideoPlayerController::new(pb);
    c.initialize();
    c
}

/// 一次组合的观察记录。
#[derive(Clone)]
struct Obs {
    /// builder 最近一次收到的 `show`。
    show: Rc<Cell<bool>>,
    /// builder 被调用的次数（证明自定义控件确实进入组合）。
    calls: Rc<Cell<u32>>,
    /// 自定义控件条自身被点击过（用于判断点击有没有被隐藏的控制条吃掉）。
    hit_custom: Rc<Cell<bool>>,
}

impl Obs {
    fn new() -> Self {
        Self {
            show: Rc::new(Cell::new(false)),
            calls: Rc::new(Cell::new(0)),
            hit_custom: Rc::new(Cell::new(false)),
        }
    }
    fn builder(&self) -> VideoControlsBuilder {
        let show = self.show.clone();
        let calls = self.calls.clone();
        let hit = self.hit_custom.clone();
        Rc::new(move |s: bool| {
            show.set(s);
            calls.set(calls.get() + 1);
            let hit = hit.clone();
            Clickable(
                Modifier::new().fill_max_width().height(24.0).background(BAR_BG, 0.0),
                move || hit.set(true),
                || {
                    Text("自定义控件条", TextStyle::new(), Modifier::new());
                },
            );
        })
    }
}

struct Harness {
    root: NodeId,
    /// 页面里控制「当前播哪个会话」的槽位（换源测试用）。
    slot: Rc<RefCell<Option<(Rc<RefCell<VideoPlayerController>>, MutableState<u64>)>>>,
    obs: Obs,
}

fn boot(auto_hide: bool, custom: bool) -> Harness {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let c0 = mock_controller();
    let obs = Obs::new();
    let slot = Rc::new(RefCell::new(None));
    {
        let slot = slot.clone();
        let c0 = c0.clone();
        let obs = obs.clone();
        comp.set_content(move || {
            let rev = remember(|| mutableStateOf(0u64));
            let cell = remember(|| Rc::new(RefCell::new(c0.clone())));
            *slot.borrow_mut() = Some((cell.clone(), rev));
            let _ = rev.get();
            let ctl = cell.borrow().clone();
            let builder = if custom { Some(obs.builder()) } else { None };
            MaterialTheme(ColorScheme::light(), move || {
                VideoPlayerWidget(
                    ctl.clone(),
                    Modifier::new().fill_max_width().height(STAGE_H),
                    auto_hide,
                    builder.clone(),
                );
            });
        });
    }
    perform_layout(root, Constraints::tight(STAGE_W, 600.0));
    Harness { root, slot, obs }
}

fn pump(h: &Harness, frames: usize) {
    for _ in 0..frames {
        advance_frame(1.0 / 60.0);
        perform_layout(h.root, Constraints::tight(STAGE_W, 600.0));
    }
}

fn rects_of(cmds: &[DrawCmd], color: Color) -> Vec<(f32, f32, f32, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::FillRect { x, y, w, h, color: cc } if *cc == color => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect()
}

/// 控制条底边是否与舞台底边重合（叠放语义，而非画面下方另起一行）。
fn bar_bottom(cmds: &[DrawCmd], color: Color) -> Option<f32> {
    rects_of(cmds, color)
        .into_iter()
        .filter(|(_, _, _, rh)| *rh > 1.0 && *rh < STAGE_H)
        .map(|(_, y, _, rh)| y + rh)
        .reduce(f32::max)
}

/// 舞台命中点：控制条所在区域（底边往上 8px）。
fn bar_point(h: &Harness) -> (NodeId, f32, f32) {
    hit_test(h.root, STAGE_W * 0.5, STAGE_H - 8.0).expect("控制条区域应可命中")
}

#[test]
fn overlay_bar_sits_at_bottom_of_stage() {
    let h = boot(true, false);
    pump(&h, 5);
    let cmds = draw_tree(h.root);
    // 画面铺满整个舞台（不是「画面 + 下方控制条」的两段式）。
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillRRect { h: hh, .. } if (*hh - STAGE_H).abs() < 1.0)),
        "画面应铺满 {STAGE_H} 高的舞台: {cmds:?}"
    );
    let bottom = bar_bottom(&cmds, OVERLAY_BG).expect("内置叠放控制条应有半透明深底");
    assert!(
        (bottom - STAGE_H).abs() < 1.0,
        "控制条底边应贴舞台底边（Positioned bottom=0），实测 {bottom}: {:?}",
        rects_of(&cmds, OVERLAY_BG)
    );
    h.obs.calls.set(0);
    pump(&h, 3);
    // controls=None ⇒ 走内置 VideoControlsOverlay，自定义 builder 一次都不该被调。
    assert_eq!(h.obs.calls.get(), 0, "内置控件路径不应调用自定义 builder");
}

#[test]
fn idle_two_seconds_hides_and_clicks_pass_through() {
    let h = boot(true, true);
    pump(&h, 5);
    assert!(h.obs.show.get(), "初始应显示控制条");
    assert!(h.obs.calls.get() > 0, "自定义 builder 应被调用");

    // 显示态：点控制条 → 自定义控件吃掉点击。
    let (t, x, y) = bar_point(&h);
    assert!(dispatch_click_at(t, x, y), "控制条点击应被消费");
    assert!(h.obs.hit_custom.get(), "显示态点击控制条应命中自定义控件");

    // 静止 2s（>120 帧 + 淡出）后收起。
    h.obs.hit_custom.set(false);
    pump(&h, 140);
    assert!(!h.obs.show.get(), "静止 2s 后控制条应自动隐藏");
    let cmds = draw_tree(h.root);
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::SaveLayer { alpha, .. } if *alpha < 0.05)),
        "隐藏应把控制条图层 alpha 拉到 0（IgnorePointer + Opacity(0)）: {cmds:?}"
    );

    // 隐藏态：同一点位 → 穿透到画面（舞台自身的 toggle_play），控制条不吃点击。
    let (t, x, y) = bar_point(&h);
    assert!(dispatch_click_at(t, x, y), "隐藏后点击应仍被舞台消费");
    assert!(!h.obs.hit_custom.get(), "隐藏后控制条不该再吃点击");
}

#[test]
fn hover_reveals_and_leaving_hides_immediately() {
    let h = boot(true, true);
    pump(&h, 140);
    assert!(!h.obs.show.get(), "前置：静止 2s 后应已隐藏");

    // 移入画面 → 立刻显形，并重新起 2s 计时。
    let (surf, _, _) = hit_test(h.root, STAGE_W * 0.5, STAGE_H * 0.4).expect("画面应可命中");
    dispatch_hover_at(surf);
    pump(&h, 10);
    assert!(h.obs.show.get(), "鼠标移入应重新显形");

    pump(&h, 100); // ~1.67s，未到期
    assert!(h.obs.show.get(), "静止 2s 内不该收回");

    // 移出画面 → 不等 2s，立刻收起（参考实现的 stage_collapse）。
    // 重组会重建节点并清掉 `hovered_node`，故先按现役节点重新登记悬停目标。
    let (cur, _, _) = hit_test(h.root, STAGE_W * 0.5, STAGE_H * 0.4).unwrap();
    dispatch_hover_at(cur);
    assert!(dispatch_hover_left(), "离开事件应被消费");
    pump(&h, 5);
    assert!(!h.obs.show.get(), "移出画面应立刻隐藏");
}

#[test]
fn auto_hide_false_keeps_controls_visible() {
    let h = boot(false, true);
    pump(&h, 5);
    assert!(h.obs.show.get(), "auto_hide=false 初始应显示");
    pump(&h, 300); // 5s，远超隐藏延时
    assert!(h.obs.show.get(), "auto_hide=false 时控制条应常显");
    let cmds = draw_tree(h.root);
    assert!(
        !cmds.iter().any(|c| matches!(c, DrawCmd::SaveLayer { alpha, .. } if *alpha < 0.05)),
        "常显时不该出现 alpha=0 的控制条图层: {cmds:?}"
    );
    let (t, x, y) = bar_point(&h);
    dispatch_click_at(t, x, y);
    assert!(h.obs.hit_custom.get(), "常显时点击控制条应命中自定义控件");
}

#[test]
fn new_session_reveals_again() {
    let h = boot(true, true);
    pump(&h, 140);
    assert!(!h.obs.show.get(), "前置：应已隐藏");

    // 换源（session 变化）⇒ 即使没有鼠标事件也应重新显形。
    let (cell, rev) = h.slot.borrow().clone().expect("槽位应已写入");
    let c1 = mock_controller();
    *cell.borrow_mut() = c1.clone();
    rev.set(rev.get() + 1);
    pump(&h, 3);
    assert!(h.obs.show.get(), "换源后控制条应重新显形");
    c1.dispose();
}
