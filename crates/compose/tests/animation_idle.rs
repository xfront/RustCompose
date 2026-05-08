//! 动画「静止即停驱」回归 —— 对齐 Compose `AnimationState`：补间只在未到达目标时留在
//! Recomposer 的动画集合里，到位后即退出，不再每帧调度其作用域重组。
//!
//! 锁死两个行为：
//! ① 目标不变时，帧驱动对 `animateFloatAsState` 完全 no-op（旧实现无条件每帧 `schedule_scope`，
//!    是 CPU 常驻偏高的主因）；
//! ② 补间完成后重新静默：作用域停止重组，且不再向宿主请求续帧（帧循环可休眠）。

use std::cell::Cell;
use std::rc::Rc;

use compose::animation::{Tween, animate_float_as_state, animations_active};
use compose::layout::{create_root_node, Alignment};
use compose::runtime::{
    advance_frame, mutableStateOf, Composition, MutableState, Recomposer,
};

/// 推进一帧：帧驱动 → 排空待重组（与 compose-render 的 host 帧循环同序）。
fn frame() {
    advance_frame(1.0 / 60.0);
    Recomposer::recompose_pending();
}

fn frames(n: u32) {
    for _ in 0..n {
        frame();
    }
}

/// 推进 n 帧，并记录每帧结束时宿主看到的 `animations_active()`。
///
/// 该标志是「读取即清除」的（宿主每帧读一次决定要不要续帧），无头测试必须同样逐帧读，
/// 否则上一帧留下的标志会一直误报为 true。
fn frames_probe(n: u32) -> Vec<bool> {
    let mut out = Vec::new();
    for _ in 0..n {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        out.push(animations_active());
    }
    out
}

/// 组合一个只做「读目标 → 补间 → 写回」的作用域，记录其重组次数与当前采样值。
struct Rig {
    runs: Rc<Cell<u32>>,
    value: Rc<Cell<f32>>,
    target: MutableState<f32>,
}

fn rig(duration_secs: f32) -> Rig {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let runs = Rc::new(Cell::new(0u32));
    let value = Rc::new(Cell::new(-1.0f32));
    let target = mutableStateOf(0f32);
    let (runs_c, value_c) = (runs.clone(), value.clone());
    comp.set_content(move || {
        runs_c.set(runs_c.get() + 1);
        let cur = animate_float_as_state(target.get(), Tween::new(duration_secs));
        value_c.set(cur);
    });
    Rig { runs, value, target }
}

#[test]
fn settled_animation_stops_scheduling_recomposition() {
    let s = rig(0.1); // 6 帧补间
    assert_eq!(s.runs.get(), 1, "初始组合应恰好执行一次");
    assert!(s.value.get().abs() < f32::EPSILON, "起始值即目标值");

    // ① 目标不变：连续 30 帧不该产生任何重组。
    frames(30);
    assert_eq!(
        s.runs.get(),
        1,
        "静止时帧驱动须为 no-op，实测重组 {} 次",
        s.runs.get()
    );

    // ② 目标改变：补间若干帧后自动停驱并收敛。
    s.target.set(1.0);
    frames(10);
    assert!(
        (s.value.get() - 1.0).abs() < 1e-5,
        "10 帧（> 6 帧时长）后应已到达目标，实测 {}",
        s.value.get()
    );
    let after_done = s.runs.get();
    assert!(after_done > 1, "补间期间应确实发生重组：{after_done}");

    // 到位后再跑 30 帧：一次都不该再重组（旧实现会一直重跑到帧循环结束）。
    frames(30);
    assert_eq!(
        s.runs.get(),
        after_done,
        "到达目标后不应再每帧重组：{} → {}",
        after_done,
        s.runs.get()
    );
}

#[test]
fn idle_scene_does_not_request_next_frame() {
    let s = rig(0.1);
    // 首次组合会把标志置位（宿主因此多绘一帧，合理）；读清后应彻底静默。
    let _ = animations_active();
    let idle = frames_probe(6);
    assert!(idle.iter().all(|b| !b), "静止时不应请求续帧：{idle:?}");

    s.target.set(1.0);
    let act = frames_probe(14);
    assert!(act.iter().any(|b| *b), "补间期间应持续请求续帧：{act:?}");
    let tail = &act[act.len() - 5..];
    assert!(
        tail.iter().all(|b| !b),
        "到达目标后应停止请求续帧（否则帧循环永不休眠）：{act:?}"
    );

    // 目标再次改变 → 重新起驱（停驱不是一次性的）。
    s.target.set(0.0);
    let again = frames_probe(3);
    assert!(again.iter().any(|b| *b), "新目标应重新进入动画驱动：{again:?}");
}
