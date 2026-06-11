//! 动画与过渡 — AnimatedVisibility / Crossfade / SharedAxis …
//!
//! 说明：核心 `DrawCmd` 没有分组透明度，淡入淡出用**同色遮罩**实现（内容之上叠一层
//! 主题底色，alpha 由 1 → 0），因此视觉上与真淡入一致且无需改渲染管线。

use compose::animation::{
    LoopMode, Spring, Tween, animate_float_as_state, animate_progress_on, clock_nanos,
    ease_out_cubic, loop_phase, progress_since_nanos, remember_infinite_transition,
};
use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::Color;
use compose::layout::{
    Alignment, Box, Canvas, DrawBuilder, Modifier, NodeId, Policy, PullPhase, ScrollInfo, add_node,
    invalidate_draw_node, measure_text_with_measurer, next_pull_key, register_pull_target,
    with_content_scope,
};
use compose::material::theme_colors;
use compose::runtime::{
    MutableState, add_frame_driver, current_scope_id, disposable_effect, mutableStateOf,
    remember, remove_frame_driver, schedule_scope, with_rt_pub,
};

use crate::tokens;

/// 遮罩式淡入（对齐 Compose 的 draw-phase 动画：**不每帧重组**）。
///
/// 机制（移植 Compose「动画只在合成/绘制阶段生效」的核心）：
/// - 帧驱动按 `ver()` 变化**只重组一次**（版本切换那一帧）；
/// - 淡入过程完全在 **draw 阶段**推进：每帧把进度写入 `Rc<Cell>` 并只标遮罩节点脏
///   （触发其 draw 闭包重跑、实时读进度），**绝不调用 `schedule_scope`**；
/// - 遮罩节点只绘制一次后由缓存回放，整棵外层树（含 content）因此不被拖入每帧重组/重绘。
///
/// 计时用单调帧时钟（[`clock_nanos`]），不再是「帧数 ÷ 假定 60fps」——120Hz 屏与掉帧时
/// 历时一致。驱动随作用域销毁注销（旧实现只 `remember` 注册、从不注销）。
fn fade_mask(
    ver: impl Fn() -> u64 + 'static,
    dur: f32,
    paint: impl Fn(f32, &mut DrawBuilder, f32, f32, f32, f32) + 'static,
) {
    let sid = current_scope_id();
    // 进度用普通 Rc<Cell> 存储：draw 闭包在 `draw_tree_tagged` 的 RUNTIME 借用内执行，
    // 不能读 MutableState（其内部会再次 borrow RUNTIME → 重入 panic）。Rc<Cell> 不经 RUNTIME。
    let prog = remember(|| Rc::new(Cell::new(0.0f32)));
    let start = remember(|| Rc::new(Cell::new(clock_nanos())));
    let prev = remember(|| Rc::new(Cell::new(ver())));
    let id_cell = remember(|| Rc::new(Cell::new(None::<NodeId>)));

    // 遮罩节点（全尺寸、无命中），绘制顺序在 content 之后（调用方需在 Box 之后调用本函数）
    let id = add_node(
        Policy::Box(Alignment::TopStart),
        Modifier::new().fill_max_size().no_hit(),
    );
    id_cell.set(Some(id));
    let prog_c = prog.clone();
    let paint_c = paint;
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b, x, y, w, h| {
            paint_c(prog_c.get(), b, x, y, w, h);
        }));
    });

    let (start_c, prev_c, id_c, sid_c, prog2) = (
        start.clone(),
        prev.clone(),
        id_cell.clone(),
        sid,
        prog.clone(),
    );
    let drv = remember(move || {
        add_frame_driver(move || {
            let now = ver();
            if prev_c.get() != now {
                // 版本切换：仅此一帧重组，刷新 content 与遮罩节点
                prev_c.set(now);
                start_c.set(clock_nanos());
                if let Some(s) = sid_c {
                    schedule_scope(s);
                }
            }
            let t = progress_since_nanos(start_c.get(), dur);
            if prog2.get() != t {
                prog2.set(t);
                // 仅标遮罩节点脏（其 draw 闭包重跑读最新进度），不波及整树；
                // 经 invalidate_draw_node 同步失效祖先，避免增量快路径停在根缓存回放里。
                if let Some(i) = id_c.get() {
                    invalidate_draw_node(i);
                }
            }
            if t < 1.0 {
                compose::animation::request_anim_frames();
            }
        })
    });
    // 作用域销毁时注销驱动：淡入本身只在切换后 dur 秒内需要帧，控件消失后不应常驻
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
}

/// `AnimatedVisibility(visible, bodyHeight) { content }` — 高度滑入 + 淡入（遮罩式）。
///
/// `visible=false` 时高度收为 0、内容随之隐藏；`true` 时高度展开到 `body_height`
/// 并对内容叠一层主题底色遮罩由 1→0 淡入。核心渲染管线无分组透明度，淡入以
/// 同色遮罩近似（与 Crossfade / FadeThrough 同机制）。
pub fn AnimatedVisibility(
    visible: bool,
    body_height: f32,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    // 主题色组合期捕获（draw 闭包内不能借 RUNTIME）
    let colors = theme_colors();
    let t = animate_float_as_state(if visible { 1.0 } else { 0.0 }, Tween::new(0.25));
    Box(
        modifier.fill_max_width().height(t * body_height).clip(0.0),
        Alignment::TopStart,
        move || {
            content();
            Canvas(
                Modifier::new().fill_max_size().no_hit(),
                move |b, x, y, w, h| {
                    if t < 1.0 {
                        // 内容未完全展开时压一层主题底色，制造淡入观感
                        b.fill_rect(x, y, w, h, colors.Surface.with_alpha((1.0 - t) * 0.9));
                    }
                },
            );
        },
    );
}

/// `Crossfade(key) { content }` — 内容切换时的淡入（遮罩式，draw-phase，零重组）。
///
/// 旧实现用 `ensure_frames` 每帧 `schedule_scope` 重组整棵内容作用域（页面切换即整页每帧重绘）。
/// 现改为 `fade_mask`：仅在 key 变化那一帧重组一次内容，淡入全程在 draw 阶段推进。
pub fn Crossfade(key: u64, modifier: Modifier, content: impl Fn() + 'static) {
    // 主题色在组合期捕获：draw 闭包内不能调用 theme_colors（其内部经 RUNTIME 借用，
    // 而 draw_tree 已在 RUNTIME 借用中执行 → 重入 panic）。
    let colors = theme_colors();
    Box(modifier, Alignment::TopStart, move || {
        content();
    });
    fade_mask(
        move || key,
        0.25,
        move |t, b, x, y, w, h| {
            if t < 1.0 {
                b.fill_rect(x, y, w, h, colors.Surface.with_alpha(1.0 - t));
            }
        },
    );
}

/// `FadeThrough(key) { content }` — Material fade-through（较慢的淡入）。
pub fn FadeThrough(key: u64, modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    Box(modifier, Alignment::TopStart, move || {
        content();
    });
    fade_mask(
        move || key,
        0.35,
        move |t, b, x, y, w, h| {
            if t < 1.0 {
                b.fill_rect(x, y, w, h, colors.Surface.with_alpha(1.0 - t));
            }
        },
    );
}

/// `SharedAxis(key, horizontal, forward) { content }` — Material shared-axis（滑入 + 淡入）。
///
/// 入场进度走核心 [`animate_progress_on`]：key 变化即从 0 重播，播完自动退出动画注册表，
/// 页面停留期间该作用域不再每帧重组（旧实现挂一条永久帧驱动 `schedule_scope` 整页）。
pub fn SharedAxis(
    key: u64,
    horizontal: bool,
    forward: bool,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let colors = theme_colors();
    let t = animate_progress_on(key, Tween::new(0.30).with_easing(ease_out_cubic));
    let dist = 32.0 * (1.0 - t) * if forward { -1.0 } else { 1.0 };
    let m = if horizontal {
        modifier.offset(dist, 0.0)
    } else {
        modifier.offset(0.0, dist)
    };
    Box(m, Alignment::TopStart, move || {
        content();
        Canvas(
            Modifier::new().fill_max_size().no_hit(),
            move |b, x, y, w, h| {
                if t < 1.0 {
                    b.fill_rect(x, y, w, h, colors.Surface.with_alpha((1.0 - t) * 0.85));
                }
            },
        );
    });
}

/// `SlideIn(visible, dx, dy) { content }` — 位移进入/退出。
pub fn SlideIn(visible: bool, dx: f32, dy: f32, modifier: Modifier, content: impl Fn() + 'static) {
    let t = animate_float_as_state(if visible { 1.0 } else { 0.0 }, Tween::new(0.28));
    Box(
        modifier.offset(dx * (1.0 - t), dy * (1.0 - t)),
        Alignment::TopStart,
        content,
    );
}

/// `Pulse(period)` — 呼吸相位 0..1，供调用方驱动尺寸 / 透明度。
///
/// 无限动画自身即在注册表内（永不自删，随作用域销毁剪枝），无需另挂帧驱动。
pub fn Pulse(period: f32) -> f32 {
    remember_infinite_transition(period)
}

/// 纯绘制型的无限相位（`Skeleton` / `Spinner` / 不确定进度条用）。
///
/// 与 [`remember_infinite_transition`] 的关键差别：相位只在 **draw 阶段**推进——
/// 帧驱动每帧写入 `Rc<Cell>` 并只失效该节点，调用方作用域**不参与每帧重组**。
/// 旧实现把相位读进组合期，`remember_infinite_transition` 每帧调度整个调用方作用域：
/// 一个 Spinner 就能让所在页面每帧重新 emit + 布局。
///
/// `progress = Some(v)`（确定态）时固定为 `v`、驱动空转且不请求帧；传 `None` 时为
/// 0..1..0 循环相位（[`LoopMode::Reverse`]）。`node` 每次组合刷新（重组会换节点句柄）。
pub(crate) fn draw_phase(
    node: NodeId,
    progress: Option<f32>,
    period: f32,
    mode: LoopMode,
) -> Rc<Cell<f32>> {
    let phase = remember(|| Rc::new(Cell::new(0.0f32)));
    let active = remember(|| Rc::new(Cell::new(false)));
    let start = remember(|| Rc::new(Cell::new(clock_nanos())));
    let slot = remember(|| Rc::new(Cell::new(Some(node))));
    slot.set(Some(node));
    active.set(progress.is_none());
    if let Some(v) = progress {
        phase.set(v);
    }
    let (phase_d, active_d, start_d, slot_d) =
        (phase.clone(), active.clone(), start.clone(), slot.clone());
    let drv = remember(move || {
        let (p, a, s, n, was_on) = (
            phase_d,
            active_d,
            start_d,
            slot_d,
            Rc::new(Cell::new(false)),
        );
        add_frame_driver(move || {
            if !a.get() {
                if was_on.get() {
                    was_on.set(false);
                }
                return;
            }
            if !was_on.get() {
                // 重新进入不确定态：相位从头开始
                was_on.set(true);
                s.set(clock_nanos());
            }
            let v = loop_phase(clock_nanos(), s.get(), period, mode);
            if (v - p.get()).abs() > f32::EPSILON {
                p.set(v);
                if let Some(id) = n.get() {
                    invalidate_draw_node(id);
                }
            }
            // 连续帧：不确定态本身就是持续动画，但只重绘该节点
            compose::animation::request_anim_frames();
        })
    });
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
    phase
}

// ════════════════════ 补齐（animation） ════════════════════

/// `AnimatedWidget(visible, dx, dy) { content }` — 可见时滑入 + 淡入（遮罩式）。
///
/// 核心管线无分组透明度，淡入用主题底色遮罩近似；`visible=false` 时不组合内容。
pub fn AnimatedWidget(
    visible: bool,
    dx: f32,
    dy: f32,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    if !visible {
        return;
    }
    let colors = theme_colors();
    let t = animate_float_as_state(1.0, Tween::new(0.25));
    Box(
        modifier.offset(dx * (1.0 - t), dy * (1.0 - t)),
        Alignment::TopStart,
        move || {
            content();
            Canvas(
                Modifier::new().fill_max_size().no_hit(),
                move |b, x, y, w, h| {
                    if t < 1.0 {
                        b.fill_rect(x, y, w, h, colors.Surface.with_alpha(1.0 - t));
                    }
                },
            );
        },
    );
}

/// `FadeScale(key) { content }` — key 变化时淡入 + 轻微下移（缩放的退化近似）。
pub fn FadeScale(key: u64, modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    let t = animate_progress_on(key, Tween::new(0.25).with_easing(ease_out_cubic));
    Box(
        modifier.offset(0.0, 6.0 * (1.0 - t)),
        Alignment::TopStart,
        move || {
            content();
            Canvas(
                Modifier::new().fill_max_size().no_hit(),
                move |b, x, y, w, h| {
                    if t < 1.0 {
                        b.fill_rect(x, y, w, h, colors.Surface.with_alpha(1.0 - t));
                    }
                },
            );
        },
    );
}

/// 把指示器显示参数写入 draw-phase Cell；任一项变化才失效节点（避免无谓重绘）。
fn pull_indicator_cells(
    dh: &Rc<Cell<f32>>,
    spin: &Rc<Cell<bool>>,
    phase: &Rc<Cell<f32>>,
    node: &Rc<Cell<Option<NodeId>>>,
    next_dh: f32,
    next_spin: bool,
    next_phase: f32,
) {
    if (dh.get() - next_dh).abs() <= f32::EPSILON
        && spin.get() == next_spin
        && (phase.get() - next_phase).abs() <= f32::EPSILON
    {
        return;
    }
    dh.set(next_dh);
    spin.set(next_spin);
    phase.set(next_phase);
    if let Some(id) = node.get() {
        invalidate_draw_node(id);
    }
}

/// 辅助：生成圆弧采样点（进度环 / 旋转弧）。
///
/// 定长数组而非 `Vec`：这些弧每帧都重采样（draw-phase 动画），堆分配不该出现在热路径。
fn arc_points<const N: usize>(cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> [(f32, f32); N] {
    let steps = N.saturating_sub(1).max(1);
    let mut pts = [(0.0f32, 0.0f32); N];
    for (s, p) in pts.iter_mut().enumerate() {
        let a = a0 + (a1 - a0) * (s as f32 / steps as f32);
        *p = (cx + a.cos() * r, cy + a.sin() * r);
    }
    pts
}

/// `PullToRefresh(refreshing, modifier, on_refresh) { content }` — 液态下拉刷新。
///
/// * `content` 被放入垂直滚动容器；滚动到顶时向下拖拽进入下拉手势，
///   内容随阻尼下移，顶部呈现「液面下凹 + 谷底旋转进度环」。
/// * 越过阈值（64px）松手 → 置 `refreshing=true` 并触发 `on_refresh`；
///   指示器以弹簧吸附到停留位（`REST`）并持续旋转；调用方完成刷新后把
///   `refreshing` 置回 `false`，指示器弹簧收回。
pub fn PullToRefresh(
    refreshing: MutableState<bool>,
    modifier: Modifier,
    on_refresh: impl Fn() + 'static,
    content: impl Fn() + 'static,
) {
    const THRESHOLD: f32 = 64.0;
    const REST: f32 = 52.0; // 刷新时指示器停留偏移
    const DAMP: f32 = 0.5; // 下拉阻尼系数

    // 显示偏移：只被手势/驱动读写，用 `Rc<Cell>` 避免每帧 `set` 触发观察者（指示器不重组）。
    let pull = remember(|| Rc::new(Cell::new(0.0f32)));
    let target = remember(|| mutableStateOf(0.0f32)); // 弹簧吸附目标
    let dragging = remember(|| mutableStateOf(false));
    let armed = remember(|| mutableStateOf(false)); // 已越阈值、等待/正在刷新
    // 弹簧仿真起点：(起点位移, 起点速度, 起Clock, 播种时的目标)
    // 目标变化（或抬手）时重新播种，其后按核心 `Spring::simulate` 解析解推进 → 帧率无关。
    let sim = remember(|| Rc::new(Cell::new((0.0f32, 0.0f32, 0.0f64, 0.0f32))));
    // 指示器显示参数（draw 阶段读取）：位移 / 是否旋转 / 旋转相位 / 节点句柄。
    // 驱动每帧写 Cell 并只失效该节点 —— 指示器作用域不再每帧重组（刷新可持续数秒）。
    let ind_dh = remember(|| Rc::new(Cell::new(0.0f32)));
    let ind_spin = remember(|| Rc::new(Cell::new(false)));
    let ind_phase = remember(|| Rc::new(Cell::new(0.0f32)));
    let ind_node = remember(|| Rc::new(Cell::new(None::<NodeId>)));
    let key = remember(next_pull_key);
    crate::widgets::overlay::PutIndexedAction(key, move |_i: usize| {
        on_refresh();
    });

    // 指示器状态维护只在帧驱动里（重组路径绝不写状态——`set` 无同值去重，
    // 组合期写自己正读的 state 会自失效，重组无限不收敛）。
    // 注意：这里**不再**给父作用域挂驱动。旧实现同时挂了驱动 A（调度父作用域）与
    // 驱动 B（调度指示器子作用域），下拉期间整棵 PullToRefresh（含滚动容器节点重建）
    // 每帧重布局；父作用域只在状态真正变化时被 `p.set` 的观察者机制叫醒一次即可。

    // 滚动容器（同时是下拉手势目标节点）+ 注册处理器。
    // 用 Box(TopStart)：指示器零高叠加在内容之上（不挤压布局，见下方独立作用域）。
    // 滚动偏移跨重组保留（节点每帧重建，偏移存在节点上会随重组被清零）；
    // 用 remember 保住偏移，并注册一次性帧驱动把实时节点偏移同步回来。
    let scroll_off = remember(|| Rc::new(Cell::new(0.0f32)));
    let id_cell = remember(|| Rc::new(Cell::new(None::<NodeId>)));
    let id = add_node(
        Policy::Box(Alignment::TopStart),
        modifier.fill_max_width().VerticalScroll(),
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].scroll = Some(ScrollInfo {
            vertical: true,
            offset: scroll_off.get(),
            content_size: 0.0,
            sticky: None,
        });
    });
    id_cell.set(Some(id));
    let off_c = scroll_off.clone();
    let id_c = id_cell.clone();
    let drv = remember(|| {
        add_frame_driver(move || {
            if let Some(id) = id_c.get() {
                let v = with_rt_pub(|rt| {
                    rt.nodes[id.index()]
                        .scroll
                        .as_ref()
                        .map(|s| s.offset)
                        .unwrap_or(0.0)
                });
                off_c.set(v);
            }
        })
    });
    // 作用域销毁时注销帧驱动，避免驱动随滚动控件消失后继续驻留
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
    let (hp, ht, hd, ha, hr, hk, hs) = (
        pull.clone(),
        target,
        dragging,
        armed,
        refreshing,
        key,
        sim.clone(),
    );
    register_pull_target(
        hk,
        id,
        Box::new(move |phase| match phase {
            PullPhase::Move(d) => {
                let r = (d * DAMP).clamp(0.0, THRESHOLD * 1.5);
                hd.set(true);
                hp.set(r);
            }
            PullPhase::Release(d) => {
                let r = (d * DAMP).clamp(0.0, THRESHOLD * 1.5);
                hd.set(false);
                // 以抬手瞬时位重新播种弹簧（保留速度=0，跟手期间位移由手势直写）
                hs.set((
                    hp.get(),
                    0.0,
                    clock_nanos(),
                    if r >= THRESHOLD { REST } else { 0.0 },
                ));
                if r >= THRESHOLD {
                    ha.set(true);
                    hr.set(true); // 进入刷新态（调用方完成后置回 false）
                    ht.set(REST);
                    crate::widgets::overlay::RunIndexedAction(hk, 0);
                } else {
                    ht.set(0.0);
                }
            }
        }),
    );

    // ── content 独立子作用域（先加，置于背后）──────────────────────────────
    // 下拉/刷新动画期间不会被帧驱动调度，保持静止 —— 这是「只对变化节点重绘」的核心。
    with_content_scope(id, content);

    // ── 指示器独立子作用域（后加，叠加在最上层）──────────────────────────
    // 只组合一次：位移 / 旋转相位由帧驱动写 `Rc<Cell>` 并仅失效本节点（draw-phase）。
    // 旧实现每帧 `schedule_scope` 重组本作用域（刷新可持续数秒），反复重建 Canvas
    // 节点与绘制闭包；现在与 Spinner / Skeleton 走同一条「只重绘、不重组」路径。
    with_content_scope(id, {
        let (dh_o, spin_o, ph_o, node_o) = (
            ind_dh.clone(),
            ind_spin.clone(),
            ind_phase.clone(),
            ind_node.clone(),
        );
        let pull_o = pull.clone();
        move || {
            let colors = theme_colors();
            // 组合期初值：首帧即挂 `refreshing=true` 时立刻可见（此后由帧驱动接管）。
            let spinning0 = refreshing.get() || armed.get();
            let p0 = pull_o.get();
            dh_o.set(if spinning0 { p0.max(REST) } else { p0 });
            spin_o.set(spinning0);
            ph_o.set(0.0);
            let (dh_c, spin_c, ph_c) = (dh_o.clone(), spin_o.clone(), ph_o.clone());
            // 零布局高度：指示器纯绘制覆盖在内容上方，不挤压下方内容。
            // 超出节点的部分仍在滚动容器裁剪区内可见。
            let nid = add_node(
                Policy::Leaf,
                Modifier::new().fill_max_width().height(0.0).no_hit(),
            );
            node_o.set(Some(nid));
            with_rt_pub(|rt| {
                rt.nodes[nid.index()].draw = Some(Box::new(move |b, x, y, w, _h| {
                    let spinning = spin_c.get();
                    // 刷新/armed 态指示器至少画在 REST 停留位（弹簧尚未到达时也可见）。
                    let dh = if spinning {
                        dh_c.get().max(REST)
                    } else {
                        dh_c.get()
                    };
                    if !spinning && dh <= 0.5 {
                        return;
                    }
                    let phase = ph_c.get();
                    let cx = x + w * 0.5;
                    let cy = y + (dh - 14.0).max(14.0); // 谷底附近
                    let depth = dh;
                    // 液面：顶部全宽 → 中心下凹到 depth 的抛物线填充
                    const STEPS: usize = 48;
                    let mut pts = [(0.0f32, 0.0f32); STEPS + 3];
                    pts[0] = (x, y);
                    pts[1] = (x + w, y);
                    for i in 0..=STEPS {
                        let t = 1.0 - i as f32 / STEPS as f32;
                        let py = y + depth * (1.0 - (1.0 - 2.0 * t).powi(2));
                        pts[i + 2] = (x + w * t, py);
                    }
                    b.fill_path(&pts, colors.primary.with_alpha(0.5));
                    // 旋转进度环
                    let outer_r = 14.0;
                    let ring_r = outer_r - 3.0;
                    // 投影（三层同心圆模拟柔和阴影）
                    for (rr, aa) in [
                        (outer_r + 1.5, 0.16f32),
                        (outer_r + 3.0, 0.09),
                        (outer_r + 5.0, 0.05),
                    ] {
                        b.fill_circle(cx, cy + 1.5, rr, Color::rgba(0.0, 0.0, 0.0, aa));
                    }
                    // 白色底盘
                    b.fill_circle(cx, cy, outer_r, Color::rgba(1.0, 1.0, 1.0, 1.0));
                    // 灰色轨道环
                    let track: [(f32, f32); 37] =
                        arc_points(cx, cy, ring_r, 0.0, std::f32::consts::TAU);
                    b.stroke_path(&track, colors.surface_variant, 3.0);
                    // 进度 / 旋转弧
                    if spinning {
                        let a0 = -std::f32::consts::FRAC_PI_2 + phase * std::f32::consts::TAU;
                        let sweep =
                            std::f32::consts::PI * (0.4 + 0.6 * (1.0 - (2.0 * phase - 1.0).abs()));
                        let arc: [(f32, f32); 31] = arc_points(cx, cy, ring_r, a0, a0 + sweep);
                        b.stroke_path(&arc, colors.primary, 3.0);
                    } else {
                        let prog = (dh / THRESHOLD).clamp(0.0, 1.0);
                        let a0 = -std::f32::consts::FRAC_PI_2;
                        let arc: [(f32, f32); 31] =
                            arc_points(cx, cy, ring_r, a0, a0 + std::f32::consts::TAU * prog);
                        b.stroke_path(&arc, colors.primary, 3.0);
                    }
                }));
            });
        }
    });

    // 帧驱动：只写指示器 draw 相位 Cell + 失效该节点，任何作用域都不因动画重组。
    // 位移走核心 `Spring` 解析解（临界阻尼、目标变化保留速度、**帧率无关**），
    // 到位且非刷新态即停止请求续帧——旧实现在此用 `cur + (tg-cur)*0.25` 的定步指数缓动，
    // 120Hz 上收回速度快一倍、掉帧时又拖沓，且驱动只在 remember 里注册、从不注销。
    let drv2 = remember(move || {
        let (p, t, d, a, rf, sim) = (pull, target, dragging, armed, refreshing, sim.clone());
        let (dh_c, spin_c, ph_c, node_c) = (
            ind_dh.clone(),
            ind_spin.clone(),
            ind_phase.clone(),
            ind_node.clone(),
        );
        let spring = Spring::critically_damped(Spring::STIFFNESS_MEDIUM);
        add_frame_driver(move || {
            let spinning = rf.get() || a.get();
            if d.get() {
                // 跟手期间位移由手势直写（本帧只同步到绘制相位）
                pull_indicator_cells(&dh_c, &spin_c, &ph_c, &node_c, p.get(), spinning, 0.0);
                return;
            }
            if !rf.get() && a.get() {
                a.set(false); // 刷新结束解除 armed，target 回 0 收回指示器
            }
            let want = if rf.get() || a.get() { REST } else { 0.0 };
            let spinning = rf.get() || a.get();
            if (t.get() - want).abs() > 1e-6 {
                t.set(want);
            }
            let (tg, cur) = (t.get(), p.get());
            let (f, v, st, seed_tg) = sim.get();
            let (f, st) = if (seed_tg - tg).abs() > 1e-6 {
                (cur, clock_nanos()) // 目标变了：从当前显示位续接
            } else {
                (f, st)
            };
            let elapsed = ((clock_nanos() - st).max(0.0) / 1e9) as f32;
            let (x, vel) = spring.simulate(f, v, tg, elapsed);
            let settled = (x - tg).abs() < 0.4 && vel.abs() < 12.0;
            let next = if settled { tg } else { x };
            sim.set((
                next,
                if settled { 0.0 } else { vel },
                if settled { clock_nanos() } else { st },
                tg,
            ));
            if settled && !spinning {
                if (cur - tg).abs() > 1e-6 {
                    p.set(tg);
                }
                // 静止：最后一次同步后不再请求续帧（帧循环得以休眠）
                pull_indicator_cells(&dh_c, &spin_c, &ph_c, &node_c, tg, false, 0.0);
                return;
            }
            if (cur - next).abs() > 1e-6 {
                p.set(next);
            }
            // 旋转相位按单调帧时钟推进（0.9s 一个来回），只在 draw 阶段消费。
            let phase = if spinning {
                loop_phase(clock_nanos(), 0.0, 0.9, LoopMode::Reverse)
            } else {
                0.0
            };
            pull_indicator_cells(&dh_c, &spin_c, &ph_c, &node_c, next, spinning, phase);
            compose::animation::request_anim_frames();
        })
    });
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv2));
}

/// `OpenContainer(open, closed, opened)` — Material 容器变换（container transform）。
///
/// 底层叠 `closed`，上层叠 `opened`，再在上层盖一层主题底色遮罩：
/// `open=false` 时遮罩 α=1 完全盖住 opened（只剩 closed）；`open=true` 时 α=0
/// opened 完全显现。过渡中 α 由 1→0，opened 淡入、closed 透出 —— 即交叉淡入，
/// 近似「容器从收起态展开为打开态」。核心无分组透明度，故以遮罩实现。
pub fn OpenContainer(
    open: bool,
    modifier: Modifier,
    closed: impl Fn() + 'static,
    opened: impl Fn() + 'static,
) {
    let colors = theme_colors();
    Box(modifier, Alignment::TopStart, move || {
        closed(); // 底层：关闭态内容
        opened(); // 上层：打开态内容（目标为打开时完全可见）
    });
    // 遮罩：主题底色 α=1-t，过渡中淡出 → opened 显、closed 透出（交叉淡入）。
    // draw-phase 推进，零每帧重组。
    fade_mask(
        move || if open { 1u64 } else { 0u64 },
        0.3,
        move |t, b, x, y, w, h| {
            if t < 1.0 {
                b.fill_rect(x, y, w, h, colors.Surface.with_alpha(1.0 - t));
            }
        },
    );
}

/// `Hero(tag) { child }` — 共享元素锚点（简化：入场缩放占位，供路由集成后升级）。
pub fn Hero(tag: &str, modifier: Modifier, child: impl Fn() + 'static) {
    let _ = tag;
    Box(modifier, Alignment::TopStart, child);
}

/// `AnimatedImage(label, playing) { }` — 动画图像占位。
///
/// 核心无 GIF 解码器，提供：图像占位框 + 播放时帧驱动的“播放中”脉冲圆点 +
/// 右下角 `GIF` 角标。真实解码接入后可替换占位绘制。
pub fn AnimatedImage(label: &str, playing: MutableState<bool>, modifier: Modifier) {
    let on = playing.get();
    let colors = theme_colors();
    let l = label.to_string();
    Box(modifier, Alignment::TopStart, move || {
        crate::widgets::ImagePlaceholder(&l, Modifier::new().fill_max_size());
        let colors1 = colors;
        // GIF 角标（左下）：尺寸由文本实测决定，不再写死 24×13
        Canvas(
            Modifier::new().fill_max_size().no_hit(),
            move |b, x, y, _w, h| {
                let s = "GIF";
                let style = tokens::label_small(colors1.on_surface_variant);
                let (tw, th) = measure_text_with_measurer(s, &style);
                let bw = tw + 2.0 * tokens::SPACE_XS;
                let bh = th + tokens::SPACE_XXS;
                let bx = x + tokens::SPACE_XXS;
                let by = y + h - bh - tokens::SPACE_XXS;
                b.fill_rrect(
                    bx,
                    by,
                    bw,
                    bh,
                    tokens::SHAPE_EXTRA_SMALL,
                    colors1.surface_variant,
                );
                b.draw_text(
                    bx + tokens::SPACE_XS,
                    by + tokens::SPACE_XXS * 0.5,
                    s,
                    &style,
                    f32::INFINITY,
                );
            },
        );
        // 播放中脉冲圆点（右上）。节点常驻、相位只在 draw 阶段推进：
        // 播放期间不重组本作用域（旧实现 `remember_infinite_transition` 每帧重组整个
        // AnimatedImage，连带 ImagePlaceholder 与角标一起重 emit）。
        let pulse_on = on;
        let dot = add_node(Policy::Leaf, Modifier::new().fill_max_size().no_hit());
        let phase = draw_phase(
            dot,
            if pulse_on { None } else { Some(0.0) },
            1.0,
            LoopMode::Reverse,
        );
        let colors2 = colors;
        with_rt_pub(|rt| {
            rt.nodes[dot.index()].draw = Some(Box::new(move |b, x, y, w, _h| {
                if !pulse_on {
                    return;
                }
                let cx = x + w - 12.0;
                let cy = y + 12.0;
                b.fill_circle(
                    cx,
                    cy,
                    5.0,
                    colors2.primary.with_alpha(0.4 + 0.6 * phase.get()),
                );
            }));
        });
    });
}
