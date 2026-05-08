//! 布局与脚手架 — Scaffold / TopAppBar / FlowRow / Grid / Split …

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Arrangement, Box, Column, CrossAxisAlignment, MeasureCtx, MeasurePolicy,
    MeasureResult, Modifier, NodeId, Policy, Row, Spacer, add_node, begin_drag_session,
    last_pointer_global, layout, with_content_scope,
};
use compose::material::{IconName, glass_spec, icon, theme_colors};
use compose::runtime::{MutableState, mutableStateOf, remember, with_rt_pub};

use crate::tokens;

use crate::widgets::overlay::{
    PutIndexedAction, PutOverlayContent, RememberOverlayKey, RunIndexedAction, call_overlay_content,
};
use crate::widgets::{GlassOrBg, InkWell, Text};

// ══════════════════════════════ Scaffold ══════════════════════════════

/// 应用栏动作（`on_click` 用 `Fn`：状态写入只需 `&self`）。
pub struct AppBarAction {
    pub icon: IconName,
    pub on_click: Box<dyn Fn()>,
}

impl AppBarAction {
    pub fn new(icon: IconName, on_click: impl Fn() + 'static) -> Self {
        Self {
            icon,
            on_click: Box::new(on_click),
        }
    }
}

/// `Scaffold(topBar, bottomBar) { content }` — Compose `Scaffold`。
pub fn Scaffold(
    modifier: Modifier,
    top_bar: Option<Box<dyn Fn()>>,
    bottom_bar: Option<Box<dyn Fn()>>,
    content: impl Fn() + 'static,
) {
    let ck = RememberOverlayKey();
    PutOverlayContent(ck, content);
    Column(
        modifier.fill_max_size(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            if let Some(tb) = top_bar.as_ref() {
                tb();
            }
            Box(
                Modifier::new().weight(1.0, true),
                Alignment::TopStart,
                move || {
                    call_overlay_content(ck);
                },
            );
            if let Some(bb) = bottom_bar.as_ref() {
                bb();
            }
        },
    );
}

/// `TopAppBar(title, navigationIcon, actions)` — Compose `TopAppBar`。
///
/// M3 规格：栏高 64dp、标题 `titleLarge`（22/28）、前后图标 24dp 装在 40dp 触控目标里、
/// 容器 `surface` + elevation 1dp 投影。
pub fn TopAppBar(
    title: impl Into<String>,
    navigation_icon: Option<IconName>,
    actions: Vec<AppBarAction>,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let g = glass_spec();
    let title = title.into();
    let ak = RememberOverlayKey();
    let icons: Vec<IconName> = actions.iter().map(|a| a.icon).collect();
    PutIndexedAction(ak, move |i: usize| {
        if let Some(a) = actions.get(i) {
            (a.on_click)();
        }
    });
    let mut m = GlassOrBg(modifier.fill_max_width(), g, colors.Surface, 0.0)
        .padding_ltrb(tokens::SPACE_XS, 0.0, tokens::SPACE_XS, 0.0)
        .height(tokens::APPBAR_HEIGHT);
    if !g.enabled {
        m = m.background(colors.Surface, 0.0).shadow(
            tokens::ELEVATION_1,
            0.0,
            tokens::shadow_color(colors.is_dark()),
        );
    }
    let id = add_node(
        Policy::Row {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Center,
        },
        m,
    );
    with_content_scope(id, move || {
        if let Some(n) = navigation_icon {
            let nid = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new().size(tokens::LIST_ICON_BOX, tokens::LIST_ICON_BOX),
            );
            with_rt_pub(|rt| {
                rt.nodes[nid.index()].on_click = Some(Box::new(|| {}));
            });
            with_content_scope(nid, move || {
                icon(
                    n,
                    tokens::ICON_DEFAULT,
                    colors.on_surface_variant,
                    Modifier::new(),
                );
            });
            Spacer(Modifier::new().width(tokens::SPACE_XS));
        }
        Text(
            title.clone(),
            tokens::title_large(colors.on_surface),
            Modifier::new()
                .weight(1.0, false)
                .padding_ltrb(tokens::SPACE_SM, 0.0, 0.0, 0.0),
        );
        for (i, ic) in icons.clone().into_iter().enumerate() {
            let aid = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new().size(tokens::LIST_ICON_BOX, tokens::LIST_ICON_BOX),
            );
            with_rt_pub(|rt| {
                rt.nodes[aid.index()].on_click = Some(Box::new(move || RunIndexedAction(ak, i)));
            });
            with_content_scope(aid, move || {
                icon(
                    ic,
                    tokens::ICON_DEFAULT,
                    colors.on_surface_variant,
                    Modifier::new(),
                );
            });
        }
    });
}

/// `BottomAppBar(actions)` — M3 底部应用栏：`surface_container` 64dp 高，图标 24dp / 40dp 目标。
pub fn BottomAppBar(actions: Vec<AppBarAction>, modifier: Modifier) {
    let colors = theme_colors();
    let g = glass_spec();
    let bk = RememberOverlayKey();
    let icons: Vec<IconName> = actions.iter().map(|a| a.icon).collect();
    PutIndexedAction(bk, move |i: usize| {
        if let Some(a) = actions.get(i) {
            (a.on_click)();
        }
    });
    let m = GlassOrBg(modifier.fill_max_width(), g, colors.surface_container, 0.0)
        .padding_ltrb(tokens::SPACE_XS, 0.0, tokens::SPACE_XS, 0.0)
        .height(tokens::APPBAR_HEIGHT);
    Row(
        m,
        Arrangement::SpaceEvenly,
        CrossAxisAlignment::Center,
        move || {
            for (i, ic) in icons.clone().into_iter().enumerate() {
                let aid = add_node(
                    Policy::Box(Alignment::Center),
                    Modifier::new().size(tokens::LIST_ICON_BOX, tokens::LIST_ICON_BOX),
                );
                with_rt_pub(|rt| {
                    rt.nodes[aid.index()].on_click =
                        Some(Box::new(move || RunIndexedAction(bk, i)));
                });
                with_content_scope(aid, move || {
                    icon(
                        ic,
                        tokens::ICON_DEFAULT,
                        colors.on_surface_variant,
                        Modifier::new(),
                    );
                });
            }
        },
    );
}

// ══════════════════════════════ 自定义测量策略 ══════════════════════════════

/// 流式换行布局 — Compose `FlowRow`。
struct FlowPolicy {
    spacing: f32,
}

impl MeasurePolicy for FlowPolicy {
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx,
        measurables: &[NodeId],
        constraints: Constraints,
    ) -> MeasureResult {
        let max_w = if constraints.has_bounded_width() {
            constraints.max_width
        } else {
            f32::INFINITY
        };
        let mut placements = Vec::new();
        let (mut x, mut y, mut row_h, mut max_x) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for &m in measurables {
            let p = ctx.measure(m, Constraints::new(0.0, max_w, 0.0, constraints.max_height));
            if x > 0.0 && x + p.width > max_w {
                x = 0.0;
                y += row_h + self.spacing;
                row_h = 0.0;
            }
            placements.push((m, x, y));
            x += p.width + self.spacing;
            max_x = max_x.max(x);
            row_h = row_h.max(p.height);
        }
        MeasureResult {
            width: constraints.constrain_width(max_x),
            height: constraints.constrain_height(y + row_h),
            placements,
        }
    }
}

/// `FlowRow(spacing) { content }` — 自动换行行布局。
pub fn FlowRow(modifier: Modifier, spacing: f32, content: impl Fn() + 'static) {
    layout(modifier, Box::new(FlowPolicy { spacing }), content);
}

/// 网格布局 — `GridView`。
struct GridPolicy {
    columns: usize,
    spacing: f32,
}

impl MeasurePolicy for GridPolicy {
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx,
        measurables: &[NodeId],
        constraints: Constraints,
    ) -> MeasureResult {
        let cols = self.columns.max(1);
        let cell_w = if constraints.has_bounded_width() {
            ((constraints.max_width - self.spacing * (cols - 1) as f32) / cols as f32).max(0.0)
        } else {
            0.0
        };
        let mut placements = Vec::new();
        let (mut y, mut row_h, mut max_x) = (0.0f32, 0.0f32, 0.0f32);
        for (i, &m) in measurables.iter().enumerate() {
            let c = if cell_w > 0.0 {
                Constraints::new(cell_w, cell_w, 0.0, constraints.max_height)
            } else {
                Constraints::new(0.0, constraints.max_width, 0.0, constraints.max_height)
            };
            let p = ctx.measure(m, c);
            let col = i % cols;
            if col == 0 && i > 0 {
                y += row_h + self.spacing;
                row_h = 0.0;
            }
            let x = col as f32 * (cell_w + self.spacing);
            placements.push((m, x, y));
            row_h = row_h.max(p.height);
            max_x = max_x.max(x + p.width);
        }
        MeasureResult {
            width: constraints.constrain_width(max_x),
            height: constraints.constrain_height(y + row_h),
            placements,
        }
    }
}

/// `Grid(columns, spacing) { content }` — 等宽网格。
pub fn Grid(columns: usize, spacing: f32, modifier: Modifier, content: impl Fn() + 'static) {
    layout(modifier, Box::new(GridPolicy { columns, spacing }), content);
}

/// 固定宽高比 — Compose `AspectRatio`。
struct AspectPolicy {
    ratio: f32,
}

impl MeasurePolicy for AspectPolicy {
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx,
        measurables: &[NodeId],
        constraints: Constraints,
    ) -> MeasureResult {
        let w = if constraints.has_bounded_width() {
            constraints.max_width
        } else {
            0.0
        };
        let h = w / self.ratio.max(0.01);
        let mut placements = Vec::new();
        for &m in measurables {
            let p = ctx.measure(m, Constraints::tight(w, h));
            placements.push((m, 0.0, 0.0));
            let _ = p;
        }
        MeasureResult {
            width: constraints.constrain_width(w),
            height: constraints.constrain_height(h),
            placements,
        }
    }
}

/// `AspectRatio(ratio) { content }`。
pub fn AspectRatio(ratio: f32, modifier: Modifier, content: impl Fn() + 'static) {
    layout(modifier, Box::new(AspectPolicy { ratio }), content);
}

// ══════════════════════════════ 尺寸 / 分割 / 展开 ══════════════════════════════

/// `SizedBox(width, height) { content }` — 固定尺寸容器。
pub fn SizedBox(width: f32, height: f32, modifier: Modifier, content: impl Fn() + 'static) {
    Box(modifier.size(width, height), Alignment::TopStart, content);
}

/// `SplitRow(ratio) { left | right }` — 可拖拽分隔的横向分割（0..1 为左占比）。
///
/// `ratio` 为初始比例；拖拽中间分隔条可在运行期实时调整（拖拽状态内部 `remember`，
/// 会话内持久）。拖拽不依赖节点 id，靠容器几何快照 + 比例 `MutableState` 实现。
pub fn SplitRow(
    modifier: Modifier,
    ratio: f32,
    left: impl Fn() + 'static,
    right: impl Fn() + 'static,
) {
    let rstate = remember(|| mutableStateOf(ratio.clamp(0.05, 0.95)));
    let r = rstate.get();
    let k1 = RememberOverlayKey();
    let k2 = RememberOverlayKey();
    PutOverlayContent(k1, left);
    PutOverlayContent(k2, right);
    Row(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Box(
                Modifier::new().weight(r, true).fill_max_height(),
                Alignment::TopStart,
                move || call_overlay_content(k1),
            );
            let colors = theme_colors();
            let rid = add_node(
                Policy::Leaf,
                Modifier::new()
                    .width(6.0)
                    .fill_max_height()
                    .background(colors.outline_variant, 0.0),
            );
            with_rt_pub(|rt| {
                rt.nodes[rid.index()].on_press =
                    Some(Box::new(move || split_grab(rid, rstate, true)));
                rt.nodes[rid.index()].on_click =
                    Some(Box::new(move || split_grab(rid, rstate, true)));
            });
            Box(
                Modifier::new().weight(1.0 - r, true).fill_max_height(),
                Alignment::TopStart,
                move || call_overlay_content(k2),
            );
        },
    );
}

/// 分隔条抓取：开启拖拽会话 + 把比例立刻跳到当前指针。
///
/// 必须在**按下**时调用（`Node::on_press`）——抬起才结算点击后，若只挂在 `on_click`
/// 上，拖拽期间指针根本进不了 `drag_move_global`。
fn split_grab(rid: NodeId, rstate: MutableState<f32>, horizontal: bool) {
    let (gx, gy) = last_pointer_global();
    let geo = with_rt_pub(|rt| {
        rt.nodes[rid.index()].parent.map(|p| {
            let n = &rt.nodes[p.index()];
            if horizontal {
                (n.x, n.width)
            } else {
                (n.y, n.height)
            }
        })
    });
    let (co, cl) = geo.unwrap_or((0.0, 1.0));
    if horizontal {
        begin_drag_session(co, 0.0, cl, 1.0, rstate, true, 0.05, 0.95);
    } else {
        begin_drag_session(0.0, co, 1.0, cl, rstate, false, 0.05, 0.95);
    }
    let nr = if cl > 0.0 {
        (((if horizontal { gx } else { gy }) - co) / cl).clamp(0.05, 0.95)
    } else {
        rstate.get()
    };
    rstate.set(nr);
}

/// `SplitColumn(ratio) { top | bottom }` — 可拖拽分隔的纵向分割（0..1 为上占比）。
pub fn SplitColumn(
    modifier: Modifier,
    ratio: f32,
    top: impl Fn() + 'static,
    bottom: impl Fn() + 'static,
) {
    let rstate = remember(|| mutableStateOf(ratio.clamp(0.05, 0.95)));
    let r = rstate.get();
    let k1 = RememberOverlayKey();
    let k2 = RememberOverlayKey();
    PutOverlayContent(k1, top);
    PutOverlayContent(k2, bottom);
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Box(
                Modifier::new().weight(r, true).fill_max_width(),
                Alignment::TopStart,
                move || call_overlay_content(k1),
            );
            let colors = theme_colors();
            let rid = add_node(
                Policy::Leaf,
                Modifier::new()
                    .height(6.0)
                    .fill_max_width()
                    .background(colors.outline_variant, 0.0),
            );
            with_rt_pub(|rt| {
                rt.nodes[rid.index()].on_press =
                    Some(Box::new(move || split_grab(rid, rstate, false)));
                rt.nodes[rid.index()].on_click =
                    Some(Box::new(move || split_grab(rid, rstate, false)));
            });
            Box(
                Modifier::new().weight(1.0 - r, true).fill_max_width(),
                Alignment::TopStart,
                move || call_overlay_content(k2),
            );
        },
    );
}

/// `Expandable(expanded, bodyHeight, header) { body }` — 带高度动画的可展开块。
pub fn Expandable(
    modifier: Modifier,
    expanded: MutableState<bool>,
    body_height: f32,
    header: impl Fn() + 'static,
    body: impl Fn() + 'static,
) {
    let open = expanded.get();
    let frac = compose::animation::animate_float_as_state(
        if open { 1.0 } else { 0.0 },
        compose::animation::Tween::new(0.22),
    );
    let bk = RememberOverlayKey();
    PutOverlayContent(bk, body);
    let hk = RememberOverlayKey();
    PutOverlayContent(hk, header);
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            // 表头：点击切换展开态（之前缺失点击绑定，导致无法展开/折叠）
            let st = expanded;
            InkWell(
                Modifier::new()
                    .fill_max_width()
                    .min_height(tokens::LIST_ONE_LINE),
                move || {
                    let s = st;
                    s.set(!s.get());
                },
                move || call_overlay_content(hk),
            );
            Box(
                Modifier::new()
                    .fill_max_width()
                    .height(frac * body_height)
                    .clip(0.0),
                Alignment::TopStart,
                move || call_overlay_content(bk),
            );
        },
    );
}
