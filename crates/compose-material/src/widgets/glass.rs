//! 液态玻璃（Liquid Glass）— 材质开关与壁纸背景。
//!
//! 真正的毛玻璃由 `compose-render` 用 valo `save_layer_backdrop` + 高斯背景模糊实现
//! （`DrawCmd::GlassPanel`）。本模块给出**切换入口**与玻璃态所需的彩色桌面：
//!
//! - [`ToggleLiquidGlass`] — 一个函数切换液态玻璃（开 ⇄ 关，返回切换后状态）
//! - [`SetLiquidGlass`] / [`liquid_glass_enabled`] — 直接设置 / 读取
//! - [`LiquidGlassWallpaper`] — 玻璃态壁纸（模糊需要有内容才看得出来）

pub use compose::material::{
    SetLiquidGlass, ToggleLiquidGlass, glass_spec, liquid_glass_enabled, liquid_glass_state,
};

use compose::foundation::Color;
use compose::layout::{Arrangement, Canvas, Clickable, CrossAxisAlignment, DrawBuilder, Modifier, Row, Spacer};
use compose::material::{ColorScheme, IconName, icon, theme_colors};

use crate::tokens;
use crate::widgets::Text;

/// 每枚光斑用多少层同心圆盘叠加近似径向衰减（`DrawCmd` 没有径向渐变原语）。
///
/// 直接画一个大实心圆 → dock 分栏缝隙里会露出一整条硬边圆弧，观感像贴了张色纸。
/// 同心环按 smoothstep 递增各层 alpha，叠加后中心累计不透明度恰等于峰值、边缘平滑归零。
pub const GLASS_BLOB_RINGS: usize = 12;

/// 壁纸光斑的「桌面级」配色：与主题 `primary` 轻混 0.22。
///
/// 不能直接用 `secondary` / `tertiary` —— M3 派生色板里它们是低饱和灰调（light 下
/// 0x625B71 / 0x7D5260），铺底后毛玻璃里只剩一团灰紫雾，模糊完全看不出颜色层次
/// （「液态玻璃背景不好看」的头号主因）。固定高饱和色板保证色相跨度，混 primary
/// 又让换 Ocean/Forest/Rose 色板时壁纸跟着变调。
fn blob_color(vivid: Color, primary: Color) -> Color {
    vivid.mix(primary, 0.22)
}

/// 一枚柔和光斑：`GLASS_BLOB_RINGS` 层同心圆，第 i 层的增量 alpha 由「累计目标」反推。
///
/// 叠加 i 层后的合成不透明度 = `want(i)`，故本层 alpha = `(want - acc) / (1 - acc)`
/// （src-over 语义）。最外层 `f = 1` 时 `want = 0`，该盘不发射。
fn draw_blob(bb: &mut DrawBuilder, cx: f32, cy: f32, r: f32, col: Color, peak: f32) {
    let mut acc = 0.0f32;
    for i in 0..GLASS_BLOB_RINGS {
        let f = 1.0 - i as f32 / GLASS_BLOB_RINGS as f32;
        let want = peak * (1.0 - f * f * (3.0 - 2.0 * f));
        let a = (want - acc) / (1.0 - acc);
        if a > 0.0 {
            bb.fill_circle(cx, cy, r * f, col.with_alpha(a));
        }
        acc = want;
    }
}

/// 玻璃态壁纸：柔和彩色光斑铺底（毛玻璃的模糊需要有色内容才可见）。
///
/// `clip(0.0)` 必需：光斑半径 `max(w,h)·0.45` 远大于自身矩形，不裁就会溢到窗口留白
/// 与浮动窗外，沿边缘出现一圈「彩色画框」。
pub fn LiquidGlassWallpaper(colors: ColorScheme) {
    let dark = colors.is_dark();
    // 暗色主题下提亮并加浓：玻璃 tint 本身是 40% 深灰，壁纸再暗就直接糊成一片墨。
    let gain = if dark { 1.35 } else { 1.0 };
    let tint = |c: Color| if dark { c.mix(Color::WHITE, 0.28) } else { c };
    let a = tint(blob_color(Color::hex(0xFF_5B4B_FF), colors.primary)); // 紫罗兰
    let b = tint(blob_color(Color::hex(0xFF_FF4F_A3), colors.primary)); // 品红
    let d = tint(blob_color(Color::hex(0xFF_18C2_FF), colors.primary)); // 天青
    let e = tint(blob_color(Color::hex(0xFF_2BE3_A6), colors.primary)); // 薄荷
    Canvas(
        Modifier::new().fill_max_size().no_hit().clip(0.0),
        move |bb, x, y, w, h| {
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            let r = w.max(h) * 0.45;
            let peak = |p: f32| (p * gain).min(0.92);
            draw_blob(bb, x + w * 0.18, y + h * 0.22, r, a, peak(0.55));
            draw_blob(bb, x + w * 0.82, y + h * 0.30, r * 0.85, b, peak(0.48));
            draw_blob(bb, x + w * 0.55, y + h * 0.86, r * 0.95, d, peak(0.42));
            draw_blob(bb, x + w * 0.30, y + h * 0.72, r * 0.60, e, peak(0.30));
        },
    );
}

/// 根背景：玻璃态铺壁纸 + 半透明底，普通态用主题 `background`。
pub fn GlassRootBackground() -> Modifier {
    let colors = theme_colors();
    if liquid_glass_enabled() {
        LiquidGlassWallpaper(colors);
        Modifier::new().fill_max_size()
    } else {
        Modifier::new()
            .fill_max_size()
            .background(colors.background, 0.0)
    }
}

/// 一个按钮切换液态玻璃（把 [`ToggleLiquidGlass`] 暴露到 UI 上）。
pub fn GlassToggleButton(modifier: Modifier) {
    let on = liquid_glass_enabled();
    let colors = theme_colors();
    let g = glass_spec();
    let label = if on {
        "Liquid Glass: On"
    } else {
        "Liquid Glass: Off"
    };
    let fg = if g.enabled {
        colors.on_surface
    } else {
        colors.on_primary
    };
    let m = if g.enabled {
        modifier.glass(g.with_radius(tokens::full_radius(tokens::CONTROL_HEIGHT)))
    } else {
        modifier.background(colors.primary, tokens::full_radius(tokens::CONTROL_HEIGHT))
    }
    .height(tokens::CONTROL_HEIGHT)
    .padding_ltrb(tokens::SPACE_XXL, 0.0, tokens::SPACE_XXL, 0.0);
    Clickable(
        m,
        move || {
            ToggleLiquidGlass();
        },
        move || {
            Row(
                Modifier::new(),
                Arrangement::Center,
                CrossAxisAlignment::Center,
                move || {
                    icon(IconName::Brush, tokens::ICON_SMALL, fg, Modifier::new());
                    Spacer(Modifier::new().width(tokens::SPACE_XS));
                    Text(label, tokens::label_large(fg), Modifier::new());
                },
            );
        },
    );
}
