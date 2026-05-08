//! Material 3 设计令牌 — 排版阶梯 / 形状 / 间距 / 状态层 / 控件度量。
//!
//! 数值取自 `androidx.compose.material3`（`Typography`、`Shapes`、各组件
//! `*Defaults`），按本项目当前的 1dp = 1px 视口使用。
//!
//! 约定：**控件里不再写裸数字**。尺寸、圆角、字号、状态层透明度一律从这里取，
//! 这样「对齐 Jetpack Compose」是一次性的表驱动改动，而不是散落在 40 个函数里的
//! 逐处调参。

use compose::foundation::Color;
use compose::layout::TextStyle;

/// 生成一个排版角色（size / lineHeight / weight / letterSpacing）。
macro_rules! text_role {
    ($(#[$meta:meta])* $name:ident, $size:expr, $line:expr, $weight:expr, $ls:expr) => {
        $(#[$meta])*
        pub fn $name(color: Color) -> TextStyle {
            TextStyle::new()
                .size($size)
                .line_height($line)
                .weight($weight)
                .letter_spacing($ls)
                .color(color)
        }
    };
}

// ══════════════════════════════ 排版阶梯 ══════════════════════════════

text_role!(
    /// `displayLarge` 57/64。
    display_large,
    57.0,
    64.0,
    400,
    -0.25
);
text_role!(
    /// `displayMedium` 45/52。
    display_medium,
    45.0,
    52.0,
    400,
    0.0
);
text_role!(
    /// `displaySmall` 36/44。
    display_small,
    36.0,
    44.0,
    400,
    0.0
);
text_role!(
    /// `headlineLarge` 32/40。
    headline_large,
    32.0,
    40.0,
    400,
    0.0
);
text_role!(
    /// `headlineMedium` 28/36。
    headline_medium,
    28.0,
    36.0,
    400,
    0.0
);
text_role!(
    /// `headlineSmall` 24/32。
    headline_small,
    24.0,
    32.0,
    400,
    0.0
);
text_role!(
    /// 页面标题 / TopAppBar 标题 `titleLarge` 22/28。
    title_large,
    22.0,
    28.0,
    400,
    0.0
);
text_role!(
    /// 小标题（M3 列表标题实际用 `bodyLarge`，此角色供卡片标题等使用）16/24/500。
    title_medium,
    16.0,
    24.0,
    500,
    0.15
);
text_role!(
    /// `titleSmall` 14/20/500。
    title_small,
    14.0,
    20.0,
    500,
    0.1
);
text_role!(
    /// 正文大（列表主行、输入内容）16/24。
    body_large,
    16.0,
    24.0,
    400,
    0.5
);
text_role!(
    /// 正文中（列表副行、普通文本）14/20。
    body_medium,
    14.0,
    20.0,
    400,
    0.25
);
text_role!(
    /// 正文小（说明、辅助信息）12/16。
    body_small,
    12.0,
    16.0,
    400,
    0.4
);
text_role!(
    /// 标签大：**按钮 / 芯片 / 标签页**的文字角色 14/20/500。
    label_large,
    14.0,
    20.0,
    500,
    0.1
);
text_role!(
    /// 标签中：导航栏文字 12/16/500。
    label_medium,
    12.0,
    16.0,
    500,
    0.5
);
text_role!(
    /// 标签小：徽标、上标 11/16/500。
    label_small,
    11.0,
    16.0,
    500,
    0.5
);

// ══════════════════════════════ 形状 ══════════════════════════════

/// 两个排版角色之间插值（M3 输入框标签上浮 = `bodyLarge` ⇄ `labelSmall`）。
///
/// 字号、行高、字重、字距一起走：只缩字号不改行高，标签会在飞行途中和正文错开基线。
pub fn lerp_role(a: &TextStyle, b: &TextStyle, t: f32, color: Color) -> TextStyle {
    let l = |x: f32, y: f32| x + (y - x) * t;
    TextStyle {
        color,
        size: l(a.size, b.size),
        weight: l(a.weight as f32, b.weight as f32).round() as u16,
        italic: if t < 0.5 { a.italic } else { b.italic },
        family: b.family.clone(),
        line_height: match (a.line_height, b.line_height) {
            (Some(x), Some(y)) => Some(l(x, y)),
            (x, y) => x.or(y),
        },
        letter_spacing: l(a.letter_spacing, b.letter_spacing),
    }
}

/// `Shapes` 圆角阶梯。
pub const SHAPE_EXTRA_SMALL: f32 = 4.0;
pub const SHAPE_SMALL: f32 = 8.0;
pub const SHAPE_MEDIUM: f32 = 12.0;
pub const SHAPE_LARGE: f32 = 16.0;
pub const SHAPE_EXTRA_LARGE: f32 = 28.0;

/// `Shapes.ExtraLarge`（胶囊）：圆角 = 高度 / 2，按钮 / 开关 / 指示条用。
pub const fn full_radius(height: f32) -> f32 {
    height * 0.5
}

// ══════════════════════════════ 间距 ══════════════════════════════

pub const SPACE_XXS: f32 = 2.0;
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SPACE_XL: f32 = 20.0;
pub const SPACE_XXL: f32 = 24.0;
pub const SPACE_XXXL: f32 = 32.0;

// ══════════════════════════════ 状态层 ══════════════════════════════

/// 悬停：`onSurface` 8%（M3 `stateLayers.stateLayerOpacity`）。
pub const STATE_HOVER: f32 = 0.08;
/// 按下：12%。
pub const STATE_PRESSED: f32 = 0.12;
/// 聚焦：12%。
pub const STATE_FOCUSED: f32 = 0.12;
/// 禁用态内容（文字 / 图标）：`onSurface` 38%。
pub const STATE_DISABLED_CONTENT: f32 = 0.38;
/// 禁用态底色（filled / tonal / outlined 按钮）：`onSurface` 12%。
pub const STATE_DISABLED_BG: f32 = 0.12;
/// 选中但非主色（芯片 / 列表项）叠加：12%。
pub const STATE_SELECTED: f32 = 0.12;

/// 状态层着色：`color` 以 `alpha` 叠在表面上。
pub fn state_layer(color: Color, alpha: f32) -> Color {
    color.with_alpha(alpha)
}

// ══════════════════════════════ 高度（elevation） ══════════════════════════════

/// M3 elevation 档位（dp）。
pub const ELEVATION_NONE: f32 = 0.0;
pub const ELEVATION_1: f32 = 1.0;
pub const ELEVATION_2: f32 = 3.0;
pub const ELEVATION_3: f32 = 6.0;
pub const ELEVATION_4: f32 = 8.0;
pub const ELEVATION_5: f32 = 12.0;

/// 投影颜色（`push_shadow` 会按 elevation 再衰减）。
pub fn shadow_color(dark: bool) -> Color {
    if dark {
        Color::rgba(0.0, 0.0, 0.0, 0.55)
    } else {
        Color::rgba(0.0, 0.0, 0.0, 0.32)
    }
}

// ══════════════════════════════ 动效 ══════════════════════════════

/// M3 `MotionTokens` 时长档位（秒，配合 `compose::animation::Tween`）。
pub const DURATION_SHORT_1: f32 = 0.05;
pub const DURATION_SHORT_2: f32 = 0.10;
pub const DURATION_SHORT_3: f32 = 0.15;
pub const DURATION_SHORT_4: f32 = 0.20;
pub const DURATION_MEDIUM_1: f32 = 0.25;
pub const DURATION_MEDIUM_2: f32 = 0.30;
pub const DURATION_MEDIUM_3: f32 = 0.35;
pub const DURATION_MEDIUM_4: f32 = 0.40;
pub const DURATION_LONG_1: f32 = 0.45;
pub const DURATION_LONG_2: f32 = 0.50;
pub const DURATION_LONG_3: f32 = 0.55;
pub const DURATION_LONG_4: f32 = 0.60;

/// 状态层淡入淡出（M3 `stateLayers` 用 short4）。
pub const STATE_LAYER_DURATION: f32 = DURATION_SHORT_4;
// 波纹线速度 / 淡出时长 / tap slop 归**引擎**所有（`compose::layout` 按压指示一节），
// 控件侧不再重复定义，避免两处数值各改一半。
/// Tooltip 悬停多久后出现 / 离开后残留（Flutter `Tooltip` 默认 wait 500ms / exit 100ms）。
pub const TOOLTIP_DELAY: f32 = 0.5;
pub const TOOLTIP_EXIT_DELAY: f32 = 0.1;

/// `Tween` + M3 曲线的便捷构造（控件侧不再手写裸时长）。
pub fn tween(duration: f32, easing: compose::animation::Easing) -> compose::animation::Tween {
    compose::animation::Tween::new(duration).with_easing(easing)
}

/// 入场（尺寸/位置可见）：emphasized decelerate + medium2。
pub fn enter_spec() -> compose::animation::Tween {
    tween(DURATION_MEDIUM_2, compose::animation::emphasized_decelerate)
}
/// 出场（退场到不可见）：accelerate + short4。
pub fn exit_spec() -> compose::animation::Tween {
    tween(DURATION_SHORT_4, compose::animation::emphasized_accelerate)
}
/// 颜色/透明度等不可见属性的状态变化：standard + short4。
pub fn state_spec() -> compose::animation::Tween {
    tween(DURATION_SHORT_4, compose::animation::standard)
}

// ══════════════════════════════ 控件度量 ══════════════════════════════

/// 按钮 / 图标按钮 / 最小触控目标：40dp。
pub const CONTROL_HEIGHT: f32 = 40.0;
/// 按钮水平内边距（无图标）。
pub const BUTTON_PAD_H: f32 = 24.0;
/// 按钮水平内边距（有前导图标时的起点）。
pub const BUTTON_PAD_H_ICON: f32 = 16.0;
/// 按钮图标尺寸 / 图标与文字间距。
pub const BUTTON_ICON: f32 = 18.0;
pub const BUTTON_ICON_GAP: f32 = 8.0;

/// 芯片：高 32 / 圆角 8 / 图标 18。
pub const CHIP_HEIGHT: f32 = 32.0;
pub const CHIP_RADIUS: f32 = SHAPE_SMALL;
pub const CHIP_ICON: f32 = 18.0;
pub const CHIP_PAD_H: f32 = 16.0;
pub const CHIP_PAD_H_ICON: f32 = 12.0;

/// FAB：常规 56 / 小 40 / 大 96，圆角 16。
pub const FAB_SIZE: f32 = 56.0;
pub const FAB_SMALL: f32 = 40.0;
pub const FAB_LARGE: f32 = 96.0;
pub const FAB_RADIUS: f32 = SHAPE_LARGE;
pub const FAB_ICON: f32 = 24.0;

/// 输入框：高 56 / 圆角 4 / 水平内边距 16。
pub const FIELD_HEIGHT: f32 = 56.0;
pub const FIELD_RADIUS: f32 = SHAPE_EXTRA_SMALL;
pub const FIELD_PAD_H: f32 = 16.0;
/// 有内容时标签上浮到顶行（`label_small` 16 行高），正文落在 28~52 的 24dp 带内。
pub const FIELD_LABEL_TOP: f32 = 8.0;
pub const FIELD_VALUE_TOP: f32 = 28.0;
pub const FIELD_VALUE_H: f32 = 24.0;

/// 菜单项 48dp 高；下拉触发框 52dp（`ExposedDropdownMenuDefaults.FieldHeight`）、最小宽 160dp。
pub const MENU_ITEM_HEIGHT: f32 = 48.0;
pub const DROPDOWN_HEIGHT: f32 = 52.0;
pub const DROPDOWN_MIN_W: f32 = 160.0;

/// 描边线宽：静置 1dp，聚焦/展开 2dp（M3 `OutlinedCard`/`ExposedDropdownMenu` 的 state 描边）。
pub const OUTLINE_W: f32 = 1.0;
pub const OUTLINE_W_FOCUSED: f32 = 2.0;

/// 列表项：单行 56 / 双行 64 / 三行 88，左右 16。
pub const LIST_ONE_LINE: f32 = 56.0;
pub const LIST_TWO_LINE: f32 = 64.0;
pub const LIST_PAD_H: f32 = 16.0;
pub const LIST_ICON_SIZE: f32 = 24.0;
pub const LIST_ICON_BOX: f32 = 40.0;

/// 数据表格：表头行 56 / 数据行 52。
pub const TABLE_HEADER_HEIGHT: f32 = 56.0;
pub const TABLE_ROW_HEIGHT: f32 = 52.0;
/// 行号列宽 48 / 缺省列宽 96。
pub const TABLE_ROWNUM_WIDTH: f32 = 48.0;
pub const TABLE_COL_WIDTH: f32 = 96.0;

/// 网格单元：56dp 高，4dp 间隙。
pub const GRID_CELL_HEIGHT: f32 = 56.0;

/// 顶栏 64 / 标签栏 48 / 底部导航 80 / 导航指示条 64×32。
pub const APPBAR_HEIGHT: f32 = 64.0;
pub const TAB_HEIGHT: f32 = 48.0;
pub const TAB_INDICATOR: f32 = 3.0;
pub const NAVBAR_HEIGHT: f32 = 80.0;
pub const NAV_INDICATOR_W: f32 = 64.0;
pub const NAV_INDICATOR_H: f32 = 32.0;
/// 导航条（Rail）指示胶囊 56×32。
pub const RAIL_INDICATOR_W: f32 = 56.0;
/// M3 指示胶囊选中时的起始缩放：从 60% 长到 100%（Flutter `NavigationBar` 的 indicator 动画）。
pub const NAV_INDICATOR_FROM: f32 = 0.6;
/// 抽屉宽度（M3 NavigationDrawer = 256dp 常驻 / 360dp 模态）。
pub const DRAWER_WIDTH: f32 = 256.0;
/// 应用内非 M3 元件：窗口标题条 36 / 状态条 28 / 分页圆点 8。
pub const TITLEBAR_HEIGHT: f32 = 36.0;
pub const STATUSBAR_HEIGHT: f32 = 28.0;
pub const PAGE_DOT: f32 = 8.0;

/// 勾选框 18（触控目标 40）/ 单选 20 / 开关 52×32 / 滑杆轨道 4、拇指 20。
pub const CHECKBOX_BOX: f32 = 18.0;
pub const RADIO_DIAMETER: f32 = 20.0;
pub const SWITCH_W: f32 = 52.0;
pub const SWITCH_H: f32 = 32.0;
pub const SLIDER_TRACK: f32 = 4.0;
pub const SLIDER_THUMB: f32 = 20.0;

/// 图标默认尺寸（图标按钮 24，行内 18~24）。
pub const ICON_DEFAULT: f32 = 24.0;
pub const ICON_SMALL: f32 = 18.0;

/// 进度指示：线性轨道 4 / 环形描边 4 / 空态插图 48 / 图片占位图标 36。
pub const PROGRESS_TRACK: f32 = 4.0;
pub const SPINNER_STROKE: f32 = 4.0;
pub const EMPTY_STATE_ICON: f32 = 48.0;
pub const PLACEHOLDER_ICON: f32 = 36.0;

/// 浮层尺寸：消息条最大宽 560、顶边距底 88、入场下滑 48（标称自身高度）、驻留 4s；命令面板宽 560。
pub const SNACKBAR_MAX_W: f32 = 560.0;
pub const SNACKBAR_BOTTOM: f32 = 88.0;
pub const SNACKBAR_SLIDE: f32 = 48.0;
/// M3 `duration.short` = 4s（Flutter `SnackBar` 默认 4s；帧计数倒计时在宿主休眠时会停摆）。
pub const SNACKBAR_DURATION: f32 = 4.0;
pub const COMMAND_PALETTE_W: f32 = 560.0;

/// 对话框默认尺寸（M3 alert dialog 宽 280~560，取 400 留出正文换行余量）。
pub const DIALOG_W: f32 = 400.0;
pub const DIALOG_H: f32 = 240.0;

/// 浮层入场的上浮距离：引擎无 `graphicsLayer` 缩放（缩放会连带内容重排），
/// 故 M3 `fadeThrough` 的 scale 用「位移 + scrim 淡入」替代。
pub const OVERLAY_RISE: f32 = 16.0;

/// 日历：日期格子 40（M3 date picker 日间触控目标），字号取 `label_medium`。
pub const CALENDAR_CELL: f32 = 40.0;

/// 代码块：等宽近似单字符宽 7 / 行高 18 / 行号列 30。
pub const CODE_CHAR_WIDTH: f32 = 7.0;
pub const CODE_LINE_HEIGHT: f32 = 18.0;
pub const CODE_GUTTER: f32 = 30.0;

/// 图表：图例色块 12、色块↔文字 4、条目间距 16、图例行高 22。
pub const CHART_LEGEND_SWATCH: f32 = 12.0;
pub const CHART_LEGEND_ROW_H: f32 = 22.0;

/// 聊天气泡最大宽度（M3 会话窗口宽度上限，超出折行）。
pub const BUBBLE_MAX_W: f32 = 320.0;
