//! Compose 动画 — 对齐 `androidx.compose.animation.core` 的分层：
//!
//! ```text
//!   Animatable<T, V>       本模块 Animatable<T>：持有「当前值 + 速度」的补间容器
//!   AnimationSpec          本模块 AnimSpec：Tween / Spring / Snap / Keyframes / Infinite
//!   Simulation             各 spec 的解析解（tween 插值、SpringSimulation 阻尼振子）
//!   VectorConverter        本模块 AnimatableValue：T ⇄ AnimationVector([f32;4])
//!   MonotonicFrameClock    runtime::advance_frame 累积的单调墙钟（frame_time_nanos）
//!   Recomposer 动画集合     RUNNING 注册表：只登记「正在跑」的动画，静止即退集
//! ```
//!
//! **两条性能/正确性不变式（均为旧实现的缺口）**
//! 1. *静止即停驱*：对齐 Compose `AnimationState`——只有未到达目标的动画留在注册表里、
//!    由帧时钟调度其作用域重组；`t >= 1` 即退集，该作用域不再每帧重跑，帧循环得以休眠。
//!    旧实现给每个 `animateXAsState` 注册一条**永久**帧驱动且从不注销（作用域销毁后仍在
//!    跑、`remember` 每次重建又加一条），动画一多就常驻重组；现在注册表按 id 自删 +
//!    按作用域存活剪枝，每帧成本从 O(创建过的动画数) 降到 O(在跑的动画数)。
//! 2. *时长以墙钟为准*：对齐 `withFrameNanos`——补间进度用累积帧时钟计算。旧实现按
//!    「帧数 ÷ 假定 60fps」计时，120Hz 屏上 0.3s 动效实际只跑 0.15s、掉帧时又拖长，
//!    且 `SpringSpec` 的物理常数（单位/秒²）根本无从定义。墙钟化后同一 spec 在任何帧率
//!    下历时一致。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::foundation::{Color, Dp, Offset};
use crate::runtime::{
    MutableState, ScopeId, current_scope_id, frame_time_and_scope, frame_time_nanos,
    keyed_remember, mutableStateOf, remember, scope_alive, schedule_scope,
};

// ══════════════════════════════ Easing ══════════════════════════════

/// 缓动 — Compose `Easing`。
pub type Easing = fn(f32) -> f32;

/// 线性。
pub fn linear(t: f32) -> f32 {
    t
}
/// 快进慢出（Compose `FastOutSlowInEasing` 近似）。
pub fn fast_out_slow_in(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}
/// 标准缓出。
pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
/// 标准缓入。
pub fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}
/// 缓入缓出。
pub fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// ── Material 3 标准曲线（Flutter `Curves` 的 cubic-bezier 等价物） ──

/// cubic-bezier(x1, y1, x2, y2) 求值：给定时间分数 `t`，二分找参数 `u` 使 x(u)=t，再取 y(u)。
/// 20 次二分收敛到 ~1e-6，无除法/开方，NaN 只会退回线性。
fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if !(t.is_finite()) || t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let axis = |a: f32, b: f32, u: f32| {
        let iu = 1.0 - u;
        3.0 * iu * iu * u * a + 3.0 * iu * u * u * b + u * u * u
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    let mut u = t;
    for _ in 0..20 {
        let x = axis(x1, x2, u);
        if (x - t).abs() < 1e-6 {
            break;
        }
        if x < t {
            lo = u;
        } else {
            hi = u;
        }
        u = (lo + hi) * 0.5;
    }
    axis(y1, y2, u)
}

/// M3 `emphasized`：起步快、收尾极缓，用于「重要且尺寸较大」的过渡。
pub fn emphasized(t: f32) -> f32 {
    cubic_bezier(t, 0.2, 0.0, 0.0, 1.0)
}
/// M3 `emphasizedAccelerate`：出场 / 加速离开。
pub fn emphasized_accelerate(t: f32) -> f32 {
    cubic_bezier(t, 0.3, 0.0, 0.8, 0.15)
}
/// M3 `emphasizedDecelerate`：入场 / 减速到位。
pub fn emphasized_decelerate(t: f32) -> f32 {
    cubic_bezier(t, 0.05, 0.7, 0.1, 1.0)
}
/// M3 `standard`：普通状态变化（双向都不可见起止时才用）。
pub fn standard(t: f32) -> f32 {
    cubic_bezier(t, 0.2, 0.0, 0.1, 1.0)
}
/// M3 `standardAccelerate`。
pub fn standard_accelerate(t: f32) -> f32 {
    cubic_bezier(t, 0.3, 0.0, 0.8, 0.15)
}
/// M3 `standardDecelerate`。
pub fn standard_decelerate(t: f32) -> f32 {
    cubic_bezier(t, 0.05, 0.7, 0.1, 1.0)
}

// ══════════════════════════ AnimationVector ══════════════════════════

/// 动画向量 — Compose `AnimationVector`（固定 4 分量：标量/尺寸用 1 个，颜色用 4 个）。
pub type AnimVector = [f32; 4];

const ZERO: AnimVector = [0.0; 4];

fn lerp_vec(a: &AnimVector, b: &AnimVector, t: f32) -> AnimVector {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// 值 ⇄ 向量双向转换 — Compose `VectorConverter` / `TwoWayConverter`。
///
/// 补间内核只认 `AnimVector`，于是 float / Dp / Offset / Color 共用同一套仿真实现；
/// 新增可动画类型只需实现这两个函数（对齐 Compose 用 converter 解耦值类型与动画内核）。
pub trait AnimatableValue: Clone + PartialEq + 'static {
    /// `T` → 向量（Compose `convert`）。
    fn to_vector(&self) -> AnimVector;
    /// 向量 → `T`（Compose `transform`）。
    fn from_vector(v: &AnimVector) -> Self;
}

impl AnimatableValue for f32 {
    fn to_vector(&self) -> AnimVector {
        [*self, 0.0, 0.0, 0.0]
    }
    fn from_vector(v: &AnimVector) -> Self {
        v[0]
    }
}

impl AnimatableValue for Dp {
    fn to_vector(&self) -> AnimVector {
        [self.0, 0.0, 0.0, 0.0]
    }
    fn from_vector(v: &AnimVector) -> Self {
        Dp(v[0])
    }
}

impl AnimatableValue for Offset {
    fn to_vector(&self) -> AnimVector {
        [self.x, self.y, 0.0, 0.0]
    }
    fn from_vector(v: &AnimVector) -> Self {
        Offset { x: v[0], y: v[1] }
    }
}

impl AnimatableValue for Color {
    fn to_vector(&self) -> AnimVector {
        [self.r, self.g, self.b, self.a]
    }
    fn from_vector(v: &AnimVector) -> Self {
        Color {
            r: v[0],
            g: v[1],
            b: v[2],
            a: v[3],
        }
    }
}

// ══════════════════════════ AnimationSpec ══════════════════════════

/// 时长补间 — Compose `TweenSpec`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tween {
    pub duration_secs: f32,
    pub easing: Easing,
}

impl Tween {
    pub fn new(duration_secs: f32) -> Self {
        Self {
            duration_secs,
            easing: fast_out_slow_in,
        }
    }
    pub fn with_easing(mut self, e: Easing) -> Self {
        self.easing = e;
        self
    }
}

impl Default for Tween {
    fn default() -> Self {
        Self::new(0.3)
    }
}

/// 位移/速度判停的绝对下限（0 距离时不至于永不终止）。
const ABS_EPSILON: f32 = 1e-4;

/// 弹簧参数 — Compose `SpringSpec` + `SpringSimulation`。
///
/// 物理模型与 Compose 一致：`mass = 1`，固有频率 `ω0 = sqrt(stiffness)`，
/// 阻尼系数 `damping = 2 · damping_ratio · ω0`；闭式解见 [`Spring::simulate`]。
///
/// **与 Compose 的一处必要偏差**：Compose 的 `visibilityThreshold` 是**绝对**值
/// （`Float.VisibilityThreshold = 1.5f`、`Alpha` 用 `0.01`），因为它的动画值类型自带单位。
/// 本库的 `f32` 既表示 0..1 归一化 alpha 又表示像素尺寸，绝对阈值无法通用，故
/// `visibility_threshold` 取**相对**语义：实际阈值 = `visibility_threshold · |target − from|`
/// 再与 [`ABS_EPSILON`] 取较大者。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Spring {
    /// 刚度（Compose `StiffnessLow/Medium/High`）。
    pub stiffness: f32,
    /// 阻尼比（<1 欠阻尼回弹，=1 临界，>1 过阻尼）。
    pub damping_ratio: f32,
    /// 相对收敛阈值（见类型文档）。
    pub visibility_threshold: f32,
}

impl Spring {
    /// Compose `StiffnessLow`。
    pub const STIFFNESS_LOW: f32 = 1500.0;
    /// Compose `StiffnessMedium`。
    pub const STIFFNESS_MEDIUM: f32 = 4000.0;
    /// Compose `StiffnessHigh`（Material 入场动效）。
    pub const STIFFNESS_HIGH: f32 = 10000.0;

    /// Material 入场默认值：轻微回弹 + 高刚度。
    pub fn default_spring() -> Self {
        Self {
            stiffness: Self::STIFFNESS_HIGH,
            damping_ratio: 0.8,
            visibility_threshold: 0.001,
        }
    }
    /// 高回弹（Compose `DampingRatioHighBouncy`）。
    pub fn bouncy() -> Self {
        Self {
            stiffness: Self::STIFFNESS_MEDIUM,
            damping_ratio: 0.2,
            visibility_threshold: 0.001,
        }
    }
    /// 临界阻尼（无过冲、最快到位）。
    pub fn critically_damped(stiffness: f32) -> Self {
        Self {
            stiffness,
            damping_ratio: 1.0,
            visibility_threshold: 0.001,
        }
    }
    /// 过阻尼（缓慢逼近，无回弹）。
    pub fn overdamped(stiffness: f32) -> Self {
        Self {
            stiffness,
            damping_ratio: 10.0,
            visibility_threshold: 0.001,
        }
    }

    /// 单分量解析解：从 `(x0, v0)` 向 `target` 仿真 `t` 秒，返回 `(value, velocity)`。
    ///
    /// 三种阻尼状态的闭式解（`ω0 = sqrt(k)`）：
    /// - 欠阻尼 `ζ<1`：`x = tgt + e^(−ζω0 t)(A cos ωd t + B sin ωd t)`，`ωd = ω0 sqrt(1−ζ²)`
    /// - 临界 `ζ≈1`：`x = tgt + e^(−ω0 t)(A + B t)`
    /// - 过阻尼 `ζ>1`：`x = tgt + C₁ e^(r₁ t) + C₂ e^(r₂ t)`，`r₁₂ = −ζω0 ± ω0 sqrt(ζ²−1)`
    pub fn simulate(&self, x0: f32, v0: f32, target: f32, t: f32) -> (f32, f32) {
        let w0 = self.stiffness.max(1e-6).sqrt();
        let zeta = self.damping_ratio;
        let a = x0 - target;
        if (zeta - 1.0).abs() < 1e-3 {
            let b = v0 + w0 * a;
            let e = (-w0 * t).exp();
            let f = a + b * t;
            (target + e * f, e * (b - w0 * f))
        } else if zeta < 1.0 {
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            let b = (v0 + zeta * w0 * a) / wd;
            let e = (-zeta * w0 * t).exp();
            let (sin, cos) = (wd * t).sin_cos();
            let f = a * cos + b * sin;
            let fp = wd * (b * cos - a * sin);
            (target + e * f, e * (fp - zeta * w0 * f))
        } else {
            let s = w0 * (zeta * zeta - 1.0).sqrt();
            let r1 = -zeta * w0 + s;
            let r2 = -zeta * w0 - s;
            let c1 = (v0 - r2 * a) / (r1 - r2);
            let c2 = a - c1;
            let e1 = finite_exp(r1 * t);
            let e2 = finite_exp(r2 * t);
            (target + c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        }
    }

    /// 四分量批量解析解（`Color` 动画等一次推进 4 个轴）。
    ///
    /// 阻尼系数、指数与三角函数只与弹簧参数和 `t` 有关，四个轴共用同一组：
    /// 逐轴调用 [`Self::simulate`] 会把 `exp`/`sin_cos` 重复算 4 次。
    /// 无位移且无初速度的轴（如标量动画的第 1..3 轴）恒等于目标值，直接跳过。
    pub(crate) fn simulate_vec(
        &self,
        x0: [f32; 4],
        v0: [f32; 4],
        target: [f32; 4],
        t: f32,
    ) -> ([f32; 4], [f32; 4]) {
        let w0 = self.stiffness.max(1e-6).sqrt();
        let zeta = self.damping_ratio;
        let mut out = target;
        let mut vel = [0.0f32; 4];
        if (zeta - 1.0).abs() < 1e-3 {
            let e = (-w0 * t).exp();
            for i in 0..4 {
                let a = x0[i] - target[i];
                if a == 0.0 && v0[i] == 0.0 {
                    continue;
                }
                let b = v0[i] + w0 * a;
                let f = a + b * t;
                out[i] = target[i] + e * f;
                vel[i] = e * (b - w0 * f);
            }
        } else if zeta < 1.0 {
            let wd = w0 * (1.0 - zeta * zeta).sqrt();
            let e = (-zeta * w0 * t).exp();
            let (sin, cos) = (wd * t).sin_cos();
            for i in 0..4 {
                let a = x0[i] - target[i];
                if a == 0.0 && v0[i] == 0.0 {
                    continue;
                }
                let b = (v0[i] + zeta * w0 * a) / wd;
                let f = a * cos + b * sin;
                let fp = wd * (b * cos - a * sin);
                out[i] = target[i] + e * f;
                vel[i] = e * (fp - zeta * w0 * f);
            }
        } else {
            let s = w0 * (zeta * zeta - 1.0).sqrt();
            let r1 = -zeta * w0 + s;
            let r2 = -zeta * w0 - s;
            let e1 = finite_exp(r1 * t);
            let e2 = finite_exp(r2 * t);
            for i in 0..4 {
                let a = x0[i] - target[i];
                if a == 0.0 && v0[i] == 0.0 {
                    continue;
                }
                let c1 = (v0[i] - r2 * a) / (r1 - r2);
                let c2 = a - c1;
                out[i] = target[i] + c1 * e1 + c2 * e2;
                vel[i] = c1 * r1 * e1 + c2 * r2 * e2;
            }
        }
        (out, vel)
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self::default_spring()
    }
}

/// 指数衰减的下溢/溢出兜底（极端 stiffness 时 e^x 可能 inf）。
fn finite_exp(x: f32) -> f32 {
    let e = x.exp();
    if e.is_finite() {
        e
    } else {
        0.0
    }
}

/// 循环波型 — Compose `RepeatMode`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LoopMode {
    /// 0→1 锯齿后重启（`RepeatMode.Restart`，旋转 / 进度环）。
    Restart,
    /// 0→1→0（`RepeatMode.Reverse`，呼吸 / 脉动）。
    Reverse,
}

/// 无限循环 — Compose `InfiniteRepeatableSpec` 的最小子集（永不自动退集）。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Infinite {
    pub period_secs: f32,
    pub mode: LoopMode,
}

/// 立即跳变 — Compose `SnapSpec`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Snap;

/// 重复播放一段**时长规格** — Compose `RepeatableSpec`。
///
/// `iterations` 次迭代后停止；[`LoopMode::Reverse`] 让奇数次迭代反向播放（来回摆动）。
/// 内层只接受时长规格（Tween / Keyframes / Snap），对齐 Compose
/// `repeatable(iterations, animation: DurationBasedAnimationSpec, repeatMode)`；
/// 若误传弹簧等无时长规格，则退化为同等时长的线性补间。
#[derive(Clone, PartialEq, Debug)]
pub struct Repeatable {
    iterations: i32,
    animation: Box<AnimSpec>,
    mode: LoopMode,
}

/// `repeatable(iterations, tween(0.3), RepeatMode.Restart)`。
pub fn repeatable(iterations: i32, animation: impl Into<AnimSpec>, mode: LoopMode) -> Repeatable {
    Repeatable {
        iterations: iterations.max(1),
        animation: Box::new(animation.into()),
        mode,
    }
}

/// 指数摩擦衰减 — Compose `ExponentialDecaySpec`（`animateDecay` 的规格）。
///
/// 速度按 `v(t) = v₀·e^(−kt)` 衰减、位移是它的积分，`k = 4·friction`（1/秒）。
/// `velocity_threshold` 是判停的绝对速度阈值（Compose `absVelocityThreshold`）。
/// 与 Compose 一致，衰减只作用于标量的首分量（`Float` 语义），其余分量保持在起点。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Decay {
    pub friction: f32,
    pub velocity_threshold: f32,
}

impl Decay {
    /// Compose `exponentialDecay(frictionMultiplier = 1f, absVelocityThreshold = 0.1f)`。
    pub fn new() -> Self {
        Self {
            friction: 1.0,
            velocity_threshold: 0.1,
        }
    }
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.velocity_threshold = threshold.max(0.0);
        self
    }
}

impl Default for Decay {
    fn default() -> Self {
        Self::new()
    }
}

/// 便捷构造：`exponential_decay(1.0, 0.1)`（Compose 参数顺序）。
pub fn exponential_decay(friction: f32, velocity_threshold: f32) -> Decay {
    Decay {
        friction: friction.max(0.0),
        velocity_threshold: velocity_threshold.max(0.0),
    }
}

/// 关键帧单站（向量域）。
#[derive(Clone, PartialEq, Debug)]
struct KeyStop {
    at_secs: f32,
    value: AnimVector,
    /// 到达本站所用缓动（Compose `to(value) with Easing`）。
    easing: Easing,
}

/// 关键帧规格（向量域）— Compose `KeyframesSpec`。
#[derive(Clone, PartialEq, Debug)]
pub struct KeyframesVec {
    duration_secs: f32,
    stops: Vec<KeyStop>,
}

/// 动画规格 — Compose `AnimationSpec` 的并集类型。
///
/// [`Tween`] / [`Spring`] / [`Snap`] / [`Keyframes<T>`] / [`Infinite`] 经 `From` 汇入，
/// 故 `animate_float_as_state(x, Tween::new(0.3))` 与 `animate_float_as_state(x, spring())`
/// 共用同一签名（对齐 Compose 的 `animationSpec` 默认参数重载）。
#[derive(Clone, PartialEq, Debug)]
pub enum AnimSpec {
    Tween(Tween),
    Spring(Spring),
    Snap,
    Keyframes(KeyframesVec),
    Infinite(Infinite),
    Repeat(Repeatable),
    Decay(Decay),
}

impl From<Tween> for AnimSpec {
    fn from(t: Tween) -> Self {
        AnimSpec::Tween(t)
    }
}
impl From<Spring> for AnimSpec {
    fn from(s: Spring) -> Self {
        AnimSpec::Spring(s)
    }
}
impl From<Snap> for AnimSpec {
    fn from(_: Snap) -> Self {
        AnimSpec::Snap
    }
}
impl From<Infinite> for AnimSpec {
    fn from(i: Infinite) -> Self {
        AnimSpec::Infinite(i)
    }
}
impl From<Repeatable> for AnimSpec {
    fn from(r: Repeatable) -> Self {
        AnimSpec::Repeat(r)
    }
}
impl From<Decay> for AnimSpec {
    fn from(d: Decay) -> Self {
        AnimSpec::Decay(d)
    }
}
impl<T: AnimatableValue> From<Keyframes<T>> for AnimSpec {
    fn from(k: Keyframes<T>) -> Self {
        AnimSpec::Keyframes(KeyframesVec {
            duration_secs: k.duration_secs,
            stops: k
                .stops
                .iter()
                .map(|s| KeyStop {
                    at_secs: s.at_secs,
                    value: s.value.to_vector(),
                    easing: s.easing,
                })
                .collect(),
        })
    }
}

impl AnimSpec {
    /// 是否永不终止（无限动画常驻注册表，随宿主作用域销毁被剪枝）。
    fn is_infinite(&self) -> bool {
        matches!(self, AnimSpec::Infinite(_))
    }

    /// 有限规格的时长（秒）；`Spring` / `Infinite` / `Decay` 无固定时长 → `None`。
    pub fn duration_secs(&self) -> Option<f32> {
        match self {
            AnimSpec::Tween(t) => Some(t.duration_secs.max(0.0)),
            AnimSpec::Keyframes(k) => Some(k.duration_secs.max(0.0)),
            AnimSpec::Repeat(r) => r
                .animation
                .duration_secs()
                .map(|d| d * r.iterations.max(1) as f32),
            AnimSpec::Snap => Some(0.0),
            AnimSpec::Spring(_) | AnimSpec::Infinite(_) | AnimSpec::Decay(_) => None,
        }
    }
}

/// 便捷构造：`tween(0.3)`。
pub fn tween(duration_secs: f32) -> Tween {
    Tween::new(duration_secs)
}
/// 便捷构造：`spring()`（Material 入场参数）。
pub fn spring() -> Spring {
    Spring::default_spring()
}
/// 便捷构造：`spring_with(damping_ratio, stiffness)`（Compose 参数顺序）。
pub fn spring_with(damping_ratio: f32, stiffness: f32) -> Spring {
    Spring {
        stiffness,
        damping_ratio,
        visibility_threshold: 0.001,
    }
}
/// 便捷构造：`snap()`。
pub fn snap() -> Snap {
    Snap
}
/// 便捷构造：`keyframes::<f32>(0.4)`。
pub fn keyframes<T: AnimatableValue>(duration_secs: f32) -> Keyframes<T> {
    Keyframes::new(duration_secs)
}

/// 类型化关键帧序列 — Compose `KeyframesSpec` 构建器。
///
/// ```ignore
/// animate_float_as_state(
///     target,
///     keyframes::<f32>(0.4)
///         .to(0.0, 0.0)                     // 起点
///         .to_with(1.25, 0.15, ease_out_cubic) // 过冲
///         .to(1.0, 0.4),                    // 终点
/// )
/// ```
/// 未提供 0s 站时，从「当前值」以线性平滑接到第一个显式站。
#[derive(Clone, PartialEq, Debug)]
pub struct Keyframes<T: AnimatableValue> {
    duration_secs: f32,
    stops: Vec<TypedStop<T>>,
}

#[derive(Clone, PartialEq, Debug)]
struct TypedStop<T: AnimatableValue> {
    at_secs: f32,
    value: T,
    easing: Easing,
}

impl<T: AnimatableValue> Keyframes<T> {
    pub fn new(duration_secs: f32) -> Self {
        Self {
            duration_secs,
            stops: Vec::new(),
        }
    }
    /// `to(value) atSeconds s`（线性到达）。
    pub fn to(self, value: T, at_secs: f32) -> Self {
        self.to_with(value, at_secs, linear)
    }
    /// `to(value) atSeconds s with Easing`。
    pub fn to_with(mut self, value: T, at_secs: f32, easing: Easing) -> Self {
        self.stops.push(TypedStop {
            at_secs,
            value,
            easing,
        });
        self
    }
}

// ══════════════════════════ 活跃动画注册表 ══════════════════════════

/// 注册表条目：一个「正在跑」的动画。
///
/// `last_frame` = 最近一次被其宿主组合采样（[`Animatable::advance`]）的帧号；
/// 落后超过 [`STALE_FRAMES`] 帧即视为「组合已不再驱动它」（控件被条件摘除 /
/// remember 槽位重初始化）→ 随帧时钟剪枝消失，绝不留常驻驱动。
#[derive(Clone, Copy)]
struct Reg {
    scope: ScopeId,
    last_frame: u64,
}

/// 判定「不再被驱动」的落后帧数（1 帧容差：本帧注册 / 事件帧起驱不该被误剪）。
const STALE_FRAMES: u64 = 2;

thread_local! {
    /// 正在推进的动画集合 — 对齐 Compose `Recomposer` 的 `runningAnimations`。
    /// 只登记「未静止」的动画：每帧只调度这些作用域；补间到位由 [`unregister`] 自删，
    /// 不再被采样或作用域销毁（含无限动画）由 [`tick_animations`] 剪枝。
    ///
    /// 用 `HashMap` 而非 `Vec`：`touch`/`unregister` 每帧每动画都会发生，
    /// 线性扫描在 N 条并发动画下是 O(N²)/帧；哈希后是 O(N)。
    static RUNNING: RefCell<HashMap<u64, Reg>> = RefCell::new(HashMap::new());
    /// 动画实例自增 id。
    static SEQ: Cell<u64> = const { Cell::new(1) };
    /// 本帧帧号快照（由 [`tick_animations`] 写入）：注册/续期时读取它而不是 runtime，
    /// 因为采样发生在 `peek_mut` 的 runtime 借用内，再借 runtime 会 RefCell 重入。
    static FRAME_NOW: Cell<u64> = const { Cell::new(0) };
    /// 外部帧驱动（[`request_anim_frames`]）的续帧请求；读取即清除，宿主每帧读一次。
    static FRAME_REQUEST: Cell<bool> = const { Cell::new(false) };
    /// 「稍后再来一帧」的最早相对延时（秒）；宿主空闲时据此设置 `WaitUntil`。
    static WAKE_DELAY: Cell<Option<f32>> = const { Cell::new(None) };
}

/// 本帧是否需要继续推进（宿主帧循环据此决定连续帧或休眠）。
///
/// 判据一：注册表非空（有补间 / 无限动画在跑，注册表本身即「静止即空」）。
/// 判据二：控件自注册的帧驱动显式 [`request_anim_frames`] 请求（读取即清除）。
pub fn animations_active() -> bool {
    let running = RUNNING.with(|r| !r.borrow().is_empty());
    let requested = FRAME_REQUEST.with(|f| {
        let v = f.get();
        f.set(false);
        v
    });
    running || requested
}

/// 显式请求平台在本帧之后再推进一帧（自定义 `add_frame_driver` 动画用）。
///
/// 内置原语（`animate_*_as_state` / `remember_infinite_transition`）经注册表自动续帧，
/// 不需要调用本函数。
pub fn request_anim_frames() {
    FRAME_REQUEST.with(|f| f.set(true));
}

/// 请求宿主在 `delay_secs` 秒后再推进一帧（相对当前时刻）。
///
/// 与 [`request_anim_frames`] 的区别：后者要**连续**帧（补间/无限动画），前者只要
/// **一次**定点唤醒——光标闪烁、自动轮播这类「几百毫秒才变一次」的节奏用它，
/// 避免聚焦输入框 / 开着轮播时把整个应用拖进 60fps 连续重绘。
/// 取多次请求中的最小延时；`delay_secs <= 0` 退化为立即续帧。
pub fn request_anim_frame_after(delay_secs: f32) {
    if delay_secs <= 0.0 {
        request_anim_frames();
        return;
    }
    WAKE_DELAY.with(|c| {
        let cur = c.get();
        if cur.map_or(true, |d| delay_secs < d) {
            c.set(Some(delay_secs));
        }
    });
}

/// 取出并清除「最早的定点唤醒」延时（秒）。宿主在空闲帧之后调用：
/// `Some(d)` → `ControlFlow::WaitUntil(now + d)`；`None` → 真正休眠。
pub fn take_wake_delay() -> Option<f32> {
    WAKE_DELAY.with(|c| c.take())
}

/// 当前注册表中的动画数量（测试 / 诊断：验证「静止即退集」与无泄漏）。
pub fn running_animations() -> usize {
    RUNNING.with(|r| r.borrow().len())
}

fn alloc_id() -> u64 {
    SEQ.with(|s| {
        let id = s.get();
        s.set(id + 1);
        id
    })
}

fn current_frame_now() -> u64 {
    FRAME_NOW.with(|f| f.get())
}

/// 登记（或续期）一个在跑的动画。
///
/// 续期同时刷新归属作用域：`remember` 槽位被复用时（父作用域重建了子作用域、
/// 条件分支切换等），动画的采样者已换成新的 `ScopeId`，注册表若仍持旧句柄，
/// 帧时钟就会去调度一个死作用域 → 动画静默停摆（与「驱动不得捕获字面 NodeId」同坑）。
fn touch(id: u64, scope: ScopeId) {
    let frame = current_frame_now();
    RUNNING.with(|r| {
        let mut map = r.borrow_mut();
        if let Some(e) = map.get_mut(&id) {
            e.last_frame = frame;
            e.scope = scope;
        } else {
            map.insert(
                id,
                Reg {
                    scope,
                    last_frame: frame,
                },
            );
            // 新起一段动画：即便宿主正处于空闲，也要至少再推进一帧（否则首帧不动）
            FRAME_REQUEST.with(|f| f.set(true));
        }
    });
}

fn unregister(id: u64) {
    RUNNING.with(|r| {
        r.borrow_mut().remove(&id);
    });
}

/// 帧时钟推进（`runtime::advance_frame` 每帧调用）：
/// ① 剪掉「作用域已销毁」或「本帧不再被组合采样」的动画（无限动画同样随宿主消失）；
/// ② 调度存活动画的作用域做定向重组。
pub(crate) fn tick_animations() {
    FRAME_NOW.with(|f| f.set(crate::runtime::current_frame()));
    RUNNING.with(|r| {
        let mut regs = r.borrow_mut();
        if regs.is_empty() {
            return;
        }
        let frame = current_frame_now();
        // 原地剪枝（不再每帧 take/回填整个注册表：回填会重新哈希全部条目）。
        // `age == 0` = 本帧刚被组合采样过（作用域必然存活），省掉一次 runtime 借用。
        regs.retain(|_, e| {
            let age = frame.saturating_sub(e.last_frame);
            age < STALE_FRAMES && (age == 0 || scope_alive(e.scope))
        });
        for e in regs.values() {
            // `schedule_scope` 内部按 scope 去重（updateTransition 多个分量共享作用域），
            // 这里不必再做二次集合。
            schedule_scope(e.scope);
        }
    });
}

// ══════════════════════════ Animatable 内核 ══════════════════════════

/// 补间状态 — Compose `Animatable` 的内部 `MutexState` + 当前 `Simulation`。
#[derive(Clone, PartialEq)]
struct AnimData {
    /// 注册表 id（静止 / 作用域销毁时据此退集）。
    id: u64,
    /// 归属作用域（每帧由帧时钟调度它做定向重组）。
    scope: ScopeId,
    spec: AnimSpec,
    /// 本次仿真的起点值（中断续接时 = 当时的当前值）。
    from: AnimVector,
    /// 本次仿真的起点速度（中断续接保留，对齐 Compose 的速度传递）。
    from_velocity: AnimVector,
    target: AnimVector,
    value: AnimVector,
    velocity: AnimVector,
    start_nanos: f64,
    last_nanos: f64,
    running: bool,
    /// 无限动画：永不自动退集（作用域销毁时由 [`tick_animations`] 剪枝）。
    infinite: bool,
}

impl AnimData {
    /// 静止于 `target` 的初始态（Compose `Animatable(targetValue)`：首帧即目标值、不入场补间）。
    /// 无限规格例外：首次组合即起驱。
    fn settled(scope: ScopeId, target: AnimVector, spec: AnimSpec, now: f64) -> Self {
        let infinite = spec.is_infinite();
        let mut data = Self {
            id: alloc_id(),
            scope,
            spec,
            from: target,
            from_velocity: ZERO,
            target,
            value: target,
            velocity: ZERO,
            start_nanos: now,
            last_nanos: now,
            running: infinite,
            infinite,
        };
        if infinite {
            touch(data.id, scope);
            data.advance(now);
        }
        data
    }

    /// 起一段新仿真：从当前值 + 当前速度出发（对齐 `Animatable.animateTo`）。
    fn start(&mut self, target: AnimVector, spec: AnimSpec, now: f64) {
        self.from = self.value;
        self.from_velocity = self.velocity;
        self.target = target;
        self.spec = spec;
        self.infinite = self.spec.is_infinite();
        self.start_nanos = now;
        self.last_nanos = now;
        // 0 时长规格（snap / tween(0)）等价直接落位，不进注册表
        if !self.infinite && self.spec.duration_secs() == Some(0.0) {
            self.value = target;
            self.velocity = ZERO;
            self.running = false;
            unregister(self.id);
            return;
        }
        self.running = true;
        touch(self.id, self.scope);
    }

    /// 推进到 `now`（同一时间戳重复调用幂等：`dt == 0` 时不改速度估计）。
    ///
    /// **不克隆 spec**：`match &self.spec` 只借用；旧写法 `self.spec.clone()` 每帧深拷贝
    /// `Keyframes` 的站点 `Vec`（动画期间每帧一次堆分配）。
    fn advance(&mut self, now: f64) {
        if !self.running {
            return;
        }
        // 同一时间戳重复采样（一帧内多次重组 / 先采样后逐帧调度）：仿真结果不变，
        // 只需续期注册表——省掉重复的弹簧闭式解，且保证 `last_frame` 仍是最新帧。
        if now <= self.last_nanos {
            touch(self.id, self.scope);
            return;
        }
        let elapsed = ((now - self.start_nanos).max(0.0) / 1e9) as f32;
        let dt = ((now - self.last_nanos).max(0.0) / 1e9) as f32;
        let (from, from_v, target) = (self.from, self.from_velocity, self.target);
        let mut next = self.value;
        // 解析速度（弹簧/衰减）；`None` 表示用有限差分估计。
        let mut analytic_vel: Option<AnimVector> = None;
        let mut finished = false;
        match &self.spec {
            AnimSpec::Tween(t) => {
                let p = (elapsed / t.duration_secs.max(1e-6)).min(1.0);
                next = lerp_vec(&from, &target, (t.easing)(p));
                finished = p >= 1.0;
            }
            AnimSpec::Keyframes(k) => {
                let secs = elapsed.min(k.duration_secs.max(1e-6));
                next = keyframe_sample(k, from, secs);
                finished = elapsed >= k.duration_secs;
            }
            AnimSpec::Spring(sp) => {
                // 相对阈值（见 `Spring` 文档）：位移与速度同时到位才算结束。
                // 四轴共享同一组指数/三角系数（批量解析解），标量动画的其余轴直接跳过。
                let (vals, vels) = sp.simulate_vec(from, from_v, target, elapsed);
                let mut vel = ZERO;
                let mut all_in_range = true;
                for i in 0..4 {
                    if from[i] == target[i] && from_v[i] == 0.0 {
                        next[i] = target[i];
                        continue;
                    }
                    next[i] = vals[i];
                    vel[i] = vels[i];
                    let span = (target[i] - from[i]).abs();
                    let eps = (sp.visibility_threshold * span).max(ABS_EPSILON);
                    if (vals[i] - target[i]).abs() > eps || vels[i].abs() > eps * 4.0 {
                        all_in_range = false;
                    }
                }
                analytic_vel = Some(vel);
                finished = all_in_range;
            }
            AnimSpec::Infinite(inf) => {
                let phase = loop_phase(now, self.start_nanos, inf.period_secs, inf.mode);
                // 无限循环不做速度估计（重启处有限差分会给出假冲量）
                next = [phase, 0.0, 0.0, 0.0];
            }
            AnimSpec::Repeat(r) => {
                // 对齐 Compose `RepeatableSpec`：按时长规格迭代，Reverse 奇数次反向。
                let dur = r.animation.duration_secs().unwrap_or(0.0).max(1e-6);
                let iters = r.iterations.max(1);
                let total = dur * iters as f32;
                let t = elapsed.min(total);
                let iter = ((t / dur).floor() as i32).min(iters - 1);
                let mut p = ((t - iter as f32 * dur) / dur).clamp(0.0, 1.0);
                if matches!(r.mode, LoopMode::Reverse) && iter % 2 == 1 {
                    p = 1.0 - p;
                }
                next = sample_duration_spec(&r.animation, from, target, p);
                finished = elapsed >= total;
            }
            AnimSpec::Decay(d) => {
                // 指数摩擦：v(t)=v₀e^(−kt)，位移为积分；k = 4·friction（1/秒）。
                let k = 4.0 * d.friction.max(0.01);
                let v0 = from_v[0];
                let e = (-k * elapsed).exp();
                next = from;
                next[0] = from[0] + v0 / k * (1.0 - e);
                analytic_vel = Some([v0 * e, 0.0, 0.0, 0.0]);
                finished = (v0 * e).abs() < d.velocity_threshold.max(ABS_EPSILON);
            }
            AnimSpec::Snap => finished = true,
        }
        match analytic_vel {
            Some(v) => {
                self.value = next;
                self.velocity = v;
            }
            None => self.set_value(next, dt),
        }
        // 衰减没有「目标值」：当前位置即收敛点，结尾精确落位而不是跳回起点。
        if matches!(self.spec, AnimSpec::Decay(_)) {
            self.target = next;
        }
        self.last_nanos = now;
        if finished {
            // 收尾取值：`Repeat(Reverse)` 末次迭代逆向结束（停在起点而非目标），
            // 衰减的收敛点已在采样里写入 `next`；其余规格按目标精确落位
            // （弹簧的判停阈值可能留一点点残差，落位消除它）。
            self.value = match &self.spec {
                // 衰减没有目标值：收敛点就是最后一次采样。
                AnimSpec::Decay(_) => next,
                // Reverse 的末次迭代逆向播放，结束在起点而不是目标。
                AnimSpec::Repeat(r) if matches!(r.mode, LoopMode::Reverse) => next,
                _ => self.target,
            };
            self.velocity = ZERO;
            self.running = false;
            unregister(self.id);
        } else {
            // 被组合采样到 = 本帧仍在驱动 → 续期注册表条目（见 STALE_FRAMES 剪枝）
            touch(self.id, self.scope);
        }
    }

    /// 写入新值并用有限差分估计速度（供中断续接喂给弹簧仿真）。
    fn set_value(&mut self, next: AnimVector, dt: f32) {
        if dt > 1e-6 {
            let inv = 1.0 / dt;
            self.velocity = [
                (next[0] - self.value[0]) * inv,
                (next[1] - self.value[1]) * inv,
                (next[2] - self.value[2]) * inv,
                (next[3] - self.value[3]) * inv,
            ];
        }
        self.value = next;
    }
}

/// 按时长规格的归一化进度 `p ∈ 0..1` 采样（`Repeatable` 的单次迭代）。
/// 无时长规格（弹簧等）退化为线性插值——Compose 同样只允许 `DurationBasedAnimationSpec`。
fn sample_duration_spec(spec: &AnimSpec, from: AnimVector, target: AnimVector, p: f32) -> AnimVector {
    match spec {
        AnimSpec::Tween(t) => lerp_vec(&from, &target, (t.easing)(p)),
        AnimSpec::Keyframes(k) => keyframe_sample(k, from, p * k.duration_secs.max(1e-6)),
        AnimSpec::Snap => target,
        _ => lerp_vec(&from, &target, p),
    }
}

/// 关键帧采样：`secs` 落在哪两站之间、用后站的缓动插值。
fn keyframe_sample(k: &KeyframesVec, from: AnimVector, secs: f32) -> AnimVector {
    if k.stops.is_empty() {
        return from;
    }
    // 隐式起点 (0s, from)：从「当前值」平滑接到第一个显式站
    let mut prev_v = from;
    let mut prev_t = 0.0f32;
    for stop in &k.stops {
        if secs <= stop.at_secs {
            let span = (stop.at_secs - prev_t).max(1e-6);
            let local = ((secs - prev_t) / span).clamp(0.0, 1.0);
            return lerp_vec(&prev_v, &stop.value, (stop.easing)(local));
        }
        prev_v = stop.value;
        prev_t = stop.at_secs;
    }
    prev_v
}

// ══════════════════════════ 公共 API ══════════════════════════

/// 补间容器句柄 — Compose `Animatable<T, V>`。
///
/// 与 `animateXAsState` 的区别：目标由调用方显式 `animate_to` / `snap_to` 下达
/// （Compose 用 `LaunchedEffect` 下达；本库在组合期或帧驱动里下达皆可）。
pub struct Animatable<T: AnimatableValue> {
    st: MutableState<AnimData>,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: AnimatableValue> Clone for Animatable<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: AnimatableValue> Copy for Animatable<T> {}

/// `remember { Animatable(initialValue) }` — 首次组合即静止于 `initial`（不补间）。
pub fn remember_animatable<T: AnimatableValue>(initial: T) -> Animatable<T> {
    let scope = current_scope_id().expect("animation called outside composition scope");
    let st = remember(|| {
        mutableStateOf(AnimData::settled(
            scope,
            initial.to_vector(),
            AnimSpec::Snap,
            frame_time_nanos(),
        ))
    });
    Animatable {
        st,
        _marker: std::marker::PhantomData,
    }
}

impl<T: AnimatableValue> Animatable<T> {
    /// 起一段补间（对齐 `Animatable.animateTo(target, spec)`）；同目标同规格重复下达不打断。
    pub fn animate_to(&self, target: T, spec: impl Into<AnimSpec>) {
        let spec = spec.into();
        let vec = target.to_vector();
        let now = frame_time_nanos();
        self.st.peek_mut(|a| {
            if a.target == vec && a.spec == spec && (a.running || a.value == vec) {
                return;
            }
            a.start(vec, spec, now);
        });
    }

    /// 立即落位（对齐 `Animatable.snapTo`）。
    pub fn snap_to(&self, target: T) {
        let vec = target.to_vector();
        self.st.peek_mut(|a| {
            a.from = vec;
            a.target = vec;
            a.value = vec;
            a.velocity = ZERO;
            a.from_velocity = ZERO;
            a.running = false;
            unregister(a.id);
        });
    }

    /// 以初速度做衰减滑行（对齐 Compose `Animatable.animateDecay(initialVelocity, decaySpec)`）。
    ///
    /// 只作用于标量首分量（`Float` 语义）：手势/拖拽结束时把指针速度交给它，
    /// 按 [`Decay`] 的指数摩擦自然停住；到位自动退出注册表。
    pub fn animate_decay(&self, velocity: f32, spec: Decay) {
        let now = frame_time_nanos();
        self.st.peek_mut(|a| {
            a.from = a.value;
            a.from_velocity = [velocity, 0.0, 0.0, 0.0];
            a.target = a.value;
            a.spec = AnimSpec::Decay(spec);
            a.start_nanos = now;
            a.last_nanos = now;
            a.running = true;
            touch(a.id, a.scope);
        });
    }

    /// 读当前值（组合期；顺带按墙钟推进）。采样它的作用域即动画归属作用域。
    pub fn value(&self) -> T {
        let (now, sid) = frame_time_and_scope();
        // 采样与取值合并到一次 runtime 借用里（旧写法两次 `peek_mut`）。
        self.st.peek_mut(|a| {
            if let Some(s) = sid {
                a.scope = s;
            }
            a.advance(now);
            T::from_vector(&a.value)
        })
    }

    /// 是否仍在补间（Compose `AnimationState.isRunning`）。
    pub fn is_running(&self) -> bool {
        self.st.peek_mut(|a| a.running)
    }

    /// 首分量当前速度（单位/秒）：手势结束时把指针速度移交给动画用。
    pub fn velocity(&self) -> f32 {
        self.st.peek_mut(|a| a.velocity[0])
    }
}

/// 通用 `animateValueAsState`：目标 / 规格变化即从当前值（含速度）补间到新目标。
///
/// 对齐 Compose：首帧返回值即目标（不入场补间），此后目标一变就重开一段仿真，
/// 仿真到位自动退出注册表（见模块头性能不变式①）。
pub fn animate_value_as_state<T: AnimatableValue>(target: T, spec: impl Into<AnimSpec>) -> T {
    let spec = spec.into();
    let vec = target.to_vector();
    let (now, scope) = frame_time_and_scope();
    let scope = scope.expect("animation called outside composition scope");
    let st = remember(|| {
        mutableStateOf(AnimData::settled(
            scope,
            vec,
            spec.clone(),
            frame_time_nanos(),
        ))
    });
    // 推进与采样合并到一次 runtime 借用里（每帧每动画少一次 borrow + 状态槽查找）。
    st.peek_mut(|a| {
        a.scope = scope; // 采样者即归属作用域（句柄可能已换代，见 touch 注释）
        if a.target != vec || a.spec != spec {
            a.start(vec, spec, now);
        }
        a.advance(now);
        T::from_vector(&a.value)
    })
}

/// `animateFloatAsState(targetValue, animationSpec)`。
pub fn animate_float_as_state(target: f32, spec: impl Into<AnimSpec>) -> f32 {
    animate_value_as_state(target, spec)
}

/// `animateDpAsState(targetValue, animationSpec)`。
pub fn animate_dp_as_state(target: Dp, spec: impl Into<AnimSpec>) -> Dp {
    animate_value_as_state(target, spec)
}

/// `animateColorAsState(targetValue, animationSpec)`。
pub fn animate_color_as_state(target: Color, spec: impl Into<AnimSpec>) -> Color {
    animate_value_as_state(target, spec)
}

/// key 变化即**重新播放**的入场进度 0→1（首次组合也播一次）。
///
/// 与 `animate_float_as_state` 的分工：后者是「追新目标」（值变化才补间，首帧静止落位），
/// 前者是「重新起跑」——`SharedAxis` / `FadeScale` 这类切页入场动效每次进入都要从 0 开始。
/// 对齐 Compose `AnimatedVisibility` / `AnimatedContent`：入场动画播完即从注册表退集，
/// 该作用域不再每帧重组（旧实现在控件里挂一条永久 `add_frame_driver(schedule_scope(..))`，
/// 页面停留期间整页每帧重布局 + 重绘，且驱动随作用域销毁永不注销）。
pub fn animate_progress_on(key: u64, spec: impl Into<AnimSpec>) -> f32 {
    let spec = spec.into();
    let anim = remember_animatable(0.0f32);
    // keyed_remember 的初始化闭包只在 key 变化（含首次组合）那一帧执行一次
    let _ = keyed_remember(key, move || {
        anim.snap_to(0.0);
        anim.animate_to(1.0, spec);
    });
    anim.value()
}

/// `updateTransition(targetState)` — 一个目标状态驱动多个动画值。
///
/// 对齐 Compose：`transition.animateFloat { it.isExpanded }` 等每个派生值各自补间，
/// 但共享同一 `MonotonicFrameClock`，因此同一帧内各分量进度一致。
/// 目标状态由调用方每次组合传入；内部只做镜像（`peek_mut` 不触发失效，
/// 避免组合期写自己正在读的状态）。
#[derive(Clone, Copy)]
pub struct Transition<S: Copy + PartialEq + 'static> {
    target: MutableState<S>,
}

/// `updateTransition(targetState)`（须在组合作用域内调用）。
pub fn update_transition<S: Copy + PartialEq + 'static>(target: S) -> Transition<S> {
    let st = remember(|| mutableStateOf(target));
    st.peek_mut(|v| *v = target);
    Transition { target: st }
}

impl<S: Copy + PartialEq + 'static> Transition<S> {
    /// 当前目标状态（Compose `Transition.targetState`）。
    pub fn target(&self) -> S {
        self.target.read_unregistered()
    }
    /// `transition.animateFloat(transitionSpec = { … }) { it.foo }`。
    pub fn animate_float(&self, to: impl Fn(S) -> f32, spec: impl Into<AnimSpec>) -> f32 {
        animate_float_as_state(to(self.target()), spec)
    }
    /// `transition.animateDp { it.foo }`。
    pub fn animate_dp(&self, to: impl Fn(S) -> Dp, spec: impl Into<AnimSpec>) -> Dp {
        animate_dp_as_state(to(self.target()), spec)
    }
    /// `transition.animateColor { it.foo }`。
    pub fn animate_color(&self, to: impl Fn(S) -> Color, spec: impl Into<AnimSpec>) -> Color {
        animate_color_as_state(to(self.target()), spec)
    }
}

/// `rememberInfiniteTransition` 精简版：0..1..0 循环相位（呼吸/脉冲动效）。///
/// 无限动画按定义永不停驱，但只调度自身作用域，且宿主作用域销毁时自动退集。
pub fn remember_infinite_transition(period_secs: f32) -> f32 {
    animate_value_as_state(0.0f32, infinite_with(period_secs, LoopMode::Reverse))
}

/// 无限循环的线性相位 0→1（Compose `infiniteRepeatable(tween(period), RepeatMode.Restart)`）。
pub fn remember_infinite_progress(period_secs: f32) -> f32 {
    animate_value_as_state(0.0f32, infinite(period_secs))
}

// ══════════════════════════ 帧时钟辅助 ══════════════════════════

/// 当前单调帧时钟（纳秒）— Compose `withFrameNanos` 提供的时间戳。
pub fn clock_nanos() -> f64 {
    frame_time_nanos()
}

/// 循环相位：自 `start_nanos` 起按 `period_secs` 循环，波型见 [`LoopMode`]。
///
/// 纯函数（不登记注册表）——供「绘制阶段自取时间」的动效直接算相位，
/// 由调用方的帧驱动/动画负责续帧（对齐 Compose `InfiniteTransition` 在
/// draw phase 读 `animateFloatBy` 的用法）。
pub fn loop_phase(now_nanos: f64, start_nanos: f64, period_secs: f32, mode: LoopMode) -> f32 {
    let period = (period_secs.max(0.01) as f64) * 1e9;
    let phase = ((now_nanos - start_nanos).max(0.0) % period) as f32 / period as f32;
    match mode {
        LoopMode::Restart => phase,
        LoopMode::Reverse => {
            if phase < 0.5 {
                phase * 2.0
            } else {
                (1.0 - phase) * 2.0
            }
        }
    }
}

/// 便捷构造：`infinite(1.2)`（Restart 锯齿，旋转类）／`infinite_with(period, Reverse)`。
pub fn infinite(period_secs: f32) -> Infinite {
    Infinite {
        period_secs,
        mode: LoopMode::Restart,
    }
}

/// 便捷构造：`infinite_with(period_secs, mode)`。
pub fn infinite_with(period_secs: f32, mode: LoopMode) -> Infinite {
    Infinite { period_secs, mode }
}

/// 自 `start_nanos` 起的归一化进度（0→1），`secs` 为时长。
pub fn progress_since_nanos(start_nanos: f64, secs: f32) -> f32 {
    let dur = secs.max(1e-6) as f64 * 1e9;
    (((frame_time_nanos() - start_nanos) / dur).clamp(0.0, 1.0)) as f32
}

/// 本帧的帧间隔（秒）— Compose 帧回调给出的 `timeDelta`。
pub fn frame_delta_secs() -> f32 {
    crate::runtime::frame_delta()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 批量解析解（共享指数/三角系数）必须与逐轴 `simulate` 等价，
    /// 覆盖欠阻尼 / 临界 / 过阻尼三支。
    #[test]
    fn spring_vec_matches_scalar_simulation() {
        let x0 = [1.0f32, -2.0, 0.5, 0.2];
        let v0 = [3.0f32, 0.0, -1.0, 2.0];
        let target = [0.0f32, 0.5, 0.5, 1.0];
        for damping in [0.4f32, 1.0, 3.0] {
            let sp = Spring {
                stiffness: 4000.0,
                damping_ratio: damping,
                visibility_threshold: 0.01,
            };
            for t in [0.0f32, 0.016, 0.08, 0.5] {
                let (vals, vels) = sp.simulate_vec(x0, v0, target, t);
                for i in 0..4 {
                    let (v, d) = sp.simulate(x0[i], v0[i], target[i], t);
                    assert!(
                        (vals[i] - v).abs() < 1e-5,
                        "阻尼 {damping} t={t} 轴 {i} 值不一致：{} vs {v}",
                        vals[i]
                    );
                    assert!(
                        (vels[i] - d).abs() < 1e-4,
                        "阻尼 {damping} t={t} 轴 {i} 速度不一致：{} vs {d}",
                        vels[i]
                    );
                }
            }
        }
    }
}
