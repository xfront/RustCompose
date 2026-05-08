//! 反馈控件 — Dialog / Snackbar / Tooltip / Popover / Skeleton …

use std::cell::Cell;
use std::rc::Rc;

use compose::animation::{animate_progress_on, clock_nanos, request_anim_frame_after};
use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Column, CrossAxisAlignment, DrawBuilder, Modifier, Policy, Row, Spacer,
    add_node, measure_text_with_measurer, viewport_size, with_content_scope,
};
use compose::material::{IconName, glass_spec, icon, theme_colors};
use compose::runtime::{
    MutableState, add_frame_driver, disposable_effect, mutableStateOf, remember,
    remove_frame_driver, with_rt_pub,
};

use crate::tokens;

use crate::widgets::{
    GlassOrBg, Text, overlay::OverlaySlot, overlay::PutAction, overlay::PutIndexedAction,
    overlay::PutOverlayContent, overlay::RememberOverlayKey, overlay::RunAction,
    overlay::RunIndexedAction,
};

// ══════════════════════════════ Snackbar ══════════════════════════════

#[derive(Clone, Default, PartialEq)]
struct SnackData {
    message: String,
    action: String,
    seq: u64,
}

/// Snackbar 宿主状态（跨重组持久，Copy 句柄）。
#[derive(Clone, Copy)]
pub struct SnackbarHostState(MutableState<SnackData>);

impl SnackbarHostState {
    /// 显示一条 Snackbar（约 2.5s 后自动消失）。
    pub fn show(&self, message: &str) {
        self.0.update(|d| {
            d.message = message.to_string();
            d.action.clear();
            d.seq += 1;
        });
    }
    /// 显示带动作标签的 Snackbar。
    pub fn show_action(&self, message: &str, action: &str) {
        self.0.update(|d| {
            d.message = message.to_string();
            d.action = action.to_string();
            d.seq += 1;
        });
    }
    pub fn dismiss(&self) {
        self.0.update(|d| d.message.clear());
    }
    /// 当前是否有内容（不注册依赖）。
    pub fn is_visible(&self) -> bool {
        self.0.with_value(|d| !d.message.is_empty())
    }
}

/// `rememberSnackbarHostState()` — 组合内创建 Snackbar 宿主状态。
pub fn RememberSnackbarHostState() -> SnackbarHostState {
    SnackbarHostState(remember(|| mutableStateOf(SnackData::default())))
}

/// `SnackbarHost(state)` — 把 Snackbar 挂到浮层宿主（底部浮出，自动定时关闭）。
pub fn SnackbarHost(state: SnackbarHostState) {
    let data = state.0.read_clone();
    let seq = data.seq;
    let visible = !data.message.is_empty();
    // M3 `duration.short`：4s 后自动收回；被新消息顶替时旧倒计时不再动作。
    let hs = state;
    auto_hide_after(seq, tokens::SNACKBAR_DURATION, visible, move || {
        if hs.0.with_value(|d| d.seq) == seq {
            hs.dismiss();
        }
    });
    let key = RememberOverlayKey();
    let (vw, vh) = viewport_size();
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        let msg = data.message.clone();
        let act = data.action.clone();
        // 入场 = 从下方滑入自身高度 + 整体淡入（M3 snackbarFadeThrough）。
        let t = animate_progress_on(seq, tokens::enter_spec());
        let fade = move |c: Color| c.with_alpha(c.a * t);
        let m = GlassOrBg(
            Modifier::new()
                .offset(
                    tokens::SPACE_XXL,
                    vh - tokens::SNACKBAR_BOTTOM + (1.0 - t) * tokens::SNACKBAR_SLIDE,
                )
                .width((vw - 2.0 * tokens::SPACE_XXL).min(tokens::SNACKBAR_MAX_W)),
            g,
            fade(colors.inverse_surface),
            tokens::SHAPE_EXTRA_SMALL,
        )
        .padding_ltrb(
            tokens::SPACE_LG,
            tokens::SPACE_MD,
            tokens::SPACE_SM,
            tokens::SPACE_MD,
        );
        let id = add_node(Policy::Leaf, m);
        with_rt_pub(|rt| {
            rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, _h| {
                b.draw_text(
                    x,
                    y,
                    &msg,
                    &tokens::body_medium(fade(colors.inverse_on_surface)),
                    f32::INFINITY,
                );
                if !act.is_empty() {
                    let style = tokens::label_large(fade(colors.inverse_primary));
                    let aw = measure_text_with_measurer(&act, &style).0;
                    b.draw_text(
                        x + w - aw - tokens::SPACE_LG,
                        y,
                        &act,
                        &style,
                        f32::INFINITY,
                    );
                }
            }));
        });
        let s = state;
        with_rt_pub(|rt| {
            rt.nodes[id.index()].on_click = Some(Box::new(move || s.dismiss()));
        });
    });
    OverlaySlot(key, visible);
}

/// `Snackbar(message, actionLabel, onAction)` — 就地渲染的 Snackbar（不依赖宿主）。
pub fn Snackbar(
    message: &str,
    action_label: Option<&str>,
    on_action: Option<Box<dyn Fn()>>,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let msg = message.to_string();
    let act = action_label.map(|s| s.to_string()).unwrap_or_default();
    let key = RememberOverlayKey();
    if let Some(f) = on_action {
        PutAction(key, f);
    }
    let id = add_node(
        Policy::Leaf,
        modifier
            .fill_max_width()
            .min_height(tokens::CONTROL_HEIGHT + 8.0)
            .background(colors.inverse_surface, tokens::SHAPE_EXTRA_SMALL)
            .padding_ltrb(
                tokens::SPACE_LG,
                tokens::SPACE_MD,
                tokens::SPACE_SM,
                tokens::SPACE_MD,
            ),
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, _h| {
            b.draw_text(
                x,
                y,
                &msg,
                &tokens::body_medium(colors.inverse_on_surface),
                f32::INFINITY,
            );
            if !act.is_empty() {
                let style = tokens::label_large(colors.inverse_primary);
                let aw = measure_text_with_measurer(&act, &style).0;
                b.draw_text(
                    x + w - aw - tokens::SPACE_LG,
                    y,
                    &act,
                    &style,
                    f32::INFINITY,
                );
            }
        }));
        rt.nodes[id.index()].on_click = Some(Box::new(move || RunAction(key)));
    });
}

// ══════════════════════════════ Dialog / Sheet / Drawer ══════════════════════════════

thread_local! {
    /// 对话框 / 浮层主体内容表（key → 内容闭包）。
    static BODY_CB: std::cell::RefCell<std::collections::HashMap<u64, Box<dyn Fn()>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn put_body(key: u64, f: impl Fn() + 'static) {
    BODY_CB.with(|m| {
        m.borrow_mut().insert(key, Box::new(f));
    });
}

fn call_body(key: u64) {
    BODY_CB.with(|m| {
        if let Some(f) = m.borrow().get(&key) {
            f();
        }
    });
}

/// 浮层入场进度：M3 `fadeThrough`（medium2 + emphasized decelerate）+ scrim 淡入色。
///
/// `key` 换新（浮层重开 → 内容作用域重建、remember 复位）即从 0 重播，播完自动退出动画注册表。
/// 本库没有 `graphicsLayer`，缩放会连带内容重排，故 M3 的 scale 入场用位移 + 淡入替代。
pub(crate) fn overlay_enter(
    key: u64,
    colors: &compose::material::ColorScheme,
) -> (f32, Color) {
    let t = animate_progress_on(key, tokens::enter_spec());
    (t, colors.scrim.with_alpha(colors.scrim.a * t))
}

/// 墙钟倒计时到点执行一次（Snackbar 自动消失）。
///
/// 为什么不用 `delay_frames`：帧号在宿主空闲休眠时不推进 —— 倒计时会停摆，消息条挂在
/// 屏上永不消失；反过来逐帧轮询又违反「静止即停驱」。这里用 `request_anim_frame_after`
/// 定点唤醒，只在到点那一帧被叫醒一次。驱动只注册一次，`key`（消息序号）变化即重新计时。
pub(crate) fn auto_hide_after(key: u64, secs: f32, armed: bool, fire: impl Fn() + 'static) {
    struct Timer {
        key: u64,
        secs: f32,
        /// 到点时刻（单调纳秒）。
        deadline: f64,
        /// 本轮已触发（触发后不再请求唤醒，等 `key` 刷新）。
        done: bool,
        fire: std::rc::Rc<dyn Fn()>,
    }
    let fire = std::rc::Rc::new(fire) as std::rc::Rc<dyn Fn()>;
    let init = fire.clone();
    let slot = remember(move || {
        std::rc::Rc::new(std::cell::RefCell::new(Timer {
            key,
            secs,
            deadline: clock_nanos() + secs as f64 * 1e9,
            done: !armed,
            fire: init,
        }))
    });
    let slot_drv = slot.clone();
    let drv = remember(move || {
        let c = slot_drv.clone();
        add_frame_driver(move || {
            let (done, due) = {
                let h = c.borrow();
                (h.done, h.deadline - clock_nanos())
            };
            if done {
                return;
            }
            if due <= 0.0 {
                let f = c.borrow().fire.clone();
                c.borrow_mut().done = true;
                f(); // 帧驱动回调此刻不持 runtime 借用，可安全写状态
            } else {
                request_anim_frame_after(due as f32 / 1e9);
            }
        })
    });
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
    let mut h = slot.borrow_mut();
    if h.key != key || h.secs != secs {
        h.key = key;
        h.secs = secs;
        h.deadline = clock_nanos() + secs as f64 * 1e9;
        h.done = !armed;
        h.fire = fire;
        if armed {
            request_anim_frame_after(secs);
        }
    }
}

/// `Dialog(open, width, height) { content }` — 居中模态对话框（scrim 点击关闭）。
pub fn Dialog(open: MutableState<bool>, width: f32, height: f32, content: impl Fn() + 'static) {
    let key = RememberOverlayKey();
    let is_open = open.get();
    let (vw, vh) = viewport_size();
    put_body(key, content);
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        let (t, scrim_c) = overlay_enter(key, &colors);
        let scrim = add_node(
            Policy::Leaf,
            Modifier::new()
                .fill_max_size()
                .no_indication()
                .background(scrim_c, 0.0),
        );
        with_rt_pub(|rt| {
            rt.nodes[scrim.index()].on_click = Some(Box::new(move || open.set(false)));
        });
        let mut m = GlassOrBg(
            Modifier::new()
                .offset((vw - width) * 0.5, (vh - height) * 0.5 + (1.0 - t) * tokens::OVERLAY_RISE)
                .size(width, height),
            g,
            colors.Surface,
            tokens::SHAPE_EXTRA_LARGE,
        )
        .padding_ltrb(
            tokens::SPACE_XXL,
            tokens::SPACE_XXL,
            tokens::SPACE_XXL,
            tokens::SPACE_LG,
        )
        .clip(tokens::SHAPE_EXTRA_LARGE);
        if !g.enabled {
            m = m
                .border(1.0, colors.outline_variant, tokens::SHAPE_EXTRA_LARGE)
                .shadow(
                    tokens::ELEVATION_3,
                    tokens::SHAPE_EXTRA_LARGE,
                    tokens::shadow_color(colors.is_dark()),
                );
        }
        let id = add_node(
            Policy::Column {
                arrangement: Arrangement::Start,
                cross: CrossAxisAlignment::Start,
            },
            m,
        );
        with_content_scope(id, move || call_body(key));
    });
    OverlaySlot(key, is_open);
}

/// `AlertDialog(open, title, message, confirmLabel, dismissLabel, onConfirm)`。
pub fn AlertDialog(
    open: MutableState<bool>,
    title: &str,
    message: &str,
    confirm_label: &str,
    dismiss_label: &str,
    on_confirm: impl Fn() + 'static,
) {
    let key = RememberOverlayKey();
    PutAction(key, on_confirm);
    let t = title.to_string();
    let m = message.to_string();
    let cl = confirm_label.to_string();
    let dl = dismiss_label.to_string();
    Dialog(open, tokens::DIALOG_W, tokens::DIALOG_H, move || {
        let colors = theme_colors();
        let cl1 = cl.clone();
        let dl1 = dl.clone();
        Text(
            t.clone(),
            tokens::headline_small(colors.on_surface),
            Modifier::new(),
        );
        Spacer(Modifier::new().height(tokens::SPACE_SM));
        Text(
            m.clone(),
            tokens::body_medium(colors.on_surface_variant),
            Modifier::new().weight(1.0, true),
        );
        Spacer(Modifier::new().height(tokens::SPACE_LG));
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::End,
            CrossAxisAlignment::Center,
            move || {
                let dl2 = dl1.clone();
                let cl2 = cl1.clone();
                crate::widgets::basic::TextButton(dl2.clone(), Modifier::new(), move || {
                    open.set(false);
                });
                Spacer(Modifier::new().width(tokens::SPACE_SM));
                crate::widgets::basic::TextButton(cl2.clone(), Modifier::new(), move || {
                    RunAction(key);
                    open.set(false);
                });
            },
        );
    });
}

/// `BottomSheet(open, height) { content }` — M3 底部抽屉：顶部 28dp 圆角 + `surface_container_low`。
pub fn BottomSheet(open: MutableState<bool>, height: f32, content: impl Fn() + 'static) {
    let key = RememberOverlayKey();
    let is_open = open.get();
    let (vw, vh) = viewport_size();
    put_body(key, content);
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        let (t, scrim_c) = overlay_enter(key, &colors);
        let scrim = add_node(
            Policy::Leaf,
            Modifier::new()
                .fill_max_size()
                .no_indication()
                .background(scrim_c, 0.0),
        );
        with_rt_pub(|rt| {
            rt.nodes[scrim.index()].on_click = Some(Box::new(move || open.set(false)));
        });
        let mut m = GlassOrBg(
            // M3 `ModalBottomSheet` 自屏底滑入（t=0 时整块在视口之下）。
            Modifier::new().offset(0.0, vh - height * t).size(vw, height),
            g,
            colors.surface_container,
            tokens::SHAPE_EXTRA_LARGE,
        )
        .padding_ltrb(
            tokens::SPACE_LG,
            tokens::SPACE_MD,
            tokens::SPACE_LG,
            tokens::SPACE_MD,
        )
        .clip(tokens::SHAPE_EXTRA_LARGE);
        if !g.enabled {
            m = m.shadow(
                tokens::ELEVATION_3,
                tokens::SHAPE_EXTRA_LARGE,
                tokens::shadow_color(colors.is_dark()),
            );
        }
        let id = add_node(
            Policy::Column {
                arrangement: Arrangement::Start,
                cross: CrossAxisAlignment::Start,
            },
            m,
        );
        with_content_scope(id, move || call_body(key));
    });
    OverlaySlot(key, is_open);
}

/// `ModalDrawer(open, width) { content }` — M3 模态抽屉：360dp 宽、`surface_container_low`、无边框。
pub fn ModalDrawer(open: MutableState<bool>, width: f32, content: impl Fn() + 'static) {
    let key = RememberOverlayKey();
    let is_open = open.get();
    let (_vw, vh) = viewport_size();
    put_body(key, content);
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        let (t, scrim_c) = overlay_enter(key, &colors);
        let scrim = add_node(
            Policy::Leaf,
            Modifier::new()
                .fill_max_size()
                .no_indication()
                .background(scrim_c, 0.0),
        );
        with_rt_pub(|rt| {
            rt.nodes[scrim.index()].on_click = Some(Box::new(move || open.set(false)));
        });
        let m = GlassOrBg(
            // M3 模态抽屉自左侧滑入。
            Modifier::new().offset(-width * (1.0 - t), 0.0).size(width, vh),
            g,
            colors.surface_container,
            0.0,
        )
        .padding_ltrb(
            tokens::SPACE_XS,
            tokens::SPACE_SM,
            tokens::SPACE_XS,
            tokens::SPACE_SM,
        );
        let id = add_node(
            Policy::Column {
                arrangement: Arrangement::Start,
                cross: CrossAxisAlignment::Start,
            },
            m,
        );
        with_content_scope(id, move || call_body(key));
    });
    OverlaySlot(key, is_open);
}

// ══════════════════════════════ Tooltip / Popover / HoverCard ══════════════════════════════

/// `Tooltip(Text) { content }` — M3 提示气泡：`inverse_surface` 底 + 4dp 圆角 + `body_small`。
///
/// 时序对齐 Flutter `Tooltip`：悬停满 [`tokens::TOOLTIP_DELAY`] 才出现，离开后再残留
/// [`tokens::TOOLTIP_EXIT_DELAY`]（避免指针扫过就闪一下）。两端都用**定点唤醒**，
/// 常驻悬停不会把应用拖进 60fps。
pub fn Tooltip(tip: &str, content: impl Fn() + 'static) {
    let key = RememberOverlayKey();
    let shown = remember(|| mutableStateOf(false));
    // (是否悬停中, 最近一次进入/离开的时刻 ns)：hover 处理器写、帧驱动读。
    let edge = remember(|| Rc::new(Cell::new((false, 0.0f64))));
    let id = add_node(Policy::Box(Alignment::TopStart), Modifier::new());
    with_rt_pub(|rt| {
        let e = edge.clone();
        rt.nodes[id.index()].hover_handler = Some(Box::new(move |enter| {
            e.set((enter, clock_nanos()));
        }));
    });
    with_content_scope(id, content);
    let drv = remember(move || {
        let e = edge.clone();
        add_frame_driver(move || {
            let (hovering, at) = e.get();
            let waited = ((clock_nanos() - at).max(0.0) / 1e9) as f32;
            let on = shown.read_unregistered();
            if hovering {
                if !on {
                    if waited >= tokens::TOOLTIP_DELAY {
                        shown.set(true);
                    } else {
                        request_anim_frame_after(tokens::TOOLTIP_DELAY - waited);
                    }
                }
            } else if on && waited >= tokens::TOOLTIP_EXIT_DELAY {
                shown.set(false);
            } else if on {
                request_anim_frame_after(tokens::TOOLTIP_EXIT_DELAY - waited);
            }
        })
    });
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
    let on = shown.get();
    let tip = tip.to_string();
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        // 入场只做淡入（气泡无 scrim）：底色与文字色一起按进度加 alpha。
        let t = animate_progress_on(key, tokens::enter_spec());
        let bg = colors.inverse_surface.with_alpha(colors.inverse_surface.a * t);
        let fg = colors.inverse_on_surface.with_alpha(colors.inverse_on_surface.a * t);
        let m = GlassOrBg(
            Modifier::new().pin_below(id, tokens::SPACE_SM),
            g,
            bg,
            tokens::SHAPE_EXTRA_SMALL,
        )
        .padding_ltrb(
            tokens::SPACE_SM,
            tokens::SPACE_XS,
            tokens::SPACE_SM,
            tokens::SPACE_XS,
        );
        let tid = add_node(Policy::Leaf, m);
        let tip2 = tip.clone();
        with_rt_pub(|rt| {
            rt.nodes[tid.index()].draw =
                Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
                    b.draw_text(x, y, &tip2, &tokens::body_small(fg), f32::INFINITY);
                }));
        });
    });
    OverlaySlot(key, on);
}

/// `Popover(expanded, width, trigger) { content }` — 锚定浮层面板。
pub fn Popover(
    expanded: MutableState<bool>,
    width: f32,
    trigger: impl Fn() + 'static,
    content: impl Fn() + 'static,
) {
    let key = RememberOverlayKey();
    let id = add_node(Policy::Box(Alignment::TopStart), Modifier::new());
    with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(move || expanded.update(|v| *v = !*v)));
    });
    with_content_scope(id, trigger);
    let open = expanded.get();
    put_body(key, content);
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        // 入场：从锚点向下落位 + 淡入（M3 `fadeThrough`；无 graphicsLayer ⇒ 不缩放内容）。
        let t = animate_progress_on(key, tokens::enter_spec());
        let fade = move |c: Color| c.with_alpha(c.a * t);
        let mut m = GlassOrBg(
            Modifier::new()
                .pin_below(id, tokens::SPACE_SM)
                .offset(0.0, -(1.0 - t) * tokens::OVERLAY_RISE)
                .width(width),
            g,
            fade(colors.Surface),
            tokens::SHAPE_MEDIUM,
        )
        .padding_ltrb(
            tokens::SPACE_MD,
            tokens::SPACE_MD,
            tokens::SPACE_MD,
            tokens::SPACE_MD,
        )
        .clip(tokens::SHAPE_MEDIUM);
        if !g.enabled {
            m = m
                .border(1.0, fade(colors.outline_variant), tokens::SHAPE_MEDIUM)
                .shadow(
                    tokens::ELEVATION_2 * t,
                    tokens::SHAPE_MEDIUM,
                    tokens::shadow_color(colors.is_dark()),
                );
        }
        let pid = add_node(
            Policy::Column {
                arrangement: Arrangement::Start,
                cross: CrossAxisAlignment::Start,
            },
            m,
        );
        with_content_scope(pid, move || call_body(key));
    });
    OverlaySlot(key, open);
}

/// `HoverCard(width, trigger) { content }` — 悬停展示的富卡片。
pub fn HoverCard(width: f32, trigger: impl Fn() + 'static, content: impl Fn() + 'static) {
    let key = RememberOverlayKey();
    let hovered = remember(|| mutableStateOf(false));
    let id = add_node(Policy::Box(Alignment::TopStart), Modifier::new());
    with_rt_pub(|rt| {
        rt.nodes[id.index()].hover_handler = Some(Box::new(move |e| hovered.set(e)));
    });
    with_content_scope(id, trigger);
    let on = hovered.get();
    put_body(key, content);
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        let t = animate_progress_on(key, tokens::enter_spec());
        let fade = move |c: Color| c.with_alpha(c.a * t);
        let mut m = GlassOrBg(
            Modifier::new()
                .pin_below(id, tokens::SPACE_SM)
                .offset(0.0, -(1.0 - t) * tokens::OVERLAY_RISE)
                .width(width),
            g,
            fade(colors.Surface),
            tokens::SHAPE_MEDIUM,
        )
        .padding_ltrb(
            tokens::SPACE_LG,
            tokens::SPACE_LG,
            tokens::SPACE_LG,
            tokens::SPACE_LG,
        )
        .clip(tokens::SHAPE_MEDIUM);
        if !g.enabled {
            m = m
                .border(1.0, fade(colors.outline_variant), tokens::SHAPE_MEDIUM)
                .shadow(
                    tokens::ELEVATION_2 * t,
                    tokens::SHAPE_MEDIUM,
                    tokens::shadow_color(colors.is_dark()),
                );
        }
        let pid = add_node(
            Policy::Column {
                arrangement: Arrangement::Start,
                cross: CrossAxisAlignment::Start,
            },
            m,
        );
        with_content_scope(pid, move || call_body(key));
    });
    OverlaySlot(key, on);
}

// ══════════════════════════════ 提示条 / 骨架 / 空态 ══════════════════════════════

/// 提示语义。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlertKind {
    Info,
    Success,
    Warning,
    Error,
}

impl AlertKind {
    fn colors(self) -> (Color, Color) {
        let s = theme_colors();
        match self {
            AlertKind::Info => (s.primary_container, s.on_primary_container),
            AlertKind::Success => (s.secondary_container, s.on_secondary_container),
            AlertKind::Warning => (s.tertiary.with_alpha(0.30), s.on_surface),
            AlertKind::Error => (s.error_container, s.on_error_container),
        }
    }
    fn icon(self) -> IconName {
        match self {
            AlertKind::Info => IconName::Info,
            AlertKind::Success => IconName::Check,
            AlertKind::Warning => IconName::Warning,
            AlertKind::Error => IconName::Error,
        }
    }
}

/// `InlineAlert(kind, title, message)` — M3 内联提示条：容器色 + 8dp 圆角 + 24dp 图标。
pub fn InlineAlert(kind: AlertKind, title: &str, message: &str) {
    let (bg, fg) = kind.colors();
    let t = title.to_string();
    let m = message.to_string();
    Row(
        Modifier::new()
            .fill_max_width()
            .background(bg, tokens::SHAPE_SMALL)
            .padding_ltrb(
                tokens::SPACE_MD,
                tokens::SPACE_MD,
                tokens::SPACE_MD,
                tokens::SPACE_MD,
            ),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            icon(kind.icon(), tokens::ICON_DEFAULT, fg, Modifier::new());
            Spacer(Modifier::new().width(tokens::SPACE_MD));
            let t2 = t.clone();
            let m2 = m.clone();
            Column(
                Modifier::new().weight(1.0, false),
                Arrangement::Center,
                CrossAxisAlignment::Start,
                move || {
                    Text(t2.clone(), tokens::title_small(fg), Modifier::new());
                    Text(m2.clone(), tokens::body_medium(fg), Modifier::new());
                },
            );
        },
    );
}

/// `Banner(message, icon, onClose)` — M3 顶部横幅：`surface_container` + 24dp 图标 + 40dp 关闭目标。
pub fn Banner(message: &str, icon_name: IconName, on_close: Option<Box<dyn Fn()>>) {
    let colors = theme_colors();
    let msg = message.to_string();
    let key = RememberOverlayKey();
    if let Some(f) = on_close {
        PutAction(key, f);
    }
    Row(
        Modifier::new()
            .fill_max_width()
            .min_height(tokens::LIST_TWO_LINE)
            .background(colors.surface_container, 0.0)
            .padding_ltrb(
                tokens::SPACE_LG,
                tokens::SPACE_MD,
                tokens::SPACE_XS,
                tokens::SPACE_MD,
            ),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            let msg2 = msg.clone();
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    icon(
                        icon_name,
                        tokens::ICON_DEFAULT,
                        colors.on_surface_variant,
                        Modifier::new(),
                    );
                    Spacer(Modifier::new().width(tokens::SPACE_MD));
                    Text(
                        msg2.clone(),
                        tokens::body_medium(colors.on_surface),
                        Modifier::new(),
                    );
                },
            );
            let cid = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new().size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
            );
            with_rt_pub(|rt| {
                rt.nodes[cid.index()].on_click = Some(Box::new(move || RunAction(key)));
            });
            with_content_scope(cid, move || {
                icon(
                    IconName::Close,
                    tokens::ICON_DEFAULT,
                    colors.on_surface_variant,
                    Modifier::new(),
                );
            });
        },
    );
}

/// `Skeleton(width, height)` — 骨架占位（微光呼吸动画）。
pub fn Skeleton(width: f32, height: f32, modifier: Modifier) {
    let colors = theme_colors();
    let id = add_node(Policy::Leaf, modifier.size(width, height));
    // 呼吸相位在 draw 阶段推进：只重绘本节点，调用方作用域不参与每帧重组
    let phase =
        crate::widgets::animation::draw_phase(id, None, 1.6, compose::animation::LoopMode::Reverse);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            b.fill_rrect(
                x,
                y,
                w,
                h,
                tokens::SHAPE_SMALL,
                colors.on_surface.with_alpha(0.08 + 0.07 * phase.get()),
            );
        }));
    });
}

/// `ShimmerPlaceholder(width, height)` — 微光占位块（列表加载态）。
pub fn ShimmerPlaceholder(width: f32, height: f32, modifier: Modifier) {
    Skeleton(width, height, modifier);
}

/// `Spinner(size)` — 无限旋转指示。
pub fn Spinner(size: f32, modifier: Modifier) {
    let colors = theme_colors();
    let id = add_node(Policy::Leaf, modifier.size(size, size));
    let phase =
        crate::widgets::animation::draw_phase(id, None, 1.2, compose::animation::LoopMode::Reverse);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let r = w.min(h) * 0.40;
            let a0 = phase.get() * std::f32::consts::TAU;
            const STEPS: usize = 18;
            // 定长数组：旋转弧每帧重采样，避免热路径上的堆分配。
            let mut pts = [(0.0f32, 0.0f32); STEPS + 1];
            for (s, p) in pts.iter_mut().enumerate() {
                let a = a0 + (s as f32 / STEPS as f32) * std::f32::consts::PI * 1.2;
                *p = (cx + a.cos() * r, cy + a.sin() * r);
            }
            b.stroke_path(
                &pts,
                colors.primary,
                (r * 0.25).clamp(1.0, tokens::SPINNER_STROKE),
            );
        }));
    });
}

/// `EmptyState(icon, title, description, actionLabel, onAction)`。
pub fn EmptyState(
    icon_name: IconName,
    title: &str,
    description: &str,
    action_label: Option<&str>,
    on_action: Option<Box<dyn Fn()>>,
) {
    let colors = theme_colors();
    let t = title.to_string();
    let d = description.to_string();
    let alabel = action_label.map(|s| s.to_string());
    let key = RememberOverlayKey();
    if let Some(f) = on_action {
        PutAction(key, f);
    }
    Column(
        Modifier::new().fill_max_width().padding_ltrb(
            tokens::SPACE_XXL,
            tokens::SPACE_XXXL,
            tokens::SPACE_XXL,
            tokens::SPACE_XXXL,
        ),
        Arrangement::Center,
        CrossAxisAlignment::Center,
        move || {
            icon(
                icon_name,
                tokens::EMPTY_STATE_ICON,
                colors.outline,
                Modifier::new(),
            );
            Spacer(Modifier::new().height(tokens::SPACE_MD));
            Text(
                t.clone(),
                tokens::title_medium(colors.on_surface),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(tokens::SPACE_XS));
            Text(
                d.clone(),
                tokens::body_medium(colors.on_surface_variant),
                Modifier::new(),
            );
            if let Some(al) = alabel.clone() {
                Spacer(Modifier::new().height(tokens::SPACE_LG));
                crate::widgets::basic::FilledButton(
                    Modifier::new(),
                    move || RunAction(key),
                    move || {
                        Text(
                            al.clone(),
                            tokens::label_large(colors.on_primary),
                            Modifier::new(),
                        );
                    },
                );
            }
        },
    );
}

/// `Notification(title, body, kind, onClose)` — M3 通知卡：`surface` + 12dp 圆角 + 投影。
pub fn Notification(title: &str, body: &str, kind: AlertKind, on_close: Option<Box<dyn Fn()>>) {
    let colors = theme_colors();
    let (bg, _fg) = kind.colors();
    let t = title.to_string();
    let b = body.to_string();
    let key = RememberOverlayKey();
    if let Some(f) = on_close {
        PutAction(key, f);
    }
    Row(
        Modifier::new()
            .fill_max_width()
            .min_height(tokens::LIST_TWO_LINE)
            .background(colors.Surface, tokens::SHAPE_MEDIUM)
            .border(1.0, colors.outline_variant, tokens::SHAPE_MEDIUM)
            .shadow(
                tokens::ELEVATION_1,
                tokens::SHAPE_MEDIUM,
                tokens::shadow_color(colors.is_dark()),
            )
            .padding_ltrb(
                tokens::SPACE_MD,
                tokens::SPACE_SM,
                tokens::SPACE_XS,
                tokens::SPACE_SM,
            ),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            let t1 = t.clone();
            let b1 = b.clone();
            Row(
                Modifier::new().weight(1.0, false),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    let kid = add_node(
                        Policy::Box(Alignment::Center),
                        Modifier::new()
                            .size(tokens::LIST_ICON_BOX, tokens::LIST_ICON_BOX)
                            .background(bg, tokens::full_radius(tokens::LIST_ICON_BOX)),
                    );
                    with_content_scope(kid, move || {
                        icon(
                            kind.icon(),
                            tokens::ICON_SMALL,
                            colors.on_surface,
                            Modifier::new(),
                        );
                    });
                    Spacer(Modifier::new().width(tokens::SPACE_MD));
                    let t2 = t1.clone();
                    let b2 = b1.clone();
                    Column(
                        Modifier::new().weight(1.0, false),
                        Arrangement::Center,
                        CrossAxisAlignment::Start,
                        move || {
                            Text(
                                t2.clone(),
                                tokens::body_large(colors.on_surface),
                                Modifier::new(),
                            );
                            Text(
                                b2.clone(),
                                tokens::body_medium(colors.on_surface_variant),
                                Modifier::new(),
                            );
                        },
                    );
                },
            );
            let cid = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new().size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
            );
            with_rt_pub(|rt| {
                rt.nodes[cid.index()].on_click = Some(Box::new(move || RunAction(key)));
            });
            with_content_scope(cid, move || {
                icon(
                    IconName::Close,
                    tokens::ICON_SMALL,
                    colors.on_surface_variant,
                    Modifier::new(),
                );
            });
        },
    );
}

/// `NotificationStack(items, onClose)` — 通知堆叠（`onClose` 按索引回调）。
pub fn NotificationStack(
    items: &[(String, String, AlertKind)],
    on_close: impl Fn(usize) + 'static,
) {
    let list: Vec<(String, String, AlertKind)> = items.to_vec();
    let key = RememberOverlayKey();
    PutIndexedAction(key, on_close);
    Column(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            for (i, (t, b, k)) in list.clone().into_iter().enumerate() {
                Notification(&t, &b, k, Some(Box::new(move || RunIndexedAction(key, i))));
                Spacer(Modifier::new().height(tokens::SPACE_MD));
            }
        },
    );
}

/// `CommandPalette(open, query, items, onSelected)` — 命令面板。
pub fn CommandPalette(
    open: MutableState<bool>,
    query: MutableState<String>,
    items: &[String],
    on_selected: impl FnMut(usize) + 'static,
) {
    let key = RememberOverlayKey();
    let is_open = open.get();
    crate::widgets::input::set_menu_callback(key, on_selected);
    let q = query.read_clone().to_lowercase();
    let filtered: Vec<(usize, String)> = items
        .iter()
        .enumerate()
        .filter(|(_, s)| q.is_empty() || s.to_lowercase().contains(&q))
        .map(|(i, s)| (i, s.clone()))
        .collect();
    let (vw, _vh) = viewport_size();
    PutOverlayContent(key, move || {
        let colors = theme_colors();
        let g = glass_spec();
        let (t, scrim_c) = overlay_enter(key, &colors);
        let scrim = add_node(
            Policy::Leaf,
            Modifier::new()
                .fill_max_size()
                .no_indication()
                .background(scrim_c, 0.0),
        );
        with_rt_pub(|rt| {
            rt.nodes[scrim.index()].on_click = Some(Box::new(move || open.set(false)));
        });
        let mut m = GlassOrBg(
            Modifier::new()
                .offset(
                    (vw - tokens::COMMAND_PALETTE_W) * 0.5,
                    tokens::SPACE_XXXL + (1.0 - t) * tokens::OVERLAY_RISE,
                )
                .width(tokens::COMMAND_PALETTE_W),
            g,
            colors.Surface,
            tokens::SHAPE_LARGE,
        )
        .padding_ltrb(
            tokens::SPACE_XS,
            tokens::SPACE_XS,
            tokens::SPACE_XS,
            tokens::SPACE_XS,
        )
        .clip(tokens::SHAPE_LARGE);
        if !g.enabled {
            m = m.shadow(
                tokens::ELEVATION_3,
                tokens::SHAPE_LARGE,
                tokens::shadow_color(colors.is_dark()),
            );
        }
        let id = add_node(
            Policy::Column {
                arrangement: Arrangement::Start,
                cross: CrossAxisAlignment::Stretch,
            },
            m,
        );
        let fl = filtered.clone();
        with_content_scope(id, move || {
            crate::widgets::basic::SearchBar(
                query,
                "Type a command…",
                Modifier::new().fill_max_width().padding_ltrb(
                    tokens::SPACE_XS,
                    tokens::SPACE_XS,
                    tokens::SPACE_XS,
                    0.0,
                ),
            );
            Spacer(Modifier::new().height(tokens::SPACE_SM));
            for (idx, label) in fl.clone() {
                let cid = add_node(
                    Policy::Leaf,
                    Modifier::new()
                        .fill_max_width()
                        .height(tokens::MENU_ITEM_HEIGHT)
                        .padding_ltrb(tokens::SPACE_LG, 0.0, tokens::SPACE_LG, 0.0),
                );
                with_rt_pub(|rt| {
                    rt.nodes[cid.index()].on_click = Some(Box::new(move || {
                        crate::widgets::input::run_menu_callback(key, idx);
                        open.set(false);
                    }));
                });
                with_rt_pub(|rt| {
                    let lb = label.clone();
                    rt.nodes[cid.index()].draw =
                        Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, h| {
                            let style = tokens::body_large(colors.on_surface);
                            let th = measure_text_with_measurer(&lb, &style).1;
                            b.draw_text(x, y + (h - th) * 0.5, &lb, &style, f32::INFINITY);
                        }));
                });
            }
        });
    });
    OverlaySlot(key, is_open);
}
