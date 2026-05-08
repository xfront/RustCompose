//! 动画规格体系回归 —— 锁死对齐 `androidx.compose.animation.core` 的四件事：
//! ① 时长以单调帧时钟为准（同一 spec 在任何帧率下历时一致）；
//! ② `Spring` 解析解行为（临界阻尼不回弹 / 欠阻尼回弹 / 到位收敛 / 初速度续接）；
//! ③ `Keyframes` / `Snap` 的采样语义；
//! ④ 注册表随静止或「采样消失」归零（无泄漏、无常驻驱帧）。

use std::cell::Cell;
use std::rc::Rc;

use compose::animation::{
    AnimSpec, LoopMode, Spring, Tween, animate_float_as_state, animate_progress_on,
    animations_active, exponential_decay, keyframes, linear, loop_phase,
    remember_animatable, remember_infinite_progress, repeatable, request_anim_frame_after,
    running_animations, snap, take_wake_delay,
};
use compose::layout::{Alignment, create_root_node};
use compose::runtime::{Composition, MutableState, Recomposer, advance_frame, mutableStateOf};

/// 推进一帧（与 compose-render 的 host 帧循环同序）。
fn frame(dt: f32) {
    advance_frame(dt);
    Recomposer::recompose_pending();
}

fn frames(n: u32, dt: f32) {
    for _ in 0..n {
        frame(dt);
    }
}

/// 组合一个「读 target → animateFloatAsState → 记录采样值」的作用域。
fn anim_rig<S: Into<AnimSpec>>(spec: S) -> (Rc<Cell<f32>>, MutableState<f32>) {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let value = Rc::new(Cell::new(-1.0f32));
    let target = mutableStateOf(0f32);
    let value_c = value.clone();
    let spec: AnimSpec = spec.into();
    comp.set_content(move || {
        value_c.set(animate_float_as_state(target.get(), spec.clone()));
    });
    (value, target)
}

/// 在**独立线程**（帧时钟是线程级的，两条不同的帧率必须各用各的钟）里跑同一段
/// 0.3s 线性补间，返回 (起补间帧取值, 0.1s 处取值, 跑完取值, 结束时注册表数量)。
fn run_tween_at(dt: f32) -> (f32, f32, f32, usize) {
    std::thread::spawn(move || {
        let (v, target) = anim_rig(Tween::new(0.3).with_easing(linear));
        target.set(1.0);
        frame(dt); // 起补间帧：elapsed = 0
        let start = v.get();
        frames((0.1 / dt).round() as u32, dt); // 累计 0.1s
        let mid = v.get();
        frames((0.2 / dt).round() as u32 + 2, dt); // 越过 0.3s
        (start, mid, v.get(), running_animations())
    })
    .join()
    .unwrap()
}

/// **帧率无关**：同一段 0.3s 线性补间，30fps 与 120fps 在同一累计时刻取值一致。
/// 旧实现按「帧数 ÷ 假定 60fps」计时——120Hz 屏上 0.3s 动效只跑 0.15s、掉帧时又拖长。
#[test]
fn tween_duration_is_frame_rate_independent() {
    let (s30, m30, e30, r30) = run_tween_at(1.0 / 30.0);
    let (s120, m120, e120, r120) = run_tween_at(1.0 / 120.0);
    assert_eq!(s30, 0.0, "起补间帧应仍在起点（30fps）");
    assert_eq!(s120, 0.0, "起补间帧应仍在起点（120fps）");
    assert!(
        (m30 - m120).abs() < 1e-5,
        "同一累计时刻（0.1s）取值应一致：30fps={m30:.5} 120fps={m120:.5}"
    );
    assert!(
        (m30 - 1.0 / 3.0).abs() < 1e-4,
        "0.1s / 0.3s 线性进度应约 1/3，实测 {m30}"
    );
    assert_eq!(e30, 1.0, "30fps 侧应在 0.3s 后到目标");
    assert_eq!(e120, 1.0, "120fps 侧应在 0.3s 后到目标");
    assert_eq!(r30, 0, "到位后应退出注册表（30fps）");
    assert_eq!(r120, 0, "到位后应退出注册表（120fps）");
}

/// **Snap**：零时长规格当帧落位，不进注册表。
#[test]
fn snap_lands_on_the_same_frame() {
    let (v, target) = anim_rig(snap());
    target.set(0.75);
    frame(1.0 / 60.0);
    assert_eq!(v.get(), 0.75, "Snap 应无补间直接落位");
    assert_eq!(running_animations(), 0, "Snap 不应登记动画");
}

/// **Keyframes**：站点时刻即取值（隐式起点 = 起补间时的当前值）。
#[test]
fn keyframes_hit_their_stops() {
    let (v, target) = anim_rig(keyframes::<f32>(0.6).to(0.5, 0.2).to(1.0, 0.6));
    target.set(1.0);
    frame(1.0 / 60.0); // 起补间帧
    frames(12, 1.0 / 60.0); // elapsed = 0.2s
    assert!(
        (v.get() - 0.5).abs() < 1e-4,
        "0.2s 处应到第一站 0.5，实测 {}",
        v.get()
    );
    frames(24, 1.0 / 60.0); // elapsed = 0.6s
    assert_eq!(v.get(), 1.0, "末站即目标，且应精确落位");
    assert_eq!(running_animations(), 0, "关键帧跑完应退集");
}

/// **Spring**：到位后精确落位、退出注册表，并且不再向宿主请求续帧。
#[test]
fn spring_settles_and_stops_requesting_frames() {
    let (v, target) = anim_rig(Spring::critically_damped(Spring::STIFFNESS_MEDIUM));
    target.set(1.0);
    frame(1.0 / 60.0);
    assert!(v.get() < 0.9, "起补间帧应尚在途中，实测 {}", v.get());
    let mut probe = Vec::new();
    for _ in 0..40 {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        probe.push(animations_active());
    }
    assert_eq!(v.get(), 1.0, "临界阻尼弹簧应最终精确到位");
    assert_eq!(running_animations(), 0, "弹簧到位应退集");
    let tail = &probe[probe.len() - 10..];
    assert!(
        tail.iter().all(|b| !b),
        "静止后不应再请求续帧（否则帧循环永不休眠）：{probe:?}"
    );
}

/// 弹簧物理：起点/初速度被严格尊重；临界阻尼不回弹，欠阻尼回弹后仍收敛。
#[test]
fn spring_simulation_physics() {
    let crit = Spring::critically_damped(4000.0);
    let (x0, v0) = crit.simulate(0.0, 0.0, 1.0, 0.0);
    assert_eq!(x0, 0.0, "t=0 应恰在起点");
    assert_eq!(v0, 0.0, "t=0 速度应为给定初速度");
    let mut overshoot = false;
    for i in 1..=60 {
        let (x, _) = crit.simulate(0.0, 0.0, 1.0, i as f32 / 600.0);
        assert!(x >= -1e-4, "临界阻尼不应下穿起点：{x}");
        if x > 1.0 + 1e-4 {
            overshoot = true;
        }
    }
    assert!(!overshoot, "临界阻尼不得回弹越过目标");
    let (end, _) = crit.simulate(0.0, 0.0, 1.0, 2.0);
    assert!((end - 1.0).abs() < 1e-3, "长时间后应贴住目标，实测 {end}");

    let bouncy = Spring::bouncy();
    let mut peaked = 0.0f32;
    for i in 1..=120 {
        let (x, _) = bouncy.simulate(0.0, 0.0, 1.0, i as f32 / 600.0);
        peaked = peaked.max(x);
    }
    assert!(peaked > 1.02, "欠阻尼（bouncy）应回弹越过目标，实测峰值 {peaked}");
    let (tail, _) = bouncy.simulate(0.0, 0.0, 1.0, 3.0);
    assert!((tail - 1.0).abs() < 0.02, "回弹后仍应收敛到目标，实测 {tail}");

    // 中断续接：手势结束时把指针速度移交给仿真，t=0 导数必须等于该速度
    let (_, d) = Spring::default_spring().simulate(0.5, 30.0, 1.0, 0.0);
    assert_eq!(d, 30.0, "simulate 的 t=0 导数应为初速度");
}

/// `animate_progress_on`：key 每次变化都从 0 重播，播完退集（不常驻驱帧）。
#[test]
fn progress_restarts_on_key_change() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let value = Rc::new(Cell::new(-1.0f32));
    let key = mutableStateOf(0u64);
    let value_c = value.clone();
    comp.set_content(move || {
        value_c.set(animate_progress_on(
            key.get(),
            Tween::new(0.2).with_easing(linear),
        ));
    });
    assert!(
        value.get() < 0.02,
        "首帧入场进度应从 0 起，实测 {}",
        value.get()
    );
    frames(12, 1.0 / 60.0);
    assert_eq!(value.get(), 1.0, "0.2s 后入场应完成");
    assert_eq!(running_animations(), 0, "入场动画播完必须退集");

    key.set(1);
    frame(1.0 / 60.0);
    assert!(
        value.get() < 0.2,
        "key 变化应重新从 ~0 起播，实测 {}",
        value.get()
    );
    frames(12, 1.0 / 60.0);
    assert_eq!(value.get(), 1.0, "第二次入场同样应完成");
    assert_eq!(running_animations(), 0, "第二次入场后同样应退集");
}

/// 采样者消失（控件被条件移除）后仍在跑的动画会被注册表剪枝——
/// 取消「永久帧驱动」之后必须保住这条性质：动画不会悬挂在无人读取的句柄上。
#[test]
fn orphaned_animation_is_pruned() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let shown = mutableStateOf(true);
    let target = mutableStateOf(0f32);
    comp.set_content(move || {
        if shown.get() {
            // 5s 长补间：不会自己到位，只能靠剪枝退集
            animate_float_as_state(target.get(), Tween::new(5.0));
        }
    });
    target.set(1.0);
    frame(1.0 / 60.0);
    assert_eq!(running_animations(), 1, "长补间应在注册表内");
    shown.set(false); // 控件消失：动画不再被任何组合采样
    frames(4, 1.0 / 60.0);
    assert_eq!(
        running_animations(),
        0,
        "停止采样后应在 STALE_FRAMES 内被剪枝"
    );
}

/// 无限动画：相位取墙钟（非帧数），并常驻注册表；`loop_phase` 的回绕/三角语义。
#[test]
fn infinite_phase_loops_on_wall_clock() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let phase = Rc::new(Cell::new(-1.0f32));
    let phase_c = phase.clone();
    comp.set_content(move || {
        phase_c.set(remember_infinite_progress(1.0));
    });
    assert!(phase.get() >= 0.0, "无限相位应从 0 起");
    frames(30, 1.0 / 60.0); // 0.5s
    assert!(
        (phase.get() - 0.5).abs() < 0.05,
        "1s 周期的 Restart 相位在 0.5s 处应约 0.5，实测 {}",
        phase.get()
    );
    frames(66, 1.0 / 60.0); // 累计 1.6s → 回绕到 0.6
    assert!(
        (phase.get() - 0.6).abs() < 0.05,
        "越过整周期应回绕，实测 {}",
        phase.get()
    );
    assert_eq!(running_animations(), 1, "无限动画应常驻注册表");

    let p = loop_phase(1_250_000_000.0, 0.0, 1.0, LoopMode::Restart);
    assert!((p - 0.25).abs() < 1e-5, "1.25s / 1s 周期应回绕到 0.25，实测 {p}");
    let tri = loop_phase(750_000_000.0, 0.0, 1.0, LoopMode::Reverse);
    assert!((tri - 0.5).abs() < 1e-5, "Reverse 在 0.75s 应为下降段的 0.5，实测 {tri}");
}

// ══════════════════════════ 对齐补充（Repeatable / Decay / 定点唤醒） ══════════════════════════

const DT: f32 = 1.0 / 60.0;

/// **Repeatable(Restart)**：按迭代重复时长规格，跑满次数后精确落位并退集。
#[test]
fn repeatable_restarts_and_finishes() {
    let (v, target) = anim_rig(repeatable(
        2,
        Tween::new(0.2).with_easing(linear),
        LoopMode::Restart,
    ));
    target.set(1.0);
    frame(DT); // 起播帧，elapsed = 0
    frames(11, DT); // 0.183s：第一轮接近末尾
    assert!(
        v.get() > 0.8 && v.get() < 1.0,
        "第一轮应接近 1，实测 {}",
        v.get()
    );
    frames(2, DT); // 0.217s：进入第二轮，从头开始
    assert!(
        v.get() < 0.15,
        "Restart 第二轮应从 0 起，实测 {}",
        v.get()
    );
    frames(12, DT); // ≥ 0.4s：两轮跑完
    assert_eq!(v.get(), 1.0, "重复结束应精确落在目标");
    assert_eq!(running_animations(), 0, "有限次重复结束应退集");
}

/// **Repeatable(Reverse)**：奇数次迭代反向播放，末次迭代结束在起点。
#[test]
fn repeatable_reverse_ping_pongs() {
    let (v, target) = anim_rig(repeatable(
        2,
        Tween::new(0.2).with_easing(linear),
        LoopMode::Reverse,
    ));
    target.set(1.0);
    frame(DT);
    frames(14, DT); // 0.233s：第二轮 1/6 处，反向 → 0.833
    assert!(
        (v.get() - 0.8333).abs() < 0.03,
        "Reverse 第二轮应回程，实测 {}",
        v.get()
    );
    frames(12, DT); // ≥ 0.4s：末次迭代反向结束 → 回到起点
    assert!(v.get() < 0.02, "Reverse 结束应回到起点，实测 {}", v.get());
    assert_eq!(running_animations(), 0, "Reverse 结束同样应退集");
}

/// **Decay**：指数摩擦滑行（v0/k 总位移），速度单调衰减，判停后退集、不跳回起点。
#[test]
fn decay_slows_down_and_stops() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let slot: Rc<Cell<Option<compose::animation::Animatable<f32>>>> =
        Rc::new(Cell::new(None));
    let slot_c = slot.clone();
    comp.set_content(move || {
        let a = remember_animatable(0.0f32);
        slot_c.set(Some(a));
        let _ = a.value();
    });
    let a = slot.get().expect("animatable 应在组合期创建");
    // 对齐 Compose：手势结束后把指针速度交给 animateDecay。
    a.animate_decay(100.0, exponential_decay(1.0, 0.1));
    frame(DT);
    let v1 = a.value();
    let vel1 = a.velocity();
    assert!(v1 > 0.0 && v1 < 5.0, "首帧应刚起步，实测 {v1}");
    frames(15, DT);
    let v2 = a.value();
    assert!(v2 > v1, "衰减过程中位移应继续前进：{v1} → {v2}");
    assert!(
        a.velocity().abs() < vel1.abs(),
        "速度应单调衰减：{} → {}",
        vel1,
        a.velocity()
    );
    frames(180, DT); // 3s：v = 100·e^-12 << 阈值
    assert_eq!(running_animations(), 0, "衰减判停应退集");
    let end = a.value();
    assert!(
        (end - 25.0).abs() < 0.5,
        "指数衰减总位移应为 v0/k = 25，实测 {end}"
    );
}

/// **定点唤醒**：`request_anim_frame_after` 不表现为「连续帧请求」，
/// 只登记最早的一次唤醒延时（聚焦光标闪烁 / 自动轮播用它退出满帧率）。
#[test]
fn timed_wake_is_not_continuous_frames() {
    let _ = take_wake_delay();
    let _ = animations_active(); // 清掉可能残留的续帧请求
    request_anim_frame_after(0.5);
    request_anim_frame_after(0.2);
    assert_eq!(take_wake_delay(), Some(0.2), "应取最早的一次唤醒");
    assert_eq!(take_wake_delay(), None, "读取即清除");
    assert!(
        !animations_active(),
        "定点唤醒不应被宿主当成连续帧请求"
    );
    request_anim_frame_after(0.0);
    assert!(animations_active(), "0 延时应退化为立即续帧");
}

/// **updateTransition**：一个目标状态驱动多个派生值，共享帧时钟且各自到位退集。
#[test]
fn update_transition_drives_multiple_values() {
    use compose::animation::update_transition;

    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let out = Rc::new(Cell::new((-1.0f32, -1.0f32)));
    let out_c = out.clone();
    let state = mutableStateOf(0.0f32);
    comp.set_content(move || {
        let tr = update_transition(state.get());
        let a = tr.animate_float(|s| s, Tween::new(0.2).with_easing(linear));
        let b = tr.animate_float(|s| s * 100.0, Tween::new(0.2).with_easing(linear));
        out_c.set((a, b));
    });
    state.set(1.0);
    frame(DT);
    frames(6, DT); // 0.1s：两个分量进度一致（0.5 / 50）
    let (a, b) = out.get();
    assert!((a - 0.5).abs() < 0.02, "a 应到一半，实测 {a}");
    assert!((b - 50.0).abs() < 2.0, "b 应到一半，实测 {b}");
    frames(10, DT); // ≥ 0.2s
    assert_eq!(out.get(), (1.0, 100.0), "两个值都应精确到位");
    assert_eq!(running_animations(), 0, "到位后应退出注册表");
}

/// **大量并发动画**：注册表用 `HashMap` 后仍应全部推进、全部退集。
/// 同时锁死「同一时间戳重复采样只续期、不重复仿真」的幂等路径。
#[test]
fn many_concurrent_animations_settle_and_drain_registry() {
    use std::cell::RefCell;

    const N: usize = 64;
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let targets = mutableStateOf(vec![0f32; N]);
    let sampled = Rc::new(RefCell::new(vec![0f32; N]));
    let sampled_c = sampled.clone();
    comp.set_content(move || {
        let ts = targets.read_clone();
        let mut out = sampled_c.borrow_mut();
        for i in 0..N {
            out[i] = animate_float_as_state(ts[i], Tween::new(0.2).with_easing(linear));
        }
    });
    // 首帧：全部静止落位，不入场补间
    frame(DT);
    assert_eq!(running_animations(), 0, "初始静止不应有动画在跑");

    targets.set((1..=N).map(|i| i as f32).collect());
    frame(DT);
    assert_eq!(running_animations(), N, "每个槽位都应进入注册表");

    frames(20, DT); // 0.35s > 0.2s
    for (i, v) in sampled.borrow().iter().enumerate() {
        assert!(
            (v - (i + 1) as f32).abs() < 1e-4,
            "第 {i} 槽应精确到位，实测 {v}"
        );
    }
    assert_eq!(running_animations(), 0, "全部到位后注册表应清空");
}
