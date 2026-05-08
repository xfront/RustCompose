//! # compose
//!
//! 跨平台 Rust GUI 库，架构严格对齐 Jetpack Compose：
//!
//! - **组合函数模型**：`fn(c: &mut Composer, ..)` 即 `@Composable`，内容闭包即 trailing lambda
//! - **状态驱动重组**：`mutableStateOf` 读取注册依赖、写入只失效读到它的作用域（定向重组）
//! - **remember 槽位**：作用域内位置即身份（对齐 SlotTable 语义），跨重组存活
//! - **Modifier 值链**：`Modifier::new().padding(16.0).background(..).clip(..)`
//! - **三阶段渲染**：measure → place → draw，产出平台无关 `DrawCmd`
//! - **Material 3**：`MaterialTheme` + text/Button/Surface/Card/Checkbox/Switch/Slider/TextField
//! - **零 `Rc`/`Arc`/`unsafe`**：节点/状态/作用域全 arena 化，`u32` 句柄引用
//!
//! 本 crate 已合并渲染层（`render`：valo + wgpu + winit 宿主 / 帧内核）与跨平台宿主
//! 粘合层（`platform`：desktop/web/android/ios/ohos/services），单 crate 即可出品
//! 桌面 / Web / Android / iOS / OHOS；控件库见 `compose-material`。

// 使 `crate::render` / `crate::platform` 内从旧独立 crate 迁来的代码沿用
// `use compose::…` 自引用路径（合并前它们跨 crate 引用 compose 根）。
extern crate self as compose;

pub mod animation;
pub mod foundation;
pub mod input_router;
pub mod layout;
pub mod material;
pub mod platform;
pub mod render;
pub mod runtime;

// ── 根级便利重导出（原 compose-platform 的对外门面；业务侧 `use compose::{App, run_app, …}`）──
pub use render::{
    App, GpuRenderer, Kernel, TextContext, ValoTextMeasurer, build_display_list,
    default_font_collection, init_text_context,
};
// winit 宿主入口（仅桌面 / Web；Android 走 android-activity，iOS / OHOS 走宿主驱动 C ABI）。
#[cfg(all(
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
pub use render::{WindowConfig, run_app, run_app_with};
#[cfg(target_os = "android")]
pub use platform::android::{AndroidApp, android_entry, run_app_with_android};

/// 一步到位的常用导入。
pub mod prelude {
    pub use crate::animation::{
        AnimSpec, AnimVector, Animatable, AnimatableValue, Easing, Infinite, Keyframes, LoopMode,
        Snap, Spring, Tween, animate_color_as_state, animate_dp_as_state, animate_float_as_state,
        animate_progress_on, animate_value_as_state, clock_nanos, ease_in_cubic, ease_in_out_cubic,
        ease_out_cubic, fast_out_slow_in, infinite, infinite_with, keyframes, linear, loop_phase,
        progress_since_nanos, remember_animatable, remember_infinite_progress,
        remember_infinite_transition, running_animations, snap, spring, spring_with, tween,
    };
    pub use crate::foundation::{Color, Constraints, Density, Dp, Offset, Rect, Size, dp, sp};
    pub use crate::layout::{
        Alignment, Arrangement, Box, Canvas, Clickable, Column, CrossAxisAlignment, DrawBuilder,
        DrawCmd, GlassSpec, MeasureCtx, MeasurePolicy, MeasureResult, Modifier, NodeId, PullPhase,
        Row, Spacer, TextStyle, dispatch_char, dispatch_click_at, dispatch_hover_at,
        dispatch_hover_left, dispatch_key, dispatch_pull_down, dispatch_scrollbar_down, focusable,
        focusable_full, hit_test, hoverable, is_mobile, layout, next_pull_key, pull_end_global,
        pull_move_global, register_pull_target, scrollbar_end_global, scrollbar_move_global,
    };
    pub use crate::material::{
        ColorScheme, IconName, LocalContentColor, MaterialTheme, SetLiquidGlass, ToggleLiquidGlass,
        draw_icon, glass_spec, icon, liquid_glass_enabled, liquid_glass_state, theme_colors,
    };
    pub use crate::runtime::NavKey;
    pub use crate::runtime::{
        Composer, Composition, CompositionLocal, MutableState, Recomposer, TaskId, advance_frame,
        clear_focus, derivedStateOf, disposable_effect, focused_scope, keyed_remember,
        launched_effect, mutableStateOf, remember, set_focus, side_effect, spawn_task,
    };
}
