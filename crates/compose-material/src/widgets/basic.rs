//! 基础控件 — Text / Button / Card / Chip / Avatar / Badge …

use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Box, Column, CrossAxisAlignment, DrawBuilder, Modifier, Policy, Row,
    Spacer, TextStyle, add_node, measure_text_with_measurer, with_content_scope,
};
use compose::material::{
    ColorScheme, IconName, LocalContentColor, draw_icon, glass_spec, icon, theme_colors,
};
use compose::runtime::{MutableState, with_rt_pub};

use crate::tokens;
use crate::widgets::{GlassOrBg, Text};

// ══════════════════════════════ 容器 ══════════════════════════════

/// `Surface(modifier, color, shape) { content }` — Compose `Surface`（玻璃态自动走毛玻璃）。
pub fn Surface(
    modifier: Modifier,
    color: compose::foundation::Color,
    shape_radius: f32,
    content: impl Fn() + 'static,
) {
    let g = glass_spec();
    let m = GlassOrBg(modifier, g, color, shape_radius);
    let id = add_node(Policy::Box(Alignment::Center), m);
    with_content_scope(id, content);
}

/// `Card { content }` — Compose `Card`（M3：`surface_container` + 1dp 描边）。
pub fn Card(modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    let g = glass_spec();
    let mut m = GlassOrBg(modifier, g, colors.surface_container, tokens::SHAPE_MEDIUM);
    if !g.enabled {
        m = m.border(1.0, colors.outline_variant, tokens::SHAPE_MEDIUM);
    }
    panel(m.padding_hv(tokens::SPACE_LG, tokens::SPACE_LG), content);
}

/// `ElevatedCard { content }` — M3 `ElevatedCard`：`surface_container` + 1 档投影。
pub fn ElevatedCard(modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    let m = GlassOrBg(
        modifier,
        glass_spec(),
        colors.surface_container,
        tokens::SHAPE_MEDIUM,
    )
    .shadow(
        tokens::ELEVATION_1,
        tokens::SHAPE_MEDIUM,
        tokens::shadow_color(colors.is_dark()),
    );
    panel(m.padding_hv(tokens::SPACE_LG, tokens::SPACE_LG), content);
}

/// `OutlinedCard { content }` — M3 `OutlinedCard`：无底色 + 1dp `outline_variant`。
pub fn OutlinedCard(modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    let m = modifier
        .border(1.0, colors.outline_variant, tokens::SHAPE_MEDIUM)
        .padding_hv(tokens::SPACE_LG, tokens::SPACE_LG);
    panel(m, content);
}

/// `FilledCard { content }` — M3 `FilledCard`：`surface_variant` 实底、无描边。
pub fn FilledCard(modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    let g = glass_spec();
    let m = GlassOrBg(modifier, g, colors.surface_variant, tokens::SHAPE_MEDIUM)
        .padding_hv(tokens::SPACE_LG, tokens::SPACE_LG);
    panel(m, content);
}

fn panel(m: Modifier, content: impl Fn() + 'static) {
    let id = add_node(
        Policy::Column {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Start,
        },
        m,
    );
    with_content_scope(id, content);
}

/// `Divider` — Compose `HorizontalDivider`（M3：1dp `outline_variant`）。
pub fn Divider(modifier: Modifier) {
    let colors = theme_colors();
    let m = modifier.height(1.0).background(colors.outline_variant, 0.0);
    add_node(Policy::Leaf, m);
}

/// `VerticalDivider`。
pub fn VerticalDivider(modifier: Modifier) {
    let colors = theme_colors();
    let m = modifier.width(1.0).background(colors.outline_variant, 0.0);
    add_node(Policy::Leaf, m);
}

/// `InkWell(onClick) { content }` — 带悬停高亮（ripple 静态近似）的可点击容器。
///
/// 覆盖层绘制在内容**之上**且标 `no_hit`，
/// 因此点击仍能落到本容器。默认圆角 8.0，需要其它圆角用 [`InkWellR`]。
pub fn InkWell(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) -> compose::layout::NodeId {
    InkWellR(modifier, 8.0, on_click, content)
}

/// `InkWell(onClick, radius) { content }` — 可指定覆盖层圆角（`border_radius` 参数）。
pub fn InkWellR(
    modifier: Modifier,
    radius: f32,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) -> compose::layout::NodeId {
    let hovered = compose::runtime::remember(|| compose::runtime::mutableStateOf(false));
    let colors = theme_colors();
    let id = add_node(Policy::Box(Alignment::TopStart), modifier);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(on_click));
        rt.nodes[id.index()].hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
    });
    with_content_scope(id, move || {
        content();
        // 覆盖层经节点自身 draw 绘制（尺寸 = 节点最终 w/h）。不能用 fill_max_size
        // 子节点：Box 测量取子节点最大值，松约束下覆盖层会把 InkWell 撑满可用高度。
        // 组合期读取 hovered 以订阅悬停态（变化 → 重组 → 重绘）；
        // draw 闭包内不得访问 runtime（draw 树遍历持有 runtime 借用），故捕获布尔值。
        let on = hovered.get();
        let color = tokens::state_layer(colors.on_surface, tokens::STATE_HOVER);
        with_rt_pub(|rt| {
            rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, hgt| {
                if on {
                    b.fill_rrect(x, y, w, hgt, radius, color);
                }
            }));
        });
    });
    id
}

// ══════════════════════════════ 按钮 ══════════════════════════════

/// 按钮语义变体 — 对齐 `androidx.compose.material3` 按钮家族。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ButtonVariant {
    /// `FilledButton`：`primary` 底 + `on_primary` 字。
    #[default]
    Primary,
    /// `FilledTonalButton`：`secondary_container` 底 + `on_secondary_container` 字。
    Secondary,
    /// 危险操作：`error` 底 + `on_error` 字。
    Danger,
    /// `TextButton`：透明底 + `primary` 字（悬停状态层）。
    Ghost,
    /// 链接样式：同 `TextButton`，但无最小高度 / 内边距。
    Link,
}

/// 按钮完整选项 — 变体 + 状态（`ButtonEx` / 内部复用）。
#[derive(Clone)]
pub struct ButtonOpts {
    pub variant: ButtonVariant,
    /// `OutlinedButton`：1px `outline` 描边 + 透明底。
    pub outlined: bool,
    /// `ElevatedButton`：`surface` 底 + 1 档投影。
    pub elevated: bool,
    /// 禁用：`on_surface` 12% 底 + 38% 字 + 命中穿透。
    pub enabled: bool,
    /// 选中：`primary` 实底且不随 hover 变。
    pub selected: bool,
    /// 覆盖变体底色（IconButton / FAB 等自定义底色的控件用）。
    pub bg_override: Option<Color>,
    /// 覆盖文字色（`bg_override` 搭配使用；`None` = 按变体取）。
    pub fg_override: Option<Color>,
    pub radius: f32,
    pub min_height: f32,
    pub pad_h: f32,
    pub pad_v: f32,
}

impl Default for ButtonOpts {
    fn default() -> Self {
        Self {
            variant: ButtonVariant::Primary,
            outlined: false,
            elevated: false,
            enabled: true,
            selected: false,
            bg_override: None,
            fg_override: None,
            radius: tokens::full_radius(tokens::CONTROL_HEIGHT),
            min_height: tokens::CONTROL_HEIGHT,
            pad_h: tokens::BUTTON_PAD_H,
            pad_v: tokens::SPACE_XS,
        }
    }
}

impl ButtonOpts {
    /// 语义变体（链式）。
    pub fn variant_(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }
    /// 禁用态（链式）。
    pub fn enabled_(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
}

/// 变体的 (容器色, 内容色) — M3 `ButtonDefaults`。
fn variant_colors(v: ButtonVariant, t: &ColorScheme) -> (Color, Color) {
    match v {
        ButtonVariant::Primary => (t.primary, t.on_primary),
        ButtonVariant::Secondary => (t.secondary_container, t.on_secondary_container),
        ButtonVariant::Danger => (t.error, t.on_error),
        // Ghost / Link：无容器，文字用主色
        ButtonVariant::Ghost | ButtonVariant::Link => (Color::rgba(0.0, 0.0, 0.0, 0.0), t.primary),
    }
}

/// 按钮内核 — M3 容器色 + 状态层（悬停）+ 禁用态。
///
/// 状态优先级：`disabled` > `selected` > `hovered` > `normal`。
/// M3 的悬停 / 按下不是「把底色调暗」，而是在容器上叠一层 `onContainer` 半透明状态层，
/// 因此实底按钮在悬停时色相不变、只是变深一点点。
fn button_core(
    modifier: Modifier,
    o: ButtonOpts,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) {
    let colors = theme_colors();
    let hovered = compose::runtime::remember(|| compose::runtime::mutableStateOf(false));
    let hov = hovered.get();
    let (base, fg) = (
        o.bg_override
            .unwrap_or_else(|| variant_bg_color(o.variant, &colors)),
        o.fg_override
            .unwrap_or_else(|| variant_colors(o.variant, &colors).1),
    );
    // 内容色：禁用统一 on_surface 38%（M3 各变体一致）
    let content_color = if o.enabled {
        fg
    } else {
        colors.on_surface.with_alpha(tokens::STATE_DISABLED_CONTENT)
    };

    let (bg, border, layer) = if !o.enabled {
        (
            colors.on_surface.with_alpha(tokens::STATE_DISABLED_BG),
            None,
            0.0f32,
        )
    } else if o.selected {
        (colors.primary, None, 0.0)
    } else if o.outlined {
        // 描边按钮：透明底 + 1dp outline + 主色状态层
        (
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            Some(colors.outline),
            if hov { tokens::STATE_PRESSED } else { 0.0 },
        )
    } else {
        match o.variant {
            ButtonVariant::Ghost | ButtonVariant::Link => (
                Color::rgba(0.0, 0.0, 0.0, 0.0),
                None,
                if hov { tokens::STATE_HOVER } else { 0.0 },
            ),
            // 实底 / tonal / danger：状态层用「内容色」叠加，色相保持不变
            _ => (base, None, if hov { tokens::STATE_HOVER } else { 0.0 }),
        }
    };

    let g = glass_spec();
    let transparent = bg.a <= 0.001;
    let mut m = if transparent {
        modifier.background(bg, o.radius)
    } else {
        GlassOrBg(modifier, g, bg, o.radius)
    };
    if o.elevated && o.enabled {
        m = m.shadow(
            tokens::ELEVATION_1,
            o.radius,
            tokens::shadow_color(colors.is_dark()),
        );
    }
    if let Some(bc) = border {
        m = m.border(1.0, bc, o.radius);
    }
    m = m.padding_hv(o.pad_h, o.pad_v).min_height(o.min_height);
    if !o.enabled {
        // 等价 Compose `enabled = false`：本节点及子树不参与命中
        m = m.no_hit();
    }

    let id = add_node(
        Policy::Row {
            arrangement: Arrangement::Center,
            cross: CrossAxisAlignment::Center,
        },
        m,
    );
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[id.index()];
        if o.enabled {
            n.on_click = Some(Box::new(on_click));
        }
        n.hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
        if layer > 0.0 {
            // 状态层：本节点后缀绘制（在孩子之后），圆角与容器一致
            let lc = if o.outlined {
                colors.primary
            } else {
                content_color
            }
            .with_alpha(layer);
            let r = o.radius;
            n.draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                b.fill_rrect(x, y, w, h, r, lc);
            }));
        }
    });
    // 内容前景色经 `LocalContentColor` 下发到本节点的内容作用域：定向重组时祖先链会重建
    // 这个值，线程局部变量做不到（否则禁用 / 反色文字会退回默认色）。
    LocalContentColor::provides_in(id, content_color, content);
}

/// 变体容器色（`bg_override` 未指定时）。
fn variant_bg_color(v: ButtonVariant, t: &ColorScheme) -> Color {
    variant_colors(v, t).0
}

/// `labelLarge` 按钮文字（M3 排版角色）；颜色由所在容器的 `LocalContentColor` 决定。
pub fn button_label(label: impl Into<String>) {
    Text(
        label.into(),
        tokens::label_large(Color::BLACK),
        Modifier::new(),
    );
}

/// `labelLarge` 按钮图标（与文字同色，18dp）。
pub fn button_icon(name: IconName) {
    icon(
        name,
        tokens::BUTTON_ICON,
        LocalContentColor::current(),
        Modifier::new().align(Alignment::Center),
    );
}

/// `Button(onClick) { content }` — Compose `FilledButton`。
pub fn Button(modifier: Modifier, on_click: impl FnMut() + 'static, content: impl Fn() + 'static) {
    FilledButton(modifier, on_click, content);
}

/// `FilledButton(onClick) { content }`。
pub fn FilledButton(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) {
    button_core(modifier, ButtonOpts::default(), on_click, content);
}

/// `OutlinedButton(onClick) { content }`。
pub fn OutlinedButton(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) {
    button_core(
        modifier,
        ButtonOpts {
            outlined: true,
            ..Default::default()
        },
        on_click,
        content,
    );
}

/// `FilledTonalButton` — `secondary_container` 底 + `on_secondary_container` 字。
pub fn TonalButton(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) {
    button_core(
        modifier,
        ButtonOpts {
            variant: ButtonVariant::Secondary,
            ..Default::default()
        },
        on_click,
        content,
    );
}

/// `ElevatedButton` — `surface` 底 + 1 档投影 + `primary` 字。
pub fn ElevatedButton(
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) {
    let colors = theme_colors();
    button_core(
        modifier,
        ButtonOpts {
            variant: ButtonVariant::Primary,
            elevated: true,
            bg_override: Some(colors.Surface),
            fg_override: Some(colors.primary),
            ..Default::default()
        },
        on_click,
        content,
    );
}

/// `TextButton(label, onClick)` — M3 文本按钮（透明底 + 主色字 + 悬停状态层）。
pub fn TextButton(label: impl Into<String>, modifier: Modifier, on_click: impl FnMut() + 'static) {
    let label = label.into();
    button_core(
        modifier,
        ButtonOpts {
            variant: ButtonVariant::Ghost,
            pad_h: tokens::SPACE_MD,
            ..Default::default()
        },
        on_click,
        move || {
            button_label(label.clone());
        },
    );
}

/// `IconButton(icon, onClick)` — 40dp 圆形触控目标，图标 24dp（M3 `IconButton`）。
pub fn IconButton(name: IconName, modifier: Modifier, on_click: impl FnMut() + 'static) {
    let colors = theme_colors();
    button_core(
        modifier.size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
        ButtonOpts {
            variant: ButtonVariant::Ghost,
            fg_override: Some(colors.on_surface_variant),
            radius: tokens::full_radius(tokens::CONTROL_HEIGHT),
            min_height: tokens::CONTROL_HEIGHT,
            pad_h: 0.0,
            pad_v: 0.0,
            ..Default::default()
        },
        on_click,
        move || {
            icon(
                name,
                tokens::ICON_DEFAULT,
                LocalContentColor::current(),
                Modifier::new().align(Alignment::Center),
            );
        },
    );
}

/// 完整能力按钮：`variant` / 禁用 / 选中 / 尺寸全部可配。
pub fn ButtonEx(
    modifier: Modifier,
    opts: ButtonOpts,
    on_click: impl FnMut() + 'static,
    content: impl Fn() + 'static,
) {
    button_core(modifier, opts, on_click, content);
}

/// `FloatingActionButton(icon, onClick)` — 56dp / 圆角 16 / `primary_container`（M3 FAB）。
pub fn Fab(name: IconName, modifier: Modifier, on_click: impl FnMut() + 'static) {
    let colors = theme_colors();
    button_core(
        modifier.size(tokens::FAB_SIZE, tokens::FAB_SIZE),
        ButtonOpts {
            variant: ButtonVariant::Secondary,
            elevated: true,
            bg_override: Some(colors.primary_container),
            fg_override: Some(colors.on_primary_container),
            radius: tokens::FAB_RADIUS,
            min_height: tokens::FAB_SIZE,
            pad_h: 0.0,
            pad_v: 0.0,
            ..Default::default()
        },
        on_click,
        move || {
            icon(
                name,
                tokens::FAB_ICON,
                LocalContentColor::current(),
                Modifier::new().align(Alignment::Center),
            );
        },
    );
}

/// `ExtendedFAB(icon, label, onClick)` — 高 56 / 圆角 16 / `labelLarge`。
pub fn ExtendedFab(
    name: IconName,
    label: impl Into<String>,
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
) {
    let colors = theme_colors();
    let label = label.into();
    button_core(
        modifier,
        ButtonOpts {
            variant: ButtonVariant::Secondary,
            elevated: true,
            bg_override: Some(colors.primary_container),
            fg_override: Some(colors.on_primary_container),
            radius: tokens::FAB_RADIUS,
            min_height: tokens::FAB_SIZE,
            pad_h: tokens::BUTTON_PAD_H_ICON,
            pad_v: 0.0,
            ..Default::default()
        },
        on_click,
        move || {
            icon(
                name,
                tokens::FAB_ICON,
                LocalContentColor::current(),
                Modifier::new(),
            );
            Spacer(Modifier::new().width(tokens::SPACE_MD));
            Text(
                label.clone(),
                tokens::title_medium(Color::BLACK),
                Modifier::new(),
            );
        },
    );
}

/// 可展开 FAB：点击展开一组动作按钮（高度动画）。
pub fn ExpandableFab(
    name: IconName,
    actions: &[(IconName, String)],
    expanded: MutableState<bool>,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let open = expanded.get();
    let frac = compose::animation::animate_float_as_state(
        if open { 1.0 } else { 0.0 },
        compose::animation::Tween::new(0.22),
    );
    let items: Vec<(IconName, String)> = actions.to_vec();
    let est = items.len() as f32 * 48.0;
    let ex = expanded;
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::End,
        move || {
            let items = items.clone();
            Box(
                Modifier::new().height(frac * est).clip(0.0),
                Alignment::TopStart,
                move || {
                    let items = items.clone();
                    Column(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::End,
                        move || {
                            for (ic, lb) in items.clone() {
                                let lb = lb.clone();
                                let ex2 = ex;
                                let m = Modifier::new()
                                    .height(tokens::CONTROL_HEIGHT)
                                    .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0)
                                    .background(colors.secondary_container, tokens::SHAPE_MEDIUM);
                                let id = add_node(
                                    Policy::Row {
                                        arrangement: Arrangement::Center,
                                        cross: CrossAxisAlignment::Center,
                                    },
                                    m,
                                );
                                with_rt_pub(|rt| {
                                    rt.nodes[id.index()].on_click =
                                        Some(Box::new(move || ex2.set(false)));
                                });
                                with_content_scope(id, move || {
                                    Text(
                                        lb.clone(),
                                        tokens::label_large(colors.on_secondary_container),
                                        Modifier::new(),
                                    );
                                    Spacer(Modifier::new().width(tokens::SPACE_SM));
                                    icon(
                                        ic,
                                        tokens::ICON_SMALL,
                                        colors.on_secondary_container,
                                        Modifier::new(),
                                    );
                                });
                            }
                        },
                    );
                },
            );
            let ex3 = ex;
            Fab(name, Modifier::new(), move || ex3.update(|v| *v = !*v));
        },
    );
}

// ══════════════════════════════ Chip ══════════════════════════════

/// M3 `Chip` 内核：32dp 高、`shapeSmall` 圆角、18dp 前导图标 + 8dp 间隙、`labelLarge`
/// 文字、1dp 描边（可选）。悬停叠 `on_surface` 8% 状态层 —— 不改容器色相。
fn chip_core(
    label: impl Into<String>,
    leading: Option<IconName>,
    trailing: Option<IconName>,
    bg: Color,
    border: Option<Color>,
    fg: Color,
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
) {
    let colors = theme_colors();
    let label = label.into();
    let hovered = compose::runtime::remember(|| compose::runtime::mutableStateOf(false));
    let hov = hovered.get();
    let has_icon = leading.is_some() || trailing.is_some();
    // M3：带图标时贴边 12dp，纯文字 16dp；垂直居中由 min_height 保证
    let pad_h = if has_icon { 12.0 } else { tokens::CHIP_PAD_H };
    let r = tokens::CHIP_RADIUS;

    let m = GlassOrBg(modifier, glass_spec(), bg, r)
        .padding_ltrb(
            pad_h,
            tokens::SPACE_XXS,
            if trailing.is_some() { 12.0 } else { pad_h },
            tokens::SPACE_XXS,
        )
        .min_height(tokens::CHIP_HEIGHT);
    let m = match border {
        Some(bc) => m.border(1.0, bc, r),
        None => m,
    };

    let id = add_node(
        Policy::Row {
            arrangement: Arrangement::Center,
            cross: CrossAxisAlignment::Center,
        },
        m,
    );
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[id.index()];
        n.on_click = Some(Box::new(on_click));
        n.hover_handler = Some(Box::new(move |enter| hovered.set(enter)));
        if hov {
            let lc = colors.on_surface.with_alpha(tokens::STATE_HOVER);
            n.draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                b.fill_rrect(x, y, w, h, r, lc);
            }));
        }
    });
    with_content_scope(id, move || {
        if let Some(n) = leading {
            icon(n, tokens::CHIP_ICON, fg, Modifier::new());
            Spacer(Modifier::new().width(8.0));
        }
        Text(label.clone(), tokens::label_large(fg), Modifier::new());
        if let Some(n) = trailing {
            Spacer(Modifier::new().width(4.0));
            icon(n, tokens::CHIP_ICON, fg, Modifier::new());
        }
    });
}

/// `Chip(label, leading, selected, onClick)` — M3 `AssistChip` 形态。
pub fn Chip(
    label: impl Into<String>,
    leading: Option<IconName>,
    selected: bool,
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
) {
    let colors = theme_colors();
    if selected {
        chip_core(
            label,
            leading,
            None,
            colors.secondary_container,
            None,
            colors.on_secondary_container,
            modifier,
            on_click,
        );
    } else {
        chip_core(
            label,
            leading,
            None,
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            Some(colors.outline),
            colors.on_surface_variant,
            modifier,
            on_click,
        );
    }
}

/// `FilterChip(label, checked)` — M3：选中态换 `secondary_container` 并显示 ✓ 前导图标。
pub fn FilterChip(label: impl Into<String>, checked: MutableState<bool>, modifier: Modifier) {
    let on = checked.get();
    let st = checked;
    let colors = theme_colors();
    let label = label.into();
    if on {
        chip_core(
            label,
            Some(IconName::Check),
            None,
            colors.secondary_container,
            None,
            colors.on_secondary_container,
            modifier,
            move || {
                st.update(|v| *v = !*v);
            },
        );
    } else {
        chip_core(
            label,
            None,
            None,
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            Some(colors.outline),
            colors.on_surface_variant,
            modifier,
            move || {
                st.update(|v| *v = !*v);
            },
        );
    }
}

/// `AssistChip(label, icon, onClick)` — M3：图标与文字同为 `primary`，描边 `outline`。
pub fn AssistChip(
    label: impl Into<String>,
    icon_name: IconName,
    modifier: Modifier,
    on_click: impl FnMut() + 'static,
) {
    let colors = theme_colors();
    chip_core(
        label,
        Some(icon_name),
        None,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        Some(colors.outline),
        colors.primary,
        modifier,
        on_click,
    );
}

// ══════════════════════════════ 头像 / 徽标 ══════════════════════════════

/// 在 (x, y, w, h) 区域内居中绘制文本。
pub fn CenteredText(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    txt: &str,
    style: &TextStyle,
) {
    let (tw, th) = measure_text_with_measurer(txt, style);
    b.draw_text(
        x + (w - tw) * 0.5,
        y + (h - th) * 0.5,
        txt,
        style,
        f32::INFINITY,
    );
}

/// `Avatar(initials, size)` — 圆形头像（首字母，M3 用 `labelLarge` 文字）。
pub fn Avatar(initials: impl Into<String>, size: f32, modifier: Modifier) {
    let colors = theme_colors();
    let initials = initials.into();
    let style = tokens::label_large(colors.on_primary_container);
    let id = add_node(Policy::Leaf, modifier.size(size, size));
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            b.fill_circle(x + w * 0.5, y + h * 0.5, w * 0.5, colors.primary_container);
            CenteredText(b, x, y, w, h, &initials, &style);
        }));
    });
}

/// `AvatarGroup(initials[], size)` — 叠放头像组。
pub fn AvatarGroup(initials: &[String], size: f32, modifier: Modifier) {
    let colors = theme_colors();
    let items: Vec<String> = initials.to_vec();
    let count = items.len().min(4);
    let overlap = size * 0.35;
    let total = size + (count.saturating_sub(1)) as f32 * (size - overlap);
    let id = add_node(Policy::Leaf, modifier.size(total, size));
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, h| {
            for i in 0..count {
                let cxp = x + size * 0.5 + i as f32 * (size - overlap);
                b.fill_circle(cxp, y + h * 0.5, size * 0.5, colors.Surface);
                b.fill_circle(cxp, y + h * 0.5, size * 0.46, colors.primary_container);
            }
        }));
    });
    let _ = items;
}

/// `Badge(label)` — M3 大徽标：16dp 高、胶囊、`error` 底 + `labelSmall` 文字。
pub fn Badge(label: impl Into<String>, modifier: Modifier) {
    let colors = theme_colors();
    let label = label.into();
    let id = add_node(Policy::Leaf, modifier.height(16.0));
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, h| {
            let tw = label.chars().count() as f32 * 6.5 + 12.0;
            let tw = tw.max(16.0);
            b.fill_rrect(x, y, tw, h, tokens::full_radius(h), colors.error);
            CenteredText(
                b,
                x,
                y,
                tw,
                h,
                &label,
                &tokens::label_small(colors.on_error),
            );
        }));
    });
}

/// `BadgedBox(Badge, modifier) { content }` — 右上角挂徽标。
pub fn BadgedBox(label: impl Into<String>, modifier: Modifier, content: impl Fn() + 'static) {
    let label = label.into();
    Box(modifier, Alignment::TopStart, move || {
        content();
        Badge(label.clone(), Modifier::new().align(Alignment::TopEnd));
    });
}

/// 状态圆点。
pub fn StatusDot(color: compose::foundation::Color, modifier: Modifier) {
    add_node(Policy::Leaf, modifier.size(8.0, 8.0).background(color, 4.0));
}

// ══════════════════════════════ 其它基础控件 ══════════════════════════════

/// `Link(label, onClick)` — 可点击链接文本（`bodyMedium` + 下划线）。
pub fn Link(label: impl Into<String>, modifier: Modifier, on_click: impl FnMut() + 'static) {
    let colors = theme_colors();
    let label = label.into();
    let id = add_node(Policy::Leaf, modifier);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(on_click));
        let style = tokens::body_medium(colors.primary);
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, _h| {
            b.draw_text(x, y, &label, &style, f32::INFINITY);
            let (tw, th) = measure_text_with_measurer(&label, &style);
            let uy = y + th - 4.0;
            b.line(x, uy, x + tw.min(w), uy, colors.primary, 1.0);
        }));
    });
}

/// `Kbd(keys)` — 键帽（快捷键展示）。
pub fn Kbd(keys: &[String], modifier: Modifier) {
    let colors = theme_colors();
    let ks: Vec<String> = keys.to_vec();
    Row(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for (i, k) in ks.clone().into_iter().enumerate() {
                if i > 0 {
                    Spacer(Modifier::new().width(tokens::SPACE_XS));
                }
                let id = add_node(Policy::Leaf, Modifier::new().height(22.0));
                with_rt_pub(|rt| {
                    let c2 = colors;
                    rt.nodes[id.index()].draw =
                        Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, h| {
                            let tw = k.chars().count() as f32 * 7.0 + 12.0;
                            b.fill_rrect(
                                x,
                                y,
                                tw,
                                h,
                                tokens::SHAPE_EXTRA_SMALL,
                                c2.surface_variant,
                            );
                            b.stroke_rrect(
                                x,
                                y,
                                tw,
                                h,
                                tokens::SHAPE_EXTRA_SMALL,
                                c2.outline.with_alpha(0.6),
                                1.0,
                            );
                            CenteredText(
                                b,
                                x,
                                y,
                                tw,
                                h,
                                &k,
                                &tokens::label_small(c2.on_surface_variant),
                            );
                        }));
                });
            }
        },
    );
}

/// `GroupBox(title) { content }` — 带标题的分组框（M3：`outline_variant` + `titleSmall`）。
pub fn GroupBox(title: impl Into<String>, modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    let g = glass_spec();
    let title = title.into();
    let m = GlassOrBg(modifier, g, colors.Surface, tokens::SHAPE_MEDIUM)
        .border(1.0, colors.outline_variant, tokens::SHAPE_MEDIUM)
        .padding_hv(tokens::SPACE_LG, tokens::SPACE_MD);
    let id = add_node(
        Policy::Column {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Start,
        },
        m,
    );
    with_content_scope(id, move || {
        Text(
            title.clone(),
            tokens::title_small(colors.on_surface_variant),
            Modifier::new().padding_ltrb(0.0, 0.0, 0.0, tokens::SPACE_SM),
        );
        content();
    });
}

/// `SearchBar(query, placeholder)` — M3 `SearchBar`：56dp 高、全圆角、
/// `surface_container` 底、24dp 前导图标、`bodyLarge` 输入。
pub fn SearchBar(query: MutableState<String>, placeholder: &str, modifier: Modifier) {
    let colors = theme_colors();
    let g = glass_spec();
    let ph = placeholder.to_string();
    let h = tokens::FIELD_HEIGHT;
    let m = GlassOrBg(
        modifier,
        g,
        colors.surface_container,
        tokens::full_radius(h),
    )
    .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0)
    .height(h)
    .min_width(240.0);
    Row(
        m,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            icon(
                IconName::Search,
                tokens::ICON_DEFAULT,
                colors.on_surface_variant,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(tokens::SPACE_SM));
            crate::widgets::input::BareTextField(
                query,
                &ph,
                // fill=true：收紧到权重份额，输入框占满剩余宽度，清空按钮才能靠到最右
                Modifier::new()
                    .weight(1.0, true)
                    .height(h - tokens::SPACE_LG),
            );
            let q = query;
            IconButton(IconName::Close, Modifier::new(), move || {
                q.set(String::new());
            });
        },
    );
}

/// 图片占位（无解码器时的可视替身）。
pub fn ImagePlaceholder(label: impl Into<String>, modifier: Modifier) {
    let colors = theme_colors();
    let label = label.into();
    let id = add_node(Policy::Leaf, modifier.fill_max_width().height(140.0));
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            b.fill_rect(x, y, w, h, colors.surface_variant);
            b.stroke_rrect(x, y, w, h, 0.0, colors.outline.with_alpha(0.5), 1.0);
            let glyph = tokens::PLACEHOLDER_ICON;
            draw_icon(
                b,
                IconName::Image,
                x + w * 0.5 - glyph * 0.5,
                y + h * 0.5 - glyph * 0.5 - tokens::SPACE_LG,
                glyph,
                colors.on_surface_variant,
            );
            CenteredText(
                b,
                x,
                y + h * 0.5,
                w,
                h * 0.5,
                &label,
                &tokens::label_medium(colors.on_surface_variant),
            );
        }));
    });
}
