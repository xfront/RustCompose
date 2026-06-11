//! 共享导入 —— 各模块经 `use crate::prelude::*;` 一次性引入全部常用 API。
//!
//! 原 `story.rs` 顶部的 `use` 列表整体搬到这里：拆分后各模块作用域独立，
//! 用公共 prelude 避免逐模块重复，同时保持「改一处即全应用」的效果。

#![allow(unused_imports)]

pub use compose::animation::{
    Spring, Tween, animate_float_as_state, ease_out_cubic, request_anim_frames,
};

pub use compose::layout::{
    Alignment, Arrangement, Box, Canvas, Clickable, Column, CrossAxisAlignment, DrawBuilder,
    Modifier, Row, Spacer, TextStyle,
};

pub use compose::material::{
    ColorScheme, IconName, MaterialTheme, THEME_PRESETS, draw_icon, icon, liquid_glass_state,
    theme_colors,
};

pub use compose::prelude::Color;

pub use compose::runtime::{
    MutableState, add_frame_driver, current_scope_id, frame_delta, mutableStateOf, remember,
    schedule_scope,
};

pub use compose_material::widgets::{set_menu_callback, SidebarSection, menu_panel_auto_width};

pub use compose_material::*;

pub use compose::{App, WindowConfig, run_app_with};

// ════════════════════════════ 页面注册表（103 页） ════════════════════════════
