//! Material 3 主题与图标原语 — 控件本体在 `compose-material` crate。
//!
//! 本模块只保留跨平台共用的 **主题系统**（`ColorScheme` / `MaterialTheme` /
//! `theme_colors`）与 **图标原语**（`IconName` / `draw_icon` / `icon`）。所有
//! composable 控件已迁至 `compose-material`。

use std::cell::Cell;

use crate::foundation::Color;
use crate::layout::{Canvas, DrawBuilder, GlassSpec, Modifier};
use crate::runtime::{MutableState, mutableStateOf};

// ══════════════════════════════ 主题 ══════════════════════════════

/// 颜色方案 — Compose `ColorScheme`（Material 3 精简，覆盖全部控件取色面）。
#[derive(Clone, Copy, Debug)]
pub struct ColorScheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub Surface: Color,
    pub on_surface: Color,
    pub surface_variant: Color,
    pub on_surface_variant: Color,
    pub surface_container: Color,
    /// M3 五档容器色（Flutter `surfaceContainer{Lowest,Low,Base,High,Highest}`）。
    /// `surface_container` = 基准档；卡片/浮层按 elevation 取对应档，不再互相近似。
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    /// 表面极值两档（导航栏底 / 顶部高亮）。
    pub surface_dim: Color,
    pub surface_bright: Color,
    /// 反色三件套（Snackbar / Tooltip 用 `inverseSurface`，其动作文字用 `inversePrimary`）。
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub inverse_primary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    /// 阴影色（M3 `elevationOverlay` 的着色基准）。
    pub shadow: Color,
    pub background: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub scrim: Color,
}

impl ColorScheme {
    pub fn light() -> Self {
        Self {
            primary: Color::hex(0x6750A4),
            on_primary: Color::WHITE,
            primary_container: Color::hex(0xEADDFF),
            on_primary_container: Color::hex(0x21005D),
            secondary: Color::hex(0x625B71),
            on_secondary: Color::WHITE,
            secondary_container: Color::hex(0xE8DEF8),
            on_secondary_container: Color::hex(0x1D192B),
            tertiary: Color::hex(0x7D5260),
            on_tertiary: Color::WHITE,
            Surface: Color::hex(0xFEFBFF),
            on_surface: Color::hex(0x1C1B1F),
            surface_variant: Color::hex(0xE7E0EC),
            on_surface_variant: Color::hex(0x49454F),
            surface_container: Color::hex(0xF3EDF7),
            surface_container_lowest: Color::hex(0xFFFFFF),
            surface_container_low: Color::hex(0xF7F2FA),
            surface_container_high: Color::hex(0xECE6F0),
            surface_container_highest: Color::hex(0xE6E0E9),
            surface_dim: Color::hex(0xDED8E1),
            surface_bright: Color::hex(0xF8F5F9),
            inverse_surface: Color::hex(0x322F35),
            inverse_on_surface: Color::hex(0xF4EFF4),
            inverse_primary: Color::hex(0xD0BCFF),
            tertiary_container: Color::hex(0xFFD8E4),
            on_tertiary_container: Color::hex(0x31111D),
            shadow: Color::hex(0x000000),
            background: Color::hex(0xFEFBFF),
            outline: Color::hex(0x79747E),
            outline_variant: Color::hex(0xCAC4D0),
            error: Color::hex(0xB3261E),
            on_error: Color::WHITE,
            error_container: Color::hex(0xF9DEDC),
            on_error_container: Color::hex(0x410E0B),
            scrim: Color::rgba(0.0, 0.0, 0.0, 0.32),
        }
    }
    pub fn dark() -> Self {
        Self {
            primary: Color::hex(0xD0BCFF),
            on_primary: Color::hex(0x381E72),
            primary_container: Color::hex(0x4F378B),
            on_primary_container: Color::hex(0xEADDFF),
            secondary: Color::hex(0xCCC2DC),
            on_secondary: Color::hex(0x332D41),
            secondary_container: Color::hex(0x4A4458),
            on_secondary_container: Color::hex(0xE8DEF8),
            tertiary: Color::hex(0xEFB8C8),
            on_tertiary: Color::hex(0x492532),
            Surface: Color::hex(0x1C1B1F),
            on_surface: Color::hex(0xE6E1E5),
            surface_variant: Color::hex(0x49454F),
            on_surface_variant: Color::hex(0xCAC4D0),
            surface_container: Color::hex(0x211F26),
            surface_container_lowest: Color::hex(0x0F0D13),
            surface_container_low: Color::hex(0x1D1B20),
            surface_container_high: Color::hex(0x2B2930),
            surface_container_highest: Color::hex(0x36343B),
            surface_dim: Color::hex(0x141218),
            surface_bright: Color::hex(0x3D3A41),
            inverse_surface: Color::hex(0xE6E1E5),
            inverse_on_surface: Color::hex(0x322F35),
            inverse_primary: Color::hex(0x6750A4),
            tertiary_container: Color::hex(0x633B48),
            on_tertiary_container: Color::hex(0xFFD8E4),
            shadow: Color::hex(0x000000),
            background: Color::hex(0x1C1B1F),
            outline: Color::hex(0x938F99),
            outline_variant: Color::hex(0x49454F),
            error: Color::hex(0xF2B8B5),
            on_error: Color::hex(0x601410),
            error_container: Color::hex(0x8C1D18),
            on_error_container: Color::hex(0xF9DEDC),
            // M3 的 barrier  scrim 统一 32% 黑（不随主题加深，否则暗色下内容几乎不可见）。
            scrim: Color::rgba(0.0, 0.0, 0.0, 0.32),
        }
    }
}

impl ColorScheme {
    /// 暗色主题判定（按 Surface 亮度，玻璃材质据此取高光方向）。
    pub fn is_dark(&self) -> bool {
        (self.Surface.r + self.Surface.g + self.Surface.b) < 1.5
    }

    /// 从种子色生成完整 M3 色板（品牌色替换 + 中性面带极轻色调）。
    ///
    /// 不是完整的 HCT 色调映射（不引入算法依赖），但保持 M3 的层级关系：
    /// 容器色 = 种子色与白/黑按固定比例混合，表面色只吃 3–5% 色调，
    /// 保证任何种子色都不会破坏正文对比度。
    pub fn from_seed(seed: Color, dark: bool) -> Self {
        let base = if dark { Self::dark() } else { Self::light() };
        let white = Color::WHITE;
        let black = Color::BLACK;
        let desat = |c: Color, amount: f32| c.mix(Color::rgb(c.luminance(), c.luminance(), c.luminance()), amount);
        let hue_turn = |c: Color| Color::rgb(c.g, c.b, c.r);

        let (primary, on_primary, primary_container, on_primary_container) = if dark {
            (
                seed.mix(white, 0.62),
                seed.mix(black, 0.72),
                seed.mix(black, 0.48),
                seed.mix(white, 0.84),
            )
        } else {
            (
                seed,
                if seed.luminance() > 0.62 { black } else { white },
                seed.mix(white, 0.82),
                seed.mix(black, 0.60),
            )
        };
        let secondary = desat(seed, 0.62).mix(if dark { white } else { black }, if dark { 0.42 } else { 0.12 });
        let secondary_container = if dark {
            seed.mix(black, 0.52)
        } else {
            seed.mix(white, 0.86)
        };
        let tertiary = hue_turn(seed).mix(seed, 0.35).mix(base.tertiary, 0.22);
        let tertiary = if dark { tertiary.mix(white, 0.45) } else { tertiary.mix(black, 0.18) };
        let tertiary_container = if dark {
            tertiary.mix(black, 0.52)
        } else {
            tertiary.mix(white, 0.84)
        };
        let tint = |c: Color, amount: f32| c.mix(seed, amount);

        Self {
            primary,
            on_primary,
            primary_container,
            on_primary_container,
            secondary,
            on_secondary: if dark { seed.mix(black, 0.72) } else { white },
            secondary_container,
            on_secondary_container: if dark { seed.mix(white, 0.82) } else { seed.mix(black, 0.55) },
            tertiary,
            on_tertiary: if dark { black.mix(seed, 0.2) } else { white },
            tertiary_container,
            on_tertiary_container: if dark { tertiary.mix(white, 0.8) } else { tertiary.mix(black, 0.55) },
            Surface: tint(base.Surface, if dark { 0.04 } else { 0.02 }),
            surface_container_lowest: tint(base.surface_container_lowest, if dark { 0.04 } else { 0.01 }),
            surface_container_low: tint(base.surface_container_low, if dark { 0.05 } else { 0.025 }),
            surface_container: tint(base.surface_container, if dark { 0.06 } else { 0.03 }),
            surface_container_high: tint(base.surface_container_high, if dark { 0.07 } else { 0.035 }),
            surface_container_highest: tint(base.surface_container_highest, if dark { 0.08 } else { 0.04 }),
            surface_dim: tint(base.surface_dim, 0.04),
            surface_bright: tint(base.surface_bright, 0.02),
            surface_variant: tint(base.surface_variant, if dark { 0.10 } else { 0.07 }),
            outline: tint(base.outline, 0.10),
            outline_variant: tint(base.outline_variant, 0.08),
            background: tint(base.background, if dark { 0.04 } else { 0.02 }),
            ..base
        }
    }
}

/// 一套命名色板（种子色，明暗各生成一份）。
#[derive(Clone, Copy, Debug)]
pub struct ThemePreset {
    pub name: &'static str,
    pub seed: Color,
}

/// Story / 设置面板共用的内置色板。
pub const THEME_PRESETS: &[ThemePreset] = &[
    ThemePreset { name: "Violet", seed: Color::hex(0x6750A4) },
    ThemePreset { name: "Ocean", seed: Color::hex(0x00639B) },
    ThemePreset { name: "Forest", seed: Color::hex(0x3E6837) },
    ThemePreset { name: "Amber", seed: Color::hex(0x8B5000) },
    ThemePreset { name: "Rose", seed: Color::hex(0x9A4058) },
];

/// MaterialTheme 局部环境 — Compose `LocalMaterialTheme`。
#[derive(Clone)]
pub struct MaterialThemeData {
    pub colors: ColorScheme,
}

/// 主题 CompositionLocal。
pub struct LocalTheme;
impl LocalTheme {
    fn local() -> crate::runtime::CompositionLocal<MaterialThemeData> {
        crate::runtime::CompositionLocal::new()
    }
}

/// 当前主题色（全局线程级镜像，渲染层 `draw_node` 等无 `Composer` 处读取）。
thread_local! {
    static CURRENT_COLORS: std::cell::RefCell<Option<ColorScheme>> = const { std::cell::RefCell::new(None) };
}

fn set_current_colors_global(colors: ColorScheme) {
    CURRENT_COLORS.with(|c| *c.borrow_mut() = Some(colors));
}

/// 渲染层读取当前主题色（无 `Composer` 时用）；未设置时回退浅色。
pub fn current_colors_global() -> ColorScheme {
    CURRENT_COLORS.with(|c| {
        c.borrow()
            .as_ref()
            .map(|x| *x)
            .unwrap_or_else(ColorScheme::light)
    })
}

/// `MaterialTheme(colorScheme) { content }`。
pub fn MaterialTheme(colors: ColorScheme, content: impl Fn() + 'static) {
    set_current_colors_global(colors);
    LocalTheme::local().provides(MaterialThemeData { colors }, content);
}

/// `LocalContentColor` — Compose 同名局部环境。
///
/// 容器（按钮 / 芯片 / 列表项 / 顶栏…）为内容提供前景色，`Text` / `icon` 在调用方
/// 未显式着色时读取它。这样禁用态、反色容器（`on_primary`）等只需在容器里算一次。
pub struct LocalContentColor;
impl LocalContentColor {
    fn local() -> crate::runtime::CompositionLocal<Color> {
        crate::runtime::CompositionLocal::new()
    }

    /// 在 `content` 作用域内提供前景色。
    pub fn provides(color: Color, content: impl Fn() + 'static) {
        Self::local().provides(color, content);
    }

    /// 内容作用域已绑定到容器节点 `node` 时下发前景色（控件容器内部用）。
    pub fn provides_in(node: crate::layout::NodeId, color: Color, content: impl Fn() + 'static) {
        Self::local().provides_in(node, color, content);
    }

    /// 当前前景色；未提供时回退主题 `on_surface`。
    pub fn current() -> Color {
        Self::local()
            .try_current()
            .unwrap_or_else(|| theme_colors().on_surface)
    }
}

/// 读取当前主题色。
///
/// 组合内走 `CompositionLocal`（含嵌套 `MaterialTheme` 覆盖）；**组合外的回调**
/// （点击 / 悬停 / 绘制 / 输入路由）没有作用域栈，`CompositionLocal::current()` 会
/// panic `CompositionLocal not provided`。此时回退到 `MaterialTheme` 每趟组合刷新的
/// 全局镜像，语义合理且不会崩。
pub fn theme_colors() -> ColorScheme {
    LocalTheme::local()
        .try_current()
        .map(|d| d.colors.clone())
        .unwrap_or_else(current_colors_global)
}

// ══════════════════════════════ 图标 ══════════════════════════════

/// 图标名（矢量字形，纯 DrawBuilder 基元，覆盖全部控件）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IconName {
    // 文件 / 文档
    Folder,
    FolderOpen,
    File,
    Note,
    Book,
    // 媒体控制
    Play,
    Pause,
    Volume,
    // 设置
    Settings,
    // 通信
    Mail,
    Send,
    Bell,
    Phone,
    // 数据结构
    Grid,
    Table,
    List,
    Tree,
    Layers,
    Database,
    // 内容形态
    Type,
    Code,
    Terminal,
    Brush,
    Palette,
    // 图表
    Chart,
    ChartLine,
    ChartPie,
    // 布局
    Layout,
    // 偏好
    Star,
    Heart,
    Bookmark,
    Flag,
    ThumbUp,
    // 方向
    ChevronRight,
    ChevronDown,
    ChevronLeft,
    ChevronUp,
    ExpandMore,
    ExpandLess,
    // 菜单
    Menu,
    MoreVert,
    // 设备 / 状态
    Sun,
    Moon,
    Cloud,
    Wifi,
    Battery,
    Power,
    // 常用操作
    Check,
    Close,
    Add,
    Remove,
    Edit,
    Delete,
    Copy,
    Save,
    Search,
    Filter,
    Refresh,
    // 用户 / 安全
    Home,
    User,
    Users,
    Lock,
    Shield,
    Eye,
    EyeOff,
    // 时间
    Calendar,
    Clock,
    // 语义反馈
    Info,
    Warning,
    Error,
    Bug,
    // 传输 / 分享
    Download,
    Upload,
    Share,
    Link,
    Tag,
    // 媒体类型
    Image,
    Music,
    Video,
    Camera,
    Mic,
    // 其它
    ZoomIn,
    ZoomOut,
    Pin,
    Rocket,
    Cart,
}

/// 归一化坐标 → 绝对点。
fn pt(x: f32, y: f32, s: f32, ux: f32, uy: f32) -> (f32, f32) {
    (x + ux * s, y + uy * s)
}

/// 圆弧采样（角度弧度制，y 轴向下）。
fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let a = a0 + (a1 - a0) * (i as f32 / n as f32);
        out.push((cx + a.cos() * r, cy + a.sin() * r));
    }
    out
}

/// 三条点的折线（箭头 / 勾选 / V 形）。
fn stroke3(b: &mut DrawBuilder, pts: &[(f32, f32); 3], color: Color, line: f32) {
    b.stroke_path(pts, color, line);
}

/// 在 (x, y) 为左上角、边长 s 的方框内绘制图标（矢量字形，纯 DrawBuilder 基元）。
pub fn draw_icon(b: &mut DrawBuilder, name: IconName, x: f32, y: f32, s: f32, color: Color) {
    let cx = x + s / 2.0;
    let cy = y + s / 2.0;
    let lw = (s * 0.09).max(1.2);
    let p = |ux: f32, uy: f32| pt(x, y, s, ux, uy);
    let pi = std::f32::consts::PI;
    match name {
        IconName::Folder => {
            b.fill_rrect(
                x + s * 0.08,
                y + s * 0.32,
                s * 0.84,
                s * 0.46,
                s * 0.06,
                color,
            );
            b.fill_rrect(
                x + s * 0.08,
                y + s * 0.20,
                s * 0.34,
                s * 0.16,
                s * 0.04,
                color,
            );
        }
        IconName::FolderOpen => {
            b.fill_path(
                &[
                    p(0.10, 0.34),
                    p(0.44, 0.34),
                    p(0.54, 0.46),
                    p(0.90, 0.46),
                    p(0.90, 0.80),
                    p(0.10, 0.80),
                ],
                color,
            );
            b.fill_path(
                &[p(0.10, 0.22), p(0.36, 0.22), p(0.46, 0.34), p(0.14, 0.34)],
                color,
            );
        }
        IconName::File => {
            let w = s * 0.7;
            let h = s * 0.82;
            let fx = x + s * 0.16;
            let fy = y + s * 0.09;
            b.fill_rrect(fx, fy, w, h, s * 0.06, color);
            b.line(fx + w - s * 0.22, fy, fx + w, fy + s * 0.22, color, lw);
            b.line(fx + w, fy + s * 0.22, fx + w, fy, color, lw);
        }
        IconName::Note => {
            b.stroke_rrect(
                x + s * 0.18,
                y + s * 0.12,
                s * 0.64,
                s * 0.76,
                s * 0.05,
                color,
                lw,
            );
            for i in 0..3 {
                let ly = 0.32 + i as f32 * 0.18;
                b.line(
                    p(0.30, ly).0,
                    p(0.30, ly).1,
                    p(0.70, ly).0,
                    p(0.70, ly).1,
                    color,
                    lw,
                );
            }
        }
        IconName::Book => {
            b.fill_rrect(
                x + s * 0.14,
                y + s * 0.18,
                s * 0.34,
                s * 0.62,
                s * 0.04,
                color,
            );
            b.fill_rrect(
                x + s * 0.52,
                y + s * 0.18,
                s * 0.34,
                s * 0.62,
                s * 0.04,
                color,
            );
            b.fill_rect(x + s * 0.47, y + s * 0.18, s * 0.06, s * 0.62, color);
        }
        IconName::Play => {
            let r = s * 0.32;
            let pts = [
                (cx - r * 0.8, cy - r),
                (cx + r, cy),
                (cx - r * 0.8, cy + r),
                (cx - r * 0.8, cy - r),
            ];
            draw_poly(b, &pts, color);
        }
        IconName::Pause => {
            b.fill_rrect(
                x + s * 0.26,
                y + s * 0.20,
                s * 0.16,
                s * 0.60,
                s * 0.03,
                color,
            );
            b.fill_rrect(
                x + s * 0.58,
                y + s * 0.20,
                s * 0.16,
                s * 0.60,
                s * 0.03,
                color,
            );
        }
        IconName::Volume => {
            b.fill_path(
                &[
                    p(0.14, 0.38),
                    p(0.30, 0.38),
                    p(0.46, 0.24),
                    p(0.46, 0.76),
                    p(0.30, 0.62),
                    p(0.14, 0.62),
                ],
                color,
            );
            b.stroke_path(&arc(x + s * 0.52, cy, s * 0.15, -1.0, 1.0, 8), color, lw);
            b.stroke_path(&arc(x + s * 0.52, cy, s * 0.27, -1.0, 1.0, 8), color, lw);
        }
        IconName::Settings => {
            b.fill_circle(cx, cy, s * 0.22, color);
            for a in 0..8 {
                let ang = a as f32 * pi / 4.0;
                let r0 = s * 0.30;
                let r1 = s * 0.42;
                b.line(
                    cx + ang.cos() * r0,
                    cy + ang.sin() * r0,
                    cx + ang.cos() * r1,
                    cy + ang.sin() * r1,
                    color,
                    lw,
                );
            }
        }
        IconName::Mail => {
            let w = s * 0.78;
            let h = s * 0.56;
            let mx = x + s * 0.11;
            let my = y + s * 0.22;
            b.stroke_rrect(mx, my, w, h, s * 0.05, color, lw);
            b.line(mx, my, cx, my + h * 0.55, color, lw);
            b.line(mx + w, my, cx, my + h * 0.55, color, lw);
        }
        IconName::Send => {
            b.fill_path(&[p(0.12, 0.18), p(0.88, 0.50), p(0.12, 0.82)], color);
            b.stroke_path(&[p(0.12, 0.18), p(0.88, 0.50), p(0.12, 0.82)], color, lw);
        }
        IconName::Bell => {
            b.fill_path(&[p(0.50, 0.14), p(0.78, 0.62), p(0.22, 0.62)], color);
            b.fill_rrect(
                x + s * 0.34,
                y + s * 0.60,
                s * 0.32,
                s * 0.12,
                s * 0.04,
                color,
            );
            b.fill_circle(cx, y + s * 0.80, s * 0.06, color);
        }
        IconName::Phone => {
            b.stroke_path(
                &[p(0.30, 0.14), p(0.44, 0.14), p(0.70, 0.60), p(0.70, 0.78)],
                color,
                lw * 2.0,
            );
            b.stroke_path(
                &[p(0.70, 0.22), p(0.70, 0.40), p(0.44, 0.86), p(0.30, 0.86)],
                color,
                lw * 2.0,
            );
        }
        IconName::Grid => {
            let g = s * 0.30;
            let gap = s * 0.06;
            for r in 0..2 {
                for c in 0..2 {
                    let gx = x + s * 0.14 + c as f32 * (g + gap);
                    let gy = y + s * 0.14 + r as f32 * (g + gap);
                    b.fill_rrect(gx, gy, g, g, s * 0.05, color);
                }
            }
        }
        IconName::Table => {
            b.stroke_rrect(
                x + s * 0.10,
                y + s * 0.16,
                s * 0.80,
                s * 0.68,
                s * 0.05,
                color,
                lw,
            );
            b.line(
                p(0.10, 0.38).0,
                p(0.10, 0.38).1,
                p(0.90, 0.38).0,
                p(0.90, 0.38).1,
                color,
                lw,
            );
            b.line(
                p(0.42, 0.38).0,
                p(0.42, 0.38).1,
                p(0.42, 0.84).0,
                p(0.42, 0.84).1,
                color,
                lw,
            );
        }
        IconName::List => {
            for i in 0..3 {
                let oy = i as f32 * 0.24;
                b.fill_circle(x + s * 0.20, y + s * (0.28 + oy), s * 0.055, color);
                b.fill_rrect(
                    x + s * 0.34,
                    y + s * (0.23 + oy),
                    s * 0.48,
                    s * 0.10,
                    s * 0.03,
                    color,
                );
            }
        }
        IconName::Tree => {
            for i in 0..2 {
                let oy = i as f32 * 0.28;
                b.fill_rrect(
                    x + s * 0.14,
                    y + s * (0.18 + oy),
                    s * 0.14,
                    s * 0.10,
                    s * 0.03,
                    color,
                );
            }
            b.fill_rrect(
                x + s * 0.34,
                y + s * 0.74,
                s * 0.14,
                s * 0.10,
                s * 0.03,
                color,
            );
            b.line(
                x + s * 0.21,
                y + s * 0.28,
                x + s * 0.21,
                y + s * 0.74,
                color,
                lw,
            );
            b.line(
                x + s * 0.21,
                y + s * 0.50,
                x + s * 0.34,
                y + s * 0.50,
                color,
                lw,
            );
            b.line(
                x + s * 0.41,
                y + s * 0.50,
                x + s * 0.41,
                y + s * 0.74,
                color,
                lw,
            );
        }
        IconName::Layers => {
            for i in 0..3u32 {
                let dy = 0.62 - i as f32 * 0.18;
                b.stroke_path(
                    &[
                        p(0.12, dy),
                        p(0.50, dy - 0.15),
                        p(0.88, dy),
                        p(0.50, dy + 0.15),
                        p(0.12, dy),
                    ],
                    color,
                    lw,
                );
            }
        }
        IconName::Database => {
            for i in 0..3 {
                let oy = 0.26 + i as f32 * 0.24;
                b.stroke_path(&arc(cx, y + s * oy, s * 0.30, pi, 2.0 * pi, 10), color, lw);
                b.stroke_path(&arc(cx, y + s * oy, s * 0.30, 0.0, pi, 10), color, lw);
            }
            b.line(
                x + s * 0.20,
                y + s * 0.26,
                x + s * 0.20,
                y + s * 0.74,
                color,
                lw,
            );
            b.line(
                x + s * 0.80,
                y + s * 0.26,
                x + s * 0.80,
                y + s * 0.74,
                color,
                lw,
            );
        }
        IconName::Type => {
            let w = s * 0.6;
            let tx = cx - w / 2.0;
            b.line(tx, y + s * 0.26, tx + w, y + s * 0.26, color, lw * 1.4);
            b.line(cx, y + s * 0.26, cx, y + s * 0.74, color, lw * 1.4);
        }
        IconName::Code => {
            stroke3(
                b,
                &[p(0.32, 0.28), p(0.14, 0.50), p(0.32, 0.72)],
                color,
                lw * 1.3,
            );
            stroke3(
                b,
                &[p(0.68, 0.28), p(0.86, 0.50), p(0.68, 0.72)],
                color,
                lw * 1.3,
            );
        }
        IconName::Terminal => {
            b.stroke_rrect(
                x + s * 0.10,
                y + s * 0.16,
                s * 0.80,
                s * 0.68,
                s * 0.06,
                color,
                lw,
            );
            stroke3(
                b,
                &[p(0.26, 0.38), p(0.36, 0.50), p(0.26, 0.62)],
                color,
                lw * 1.2,
            );
            b.line(
                p(0.46, 0.62).0,
                p(0.46, 0.62).1,
                p(0.66, 0.62).0,
                p(0.66, 0.62).1,
                color,
                lw * 1.2,
            );
        }
        IconName::Brush => {
            b.fill_circle(cx, cy - s * 0.1, s * 0.18, color);
            b.line(cx, cy, cx + s * 0.18, cy + s * 0.34, color, lw * 1.6);
        }
        IconName::Palette => {
            b.stroke_circle(cx, cy, s * 0.36, color, lw);
            b.fill_circle(x + s * 0.34, y + s * 0.38, s * 0.07, color);
            b.fill_circle(x + s * 0.62, y + s * 0.34, s * 0.07, color);
            b.fill_circle(x + s * 0.42, y + s * 0.62, s * 0.07, color);
        }
        IconName::Chart => {
            let bars = [0.4, 0.7, 0.55, 0.85];
            let bw = s * 0.16;
            let gap = s * 0.06;
            for (i, h) in bars.iter().enumerate() {
                let bx = x + s * 0.12 + i as f32 * (bw + gap);
                let bh = s * 0.7 * h;
                b.fill_rrect(bx, y + s * 0.82 - bh, bw, bh, s * 0.02, color);
            }
        }
        IconName::ChartLine => {
            b.line(
                x + s * 0.14,
                y + s * 0.86,
                x + s * 0.86,
                y + s * 0.86,
                color,
                lw,
            );
            b.line(
                x + s * 0.14,
                y + s * 0.14,
                x + s * 0.14,
                y + s * 0.86,
                color,
                lw,
            );
            let pts = [p(0.20, 0.70), p(0.40, 0.46), p(0.60, 0.58), p(0.84, 0.26)];
            b.stroke_path(&pts, color, lw * 1.3);
        }
        IconName::ChartPie => {
            let mut pts = vec![(cx, cy)];
            for i in 0..=14 {
                let a = -pi / 2.0 + (i as f32 / 14.0) * pi * 1.25;
                pts.push((cx + a.cos() * s * 0.36, cy + a.sin() * s * 0.36));
            }
            b.fill_path(&pts, color);
            let mut pts2 = vec![(cx, cy)];
            for i in 0..=14 {
                let a = -pi / 2.0 + pi * 1.35 + (i as f32 / 14.0) * pi * 0.55;
                pts2.push((cx + a.cos() * s * 0.36, cy + a.sin() * s * 0.36));
            }
            b.fill_path(&pts2, color.with_alpha(0.45));
        }
        IconName::Layout => {
            let colw = s * 0.30;
            let gap = s * 0.08;
            let top = y + s * 0.18;
            let h = s * 0.64;
            b.fill_rrect(x + s * 0.14, top, colw, h, s * 0.04, color);
            b.fill_rrect(x + s * 0.14 + colw + gap, top, s * 0.44, h, s * 0.04, color);
        }
        IconName::Star => {
            let r = s * 0.4;
            let mut pts = Vec::with_capacity(10);
            for i in 0..10 {
                let rr = if i % 2 == 0 { r } else { r * 0.45 };
                let ang = -pi / 2.0 + i as f32 * pi / 5.0;
                pts.push((cx + ang.cos() * rr, cy + ang.sin() * rr));
            }
            pts.push(pts[0]);
            draw_poly(b, &pts, color);
        }
        IconName::Heart => {
            let mut pts = Vec::with_capacity(26);
            for i in 0..=24 {
                let t = i as f32 / 24.0 * 2.0 * pi;
                let hx = 16.0 * t.sin().powi(3);
                let hy = -(13.0 * t.cos()
                    - 5.0 * (2.0 * t).cos()
                    - 2.0 * (3.0 * t).cos()
                    - (4.0 * t).cos());
                pts.push((cx + hx * s / 34.0, cy + hy * s / 34.0));
            }
            b.fill_path(&pts, color);
        }
        IconName::Bookmark => {
            b.fill_path(
                &[
                    p(0.28, 0.14),
                    p(0.72, 0.14),
                    p(0.72, 0.86),
                    p(0.50, 0.62),
                    p(0.28, 0.86),
                ],
                color,
            );
        }
        IconName::Flag => {
            b.fill_rect(x + s * 0.22, y + s * 0.12, s * 0.06, s * 0.76, color);
            b.fill_path(&[p(0.28, 0.16), p(0.84, 0.28), p(0.28, 0.42)], color);
        }
        IconName::ThumbUp => {
            b.fill_rrect(
                x + s * 0.16,
                y + s * 0.42,
                s * 0.52,
                s * 0.34,
                s * 0.05,
                color,
            );
            b.fill_path(
                &[
                    p(0.68, 0.42),
                    p(0.86, 0.18),
                    p(0.84, 0.44),
                    p(0.86, 0.44),
                    p(0.86, 0.62),
                    p(0.68, 0.62),
                ],
                color,
            );
        }
        IconName::ChevronRight => {
            stroke3(
                b,
                &[p(0.34, 0.22), p(0.66, 0.50), p(0.34, 0.78)],
                color,
                lw * 1.4,
            );
        }
        IconName::ChevronDown | IconName::ExpandMore => {
            stroke3(
                b,
                &[p(0.22, 0.34), p(0.50, 0.66), p(0.78, 0.34)],
                color,
                lw * 1.4,
            );
        }
        IconName::ChevronLeft => {
            stroke3(
                b,
                &[p(0.66, 0.22), p(0.34, 0.50), p(0.66, 0.78)],
                color,
                lw * 1.4,
            );
        }
        IconName::ChevronUp | IconName::ExpandLess => {
            stroke3(
                b,
                &[p(0.22, 0.66), p(0.50, 0.34), p(0.78, 0.66)],
                color,
                lw * 1.4,
            );
        }
        IconName::Menu => {
            for i in 0..3 {
                let ly = y + s * 0.28 + i as f32 * s * 0.22;
                b.line(x + s * 0.2, ly, x + s * 0.8, ly, color, lw * 1.4);
            }
        }
        IconName::MoreVert => {
            for i in 0..3 {
                b.fill_circle(cx, y + s * (0.26 + i as f32 * 0.24), s * 0.06, color);
            }
        }
        IconName::Sun => {
            b.fill_circle(cx, cy, s * 0.20, color);
            for a in 0..8 {
                let ang = a as f32 * pi / 4.0;
                b.line(
                    cx + ang.cos() * s * 0.30,
                    cy + ang.sin() * s * 0.30,
                    cx + ang.cos() * s * 0.42,
                    cy + ang.sin() * s * 0.42,
                    color,
                    lw,
                );
            }
        }
        IconName::Moon => {
            let outer = s * 0.36;
            let inner = s * 0.30;
            let mut moon = Vec::with_capacity(28);
            for i in 0..14 {
                let ang = -pi / 2.0 + i as f32 * pi / 14.0;
                moon.push((cx + ang.cos() * outer, cy + ang.sin() * outer));
            }
            for i in 0..14 {
                let ang = pi / 2.0 - i as f32 * pi / 14.0;
                moon.push((cx + s * 0.12 + ang.cos() * inner, cy + ang.sin() * inner));
            }
            moon.push(moon[0]);
            draw_poly(b, &moon, color);
        }
        IconName::Cloud => {
            b.fill_circle(x + s * 0.36, y + s * 0.58, s * 0.16, color);
            b.fill_circle(cx, y + s * 0.44, s * 0.20, color);
            b.fill_circle(x + s * 0.66, y + s * 0.58, s * 0.16, color);
            b.fill_rect(x + s * 0.32, y + s * 0.56, s * 0.40, s * 0.16, color);
        }
        IconName::Wifi => {
            for (i, r) in [0.16f32, 0.26, 0.36].iter().enumerate() {
                let _ = i;
                b.stroke_path(
                    &arc(cx, y + s * 0.78, s * r, pi * 1.25, pi * 1.75, 10),
                    color,
                    lw * 1.2,
                );
            }
            b.fill_circle(cx, y + s * 0.80, s * 0.05, color);
        }
        IconName::Battery => {
            b.stroke_rrect(
                x + s * 0.10,
                y + s * 0.30,
                s * 0.66,
                s * 0.40,
                s * 0.06,
                color,
                lw,
            );
            b.fill_rrect(
                x + s * 0.78,
                y + s * 0.42,
                s * 0.08,
                s * 0.16,
                s * 0.03,
                color,
            );
            b.fill_rrect(
                x + s * 0.16,
                y + s * 0.36,
                s * 0.44,
                s * 0.28,
                s * 0.03,
                color,
            );
        }
        IconName::Power => {
            b.stroke_path(
                &arc(
                    cx,
                    cy + s * 0.04,
                    s * 0.26,
                    -pi / 2.0 + 0.45,
                    pi * 1.5 - 0.45,
                    16,
                ),
                color,
                lw * 1.4,
            );
            b.line(cx, y + s * 0.16, cx, cy + s * 0.04, color, lw * 1.4);
        }
        IconName::Check => {
            stroke3(
                b,
                &[p(0.18, 0.52), p(0.42, 0.76), p(0.82, 0.26)],
                color,
                lw * 1.6,
            );
        }
        IconName::Close => {
            b.line(
                p(0.22, 0.22).0,
                p(0.22, 0.22).1,
                p(0.78, 0.78).0,
                p(0.78, 0.78).1,
                color,
                lw * 1.4,
            );
            b.line(
                p(0.78, 0.22).0,
                p(0.78, 0.22).1,
                p(0.22, 0.78).0,
                p(0.22, 0.78).1,
                color,
                lw * 1.4,
            );
        }
        IconName::Add => {
            b.line(
                p(0.22, 0.50).0,
                p(0.22, 0.50).1,
                p(0.78, 0.50).0,
                p(0.78, 0.50).1,
                color,
                lw * 1.5,
            );
            b.line(
                p(0.50, 0.22).0,
                p(0.50, 0.22).1,
                p(0.50, 0.78).0,
                p(0.50, 0.78).1,
                color,
                lw * 1.5,
            );
        }
        IconName::Remove => {
            b.line(
                p(0.22, 0.50).0,
                p(0.22, 0.50).1,
                p(0.78, 0.50).0,
                p(0.78, 0.50).1,
                color,
                lw * 1.5,
            );
        }
        IconName::Edit => {
            b.stroke_path(&[p(0.24, 0.76), p(0.64, 0.36)], color, lw * 1.6);
            b.fill_path(
                &[p(0.56, 0.16), p(0.84, 0.44), p(0.68, 0.50), p(0.50, 0.32)],
                color,
            );
        }
        IconName::Delete => {
            b.fill_rrect(
                x + s * 0.36,
                y + s * 0.12,
                s * 0.28,
                s * 0.09,
                s * 0.03,
                color,
            );
            b.stroke_rrect(
                x + s * 0.22,
                y + s * 0.26,
                s * 0.56,
                s * 0.56,
                s * 0.05,
                color,
                lw,
            );
            b.line(
                x + s * 0.40,
                y + s * 0.38,
                x + s * 0.40,
                y + s * 0.70,
                color,
                lw,
            );
            b.line(
                x + s * 0.60,
                y + s * 0.38,
                x + s * 0.60,
                y + s * 0.70,
                color,
                lw,
            );
        }
        IconName::Copy => {
            b.stroke_rrect(
                x + s * 0.16,
                y + s * 0.16,
                s * 0.46,
                s * 0.46,
                s * 0.05,
                color,
                lw,
            );
            b.stroke_rrect(
                x + s * 0.38,
                y + s * 0.38,
                s * 0.46,
                s * 0.46,
                s * 0.05,
                color,
                lw,
            );
        }
        IconName::Save => {
            b.stroke_rrect(
                x + s * 0.14,
                y + s * 0.14,
                s * 0.72,
                s * 0.72,
                s * 0.06,
                color,
                lw,
            );
            b.fill_rrect(
                x + s * 0.30,
                y + s * 0.17,
                s * 0.40,
                s * 0.22,
                s * 0.03,
                color,
            );
            b.stroke_rrect(
                x + s * 0.26,
                y + s * 0.50,
                s * 0.48,
                s * 0.32,
                s * 0.03,
                color,
                lw,
            );
        }
        IconName::Search => {
            b.stroke_circle(x + s * 0.42, y + s * 0.42, s * 0.22, color, lw * 1.3);
            b.line(
                p(0.58, 0.58).0,
                p(0.58, 0.58).1,
                p(0.82, 0.82).0,
                p(0.82, 0.82).1,
                color,
                lw * 1.5,
            );
        }
        IconName::Filter => {
            b.fill_path(
                &[
                    p(0.14, 0.20),
                    p(0.86, 0.20),
                    p(0.60, 0.54),
                    p(0.60, 0.86),
                    p(0.40, 0.86),
                    p(0.40, 0.54),
                ],
                color,
            );
        }
        IconName::Refresh => {
            b.stroke_path(&arc(cx, cy, s * 0.32, -2.4, 1.7, 16), color, lw * 1.3);
            b.fill_path(&[p(0.46, 0.08), p(0.72, 0.10), p(0.58, 0.32)], color);
        }
        IconName::Home => {
            b.fill_path(
                &[
                    (cx, y + s * 0.16),
                    (x + s * 0.88, y + s * 0.48),
                    (x + s * 0.12, y + s * 0.48),
                ],
                color,
            );
            b.fill_rrect(
                x + s * 0.26,
                y + s * 0.46,
                s * 0.48,
                s * 0.36,
                s * 0.03,
                color,
            );
        }
        IconName::User => {
            b.fill_circle(cx, y + s * 0.30, s * 0.17, color);
            b.fill_rrect(
                x + s * 0.16,
                y + s * 0.52,
                s * 0.68,
                s * 0.34,
                s * 0.16,
                color,
            );
        }
        IconName::Users => {
            b.fill_circle(x + s * 0.36, y + s * 0.30, s * 0.15, color);
            b.fill_rrect(
                x + s * 0.16,
                y + s * 0.50,
                s * 0.44,
                s * 0.32,
                s * 0.12,
                color,
            );
            b.fill_circle(x + s * 0.68, y + s * 0.34, s * 0.12, color.with_alpha(0.6));
            b.fill_rrect(
                x + s * 0.52,
                y + s * 0.54,
                s * 0.34,
                s * 0.28,
                s * 0.10,
                color.with_alpha(0.6),
            );
        }
        IconName::Lock => {
            b.stroke_path(
                &arc(cx, y + s * 0.42, s * 0.18, pi, 2.0 * pi, 12),
                color,
                lw * 1.4,
            );
            b.fill_rrect(
                x + s * 0.22,
                y + s * 0.42,
                s * 0.56,
                s * 0.40,
                s * 0.07,
                color,
            );
        }
        IconName::Shield => {
            b.fill_path(
                &[
                    p(0.50, 0.10),
                    p(0.84, 0.24),
                    p(0.84, 0.50),
                    p(0.50, 0.90),
                    p(0.16, 0.50),
                    p(0.16, 0.24),
                ],
                color,
            );
        }
        IconName::Eye | IconName::EyeOff => {
            let mut lens = arc(cx, cy, s * 0.32, pi, 2.0 * pi, 12);
            lens.extend(arc(cx, cy, s * 0.32, 0.0, pi, 12));
            b.stroke_path(&lens, color, lw * 1.1);
            b.fill_circle(cx, cy, s * 0.13, color);
            if name == IconName::EyeOff {
                b.line(
                    p(0.14, 0.86).0,
                    p(0.14, 0.86).1,
                    p(0.86, 0.14).0,
                    p(0.86, 0.14).1,
                    color,
                    lw * 1.3,
                );
            }
        }
        IconName::Calendar => {
            b.stroke_rrect(
                x + s * 0.12,
                y + s * 0.18,
                s * 0.76,
                s * 0.66,
                s * 0.06,
                color,
                lw,
            );
            b.line(
                x + s * 0.12,
                y + s * 0.38,
                x + s * 0.88,
                y + s * 0.38,
                color,
                lw,
            );
            b.fill_rect(x + s * 0.24, y + s * 0.10, s * 0.08, s * 0.16, color);
            b.fill_rect(x + s * 0.68, y + s * 0.10, s * 0.08, s * 0.16, color);
            for r in 0..2 {
                for c in 0..3 {
                    b.fill_circle(
                        x + s * (0.28 + c as f32 * 0.22),
                        y + s * (0.52 + r as f32 * 0.18),
                        s * 0.05,
                        color,
                    );
                }
            }
        }
        IconName::Clock => {
            b.stroke_circle(cx, cy, s * 0.36, color, lw * 1.2);
            b.line(cx, cy, cx, y + s * 0.24, color, lw * 1.2);
            b.line(cx, cy, x + s * 0.70, cy, color, lw * 1.2);
        }
        IconName::Info => {
            b.stroke_circle(cx, cy, s * 0.38, color, lw * 1.2);
            b.fill_circle(cx, y + s * 0.30, s * 0.06, color);
            b.fill_rect(cx - s * 0.04, y + s * 0.42, s * 0.08, s * 0.26, color);
        }
        IconName::Warning => {
            b.stroke_path(
                &[p(0.12, 0.84), p(0.88, 0.84), p(0.50, 0.14), p(0.12, 0.84)],
                color,
                lw * 1.2,
            );
            b.fill_rect(cx - s * 0.045, y + s * 0.38, s * 0.09, s * 0.24, color);
            b.fill_circle(cx, y + s * 0.70, s * 0.055, color);
        }
        IconName::Error => {
            b.stroke_circle(cx, cy, s * 0.38, color, lw * 1.2);
            b.line(
                p(0.34, 0.34).0,
                p(0.34, 0.34).1,
                p(0.66, 0.66).0,
                p(0.66, 0.66).1,
                color,
                lw * 1.3,
            );
            b.line(
                p(0.66, 0.34).0,
                p(0.66, 0.34).1,
                p(0.34, 0.66).0,
                p(0.34, 0.66).1,
                color,
                lw * 1.3,
            );
        }
        IconName::Bug => {
            b.fill_rrect(
                x + s * 0.30,
                y + s * 0.26,
                s * 0.40,
                s * 0.50,
                s * 0.18,
                color,
            );
            b.fill_circle(cx, y + s * 0.22, s * 0.10, color);
            for i in 0..3 {
                let oy = 0.36 + i as f32 * 0.16;
                b.line(
                    x + s * 0.28,
                    y + s * oy,
                    x + s * 0.14,
                    y + s * (oy - 0.06),
                    color,
                    lw,
                );
                b.line(
                    x + s * 0.72,
                    y + s * oy,
                    x + s * 0.86,
                    y + s * (oy - 0.06),
                    color,
                    lw,
                );
            }
        }
        IconName::Download => {
            b.line(cx, y + s * 0.18, cx, y + s * 0.62, color, lw * 1.3);
            stroke3(
                b,
                &[p(0.30, 0.46), p(0.50, 0.66), p(0.70, 0.46)],
                color,
                lw * 1.3,
            );
            b.line(
                p(0.20, 0.80).0,
                p(0.20, 0.80).1,
                p(0.80, 0.80).0,
                p(0.80, 0.80).1,
                color,
                lw * 1.3,
            );
        }
        IconName::Upload => {
            b.line(cx, y + s * 0.66, cx, y + s * 0.22, color, lw * 1.3);
            stroke3(
                b,
                &[p(0.30, 0.42), p(0.50, 0.22), p(0.70, 0.42)],
                color,
                lw * 1.3,
            );
            b.line(
                p(0.20, 0.80).0,
                p(0.20, 0.80).1,
                p(0.80, 0.80).0,
                p(0.80, 0.80).1,
                color,
                lw * 1.3,
            );
        }
        IconName::Share => {
            b.fill_circle(x + s * 0.72, y + s * 0.28, s * 0.11, color);
            b.fill_circle(x + s * 0.28, y + s * 0.50, s * 0.11, color);
            b.fill_circle(x + s * 0.72, y + s * 0.72, s * 0.11, color);
            b.line(
                p(0.36, 0.44).0,
                p(0.36, 0.44).1,
                p(0.64, 0.32).0,
                p(0.64, 0.32).1,
                color,
                lw,
            );
            b.line(
                p(0.36, 0.56).0,
                p(0.36, 0.56).1,
                p(0.64, 0.68).0,
                p(0.64, 0.68).1,
                color,
                lw,
            );
        }
        IconName::Link => {
            b.stroke_rrect(
                x + s * 0.10,
                y + s * 0.36,
                s * 0.44,
                s * 0.28,
                s * 0.14,
                color,
                lw * 1.2,
            );
            b.stroke_rrect(
                x + s * 0.46,
                y + s * 0.36,
                s * 0.44,
                s * 0.28,
                s * 0.14,
                color,
                lw * 1.2,
            );
        }
        IconName::Tag => {
            b.stroke_path(
                &[
                    p(0.14, 0.20),
                    p(0.62, 0.20),
                    p(0.86, 0.50),
                    p(0.62, 0.80),
                    p(0.14, 0.80),
                    p(0.14, 0.20),
                ],
                color,
                lw * 1.1,
            );
            b.fill_circle(x + s * 0.32, y + s * 0.50, s * 0.06, color);
        }
        IconName::Image => {
            b.stroke_rrect(
                x + s * 0.10,
                y + s * 0.16,
                s * 0.80,
                s * 0.68,
                s * 0.06,
                color,
                lw,
            );
            b.fill_circle(x + s * 0.28, y + s * 0.34, s * 0.08, color);
            b.fill_path(
                &[
                    p(0.14, 0.78),
                    p(0.42, 0.52),
                    p(0.62, 0.70),
                    p(0.86, 0.52),
                    p(0.86, 0.78),
                ],
                color,
            );
        }
        IconName::Music => {
            b.fill_path(
                &[p(0.36, 0.76), p(0.36, 0.28), p(0.70, 0.20), p(0.70, 0.68)],
                color,
            );
            b.fill_circle(x + s * 0.28, y + s * 0.76, s * 0.11, color);
            b.fill_circle(x + s * 0.64, y + s * 0.68, s * 0.11, color);
        }
        IconName::Video => {
            b.stroke_rrect(
                x + s * 0.10,
                y + s * 0.22,
                s * 0.56,
                s * 0.56,
                s * 0.06,
                color,
                lw,
            );
            b.fill_path(
                &[p(0.68, 0.34), p(0.90, 0.26), p(0.90, 0.74), p(0.68, 0.66)],
                color,
            );
        }
        IconName::Camera => {
            b.stroke_rrect(
                x + s * 0.10,
                y + s * 0.28,
                s * 0.80,
                s * 0.50,
                s * 0.07,
                color,
                lw,
            );
            b.fill_path(
                &[p(0.36, 0.28), p(0.46, 0.16), p(0.58, 0.16), p(0.68, 0.28)],
                color,
            );
            b.stroke_circle(cx, cy + s * 0.03, s * 0.17, color, lw * 1.2);
        }
        IconName::Mic => {
            b.fill_rrect(
                x + s * 0.40,
                y + s * 0.14,
                s * 0.20,
                s * 0.44,
                s * 0.10,
                color,
            );
            b.stroke_path(
                &arc(cx, y + s * 0.62, s * 0.20, 0.0, pi, 12),
                color,
                lw * 1.2,
            );
            b.line(cx, y + s * 0.82, cx, y + s * 0.92, color, lw * 1.2);
        }
        IconName::ZoomIn | IconName::ZoomOut => {
            b.stroke_circle(x + s * 0.40, y + s * 0.40, s * 0.24, color, lw * 1.2);
            b.line(
                p(0.56, 0.56).0,
                p(0.56, 0.56).1,
                p(0.84, 0.84).0,
                p(0.84, 0.84).1,
                color,
                lw * 1.6,
            );
            b.line(
                p(0.30, 0.40).0,
                p(0.30, 0.40).1,
                p(0.50, 0.40).0,
                p(0.50, 0.40).1,
                color,
                lw * 1.2,
            );
            if name == IconName::ZoomIn {
                b.line(
                    p(0.40, 0.30).0,
                    p(0.40, 0.30).1,
                    p(0.40, 0.50).0,
                    p(0.40, 0.50).1,
                    color,
                    lw * 1.2,
                );
            }
        }
        IconName::Pin => {
            b.fill_circle(cx, y + s * 0.34, s * 0.20, color);
            b.fill_path(&[p(0.44, 0.50), p(0.56, 0.50), p(0.50, 0.90)], color);
        }
        IconName::Rocket => {
            b.fill_path(
                &[
                    p(0.50, 0.10),
                    p(0.68, 0.40),
                    p(0.68, 0.76),
                    p(0.32, 0.76),
                    p(0.32, 0.40),
                ],
                color,
            );
            b.fill_circle(cx, y + s * 0.40, s * 0.08, color.with_alpha(0.5));
            b.fill_path(&[p(0.32, 0.62), p(0.18, 0.86), p(0.34, 0.80)], color);
            b.fill_path(&[p(0.68, 0.62), p(0.82, 0.86), p(0.66, 0.80)], color);
        }
        IconName::Cart => {
            b.stroke_path(
                &[p(0.12, 0.20), p(0.26, 0.20), p(0.40, 0.68), p(0.84, 0.68)],
                color,
                lw * 1.3,
            );
            b.stroke_path(&[p(0.34, 0.44), p(0.78, 0.44)], color, lw * 1.1);
            b.fill_circle(x + s * 0.40, y + s * 0.86, s * 0.07, color);
            b.fill_circle(x + s * 0.72, y + s * 0.86, s * 0.07, color);
        }
    }
}

fn draw_poly(b: &mut DrawBuilder, pts: &[(f32, f32)], color: Color) {
    if pts.len() < 2 {
        return;
    }
    for w in pts.windows(2) {
        b.line(w[0].0, w[0].1, w[1].0, w[1].1, color, 1.6);
    }
}

/// `Icon(name, size, color, modifier)` — 矢量图标组合（平台无关 DrawCmd）。
pub fn icon(name: IconName, size: f32, color: Color, modifier: Modifier) {
    let m = modifier.size(size, size);
    Canvas(m, move |b, x, y, _w, _h| {
        draw_icon(b, name, x, y, size, color);
    });
}

// ══════════════════════════════ 液态玻璃开关 ══════════════════════════════

thread_local! {
    /// 全局液态玻璃开关（可观察状态，惰性创建）。
    static LIQUID_GLASS: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
}

/// 液态玻璃开关状态 — Compose `MutableState<bool>`。
///
/// 读取注册依赖，因此控件在组合中读它、外部切换它，即可定向重组（无需手动刷新）。
pub fn liquid_glass_state() -> MutableState<bool> {
    LIQUID_GLASS.with(|g| match g.get() {
        Some(st) => st,
        None => {
            let st = mutableStateOf(false);
            g.set(Some(st));
            st
        }
    })
}

/// **一个函数切换液态玻璃**：开 ⇄ 关，返回切换后的状态。
///
/// ```ignore
/// let on = ToggleLiquidGlass();   // true = 已切到液态玻璃
/// ```
pub fn ToggleLiquidGlass() -> bool {
    let st = liquid_glass_state();
    st.update(|v| *v = !*v);
    st.read_unregistered()
}

/// 直接设置液态玻璃开关。
pub fn SetLiquidGlass(on: bool) {
    liquid_glass_state().set(on);
}

/// 当前是否处于液态玻璃模式（不注册依赖，事件/绘制路径使用）。
pub fn liquid_glass_enabled() -> bool {
    liquid_glass_state().read_unregistered()
}

/// 由当前主题 + 开关解析玻璃材质规格（组合内调用：读状态即注册依赖，切换后自动重组）。
///
/// 返回 `enabled = false` 的规格时，控件应退回不透明底色。
pub fn glass_spec() -> GlassSpec {
    let on = liquid_glass_state().get();
    let colors = theme_colors();
    if !on {
        return GlassSpec::off();
    }
    let dark = colors.is_dark();
    GlassSpec {
        enabled: true,
        blur: 16.0,
        tint: if dark {
            Color::rgba(0.15, 0.15, 0.18, 0.50)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 0.40)
        },
        highlight: if dark {
            Color::rgba(1.0, 1.0, 1.0, 0.30)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 0.90)
        },
        outline: if dark {
            Color::rgba(1.0, 1.0, 1.0, 0.18)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 0.75)
        },
        line: 1.2,
        radius: 18.0,
    }
}
