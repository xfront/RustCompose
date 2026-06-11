//! Demos 分组的页面实现。

use crate::common::*;
use crate::prelude::*;

pub(crate) fn page_spreadsheet() {
    let colors = theme_colors();
    section("Quarterly report · Q4 2026");
    Row(
        Modifier::new().fill_max_width().height(104.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            stat_card("REVENUE", "$128,400", "+12.4%", true);
            Spacer(Modifier::new().width(10.0));
            stat_card("ORDERS", "1,284", "+8.1%", true);
            Spacer(Modifier::new().width(10.0));
            stat_card("MARGIN", "42.6%", "-1.2%", false);
        },
    );
    Spacer(Modifier::new().height(12.0));
    Row(
        Modifier::new()
            .fill_max_width()
            .height(34.0)
            .background(colors.surface_container_low, 8.0)
            .border(1.0, colors.outline_variant, 8.0)
            .padding_hv(10.0, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                "fx",
                TextStyle::new()
                    .size(11.0)
                    .weight(750)
                    .italic(true)
                    .color(colors.primary),
                Modifier::new()
                    .padding_hv(7.0, 3.0)
                    .background(colors.primary_container, 6.0),
            );
            Spacer(Modifier::new().width(10.0));
            Text(
                "=SUM(D2:D7)  ·  毛利率 = (收入 − 成本) / 收入  ·  数据每 5 分钟刷新",
                TextStyle::new()
                    .size(11.5)
                    .color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(12.0));
    DataTable(
        &[
            "Item".to_string(),
            "Qty".to_string(),
            "Unit price".to_string(),
            "Total".to_string(),
        ],
        &[
            vec![
                "Mechanical keyboard".to_string(),
                "128".to_string(),
                "$89.00".to_string(),
                "$11,392".to_string(),
            ],
            vec![
                "Studio monitor".to_string(),
                "64".to_string(),
                "$249.00".to_string(),
                "$15,936".to_string(),
            ],
            vec![
                "USB-C dock".to_string(),
                "210".to_string(),
                "$119.00".to_string(),
                "$24,990".to_string(),
            ],
            vec![
                "Webcam Pro".to_string(),
                "96".to_string(),
                "$159.00".to_string(),
                "$15,264".to_string(),
            ],
            vec![
                "Standing desk".to_string(),
                "42".to_string(),
                "$399.00".to_string(),
                "$16,758".to_string(),
            ],
            vec![
                "Total".to_string(),
                "540".to_string(),
                "—".to_string(),
                "$84,340".to_string(),
            ],
        ],
        true,
        true,
        Modifier::new().border(1.0, colors.outline_variant, 10.0),
    );
}

pub(crate) fn stat_card(label: &'static str, value: &'static str, delta: &'static str, positive: bool) {
    let colors = theme_colors();
    let chip_fg = if positive {
        Color::rgb(0.13, 0.53, 0.30)
    } else {
        colors.error
    };
    let chip_bg = chip_fg.with_alpha(0.12);
    Column(
        Modifier::new()
            .weight(1.0, true)
            .padding(14.0)
            .background(colors.surface_container_low, 14.0)
            .border(1.0, colors.outline_variant, 14.0),
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            Text(
                label,
                TextStyle::new()
                    .size(9.5)
                    .weight(700)
                    .letter_spacing(1.2)
                    .color(colors.on_surface_variant),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(6.0));
            Text(
                value,
                TextStyle::new()
                    .size(24.0)
                    .weight(780)
                    .color(colors.on_surface),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(4.0));
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Text(
                        delta,
                        TextStyle::new().size(10.5).weight(700).color(chip_fg),
                        Modifier::new()
                            .padding_hv(6.0, 2.0)
                            .background(chip_bg, 8.0),
                    );
                    Spacer(Modifier::new().width(6.0));
                    Text(
                        "vs last quarter",
                        TextStyle::new()
                            .size(10.0)
                            .color(colors.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        },
    );
}

pub(crate) fn page_chat() {
    let input = remember(|| mutableStateOf(String::new()));
    let rich = remember(|| mutableStateOf(String::new()));
    let msgs = remember(|| {
        mutableStateOf(vec![
            ("Hey! The new story build looks sharp.".to_string(), false, "09:41".to_string()),
            ("Thanks — gradients and the new hero header just landed.".to_string(), true, "09:42".to_string()),
            ("Can you ship the hero transition demo too?".to_string(), false, "09:42".to_string()),
            ("Already in. Replay it from the Material Motion page.".to_string(), true, "09:43".to_string()),
        ])
    });
    let colors = theme_colors();
    section("Chat（list + Bubble + composer）");
    Column(
        Modifier::new()
            .fill_max_width()
            .padding(14.0)
            .background(colors.surface_container_low, 16.0)
            .border(1.0, colors.outline_variant, 16.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Avatar("AL", 38.0, Modifier::new());
                    Spacer(Modifier::new().width(10.0));
                    Column(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Start,
                        move || {
                            Text(
                                "Ada Lovelace",
                                TextStyle::new()
                                    .size(13.5)
                                    .weight(700)
                                    .color(colors.on_surface),
                                Modifier::new(),
                            );
                            Row(
                                Modifier::new(),
                                Arrangement::Start,
                                CrossAxisAlignment::Center,
                                move || {
                                    StatusDot(Color::rgb(0.18, 0.66, 0.38), Modifier::new());
                                    Spacer(Modifier::new().width(5.0));
                                    Text(
                                        "online · design systems",
                                        TextStyle::new()
                                            .size(10.5)
                                            .color(colors.on_surface_variant),
                                        Modifier::new(),
                                    );
                                },
                            );
                        },
                    );
                    Spacer(Modifier::new().weight(1.0, true));
                    IconButton(IconName::Search, Modifier::new(), || {});
                    IconButton(IconName::MoreVert, Modifier::new(), || {});
                },
            );
            Spacer(Modifier::new().height(12.0));
            Divider(Modifier::new());
            Spacer(Modifier::new().height(10.0));
            let current = msgs.read_clone();
            for (text, mine, at) in &current {
                Row(
                    Modifier::new().fill_max_width(),
                    if *mine {
                        Arrangement::End
                    } else {
                        Arrangement::Start
                    },
                    CrossAxisAlignment::Center,
                    {
                        let (text, mine, at) = (text.clone(), *mine, at.clone());
                        move || {
                            if mine {
                                Text(
                                    at.clone(),
                                    TextStyle::new()
                                        .size(9.5)
                                        .color(colors.on_surface_variant),
                                    Modifier::new().padding_hv(0.0, 6.0),
                                );
                                Spacer(Modifier::new().width(6.0));
                            }
                            Bubble(&text, mine, Modifier::new());
                            if !mine {
                                Spacer(Modifier::new().width(6.0));
                                Text(
                                    at.clone(),
                                    TextStyle::new()
                                        .size(9.5)
                                        .color(colors.on_surface_variant),
                                    Modifier::new().padding_hv(0.0, 6.0),
                                );
                            }
                        }
                    },
                );
                Spacer(Modifier::new().height(6.0));
            }
            Spacer(Modifier::new().height(6.0));
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    TextField(
                        input.clone(),
                        "Write a message…",
                        Modifier::new().weight(1.0, true),
                    );
                    Spacer(Modifier::new().width(8.0));
                    let st = msgs;
                    let inp = input;
                    IconButton(IconName::Send, Modifier::new(), move || {
                        let text = inp.read_clone();
                        if text.trim().is_empty() {
                            return;
                        }
                        st.update(|v| v.push((text, true, "now".to_string())));
                        inp.set(String::new());
                    });
                },
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    section("RichTextEdit（多行 · 选区 · 复制/粘贴/剪切）");
    RichTextEdit(
        rich,
        "Multi-line text…  Enter 换行，↑↓ 跨行，Home/End 行首末，Shift+方向键 选区，Ctrl+A/C/V/X 全选/复制/粘贴/剪切",
        Modifier::new().fill_max_width().height(140.0),
    );
}

// ── Animation 演示页（pages_animation 1–4 节）──

/// 立体感小球：投影 + 球体 + 高光。
pub(crate) fn anim_ball(b: &mut DrawBuilder, x: f32, y: f32, r: f32, col: Color) {
    b.fill_circle(x + 1.5, y + 2.0, r, Color::rgba(0.0, 0.0, 0.0, 0.15));
    b.fill_circle(x, y, r, col);
    b.fill_circle(
        x - r * 0.25,
        y - r * 0.3,
        r * 0.35,
        Color::rgba(1.0, 1.0, 1.0, 0.35),
    );
}

pub(crate) fn lerp3(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> Color {
    Color::rgba(
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
        1.0,
    )
}

pub(crate) fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

pub(crate) fn ease_in_out_back(t: f32) -> f32 {
    let c2 = 1.70158 * 1.525;
    if t < 0.5 {
        ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
    } else {
        ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
    }
}

/// 弹簧仿真解析解（目标 1、初值 0、初速 0）：x'' = k(1-x) - c·x'。
pub(crate) fn spring_value(t: f32, k: f32, c: f32) -> f32 {
    let disc = c * c - 4.0 * k;
    if disc.abs() < 1e-3 {
        // 临界阻尼
        let a = c / 2.0;
        1.0 - (-a * t).exp() * (1.0 + a * t)
    } else if disc > 0.0 {
        // 过阻尼
        let s = disc.sqrt();
        let r1 = (-c + s) / 2.0;
        let r2 = (-c - s) / 2.0;
        1.0 - (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / (r2 - r1)
    } else {
        // 欠阻尼
        let s = (-disc).sqrt();
        1.0 - (-c * t / 2.0).exp() * ((s * t / 2.0).cos() + (c / s) * (s * t / 2.0).sin())
    }
}

/// LCG 伪随机（0..1）。
pub(crate) fn next_rand(seed: &MutableState<u64>) -> f32 {
    let s = seed.get().wrapping_add(1);
    seed.set(s);
    let x = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (x >> 33) as f32 / (u32::MAX as f32)
}

/// 简易动画控制器（`SharedAnimation` 的最小子集）。
///
/// progress 0→1，支持循环 / 暂停 / 恢复 / 重置；时间源用 `Instant`，
/// 暂停期间扣除累计时长，恢复后从原进度继续。
#[derive(Clone, Copy)]
pub(crate) struct AnimCtrl {
    /// 起始时刻（`None` = 未开始 / 已重置）
    pub(crate) start: MutableState<Option<std::time::Instant>>,
    /// 暂停累计时长（秒）
    pub(crate) paused_total: MutableState<f32>,
    /// 暂停起始时刻（`Some` = 处于暂停）
    pub(crate) paused_at: MutableState<Option<std::time::Instant>>,
    /// 初速度（fling 用）
    pub(crate) velocity: MutableState<f32>,
    /// 单轮时长（秒）
    pub(crate) duration: f32,
    /// 是否循环播放
    pub(crate) looping: bool,
}

impl AnimCtrl {
    fn new(duration: f32, looping: bool) -> Self {
        Self {
            start: mutableStateOf(None),
            paused_total: mutableStateOf(0.0),
            paused_at: mutableStateOf(None),
            velocity: mutableStateOf(0.0),
            duration,
            looping,
        }
    }

    /// 从头开始播放。
    fn start(&self) {
        self.start.set(Some(std::time::Instant::now()));
        self.paused_total.set(0.0);
        self.paused_at.set(None);
    }

    /// 以给定初速度重新发射（fling）。
    fn launch(&self, velocity: f32) {
        self.velocity.set(velocity);
        self.start();
    }

    fn pause(&self) {
        if self.paused_at.get().is_none() && self.start.get().is_some() {
            self.paused_at.set(Some(std::time::Instant::now()));
        }
    }

    fn resume(&self) {
        if let Some(p) = self.paused_at.get() {
            self.paused_total
                .set(self.paused_total.get() + p.elapsed().as_secs_f32());
            self.paused_at.set(None);
        }
    }

    /// 回到起点并停止。
    fn reset(&self) {
        self.start.set(None);
        self.paused_total.set(0.0);
        self.paused_at.set(None);
    }

    fn is_paused(&self) -> bool {
        self.paused_at.get().is_some()
    }

    fn velocity(&self) -> f32 {
        self.velocity.get()
    }

    /// 已播放时长（秒，已扣除暂停）。
    fn elapsed(&self) -> f32 {
        let Some(t0) = self.start.get() else {
            return 0.0;
        };
        let mut e = t0.elapsed().as_secs_f32() - self.paused_total.get();
        if let Some(p) = self.paused_at.get() {
            e -= p.elapsed().as_secs_f32();
        }
        e.max(0.0)
    }

    /// 归一化进度 0..1（循环时取小数部分）。
    fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        let t = self.elapsed() / self.duration;
        if self.looping { t.fract() } else { t.min(1.0) }
    }

    /// 是否仍在推进（决定是否继续请求帧）。
    fn active(&self) -> bool {
        if self.start.get().is_none() || self.is_paused() {
            return false;
        }
        self.looping || self.elapsed() < self.duration
    }
}

/// 26 条缓动曲线（`Easing` 枚举 25 条 + `Spring`）。
const EASINGS: [(&str, u8); 26] = [
    ("Linear", 0),
    ("EaseInQuad", 1),
    ("EaseOutQuad", 2),
    ("EaseInOutQuad", 3),
    ("EaseInCubic", 4),
    ("EaseOutCubic", 5),
    ("EaseInOutCubic", 6),
    ("EaseInQuart", 7),
    ("EaseOutQuart", 8),
    ("EaseInOutQuart", 9),
    ("EaseInQuint", 10),
    ("EaseOutQuint", 11),
    ("EaseInOutQuint", 12),
    ("EaseInSine", 13),
    ("EaseOutSine", 14),
    ("EaseInOutSine", 15),
    ("EaseInExpo", 16),
    ("EaseOutExpo", 17),
    ("EaseInOutExpo", 18),
    ("EaseInCirc", 19),
    ("EaseOutCirc", 20),
    ("EaseInOutCirc", 21),
    ("EaseInBack", 22),
    ("EaseOutBack", 23),
    ("EaseInOutBack", 24),
    ("Spring", 25),
];

/// 按 `apply_easing` 实现的 26 条缓动曲线。
pub(crate) fn easing(kind: u8, t: f32) -> f32 {
    match kind {
        0 => t,
        1 => t * t,
        2 => t * (2.0 - t),
        3 => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        4 => t * t * t,
        5 => {
            let t = t - 1.0;
            t * t * t + 1.0
        }
        6 => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let t = t - 1.0;
                4.0 * t * t * t + 1.0
            }
        }
        7 => t * t * t * t,
        8 => {
            let t = t - 1.0;
            1.0 - t * t * t * t
        }
        9 => {
            if t < 0.5 {
                8.0 * t * t * t * t
            } else {
                let t = t - 1.0;
                1.0 - 8.0 * t * t * t * t
            }
        }
        10 => t * t * t * t * t,
        11 => {
            let t = t - 1.0;
            1.0 + t * t * t * t * t
        }
        12 => {
            if t < 0.5 {
                16.0 * t * t * t * t * t
            } else {
                let t = t - 1.0;
                1.0 + 16.0 * t * t * t * t * t
            }
        }
        13 => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        14 => (t * std::f32::consts::FRAC_PI_2).sin(),
        15 => 0.5 * (1.0 - (std::f32::consts::PI * t).cos()),
        16 => {
            if t == 0.0 {
                0.0
            } else {
                2.0f32.powf(10.0 * (t - 1.0))
            }
        }
        17 => {
            if (t - 1.0).abs() < f32::EPSILON {
                1.0
            } else {
                1.0 - 2.0f32.powf(-10.0 * t)
            }
        }
        18 => {
            if t == 0.0 {
                return 0.0;
            }
            if (t - 1.0).abs() < f32::EPSILON {
                return 1.0;
            }
            if t < 0.5 {
                0.5 * 2.0f32.powf(20.0 * t - 10.0)
            } else {
                1.0 - 0.5 * 2.0f32.powf(-20.0 * t + 10.0)
            }
        }
        19 => 1.0 - (1.0 - t * t).sqrt(),
        20 => (1.0 - (t - 1.0) * (t - 1.0)).sqrt(),
        21 => {
            if t < 0.5 {
                0.5 * (1.0 - (1.0 - 4.0 * t * t).sqrt())
            } else {
                0.5 * ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0)
            }
        }
        22 => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        }
        23 => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            let t = t - 1.0;
            1.0 + c3 * t * t * t + c1 * t * t
        }
        24 => {
            let c1 = 1.70158;
            let c2 = c1 * 1.525;
            if t < 0.5 {
                0.5 * ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2))
            } else {
                let t = t - 1.0;
                0.5 * ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t + c2) + 2.0)
            }
        }
        _ => {
            // Spring：过冲后回弹
            let c4 = std::f32::consts::PI / 3.0;
            if t == 0.0 {
                return 0.0;
            }
            if (t - 1.0).abs() < f32::EPSILON {
                return 1.0;
            }
            2.0f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
        }
    }
}

/// `Interval::map`：全局进度 → 区间内局部缓动进度。
pub(crate) fn interval_map(t: f32, begin: f32, end: f32, kind: u8) -> f32 {
    if t <= begin {
        return 0.0;
    }
    if t >= end {
        return 1.0;
    }
    let local = ((t - begin) / (end - begin)).clamp(0.0, 1.0);
    easing(kind, local)
}

/// `TweenSequence::value_at`：多阶段等权路径插值。
pub(crate) fn sequence_value(t: f32, segs: &[(f32, f32, u8)]) -> f32 {
    let n = segs.len();
    if n == 0 {
        return 0.0;
    }
    let mut acc = 0.0;
    for &(from, to, kind) in segs.iter() {
        let end = acc + 1.0 / n as f32;
        if t <= end {
            let local = ((t - acc) / (end - acc)).clamp(0.0, 1.0);
            return from + (to - from) * easing(kind, local);
        }
        acc = end;
    }
    segs[n - 1].1
}

/// `TweenSequence<Color>::value_at`：多阶段等权颜色插值。
pub(crate) fn sequence_color(t: f32, segs: &[(Color, Color)]) -> Color {
    let n = segs.len();
    if n == 0 {
        return Color::rgba(0.0, 0.0, 0.0, 1.0);
    }
    let mut acc = 0.0;
    for &(from, to) in segs.iter() {
        let end = acc + 1.0 / n as f32;
        if t <= end {
            let local = ((t - acc) / (end - acc)).clamp(0.0, 1.0);
            return Color::rgba(
                from.r + (to.r - from.r) * local,
                from.g + (to.g - from.g) * local,
                from.b + (to.b - from.b) * local,
                from.a + (to.a - from.a) * local,
            );
        }
        acc = end;
    }
    segs[n - 1].1
}

/// 构造 `anim_card` 的按钮项。
pub(crate) fn btn(label: &'static str, f: impl Fn() + 'static) -> (&'static str, std::rc::Rc<dyn Fn()>) {
    (label, std::rc::Rc::new(f) as std::rc::Rc<dyn Fn()>)
}

/// 注册「动画控制器 → 每帧调度作用域重组」的帧驱动。
///
/// 只在首次组合时注册（`remember` 保证），控制器活跃期间持续请求帧，
/// 动画结束 / 暂停后自动停止，帧循环得以休眠。
pub(crate) fn register_anim_driver(ctrl: AnimCtrl) {
    let sid = current_scope_id();
    remember(|| {
        if let Some(sid) = sid {
            add_frame_driver(move || {
                if ctrl.active() {
                    request_anim_frames();
                    schedule_scope(sid);
                }
            });
        }
        true
    });
}

/// 演示卡片：画布在上，编号说明与按钮在下（`make_section`）。
pub(crate) fn anim_card(
    caption: &str,
    buttons: Vec<(&'static str, std::rc::Rc<dyn Fn()>)>,
    demo: impl Fn() + 'static,
) {
    let colors = theme_colors();
    let cap = caption.to_string();
    Card(
        Modifier::new()
            .padding(12.0)
            .border(1.0, colors.outline_variant, 12.0),
        move || {
            demo();
            Spacer(Modifier::new().height(10.0));
            Text(
                cap.clone(),
                TextStyle::new().size(12.5).color(colors.on_surface),
                Modifier::new(),
            );
            if !buttons.is_empty() {
                Spacer(Modifier::new().height(8.0));
                let btns = buttons.clone();
                Row(
                    Modifier::new(),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        for (i, (label, on_click)) in btns.iter().enumerate() {
                            if i > 0 {
                                Spacer(Modifier::new().width(8.0));
                            }
                            let l = label.to_string();
                            let f = on_click.clone();
                            Button(
                                Modifier::new(),
                                move || f(),
                                move || {
                                    Text(
                                        l.clone(),
                                        TextStyle::new().size(12.5).color(colors.on_primary),
                                        Modifier::new(),
                                    );
                                },
                            );
                        }
                    },
                );
            }
        },
    );
    Spacer(Modifier::new().height(12.0));
}

pub(crate) fn page_animation() {
    section("Animation");
    Spacer(Modifier::new().height(8.0));

    // ── 1 · Tween — 一个控制器同时驱动位置 + 颜色 + 尺寸（自动循环）──
    // 绘制闭包只读取「组合阶段捕获的值」，绝不在绘制时访问 MutableState（避免 with_rt 重入）。
    {
        let run = remember(|| mutableStateOf(true));
        let t0 = remember(|| mutableStateOf(Some(std::time::Instant::now())));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() && t0.get().is_some() {
                        request_anim_frames();
                        schedule_scope(sid);
                    }
                });
            }
            true
        });
        let (run_b, t0_b) = (run, t0);
        let demo = move || {
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            let t = ease_in_out_cubic((e % 2.5) / 2.5);
            Box(
                Modifier::new().fill_max_width().height(90.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        let bx = x + 20.0 + t * (w - 40.0);
                        let col = lerp3((0.25, 0.55, 1.0), (1.0, 0.35, 0.2), t);
                        let r = 10.0 + 14.0 * t;
                        for i in (1..=3).rev() {
                            let gt = (t - i as f32 * 0.04).max(0.0);
                            let gx = x + 20.0 + gt * (w - 40.0);
                            let gr = r * (1.0 - i as f32 * 0.15);
                            b.fill_circle(
                                gx,
                                y + h / 2.0,
                                gr,
                                col.with_alpha(0.12 - i as f32 * 0.03),
                            );
                        }
                        anim_ball(b, bx, y + h / 2.0, r, col);
                        b.fill_rect(
                            x + 10.0,
                            y + h - 6.0,
                            w - 20.0,
                            3.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        b.fill_rect(x + 10.0, y + h - 6.0, (w - 20.0) * t, 3.0, col);
                    });
                },
            );
        };
        anim_card(
            "1 · Tween — one controller drives position + color + size",
            vec![btn("Start Tween", move || {
                run_b.set(true);
                t0_b.set(Some(std::time::Instant::now()));
            })],
            demo,
        );
    }

    // ── 2 · SpringSimulation — underdamped / critical / overdamped ──
    {
        let run = remember(|| mutableStateOf(false));
        let t0 = remember(|| mutableStateOf(None::<std::time::Instant>));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() {
                        request_anim_frames();
                        if let Some(t0v) = t0.get() {
                            if t0v.elapsed().as_secs_f32() < 6.0 {
                                schedule_scope(sid);
                            } else {
                                run.set(false);
                            }
                        }
                    }
                });
            }
            true
        });
        let (run_b, t0_b) = (run, t0);
        let demo = move || {
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            Box(
                Modifier::new().fill_max_width().height(100.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, _h| {
                        let x0 = x + 24.0;
                        let x1 = x + w - 90.0;
                        let springs: [(&str, f32, (f32, f32, f32), f32); 3] = [
                            ("Under", 22.0, (0.2, 0.75, 0.35), 6.0),
                            ("Critical", 48.0, (0.25, 0.55, 1.0), 22.0),
                            ("Over", 74.0, (0.95, 0.55, 0.15), 55.0),
                        ];
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        for (label, ry, rgb, damping) in springs {
                            b.fill_rect(
                                x0,
                                y + ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.05),
                            );
                            let val = spring_value(e, 120.0, damping);
                            let bx = x0 + val * (x1 - x0);
                            anim_ball(b, bx, y + ry, 7.0, lerp3(rgb, rgb, 0.0));
                            b.draw_text(x1 + 8.0, y + ry + 4.0, label, &gray, 60.0);
                        }
                    });
                },
            );
        };
        anim_card(
            "2 · SpringSimulation — underdamped / critical / overdamped",
            vec![btn("Release Spring", move || {
                run_b.set(true);
                t0_b.set(Some(std::time::Instant::now()));
            })],
            demo,
        );
    }

    // ── 3 · CurvedAnimation — 同一控制器，四种曲线（自动循环）──
    {
        let run = remember(|| mutableStateOf(true));
        let t0 = remember(|| mutableStateOf(Some(std::time::Instant::now())));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() && t0.get().is_some() {
                        request_anim_frames();
                        schedule_scope(sid);
                    }
                });
            }
            true
        });
        let (run_b, t0_b) = (run, t0);
        let demo = move || {
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            let t = (e % 2.0) / 2.0;
            Box(
                Modifier::new().fill_max_width().height(110.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        let row_h = (h - 10.0) / 4.0;
                        let x0 = x + 16.0;
                        let x1 = x + w - 100.0;
                        let curves: [(&str, (f32, f32, f32), u8); 4] = [
                            ("Linear", (0.6, 0.6, 0.6), 0),
                            ("EaseIn", (0.9, 0.3, 0.3), 1),
                            ("EaseOut", (0.3, 0.6, 0.9), 2),
                            ("EaseInOutBack", (0.85, 0.4, 0.9), 3),
                        ];
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        for (i, (label, rgb, kind)) in curves.into_iter().enumerate() {
                            let ry = y + 12.0 + row_h * i as f32 + row_h / 2.0;
                            let prog = match kind {
                                1 => t * t * t,
                                2 => 1.0 - (1.0 - t).powi(3),
                                3 => ease_in_out_back(t),
                                _ => t,
                            };
                            let bx = x0 + prog * (x1 - x0);
                            b.fill_rect(
                                x0,
                                ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.04),
                            );
                            anim_ball(b, bx, ry, 7.0, lerp3(rgb, rgb, 0.0));
                            b.draw_text(x1 + 8.0, ry + 4.0, label, &gray, 90.0);
                        }
                    });
                },
            );
        };
        anim_card(
            "3 · CurvedAnimation — same controller, four different curves",
            vec![btn("Start Curves", move || {
                run_b.set(true);
                t0_b.set(Some(std::time::Instant::now()));
            })],
            demo,
        );
    }

    // ── 4 · AnimatedValue — 隐式动画（随机新目标，0.5s 缓动过去）──
    {
        type BallState = (f32, f32, f32, (f32, f32, f32));
        let cur: MutableState<BallState> =
            remember(|| mutableStateOf((0.2f32, 30.0f32, 12.0f32, (0.2f32, 0.6f32, 1.0f32))));
        let tgt: MutableState<BallState> =
            remember(|| mutableStateOf((0.2f32, 30.0f32, 12.0f32, (0.2f32, 0.6f32, 1.0f32))));
        let t0 = remember(|| mutableStateOf(None::<std::time::Instant>));
        let run = remember(|| mutableStateOf(false));
        let seed = remember(|| mutableStateOf(1u64));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() {
                        request_anim_frames();
                        if let Some(t0v) = t0.get() {
                            if t0v.elapsed().as_secs_f32() < 0.7 {
                                schedule_scope(sid);
                            } else {
                                run.set(false);
                            }
                        }
                    }
                });
            }
            true
        });
        let (cur_b, tgt_b, t0_b, run_b, seed_b) = (cur, tgt, t0, run, seed);
        let demo = move || {
            let cur_v = cur_b.get();
            let tgt_v = tgt_b.get();
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.6);
            let te = ease_in_out_cubic((e / 0.5).min(1.0));
            Box(
                Modifier::new().fill_max_width().height(80.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        let (fx, cy, cs, cc) = cur_v;
                        let (tfx, ty, ts, tc) = tgt_v;
                        let bx = x + 20.0 + (fx + (tfx - fx) * te) * (w - 40.0);
                        let by = y + (cy + (ty - cy) * te).min(h - 20.0);
                        let r = cs + (ts - cs) * te;
                        let col = lerp3(cc, tc, te);
                        b.fill_rect(bx, y + 4.0, 1.0, h - 8.0, Color::rgba(0.0, 0.0, 0.0, 0.08));
                        b.fill_rect(
                            x + 10.0,
                            by,
                            w - 20.0,
                            1.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.08),
                        );
                        anim_ball(b, bx, by, r, col);
                    });
                },
            );
        };
        anim_card(
            "4 · AnimatedValue — implicit animation (click to randomize)",
            vec![btn("Randomize Target", move || {
                let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.6);
                let te = ease_in_out_cubic((e / 0.5).min(1.0));
                let (fx, cy, cs, cc) = cur_b.get();
                let (tfx, ty, ts, tc) = tgt_b.get();
                cur_b.set((
                    fx + (tfx - fx) * te,
                    cy + (ty - cy) * te,
                    cs + (ts - cs) * te,
                    (
                        cc.0 + (tc.0 - cc.0) * te,
                        cc.1 + (tc.1 - cc.1) * te,
                        cc.2 + (tc.2 - cc.2) * te,
                    ),
                ));
                tgt_b.set((
                    0.05 + next_rand(&seed_b) * 0.9,
                    15.0 + next_rand(&seed_b) * 35.0,
                    10.0 + next_rand(&seed_b) * 14.0,
                    (
                        0.15 + next_rand(&seed_b) * 0.85,
                        0.15 + next_rand(&seed_b) * 0.85,
                        0.15 + next_rand(&seed_b) * 0.85,
                    ),
                ));
                t0_b.set(Some(std::time::Instant::now()));
                run_b.set(true);
            })],
            demo,
        );
    }

    // ── 5 · Animation::fling — inertial motion ──
    {
        let ctrl = remember(|| AnimCtrl::new(3.0, false));
        register_anim_driver(ctrl);
        let demo = move || {
            let elapsed = ctrl.elapsed();
            let vel = ctrl.velocity();
            Box(
                Modifier::new().fill_max_width().height(60.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        // 测量探测阶段 w/h 为 0，需早退（`if w < 10.0 { return; }`）
                        if w < 10.0 {
                            return;
                        }
                        let track_y = y + h / 2.0;
                        b.fill_rrect(
                            x + 15.0,
                            track_y - 2.0,
                            w - 30.0,
                            4.0,
                            2.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        // 摩擦衰减：位移 = v·(1−e^(−friction·t)) / friction
                        let friction = 1.8f32;
                        let decay = (-friction * elapsed).exp();
                        let displacement = vel * (1.0 - decay) / friction;
                        let lo = x + 15.0;
                        let hi = (x + w - 15.0).max(lo);
                        let pos = (x + 20.0 + displacement * w * 0.3).clamp(lo, hi);
                        anim_ball(b, pos, track_y, 10.0, Color::rgba(0.55, 0.3, 0.9, 1.0));
                    });
                },
            );
        };
        anim_card(
            "5 · Animation::fling — inertial motion",
            vec![
                btn("Slow Fling", {
                    let c = ctrl;
                    move || c.launch(0.5)
                }),
                btn("Medium Fling", {
                    let c = ctrl;
                    move || c.launch(1.5)
                }),
                btn("Fast Fling", {
                    let c = ctrl;
                    move || c.launch(3.0)
                }),
            ],
            demo,
        );
    }

    // ── 6 · TweenSequence — multi-stage path (right→down←left↑up) ──
    {
        let ctrl = remember(|| AnimCtrl::new(3.0, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let t = ctrl.progress();
            let segs_x: [(f32, f32, u8); 4] = [
                (40.0, 260.0, 5),
                (260.0, 260.0, 0),
                (260.0, 40.0, 5),
                (40.0, 40.0, 0),
            ];
            let segs_y: [(f32, f32, u8); 4] = [
                (20.0, 20.0, 0),
                (20.0, 55.0, 5),
                (55.0, 55.0, 0),
                (55.0, 20.0, 5),
            ];
            let cols: [(Color, Color); 4] = [
                (
                    Color::rgba(0.25, 0.55, 1.0, 1.0),
                    Color::rgba(0.2, 0.8, 0.4, 1.0),
                ),
                (
                    Color::rgba(0.2, 0.8, 0.4, 1.0),
                    Color::rgba(1.0, 0.6, 0.1, 1.0),
                ),
                (
                    Color::rgba(1.0, 0.6, 0.1, 1.0),
                    Color::rgba(0.7, 0.3, 0.9, 1.0),
                ),
                (
                    Color::rgba(0.7, 0.3, 0.9, 1.0),
                    Color::rgba(0.25, 0.55, 1.0, 1.0),
                ),
            ];
            let bx = sequence_value(t, &segs_x);
            let by = sequence_value(t, &segs_y);
            let col = sequence_color(t, &cols);
            let seg = ((t * 4.0) as i32).clamp(0, 3) as usize;
            let labels = ["Move Right", "Move Down", "Move Left", "Move Up"];
            Box(
                Modifier::new().fill_max_width().height(80.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        b.stroke_rrect(
                            x + 39.5,
                            y + 19.5,
                            221.0,
                            36.0,
                            0.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                            1.0,
                        );
                        b.fill_circle(x + bx, y + by, 14.0, col.with_alpha(0.15));
                        anim_ball(b, x + bx, y + by, 9.0, col);
                        b.draw_text(x + w - 100.0, y + h / 2.0 + 4.0, labels[seg], &gray, 90.0);
                        b.fill_rrect(
                            x + 10.0,
                            y + h - 6.0,
                            w - 20.0,
                            3.0,
                            1.5,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        b.fill_rrect(x + 10.0, y + h - 6.0, (w - 20.0) * t, 3.0, 1.5, col);
                    });
                },
            );
        };
        anim_card(
            "6 · TweenSequence — multi-stage path (right→down←left↑up)",
            vec![btn("Start Sequence", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 7 · Interval + ReverseAnimation — staggered properties ──
    {
        let ctrl = remember(|| AnimCtrl::new(2.5, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let raw = ctrl.progress();
            let opacity = interval_map(raw, 0.0, 0.3, 5);
            let scale = 0.5 + 0.5 * interval_map(raw, 0.2, 0.7, 24);
            let slide = interval_map(raw, 0.5, 1.0, 6);
            // ReverseAnimation：进度反转 1.0 → 0.0
            let rev = 1.0 - raw;
            Box(
                Modifier::new().fill_max_width().height(80.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(9.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let gray2 = TextStyle::new()
                            .size(11.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let x0_bar = x + 30.0;
                        let x1_bar = x + w - 80.0;
                        let span = x1_bar - x0_bar;
                        let intervals: [(f32, f32, Color, &str); 3] = [
                            (0.0, 0.3, Color::rgba(0.2, 0.7, 0.4, 1.0), "Opacity"),
                            (0.2, 0.7, Color::rgba(0.9, 0.4, 0.2, 1.0), "Scale"),
                            (0.5, 1.0, Color::rgba(0.3, 0.5, 0.95, 1.0), "Slide"),
                        ];
                        for (i, (begin, end, c_col, label)) in intervals.iter().enumerate() {
                            let bar_y = y + 8.0 + 14.0 * i as f32;
                            b.fill_rrect(
                                x0_bar,
                                bar_y,
                                span,
                                6.0,
                                3.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.04),
                            );
                            b.fill_rrect(
                                x0_bar + begin * span,
                                bar_y,
                                (end - begin) * span,
                                6.0,
                                3.0,
                                c_col.with_alpha(0.3),
                            );
                            b.fill_circle(x0_bar + raw * span, bar_y + 3.0, 4.0, *c_col);
                            b.draw_text(x1_bar + 6.0, bar_y + 5.0, label, &gray, 60.0);
                        }
                        let ball_y = y + h - 18.0;
                        let slide_x = x + 30.0 + 200.0 * slide;
                        b.fill_circle(
                            slide_x,
                            ball_y,
                            12.0 * scale,
                            Color::rgba(0.25, 0.55, 1.0, opacity * 0.2),
                        );
                        b.fill_circle(
                            slide_x,
                            ball_y,
                            8.0 * scale,
                            Color::rgba(0.25, 0.55, 1.0, opacity),
                        );
                        b.draw_text(
                            x + w - 60.0,
                            ball_y + 4.0,
                            &format!("{:.2}", rev),
                            &gray2,
                            60.0,
                        );
                    });
                },
            );
        };
        anim_card(
            "7 · Interval + ReverseAnimation — staggered properties from one controller",
            vec![btn("Start Staggered", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 8 · Spring Presets — Default / Stiff / Bouncy / Smooth / Gentle ──
    {
        // (名称, 刚度 k, 阻尼 颜色) —— 质量均为 1（`SpringPreset`）
        const PRESETS: [(&str, f32, f32, (f32, f32, f32)); 5] = [
            ("Default", 100.0, 10.0, (0.25, 0.55, 1.0)),
            ("Stiff", 180.0, 26.0, (0.9, 0.35, 0.2)),
            ("Bouncy", 120.0, 8.0, (0.85, 0.4, 0.9)),
            ("Smooth", 150.0, 24.5, (0.2, 0.75, 0.4)),
            ("Gentle", 80.0, 14.0, (1.0, 0.65, 0.1)),
        ];
        let ctrl = remember(|| AnimCtrl::new(4.0, false));
        register_anim_driver(ctrl);
        let demo = move || {
            let elapsed = ctrl.elapsed();
            let running = elapsed < 4.0;
            Box(
                Modifier::new().fill_max_width().height(110.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let x0 = x + 24.0;
                        let x1 = x + w - 90.0;
                        let row_h = (h - 10.0) / PRESETS.len() as f32;
                        for (i, (name, k, damp, rgb)) in PRESETS.iter().enumerate() {
                            let ry = y + 12.0 + row_h * i as f32 + row_h / 2.0;
                            let val = if running {
                                spring_value(elapsed, *k, *damp)
                            } else {
                                0.0
                            };
                            let bx = x0 + val * (x1 - x0);
                            b.fill_rect(
                                x0,
                                ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.04),
                            );
                            anim_ball(b, bx, ry, 6.0, Color::rgba(rgb.0, rgb.1, rgb.2, 1.0));
                            b.draw_text(x1 + 8.0, ry + 4.0, name, &gray, 60.0);
                        }
                    });
                },
            );
        };
        anim_card(
            "8 · Spring Presets — Default / Stiff / Bouncy / Smooth / Gentle",
            vec![btn("Release Springs", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 9 · Easing Showcase — all 25 easing curves ──
    {
        let ctrl = remember(|| AnimCtrl::new(3.0, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let raw = ctrl.progress();
            Box(
                Modifier::new().fill_max_width().height(12.0 * 26.0 + 20.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, _h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(8.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let x0 = x + 16.0;
                        let x1 = x + w - 130.0;
                        for (i, (label, kind)) in EASINGS.iter().enumerate() {
                            let ry = y + 10.0 + 12.0 * i as f32 + 6.0;
                            // 轨道
                            b.fill_rect(
                                x0,
                                ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.03),
                            );
                            b.draw_text(x1 + 6.0, ry + 3.0, label, &gray, 120.0);
                            // 曲线圆点
                            let prog = easing(*kind, raw);
                            b.fill_circle(
                                x0 + prog * (x1 - x0),
                                ry,
                                3.5,
                                Color::rgba(0.3, 0.6, 0.9, 1.0),
                            );
                        }
                    });
                },
            );
        };
        anim_card(
            "9 · Easing Showcase — all 25 easing curves",
            vec![btn("Start Easing", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 10 · Animation Control — pause / resume / reset ──
    {
        let ctrl = remember(|| AnimCtrl::new(2.0, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let t = easing(6, ctrl.progress());
            let running = ctrl.active();
            Box(
                Modifier::new().fill_max_width().height(70.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let bx = x + 20.0 + t * (w - 40.0);
                        let by = y + h / 2.0;
                        b.fill_rrect(
                            x + 20.0,
                            by - 2.0,
                            w - 40.0,
                            4.0,
                            2.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        anim_ball(b, bx, by, 12.0, Color::rgba(0.2, 0.7, 0.4, 1.0));
                        b.draw_text(
                            x + w - 70.0,
                            by + 4.0,
                            if running { "Running" } else { "Stopped" },
                            &gray,
                            60.0,
                        );
                    });
                },
            );
        };
        anim_card(
            "10 · Animation Control — pause / resume / reset",
            vec![
                btn("Pause", {
                    let c = ctrl;
                    move || c.pause()
                }),
                btn("Resume", {
                    let c = ctrl;
                    move || c.resume()
                }),
                btn("Reset", {
                    let c = ctrl;
                    move || c.reset()
                }),
            ],
            demo,
        );
    }
}

/// 转场面板的容器色 / 前景色（四种内容轮换）。
pub(crate) fn motion_tone(colors: ColorScheme, i: usize) -> (Color, Color) {
    match i % 4 {
        0 => (colors.primary_container, colors.on_primary_container),
        1 => (colors.secondary_container, colors.on_secondary_container),
        2 => (colors.tertiary_container, colors.on_tertiary_container),
        _ => (colors.surface_variant, colors.on_surface_variant),
    }
}

/// SharedAxis / FadeScale 的演示内容：一整块「列表项」卡片。
pub(crate) fn motion_panel(
    bg: Color,
    fg: Color,
    glyph: IconName,
    title: &'static str,
    sub: &'static str,
    idx: usize,
) {
    Box(
        Modifier::new()
            .fill_max_width()
            .height(92.0)
            .background(bg, 14.0)
            .clip(14.0),
        Alignment::Center,
        move || {
            Row(
                Modifier::new().fill_max_width().padding(16.0),
                Arrangement::SpaceBetween,
                CrossAxisAlignment::Center,
                move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            Box(
                                Modifier::new()
                                    .size(38.0, 38.0)
                                    .background(fg.mix(bg, 0.82), 19.0)
                                    .clip(19.0),
                                Alignment::Center,
                                move || {
                                    Canvas(Modifier::new().size(21.0, 21.0).no_hit(), move |b, x, y, w, h| {
                                        draw_icon(b, glyph, x, y, w.min(h), fg);
                                    });
                                },
                            );
                            Spacer(Modifier::new().width(12.0));
                            Column(
                                Modifier::new(),
                                Arrangement::Start,
                                CrossAxisAlignment::Start,
                                move || {
                                    Text(
                                        format!("{} · {:02}", title, idx + 1),
                                        TextStyle::new().size(14.5).weight(700).color(fg),
                                        Modifier::new(),
                                    );
                                    Spacer(Modifier::new().height(3.0));
                                    Text(
                                        sub,
                                        TextStyle::new()
                                            .size(12.0)
                                            .color(fg.mix(bg, 0.26)),
                                        Modifier::new(),
                                    );
                                },
                            );
                        },
                    );
                    Canvas(Modifier::new().size(18.0, 18.0).no_hit(), move |b, x, y, w, h| {
                        draw_icon(b, IconName::ChevronRight, x, y, w.min(h), fg);
                    });
                },
            );
        },
    );
}

pub(crate) fn page_material_motion() {
    let colors = theme_colors();
    section("Material Motion（SharedAxis / FadeScale）");
    Text(
        "M3 三条转场曲线：X 轴滑动（层级导航）、Y 轴滑动（分组切换）、淡入缩放（同级内容切换）"
            .to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    // ── SharedAxis · X ──
    let hkey = remember(|| mutableStateOf(0u64));
    let hfwd = remember(|| mutableStateOf(true));
    {
        let (k, f) = (hkey, hfwd);
        let prev = btn("Previous", move || {
            f.set(false);
            k.update(|n| *n += 1);
        });
        let (k, f) = (hkey, hfwd);
        let next = btn("Next", move || {
            f.set(true);
            k.update(|n| *n += 1);
        });
        anim_card(
            "SharedAxis · X — 列表进入详情时页面左右滑动，返回时反向",
            vec![prev, next],
            move || {
                let k = hkey.get();
                let fwd = hfwd.get();
                SharedAxis(k, true, fwd, Modifier::new().fill_max_width(), move || {
                    let (bg, fg) = motion_tone(colors, k as usize);
                    let (glyph, title, sub) = match k as usize % 4 {
                        0 => (IconName::Mail, "Inbox", "12 封未读邮件"),
                        1 => (IconName::File, "Drafts", "3 份草稿待发送"),
                        2 => (IconName::Star, "Starred", "你收藏的会话"),
                        _ => (IconName::Settings, "Settings", "工作区偏好设置"),
                    };
                    motion_panel(bg, fg, glyph, title, sub, k as usize % 4);
                });
            },
        );
    }

    // ── SharedAxis · Y ──
    let vkey = remember(|| mutableStateOf(0u64));
    let vfwd = remember(|| mutableStateOf(true));
    {
        let (k, f) = (vkey, vfwd);
        let prev = btn("Previous", move || {
            f.set(false);
            k.update(|n| *n += 1);
        });
        let (k, f) = (vkey, vfwd);
        let next = btn("Next", move || {
            f.set(true);
            k.update(|n| *n += 1);
        });
        anim_card(
            "SharedAxis · Y — 同一层级内切换分组（Inbox ↔ Drafts ↔ Starred）",
            vec![prev, next],
            move || {
                let k = vkey.get();
                let fwd = vfwd.get();
                SharedAxis(k, false, fwd, Modifier::new().fill_max_width(), move || {
                    let (bg, fg) = motion_tone(colors, (k as usize + 2) % 4);
                    motion_panel(bg, fg, IconName::Grid, "Group", "切换分组时的纵向位移", k as usize);
                });
            },
        );
    }

    // ── FadeScale ──
    let fkey = remember(|| mutableStateOf(0u64));
    {
        let k = fkey;
        let replay = btn("Replay", move || k.update(|n| *n += 1));
        anim_card(
            "FadeScale — 同级内容切换：透明度 + 缩放同时插值",
            vec![replay],
            move || {
                let k = fkey.get();
                FadeScale(k, Modifier::new().fill_max_width(), move || {
                    let (bg, fg) = motion_tone(colors, (k as usize + 1) % 4);
                    motion_panel(bg, fg, IconName::Chart, "Insight", "每周活跃度 · 同比 +18%", k as usize);
                });
            },
        );
    }
}

pub(crate) fn page_pull_refresh() {
    let colors = theme_colors();
    let refreshing = remember(|| mutableStateOf(false));
    let stamp = remember(|| mutableStateOf(0usize));
    let tick = remember(|| mutableStateOf(0u32));
    // 模拟 ~0.5s 网络延迟后完成刷新。驱动只在组合时注册一次（回调里注册会
    // 随刷新次数累积泄漏）；结束写 false 仅在 true→false 时刻发生一次。
    remember(|| {
        let (tick, refreshing) = (tick, refreshing);
        add_frame_driver(move || {
            if !refreshing.get() {
                return;
            }
            request_anim_frames();
            let n = tick.get() + 1;
            tick.set(n);
            if n >= 30 {
                refreshing.set(false);
            }
        });
        true
    });
    section("Pull To Refresh（下拉刷新）");
    Text(
        "在列表内按住向下拖动：进度弧随拉动增长，越过阈值松手即刷新".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));
    PullToRefresh(
        refreshing,
        Modifier::new()
            .fill_max_width()
            .height(240.0)
            .border(1.0, colors.outline_variant, 12.0),
        {
            let stamp = stamp;
            let tick = tick;
            move || {
                stamp.update(|n| *n += 1);
                tick.set(0);
            }
        },
        move || {
            let stamp = stamp;
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    for i in 0..12 {
                        Card(Modifier::new().padding(12.0), move || {
                            Row(
                                Modifier::new().fill_max_width(),
                                Arrangement::SpaceBetween,
                                CrossAxisAlignment::Center,
                                move || {
                                    Text(
                                        format!("Item {}", i + 1),
                                        TextStyle::new(),
                                        Modifier::new(),
                                    );
                                    Text(
                                        format!("刷新 #{}", stamp.get()),
                                        TextStyle::new()
                                            .size(12.0)
                                            .color(colors.on_surface_variant),
                                        Modifier::new(),
                                    );
                                },
                            );
                        });
                        Spacer(Modifier::new().height(8.0));
                    }
                },
            );
        },
    );
}

pub(crate) fn mini_stat(label: &'static str, value: &'static str, colors: ColorScheme) {
    Column(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                value,
                TextStyle::new().size(17.0).weight(700).color(colors.primary),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(2.0));
            Text(
                label,
                TextStyle::new().size(11.0).color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

pub(crate) fn page_hero() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    // 单一进度 t 驱动头图高度 / 头像尺寸 / 详情透明度，展开收拢天然同步。
    let t = animate_float_as_state(
        if open.get() { 1.0 } else { 0.0 },
        Tween::new(0.42).with_easing(ease_out_cubic),
    );
    section("Hero Animation（共享元素展开）");
    Text(
        "点击卡片：头图、头像与详情由同一个进度 t 插值，展开收拢保持同步".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    Clickable(
        Modifier::new().fill_max_width(),
        move || open.set(!open.get()),
        move || {
            Card(Modifier::new().fill_max_width(), move || {
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(64.0 + t * 66.0)
                        .background_gradient(colors.primary, colors.tertiary, 14.0)
                        .clip(14.0),
                    Alignment::Center,
                    move || {
                        Canvas(Modifier::new().fill_max_size().no_hit(), move |b, x, y, w, h| {
                            b.fill_circle(
                                x + w * 0.84,
                                y + h * 0.30,
                                44.0,
                                Color::rgba(1.0, 1.0, 1.0, 0.10),
                            );
                            b.fill_circle(
                                x + w * 0.66,
                                y + h * 0.82,
                                26.0,
                                Color::rgba(1.0, 1.0, 1.0, 0.08),
                            );
                            draw_icon(
                                b,
                                IconName::Star,
                                x + w - 86.0,
                                y + h * 0.5 - 18.0,
                                36.0,
                                Color::rgba(1.0, 1.0, 1.0, 0.30),
                            );
                        });
                    },
                );
                Spacer(Modifier::new().height(12.0));
                Row(
                    Modifier::new().fill_max_width(),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        Avatar("AL", 42.0 + t * 10.0, Modifier::new());
                        Spacer(Modifier::new().width(12.0));
                        Column(
                            Modifier::new(),
                            Arrangement::Start,
                            CrossAxisAlignment::Start,
                            move || {
                                Text(
                                    "Ada Lovelace",
                                    TextStyle::new()
                                        .size(16.0)
                                        .weight(700)
                                        .color(colors.on_surface),
                                    Modifier::new(),
                                );
                                Spacer(Modifier::new().height(2.0));
                                Text(
                                    "Staff Engineer · Design Systems",
                                    TextStyle::new()
                                        .size(12.0)
                                        .color(colors.on_surface_variant),
                                    Modifier::new(),
                                );
                            },
                        );
                    },
                );
                // 高度与透明度都随 t 增长：t=0 时整块被裁掉，不占位。
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(t * 84.0)
                        .clip(0.0),
                    Alignment::TopStart,
                    move || {
                        Column(
                            Modifier::new().fill_max_width(),
                            Arrangement::Start,
                            CrossAxisAlignment::Stretch,
                            move || {
                                Spacer(Modifier::new().height(12.0));
                                Divider(Modifier::new());
                                Spacer(Modifier::new().height(10.0));
                                Row(
                                    Modifier::new().fill_max_width(),
                                    Arrangement::SpaceEvenly,
                                    CrossAxisAlignment::Center,
                                    move || {
                                        mini_stat("Projects", "24", colors);
                                        mini_stat("Followers", "1.2k", colors);
                                        mini_stat("Stars", "318", colors);
                                    },
                                );
                            },
                        );
                    },
                );
            });
        },
    );
    Spacer(Modifier::new().height(10.0));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Button(
                Modifier::new(),
                move || open.set(!open.get()),
                move || {
                    Text(
                        if open.get() { "Collapse" } else { "Expand" },
                        TextStyle::new().color(colors.on_primary),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().width(10.0));
            Text(
                format!("progress t = {:.2}", t),
                TextStyle::new().size(12.0).color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

pub(crate) fn fade_panel(
    bg1: Color,
    bg2: Color,
    fg: Color,
    glyph: IconName,
    title: &'static str,
    sub: &'static str,
) {
    Box(
        Modifier::new()
            .fill_max_width()
            .height(148.0)
            .background_gradient(bg1, bg2, 18.0)
            .clip(18.0),
        Alignment::TopStart,
        move || {
            Row(
                Modifier::new().fill_max_size().padding(22.0),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Box(
                        Modifier::new()
                            .size(54.0, 54.0)
                            .background(Color::rgba(1.0, 1.0, 1.0, 0.18), 27.0)
                            .clip(27.0),
                        Alignment::Center,
                        move || {
                            Canvas(Modifier::new().size(28.0, 28.0).no_hit(), move |b, x, y, w, h| {
                                draw_icon(b, glyph, x, y, w.min(h), fg);
                            });
                        },
                    );
                    Spacer(Modifier::new().width(16.0));
                    Column(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Start,
                        move || {
                            Text(
                                title,
                                TextStyle::new().size(20.0).weight(750).color(fg),
                                Modifier::new(),
                            );
                            Spacer(Modifier::new().height(5.0));
                            Text(
                                sub,
                                TextStyle::new().size(12.5).color(fg.mix(bg2, 0.22)),
                                Modifier::new(),
                            );
                        },
                    );
                },
            );
        },
    );
}

pub(crate) fn page_cross_fade() {
    let colors = theme_colors();
    let sel = remember(|| mutableStateOf(0usize));
    section("Cross Fade（内容渐隐渐显）");
    Text(
        "切换选项：旧内容淡出、新内容淡入，同一容器内交叉过渡".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    Crossfade(
        sel.get() as u64,
        Modifier::new().fill_max_width(),
        move || {
            let i = sel.get() % 3;
            let (bg1, bg2, fg, glyph, title, sub) = match i {
                0 => (
                    colors.primary_container,
                    colors.secondary_container,
                    colors.on_primary_container,
                    IconName::Play,
                    "Morning focus",
                    "深度工作 · 25 分钟专注 + 5 分钟休息",
                ),
                1 => (
                    colors.secondary_container,
                    colors.tertiary_container,
                    colors.on_secondary_container,
                    IconName::Star,
                    "Evening review",
                    "回顾今天完成的 6 个任务，整理明日清单",
                ),
                _ => (
                    colors.tertiary_container,
                    colors.primary_container,
                    colors.on_tertiary_container,
                    IconName::Chart,
                    "Weekly retro",
                    "团队 12 人 · 交付 38 个变更 · 0 次回滚",
                ),
            };
            fade_panel(bg1, bg2, fg, glyph, title, sub);
        },
    );
    Spacer(Modifier::new().height(14.0));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for (i, label) in ["Focus", "Review", "Retro"].iter().enumerate() {
                if i > 0 {
                    Spacer(Modifier::new().width(8.0));
                }
                let s = sel;
                let selected = sel.get() == i;
                Chip(label.to_string(), None, selected, Modifier::new(), move || s.set(i));
            }
        },
    );
}

pub(crate) fn page_custom_widget() {
    let colors = theme_colors();
    let v = remember(|| mutableStateOf(0.62f32));
    let shown = animate_float_as_state(v.get(), Spring::bouncy());
    section("Custom Widget（Canvas 自绘仪表盘）");
    Text(
        "组合基础图元（圆弧 / 刻度 / 指针）自绘控件；滑块驱动 Spring 弹性跟随".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    let gauge = if shown < 0.5 {
        colors.primary
    } else if shown < 0.8 {
        colors.tertiary
    } else {
        colors.error
    };
    let label = format!("{:.0}%", shown * 100.0);
    let value_style = TextStyle::new().size(30.0).weight(800).color(gauge);
    let tick_style = TextStyle::new().size(10.0).color(colors.on_surface_variant);
    Card(Modifier::new().fill_max_width(), move || {
        let label = label.clone();
        let value_style = value_style.clone();
        let tick_style = tick_style.clone();
        Canvas(
            Modifier::new().fill_max_width().height(260.0).no_hit(),
            move |b, x, y, w, h| {
                let cx = x + w * 0.5;
                let cy = y + h * 0.60;
                let r = (w * 0.36).min(h * 0.52);
                let a0 = std::f32::consts::PI * 0.78;
                let span = std::f32::consts::PI * 1.44;

                let mut track = Vec::with_capacity(49);
                for i in 0..=48 {
                    let a = a0 + span * i as f32 / 48.0;
                    track.push((cx + r * a.cos(), cy + r * a.sin()));
                }
                b.stroke_path(&track, colors.surface_variant, 13.0);

                let steps = ((48.0 * shown).round() as usize).max(1);
                let mut arc = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = a0 + span * shown * i as f32 / steps as f32;
                    arc.push((cx + r * a.cos(), cy + r * a.sin()));
                }
                b.stroke_path(&arc, gauge, 13.0);
                b.fill_circle(cx + r * a0.cos(), cy + r * a0.sin(), 6.5, gauge);

                for i in 0..=10 {
                    let a = a0 + span * i as f32 / 10.0;
                    let (s, c) = (a.sin(), a.cos());
                    b.line(
                        cx + (r + 11.0) * c,
                        cy + (r + 11.0) * s,
                        cx + (r + 18.0) * c,
                        cy + (r + 18.0) * s,
                        colors.outline,
                        2.0,
                    );
                }

                let a = a0 + span * shown;
                b.line(
                    cx,
                    cy,
                    cx + (r - 26.0) * a.cos(),
                    cy + (r - 26.0) * a.sin(),
                    colors.on_surface,
                    3.0,
                );
                b.fill_circle(cx, cy, 7.0, colors.on_surface);
                b.fill_circle(cx, cy, 3.0, colors.surface_container_low);

                // 读数放在轴心下方的开口区，避免与指针重叠。
                let label_w = label.chars().count() as f32 * 8.2;
                b.draw_text(cx - label_w, cy + 16.0, &label, &value_style, 160.0);
                b.draw_text(cx - r + 24.0, cy + 14.0, "0", &tick_style, 40.0);
                b.draw_text(cx + r - 34.0, cy + 14.0, "100", &tick_style, 40.0);
            },
        );
        Spacer(Modifier::new().height(6.0));
        Slider(v, Modifier::new().fill_max_width());
        Spacer(Modifier::new().height(10.0));
        Row(
            Modifier::new(),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            move || {
                for (i, pct) in [0.25f32, 0.5, 0.75, 1.0].iter().enumerate() {
                    if i > 0 {
                        Spacer(Modifier::new().width(8.0));
                    }
                    let p = *pct;
                    OutlinedButton(
                        Modifier::new(),
                        move || v.set(p),
                        move || {
                            Text(
                                format!("{:.0}%", p * 100.0),
                                TextStyle::new().size(12.0).color(colors.primary),
                                Modifier::new(),
                            );
                        },
                    );
                }
            },
        );
    });
}

pub(crate) fn status_pill(text: &'static str, bg: Color, fg: Color) {
    Box(
        Modifier::new().background(bg, 999.0).padding_ltrb(10.0, 4.0, 10.0, 4.0),
        Alignment::Center,
        move || {
            Text(
                text,
                TextStyle::new().size(11.0).weight(600).color(fg),
                Modifier::new(),
            );
        },
    );
}

/// 异步任务的步骤行：圆点（待办 / 进行中 / 完成）+ 名称 + 状态。
pub(crate) fn async_step(label: &'static str, state: u8, colors: ColorScheme) {
    let (dot, text) = match state {
        0 => (colors.outline_variant, colors.on_surface_variant),
        1 => (colors.primary, colors.on_surface),
        _ => (colors.primary, colors.on_surface),
    };
    let status = match state {
        0 => "waiting",
        1 => "running",
        _ => "done",
    };
    Row(
        Modifier::new().fill_max_width().padding_ltrb(0.0, 5.0, 0.0, 5.0),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Canvas(Modifier::new().size(16.0, 16.0).no_hit(), move |b, x, y, w, h| {
                        let cx = x + w * 0.5;
                        let cy = y + h * 0.5;
                        if state == 2 {
                            b.fill_circle(cx, cy, 7.0, dot);
                            draw_icon(
                                b,
                                IconName::Check,
                                cx - 5.0,
                                cy - 5.0,
                                10.0,
                                colors.surface_container_low,
                            );
                        } else if state == 1 {
                            b.stroke_circle(cx, cy, 6.0, dot, 2.0);
                            b.fill_circle(cx, cy, 2.5, dot);
                        } else {
                            b.stroke_circle(cx, cy, 6.0, dot, 1.5);
                        }
                    });
                    Spacer(Modifier::new().width(10.0));
                    Text(
                        label,
                        TextStyle::new().size(12.5).color(text),
                        Modifier::new(),
                    );
                },
            );
            Text(
                status,
                TextStyle::new()
                    .size(11.0)
                    .color(if state == 0 {
                        colors.outline
                    } else {
                        colors.primary
                    }),
                Modifier::new(),
            );
        },
    );
}

pub(crate) fn page_async() {
    let colors = theme_colors();
    // 0 = idle / 1 = running / 2 = done
    let phase = remember(|| mutableStateOf(0u8));
    let prog = remember(|| mutableStateOf(0.0f32));
    // 组合期注册一次：只有运行中才请求续帧（结束即停驱，保持空闲休眠）。
    remember(|| {
        let (phase, prog) = (phase, prog);
        add_frame_driver(move || {
            if phase.get() != 1 {
                return;
            }
            request_anim_frames();
            let p = (prog.get() + frame_delta() * 0.85).min(1.0);
            prog.set(p);
            if p >= 1.0 {
                phase.set(2);
            }
        });
        true
    });
    let p = prog.get();
    let running = phase.get() == 1;
    let done = phase.get() == 2;
    section("Async（异步任务模拟）");
    Text(
        "帧驱动推进的模拟任务：进度条 · 步骤清单 · 状态徽标随状态机同步".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    Card(Modifier::new().fill_max_width(), move || {
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            CrossAxisAlignment::Center,
            move || {
                Column(
                    Modifier::new(),
                    Arrangement::Start,
                    CrossAxisAlignment::Start,
                    move || {
                        Text(
                            "Workspace sync",
                            TextStyle::new()
                                .size(15.0)
                                .weight(700)
                                .color(colors.on_surface),
                            Modifier::new(),
                        );
                        Spacer(Modifier::new().height(3.0));
                        Text(
                            if done {
                                "同步完成 · 1,284 条记录已更新"
                            } else if running {
                                "正在拉取远端变更…"
                            } else {
                                "等待开始 · 预计 1.2s"
                            },
                            TextStyle::new().size(12.0).color(colors.on_surface_variant),
                            Modifier::new(),
                        );
                    },
                );
                Row(
                    Modifier::new(),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        if running {
                            Spinner(18.0, Modifier::new());
                            Spacer(Modifier::new().width(8.0));
                        }
                        if done {
                            status_pill(
                                "DONE",
                                colors.secondary_container,
                                colors.on_secondary_container,
                            );
                        } else if running {
                            status_pill(
                                "RUNNING",
                                colors.primary_container,
                                colors.on_primary_container,
                            );
                        } else {
                            status_pill(
                                "IDLE",
                                colors.surface_variant,
                                colors.on_surface_variant,
                            );
                        }
                    },
                );
            },
        );
        Spacer(Modifier::new().height(14.0));
        LinearProgress(Some(p), Modifier::new().fill_max_width());
        Spacer(Modifier::new().height(6.0));
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            CrossAxisAlignment::Center,
            move || {
                Text(
                    if done { "全部完成" } else { "同步进度" },
                    TextStyle::new().size(11.0).color(colors.on_surface_variant),
                    Modifier::new(),
                );
                Text(
                    format!("{:.0}%", p * 100.0),
                    TextStyle::new().size(11.0).weight(600).color(colors.primary),
                    Modifier::new(),
                );
            },
        );
        Spacer(Modifier::new().height(10.0));
        Divider(Modifier::new());
        async_step("Connect to workspace", if done { 2 } else if running { 1 } else { 0 }, colors);
        async_step(
            "Fetch remote records",
            if p > 0.30 || done { 2 } else if running { 1 } else { 0 },
            colors,
        );
        async_step(
            "Merge local changes",
            if p > 0.65 || done { 2 } else if running { 1 } else { 0 },
            colors,
        );
        async_step("Write index", if done { 2 } else if p > 0.92 { 1 } else { 0 }, colors);
        Spacer(Modifier::new().height(12.0));
        Row(
            Modifier::new(),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            move || {
                let (ph, pg) = (phase, prog);
                Button(
                    Modifier::new(),
                    move || {
                        pg.set(0.0);
                        ph.set(1);
                    },
                    move || {
                        Text(
                            if done { "Run again" } else { "Run task" },
                            TextStyle::new().color(colors.on_primary),
                            Modifier::new(),
                        );
                    },
                );
                Spacer(Modifier::new().width(8.0));
                let (ph, pg) = (phase, prog);
                TextButton(
                    "Reset",
                    Modifier::new(),
                    move || {
                        pg.set(0.0);
                        ph.set(0);
                    },
                );
            },
        );
    });
}

pub(crate) fn metric_tile(label: &'static str, value: String, color: Color, colors: ColorScheme) {
    Column(
        Modifier::new().padding_ltrb(20.0, 8.0, 20.0, 8.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                value.clone(),
                TextStyle::new().size(30.0).weight(800).color(color),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(2.0));
            Text(
                label,
                TextStyle::new().size(11.0).color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

pub(crate) fn page_reactive() {
    let colors = theme_colors();
    let count = remember(|| mutableStateOf(3i32));
    let step = remember(|| mutableStateOf(1i32));
    section("Reactive & DSL（state → UI 自动刷新）");
    Text(
        "只修改状态，界面由组合自动推导；派生值在组合期直接计算，无手动刷新".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    let n = count.get();
    let s = step.get();
    card_section("派生状态", move || {
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceEvenly,
            CrossAxisAlignment::Center,
            move || {
                metric_tile("count", format!("{}", n), colors.primary, colors);
                metric_tile("count × 2", format!("{}", n * 2), colors.tertiary, colors);
                metric_tile(
                    "count²",
                    format!("{}", n * n),
                    colors.secondary,
                    colors,
                );
                metric_tile(
                    "parity",
                    if n % 2 == 0 { "even" } else { "odd" }.to_string(),
                    colors.on_surface_variant,
                    colors,
                );
            },
        );
        Spacer(Modifier::new().height(12.0));
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            move || {
                let c = count;
                OutlinedButton(
                    Modifier::new(),
                    move || c.update(|v| *v -= s),
                    move || {
                        Text(
                            format!("− {}", s),
                            TextStyle::new().color(colors.primary),
                            Modifier::new(),
                        );
                    },
                );
                Spacer(Modifier::new().width(10.0));
                for (i, opt) in [1i32, 5, 10].iter().enumerate() {
                    if i > 0 {
                        Spacer(Modifier::new().width(6.0));
                    }
                    let st = step;
                    let chosen = s == *opt;
                    let val = *opt;
                    Chip(
                        format!("step {}", val),
                        None,
                        chosen,
                        Modifier::new(),
                        move || st.set(val),
                    );
                }
                Spacer(Modifier::new().width(10.0));
                let c = count;
                Button(
                    Modifier::new(),
                    move || c.update(|v| *v += s),
                    move || {
                        Text(
                            format!("+ {}", s),
                            TextStyle::new().color(colors.on_primary),
                            Modifier::new(),
                        );
                    },
                );
                Spacer(Modifier::new().width(8.0));
                let c = count;
                TextButton("Reset", Modifier::new(), move || c.set(0));
            },
        );
    });

    card_section("声明式组合", move || {
        CodeBlock(
            "let count = remember(|| mutableStateOf(3));\nlet step  = remember(|| mutableStateOf(1));\n\nText(format!(\"count = {}\", count.get()), ..);\n\nButton(Modifier::new(), move || {\n    count.update(|v| *v += step.get());\n}, ..);",
            Modifier::new(),
        );
    });
}

// ════════════════════════════ Basics ════════════════════════════
