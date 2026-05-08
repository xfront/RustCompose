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

use compose::layout::{Arrangement, Canvas, Clickable, CrossAxisAlignment, Modifier, Row, Spacer};
use compose::material::{ColorScheme, IconName, icon, theme_colors};

use crate::tokens;
use crate::widgets::Text;

/// 玻璃态壁纸：柔和彩色光斑铺底（毛玻璃的模糊需要有色内容才可见）。
pub fn LiquidGlassWallpaper(colors: ColorScheme) {
    let a = colors.primary;
    let b = colors.tertiary;
    let d = colors.secondary;
    Canvas(
        Modifier::new().fill_max_size().no_hit(),
        move |bb, x, y, w, h| {
            let r = w.max(h) * 0.45;
            bb.fill_circle(x + w * 0.18, y + h * 0.22, r, a.with_alpha(0.55));
            bb.fill_circle(x + w * 0.82, y + h * 0.30, r * 0.85, b.with_alpha(0.48));
            bb.fill_circle(x + w * 0.55, y + h * 0.86, r * 0.95, d.with_alpha(0.42));
            bb.fill_circle(x + w * 0.30, y + h * 0.72, r * 0.60, a.with_alpha(0.30));
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
