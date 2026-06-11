//! Docking —— 可拖拽停靠面板布局。
//!
//! 对齐 Jetpack Compose / ModernDocking / VS 2022 的停靠语义：
//! * 布局树只有两种节点：[`DockNode::Split`]（带比例的水平/垂直分割）与
//!   [`DockNode::Tabs`]（标签组，单面板也是一个标签组）；
//! * 标签头可拖出，落到目标面板的 **5 区罗盘**（上/下/左/右/中心）后重新停靠，
//!   中心区合并为同一个标签组；拖出布局外则浮起为窗口；
//! * 分割条可拖拽，带最小尺寸约束与 hover 高亮；
//! * 最大化/还原、自动隐藏到四边（悬停滑出 + 钉住恢复）、浮动窗口（拖动/缩放/回靠）；
//! * 布局与完整状态可序列化/恢复；每次操作派发 [`DockingEvent`]。
//!
//! **架构约束**：状态全部是 arena 里的 [`MutableState`] 句柄（`Copy`），面板内容闭包
//! 存在线程局部表中按 [`PanelId`] 转发，节点/几何全部通过 `NodeId` 查询，零 `Rc`/`Arc`。
//! 拖拽提交点依赖核心引擎的 `begin_drag_2d_with_end`（抬起时给出最终全局坐标）。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use compose::animation::{
    Tween, animate_progress_on, clock_nanos, request_anim_frame_after,
};
use compose::cursor::{Cursor, set_cursor};
use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Box, Canvas, Clickable, Column, CrossAxisAlignment, DrawBuilder,
    GlassSpec, Modifier, NodeId, Policy, Row, Spacer, add_node, begin_drag_2d_full,
    drag_2d_cancel_global, hoverable, invalidate_draw_node,
    last_pointer_global, node_abs_rect, scroll_by, scroll_metrics, viewport_size,
    with_content_scope,
};
use compose::material::{ColorScheme, IconName, glass_spec, icon, theme_colors};
use compose::runtime::{MutableState, mutableStateOf, remember, with_rt_pub};

use crate::tokens;
use crate::widgets::{GlassOrBg, HorizontalScroll, Text};

use super::dock_layout::{DockLayout, WINDOW_MAIN, WindowBounds, WindowId};
pub use super::dock_model::{
    AnchorId, DockDirection, DockNodeId, DockRegion, DockTarget, DockableOptions, DockingSettings,
    DockingStyle, EmptyAnchorVisibility, NodeIdGenerator, TabPreference, region_is_allowed,
};

// ══════════════════════════════ 常量 ══════════════════════════════

/// 分割条视觉宽度（px）。
pub const DOCK_SPLITTER_PX: f32 = 4.0;
/// 标签栏 / 单面板头高度（px）。
pub const DOCK_HEADER_H: f32 = 30.0;
/// 按下到进入拖拽的位移阈值（px）。
pub const DOCK_DRAG_THRESHOLD: f32 = 5.0;
/// 5 区罗盘的相对阈值（0.28，先判上下再判左右）。
pub const DOCK_ZONE_THRESHOLD: f32 = 0.28;
/// 整窗外缘停靠带宽度（px）：贴到容器任一边这条带内即视为整窗停靠，
/// 优先于组内罗盘（VS：从底部工具窗拖到左/右屏边 = 全高侧栏，而非底部条二分）。
pub const DOCK_OUTER_EDGE_GRAB: f32 = 24.0;
/// 分割子区域最小尺寸（px）。
pub const DOCK_MIN_EXTENT: f32 = 60.0;
/// 自动隐藏边条厚度（px）。
pub const DOCK_STRIP_THICKNESS: f32 = 30.0;
/// 比例下限 / 上限。
pub const DOCK_RATIO_MIN: f32 = 0.05;
pub const DOCK_RATIO_MAX: f32 = 0.95;
/// 浮动窗口最小尺寸。
pub const DOCK_FLOATING_MIN_W: f32 = 200.0;
pub const DOCK_FLOATING_MIN_H: f32 = 150.0;
/// VS 停靠指南针单格尺寸 / 间距（px）。
pub const DOCK_GUIDE_SIZE: f32 = 34.0;
pub const DOCK_GUIDE_GAP: f32 = 5.0;
/// 指南命中额外容差（px）——VS 的指南吸附区比图标略大。
pub const DOCK_GUIDE_PAD: f32 = 5.0;
/// 标签头最大宽度（超出按测量截断加省略号）。
pub const DOCK_TAB_MAX_W: f32 = 190.0;

// ══════════════════════════════ DockingTheme ══════════════════════════════

/// Dock 的外观度量（对齐 KT `com.derlio.docking.DockingTheme`）。
///
/// 常量版本（[`DOCK_HEADER_H`] 等）是本结构的**默认值**；主题只改渲染层的读取，
/// 未提供 `LocalDockingTheme` 时行为与旧实现逐像素一致。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockingTheme {
    /// 标签条 / 单面板头高度。
    pub header_height: f32,
    /// 分割条宽度。
    pub splitter_width: f32,
    /// 标签头最大宽度。
    pub tab_max_width: f32,
    /// 自动隐藏边条厚度。
    pub strip_thickness: f32,
    /// 停靠指南单格尺寸。
    pub guide_size: f32,
    /// 指南格间距。
    pub guide_gap: f32,
    /// 指南命中额外容差。
    pub guide_pad: f32,
    /// 强调色覆盖（`None` = 主题 `primary`）：指南箭头、活动标签 accent、边条滑出高亮。
    pub accent: Option<Color>,
    /// 强调底色覆盖（`None` = 主题 `primary_container`）：落点预览、选中条目。
    pub accent_container: Option<Color>,
    /// 拖放悬停高亮透明度。当前仅作用于分割条染色与落点预览线；
    /// 指南格底色另有 0.35/0.55 硬编码，后续迁移后一并生效。
    pub hover_alpha: f32,
}

impl Default for DockingTheme {
    fn default() -> Self {
        Self::standard()
    }
}

impl DockingTheme {
    pub fn with_header_height(mut self, h: f32) -> Self {
        self.header_height = h;
        self
    }
    pub fn with_splitter_width(mut self, w: f32) -> Self {
        self.splitter_width = w;
        self
    }
    pub fn with_guides(mut self, size: f32, gap: f32, pad: f32) -> Self {
        self.guide_size = size;
        self.guide_gap = gap;
        self.guide_pad = pad;
        self
    }
    pub fn with_accent(mut self, accent: Color, container: Color) -> Self {
        self.accent = Some(accent);
        self.accent_container = Some(container);
        self
    }
    pub fn with_hover_alpha(mut self, a: f32) -> Self {
        self.hover_alpha = a;
        self
    }

    /// 标准主题（= 默认值，对齐 KT `DockingTheme.standard`）。
    pub const fn standard() -> Self {
        DockingTheme {
            header_height: DOCK_HEADER_H,
            splitter_width: DOCK_SPLITTER_PX,
            tab_max_width: DOCK_TAB_MAX_W,
            strip_thickness: DOCK_STRIP_THICKNESS,
            guide_size: DOCK_GUIDE_SIZE,
            guide_gap: DOCK_GUIDE_GAP,
            guide_pad: DOCK_GUIDE_PAD,
            accent: None,
            accent_container: None,
            hover_alpha: 0.35,
        }
    }

    /// 紧凑主题：头 26 / 分割条 6 / 指南 28（对齐 KT `DockingTheme.compact`）。
    pub const fn compact() -> Self {
        DockingTheme {
            header_height: 26.0,
            splitter_width: 6.0,
            tab_max_width: 160.0,
            strip_thickness: 24.0,
            guide_size: 28.0,
            guide_gap: 4.0,
            guide_pad: 6.0,
            accent: None,
            accent_container: None,
            hover_alpha: 0.30,
        }
    }
}

/// `LocalDockingTheme`（KT 同名 CompositionLocal）。
pub struct LocalDockingTheme;
impl LocalDockingTheme {
    /// 当前 dock 主题。
    ///
    /// 读的是**全局镜像**而不是 `CompositionLocal` 栈：主题既要在组合期决定头高/分割条
    /// 宽，也要在**绘制闭包与事件回调**里被读到，而这两类回调跑在
    /// `perform_layout` 的 `RUNTIME.borrow_mut()` 之内——`CompositionLocal::try_current`
    /// 内部会再次借用运行时，直接 `RefCell already borrowed` panic。
    /// `ProvideDockingTheme` / `DockLayout` 每趟组合刷新镜像，语义等价。
    pub fn current() -> DockingTheme {
        docking_theme_global()
    }
}

thread_local! {
    static DOCK_THEME: Cell<Option<DockingTheme>> = const { Cell::new(None) };
}

fn set_docking_theme_global(t: DockingTheme) {
    DOCK_THEME.with(|c| c.set(Some(t)));
}

/// 组合外回调（命中 / 绘制 / 拖拽）读取的 dock 主题镜像。
pub fn docking_theme_global() -> DockingTheme {
    DOCK_THEME.with(|c| c.get()).unwrap_or_default()
}

/// 当前 dock 主题（= `LocalDockingTheme.current()`，见其文档）。
pub fn docking_theme() -> DockingTheme {
    LocalDockingTheme::current()
}

/// `ProvideDockingTheme(theme) { content }`（KT `DockingTheme` 顶层 Provider）。
pub fn ProvideDockingTheme(theme: DockingTheme, content: impl Fn() + 'static) {
    set_docking_theme_global(theme);
    content();
}

// 度量读取（保持与旧常量同名语义，方便逐点替换）。
fn th_header_h() -> f32 {
    docking_theme().header_height
}
fn th_splitter_w() -> f32 {
    docking_theme().splitter_width
}
fn th_tab_max_w() -> f32 {
    docking_theme().tab_max_width
}

/// 强调色（主题覆盖优先）。
fn dock_accent(colors: ColorScheme) -> Color {
    docking_theme().accent.unwrap_or(colors.primary)
}
/// 强调底色（主题覆盖优先）。
fn dock_accent_container(colors: ColorScheme) -> Color {
    docking_theme()
        .accent_container
        .unwrap_or(colors.primary_container)
}

/// 面板 ID（全局注册时分配，稳定且可序列化）。
pub type PanelId = u64;
/// 矩形 `(x, y, w, h)`，一律为窗口绝对像素坐标。
pub type DockRect = (f32, f32, f32, f32);

/// VS 式停靠指南（dock guide）：拖拽期间**只有命中指南才投放**，
/// 而不是按目标矩形的五分区盲判——这是 VS 停靠手感的核心。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockGuide {
    /// 悬停目标组中心的 5 向罗盘（中心 = 并入标签组）。
    Inner { target: PanelId, dir: DockDirection },
    /// 主窗口四边的外侧指南（整窗分割，VS 的外圈菱形）。
    Outer { dir: DockDirection },
    /// 落在某组标签条上：作为标签插入到 `index`（同组即重排）。
    Tab { lead: PanelId, index: usize },
}

impl DockGuide {
    /// 指南对应的分割方向（`Tab` 视为中心）。
    pub fn direction(self) -> DockDirection {
        match self {
            DockGuide::Inner { dir, .. } | DockGuide::Outer { dir } => dir,
            DockGuide::Tab { .. } => DockDirection::Center,
        }
    }
}

/// 组内 5 向罗盘的指南矩形（中心格 + 四向格）。
pub fn dock_inner_guides(r: DockRect) -> [(DockDirection, DockRect); 5] {
    let (x, y, w, h) = r;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let th = docking_theme();
    let s = th.guide_size;
    let step = s + th.guide_gap;
    [
        (DockDirection::Center, (cx - s / 2.0, cy - s / 2.0, s, s)),
        (DockDirection::North, (cx - s / 2.0, cy - step - s / 2.0, s, s)),
        (DockDirection::South, (cx - s / 2.0, cy + step - s / 2.0, s, s)),
        (DockDirection::West, (cx - step - s / 2.0, cy - s / 2.0, s, s)),
        (DockDirection::East, (cx + step - s / 2.0, cy - s / 2.0, s, s)),
    ]
}

/// 主窗口四边外侧指南：贴边中点，向内缩进半个指南。
pub fn dock_outer_guides(r: DockRect) -> [(DockDirection, DockRect); 4] {
    let (x, y, w, h) = r;
    let s = docking_theme().guide_size;
    let m = s / 2.0 + 6.0;
    [
        (DockDirection::West, (x + m - s / 2.0, y + h / 2.0 - s / 2.0, s, s)),
        (DockDirection::East, (x + w - m - s / 2.0, y + h / 2.0 - s / 2.0, s, s)),
        (DockDirection::North, (x + w / 2.0 - s / 2.0, y + m - s / 2.0, s, s)),
        (DockDirection::South, (x + w / 2.0 - s / 2.0, y + h - m - s / 2.0, s, s)),
    ]
}

/// 指南命中（带容差）。
pub fn dock_guide_hit(guides: &[(DockDirection, DockRect)], x: f32, y: f32) -> Option<DockDirection> {
    let p = docking_theme().guide_pad;
    guides.iter().find_map(|(dir, r)| {
        if x >= r.0 - p && x <= r.0 + r.2 + p && y >= r.1 - p && y <= r.1 + r.3 + p {
            Some(*dir)
        } else {
            None
        }
    })
}

// ══════════════════════════════ 液态玻璃表面 ══════════════════════════════

/// dock 的容器 / 浮层表面：跟随全局 [`glass_spec`] 开关。
///
/// * 玻璃态：`GlassPanel`（背景高斯模糊 + 半透明 tint + 镜面高光 + 内描边）。
///   它自带高光与描边，故不再叠加 `.border()`，否则会双重描边。
/// * 普通态：完全保持原有的不透明底色 + 细描边，零视觉回归。
///
/// 面板主体、锚点占位、自动隐藏边条、滑出预览、浮动窗口、标签下拉与右键菜单
/// 共用此入口；`glass_spec()` 在组合内读取状态会注册依赖，切换开关自动重组 dock。
fn dock_surface(
    m: Modifier,
    g: GlassSpec,
    color: Color,
    radius: f32,
    border_width: f32,
    border_color: Color,
) -> Modifier {
    let m = GlassOrBg(m, g, color, radius);
    if g.enabled {
        m
    } else {
        m.border(border_width, border_color, radius)
    }
}

/// 面板**内部**的装饰底色（标签条 / 活动标签 / 菜单高亮项）。
///
/// 玻璃态下必须降为低 alpha，否则不透明色块会把下层玻璃材质完全盖住；
/// 普通态原色返回。
fn dock_inner_color(color: Color, g: GlassSpec) -> Color {
    if g.enabled {
        color.with_alpha(0.22)
    } else {
        color
    }
}

// ══════════════════════════════ 枚举 ══════════════════════════════

// 注：停靠方向/区域枚举已迁到 `dock_model::DockRegion`（对齐 KT `DockRegion`），
// 本模块沿用旧名 `DockDirection`（类型别名，源码兼容）。

/// 自动隐藏边缘。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl DockEdge {
    pub fn index(self) -> u8 {
        match self {
            DockEdge::Left => 0,
            DockEdge::Right => 1,
            DockEdge::Top => 2,
            DockEdge::Bottom => 3,
        }
    }

    pub fn from_index(v: u8) -> Option<Self> {
        Some(match v {
            0 => DockEdge::Left,
            1 => DockEdge::Right,
            2 => DockEdge::Top,
            3 => DockEdge::Bottom,
            _ => return None,
        })
    }

    /// 左右边条（文字竖排）。
    pub fn is_side(self) -> bool {
        matches!(self, DockEdge::Left | DockEdge::Right)
    }

    /// 该边缘对应的停靠方向。
    pub fn direction(self) -> DockDirection {
        match self {
            DockEdge::Left => DockDirection::West,
            DockEdge::Right => DockDirection::East,
            DockEdge::Top => DockDirection::North,
            DockEdge::Bottom => DockDirection::South,
        }
    }

    fn align(self) -> Alignment {
        match self {
            DockEdge::Left => Alignment::CenterStart,
            DockEdge::Right => Alignment::CenterEnd,
            DockEdge::Top => Alignment::TopCenter,
            DockEdge::Bottom => Alignment::BottomCenter,
        }
    }

    fn strip_align(self) -> Alignment {
        match self {
            DockEdge::Left => Alignment::CenterStart,
            DockEdge::Right => Alignment::CenterEnd,
            DockEdge::Top => Alignment::TopStart,
            DockEdge::Bottom => Alignment::BottomStart,
        }
    }
}

/// 分割方向。语义与参考实现一致：`Horizontal` = 左右并排（竖直分割条），
/// `Vertical` = 上下堆叠（水平分割条）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitOrientation {
    Horizontal,
    Vertical,
}

impl SplitOrientation {
    fn tag(self) -> char {
        match self {
            SplitOrientation::Horizontal => 'H',
            SplitOrientation::Vertical => 'V',
        }
    }

    fn from_tag(c: &str) -> Option<Self> {
        match c {
            "H" => Some(SplitOrientation::Horizontal),
            "V" => Some(SplitOrientation::Vertical),
            _ => None,
        }
    }
}

// ══════════════════════════════ 布局树 ══════════════════════════════

/// 停靠布局树节点 —— 对齐 compose-docking 的 `model/DockNode.kt`。
///
/// 四种形态与 KT 一一对应：
/// * [`DockNode::Leaf`] 单个停靠面板；
/// * [`DockNode::Split`] 二分节点（左右并排 / 上下堆叠 + 比例）；
/// * [`DockNode::Tabs`] 标签组（成员恒为 Leaf，带选中下标）；
/// * [`DockNode::Anchor`] 命名占位锚点（成员全部离开后仍保留槽位）。
///
/// 节点带**稳定 id**（[`DockNodeId`]），随布局持久化，作为每节点 UI 状态的 key
/// （分割条拖动、标签行滚动等在树重组后仍能存活）。
///
/// 出于 Rust 语义考虑，`PartialEq` 比较**结构**（忽略节点 id），
/// 便于「同形状、不同 id」的树互相比较。
#[derive(Clone, Debug)]
pub enum DockNode {
    /// 单个停靠面板（KT `DockNode.Leaf`）。
    Leaf {
        id: DockNodeId,
        /// 面板 id。
        dockable_id: PanelId,
    },
    /// 二分节点（KT `DockNode.Split`）。
    Split {
        id: DockNodeId,
        orientation: SplitOrientation,
        /// `first` 占比（0..1）。
        proportion: f32,
        first: Box<DockNode>,
        second: Box<DockNode>,
    },
    /// 标签组（KT `DockNode.Tabs`）；成员恒为 [`DockNode::Leaf`]。
    Tabs {
        id: DockNodeId,
        tabs: Vec<DockNode>,
        selected_index: usize,
    },
    /// 命名占位锚点（KT `DockNode.Anchor`）：最后一个成员离开后还原此占位。
    Anchor {
        id: DockNodeId,
        anchor_id: AnchorId,
    },
}

impl PartialEq for DockNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                DockNode::Leaf {
                    dockable_id: a, ..
                },
                DockNode::Leaf {
                    dockable_id: b, ..
                },
            ) => a == b,
            (
                DockNode::Split {
                    orientation: o1,
                    proportion: p1,
                    first: f1,
                    second: s1,
                    ..
                },
                DockNode::Split {
                    orientation: o2,
                    proportion: p2,
                    first: f2,
                    second: s2,
                    ..
                },
            ) => o1 == o2 && p1 == p2 && f1 == f2 && s1 == s2,
            (
                DockNode::Tabs {
                    tabs: t1,
                    selected_index: i1,
                    ..
                },
                DockNode::Tabs {
                    tabs: t2,
                    selected_index: i2,
                    ..
                },
            ) => t1 == t2 && i1 == i2,
            (
                DockNode::Anchor {
                    anchor_id: a, ..
                },
                DockNode::Anchor {
                    anchor_id: b, ..
                },
            ) => a == b,
            _ => false,
        }
    }
}

thread_local! {
    /// 节点 id 生成器（会话基址 + 计数器，见 [`NodeIdGenerator`]）。
    static NODE_IDS: RefCell<NodeIdGenerator> = RefCell::new(NodeIdGenerator::new());
    /// 全局标签条位置偏好（`TabPreference::Default` 解析为此值，对齐 KT 的全局 tabPosition）。
    static TAB_DEFAULT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// 设置全局标签条位置偏好（影响所有 `TabPreference::Default` 的面板）。
///
/// 取值用 KT 序号：[`TabPreference::kt_index`] 不适用此法——这里直接接受枚举，
/// 由 [`default_tab_preference`] 还原。
pub fn set_default_tab_preference(pref: TabPreference) {
    let v = match pref {
        TabPreference::Default | TabPreference::Top | TabPreference::TopAlways => 0,
        TabPreference::Bottom | TabPreference::BottomAlways => 1,
    };
    TAB_DEFAULT.with(|c| c.set(v));
}

/// 当前全局标签条位置偏好。
pub fn default_tab_preference() -> TabPreference {
    if TAB_DEFAULT.with(|c| c.get()) == 1 {
        TabPreference::Bottom
    } else {
        TabPreference::Top
    }
}

/// 解析面板的标签条位置：`Default` 跟随 `settings`，其余用自身偏好。
fn resolved_tab_preference(settings: DockingSettings, id: PanelId) -> TabPreference {
    let own = dock_panel_info(id)
        .map(|i| i.options.tab_preference)
        .unwrap_or(TabPreference::Default);
    settings.resolve(own)
}

/// 沿「根 → 该面板」的 Split 路径递归，取**最后一个分叉方向**（离面板最近的边）：
/// 底部 → `Bottom`，其余三边按方向返回；中央/未找到 → `None`（走偏好兜底）。
/// 示例：根 H:左|右 → 右内 V:上|下 → 下内组 ⇒ Bottom；
/// 属性在右内 V 的上侧再 H 分叉 ⇒ Right（标签仍在上方）。
fn edge_in_tree(node: &DockNode, id: PanelId, current: Option<DockEdge>) -> Option<DockEdge> {
    match node {
        DockNode::Leaf { dockable_id, .. } if *dockable_id == id => current,
        DockNode::Tabs { tabs, .. } => {
            for child in tabs {
                if child.collect_ids().contains(&id) {
                    return edge_in_tree(child, id, current);
                }
            }
            None
        }
        DockNode::Split {
            orientation,
            first,
            second,
            ..
        } => {
            let in_first = first.collect_ids().contains(&id);
            let in_second = second.collect_ids().contains(&id);
            let next = if in_first {
                Some(match orientation {
                    // 已在底部区（外层 Vertical second）：区内左右拆分不改变「底边」判定，
                    // 否则标签会因为最内层是 Horizontal 而跳到内容上方。
                    SplitOrientation::Horizontal if current == Some(DockEdge::Bottom) => DockEdge::Bottom,
                    SplitOrientation::Horizontal => DockEdge::Left,
                    SplitOrientation::Vertical => DockEdge::Top,
                })
            } else if in_second {
                Some(match orientation {
                    SplitOrientation::Horizontal if current == Some(DockEdge::Bottom) => DockEdge::Bottom,
                    SplitOrientation::Horizontal => DockEdge::Right,
                    SplitOrientation::Vertical => DockEdge::Bottom,
                })
            } else {
                return None;
            };
            edge_in_tree(first, id, next).or_else(|| edge_in_tree(second, id, next))
        }
        _ => None,
    }
}

/// 面板当前停靠边（优先结构位置；隐藏面板用其隐藏边；浮动窗口搜其自身树）。
fn dock_edge_of(state: DockState, id: PanelId) -> Option<DockEdge> {
    let m = state.model();
    if let Some(root) = m.root.as_ref() {
        if let Some(e) = edge_in_tree(root, id, None) {
            return Some(e);
        }
    }
    for f in &m.floating {
        if let Some(e) = edge_in_tree(&f.root, id, None) {
            return Some(e);
        }
    }
    m.hidden
        .iter()
        .find(|h| h.active == id || h.items.contains(&id))
        .map(|h| h.edge)
}

/// 标签条位置：**优先按当前停靠边**（底部 → 内容下方；左/右/上 → 内容上方），
/// `tab_preference` 仅在结构无法判定时兜底（例如面板不在任何布局树中）。
/// 这样把原底部面板拖到侧边/顶部后，标签会立即跟着换到上方，不会保留初始偏好。
fn tab_bar_on_bottom(state: DockState, active: PanelId) -> bool {
    match dock_edge_of(state, active) {
        Some(DockEdge::Bottom) => true,
        Some(_) => false,
        None => resolved_tab_preference(state.settings(), active).is_bottom(),
    }
}

/// 单面板沿某轴的最小尺寸（未指定则取 `settings` 的全局值）。
fn leaf_min_along(id: PanelId, along_width: bool, settings: DockingSettings) -> f32 {
    match dock_panel_info(id).and_then(|i| i.options.min_size) {
        Some((w, h)) => {
            if along_width {
                w
            } else {
                h
            }
        }
        None => {
            if along_width {
                settings.min_pane_w
            } else {
                settings.min_pane_h
            }
        }
    }
}

/// 子树沿某轴所需的最小尺寸（对齐 KT `clampProportion` 的尺寸传播）。
///
/// 同轴分割累加、异轴分割取最大——嵌套分割时两侧的可压缩量不同。
fn min_extent_along(node: &DockNode, along_width: bool, settings: DockingSettings) -> f32 {
    match node {
        DockNode::Leaf { dockable_id, .. } => leaf_min_along(*dockable_id, along_width, settings),
        DockNode::Anchor { .. } => 0.0,
        DockNode::Tabs { tabs, .. } => tabs
            .iter()
            .map(|t| min_extent_along(t, along_width, settings))
            .fold(0.0, f32::max),
        DockNode::Split {
            orientation,
            first,
            second,
            ..
        } => {
            let a = min_extent_along(first, along_width, settings);
            let b = min_extent_along(second, along_width, settings);
            let same_axis = matches!(orientation, SplitOrientation::Horizontal) == along_width;
            if same_axis { a + b } else { a.max(b) }
        }
    }
}

/// `path` 所指分割节点两侧的钳制比例（对齐 KT `clampProportion`）。
///
/// 返回 `(lo, hi)`：`first` 侧占比的合法区间；空间不足以容纳两侧最小尺寸时
/// 退化为中点，保证区间始终合法且非空。
fn ratio_bounds(root: Option<&DockNode>, path: &[u8], along_width: bool, total: f32, settings: DockingSettings) -> (f32, f32) {
    let Some(node) = root.and_then(|r| split_at_path(r, path)) else {
        return (DOCK_RATIO_MIN, 1.0 - DOCK_RATIO_MIN);
    };
    let DockNode::Split { first, second, .. } = node else {
        return (DOCK_RATIO_MIN, 1.0 - DOCK_RATIO_MIN);
    };
    if total <= 0.0 {
        return (DOCK_RATIO_MIN, 1.0 - DOCK_RATIO_MIN);
    }
    let min_a = min_extent_along(first, along_width, settings);
    let min_b = min_extent_along(second, along_width, settings);
    let lo = (min_a / total).max(DOCK_RATIO_MIN);
    let hi = (1.0 - min_b / total).min(1.0 - DOCK_RATIO_MIN);
    if lo >= hi {
        // 两侧最小尺寸之和已超过可用空间：锁定在中点
        (0.5, 0.5)
    } else {
        (lo, hi)
    }
}

/// 测试辅助：当前视口下根水平分割两侧比例的合法区间。
pub fn dock_ratio_bounds_test(state: &DockState) -> (f32, f32) {
    let model = state.model();
    let (vw, _vh) = viewport_size();
    ratio_bounds(model.root.as_ref(), &[], true, vw, state.settings())
}

/// 沿 `path` 定位分割节点（`path` 为该节点自身路径，根为 `[]`）。
fn split_at_path<'a>(root: &'a DockNode, path: &[u8]) -> Option<&'a DockNode> {
    let mut cur = root;
    for &step in path {
        let DockNode::Split { first, second, .. } = cur else {
            return None;
        };
        cur = if step == 0 { first } else { second };
    }
    Some(cur)
}

/// 分配下一个节点 id。
fn next_node_id() -> DockNodeId {
    NODE_IDS.with(|g| g.borrow_mut().next())
}

impl DockNode {
    /// 单面板叶子（KT `Leaf`）—— 不带标签条的单面板停靠。
    pub fn leaf(dockable_id: PanelId) -> Self {
        DockNode::Leaf {
            id: next_node_id(),
            dockable_id,
        }
    }

    /// 由若干面板构造标签组（成员为叶子，选中第一个）。
    pub fn tabs(items: &[PanelId]) -> Self {
        DockNode::Tabs {
            id: next_node_id(),
            tabs: items.iter().map(|&p| DockNode::leaf(p)).collect(),
            selected_index: 0,
        }
    }

    /// 二分节点构造。
    pub fn split(
        orientation: SplitOrientation,
        proportion: f32,
        first: DockNode,
        second: DockNode,
    ) -> Self {
        DockNode::Split {
            id: next_node_id(),
            orientation,
            proportion: proportion.clamp(DOCK_RATIO_MIN, DOCK_RATIO_MAX),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// 命名锚点占位（KT `Anchor`）。
    pub fn anchor(anchor_id: AnchorId) -> Self {
        DockNode::Anchor {
            id: next_node_id(),
            anchor_id,
        }
    }

    /// 本节点 id。
    pub fn id(&self) -> DockNodeId {
        match self {
            DockNode::Leaf { id, .. }
            | DockNode::Split { id, .. }
            | DockNode::Tabs { id, .. }
            | DockNode::Anchor { id, .. } => *id,
        }
    }

    /// 全部面板 id（按树序）。
    pub fn collect_ids(&self) -> Vec<PanelId> {
        match self {
            DockNode::Leaf { dockable_id, .. } => vec![*dockable_id],
            DockNode::Tabs { tabs, .. } => {
                let mut v = Vec::new();
                for t in tabs {
                    v.extend(t.collect_ids());
                }
                v
            }
            DockNode::Split { first, second, .. } => {
                let mut v = first.collect_ids();
                v.extend(second.collect_ids());
                v
            }
            DockNode::Anchor { .. } => Vec::new(),
        }
    }

    /// 子树中第一个面板 id。
    pub fn first_id(&self) -> Option<PanelId> {
        match self {
            DockNode::Leaf { dockable_id, .. } => Some(*dockable_id),
            DockNode::Tabs { tabs, .. } => tabs.first().and_then(|t| t.first_id()),
            DockNode::Split { first, .. } => first.first_id(),
            DockNode::Anchor { .. } => None,
        }
    }

    /// 本节点作为「标签组」时的面板 id 列表（Leaf → 自身；Tabs → 成员）。
    pub fn group_panels(&self) -> Vec<PanelId> {
        match self {
            DockNode::Leaf { dockable_id, .. } => vec![*dockable_id],
            DockNode::Tabs { tabs, .. } => tabs.iter().flat_map(|t| t.collect_ids()).collect(),
            _ => Vec::new(),
        }
    }

    /// 本节点作为「标签组」时的当前面板。
    pub fn active_panel(&self) -> Option<PanelId> {
        match self {
            DockNode::Leaf { dockable_id, .. } => Some(*dockable_id),
            DockNode::Tabs {
                tabs,
                selected_index,
                ..
            } => tabs.get(*selected_index).and_then(|t| t.first_id()),
            _ => None,
        }
    }

    /// 按节点 id 查找子树节点。
    pub fn find_node(&self, id: DockNodeId) -> Option<&DockNode> {
        if self.id() == id {
            return Some(self);
        }
        match self {
            DockNode::Split { first, second, .. } => {
                first.find_node(id).or_else(|| second.find_node(id))
            }
            DockNode::Tabs { tabs, .. } => tabs.iter().find_map(|t| t.find_node(id)),
            _ => None,
        }
    }

    /// 按面板 id 查找其叶子节点。
    pub fn find_leaf(&self, panel: PanelId) -> Option<&DockNode> {
        match self {
            DockNode::Leaf { dockable_id, .. } if *dockable_id == panel => Some(self),
            DockNode::Split { first, second, .. } => {
                first.find_leaf(panel).or_else(|| second.find_leaf(panel))
            }
            DockNode::Tabs { tabs, .. } => tabs.iter().find_map(|t| t.find_leaf(panel)),
            _ => None,
        }
    }

    /// 按**节点 id** 查找子树节点（可变）。
    pub fn find_node_mut(&mut self, id: DockNodeId) -> Option<&mut DockNode> {
        if self.id() == id {
            return Some(self);
        }
        match self {
            DockNode::Split { first, second, .. } => first
                .find_node_mut(id)
                .or_else(|| second.find_node_mut(id)),
            DockNode::Tabs { tabs, .. } => tabs.iter_mut().find_map(|t| t.find_node_mut(id)),
            _ => None,
        }
    }

    /// 按锚点 id 查找占位节点。
    pub fn find_anchor(&self, anchor_id: AnchorId) -> Option<&DockNode> {
        match self {
            DockNode::Anchor {
                anchor_id: a, ..
            } if *a == anchor_id => Some(self),
            DockNode::Split { first, second, .. } => {
                first.find_anchor(anchor_id).or_else(|| second.find_anchor(anchor_id))
            }
            DockNode::Tabs { tabs, .. } => tabs.iter().find_map(|t| t.find_anchor(anchor_id)),
            _ => None,
        }
    }

    /// 选中的标签页（叶子）。
    pub fn selected_tab(&self) -> Option<&DockNode> {
        match self {
            DockNode::Tabs {
                tabs,
                selected_index,
                ..
            } => tabs.get(*selected_index),
            _ => None,
        }
    }

    /// 子树中是否含指定锚点。
    pub fn contains_anchor(&self, anchor_id: AnchorId) -> bool {
        match self {
            DockNode::Anchor {
                anchor_id: a, ..
            } => *a == anchor_id,
            DockNode::Split { first, second, .. } => {
                first.contains_anchor(anchor_id) || second.contains_anchor(anchor_id)
            }
            DockNode::Tabs { tabs, .. } => tabs.iter().any(|t| t.contains_anchor(anchor_id)),
            _ => false,
        }
    }
}

/// 由面板列表构造标签组，并选中指定面板（KT `Tabs(selectedIndex)` 语义）。
fn tabs_group(items: &[PanelId], active: PanelId) -> DockNode {
    let mut tabs: Vec<DockNode> = items.iter().map(|&p| DockNode::leaf(p)).collect();
    let selected_index = tabs
        .iter()
        .position(|t| t.collect_ids().contains(&active))
        .unwrap_or(0);
    if tabs.is_empty() {
        return DockNode::Tabs {
            id: next_node_id(),
            tabs,
            selected_index,
        };
    }
    DockNode::Tabs {
        id: next_node_id(),
        tabs,
        selected_index,
    }
}

/// 把「单叶子组」摊平成叶子（其它形态原样返回）——并入标签组时用。
fn as_leaf(node: DockNode) -> DockNode {
    match node {
        DockNode::Tabs { tabs, .. } if tabs.len() == 1 => tabs.into_iter().next().unwrap(),
        other => other,
    }
}

/// 从树中移除一个面板；标签组删空返回 `None`，`Split` 某侧空则提升兄弟节点
/// （自动消除多余分割层）。锚点占位不受影响。
fn remove_panel(node: DockNode, id: PanelId) -> Option<DockNode> {
    match node {
        DockNode::Leaf {
            id: nid,
            dockable_id,
        } => {
            if dockable_id == id {
                None
            } else {
                Some(DockNode::Leaf {
                    id: nid,
                    dockable_id,
                })
            }
        }
        DockNode::Tabs {
            id: nid,
            mut tabs,
            mut selected_index,
        } => {
            let before = tabs.len();
            tabs.retain(|t| !t.collect_ids().contains(&id));
            if tabs.is_empty() {
                return None;
            }
            if tabs.len() != before && selected_index >= tabs.len() {
                selected_index = 0;
            }
            Some(DockNode::Tabs {
                id: nid,
                tabs,
                selected_index,
            })
        }
        DockNode::Split {
            id: nid,
            orientation,
            proportion,
            first,
            second,
        } => {
            let a = remove_panel(*first, id);
            let b = remove_panel(*second, id);
            match (a, b) {
                (Some(a), Some(b)) => Some(DockNode::Split {
                    id: nid,
                    orientation,
                    proportion,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        }
        DockNode::Anchor { .. } => Some(node),
    }
}

/// 方向 → 二分节点。`WEST/EAST` 左右分割，`NORTH/SOUTH` 上下分割；
/// 记住 `proportion / 1-proportion` 的方向语义（新面板占 `proportion` 份额）。
fn make_split(
    existing: DockNode,
    new: DockNode,
    dir: DockDirection,
    proportion: f32,
) -> DockNode {
    let r = proportion.clamp(DOCK_RATIO_MIN, DOCK_RATIO_MAX);
    match dir {
        DockDirection::West => DockNode::split(SplitOrientation::Horizontal, r, new, existing),
        DockDirection::East => DockNode::split(SplitOrientation::Horizontal, 1.0 - r, existing, new),
        DockDirection::North => DockNode::split(SplitOrientation::Vertical, r, new, existing),
        DockDirection::South => DockNode::split(SplitOrientation::Vertical, 1.0 - r, existing, new),
        DockDirection::Center => {
            DockNode::split(SplitOrientation::Horizontal, 1.0 - r, existing, new)
        }
    }
}

/// 相对 `target` 插入 `new`；`CENTER` 并入目标标签组（目标为单面板时升级为标签组），
/// 其余方向建新分割。
fn insert_relative(
    node: DockNode,
    target: PanelId,
    new: DockNode,
    dir: DockDirection,
    proportion: f32,
) -> DockNode {
    let in_first = match &node {
        DockNode::Split { first, .. } => first.collect_ids().contains(&target),
        _ => node.collect_ids().contains(&target),
    };
    match node {
        DockNode::Leaf {
            id: nid,
            dockable_id,
        } if dockable_id == target => {
            if dir == DockDirection::Center {
                // 单面板升级为标签组：目标在前，新面板在后
                DockNode::Tabs {
                    id: nid,
                    tabs: vec![
                        DockNode::Leaf {
                            id: nid,
                            dockable_id,
                        },
                        as_leaf(new),
                    ],
                    selected_index: 0,
                }
            } else {
                make_split(
                    DockNode::Leaf {
                        id: nid,
                        dockable_id,
                    },
                    new,
                    dir,
                    proportion,
                )
            }
        }
        DockNode::Tabs {
            id: nid,
            mut tabs,
            selected_index,
        } if tabs.iter().any(|t| t.collect_ids().contains(&target)) => {
            if dir == DockDirection::Center {
                for t in as_leaf(new).group_panels() {
                    if !tabs.iter().any(|x| x.collect_ids().contains(&t)) {
                        tabs.push(DockNode::leaf(t));
                    }
                }
                DockNode::Tabs {
                    id: nid,
                    tabs,
                    selected_index,
                }
            } else {
                make_split(
                    DockNode::Tabs {
                        id: nid,
                        tabs,
                        selected_index,
                    },
                    new,
                    dir,
                    proportion,
                )
            }
        }
        DockNode::Split {
            id: nid,
            orientation,
            proportion: r,
            first,
            second,
        } => {
            if in_first {
                DockNode::Split {
                    id: nid,
                    orientation,
                    proportion: r,
                    first: Box::new(insert_relative(*first, target, new, dir, proportion)),
                    second,
                }
            } else {
                DockNode::Split {
                    id: nid,
                    orientation,
                    proportion: r,
                    first,
                    second: Box::new(insert_relative(*second, target, new, dir, proportion)),
                }
            }
        }
        other => make_split(other, new, dir, proportion),
    }
}

/// 按路径更新某一层分割比例（路径元素 `0 = first`、`1 = second`）。
/// 用路径而非面板 id，避免「同 id 出现在多层」时改错祖先。
fn set_ratio_by_path(node: &mut DockNode, path: &[u8], ratio: f32) {
    // `path` 寻址的是分割节点**自身**（与 render_splitter 传入的 splitter_path 一致）：
    // 根分割 = []，其 second 子分割 = [1]，再下一层 first = [1,0]。
    // 故须沿整条 path 走到目标节点再写比例；空 path 即当前节点。
    if let DockNode::Split {
        proportion: r,
        first,
        second,
        ..
    } = node
    {
        if path.is_empty() {
            *r = ratio.clamp(DOCK_RATIO_MIN, DOCK_RATIO_MAX);
            return;
        }
        let child = if path[0] == 0 { first } else { second };
        set_ratio_by_path(child, &path[1..], ratio);
    }
}

/// 读取指定路径分割节点的当前比例（Esc 取消拖拽时恢复原值用）。
fn ratio_by_path(node: &DockNode, path: &[u8]) -> Option<f32> {
    match node {
        DockNode::Split {
            proportion, first, second, ..
        } => {
            if path.is_empty() {
                Some(*proportion)
            } else {
                let child = if path[0] == 0 { first } else { second };
                ratio_by_path(child, &path[1..])
            }
        }
        _ => None,
    }
}

/// 按**节点 id** 更新分割比例（KT `setSplitProportion(nodeId, proportion)` 的寻址方式）。
fn set_proportion_by_node_id(node: &mut DockNode, node_id: DockNodeId, proportion: f32) -> bool {
    if node.id() == node_id {
        if let DockNode::Split { proportion: r, .. } = node {
            *r = proportion.clamp(DOCK_RATIO_MIN, DOCK_RATIO_MAX);
            return true;
        }
        return false;
    }
    match node {
        DockNode::Split { first, second, .. } => {
            set_proportion_by_node_id(first, node_id, proportion)
                || set_proportion_by_node_id(second, node_id, proportion)
        }
        DockNode::Tabs { tabs, .. } => tabs
            .iter_mut()
            .any(|t| set_proportion_by_node_id(t, node_id, proportion)),
        _ => false,
    }
}

/// 找到含该面板的「组」节点（标签组或单叶子）。
fn find_group(node: &DockNode, id: PanelId) -> Option<DockNode> {
    match node {
        DockNode::Leaf { dockable_id, .. } if *dockable_id == id => Some(node.clone()),
        DockNode::Leaf { .. } => None,
        DockNode::Tabs { .. } => {
            if node.collect_ids().contains(&id) {
                Some(node.clone())
            } else {
                None
            }
        }
        DockNode::Split { first, second, .. } => {
            find_group(first, id).or_else(|| find_group(second, id))
        }
        DockNode::Anchor { .. } => None,
    }
}

/// 选中某面板所在的标签组（KT `selectTab` 的语义，按面板 id）。
fn set_active(node: &mut DockNode, id: PanelId) {
    match node {
        DockNode::Tabs {
            tabs,
            selected_index,
            ..
        } => {
            if let Some(pos) = tabs.iter().position(|t| t.collect_ids().contains(&id)) {
                *selected_index = pos;
            }
        }
        DockNode::Split { first, second, .. } => {
            set_active(first, id);
            set_active(second, id);
        }
        _ => {}
    }
}

/// 在同一标签组内把 `id` 移到 `insert_index`（0..len）。
fn reorder_in_group(node: &mut DockNode, id: PanelId, insert_index: usize) {
    if let DockNode::Tabs { tabs, .. } = node {
        if let Some(from) = tabs.iter().position(|t| t.collect_ids().contains(&id)) {
            let mut v = tabs.clone();
            let item = v.remove(from);
            let to = insert_index.min(v.len());
            v.insert(to, item);
            *tabs = v;
        }
    } else if let DockNode::Split { first, second, .. } = node {
        reorder_in_group(first, id, insert_index);
        reorder_in_group(second, id, insert_index);
    }
}

/// 记录面板在树中**直接父分割**的位置（兄弟 + 方向 + 比例），供浮动/自动隐藏后精确恢复。
///
/// 必须递归到包含 `id` 的最深分割：旧实现只要 `id` 落在某个 Split 的 second 分支，
/// 就直接拿 `first.first_id()` 当兄弟。嵌套布局（IDE 的「左导航 / 中文档 / 右属性」）
/// 因此会把最右面板恢复到最左兄弟旁边，看起来像「跑到中间」。
fn find_tree_position(node: &DockNode, id: PanelId) -> Option<SavedTreePosition> {
    let DockNode::Split {
        orientation,
        proportion,
        first,
        second,
        ..
    } = node
    else {
        return None;
    };
    // 只有「直接包含 id 的组 / 叶子 / 锚点」的另一侧才是真正的兄弟；
    // 若这一侧本身还是 Split，id 位于更深子树，必须继续递归下去。
    let is_direct = |n: &DockNode| {
        matches!(
            n,
            DockNode::Tabs { .. } | DockNode::Leaf { .. } | DockNode::Anchor { .. }
        )
    };
    if first.collect_ids().contains(&id) {
        if is_direct(first) {
            if let Some(sibling) = second.first_id() {
                return Some(SavedTreePosition {
                    sibling,
                    direction: if *orientation == SplitOrientation::Horizontal {
                        DockDirection::West
                    } else {
                        DockDirection::North
                    },
                    ratio: *proportion,
                });
            }
        }
        return find_tree_position(first, id);
    }
    if second.collect_ids().contains(&id) {
        if is_direct(second) {
            if let Some(sibling) = first.first_id() {
                return Some(SavedTreePosition {
                    sibling,
                    direction: if *orientation == SplitOrientation::Horizontal {
                        DockDirection::East
                    } else {
                        DockDirection::South
                    },
                    ratio: 1.0 - *proportion,
                });
            }
        }
        return find_tree_position(second, id);
    }
    None
}

/// 几何单一真源：给定矩形与分割节点，算出两个子矩形（渲染/预览/命中一致）。
pub fn dock_split_bounds(
    orientation: SplitOrientation,
    ratio: f32,
    rect: DockRect,
) -> (DockRect, DockRect) {
    let (x, y, w, h) = rect;
    let sp = docking_theme().splitter_width;
    match orientation {
        SplitOrientation::Horizontal => {
            let fw = (w * ratio - sp / 2.0).max(0.0);
            let sw = (w * (1.0 - ratio) - sp / 2.0).max(0.0);
            ((x, y, fw, h), (x + fw + sp, y, sw, h))
        }
        SplitOrientation::Vertical => {
            let fh = (h * ratio - sp / 2.0).max(0.0);
            let sh = (h * (1.0 - ratio) - sp / 2.0).max(0.0);
            ((x, y, w, fh), (x, y + fh + sp, w, sh))
        }
    }
}

fn point_in(rect: DockRect, x: f32, y: f32) -> bool {
    x >= rect.0 && x <= rect.0 + rect.2 && y >= rect.1 && y <= rect.1 + rect.3
}

/// 5 区判定：先上下后左右，角落归 N/S；中心为合并标签。
pub fn dock_direction_in_rect(rect: DockRect, x: f32, y: f32) -> DockDirection {
    if rect.2 <= 0.0 || rect.3 <= 0.0 {
        return DockDirection::Center;
    }
    let rx = (x - rect.0) / rect.2;
    let ry = (y - rect.1) / rect.3;
    let t = DOCK_ZONE_THRESHOLD;
    if ry < t {
        DockDirection::North
    } else if ry > 1.0 - t {
        DockDirection::South
    } else if rx < t {
        DockDirection::West
    } else if rx > 1.0 - t {
        DockDirection::East
    } else {
        DockDirection::Center
    }
}

/// 拖放预览矩形（目标面板的半区或整体）。
pub fn dock_preview_rect(rect: DockRect, dir: DockDirection) -> DockRect {
    let (x, y, w, h) = rect;
    match dir {
        DockDirection::North => (x, y, w, h / 2.0),
        DockDirection::South => (x, y + h / 2.0, w, h / 2.0),
        DockDirection::West => (x, y, w / 2.0, h),
        DockDirection::East => (x + w / 2.0, y, w / 2.0, h),
        DockDirection::Center => rect,
    }
}

// ══════════════════════════════ 序列化 ══════════════════════════════

/// 树 → 文本（节点带稳定 id）：
///
/// * `L(<nodeId>,<dockableId>)` 单面板叶子；
/// * `S(<nodeId>,<H|V>,<proportion>,<child1>,<child2>)` 二分节点；
/// * `T(<nodeId>,<selectedIndex>,<leaf1>,<leaf2>,…)` 标签组；
/// * `A(<nodeId>,<anchorId>)` 命名锚点占位。
///
/// 解析端仍兼容旧格式 `T(id1,id2|active)` / `S(H,ratio,c1,c2)`（无节点 id）。
pub fn serialize_dock_node(node: &DockNode) -> String {
    match node {
        DockNode::Leaf {
            id,
            dockable_id,
        } => format!("L({id},{dockable_id})"),
        DockNode::Split {
            id,
            orientation,
            proportion,
            first,
            second,
        } => format!(
            "S({id},{},{:.4},{},{})",
            orientation.tag(),
            proportion,
            serialize_dock_node(first),
            serialize_dock_node(second)
        ),
        DockNode::Tabs {
            id,
            tabs,
            selected_index,
        } => {
            let inner = tabs
                .iter()
                .map(serialize_dock_node)
                .collect::<Vec<_>>()
                .join(",");
            format!("T({id},{selected_index},{inner})")
        }
        DockNode::Anchor { id, anchor_id } => format!("A({id},{anchor_id})"),
    }
}

fn find_close_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        if i < open {
            continue;
        }
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn split_top_level(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// 解析一个节点，返回（节点, 消耗长度）。
fn parse_node(s: &str) -> Option<(DockNode, usize)> {
    // 单面板叶子：L(<nodeId>,<dockableId>)
    if s.starts_with("L(") {
        let close = find_close_paren(s, 1)?;
        let parts = split_top_level(&s[2..close]);
        if parts.len() != 2 {
            return None;
        }
        let id: DockNodeId = parts[0].parse().ok()?;
        let dockable_id: PanelId = parts[1].parse().ok()?;
        return Some((
            DockNode::Leaf {
                id,
                dockable_id,
            },
            close + 1,
        ));
    }
    // 命名锚点：A(<nodeId>,<anchorId>)
    if s.starts_with("A(") {
        let close = find_close_paren(s, 1)?;
        let parts = split_top_level(&s[2..close]);
        if parts.len() != 2 {
            return None;
        }
        let id: DockNodeId = parts[0].parse().ok()?;
        let anchor_id: AnchorId = parts[1].parse().ok()?;
        return Some((
            DockNode::Anchor { id, anchor_id },
            close + 1,
        ));
    }
    // 标签组：T(<nodeId>,<selectedIndex>,<leaf…>)；旧格式 T(id1,…|active) 亦兼容。
    if s.starts_with("T(") {
        let close = find_close_paren(s, 1)?;
        let inner = &s[2..close];
        if let Some((ids_s, active_s)) = inner.split_once('|') {
            // 旧格式：无节点 id 的面板列表 + 激活面板
            let items: Vec<PanelId> = ids_s
                .split(',')
                .filter_map(|v| v.parse().ok())
                .collect();
            if items.is_empty() {
                return None;
            }
            let active: PanelId = active_s.parse().ok()?;
            let tabs: Vec<DockNode> = items.iter().map(|&p| DockNode::leaf(p)).collect();
            let selected_index = items.iter().position(|&p| p == active).unwrap_or(0);
            return Some((
                DockNode::Tabs {
                    id: next_node_id(),
                    tabs,
                    selected_index,
                },
                close + 1,
            ));
        }
        let parts = split_top_level(inner);
        if parts.len() < 2 {
            return None;
        }
        let id: DockNodeId = parts[0].parse().ok()?;
        let selected_index: usize = parts[1].parse().ok()?;
        let mut tabs = Vec::new();
        for p in &parts[2..] {
            let (child, _) = parse_node(p)?;
            tabs.push(child);
        }
        if tabs.is_empty() {
            return None;
        }
        let selected_index = selected_index.min(tabs.len() - 1);
        return Some((
            DockNode::Tabs {
                id,
                tabs,
                selected_index,
            },
            close + 1,
        ));
    }
    // 二分节点：S(<nodeId>,<H|V>,<proportion>,<c1>,<c2>)；旧格式 S(H,ratio,c1,c2) 兼容。
    if s.starts_with("S(") {
        let close = find_close_paren(s, 1)?;
        let parts = split_top_level(&s[2..close]);
        let (id, orientation, proportion, first, second) = match parts.len() {
            5 => {
                let id: DockNodeId = parts[0].parse().ok()?;
                let orientation = SplitOrientation::from_tag(parts[1])?;
                let proportion: f32 = parts[2].parse().ok()?;
                let (first, _) = parse_node(parts[3])?;
                let (second, _) = parse_node(parts[4])?;
                (id, orientation, proportion, first, second)
            }
            4 => {
                // 旧格式：无节点 id
                let orientation = SplitOrientation::from_tag(parts[0])?;
                let proportion: f32 = parts[1].parse().ok()?;
                let (first, _) = parse_node(parts[2])?;
                let (second, _) = parse_node(parts[3])?;
                (next_node_id(), orientation, proportion, first, second)
            }
            _ => return None,
        };
        return Some((
            DockNode::Split {
                id,
                orientation,
                proportion: proportion.clamp(DOCK_RATIO_MIN, DOCK_RATIO_MAX),
                first: Box::new(first),
                second: Box::new(second),
            },
            close + 1,
        ));
    }
    None
}

/// 文本 → 树；格式错误返回 `None`（不静默吞异常，仅返回布尔结果）。
///
/// 注意：本函数解析的是**单棵布局树**，不是「布局」——布局（窗口列表）的解析见
/// [`parse_dock_layout_text`](super::dock_layout::parse_dock_layout_text)。
/// [`parse_dock_node`] 是语义更清晰的别名。
pub fn parse_dock_layout(s: &str) -> Option<DockNode> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (node, used) = parse_node(s)?;
    if used == s.len() {
        Some(node)
    } else {
        None
    }
}

/// [`parse_dock_layout`] 的语义别名：解析**单棵布局树**（不是窗口列表）。
pub fn parse_dock_node(s: &str) -> Option<DockNode> {
    parse_dock_layout(s)
}

/// 只保留仍注册的面板，空标签组删除、空分割提升兄弟；锚点占位恒保留。
fn prune_node(node: DockNode, valid: &dyn Fn(PanelId) -> bool) -> Option<DockNode> {
    match node {
        DockNode::Leaf {
            id,
            dockable_id,
        } => {
            if valid(dockable_id) {
                Some(DockNode::Leaf {
                    id,
                    dockable_id,
                })
            } else {
                None
            }
        }
        DockNode::Tabs {
            id,
            mut tabs,
            mut selected_index,
        } => {
            let before = tabs.len();
            tabs.retain(|t| t.collect_ids().iter().all(|&i| valid(i)));
            if tabs.is_empty() {
                return None;
            }
            if tabs.len() != before && selected_index >= tabs.len() {
                selected_index = 0;
            }
            Some(DockNode::Tabs {
                id,
                tabs,
                selected_index,
            })
        }
        DockNode::Anchor { .. } => Some(node),
        DockNode::Split {
            id,
            orientation,
            proportion,
            first,
            second,
        } => {
            let a = prune_node(*first, valid);
            let b = prune_node(*second, valid);
            match (a, b) {
                (Some(a), Some(b)) => Some(DockNode::Split {
                    id,
                    orientation,
                    proportion,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        }
    }
}

// ══════════════════════════════ 反序列化解析器 ══════════════════════════════

/// 旧面板 id → 新面板 id 的映射（对齐 KT `DockableResolver`）。
///
/// 返回 `None` 表示该叶子无法解析，随之从树中丢弃；空掉的容器逐层收敛
/// （与 [`prune_node`] 对未注册面板的处理同构）。
pub type DockableResolver<'a> = dyn FnMut(PanelId) -> Option<PanelId> + 'a;

/// 按解析器重写树内所有叶子 id。
fn remap_ids(node: DockNode, resolve: &mut DockableResolver) -> Option<DockNode> {
    match node {
        DockNode::Leaf { id, dockable_id } => resolve(dockable_id).map(|new_id| DockNode::Leaf {
            id,
            dockable_id: new_id,
        }),
        DockNode::Tabs {
            id,
            tabs,
            selected_index,
        } => {
            let before = tabs.len();
            let tabs: Vec<DockNode> = tabs
                .into_iter()
                .filter_map(|t| remap_ids(t, resolve))
                .collect();
            if tabs.is_empty() {
                return None;
            }
            let selected_index =
                if tabs.len() != before && selected_index >= tabs.len() {
                    0
                } else {
                    selected_index
                };
            Some(DockNode::Tabs {
                id,
                tabs,
                selected_index,
            })
        }
        DockNode::Anchor { .. } => Some(node),
        DockNode::Split {
            id,
            orientation,
            proportion,
            first,
            second,
        } => {
            match (
                remap_ids(*first, resolve),
                remap_ids(*second, resolve),
            ) {
                (Some(a), Some(b)) => Some(DockNode::Split {
                    id,
                    orientation,
                    proportion,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        }
    }
}

/// 重映射**单个容器**内的面板 id：去重只在本容器内进行。
///
/// 同一面板会合法地同时出现在 `root` 与 `saved_before_maximize`（最大化快照），
/// 也会出现在 hidden / floating 的恢复信息里。若跨容器共享 `seen`，往返后
/// 最大化快照中会丢掉仍停靠在主窗口里的面板。
fn remap_container(node: DockNode, resolve: &mut DockableResolver) -> Option<DockNode> {
    let mut seen = std::collections::HashSet::new();
    let mut map = |id: PanelId| resolve(id).filter(|n| seen.insert(*n) && dock_panel_exists(*n));
    remap_ids(node, &mut map)
}

// ══════════════════════════════ 状态模型 ══════════════════════════════

/// 恢复位置：兄弟面板 + 相对方向 + 当时的分割比例。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SavedTreePosition {
    pub sibling: PanelId,
    pub direction: DockDirection,
    pub ratio: f32,
}

/// 自动隐藏到某边缘的整组面板。
#[derive(Clone, Debug)]
pub struct HiddenPanel {
    pub edge: DockEdge,
    pub items: Vec<PanelId>,
    pub active: PanelId,
    pub saved: Option<SavedTreePosition>,
    /// 隐藏前的完整主树。仅靠 `saved.sibling` 恢复会在嵌套布局里递归到错误层级
    /// （最右面板被塞进文档区），故内存态优先整树还原；不参与序列化与相等性比较。
    pub tree_before: Option<DockNode>,
}

impl PartialEq for HiddenPanel {
    fn eq(&self, other: &Self) -> bool {
        self.edge == other.edge
            && self.items == other.items
            && self.active == other.active
            && self.saved == other.saved
    }
}

/// 浮动窗口（对齐 KT `DockWindow(kind=Floating)`：与主窗口**同构**的布局树）。
///
/// `id` 是该浮动窗口的 [`WindowId`](super::dock_layout::WindowId)，
/// 与 KT 的浮动窗口模型一一对应（一个浮动组 = 一个浮动窗口）。
/// 布局树与主窗口一致支持分割嵌套——跨窗口停靠因此可以落边区域
/// （在浮动窗口内分割），而不是只能并标签。
#[derive(Clone, PartialEq, Debug)]
pub struct FloatingPanel {
    /// 浮动窗口 id（自 1 递增，`0` 留给主窗口）。
    pub id: WindowId,
    /// 浮动窗口布局树（非空；单面板时为单成员标签组）。
    pub root: DockNode,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub saved: Option<SavedTreePosition>,
}

impl FloatingPanel {
    /// 由扁平标签组构造（兼容旧模型/测试/存档路径）。
    pub fn from_items(
        id: WindowId,
        items: Vec<PanelId>,
        active: PanelId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        saved: Option<SavedTreePosition>,
    ) -> Self {
        let root = tabs_group(&items, active);
        FloatingPanel { id, root, x, y, w, h, saved }
    }

    /// 组内全部面板 id。
    pub fn items(&self) -> Vec<PanelId> {
        self.root.collect_ids()
    }

    /// 活动面板（无选中时取第一个）。
    pub fn active(&self) -> PanelId {
        self.root
            .active_panel()
            .or_else(|| self.root.first_id())
            .unwrap_or(0)
    }

    /// 是否包含某面板。
    pub fn contains(&self, id: PanelId) -> bool {
        self.root.collect_ids().contains(&id)
    }
}

/// 下一个可用的浮动窗口 id（自 1 递增，`0` 留给主窗口）。
fn next_window_id(floating: &[FloatingPanel]) -> WindowId {
    floating
        .iter()
        .map(|f| f.id)
        .max()
        .unwrap_or(WINDOW_MAIN)
        .max(WINDOW_MAIN)
        + 1
}

/// 全部停靠状态（存在单个 `MutableState` 里，值整体替换即触发重组）。
#[derive(Clone, PartialEq, Debug)]
pub struct DockModel {
    pub root: Option<DockNode>,
    pub hidden: Vec<HiddenPanel>,
    pub floating: Vec<FloatingPanel>,
    pub maximized: Option<PanelId>,
    pub saved_before_maximize: Option<DockNode>,
    /// 自动隐藏面板的滑出预览（peek）。
    pub expanded: Option<PanelId>,
}

impl DockModel {
    pub fn new(root: DockNode) -> Self {
        DockModel {
            root: Some(root),
            hidden: Vec::new(),
            floating: Vec::new(),
            maximized: None,
            saved_before_maximize: None,
            expanded: None,
        }
    }

    pub fn empty() -> Self {
        DockModel {
            root: None,
            hidden: Vec::new(),
            floating: Vec::new(),
            maximized: None,
            saved_before_maximize: None,
            expanded: None,
        }
    }

    /// 布局树 + hidden + floating 中所有面板 id。
    pub fn all_ids(&self) -> Vec<PanelId> {
        let mut v = self.root.as_ref().map(|r| r.collect_ids()).unwrap_or_default();
        for h in &self.hidden {
            v.extend(h.items.iter().copied());
        }
        for f in &self.floating {
            v.extend(f.items());
        }
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// 停靠事件（对齐参考实现的 13 种事件，含其未接线的 `Hidden/Pinned/Unpinned`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockingEvent {
    /// 停靠完成（对齐 KT `Docked(dockableId, windowId, isTemporary)`）。
    Docked(PanelId, WindowId, bool),
    /// 从停靠位置移除（`is_temporary` 标记「移动」产生的临时摘除）。
    Undocked(PanelId, bool),
    Shown(PanelId),
    Hidden(PanelId),
    Pinned(PanelId),
    Unpinned(PanelId),
    Maximized(PanelId),
    /// 最大化被还原（对齐 KT `MaximizeRestored`）。
    MaximizeRestored(PanelId),
    AutoHidden(PanelId, DockEdge),
    AutoShown(PanelId),
    Floated(PanelId),
    Unfloated(PanelId),
    Closed(PanelId),
    /// 浮动窗口被加入布局（对齐 KT `WindowOpened`）。
    WindowOpened(WindowId),
    /// 浮动窗口被移出布局（对齐 KT `WindowClosed`）。
    WindowClosed(WindowId),
    /// 布局整体变更（持久化 / 命名布局恢复等，对齐 KT `LayoutChanged`）。
    LayoutChanged(LayoutChangeKind),
}

/// 布局变更种类（对齐 KT `LayoutChangeKind`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutChangeKind {
    Added,
    Removed,
    Restored,
    Persisted,
}

thread_local! {
    static DOCK_EVENTS: RefCell<Vec<DockingEvent>> = const { RefCell::new(Vec::new()) };
}

fn fire(ev: DockingEvent) {
    DOCK_EVENTS.with(|q| q.borrow_mut().push(ev));
}

/// 取出并清空自上次调用以来派发的停靠事件。
pub fn drain_dock_events() -> Vec<DockingEvent> {
    DOCK_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

// ══════════════════════════════ 面板注册表 ══════════════════════════════

/// 面板描述（不含内容闭包：闭包不可 `Clone`/`PartialEq`，另存注册表）。
#[derive(Clone, PartialEq, Debug)]
pub struct DockablePanel {
    pub title: String,
    /// 是否允许自动隐藏（Rust 扩展；KT 无对应概念）。
    pub auto_hide_allowed: bool,
    /// 是否允许最大化 / 还原（Rust 扩展；KT 对应 `DockableOptions.maximizable`）。
    pub min_max_allowed: bool,
    /// 引擎约束（对齐 KT `DockableOptions`）：可关闭性、可浮起性、参与哪些分割区域、
    /// 是否锁定当前窗口、标签条位置偏好、所属锚点。
    pub options: DockableOptions,
}

impl Default for DockablePanel {
    fn default() -> Self {
        DockablePanel {
            title: String::new(),
            auto_hide_allowed: true,
            min_max_allowed: true,
            options: DockableOptions::default(),
        }
    }
}

impl DockablePanel {
    pub fn new(title: impl Into<String>) -> Self {
        DockablePanel {
            title: title.into(),
            ..Default::default()
        }
    }

    /// 带引擎约束的面板（对齐 KT `DockableSpec.options`）。
    pub fn with_options(title: impl Into<String>, options: DockableOptions) -> Self {
        DockablePanel {
            title: title.into(),
            options,
            ..Default::default()
        }
    }
}

struct PanelEntry {
    info: DockablePanel,
    content: Box<dyn Fn()>,
}

thread_local! {
    static PANEL_SEED: Cell<u64> = const { Cell::new(0xD0C0_0000) };
    static PANELS: RefCell<HashMap<PanelId, PanelEntry>> = RefCell::new(HashMap::new());
    /// 本趟组合登记的标签组矩形（NodeId 每趟刷新，拖拽释放时按真实几何查询）。
    static GROUP_RECTS: RefCell<Vec<GroupRect>> = const { RefCell::new(Vec::new()) };
    /// 本趟组合登记的每个标签头节点（用于标签重排插入点判定）。
    static TAB_RECTS: RefCell<Vec<TabRect>> = const { RefCell::new(Vec::new()) };
    /// 本趟组合登记的标签条节点（VS：拖到标签条 = 插入为标签）。
    static TAB_BARS: RefCell<Vec<TabBarRect>> = const { RefCell::new(Vec::new()) };
    /// DockLayout 根节点（窗口绝对矩形）。
    static ROOT_NODE: Cell<Option<NodeId>> = const { Cell::new(None) };
    /// 当前 hover 的分割条路径散列（0 = 无）。
    static HOVER_SPLIT: Cell<u64> = const { Cell::new(0) };
    /// 当前 hover 的标签头（绘制闭包只读此 Cell，禁止在绘制里读 MutableState）。
    static HOVER_TAB: Cell<PanelId> = const { Cell::new(0) };
    /// 当前 hover 的自动隐藏边条标签。
    static HOVER_STRIP: Cell<PanelId> = const { Cell::new(0) };
    /// 当前 hover 的浮动窗缩放热区（NodeId 散列）。
    static HOVER_FLOAT_EDGE: Cell<u64> = const { Cell::new(0) };
    /// 浮动窗正在被拖动/缩放（整窗ghosting 半透明反馈）。
    static FLOAT_GHOST: Cell<bool> = const { Cell::new(false) };
    /// 浮动窗 ghosting 覆盖层节点（每次组合刷新；改 FLOAT_GHOST 时失效它而非重组）。
    static FLOAT_GHOST_NODE: Cell<Option<NodeId>> = const { Cell::new(None) };
    /// 当前组合中的停靠状态句柄（Esc 等全局快捷键关闭浮层用）。
    static ACTIVE_DOCK: Cell<Option<DockState>> = const { Cell::new(None) };
    /// 双击检测：上次按压的面板与时间（ns）。
    static LAST_HEADER_PRESS: Cell<(PanelId, f64)> = const { Cell::new((0, -1.0)) };
    /// 分割条双击检测：上次点击的路径散列与时间（ns）。
    static LAST_SPLIT_CLICK: Cell<(u64, f64)> = const { Cell::new((0, -1.0)) };
}

#[derive(Clone)]
struct GroupRect {
    ids: Vec<PanelId>,
    active: PanelId,
    node: NodeId,
}

#[derive(Clone, Copy)]
struct TabRect {
    id: PanelId,
    node: NodeId,
}

#[derive(Clone)]
struct TabBarRect {
    ids: Vec<PanelId>,
    active: PanelId,
    node: NodeId,
}

/// 注册一个可停靠面板，返回稳定的 [`PanelId`]。同一 id 重复注册会覆盖内容与描述。
pub fn register_dock_panel(info: DockablePanel, content: impl Fn() + 'static) -> PanelId {
    let id = PANEL_SEED.with(|s| {
        let v = s.get() + 1;
        s.set(v);
        v
    });
    PANELS.with(|p| {
        p.borrow_mut().insert(
            id,
            PanelEntry {
                info,
                content: Box::new(content),
            },
        );
    });
    id
}

/// 注销面板（内容闭包随之释放）。
pub fn unregister_dock_panel(id: PanelId) {
    PANELS.with(|p| {
        p.borrow_mut().remove(&id);
    });
}

/// 清空面板注册表（切换页面 / 测试隔离）。
pub fn clear_dock_panels() {
    PANELS.with(|p| p.borrow_mut().clear());
}

pub fn dock_panel_exists(id: PanelId) -> bool {
    PANELS.with(|p| p.borrow().contains_key(&id))
}

pub fn dock_panel_info(id: PanelId) -> Option<DockablePanel> {
    PANELS.with(|p| p.borrow().get(&id).map(|e| e.info.clone()))
}

/// 组合某个面板的内容。take-put：执行期间不持有注册表借用，
/// 内容闭包里也可以安全地再次访问注册表 / 发起组合。
pub fn call_dock_panel(id: PanelId) {
    let taken = PANELS.with(|p| {
        p.borrow_mut()
            .get_mut(&id)
            .map(|e| std::mem::replace(&mut e.content, Box::new(|| {})))
    });
    if let Some(f) = taken {
        f();
        PANELS.with(|p| {
            if let Some(e) = p.borrow_mut().get_mut(&id) {
                e.content = f;
            }
        });
    }
}

fn panel_title(id: PanelId) -> String {
    dock_panel_info(id)
        .map(|i| i.title)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| format!("Panel {id}"))
}

// ══════════════════════════════ 状态句柄 ══════════════════════════════

/// 拖拽期间的 UI 反馈（全部 `Copy`，存于 arena state）。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DockDragUi {
    pub source: Option<PanelId>,
    pub start_x: f32,
    pub start_y: f32,
    pub x: f32,
    pub y: f32,
    pub moved: bool,
    pub target: Option<PanelId>,
    pub direction: Option<DockDirection>,
    pub target_rect: Option<DockRect>,
    /// 停靠容器（DockLayout 内容区）的真实矩形，判定期缓存、渲染期读取。
    /// 不含标题栏/菜单栏；渲染期不能现读 ROOT_NODE（此刻刚 add 尚未 place）。
    pub container_rect: DockRect,
    pub root_direction: Option<DockDirection>,
    /// 命中的 VS 指南（`None` = 悬停处不可投放）。
    pub guide: Option<DockGuide>,
    /// 标签条插入光标位置 `(x, bar_rect)`，仅 `Tab` 指南时有值。
    pub caret: Option<(f32, DockRect)>,
}

impl DockDragUi {
    pub fn idle() -> Self {
        DockDragUi {
            source: None,
            start_x: 0.0,
            start_y: 0.0,
            x: 0.0,
            y: 0.0,
            moved: false,
            target: None,
            direction: None,
            target_rect: None,
            container_rect: (0.0, 0.0, 0.0, 0.0),
            root_direction: None,
            guide: None,
            caret: None,
        }
    }
}

/// 右键上下文菜单状态。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DockContextMenu {
    pub panel: Option<PanelId>,
    pub x: f32,
    pub y: f32,
}

impl DockContextMenu {
    fn idle() -> Self {
        DockContextMenu {
            panel: None,
            x: 0.0,
            y: 0.0,
        }
    }
}

/// 标签条「全部面板」下拉状态：`(当前活动面板, 屏幕 x, 屏幕 y)`。
/// 与 `Tabs` 控件的「更多」下拉同语义：列出该标签组全部面板，点选切换。
pub type DockTabDropdown = Option<(PanelId, f32, f32)>;

/// 停靠状态句柄（全字段 `Copy`，可安全被 `'static` 手势闭包捕获）。
#[derive(Clone, Copy)]
pub struct DockState {
    model: MutableState<DockModel>,
    drag: MutableState<DockDragUi>,
    menu: MutableState<DockContextMenu>,
    tab_dropdown: MutableState<DockTabDropdown>,
    hover_tab: MutableState<PanelId>,
    settings: MutableState<DockingSettings>,
    /// 最近聚焦的面板（KT `DockState.activeDockableId`）。
    active: MutableState<Option<PanelId>>,
    /// 具名布局快照（KT `NamedLayouts`）。
    named: MutableState<HashMap<String, DockModel>>,
}

/// 在组合外（测试 / 初始化）创建状态句柄。
pub fn dock_state(initial: DockModel) -> DockState {
    let seed_active = initial.root.as_ref().and_then(|r| r.first_id());
    DockState {
        model: mutableStateOf(initial),
        drag: mutableStateOf(DockDragUi::idle()),
        menu: mutableStateOf(DockContextMenu::idle()),
        tab_dropdown: mutableStateOf(None),
        hover_tab: mutableStateOf(0),
        settings: mutableStateOf(DockingSettings {
            default_tab_preference: default_tab_preference(),
            ..DockingSettings::default()
        }),
        active: mutableStateOf(seed_active),
        named: mutableStateOf(HashMap::new()),
    }
}

/// 组合期创建并记住状态句柄（跨重组保持）。
pub fn remember_dock_state(initial: DockModel) -> DockState {
    remember(move || dock_state(initial))
}

/// 组合期创建状态句柄，并用 `dock_layout {}` DSL 的声明结果作初始布局
/// （对齐 KT `rememberDockState { mainWindow { … } }`）。
pub fn remember_dock_state_in(
    layout: &DockLayoutSpec,
    settings: DockingSettings,
) -> DockState {
    let state = remember_dock_state(layout.to_model());
    state.set_settings(settings);
    state
}

impl DockState {
    fn mutate(&self, f: impl FnOnce(&mut DockModel)) {
        self.model.update(f);
    }

    /// 读快照（组合期调用会订阅，状态变化即重组）。
    pub fn model(&self) -> DockModel {
        self.model.with_value(|m| m.clone())
    }

    pub fn read<R>(&self, f: impl FnOnce(&DockModel) -> R) -> R {
        self.model.with_value(f)
    }

    /// 持久化视图：当前状态的**窗口列表**（对齐 KT `DockState.layout`）。
    ///
    /// 这是 KT 语义的「唯一真源」——布局 = 主窗口 + 浮动窗口列表；
    /// 拖拽中、菜单、边条 peek 等瞬时 UI 状态不在其中。
    pub fn layout(&self) -> DockLayout {
        DockLayout::from_model(&self.model())
    }

    /// 用一份窗口布局替换当前状态（对齐 KT「计算整份下一份布局再赋值」）。
    ///
    /// 自动隐藏边条等 Rust 扩展状态保持不变。
    pub fn set_layout(&self, layout: &DockLayout) {
        self.mutate(|m| layout.apply_to(m));
    }

    pub fn drag(&self) -> DockDragUi {
        self.drag.get()
    }

    pub fn menu(&self) -> DockContextMenu {
        self.menu.get()
    }

    /// 打开标签条「▼ 全部面板」下拉（也可由 View 菜单等处程序化触发）。
    /// `lead` 为标签组的活动面板，`(x, y)` 为浮层左上角的窗口坐标。
    pub fn open_tab_dropdown(&self, lead: PanelId, x: f32, y: f32) {
        self.tab_dropdown.set(Some((lead, x, y)));
    }

    /// 关闭标签条下拉。
    pub fn close_tab_dropdown(&self) {
        self.tab_dropdown.set(None);
    }

    /// 当前标签条下拉状态。
    pub fn tab_dropdown(&self) -> DockTabDropdown {
        self.tab_dropdown.get()
    }

    pub fn is_docked(&self, id: PanelId) -> bool {
        self.read(|m| m.root.as_ref().is_some_and(|r| r.collect_ids().contains(&id)))
    }

    pub fn is_hidden(&self, id: PanelId) -> bool {
        self.read(|m| m.hidden.iter().any(|h| h.items.contains(&id)))
    }

    pub fn is_floating(&self, id: PanelId) -> bool {
        self.read(|m| m.floating.iter().any(|f| f.contains(id)))
    }

    pub fn is_maximized(&self, id: PanelId) -> bool {
        self.read(|m| m.maximized == Some(id))
    }

    // ──────────── 设置 / 活动面板 / 显隐（KT `DockingSettings`·`activeDockableId`·`show`）

    /// 当前停靠设置（组合期读取会订阅：改设置即重渲染）。
    pub fn settings(&self) -> DockingSettings {
        self.settings.get()
    }

    /// 替换整份设置。
    pub fn set_settings(&self, s: DockingSettings) {
        self.settings.set(s);
    }

    /// 链式设置（构造期使用）。
    pub fn with_settings(self, s: DockingSettings) -> Self {
        self.set_settings(s);
        self
    }

    /// 最近聚焦的面板（KT `activeDockableId`）。
    ///
    /// 记录的面板已关闭或从未打开时，回落到布局树里的第一个面板。
    pub fn active_dockable(&self) -> Option<PanelId> {
        let hint = self.active.with_value(|s| *s);
        match hint {
            Some(id) if self.is_open(id) => Some(id),
            _ => self.read(|m| m.root.as_ref().and_then(|r| r.first_id())),
        }
    }

    /// 显式设置活动面板；面板未打开时忽略。
    pub fn set_active_dockable(&self, id: PanelId) {
        if self.is_open(id) && self.active.with_value(|s| *s) != Some(id) {
            self.active.set(Some(id));
        }
    }

    /// 该面板能否浮起为独立窗口（KT `canFloat`：已注册且 `options.floatable`）。
    pub fn can_float(&self, id: PanelId) -> bool {
        dock_panel_info(id)
            .map(|i| i.options.floatable)
            .unwrap_or(false)
    }

    /// 该面板能否关闭（KT `canClose`：已注册且 `options.closable`）。
    pub fn can_close(&self, id: PanelId) -> bool {
        dock_panel_info(id)
            .map(|i| i.options.closable)
            .unwrap_or(false)
    }

    /// 显示面板（KT `show`）：已在布局中→置活动；自动隐藏→钉回原位；
    /// 浮动中→置活动；未打开→插入主窗口（有锚点则落锚点，否则根级居中）。
    pub fn show(&self, id: PanelId) {
        if dock_panel_info(id).is_none() {
            return;
        }
        if self.is_hidden(id) {
            self.restore_from_edge(id);
            self.set_active_dockable(id);
            return;
        }
        if self.is_open(id) {
            self.set_active_dockable(id);
            return;
        }
        let anchor = dock_panel_info(id).and_then(|i| i.options.anchor);
        let lead = anchor.and_then(|a| {
            self.read(|m| {
                m.root
                    .as_ref()
                    .and_then(|r| r.find_anchor(a))
                    .and_then(|n| n.active_panel().or_else(|| n.first_id()))
            })
        });
        self.dock(id, lead, DockDirection::Center, 0.3);
        fire(DockingEvent::Shown(id));
        self.set_active_dockable(id);
    }

    /// 隐藏面板（KT `hide`）：从布局中摘下但保留注册（可用 `show` 再显示）。
    pub fn hide(&self, id: PanelId) {
        if !self.is_open(id) {
            return;
        }
        self.undock(id);
    }

    // ──────────── 窗口级操作（KT `setWindowBounds`·`closeWindow`·`restoreWindow`）

    /// 设置某窗口的位置与尺寸（KT `setWindowBounds`）。主窗口无边界，忽略。
    pub fn set_window_bounds(&self, window: WindowId, bounds: WindowBounds) {
        if window == WINDOW_MAIN {
            return;
        }
        self.mutate(|m| {
            if let Some(f) = m.floating.iter_mut().find(|f| f.id == window) {
                f.x = bounds.x;
                f.y = bounds.y;
                f.w = bounds.width;
                f.h = bounds.height;
            }
        });
    }

    /// 读取某窗口边界（主窗口返回 `None`）。
    pub fn window_bounds(&self, window: WindowId) -> Option<WindowBounds> {
        self.read(|m| {
            m.floating.iter().find(|f| f.id == window).map(|f| WindowBounds {
                x: f.x,
                y: f.y,
                width: f.w,
                height: f.h,
            })
        })
    }

    /// 全部窗口 id（主窗口恒在首位，对齐 KT `layout.windows`）。
    pub fn window_ids(&self) -> Vec<WindowId> {
        let mut out = vec![WINDOW_MAIN];
        out.extend(self.read(|m| m.floating.iter().map(|f| f.id).collect::<Vec<_>>()));
        out
    }

    /// 关闭浮动窗口：其中每个面板按 `saved` 位置回到主窗口（无处可回则丢弃）。
    pub fn close_window(&self, window: WindowId) {
        if window == WINDOW_MAIN {
            return;
        }
        let back = self.read(|m| {
            m.floating
                .iter()
                .find(|f| f.id == window)
                .map(|f| (f.root.collect_ids(), f.active(), f.saved.clone()))
        });
        let Some((items, active, saved)) = back else {
            return;
        };
        if items.is_empty() {
            return;
        }
        self.mutate(|m| {
            let Some(pos) = m.floating.iter().position(|f| f.id == window) else {
                return;
            };
            m.floating.remove(pos);
            let group = tabs_group(&items, active);
            Self::insert_into_root(m, group, saved);
        });
        fire(DockingEvent::WindowClosed(window));
        for pid in items {
            fire(DockingEvent::Docked(pid, WINDOW_MAIN, false));
        }
    }

    /// 把一棵子树插回主窗口根（优先按 `saved` 原位）。
    fn insert_into_root(m: &mut DockModel, group: DockNode, saved: Option<SavedTreePosition>) {
        let root = m.root.take();
        m.root = Some(match (root, saved) {
            (Some(r), Some(s)) if r.collect_ids().contains(&s.sibling) => {
                insert_relative(r, s.sibling, group, s.direction, s.ratio)
            }
            (Some(r), _) => match r.first_id() {
                Some(t) => insert_relative(r, t, group, DockDirection::Center, 0.3),
                None => group,
            },
            (None, _) => group,
        });
    }

    /// 取消某窗口里面板的最大化（KT `restore(windowId)`）。
    ///
    /// 最大化只有一份全局记录：若活动最大化面板属于 `window` 则还原，否则不动。
    pub fn restore_window(&self, window: WindowId) {
        let maximized = self.read(|m| m.maximized);
        if let Some(id) = maximized {
            if self.window_of(id) == Some(window) {
                self.restore();
            }
        }
    }

    // ──────────── 具名布局 / 快照（KT `NamedLayouts`·`captureLayout`）

    /// 内存快照：整份停靠状态（含浮动窗口与自动隐藏）。
    pub fn capture(&self) -> DockModel {
        self.model()
    }

    /// 恢复内存快照（对齐 KT `restoreLayout`），并派发 `LayoutChanged`。
    pub fn restore_capture(&self, snapshot: &DockModel) {
        self.model.set(snapshot.clone());
        fire(DockingEvent::LayoutChanged(LayoutChangeKind::Restored));
    }

    /// 具名布局操作入口（KT `state.layouts`）。
    pub fn layouts(&self) -> NamedLayouts {
        NamedLayouts { state: *self }
    }

    /// 从隐藏组 / 浮动组里移除单个面板（空组删除）。
    fn remove_from_aux(m: &mut DockModel, id: PanelId) {
        for h in &mut m.hidden {
            h.items.retain(|&x| x != id);
            if h.active == id {
                h.active = h.items.first().copied().unwrap_or(0);
            }
        }
        m.hidden.retain(|h| !h.items.is_empty());
        for f in &mut m.floating {
            let taken = std::mem::replace(&mut f.root, DockNode::leaf(0));
            if let Some(nr) = remove_panel(taken, id) {
                f.root = nr;
            }
        }
        m.floating.retain(|f| !f.root.collect_ids().is_empty());
        if m.maximized == Some(id) {
            m.maximized = None;
            m.saved_before_maximize = None;
        }
        if m.expanded == Some(id) {
            m.expanded = None;
        }
    }

    fn detach(m: &mut DockModel, id: PanelId) {
        if let Some(r) = m.root.take() {
            m.root = remove_panel(r, id);
        }
        Self::remove_from_aux(m, id);
    }

    /// 把 `id` 从当前位置摘下，按 `target + direction` 重新停靠。
    /// `target = None` 时按方向停靠到根级。
    pub fn dock(
        &self,
        id: PanelId,
        target: Option<PanelId>,
        direction: DockDirection,
        ratio: f32,
    ) {
        self.dock_group(DockNode::tabs(&[id]), id, target, direction, ratio);
    }

    /// 把 `group` 插入窗口根槽的 `target` 相对位置（主窗口与浮动窗口共用）。
    fn insert_group(
        root_slot: &mut Option<DockNode>,
        group: DockNode,
        target: Option<PanelId>,
        direction: DockDirection,
        ratio: f32,
    ) {
        *root_slot = Some(match root_slot.take() {
            Some(r) => {
                let t = target.filter(|t| r.collect_ids().contains(t));
                match t {
                    Some(t) => insert_relative(r, t, group, direction, ratio),
                    None => {
                        if direction == DockDirection::Center {
                            match r.first_id() {
                                Some(t) => {
                                    insert_relative(r, t, group, DockDirection::Center, ratio)
                                }
                                None => group,
                            }
                        } else {
                            make_split(r, group, direction, ratio)
                        }
                    }
                }
            }
            None => group,
        });
    }

    fn dock_group(
        &self,
        group: DockNode,
        lead: PanelId,
        target: Option<PanelId>,
        direction: DockDirection,
        ratio: f32,
    ) {
        self.dock_group_into(group, lead, target, WINDOW_MAIN, direction, ratio);
    }

    /// 把 `group` 停靠进指定窗口（`WINDOW_MAIN` 或某个浮动窗口）。
    ///
    /// 跨窗口停靠使浮动窗口成为完整的停靠目标：中心并入标签组，
    /// 边区域在浮动窗口内分割。目标窗口不存在时回落主窗口根级。
    fn dock_group_into(
        &self,
        group: DockNode,
        lead: PanelId,
        target: Option<PanelId>,
        window: WindowId,
        direction: DockDirection,
        ratio: f32,
    ) {
        self.mutate(|m| {
            for gid in group.collect_ids() {
                Self::detach(m, gid);
            }
            if window == WINDOW_MAIN {
                Self::insert_group(&mut m.root, group, target, direction, ratio);
            } else if let Some(f) = m.floating.iter_mut().find(|f| f.id == window) {
                let mut slot = Some(std::mem::replace(&mut f.root, DockNode::leaf(0)));
                Self::insert_group(&mut slot, group, target, direction, ratio);
                f.root = slot.unwrap_or_else(|| DockNode::leaf(lead));
            } else {
                // 目标窗口已消失：回落主窗口根级
                Self::insert_group(&mut m.root, group, None, direction, ratio);
            }
            fire(DockingEvent::Undocked(lead, true));
            fire(DockingEvent::Docked(lead, window, true));
        });
    }

    /// 关闭面板（先从树/隐藏/浮动中摘除；事件顺序对齐参考：`Closed` 先于 `Undocked`）。
    pub fn close(&self, id: PanelId) {
        self.mutate(|m| {
            fire(DockingEvent::Closed(id));
            Self::detach(m, id);
            fire(DockingEvent::Undocked(id, false));
        });
    }

    /// 切换标签组内的活动面板。
    pub fn switch_tab(&self, id: PanelId) {
        self.mutate(|m| {
            if let Some(r) = m.root.as_mut() {
                set_active(r, id);
            }
            for f in &mut m.floating {
                if f.contains(id) {
                    set_active(&mut f.root, id);
                }
            }
            for h in &mut m.hidden {
                if h.items.contains(&id) {
                    h.active = id;
                }
            }
            fire(DockingEvent::Shown(id));
        });
        self.active.set(Some(id));
    }

    /// 组内标签重排（`insert_index` 为移除自身后的插入位置）。
    pub fn reorder_tab(&self, id: PanelId, insert_index: usize) {
        self.mutate(|m| {
            if let Some(r) = m.root.as_mut() {
                reorder_in_group(r, id, insert_index);
            }
            for f in &mut m.floating {
                if f.contains(id) {
                    reorder_in_group(&mut f.root, id, insert_index);
                }
            }
        });
    }

    /// 更新指定路径上的分割比例（分割条拖拽）。
    pub fn set_split_ratio(&self, path: &[u8], ratio: f32) {
        let p = path.to_vec();
        self.mutate(|m| {
            if let Some(r) = m.root.as_mut() {
                set_ratio_by_path(r, &p, ratio);
            }
        });
    }

    /// 当前悬停的标签头（0 = 无）；VS 的「悬停才显示关闭按钮」由此驱动。
    pub fn hover_tab(&self) -> PanelId {
        self.hover_tab.get()
    }

    pub fn set_hover_tab(&self, id: PanelId) {
        if self.hover_tab.get() != id {
            self.hover_tab.set(id);
        }
    }

    /// 作为标签插入到 `lead` 所在标签组的 `index` 位（VS 拖到标签条的语义）。
    pub fn dock_as_tab(&self, id: PanelId, lead: PanelId, index: usize) {
        self.dock_group(DockNode::tabs(&[id]), id, Some(lead), DockDirection::Center, 0.5);
        self.reorder_tab(id, index);
    }

    /// `id` 所在标签组的全部成员（主窗口 / 浮动窗口）。
    pub fn group_of(&self, id: PanelId) -> Vec<PanelId> {
        self.read(|m| {
            m.root
                .as_ref()
                .and_then(|r| find_group(r, id).map(|g| g.collect_ids()))
                .or_else(|| {
                    m.floating
                        .iter()
                        .find(|f| f.contains(id))
                        .and_then(|f| find_group(&f.root, id).map(|g| g.collect_ids()))
                })
                .unwrap_or_default()
        })
    }

    /// VS「关闭其他」：关掉同组除 `id` 外的全部面板。
    pub fn close_others(&self, id: PanelId) {
        for other in self.group_of(id) {
            if other != id && dock_panel_info(other).is_some_and(|p| p.options.closable) {
                self.close(other);
            }
        }
    }

    /// VS「全部关闭」：关掉同组所有可关闭面板。
    pub fn close_all(&self, id: PanelId) {
        for other in self.group_of(id) {
            if dock_panel_info(other).is_some_and(|p| p.options.closable) {
                self.close(other);
            }
        }
    }

    /// 把整个浮动窗口回靠到主窗口（拖拽浮动标题栏落到指南上时调用）。
    pub fn redock_floating(
        &self,
        window: WindowId,
        target: Option<PanelId>,
        direction: DockDirection,
        ratio: f32,
    ) {
        self.mutate(|m| {
            let Some(pos) = m.floating.iter().position(|f| f.id == window) else {
                return;
            };
            let f = m.floating.remove(pos);
            let lead = f.active();
            let group = f.root;
            for gid in group.collect_ids() {
                Self::detach(m, gid);
            }
            Self::insert_group(&mut m.root, group, target, direction, ratio);
            fire(DockingEvent::Undocked(lead, true));
            fire(DockingEvent::Docked(lead, WINDOW_MAIN, true));
        });
    }

    /// 最大化：保存整树并把根替换为该面板。
    pub fn maximize(&self, id: PanelId) {
        self.mutate(|m| {
            if m.maximized.is_some() {
                return;
            }
            m.saved_before_maximize = m.root.clone();
            m.root = Some(DockNode::tabs(&[id]));
            m.maximized = Some(id);
            fire(DockingEvent::Maximized(id));
        });
    }

    /// 还原最大化前的整树。
    pub fn restore(&self) {
        self.mutate(|m| {
            let Some(id) = m.maximized.take() else { return };
            if let Some(saved) = m.saved_before_maximize.take() {
                m.root = Some(saved);
            }
            fire(DockingEvent::MaximizeRestored(id));
        });
    }

    pub fn toggle_maximize(&self, id: PanelId) {
        if self.is_maximized(id) {
            self.restore();
        } else {
            self.maximize(id);
        }
    }

    /// 整组自动隐藏到边缘，记录恢复位置。
    pub fn hide_to_edge(&self, id: PanelId, edge: DockEdge) {
        self.mutate(|m| {
            let Some(root) = m.root.as_ref() else { return };
            let Some(group) = find_group(root, id) else { return };
            // 保留原始方向与比例（与 `float_panel` 一致）：`edge` 只决定边条停靠位置，
            // 不该改写恢复位置——否则面板会回到错误的一侧或使用错误比例。
            let saved = find_tree_position(root, id);
            let items = group.collect_ids();
            let active = group.active_panel().unwrap_or(id);
            let tree_before = root.clone(); // detach 前拷贝：之后 m 需要可变借用
            for gid in &items {
                Self::detach(m, *gid);
            }
            m.hidden.push(HiddenPanel {
                edge,
                items,
                active,
                saved,
                tree_before: Some(tree_before),
            });
            fire(DockingEvent::AutoHidden(id, edge));
            fire(DockingEvent::Hidden(id));
        });
    }

    /// 从边缘恢复整组（按 `SavedTreePosition` 恢复原位置与尺寸）。
    pub fn restore_from_edge(&self, id: PanelId) {
        self.mutate(|m| {
            let Some(pos) = m.hidden.iter().position(|h| h.items.contains(&id)) else {
                return;
            };
            let h = m.hidden.remove(pos);
            // peek 的 expanded 记录的是触发边条的 tab（通常是组内第一个），而恢复
            // 传入的是当前点击的 tab（在 peek 内 switch_tab 后可能已换人）。只按
            // 单个 id 比较会漏清：残留的 expanded 在 render_peek 里找不到 hidden
            // 组，fallback 成左侧单面板 peek —— 即「恢复后输出浮在文档区左侧」。
            if m.expanded.map(|p| h.items.contains(&p)).unwrap_or(false) {
                m.expanded = None;
            }
            let group = tabs_group(&h.items, h.active);
            let root = m.root.take();
            // 优先整树还原：只靠 saved.sibling 会在嵌套布局里递归进错误层级
            // （最右属性面板被塞进文档区）。仅当当前树恰好等于「原树移除本组」时
            // 才整树还原，避免覆盖隐藏期间发生的其它停靠改动。
            let exact = h.tree_before.as_ref().filter(|before| {
                let expected: Vec<PanelId> = before
                    .collect_ids()
                    .into_iter()
                    .filter(|id| !h.items.contains(id))
                    .collect();
                let now = root.as_ref().map(|r| r.collect_ids()).unwrap_or_default();
                now.len() == expected.len() && expected.iter().all(|id| now.contains(id))
            });
            m.root = Some(if let Some(before) = exact {
                before.clone()
            } else {
                match (root, h.saved) {
                    (Some(r), Some(s)) if r.collect_ids().contains(&s.sibling) => {
                        insert_relative(r, s.sibling, group, s.direction, s.ratio)
                    }
                    (Some(r), _) => match r.first_id() {
                        Some(t) => insert_relative(r, t, group, DockDirection::Center, 0.3),
                        None => group,
                    },
                    (None, _) => group,
                }
            });
            fire(DockingEvent::Pinned(id));
            for gid in h.items {
                fire(DockingEvent::Docked(gid, WINDOW_MAIN, false));
            }
        });
    }

    /// 自动隐藏开关：已隐藏则钉回，否则按面板中心选最近边缘隐藏。
    pub fn toggle_auto_hide(&self, id: PanelId) {
        if self.is_hidden(id) {
            self.restore_from_edge(id);
            return;
        }
        let model = self.model();
        let edge = nearest_edge_for(id, model.root.as_ref());
        self.hide_to_edge(id, edge);
        fire(DockingEvent::Unpinned(id));
    }

    /// 悬停边条时滑出预览。
    pub fn expand_from_edge(&self, id: PanelId) {
        self.mutate(|m| {
            if m.expanded != Some(id) {
                m.expanded = Some(id);
                fire(DockingEvent::AutoShown(id));
            }
        });
    }

    pub fn collapse_to_edge(&self) {
        self.mutate(|m| {
            m.expanded = None;
        });
    }

    /// 整组浮起为窗口（参考实现：同标签组作为一个单元一起浮起）。
    ///
    /// 面板不可浮起（[`Self::can_float`] 为假）时不做任何事。
    pub fn float_panel(&self, id: PanelId) {
        if !self.can_float(id) {
            return;
        }
        self.mutate(|m| {
            let Some(root) = m.root.as_ref() else { return };
            let Some(group) = find_group(root, id) else { return };
            let saved = find_tree_position(root, id);
            let items = group.collect_ids();
            let active = group.active_panel().unwrap_or(id);
            for gid in &items {
                Self::detach(m, *gid);
            }
            let n = m.floating.len() as f32;
            m.floating.push(FloatingPanel {
                id: next_window_id(&m.floating),
                root: tabs_group(&items, active),
                x: 100.0 + n * 24.0,
                y: 100.0 + n * 24.0,
                w: 400.0,
                h: 300.0,
                saved,
            });
            fire(DockingEvent::Floated(id));
        });
    }

    /// 浮动窗口回靠到保存的位置。
    pub fn unfloat_panel(&self, id: PanelId) {
        self.mutate(|m| {
            let Some(pos) = m.floating.iter().position(|f| f.contains(id)) else {
                return;
            };
            let f = m.floating.remove(pos);
            let group = f.root.clone();
            let items = f.root.collect_ids();
            let root = m.root.take();
            m.root = Some(match (root, f.saved) {
                (Some(r), Some(s)) if r.collect_ids().contains(&s.sibling) => {
                    insert_relative(r, s.sibling, group, s.direction, s.ratio)
                }
                (Some(r), _) => match r.first_id() {
                    Some(t) => insert_relative(r, t, group, DockDirection::Center, 0.3),
                    None => group,
                },
                (None, _) => group,
            });
            fire(DockingEvent::Unfloated(id));
            for gid in items {
                fire(DockingEvent::Docked(gid, WINDOW_MAIN, false));
            }
        });
    }

    pub fn toggle_float(&self, id: PanelId) {
        if self.is_floating(id) {
            self.unfloat_panel(id);
        } else {
            self.float_panel(id);
        }
    }

    pub fn set_floating_rect(&self, id: PanelId, x: f32, y: f32, w: f32, h: f32) {
        self.mutate(|m| {
            if let Some(f) = m.floating.iter_mut().find(|f| f.contains(id)) {
                f.x = x;
                f.y = y;
                f.w = w.max(DOCK_FLOATING_MIN_W);
                f.h = h.max(DOCK_FLOATING_MIN_H);
            }
        });
    }

    /// 布局树序列化字符串。
    pub fn save_layout(&self) -> String {
        self.read(|m| m.root.as_ref().map(serialize_dock_node).unwrap_or_default())
    }

    /// 恢复布局树；只接受仍注册的面板，返回是否成功。
    pub fn restore_layout(&self, text: &str) -> bool {
        let Some(node) = parse_dock_layout(text) else {
            return false;
        };
        let Some(node) = prune_node(node, &dock_panel_exists) else {
            return false;
        };
        self.mutate(|m| {
            m.root = Some(node);
            m.maximized = None;
            m.saved_before_maximize = None;
            m.expanded = None;
        });
        true
    }

    /// 完整状态（树 / 最大化 / 隐藏 / 浮动）。格式头 `mdl2`。
    ///
    /// `mdl2` 的浮动行携带**窗口 id 与整棵布局树**（字段用 `;` 分隔——树文本里含 `|`）：
    /// `float=wid;x,y,w,h;savedSib;savedDir;savedRatio;TREE`。
    /// 读取仍兼容 `mdl1`（旧浮动行是扁平标签组）。
    pub fn save_full_state(&self) -> String {
        Self::encode_model(&self.model())
    }

    /// 任意模型 -> `mdl2` 文本（不读写运行时状态；命名布局持久化复用此入口）。
    fn encode_model(m: &DockModel) -> String {
        let mut s = String::from("mdl2\n");
        s.push_str("tree=");
        s.push_str(&m.root.as_ref().map(serialize_dock_node).unwrap_or_default());
        s.push('\n');
        s.push_str("max=");
        s.push_str(&m.maximized.map(|v| v.to_string()).unwrap_or_else(|| "-".into()));
        s.push('\n');
        s.push_str("savedMax=");
        s.push_str(
            &m.saved_before_maximize
                .as_ref()
                .map(serialize_dock_node)
                .unwrap_or_default(),
        );
        s.push('\n');
        for h in &m.hidden {
            s.push_str(&format!(
                "hidden={}|{}|{}|{:.4}|{}|{}\n",
                h.edge.index(),
                opt_u64(h.saved.map(|p| p.sibling as i64)),
                h.saved.map(|p| p.direction.index() as i64).unwrap_or(-1),
                h.saved.map(|p| p.ratio).unwrap_or(0.2),
                join_ids(&h.items),
                h.active,
            ));
        }
        for f in &m.floating {
            s.push_str(&format!(
                "float={};{:.1},{:.1},{:.1},{:.1};{};{};{:.4};{}\n",
                f.id,
                f.x,
                f.y,
                f.w,
                f.h,
                opt_u64(f.saved.map(|p| p.sibling as i64)),
                f.saved.map(|p| p.direction.index() as i64).unwrap_or(-1),
                f.saved.map(|p| p.ratio).unwrap_or(0.3),
                serialize_dock_node(&f.root),
            ));
        }
        s
    }

    /// 恢复完整状态；格式/数据非法返回 `false`（不改动现有状态）。
    ///
    /// 接受 `mdl2`（浮动行携带窗口 id + 整棵树）与旧 `mdl1`（扁平标签组，
    /// 窗口 id 按出现顺序重排）。
    pub fn restore_full_state(&self, text: &str) -> bool {
        self.restore_full_state_with(text, &mut |id| Some(id))
    }

    /// 恢复完整状态，并让序列化里的面板 id 经过解析器（对齐 KT `DockableResolver`）。
    ///
    /// 典型用法：应用升级后面板 id 变化，把旧 id 映射到新注册的面板；解析为 `None`
    /// 的叶子（以及收敛后空掉的容器）整体丢弃。
    pub fn restore_full_state_with(
        &self,
        text: &str,
        resolve: &mut dyn FnMut(PanelId) -> Option<PanelId>,
    ) -> bool {
        let Some(mut m) = Self::decode_model_with(text, resolve) else {
            return false;
        };
        self.mutate(|mm| std::mem::swap(mm, &mut m));
        self.drag.set(DockDragUi::idle());
        self.menu.set(DockContextMenu::idle());
        self.tab_dropdown.set(None);
        fire(DockingEvent::LayoutChanged(LayoutChangeKind::Restored));
        true
    }

    /// 文本 -> 模型；旧 id 经 `resolve` 重映射后仍按注册表剪枝（未注册一律丢弃）。
    pub fn decode_model_with(
        text: &str,
        resolve: &mut dyn FnMut(PanelId) -> Option<PanelId>,
    ) -> Option<DockModel> {
        let mut m = Self::decode_model_raw(text)?;
        {
            let resolve = &mut *resolve;
            // 去重按容器独立：同一面板会合法地同时存在于 root 与最大化快照 / 浮窗中。
            m.root = m
                .root
                .take()
                .and_then(|n| remap_container(n, &mut *resolve))
                .and_then(|n| prune_node(n, &dock_panel_exists));
            m.saved_before_maximize = m
                .saved_before_maximize
                .take()
                .and_then(|n| remap_container(n, &mut *resolve));
            for h in &mut m.hidden {
                let mut seen = std::collections::HashSet::new();
                let mut map = |id: PanelId| {
                    resolve(id).filter(|n| seen.insert(*n) && dock_panel_exists(*n))
                };
                h.items = h.items.iter().copied().filter_map(&mut map).collect::<Vec<_>>();
                if !h.items.is_empty() && !h.items.contains(&h.active) {
                    h.active = h.items[0];
                }
            }
            m.hidden.retain(|h| !h.items.is_empty());
            let mut keep: Vec<FloatingPanel> = Vec::new();
            for f in m.floating.drain(..) {
                if let Some(root) = remap_container(f.root.clone(), &mut *resolve) {
                    if !root.collect_ids().is_empty() {
                        let mut f = f;
                        f.root = root;
                        keep.push(f);
                    }
                }
            }
            m.floating = keep;
        }
        if m.root.is_none() && m.floating.is_empty() && m.hidden.is_empty() {
            return None;
        }
        m.maximized = m
            .maximized
            .filter(|id| m.root.as_ref().is_some_and(|r| r.collect_ids().contains(id)));
        Some(m)
    }

    /// 文本 -> 模型（只做语法解析；注册表剪枝与 id 重映射留给 `decode_model_with`）。
    fn decode_model_raw(text: &str) -> Option<DockModel> {
        let mut lines = text.trim().lines();
        let version = match lines.next() {
            Some("mdl2") => 2,
            Some("mdl1") => 1,
            _ => return None,
        };
        let mut tree: Option<DockNode> = None;
        let mut max: Option<PanelId> = None;
        let mut saved_max: Option<DockNode> = None;
        let mut hidden: Vec<HiddenPanel> = Vec::new();
        let mut floating: Vec<FloatingPanel> = Vec::new();
        for line in lines {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k {
                // 此处只做文本解析，不剪枝：未注册面板要留给 decode_model_with 先经
                // resolver 映射（升级后旧 id → 新面板），映射后再统一 prune。
                "tree" => tree = parse_dock_layout(v),
                "max" => max = v.parse().ok(),
                "savedMax" => saved_max = parse_dock_layout(v),
                "hidden" => {
                    let p: Vec<&str> = v.split('|').collect();
                    if p.len() >= 6 {
                        if let (Ok(edge), Ok(active)) =
                            (p[0].parse::<u8>(), p[5].parse::<PanelId>())
                        {
                            if let Some(edge) = DockEdge::from_index(edge) {
                                let items = parse_ids(p[4]);
                                if !items.is_empty() {
                                    hidden.push(HiddenPanel {
                                        edge,
                                        items,
                                        active,
                                        saved: parse_saved(p.get(1), p.get(2), p.get(3)),
                                        tree_before: None,
                                    });
                                }
                            }
                        }
                    }
                }
                "float" => {
                    if version >= 2 {
                        // mdl2：wid;x,y,w,h;sib;dir;ratio;TREE（树文本含 `|`，故用 `;` 分隔）
                        let p: Vec<&str> = v.split(';').collect();
                        if p.len() >= 6 {
                            let wid: WindowId = p[0].parse().unwrap_or(0);
                            let nums: Vec<f32> = p[1]
                                .split(',')
                                .filter_map(|s| s.parse().ok())
                                .collect();
                            let tree_text = p[5..].join(";");
                            if nums.len() == 4 {
                                if let Some(node) = parse_dock_layout(&tree_text)
                                    .and_then(|n| prune_node(n, &dock_panel_exists))
                                {
                                    if !node.collect_ids().is_empty() {
                                        floating.push(FloatingPanel {
                                            id: if wid == 0 {
                                                next_window_id(&floating)
                                            } else {
                                                wid
                                            },
                                            root: node,
                                            x: nums[0],
                                            y: nums[1],
                                            w: nums[2].max(DOCK_FLOATING_MIN_W),
                                            h: nums[3].max(DOCK_FLOATING_MIN_H),
                                            saved: parse_saved(p.get(2), p.get(3), p.get(4)),
                                        });
                                    }
                                }
                            }
                        }
                    } else {
                        // mdl1：x|y|w|h|sib|dir|ratio|items|active（扁平标签组）
                        let p: Vec<&str> = v.split('|').collect();
                        if p.len() >= 9 {
                            let nums: Vec<f32> = p[..4]
                                .iter()
                                .filter_map(|s| s.parse().ok())
                                .collect();
                            if nums.len() == 4 {
                                if let Ok(active) = p[8].parse::<PanelId>() {
                                    let items = parse_ids(p[7]);
                                    if !items.is_empty() {
                                        floating.push(FloatingPanel::from_items(
                                            next_window_id(&floating),
                                            items,
                                            active,
                                            nums[0],
                                            nums[1],
                                            nums[2].max(DOCK_FLOATING_MIN_W),
                                            nums[3].max(DOCK_FLOATING_MIN_H),
                                            parse_saved(p.get(4), p.get(5), p.get(6)),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Some(DockModel {
            root: tree,
            hidden,
            floating,
            maximized: max,
            saved_before_maximize: saved_max,
            expanded: None,
        })
    }

    /// 重置为给定布局，清空隐藏/浮动/最大化。
    pub fn reset(&self, root: DockNode) {
        self.mutate(|m| *m = DockModel::new(root));
        self.drag.set(DockDragUi::idle());
        self.menu.set(DockContextMenu::idle());
        self.tab_dropdown.set(None);
    }

    // ══════════════ KT 对齐操作层（compose-docking `state/DockState.kt`） ══════════════

    /// 停靠到指定目标（对齐 KT `DockState.dock(dockableId, target, region, proportion)`）。
    ///
    /// `proportion` 为 `None` 时用 KT 默认值：窗口根停靠 `0.25`、相对其它停靠项 `0.5`。
    /// 返回是否真的发生了停靠（目标已消失 / 风格不允许时返回 `false`）。
    pub fn dock_to(
        &self,
        id: PanelId,
        target: DockTarget,
        region: DockRegion,
        proportion: Option<f32>,
    ) -> bool {
        if !self.can_dock(id, target, region) {
            return false;
        }
        let ratio = proportion.unwrap_or(match target {
            DockTarget::Root { .. } => 0.25,
            _ => 0.5,
        });
        // 解析目标 → (具体面板, 区域, 目标窗口)；浮动窗口是完整停靠目标（KT 语义）
        let (target_panel, target_region, target_window) = match target {
            DockTarget::OnDockable(p) => (Some(p), region, self.window_of(p).unwrap_or(WINDOW_MAIN)),
            DockTarget::OnNode { window_id, node_id } => {
                let found = self.read(|m| {
                    let tree: Option<&DockNode> = if window_id == WINDOW_MAIN {
                        m.root.as_ref()
                    } else {
                        m.floating.iter().find(|f| f.id == window_id).map(|f| &f.root)
                    };
                    tree.and_then(|r| r.find_node(node_id)).and_then(|n| n.first_id())
                });
                (found, region, window_id)
            }
            DockTarget::Root { window_id } => (None, region, window_id),
            DockTarget::Anchor(anchor_id) => {
                // 锚点：优先落到占位节点上，其次并入共享该锚点的停靠项
                let found = self.read(|m| {
                    m.root.as_ref().and_then(|r| {
                        r.find_anchor(anchor_id)
                            .and_then(|n| n.first_id())
                            .or_else(|| {
                                r.collect_ids().into_iter().find(|&p| {
                                    dock_panel_info(p)
                                        .and_then(|i| i.options.anchor)
                                        .map(|a| a == anchor_id)
                                        .unwrap_or(false)
                                })
                            })
                    })
                });
                match found {
                    Some(p) => (Some(p), region, WINDOW_MAIN),
                    // 没有占位也没有成员 → 退化为窗口根停靠
                    None => {
                        self.dock(id, None, region, ratio);
                        return true;
                    }
                }
            }
        };
        if target_window == WINDOW_MAIN {
            self.dock(id, target_panel, target_region, ratio);
        } else {
            self.dock_group_into(
                DockNode::tabs(&[id]),
                id,
                target_panel,
                target_window,
                target_region,
                ratio,
            );
        }
        true
    }

    /// 从布局中彻底移除（对齐 KT `DockState.undock`）。
    pub fn undock(&self, id: PanelId) {
        if !self.is_open(id) {
            return;
        }
        self.mutate(|m| {
            Self::detach(m, id);
        });
        fire(DockingEvent::Undocked(id, false));
    }

    /// 移入新的浮动窗口（对齐 KT `DockState.moveToNewWindow`）。
    ///
    /// 面板不可浮起（`options.floatable == false`）时不做任何事。
    pub fn move_to_new_window(&self, id: PanelId, bounds: Option<WindowBounds>) {
        let Some(info) = dock_panel_info(id) else { return };
        if !info.options.floatable {
            return;
        }
        self.mutate(|m| {
            let Some(root) = m.root.as_ref() else { return };
            let Some(group) = find_group(root, id) else { return };
            let saved = find_tree_position(root, id);
            let group = group.clone();
            let items = group.collect_ids();
            let active = group.active_panel().unwrap_or(id);
            for gid in &items {
                Self::detach(m, *gid);
            }
            let n = m.floating.len() as f32;
            let b = bounds.unwrap_or(WindowBounds {
                x: 100.0 + n * 24.0,
                y: 100.0 + n * 24.0,
                width: 400.0,
                height: 300.0,
            });
            let wid = next_window_id(&m.floating);
            m.floating.push(FloatingPanel {
                id: wid,
                root: group,
                x: b.x,
                y: b.y,
                w: b.width,
                h: b.height,
                saved,
            });
            fire(DockingEvent::WindowOpened(wid));
            fire(DockingEvent::Floated(id));
        });
    }

    /// 该停靠是否合法（对齐 KT `DockState.canDock`）。
    ///
    /// 校验三件事：目标风格与被拖风格**双向**允许该区域、
    /// `limited_to_window` 面板不得跨窗口、被拖面板必须已注册。
    pub fn can_dock(&self, id: PanelId, target: DockTarget, region: DockRegion) -> bool {
        let Some(dragged_info) = dock_panel_info(id) else {
            return false;
        };
        let dragged_style = dragged_info.options.docking_style;
        // 目标风格：锚点/根节点不限方向
        let target_style = match target {
            DockTarget::OnDockable(p) => dock_panel_info(p)
                .map(|i| i.options.docking_style)
                .unwrap_or(DockingStyle::Both),
            DockTarget::OnNode { window_id, node_id } => self
                .read(|m| {
                    let tree: Option<&DockNode> = if window_id == WINDOW_MAIN {
                        m.root.as_ref()
                    } else {
                        m.floating.iter().find(|f| f.id == window_id).map(|f| &f.root)
                    };
                    tree.and_then(|r| r.find_node(node_id)).and_then(|n| {
                        n.active_panel()
                            .and_then(|p| dock_panel_info(p))
                            .map(|i| i.options.docking_style)
                    })
                })
                .unwrap_or(DockingStyle::Both),
            DockTarget::Root { .. } => DockingStyle::Both,
            DockTarget::Anchor(_) => DockingStyle::Both,
        };
        if !region_is_allowed(region, target_style, dragged_style) {
            return false;
        }
        // limited_to_window：不得跨窗口
        if dragged_info.options.limited_to_window {
            let cur = self.window_of(id);
            let dst = match target {
                DockTarget::OnDockable(p) => self.window_of(p),
                DockTarget::OnNode { window_id, .. } => Some(window_id),
                DockTarget::Root { window_id } => Some(window_id),
                DockTarget::Anchor(_) => None,
            };
            if let (Some(c), Some(d)) = (cur, dst) {
                if c != d {
                    return false;
                }
            }
        }
        true
    }

    /// 确保主窗口里有指定锚点（对齐 KT `DockState.ensureAnchor`）。
    ///
    /// 缺失时按 `region` 补一个占位节点；已存在（占位或已有成员）时返回 `false`。
    /// 该操作**幂等**。
    pub fn ensure_anchor(
        &self,
        anchor_id: AnchorId,
        region: DockRegion,
        proportion: f32,
    ) -> bool {
        let exists = self.read(|m| {
            m.root.as_ref().is_some_and(|r| {
                r.contains_anchor(anchor_id)
                    || r.collect_ids().into_iter().any(|p| {
                        dock_panel_info(p)
                            .and_then(|i| i.options.anchor)
                            .map(|a| a == anchor_id)
                            .unwrap_or(false)
                    })
            })
        });
        if exists {
            return false;
        }
        let anchor = DockNode::anchor(anchor_id);
        self.mutate(|m| {
            let root = m.root.take();
            m.root = Some(match root {
                Some(r) => match region {
                    DockRegion::Center => match r.first_id() {
                        Some(t) => insert_relative(r, t, anchor, DockDirection::Center, proportion),
                        None => anchor,
                    },
                    DockRegion::North => {
                        make_split(r, anchor, DockDirection::North, proportion)
                    }
                    DockRegion::South => {
                        make_split(r, anchor, DockDirection::South, proportion)
                    }
                    DockRegion::East => make_split(r, anchor, DockDirection::East, proportion),
                    DockRegion::West => make_split(r, anchor, DockDirection::West, proportion),
                },
                None => anchor,
            });
        });
        true
    }

    /// 按**节点 id** 选中标签（对齐 KT `DockState.selectTab(nodeId, index)`）。
    pub fn select_tab(&self, node_id: DockNodeId, index: usize) -> bool {
        let mut ok = false;
        self.mutate(|m| {
            let Some(r) = m.root.as_mut() else { return };
            if let Some(DockNode::Tabs {
                tabs,
                selected_index,
                ..
            }) = r.find_node_mut(node_id)
            {
                if index < tabs.len() {
                    let old = tabs[*selected_index].first_id();
                    *selected_index = index;
                    let new = tabs[index].first_id();
                    ok = true;
                    if old != new {
                        if let Some(o) = old {
                            fire(DockingEvent::Hidden(o));
                        }
                        if let Some(n) = new {
                            fire(DockingEvent::Shown(n));
                        }
                    }
                }
            }
        });
        ok
    }

    /// 按**节点 id** 移动标签（对齐 KT `DockState.moveTab(nodeId, fromIndex, toIndex)`）。
    pub fn move_tab(&self, node_id: DockNodeId, from: usize, to: usize) -> bool {
        let mut ok = false;
        self.mutate(|m| {
            let Some(r) = m.root.as_mut() else { return };
            if let Some(DockNode::Tabs { tabs, .. }) = r.find_node_mut(node_id) {
                if from < tabs.len() && to < tabs.len() && from != to {
                    let item = tabs.remove(from);
                    tabs.insert(to, item);
                    ok = true;
                }
            }
        });
        ok
    }

    /// 按**节点 id** 设置分割比例（对齐 KT `DockState.setSplitProportion(nodeId, proportion)`）。
    pub fn set_split_proportion(&self, node_id: DockNodeId, proportion: f32) -> bool {
        let mut ok = false;
        self.mutate(|m| {
            if let Some(r) = m.root.as_mut() {
                ok = set_proportion_by_node_id(r, node_id, proportion);
            }
        });
        ok
    }

    /// 分割条双击复位为均分（对齐 KT `DockState.resetSplitProportion(nodeId)`）。
    pub fn reset_split_proportion(&self, node_id: DockNodeId) -> bool {
        self.set_split_proportion(node_id, 0.5)
    }

    /// 面板所在窗口 id（对齐 KT `DockState.windowOf`）：主窗口 = 0，浮动窗口 = 其 id。
    pub fn window_of(&self, id: PanelId) -> Option<WindowId> {
        self.read(|m| {
            if let Some(f) = m.floating.iter().find(|f| f.contains(id)) {
                return Some(f.id);
            }
            if m.root.as_ref().is_some_and(|r| r.collect_ids().contains(&id)) {
                return Some(WINDOW_MAIN);
            }
            None
        })
    }

    /// 面板是否在布局中（停靠 / 隐藏 / 浮动都算，对齐 KT `DockState.isOpen`）。
    pub fn is_open(&self, id: PanelId) -> bool {
        self.is_docked(id) || self.is_hidden(id) || self.is_floating(id)
    }
}

/// 具名布局快照集合（KT `NamedLayouts`，经 [`DockState::layouts`] 取得）。
///
/// 快照含整份 [`DockModel`]（主窗口树 + 浮动窗口 + 自动隐藏边条）。
/// 恢复时已注销的面板会被剔除，其余按快照结构还原。
#[derive(Clone, Copy)]
pub struct NamedLayouts {
    state: DockState,
}

impl NamedLayouts {
    /// 保存当前布局为具名快照。快照里有未注册的面板时返回 `false`（仍会保存）。
    pub fn save(&self, name: &str) -> bool {
        let snap = self.state.model();
        let known = snap.all_ids().iter().all(|p| dock_panel_info(*p).is_some());
        self.state.named.update(|n| {
            n.insert(name.to_string(), snap);
        });
        known
    }

    /// 恢复具名快照；不存在返回 `false`。
    pub fn restore(&self, name: &str) -> bool {
        let Some(mut snap) = self.state.named.with_value(|n| n.get(name).cloned()) else {
            return false;
        };
        let stale: Vec<PanelId> = snap
            .all_ids()
            .into_iter()
            .filter(|p| dock_panel_info(*p).is_none())
            .collect();
        for id in stale {
            snap.root = snap.root.take().and_then(|r| remove_panel(r, id));
            for h in &mut snap.hidden {
                h.items.retain(|&x| x != id);
                if h.active == id {
                    h.active = h.items.first().copied().unwrap_or(0);
                }
            }
            snap.hidden.retain(|h| !h.items.is_empty());
            for f in &mut snap.floating {
                let taken = std::mem::replace(&mut f.root, DockNode::leaf(0));
                if let Some(nr) = remove_panel(taken, id) {
                    f.root = nr;
                }
            }
            snap.floating.retain(|f| !f.root.collect_ids().is_empty());
            if snap.maximized == Some(id) {
                snap.maximized = None;
                snap.saved_before_maximize = None;
            }
            if snap.expanded == Some(id) {
                snap.expanded = None;
            }
        }
        self.state.model.set(snap);
        fire(DockingEvent::LayoutChanged(LayoutChangeKind::Restored));
        true
    }

    /// 删除快照；不存在返回 `false`。
    pub fn remove(&self, name: &str) -> bool {
        let had = self.state.named.with_value(|n| n.contains_key(name));
        if had {
            self.state.named.update(|n| {
                n.remove(name);
            });
        }
        had
    }

    pub fn contains(&self, name: &str) -> bool {
        self.state.named.with_value(|n| n.contains_key(name))
    }

    /// 全部快照名（字典序，保证可预测）。
    pub fn names(&self) -> Vec<String> {
        let mut v = self.state.named.with_value(|n| n.keys().cloned().collect::<Vec<_>>());
        v.sort();
        v
    }
}

// ══════════════════════════════ 布局存储 ══════════════════════════════

/// 布局存储后端（对齐 KT `docking/storage/LayoutStorage.kt`）。
///
/// 两类条目分开：*layout* 是**单个**完整状态文本（[`DockState::save_full_state`] 的产物），
/// *layouts* 是**命名布局集合**（[`NamedLayouts`] 的全部快照）。KT 用 `suspend` 串行化 IO，
/// 本库调用点都在组合期或事件回调里（不允许跨帧挂起），故签名保持同步。
pub trait LayoutStorage {
    fn save_layout(&self, key: &str, layout: &str);
    fn load_layout(&self, key: &str) -> Option<String>;
    fn remove_layout(&self, key: &str);
    fn save_layouts(&self, key: &str, layouts: &HashMap<String, String>);
    /// 读命名布局集合；无该键返回 `None`（与「空集合」区分，便于识别首次启动）。
    fn load_layouts(&self, key: &str) -> Option<HashMap<String, String>>;
    fn remove_layouts(&self, key: &str);
}

/// 命名布局集合的文本编码：`dln1` + 若干「长度行 / 名字 / 正文」。
///
/// 用字符长度前缀而非分隔符，因为正文本身含换行（`mdl2` 是多行格式）。
pub fn encode_named_layouts(map: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut out = String::from("dln1");
    for k in keys {
        let v = &map[k];
        out.push('\n');
        out.push_str(&format!("{} {}", k.chars().count(), v.chars().count()));
        out.push('\n');
        out.push_str(k);
        out.push('\n');
        out.push_str(v);
    }
    out
}

/// [`encode_named_layouts`] 的逆；格式非法返回 `None`。
pub fn decode_named_layouts(text: &str) -> Option<HashMap<String, String>> {
    let cs: Vec<char> = text.chars().collect();
    let line_end = |i: usize| -> usize {
        let mut j = i;
        while j < cs.len() && cs[j] != '\n' {
            j += 1;
        }
        j
    };
    let h = line_end(0);
    if cs[..h].iter().collect::<String>() != "dln1" {
        return None;
    }
    let mut i = h + 1;
    let mut out = HashMap::new();
    while i < cs.len() {
        let j = line_end(i);
        let head: String = cs[i..j].iter().collect();
        i = j + 1;
        let mut it = head.split(' ');
        let n: usize = it.next()?.parse().ok()?;
        let m: usize = it.next()?.parse().ok()?;
        if i + n > cs.len() || i + n + 1 + m > cs.len() {
            return None;
        }
        let name: String = cs[i..i + n].iter().collect();
        i += n + 1;
        let body: String = cs[i..i + m].iter().collect();
        i += m;
        if i < cs.len() {
            if cs[i] != '\n' {
                return None;
            }
            i += 1;
        }
        out.insert(name, body);
    }
    Some(out)
}

thread_local! {
    static MEM_LAYOUTS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static MEM_GROUPS: RefCell<HashMap<String, HashMap<String, String>>> =
        RefCell::new(HashMap::new());
}

/// 进程内存储：测试与没有文件系统的宿主用。数据在线程局部，与 dock 其余状态同语义。
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryLayoutStorage;

static MEMORY_STORAGE: MemoryLayoutStorage = MemoryLayoutStorage;

impl MemoryLayoutStorage {
    /// `remember_auto_persist` 需要的 `static 句柄。
    pub fn shared() -> &'static dyn LayoutStorage {
        &MEMORY_STORAGE
    }

    /// 清空全部内存条目（测试之间隔离状态用）。
    pub fn clear() {
        MEM_LAYOUTS.with(|m| m.borrow_mut().clear());
        MEM_GROUPS.with(|g| g.borrow_mut().clear());
    }
}

impl LayoutStorage for MemoryLayoutStorage {
    fn save_layout(&self, key: &str, layout: &str) {
        MEM_LAYOUTS.with(|m| {
            m.borrow_mut()
                .insert(key.to_string(), layout.to_string());
        });
    }

    fn load_layout(&self, key: &str) -> Option<String> {
        MEM_LAYOUTS.with(|m| m.borrow().get(key).cloned())
    }

    fn remove_layout(&self, key: &str) {
        MEM_LAYOUTS.with(|m| {
            m.borrow_mut().remove(key);
        });
    }

    fn save_layouts(&self, key: &str, layouts: &HashMap<String, String>) {
        MEM_GROUPS.with(|g| g.borrow_mut().insert(key.to_string(), layouts.clone()));
    }

    fn load_layouts(&self, key: &str) -> Option<HashMap<String, String>> {
        MEM_GROUPS.with(|g| g.borrow().get(key).cloned())
    }

    fn remove_layouts(&self, key: &str) {
        MEM_GROUPS.with(|g| {
            g.borrow_mut().remove(key);
        });
    }
}

/// 键名净化：只保留 ASCII 字母数字、`-`、`_`，其它一律替换为下划线。
///
/// 因此键无法表示路径分隔符或 `..`，即便调用方把用户输入当键也不会越出目录。
fn safe_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        out.push(if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            c
        } else {
            '_'
        });
    }
    if out.is_empty() {
        out.push_str("layout");
    }
    out
}

/// 文件存储：`dir/<safe(key)>.dl`（单布局）与 `dir/<safe(key)>.dlgroup`（命名集合）。
///
/// 写入先落 `.tmp` 再 `rename`，进程中途被杀不会留下半截文件。IO 错误按「保存失败」
/// 静默处理——持久化是尽力而为，不该让 UI 崩。
#[derive(Clone, Debug)]
pub struct FileLayoutStorage {
    pub dir: std::path::PathBuf,
}

impl FileLayoutStorage {
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        FileLayoutStorage { dir: dir.into() }
    }

    fn path(&self, key: &str, ext: &str) -> std::path::PathBuf {
        self.dir.join(format!("{}.{}", safe_key(key), ext))
    }

    fn write_atomic(&self, path: &std::path::Path, data: &str) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, data).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

impl LayoutStorage for FileLayoutStorage {
    fn save_layout(&self, key: &str, layout: &str) {
        let p = self.path(key, "dl");
        self.write_atomic(&p, layout);
    }

    fn load_layout(&self, key: &str) -> Option<String> {
        std::fs::read_to_string(self.path(key, "dl")).ok()
    }

    fn remove_layout(&self, key: &str) {
        let _ = std::fs::remove_file(self.path(key, "dl"));
    }

    fn save_layouts(&self, key: &str, layouts: &HashMap<String, String>) {
        let p = self.path(key, "dlgroup");
        self.write_atomic(&p, &encode_named_layouts(layouts));
    }

    fn load_layouts(&self, key: &str) -> Option<HashMap<String, String>> {
        decode_named_layouts(&std::fs::read_to_string(self.path(key, "dlgroup")).ok()?)
    }

    fn remove_layouts(&self, key: &str) {
        let _ = std::fs::remove_file(self.path(key, "dlgroup"));
    }
}

/// 面板当前允许的操作（对齐 KT `DockCapabilities`）。
///
/// 上下文菜单 / 标签下拉据此决定条目是否出现，控件层不再各自翻 `DockableOptions`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockCapabilities {
    pub can_dock: bool,
    pub can_float: bool,
    pub can_close: bool,
    pub can_auto_hide: bool,
    pub can_maximize: bool,
}

impl DockCapabilities {
    pub const fn all() -> Self {
        DockCapabilities {
            can_dock: true,
            can_float: true,
            can_close: true,
            can_auto_hide: true,
            can_maximize: true,
        }
    }

    /// 全部禁止（面板未注册时的返回值）。
    pub const fn none() -> Self {
        DockCapabilities {
            can_dock: false,
            can_float: false,
            can_close: false,
            can_auto_hide: false,
            can_maximize: false,
        }
    }
}

impl DockState {
    /// 面板允许的操作集合（KT `DockCapabilities`）。
    pub fn capabilities(&self, id: PanelId) -> DockCapabilities {
        let Some(info) = dock_panel_info(id) else {
            return DockCapabilities::none();
        };
        DockCapabilities {
            can_dock: self.can_dock(
                id,
                DockTarget::Root {
                    window_id: WINDOW_MAIN,
                },
                DockRegion::Center,
            ),
            can_float: self.can_float(id),
            can_close: self.can_close(id),
            can_auto_hide: info.auto_hide_allowed,
            can_maximize: info.min_max_allowed,
        }
    }

    /// 把所有浮动窗口合回主窗口（KT `mergeFloatingIntoMain`）。
    pub fn merge_floating_into_main(&self) {
        let wins: Vec<WindowId> = self.read(|m| m.floating.iter().map(|f| f.id).collect());
        for w in wins {
            self.close_window(w);
        }
    }

    /// 把当前完整状态写入存储（KT `saveLayout`）。
    pub fn save_layout_to(&self, storage: &dyn LayoutStorage, key: &str) {
        let text = self.save_full_state();
        storage.save_layout(key, &text);
        fire(DockingEvent::LayoutChanged(LayoutChangeKind::Persisted));
    }

    /// 从存储恢复完整状态；键不存在或数据非法返回 `false`（KT `restoreLayout`）。
    pub fn restore_layout_from(&self, storage: &dyn LayoutStorage, key: &str) -> bool {
        self.restore_layout_from_with(storage, key, &mut |id| Some(id))
    }

    /// 同 [`Self::restore_layout_from`]，但旧面板 id 先经解析器映射（KT `DockableResolver`）。
    pub fn restore_layout_from_with(
        &self,
        storage: &dyn LayoutStorage,
        key: &str,
        resolve: &mut DockableResolver,
    ) -> bool {
        match storage.load_layout(key) {
            Some(text) => self.restore_full_state_with(&text, resolve),
            None => false,
        }
    }

    /// 删除存储里的单个布局（KT `removeSavedLayout`）。
    pub fn remove_saved_layout(&self, storage: &dyn LayoutStorage, key: &str) {
        storage.remove_layout(key);
    }

    /// 命名布局集合写入存储。
    pub fn save_named_layouts_to(&self, storage: &dyn LayoutStorage, key: &str) {
        let map = self.named.with_value(|n| {
            n.iter()
                .map(|(k, m)| (k.clone(), Self::encode_model(m)))
                .collect::<HashMap<String, String>>()
        });
        storage.save_layouts(key, &map);
    }

    /// 存储读回命名布局集合（整体覆盖现有集合）；键不存在返回 `false`。
    pub fn restore_named_layouts_from(&self, storage: &dyn LayoutStorage, key: &str) -> bool {
        self.restore_named_layouts_from_with(storage, key, &mut |id| Some(id))
    }

    /// 同 [`Self::restore_named_layouts_from`]，但带面板解析器。
    ///
    /// 无法解析成已注册面板的快照整体丢弃（宁可少一条布局，也不恢复出半棵树）。
    pub fn restore_named_layouts_from_with(
        &self,
        storage: &dyn LayoutStorage,
        key: &str,
        resolve: &mut DockableResolver,
    ) -> bool {
        let Some(map) = storage.load_layouts(key) else {
            return false;
        };
        let mut out: HashMap<String, DockModel> = HashMap::new();
        for (name, text) in map {
            if let Some(m) = Self::decode_model_with(&text, resolve) {
                out.insert(name, m);
            }
        }
        self.named.update(|n| *n = out);
        true
    }
}

/// 布局变化后自动落盘（对齐 KT `rememberAutoPersist(state, storage, key, intervalMs)`）。
///
/// 只在组合期调用。布局一变即重组 → 比较状态指纹；**到点才序列化写盘**，未到点则登记
/// 一条定点唤醒帧驱动（`min_interval_ms` 后补写并自注销）。于是连续拖拽期间不会每帧写盘，
/// 静止时也不会常驻 60fps。首次组合只记录基线、不写盘，避免刚启动就把尚未恢复的初始
/// 布局覆盖进存储。
pub fn remember_auto_persist(
    state: DockState,
    storage: &'static dyn LayoutStorage,
    key: impl Into<String>,
    min_interval_ms: u64,
) {
    let key = std::rc::Rc::new(key.into());
    let interval_ns = min_interval_ms.max(1) as f64 * 1_000_000_000.0;

    #[derive(Default)]
    struct Slot {
        /// 最近一次已写盘的文本与时刻。
        saved_text: String,
        saved_ns: f64,
        seeded: bool,
        /// 待写入文本与计划的写入时刻。
        pending: Option<(String, f64)>,
        handle: Option<compose::runtime::FrameDriverHandle>,
    }

    let slot = remember(|| std::rc::Rc::new(RefCell::new(Slot::default())));
    // 组合期读模型：布局（含拖动比例、浮动窗位置）变化即重组，这里才拿得到新指纹。
    let text = state.save_full_state();
    let now = clock_nanos();
    let mut start_driver = false;
    {
        let mut s = slot.borrow_mut();
        if !s.seeded {
            s.seeded = true;
            s.saved_text = text.clone();
            s.saved_ns = now;
        } else if text != s.saved_text {
            if now - s.saved_ns >= interval_ns {
                s.saved_text = text.clone();
                s.saved_ns = now;
                s.pending = None;
                drop(s);
                storage.save_layout(&key, &text);
            } else {
                s.pending = Some((text, s.saved_ns + interval_ns));
                start_driver = s.handle.is_none();
            }
        }
    }
    if start_driver {
        let st = slot.clone();
        let k = key.clone();
        let h = compose::runtime::add_frame_driver(move || {
            let now = clock_nanos();
            let flush = {
                // 先取走待发条目再判断：既避开 `&s.pending` 与写回的可借冲突，
                // 也保证两次唤醒之间不会重复写同一条。
                let pending = st.borrow_mut().pending.take();
                match pending {
                    Some((t, due)) if now >= due => {
                        let mut s = st.borrow_mut();
                        s.saved_text = t.clone();
                        s.saved_ns = now;
                        Some(t)
                    }
                    Some((t, due)) => {
                        // 还没到点：放回并请求一次定点唤醒，避免为等待而连续跑帧。
                        st.borrow_mut().pending = Some((t, due));
                        request_anim_frame_after((due - now) as f32 / 1_000_000_000.0_f32);
                        None
                    }
                    None => None,
                }
            };
            if let Some(t) = flush {
                storage.save_layout(&k, &t);
                let h = st.borrow_mut().handle.take();
                if let Some(h) = h {
                    compose::runtime::remove_frame_driver(h);
                }
            }
        });
        slot.borrow_mut().handle = Some(h);
    }
}

// ══════════════════════════ 声明式布局构造 ══════════════════════════
//
// 对齐 KT 的 `dockLayout { window(WINDOW_MAIN) { … } }` DSL：KT 在组合期注册 dockable，
// Rust 侧面板由 `register_dock_panel` 先拿到 [`PanelId`]，因此本 DSL 只做**声明式布局树构造**，
// 产物 [`DockLayoutSpec`] 可转成 [`DockModel`]（`dock_state(spec.to_model())`）或 [`DockLayout`]。

/// DSL 里的一个窗格。
#[derive(Clone, PartialEq, Debug)]
pub enum DockPane {
    /// 单面板。
    Leaf(PanelId),
    /// 标签组（`selected` 为选中下标，越界时钳到最后一项）。
    Tabs { ids: Vec<PanelId>, selected: usize },
    /// 锚点占位。
    Anchor(AnchorId),
    /// 已构造好的布局树（逃生舱）。
    Node(DockNode),
    /// 分割容器：方向 + 子窗格（权重决定同级占比）。
    Split { orientation: SplitOrientation, parts: Vec<DockSlot> },
}

/// 带权重的窗格：`weight` 是同级之间的相对份额（`<= 0` 视作 1）。
///
/// 与 KT 的 `child(proportion)` 同义——两栏写 `0.25 / 0.75` 即左栏占 1/4。
#[derive(Clone, PartialEq, Debug)]
pub struct DockSlot {
    /// 窗格内容。
    pub pane: DockPane,
    /// 同级相对权重。
    pub weight: f32,
}

impl DockSlot {
    fn new(pane: DockPane, weight: f32) -> Self {
        DockSlot { pane, weight: if weight > 0.0 { weight } else { 1.0 } }
    }
}

/// 窗格收集器：一个窗口 / 分割子树的内容作用域。
#[derive(Clone, Default)]
pub struct DockPaneList {
    slots: Vec<DockSlot>,
}

impl DockPaneList {
    /// 追加一个单面板窗格（同级等分）。
    pub fn dock(&mut self, id: PanelId) {
        self.dock_weighted(id, 1.0);
    }

    /// 追加一个指定权重的单面板窗格。
    pub fn dock_weighted(&mut self, id: PanelId, weight: f32) {
        self.slots.push(DockSlot::new(DockPane::Leaf(id), weight));
    }

    /// 追加一个标签组窗格（选中第一个）。
    pub fn tabs(&mut self, ids: &[PanelId]) {
        self.tabs_selected(ids, 0);
    }

    /// 追加一个标签组窗格并指定选中项。
    pub fn tabs_selected(&mut self, ids: &[PanelId], selected: usize) {
        self.tabs_weighted(ids, selected, 1.0);
    }

    /// 追加一个指定权重的标签组窗格。
    pub fn tabs_weighted(&mut self, ids: &[PanelId], selected: usize, weight: f32) {
        self.slots.push(DockSlot::new(
            DockPane::Tabs {
                ids: ids.to_vec(),
                selected,
            },
            weight,
        ));
    }

    /// 追加一个锚点占位窗格。
    pub fn anchor(&mut self, anchor_id: AnchorId) {
        self.slots.push(DockSlot::new(DockPane::Anchor(anchor_id), 1.0));
    }

    /// 直接追加一棵已构造的布局树。
    pub fn node(&mut self, node: DockNode) {
        self.node_weighted(node, 1.0);
    }

    /// 见 [`Self::node`]，带权重。
    pub fn node_weighted(&mut self, node: DockNode, weight: f32) {
        self.slots.push(DockSlot::new(DockPane::Node(node), weight));
    }

    /// 左右分割容器：本容器在父级里占 `weight` 份额。
    pub fn row(&mut self, weight: f32, init: impl FnOnce(&mut DockPaneList)) {
        self.split_with(SplitOrientation::Horizontal, weight, init);
    }

    /// 上下分割容器：本容器在父级里占 `weight` 份额。
    pub fn col(&mut self, weight: f32, init: impl FnOnce(&mut DockPaneList)) {
        self.split_with(SplitOrientation::Vertical, weight, init);
    }

    /// 任意方向分割容器。
    pub fn split_with(
        &mut self,
        orientation: SplitOrientation,
        weight: f32,
        init: impl FnOnce(&mut DockPaneList),
    ) {
        let mut sub = DockPaneList::default();
        init(&mut sub);
        self.slots.push(DockSlot::new(
            DockPane::Split {
                orientation,
                parts: sub.slots,
            },
            weight,
        ));
    }

    /// 同级窗格数。
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// 作为窗口内容：全部是面板时并成同一标签组（KT `window {}` 语义），
    /// 含分割/锚点窗格时按左右分割排布。
    pub fn to_window_node(&self) -> Option<DockNode> {
        let tabifiable = self
            .slots
            .iter()
            .all(|s| matches!(s.pane, DockPane::Leaf(_) | DockPane::Tabs { .. }));
        if tabifiable && self.slots.len() > 1 {
            let mut ids: Vec<PanelId> = Vec::new();
            let mut selected = 0usize;
            let mut declared = false;
            for s in &self.slots {
                match &s.pane {
                    DockPane::Leaf(id) => ids.push(*id),
                    DockPane::Tabs { ids: g, selected: sel } => {
                        if !declared && *sel > 0 {
                            selected = ids.len() + (*sel).min(g.len().saturating_sub(1));
                            declared = true;
                        }
                        ids.extend(g.iter().copied());
                    }
                    _ => {}
                }
            }
            if !ids.is_empty() {
                return Some(tabs_node(&ids, selected));
            }
        }
        self.to_split_node()
    }

    /// 作为分割子树（同级按权重串成二分链）。
    pub fn to_split_node(&self) -> Option<DockNode> {
        chain_slots(self.slots.clone(), SplitOrientation::Horizontal)
    }
}

/// 面板 id 列表 → 标签组节点。
fn tabs_node(ids: &[PanelId], selected: usize) -> DockNode {
    DockNode::Tabs {
        id: next_node_id(),
        tabs: ids.iter().map(|&p| DockNode::leaf(p)).collect(),
        selected_index: selected.min(ids.len() - 1),
    }
}

/// 单个窗格 → 布局树。
fn slot_node(slot: &DockSlot) -> Option<DockNode> {
    match &slot.pane {
        DockPane::Leaf(id) => Some(DockNode::leaf(*id)),
        DockPane::Anchor(a) => Some(DockNode::anchor(*a)),
        DockPane::Node(n) => Some(n.clone()),
        DockPane::Tabs { ids, selected } => {
            if ids.is_empty() {
                None
            } else {
                Some(tabs_node(ids, *selected))
            }
        }
        DockPane::Split { orientation, parts } => chain_slots(parts.clone(), *orientation),
    }
}

/// 同级窗格 → 二分链：每层首项占比 = 其权重 / 剩余权重之和。
///
/// 三栏等分因此得到 `1/3` 与 `1/2` 的级联比例，语义与逐层相对份额一致。
fn chain_slots(parts: Vec<DockSlot>, orientation: SplitOrientation) -> Option<DockNode> {
    let built: Vec<(f32, DockNode)> = parts
        .iter()
        .filter_map(|s| slot_node(s).map(|n| (s.weight, n)))
        .collect();
    chain_built(built, orientation)
}

fn chain_built(mut built: Vec<(f32, DockNode)>, orientation: SplitOrientation) -> Option<DockNode> {
    match built.len() {
        0 => None,
        1 => Some(built.pop().map(|(_, n)| n)?),
        n => {
            let total: f32 = built.iter().map(|(w, _)| *w).sum();
            let total = if total > 0.0 { total } else { n as f32 };
            let (weight, first) = built.remove(0);
            let second = chain_built(built, orientation)?;
            Some(DockNode::split(
                orientation,
                (weight / total).clamp(DOCK_RATIO_MIN, DOCK_RATIO_MAX),
                first,
                second,
            ))
        }
    }
}

/// 一个浮动窗口的声明。
#[derive(Clone, PartialEq, Debug)]
pub struct DockWindowSpec {
    /// 窗口 id（`None` = 自动分配，自 1 递增）。
    pub id: Option<WindowId>,
    /// 屏幕边界。
    pub bounds: WindowBounds,
    /// 窗口内容树。
    pub node: Option<DockNode>,
}

/// 顶层布局声明（KT `dockLayout {}` 的产物）。
#[derive(Clone, PartialEq, Debug)]
pub struct DockLayoutSpec {
    /// 主窗口内容树。
    pub main: Option<DockNode>,
    /// 浮动窗口列表。
    pub floating: Vec<DockWindowSpec>,
}

impl DockLayoutSpec {
    /// 转成运行时模型（自动隐藏边条为空）。
    pub fn to_model(&self) -> DockModel {
        let mut m = DockModel {
            root: self.main.clone(),
            ..DockModel::empty()
        };
        for (i, w) in self.floating.iter().enumerate() {
            let Some(root) = w.node.clone() else {
                continue;
            };
            if root.collect_ids().is_empty() {
                continue;
            }
            m.floating.push(FloatingPanel {
                id: w.id.unwrap_or(i as WindowId + 1),
                root,
                x: w.bounds.x,
                y: w.bounds.y,
                w: w.bounds.width,
                h: w.bounds.height,
                saved: None,
            });
        }
        m
    }

    /// 转成窗口列表视图（持久化模型）。
    pub fn to_layout(&self) -> DockLayout {
        DockLayout::from_model(&self.to_model())
    }
}

/// [`DockLayoutSpec`] 的构造作用域。
#[derive(Default)]
pub struct DockLayoutBuilder {
    main: Option<DockNode>,
    floating: Vec<DockWindowSpec>,
}

impl DockLayoutBuilder {
    /// 单个面板占满主窗口。
    pub fn dock(&mut self, id: PanelId) {
        self.main = Some(DockNode::leaf(id));
    }

    /// 多个面板并到主窗口同一标签组。
    pub fn display(&mut self, ids: &[PanelId]) {
        self.display_selected(ids, 0);
    }

    /// 多个面板并到主窗口同一标签组并指定选中项。
    pub fn display_selected(&mut self, ids: &[PanelId], selected: usize) {
        let tabs: Vec<DockNode> = ids.iter().map(|&p| DockNode::leaf(p)).collect();
        self.main = if tabs.is_empty() {
            None
        } else {
            Some(DockNode::Tabs {
                id: next_node_id(),
                tabs,
                selected_index: selected.min(ids.len() - 1),
            })
        };
    }

    /// 主窗口内容（多窗格 → 标签组）。
    pub fn main(&mut self, init: impl FnOnce(&mut DockPaneList)) {
        let mut sub = DockPaneList::default();
        init(&mut sub);
        self.main = sub.to_window_node();
    }

    /// 直接指定主窗口树。
    pub fn main_node(&mut self, node: DockNode) {
        self.main = Some(node);
    }

    /// 把 `id` 停靠到已有面板 `lead` 的某方向（KT `dockOn`）。
    ///
    /// `lead` 尚未出现在布局里时退化为追加到主窗口。
    pub fn dock_on(&mut self, lead: PanelId, direction: DockDirection, id: PanelId, proportion: f32) {
        let group = DockNode::tabs(&[id]);
        self.main = Some(match self.main.take() {
            Some(r) if r.collect_ids().contains(&lead) => {
                insert_relative(r, lead, group, direction, proportion)
            }
            Some(r) => {
                let target = r.first_id().unwrap_or(lead);
                insert_relative(r, target, group, direction, proportion)
            }
            None => group,
        });
    }

    /// 在主窗口某边缘放一个锚点占位（KT `anchorAtRoot`）。
    pub fn anchor_at_root(&mut self, region: DockRegion, anchor_id: AnchorId, proportion: f32) {
        let node = DockNode::anchor(anchor_id);
        self.main = Some(match self.main.take() {
            Some(r) => make_split(r, node, region, proportion),
            None => node,
        });
    }

    /// 声明一个浮动窗口（KT `floatingWindow`）。
    pub fn floating_window(
        &mut self,
        bounds: WindowBounds,
        init: impl FnOnce(&mut DockFloatingBuilder),
    ) {
        let mut b = DockFloatingBuilder {
            id: None,
            bounds,
            list: DockPaneList::default(),
        };
        init(&mut b);
        let node = b.list.to_window_node();
        self.floating.push(DockWindowSpec {
            id: b.id,
            bounds: b.bounds,
            node,
        });
    }
}

/// 浮动窗口声明作用域。
pub struct DockFloatingBuilder {
    id: Option<WindowId>,
    bounds: WindowBounds,
    list: DockPaneList,
}

impl DockFloatingBuilder {
    /// 指定浮动窗口 id（默认按声明顺序自动分配）。
    pub fn window_id(&mut self, id: WindowId) {
        self.id = Some(id);
    }

    /// 覆盖窗口边界。
    pub fn bounds(&mut self, bounds: WindowBounds) {
        self.bounds = bounds;
    }

    /// 浮动窗口内容（多窗格 → 标签组）。
    pub fn content(&mut self, init: impl FnOnce(&mut DockPaneList)) {
        init(&mut self.list);
    }

    /// 单个面板占满浮动窗口。
    pub fn dock(&mut self, id: PanelId) {
        self.list.dock(id);
    }

    /// 左右 / 上下分割。
    pub fn row(&mut self, proportion: f32, init: impl FnOnce(&mut DockPaneList)) {
        self.list.row(proportion, init);
    }

    /// 见 [`Self::row`]。
    pub fn col(&mut self, proportion: f32, init: impl FnOnce(&mut DockPaneList)) {
        self.list.col(proportion, init);
    }
}

/// 声明式构造布局（KT `dockLayout {}`）。
///
/// ```ignore
/// let spec = dock_layout(|l| {
///     l.main(|m| {
///         m.row(0.25, |c| c.dock(explorer));
///         m.col(0.7, |c| {
///             c.dock(editor);
///             c.tabs(&[output, problems]);
///         });
///     });
///     l.floating_window(WindowBounds { x: 80.0, y: 80.0, width: 320.0, height: 240.0 }, |f| {
///         f.dock(props);
///     });
/// });
/// let state = dock_state(spec.to_model());
/// ```
pub fn dock_layout(init: impl FnOnce(&mut DockLayoutBuilder)) -> DockLayoutSpec {
    let mut b = DockLayoutBuilder::default();
    init(&mut b);
    DockLayoutSpec {
        main: b.main,
        floating: b.floating,
    }
}

fn join_ids(ids: &[PanelId]) -> String {
    ids.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_ids(s: &str) -> Vec<PanelId> {
    s.split(',')
        .filter_map(|v| v.parse().ok())
        .filter(|&id| dock_panel_exists(id))
        .collect()
}

fn opt_u64(v: Option<i64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "-".into())
}

fn parse_saved(sib: Option<&&str>, dir: Option<&&str>, ratio: Option<&&str>) -> Option<SavedTreePosition> {
    let sibling: PanelId = sib?.parse().ok()?;
    let direction = DockDirection::from_index(dir?.parse().ok()?)?;
    let ratio: f32 = ratio?.parse().ok()?;
    Some(SavedTreePosition {
        sibling,
        direction,
        ratio,
    })
}

/// 按面板中心到布局四边的距离选最近边（比参考实现的固定虚拟尺寸更准确）。
fn nearest_edge_for(id: PanelId, tree: Option<&DockNode>) -> DockEdge {
    let root = dock_layout_rect();
    let root_valid = root.2 > 1.0 && root.3 > 1.0;
    let groups: Vec<GroupRect> = GROUP_RECTS.with(|g| g.borrow().clone());
    let center = groups
        .iter()
        .find(|g| g.ids.contains(&id))
        .and_then(|g| {
            let (x, y, w, h) = node_abs_rect(g.node);
            // 陈旧或未落位的组几何（宽高为 0）会把中心算到原点，
            // 相对有效 root 就判成 Top —— 左侧面板也会「跑到顶部」。
            if w > 1.0 && h > 1.0 {
                Some((x + w / 2.0, y + h / 2.0))
            } else {
                None
            }
        })
        // 几何不可用：从树中位置推断，最可靠且不依赖点击落点。
        // root 尚未落位（宽高为 0）时推断出的中心也在原点，必须跳过并交给指针回退。
        .or_else(|| {
            if !root_valid {
                return None;
            }
            tree.and_then(|n| find_tree_position(n, id)).and_then(|p| {
                let cx = root.0 + root.2 / 2.0;
                let cy = root.1 + root.3 / 2.0;
                match p.direction {
                    DockDirection::West => Some((root.0, cy)),
                    DockDirection::East => Some((root.0 + root.2, cy)),
                    DockDirection::North => Some((cx, root.1)),
                    DockDirection::South => Some((cx, root.1 + root.3)),
                    _ => None,
                }
            })
        })
        // 树中也没有：回退到触发点击的指针位置（若回退 root 中心，宽窗口会判成 Top）。
        .or_else(|| {
            let (px, py) = last_pointer_global();
            if px > 0.0 || py > 0.0 {
                Some((px, py))
            } else {
                None
            }
        })
        .unwrap_or((root.0 + root.2 / 2.0, root.1 + root.3 / 2.0));
    let dl = center.0 - root.0;
    let dr = root.0 + root.2 - center.0;
    let dt = center.1 - root.1;
    let db = root.1 + root.3 - center.1;
    let m = dl.min(dr).min(dt).min(db);
    if m == dl {
        DockEdge::Left
    } else if m == dr {
        DockEdge::Right
    } else if m == dt {
        DockEdge::Top
    } else {
        DockEdge::Bottom
    }
}

fn dock_layout_rect() -> DockRect {
    ROOT_NODE
        .with(|r| r.get())
        .map(node_abs_rect)
        .unwrap_or((0.0, 0.0, 0.0, 0.0))
}

// ══════════════════════════════ 拖放判定 ══════════════════════════════

#[derive(Clone, Copy)]
struct DockDrop {
    guide: Option<DockGuide>,
    /// 预览基准矩形：Inner = 目标组矩形，Outer = 主窗口矩形，Tab = 标签条矩形。
    rect: DockRect,
    outside: bool,
}

/// 标签条插入位：统计中心点落在指针左侧的标签数（排除被拖者）。
fn tab_insert_index(ids: &[PanelId], gx: f32, source: PanelId) -> usize {
    let tabs: Vec<TabRect> = TAB_RECTS.with(|t| t.borrow().to_vec());
    let mut before = 0usize;
    for t in &tabs {
        if t.id == source || !ids.contains(&t.id) {
            continue;
        }
        let r = node_abs_rect(t.node);
        if gx > r.0 + r.2 / 2.0 {
            before += 1;
        }
    }
    before
}

/// 标签条插入光标的 x 坐标（第 `index` 个非源标签的左缘，末尾取右缘）。
fn tab_caret_x(ids: &[PanelId], index: usize, source: PanelId) -> f32 {
    let tabs: Vec<TabRect> = TAB_RECTS.with(|t| t.borrow().to_vec());
    let mut seen = 0usize;
    let mut last_right = None;
    for t in &tabs {
        if t.id == source || !ids.contains(&t.id) {
            continue;
        }
        let r = node_abs_rect(t.node);
        if seen == index {
            return r.0;
        }
        seen += 1;
        last_right = Some(r.0 + r.2);
    }
    last_right.unwrap_or_else(|| {
        TAB_BARS.with(|b| {
            b.borrow()
                .iter()
                .find(|bar| bar.ids == ids)
                .map(|bar| node_abs_rect(bar.node).0 + 4.0)
                .unwrap_or(0.0)
        })
    })
}

/// 释放点归属（VS 语义）：外侧指南 → 标签条 → 组内罗盘 → 无目标 / 布局外。
///
/// 与旧版「矩形五分区盲判」不同：**只有命中指南才投放**，悬停在组内空白处
/// 不产生目标（抬起即取消），这正是 VS 拖拽时「跟着菱形走」的手感。
fn dock_drop_target(gx: f32, gy: f32, source: PanelId) -> DockDrop {
    let root = dock_layout_rect();
    let bars: Vec<TabBarRect> = TAB_BARS.with(|b| b.borrow().clone());
    // 1) 整窗外缘停靠带：贴到容器四条边（不限中点）即视为**整窗**边区停靠，
    //    优先于组内小罗盘。否则从底部面板标签拖向左/右边缘时，会先命中底部组自己的
    //    内层西方位，结果只把底部条一分为二（用户看到的「变成左右 2 个 panel」），
    //    而 VS 在这里应把面板停到整窗左侧（全高）。
    if root.2 > 0.0 && root.3 > 0.0 && point_in(root, gx, gy) {
        let dl = gx - root.0;
        let dr = root.0 + root.2 - gx;
        let dt = gy - root.1;
        let db = root.1 + root.3 - gy;
        let m = dl.min(dr).min(dt).min(db);
        let in_tab_bar = bars
            .iter()
            .any(|bar| point_in(node_abs_rect(bar.node), gx, gy));
        if m <= DOCK_OUTER_EDGE_GRAB && !(m == (root.1 + root.3 - gy) && in_tab_bar) {
            let dir = if m == dl {
                DockDirection::West
            } else if m == dr {
                DockDirection::East
            } else if m == dt {
                DockDirection::North
            } else {
                DockDirection::South
            };
            return DockDrop {
                guide: Some(DockGuide::Outer { dir }),
                rect: root,
                outside: false,
            };
        }
    }
    for bar in bars.iter().rev() {
        if bar.ids.contains(&source) && bar.ids.len() == 1 {
            continue;
        }
        let rect = node_abs_rect(bar.node);
        if point_in(rect, gx, gy) {
            // VS：把同组标签拖到**本标签条的左/右端** = 拆成左右两个面板
            // （图1→图2）；中段仍按标签重排处理。此前自组只有 Tab 目标，
            // 完全没有 Inner 指南 ⇒ 同组内两个标签间无法产生 drop target。
            if bar.ids.contains(&source) && bar.ids.len() > 1 && rect.2 > 0.0 {
                let rel = (gx - rect.0) / rect.2;
                let others: Vec<PanelId> = bar
                    .ids
                    .iter()
                    .copied()
                    .filter(|&id| id != source)
                    .collect();
                if rel < 0.20 {
                    if let Some(target) = others.first().copied() {
                        return DockDrop {
                            guide: Some(DockGuide::Inner {
                                target,
                                dir: DockDirection::West,
                            }),
                            rect,
                            outside: false,
                        };
                    }
                } else if rel > 0.80 {
                    if let Some(target) = others.last().copied() {
                        return DockDrop {
                            guide: Some(DockGuide::Inner {
                                target,
                                dir: DockDirection::East,
                            }),
                            rect,
                            outside: false,
                        };
                    }
                }
            }
            let index = tab_insert_index(&bar.ids, gx, source);
            return DockDrop {
                guide: Some(DockGuide::Tab {
                    lead: bar.active,
                    index,
                }),
                rect,
                outside: false,
            };
        }
    }
    // 3) 内层小罗盘：悬停目标组中心的 5 向十字，停靠到该组 / 中心并标签。
    let groups: Vec<GroupRect> = GROUP_RECTS.with(|g| g.borrow().clone());
    for g in groups.iter().rev() {
        if g.ids.contains(&source) {
            continue;
        }
        let rect = node_abs_rect(g.node);
        if point_in(rect, gx, gy) {
            if let Some(dir) = dock_guide_hit(&dock_inner_guides(rect), gx, gy) {
                return DockDrop {
                    guide: Some(DockGuide::Inner {
                        target: g.active,
                        dir,
                    }),
                    rect,
                    outside: false,
                };
            }
        }
    }
    // 3) 外层十字：贴在停靠容器四条边中点的指南，命中即停放到最外层 / 整窗分割。
    if root.2 > 0.0 && point_in(root, gx, gy) {
        if let Some(dir) = dock_guide_hit(&dock_outer_guides(root), gx, gy) {
            return DockDrop {
                guide: Some(DockGuide::Outer { dir }),
                rect: root,
                outside: false,
            };
        }
    }
    // 4) 布局内但无指南 → 取消；布局外 → 浮起。
    DockDrop {
        guide: None,
        rect: root,
        outside: root.2 <= 0.0 || !point_in(root, gx, gy),
    }
}

fn begin_tab_drag(state: DockState, ids: Vec<PanelId>) {
    let id = ids.first().copied().unwrap_or(0); // lead：参与指南判定与事件
    let (gx, gy) = last_pointer_global();
    state.drag.set(DockDragUi {
        source: Some(id),
        start_x: gx,
        start_y: gy,
        x: gx,
        y: gy,
        moved: false,
        target: None,
        direction: None,
        target_rect: None,
        container_rect: dock_layout_rect(),
        root_direction: None,
        guide: None,
        caret: None,
    });
    let on_move = move |x: f32, y: f32| {
        let mut d = state.drag.get();
        if !d.moved
            && (x - d.start_x).abs() + (y - d.start_y).abs() > DOCK_DRAG_THRESHOLD
        {
            d.moved = true;
            set_cursor(Cursor::Grabbing);
        }
        d.x = x;
        d.y = y;
        if d.moved {
            apply_drop_feedback(&mut d, dock_drop_target(x, y, id), id);
        }
        state.drag.set(d);
    };
    let on_end = move |x: f32, y: f32| {
        set_cursor(Cursor::Default);
        let d = state.drag.get();
        state.drag.set(DockDragUi::idle());
        if !d.moved {
            return;
        }
        commit_drop_group(state, &ids, id, dock_drop_target(x, y, id));
    };
    let on_cancel = move || {
        set_cursor(Cursor::Default);
        state.drag.set(DockDragUi::idle());
    };
    begin_drag_2d_full(on_move, on_end, on_cancel);
}

/// 把投放判定写进拖拽反馈（旧字段保留兼容，新字段驱动 VS 指南渲染）。
fn apply_drop_feedback(d: &mut DockDragUi, drop: DockDrop, source: PanelId) {
    d.guide = drop.guide;
    // 判定期（拖拽事件回调）ROOT_NODE 是上一趟已 place 的内容区，矩形真实。
    let container = dock_layout_rect();
    if container.2 > 0.0 && container.3 > 0.0 {
        d.container_rect = container;
    }
    d.target_rect = if drop.outside { None } else { Some(drop.rect) };
    d.caret = None;
    d.target = None;
    d.direction = None;
    d.root_direction = None;
    match drop.guide {
        Some(DockGuide::Inner { target, dir }) => {
            d.target = Some(target);
            d.direction = Some(dir);
        }
        Some(DockGuide::Outer { dir }) => {
            d.direction = Some(dir);
            d.root_direction = Some(dir);
        }
        Some(DockGuide::Tab { lead, index }) => {
            d.target = Some(lead);
            d.direction = Some(DockDirection::Center);
            let ids = TAB_BARS.with(|b| {
                b.borrow()
                    .iter()
                    .find(|bar| bar.active == lead)
                    .map(|bar| bar.ids.clone())
                    .unwrap_or_default()
            });
            d.caret = Some((tab_caret_x(&ids, index, source), drop.rect));
        }
        None => {}
    }
}

/// 抬起结算：指南 → 重排 / 插标签 / 分割 / 整窗分割；布局外 → 浮起；无指南 → 取消。
fn commit_drop(state: DockState, id: PanelId, drop: DockDrop) {
    match drop.guide {
        Some(DockGuide::Tab { lead, index }) => {
            let same_group = state.group_of(id).contains(&lead);
            if same_group {
                state.reorder_tab(id, index);
            } else {
                state.dock_as_tab(id, lead, index);
            }
        }
        Some(DockGuide::Inner { target, dir }) => {
            state.dock(id, Some(target), dir, 0.5);
        }
        Some(DockGuide::Outer { dir }) => {
            state.dock(id, None, dir, 0.25);
        }
        None => {
            if drop.outside && dock_panel_info(id).is_some_and(|p| p.options.floatable) {
                state.float_panel(id);
            }
        }
    }
}

/// 整组投放（拖标签条空白区时使用）：组内全部面板作为一个单元移动，
/// 投到目标组中心 = 并标签，投到边区 = 分割整组，拖出窗口 = 整组浮起。
/// 单面板直接复用原提交逻辑（行为零回归）。
fn commit_drop_group(state: DockState, ids: &[PanelId], lead: PanelId, drop: DockDrop) {
    if ids.len() <= 1 {
        commit_drop(state, lead, drop);
        return;
    }
    let group = DockNode::tabs(ids);
    match drop.guide {
        Some(DockGuide::Tab { lead: target, index }) => {
            if state.group_of(lead).contains(&target) {
                state.reorder_tab(lead, index);
            } else {
                state.dock_group(group, lead, Some(target), DockDirection::Center, 0.5);
                state.reorder_tab(lead, index);
            }
        }
        Some(DockGuide::Inner { target, dir }) => {
            state.dock_group(group, lead, Some(target), dir, 0.5);
        }
        Some(DockGuide::Outer { dir }) => {
            state.dock_group(group, lead, None, dir, 0.25);
        }
        None => {
            if drop.outside && dock_panel_info(lead).is_some_and(|p| p.options.floatable) {
                state.float_panel(lead); // 同组整体浮起（参考实现语义）
            }
        }
    }
}

fn path_key(path: &[u8]) -> u64 {
    let mut k = 0xD0C0_1234u64;
    for &b in path {
        k = k.wrapping_mul(131).wrapping_add(b as u64 + 1);
    }
    k
}

fn begin_splitter_drag(rid: NodeId, path: Vec<u8>, horizontal: bool, state: DockState) {
    let parent = with_rt_pub(|rt| rt.nodes[rid.index()].parent);
    let Some(p) = parent else {
        return;
    };
    let (px, py, pw, ph) = node_abs_rect(p);
    let (origin, total) = if horizontal { (px, pw) } else { (py, ph) };
    if total <= 0.0 {
        return;
    }
    let root = state.model().root;
    let (lo, hi) = ratio_bounds(root.as_ref(), &path, horizontal, total, state.settings());
    // VS：拖分割条时指针保持双向箭头；Esc 取消恢复拖拽前比例。
    set_cursor(if horizontal { Cursor::ResizeEw } else { Cursor::ResizeNs });
    let orig = state
        .model()
        .root
        .as_ref()
        .and_then(|r| ratio_by_path(r, &path));
    let cancel_path = path.clone();
    let on_move = move |x: f32, y: f32| {
        let cur = if horizontal { x } else { y };
        let nr = ((cur - origin) / total).clamp(lo, hi);
        state.set_split_ratio(&path, nr);
    };
    let on_end = move |_x, _y| set_cursor(Cursor::Default);
    let on_cancel = move || {
        if let Some(r) = orig {
            state.set_split_ratio(&cancel_path, r);
        }
        set_cursor(Cursor::Default);
    };
    begin_drag_2d_full(on_move, on_end, on_cancel);
}

// ══════════════════════════════ 渲染 ══════════════════════════════

/// 停靠区入口（KT `DockArea(state, modifier)`）——[`DockLayout`] 的同义别名。
///
/// KT 用 `DockArea` 命名渲染槽位、`DockLayout` 命名持久化模型；本库的渲染入口历史上
/// 叫 `DockLayout`，故两者都可用，新代码建议按 KT 语义写 [`DockArea`]。
#[allow(non_snake_case)]
pub fn DockArea(state: DockState, modifier: Modifier) {
    DockLayout(state, modifier)
}

/// 停靠布局主入口。
///
/// 面板由 [`register_dock_panel`] 注册；`modifier` 一般应带 `fill_max_size()`。
/// 所有交互（分割条、标签头、边条、浮动窗口）都在本控件的组合树里完成。
#[allow(non_snake_case)] // composable 命名与全库控件一致
pub fn DockLayout(state: DockState, modifier: Modifier) {
    // 注：本函数（值命名空间）与 [`DockLayout`](super::dock_layout::DockLayout) 结构体
    // （类型命名空间）同名但语义不同——前者是停靠 UI 的渲染入口，后者是对齐
    // compose-docking `DockLayout.kt` 的**持久化窗口列表模型**。
    ACTIVE_DOCK.with(|c| c.set(Some(state)));
    let m = state.model();
    let d = state.drag();
    let menu = state.menu();
    let tab_dropdown = state.tab_dropdown.get();
    let colors = theme_colors();
    // 液态玻璃是全局 MutableState：在此读一次，把依赖锚定到 DockLayout 自身的作用域，
    // 保证开关切换时整棵停靠树重组（各表面内部再各自解析规格）。
    let _ = glass_spec().enabled;

    // 每趟组合刷新几何表：NodeId 会随重组换 arena 下标，绝不长期缓存。
    GROUP_RECTS.with(|g| g.borrow_mut().clear());
    TAB_RECTS.with(|t| t.borrow_mut().clear());
    TAB_BARS.with(|t| t.borrow_mut().clear());

    // 中央内容区自动缩小（VS 语义）：某边有自动隐藏边条时，内容区向内让出一条
    // 边条厚度——边条保持全高/全宽，内容不再被覆盖。
    let th = docking_theme().strip_thickness;
    let (mut pl, mut pt, mut pr, mut pb) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for h in &m.hidden {
        match h.edge {
            DockEdge::Left => pl = th,
            DockEdge::Right => pr = th,
            DockEdge::Top => pt = th,
            DockEdge::Bottom => pb = th,
        }
    }
    let content_mod = if (pl, pt, pr, pb) != (0.0, 0.0, 0.0, 0.0) {
        Modifier::new().fill_max_size().padding_ltrb(pl, pt, pr, pb)
    } else {
        Modifier::new().fill_max_size()
    };

    let root_id = add_node(Policy::Box(Alignment::TopStart), modifier);
    ROOT_NODE.with(|r| r.set(Some(root_id)));

    let tree = m.root.clone();
    let maximized = m.maximized;
    let hidden = m.hidden.clone();
    let floating = m.floating.clone();
    let expanded = m.expanded;

    with_content_scope(root_id, move || {
        if let Some(mid) = maximized {
            render_panel_group(state, &DockNode::tabs(&[mid]), content_mod.clone());
        } else if let Some(root) = &tree {
            render_node(state, root, &[], content_mod.clone());
        } else {
            Box(
                Modifier::new().fill_max_size(),
                Alignment::Center,
                move || {
                    Text(
                        "所有面板已关闭 · 右键或菜单可重置布局",
                        tokens::body_small(colors.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        }

        // 浮动窗口（后组合 → 覆盖在主布局之上）。
        for f in &floating {
            render_floating(state, f, colors);
        }

        // 自动隐藏边条。
        render_edge_strips(state, &hidden, colors);

        // 滑出预览（peek）：全屏遮罩 + 边缘面板。
        if let Some(pid) = expanded {
            render_peek(state, pid, colors);
        }

        // 拖放反馈：半区预览 + 5 区罗盘 + 跟随光标的幽灵标签。
        render_drag_overlay(state, d, colors);

        // 右键上下文菜单。
        if let Some(pid) = menu.panel {
            render_context_menu(state, pid, menu.x, menu.y, colors);
        }

        // 标签条「▼ 全部面板」下拉（最后组合 → 浮在最上层）。
        if tab_dropdown.is_some() {
            render_tab_dropdown(state, tab_dropdown, colors);
        }
    });
}

fn render_node(state: DockState, node: &DockNode, path: &[u8], modifier: Modifier) {
    match node {
        DockNode::Tabs { .. } | DockNode::Leaf { .. } => render_panel_group(state, node, modifier),
        DockNode::Anchor { anchor_id, .. } => render_anchor(state, *anchor_id, modifier),
        DockNode::Split {
            orientation,
            proportion,
            first,
            second,
            ..
        } => {
            let r = *proportion;
            let mut p1 = path.to_vec();
            p1.push(0);
            let mut p2 = path.to_vec();
            p2.push(1);
            let first = first.clone();
            let second = second.clone();
            let splitter_path = path.to_vec();
            match orientation {
                SplitOrientation::Horizontal => Row(
                    modifier,
                    Arrangement::Start,
                    CrossAxisAlignment::Stretch,
                    move || {
                        render_node(
                            state,
                            &first,
                            &p1,
                            Modifier::new().weight(r, true).fill_max_height(),
                        );
                        render_splitter(state, &splitter_path, true);
                        render_node(
                            state,
                            &second,
                            &p2,
                            Modifier::new().weight(1.0 - r, true).fill_max_height(),
                        );
                    },
                ),
                SplitOrientation::Vertical => Column(
                    modifier,
                    Arrangement::Start,
                    CrossAxisAlignment::Stretch,
                    move || {
                        render_node(
                            state,
                            &first,
                            &p1,
                            Modifier::new().weight(r, true).fill_max_width(),
                        );
                        render_splitter(state, &splitter_path, false);
                        render_node(
                            state,
                            &second,
                            &p2,
                            Modifier::new().weight(1.0 - r, true).fill_max_width(),
                        );
                    },
                ),
            }
        }
    }
}

fn render_splitter(state: DockState, path: &[u8], horizontal: bool) {
    let colors = theme_colors();
    let key = path_key(path);
    let m = if horizontal {
        Modifier::new().width(th_splitter_w()).fill_max_height()
    } else {
        Modifier::new().height(th_splitter_w()).fill_max_width()
    };
    let rid = add_node(Policy::Leaf, m);
    let p = path.to_vec();
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[rid.index()];
        // `dispatch_press_down` 只认「最近的 on_click 祖先」，故 on_press 必须成对挂 on_click。
        // on_click 同时承担 VS 的「双击分割条 = 均分」。
        let p2 = p.clone();
        n.on_click = Some(Box::new(move || {
            let now = clock_nanos();
            let (last_key, last_t) = LAST_SPLIT_CLICK.with(|c| c.get());
            LAST_SPLIT_CLICK.with(|c| c.set((key, now)));
            if last_key == key && last_t >= 0.0 && now - last_t < 400_000_000.0 {
                state.set_split_ratio(&p2, 0.5);
            }
        }));
        n.on_press = Some(Box::new(move || {
            begin_splitter_drag(rid, p.clone(), horizontal, state)
        }));
        n.hover_handler = Some(Box::new(move |enter| {
            HOVER_SPLIT.with(|c| c.set(if enter { key } else { 0 }));
            set_cursor(if enter {
                if horizontal { Cursor::ResizeEw } else { Cursor::ResizeNs }
            } else {
                Cursor::Default
            });
            invalidate_draw_node(rid);
        }));
        n.draw = Some(Box::new(
            move |b: &mut DrawBuilder, x, y, w: f32, h: f32| {
                let hot = HOVER_SPLIT.with(|c| c.get()) == key;
                // VS：静止时是与背景几乎同色的缝，悬停才染主色并露出抓握点。
                let base = if hot {
                    dock_accent(colors).with_alpha(docking_theme().hover_alpha)
                } else {
                    colors.surface_container_high
                };
                b.fill_rect(x, y, w, h, base);
                let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                let line = if hot {
                    dock_accent(colors)
                } else {
                    colors.outline_variant.with_alpha(0.7)
                };
                if horizontal {
                    b.line(cx, y, cx, y + h, line, 1.0);
                    if hot {
                        for i in -1..=1 {
                            b.fill_circle(cx, cy + i as f32 * 7.0, 1.4, dock_accent(colors));
                        }
                    }
                } else {
                    b.line(x, cy, x + w, cy, line, 1.0);
                    if hot {
                        for i in -1..=1 {
                            b.fill_circle(cx + i as f32 * 7.0, cy, 1.4, dock_accent(colors));
                        }
                    }
                }
            },
        ));
    });
}

fn render_panel_group(state: DockState, node: &DockNode, modifier: Modifier) {
    let (items, active) = match node {
        DockNode::Tabs {
            tabs,
            selected_index,
            ..
        } => {
            let items: Vec<PanelId> = tabs.iter().flat_map(|t| t.collect_ids()).collect();
            let active = tabs
                .get(*selected_index)
                .and_then(|t| t.first_id())
                .unwrap_or(0);
            (items, active)
        }
        DockNode::Leaf { dockable_id, .. } => (vec![*dockable_id], *dockable_id),
        _ => return,
    };
    let colors = theme_colors();
    let id = add_node(
        Policy::Column {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Stretch,
        },
        dock_surface(
            modifier,
            glass_spec(),
            colors.Surface,
            tokens::SHAPE_EXTRA_SMALL,
            0.5,
            colors.outline_variant,
        ),
    );
    GROUP_RECTS.with(|g| {
        g.borrow_mut().push(GroupRect {
            ids: items.clone(),
            active,
            node: id,
        })
    });
    // 拖拽入口只挂在标签条（tab 本体 + 同一行空白）上，内容区刻意不挂 on_press：
    // 在面板内容里拖动不应触发 drag & drop 指南，也不应与内容交互（文本选择/按钮）抢事件。
    with_content_scope(id, move || {
        let bottom = tab_bar_on_bottom(state, active);
        if bottom {
            // VS2022 底部工具窗口：标签条在内容下方（输出 / 错误列表）。
            Box(
                Modifier::new().weight(1.0, true).fill_max_width(),
                Alignment::TopStart,
                move || call_dock_panel(active),
            );
            render_tab_bar(state, &items, active, colors);
        } else {
            render_tab_bar(state, &items, active, colors);
            Box(
                Modifier::new().weight(1.0, true).fill_max_width(),
                Alignment::TopStart,
                move || call_dock_panel(active),
            );
        }
    });
}

/// 渲染命名锚点占位（KT `DockNode.Anchor`）：空槽位提示，仍是合法投放目标。
///
/// 占位文案遵循 `DockingSettings::empty_anchor_visibility`：`Always` 常驻，
/// `WhileDragging` 只在拖拽期间露出提示（平时就是一片空槽）。
/// `collapsed_anchor_thickness > 0` 时不画文案，只画一条贴边 accent，视觉上「收起」。
fn render_anchor(state: DockState, anchor_id: AnchorId, modifier: Modifier) {
    let colors = theme_colors();
    let st = state.settings();
    let dragging = state.drag().source.is_some();
    let show_label = match st.empty_anchor_visibility {
        EmptyAnchorVisibility::Always => !st.collapses_empty_anchors(),
        EmptyAnchorVisibility::WhileDragging => dragging && !st.collapses_empty_anchors(),
    };
    let hint = dragging;
    let id = add_node(
        Policy::Box(Alignment::TopStart),
        dock_surface(
            modifier,
            glass_spec(),
            colors.surface_variant,
            tokens::SHAPE_EXTRA_SMALL,
            0.5,
            colors.outline_variant,
        ),
    );
    with_content_scope(id, move || {
        if st.collapses_empty_anchors() {
            // 收起态：贴边 accent 条（宽度=collapsed_anchor_thickness）。
            let t = st.collapsed_anchor_thickness;
            let accent = if hint {
                dock_accent(colors)
            } else {
                colors.outline_variant
            };
            Canvas(
                Modifier::new().fill_max_size().no_hit(),
                move |b: &mut DrawBuilder, x, y, _w, h| {
                    b.fill_rect(x, y, t.max(1.0), h, accent);
                },
            );
        }
        if show_label {
            Box(
                Modifier::new().fill_max_size(),
                Alignment::Center,
                move || {
                    Text(
                        format!("锚点 #{anchor_id}"),
                        tokens::label_small(colors.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        }
    });
}

fn render_tab_bar(state: DockState, items: &[PanelId], active: PanelId, colors: ColorScheme) {
    let items = items.to_vec();
    let g = glass_spec();
    // VS：文档井（document well）与工具窗口标题条是两种视觉语言。
    let is_doc = items.iter().any(|&id| {
        dock_panel_info(id).is_some_and(|p| p.options.r#type.as_deref() == Some("document"))
    });
    let bar = add_node(
        Policy::Row {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Center,
        },
        Modifier::new()
            .fill_max_width()
            .height(th_header_h())
            .background(
                dock_inner_color(
                    if is_doc {
                        colors.surface_container
                    } else {
                        colors.surface_container_low
                    },
                    g,
                ),
                0.0,
            ),
    );
    TAB_BARS.with(|t| {
        t.borrow_mut().push(TabBarRect {
            ids: items.clone(),
            active,
            node: bar,
        })
    });
    let bar_ids = items.clone();
    let bar_state = state;
    with_rt_pub(|rt| {
        let node = &mut rt.nodes[bar.index()];
        // 空 on_click 仅用于让按下派发把本节点识别为「可交互链」（ dispatch_press_down
        // 沿祖先找最近的 on_click 才登记按压）；真实动作由 on_press 的整组拖拽承担。
        node.on_click = Some(Box::new(|| {}));
        node.on_press = Some(Box::new(move || {
            begin_tab_drag(bar_state, bar_ids.clone());
        }));
    });
    with_content_scope(bar, move || {
        // 标签井：溢出时横向滚动（滚轮 / 两侧箭头），隐藏滚动条保持 VS 的干净井边。
        let well_items = items.clone();
        let well = HorizontalScroll(
            Modifier::new()
                .weight(1.0, true)
                .fill_max_height()
                .hide_scrollbar(),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            move || {
                for id in well_items.iter().copied() {
                    render_tab(state, id, id == active, is_doc, colors);
                }
            },
        );
        let (off, content, viewport) = scroll_metrics(well).unwrap_or((0.0, 0.0, 1.0));
        let overflow = content > viewport + 1.0;
        let dim = colors.on_surface_variant.with_alpha(if overflow { 0.9 } else { 0.3 });
        Clickable(
            Modifier::new().size(20.0, th_header_h()).no_indication(),
            move || scroll_by(well, -140.0),
            move || icon(IconName::ChevronLeft, 12.0, dim, Modifier::new()),
        );
        Clickable(
            Modifier::new().size(20.0, th_header_h()).no_indication(),
            move || scroll_by(well, 140.0),
            move || icon(IconName::ChevronRight, 12.0, dim, Modifier::new()),
        );
        let lead = active;
        // VS 工具窗口三键：钉住（自动隐藏）/ 下拉 / 关闭。最大化走双击标题。
        let info = dock_panel_info(lead).unwrap_or_default();
        if info.auto_hide_allowed {
            pin_button(colors, false, move || state.toggle_auto_hide(lead));
        }
        dock_action_button(IconName::ChevronDown, colors, move || {
            let (x, y) = last_pointer_global();
            state.open_tab_dropdown(lead, x, y);
        });
        if info.options.closable {
            dock_action_button(IconName::Close, colors, move || state.close(lead));
        }
    });
}

/// VS 的图钉按钮：停靠态横放（点下即自动隐藏），自动隐藏态竖放（点下即钉回）。
fn pin_button(colors: ColorScheme, vertical: bool, on_click: impl FnMut() + 'static) {
    Clickable(
        Modifier::new().size(24.0, 24.0).no_indication(),
        on_click,
        move || {
            Canvas(
                Modifier::new().size(14.0, 14.0).no_hit(),
                move |b: &mut DrawBuilder, x, y, w, h| {
                    let c = colors.on_surface_variant;
                    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                    if vertical {
                        b.fill_circle(cx, y + h * 0.30, w * 0.20, c);
                        b.fill_path(
                            &[
                                (cx - w * 0.06, y + h * 0.48),
                                (cx + w * 0.06, y + h * 0.48),
                                (cx, y + h * 0.92),
                            ],
                            c,
                        );
                        b.line(cx - w * 0.30, y + h * 0.48, cx + w * 0.30, y + h * 0.48, c, 1.4);
                    } else {
                        // 横放：针尖朝左（VS 停靠态图钉）。
                        b.fill_circle(x + w * 0.70, cy, h * 0.20, c);
                        b.fill_path(
                            &[
                                (x + w * 0.52, cy - h * 0.06),
                                (x + w * 0.52, cy + h * 0.06),
                                (x + w * 0.08, cy),
                            ],
                            c,
                        );
                        b.line(x + w * 0.52, cy - h * 0.30, x + w * 0.52, cy + h * 0.30, c, 1.4);
                    }
                },
            );
        },
    );
}

/// 标签标题按测量截断（VS 长文件名的省略号），保证标签井宽度可预期。
fn tab_title(id: PanelId) -> String {
    let title = panel_title(id);
    let style = tokens::label_medium(Color::BLACK);
    let (w, _) = compose::layout::measure_text_with_measurer(&title, &style);
    if w <= th_tab_max_w() - 34.0 {
        return title;
    }
    let mut out = String::new();
    let mut acc = 0.0;
    for ch in title.chars() {
        let (cw, _) = compose::layout::measure_text_with_measurer(&ch.to_string(), &style);
        if acc + cw > th_tab_max_w() - 46.0 {
            break;
        }
        acc += cw;
        out.push(ch);
    }
    out.push('…');
    out
}

fn render_tab(state: DockState, id: PanelId, active: bool, is_doc: bool, colors: ColorScheme) {
    let title = tab_title(id);
    let info = dock_panel_info(id).unwrap_or_default();
    let g = glass_spec();
    let hovered = state.hover_tab() == id;
    let cid = add_node(
        Policy::Row {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Center,
        },
        Modifier::new().height(th_header_h()).padding_hv(10.0, 0.0),
    );
    TAB_RECTS.with(|t| t.borrow_mut().push(TabRect { id, node: cid }));
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[cid.index()];
        // 单击切换 / 双击最大化（0.4s 阈值）。
        n.on_click = Some(Box::new(move || {
            let now = clock_nanos();
            let (last_id, last_t) = LAST_HEADER_PRESS.with(|c| c.get());
            LAST_HEADER_PRESS.with(|c| c.set((id, now)));
            if last_id == id
                && last_t >= 0.0
                && now - last_t < 400_000_000.0
                && dock_panel_info(id).is_some_and(|p| p.min_max_allowed)
            {
                state.toggle_maximize(id);
            } else {
                state.switch_tab(id);
            }
        }));
        n.on_press = Some(Box::new(move || begin_tab_drag(state, vec![id])));
        n.on_secondary_click = Some(Box::new(move || {
            let (x, y) = last_pointer_global();
            state.menu.set(DockContextMenu {
                panel: Some(id),
                x,
                y,
            });
        }));
        // VS：中键点击标签 = 关闭（不可关闭面板除外）。
        if info.options.closable {
            n.on_middle_click = Some(Box::new(move || state.close(id)));
        }
        n.hover_handler = Some(Box::new(move |enter| {
            HOVER_TAB.with(|c| c.set(if enter { id } else { 0 }));
            invalidate_draw_node(cid);
            state.set_hover_tab(if enter { id } else { 0 });
        }));
        // 背景 / 强调线走绘制闭包：悬停反馈不需要重组整条标签井。
        n.draw = Some(Box::new(
            move |b: &mut DrawBuilder, x, y, w: f32, h: f32| {
                let hot = HOVER_TAB.with(|c| c.get()) == id;
                let bg = if active {
                    dock_inner_color(colors.Surface, g)
                } else if hot {
                    dock_inner_color(colors.surface_container_high, g)
                } else {
                    Color::TRANSPARENT
                };
                if bg.a > 0.0 {
                    b.fill_rect(x, y, w, h, bg);
                }
                if active {
                    // VS2022：文档井强调线在标签**顶**缘，工具窗口标签在底缘。
                    if is_doc {
                        b.fill_rect(x, y, w, 2.0, dock_accent(colors));
                    } else {
                        b.fill_rect(x, y + h - 2.0, w, 2.0, dock_accent(colors));
                    }
                }
                // 标签之间的 1px 分隔缝（VS 井内标签边界）。
                b.fill_rect(x + w - 1.0, y + 6.0, 1.0, h - 12.0, colors.outline_variant.with_alpha(0.6));
            },
        ));
    });
    with_content_scope(cid, move || {
        Text(
            title.clone(),
            tokens::label_medium(if active {
                colors.on_surface
            } else {
                colors.on_surface_variant
            }),
            Modifier::new(),
        );
        // VS：关闭按钮只在活动标签或悬停标签上出现。
        if info.options.closable && (active || hovered) {
            Spacer(Modifier::new().width(6.0));
            Clickable(
                Modifier::new().size(16.0, 16.0).no_indication(),
                move || state.close(id),
                move || icon(IconName::Close, 10.0, colors.on_surface_variant, Modifier::new()),
            );
        }
    });
}

fn dock_action_button(
    name: IconName,
    colors: ColorScheme,
    on_click: impl FnMut() + 'static,
) {
    Clickable(
        Modifier::new().size(24.0, 24.0).no_indication(),
        on_click,
        move || icon(name, 14.0, colors.on_surface_variant, Modifier::new()),
    );
}

// ── 拖放反馈 ──

fn render_drag_overlay(state: DockState, d: DockDragUi, colors: ColorScheme) {
    let _ = state;
    if !d.moved || d.source.is_none() {
        return;
    }
    let title = d.source.map(panel_title).unwrap_or_default();
    let guide = d.guide;
    let rect = d.target_rect;
    let caret = d.caret;
    // 外层十字画在停靠容器四边。容器矩形用判定期缓存的 DockLayout 内容区真实几何
    // （不含标题栏/菜单栏）；组合期现读 ROOT_NODE 尚未 place、绝对矩形为 0。
    let root = d.container_rect;
    Canvas(
        Modifier::new().fill_max_size().no_hit(),
        move |b: &mut DrawBuilder, _x, _y, _w, _h| {
            let primary = dock_accent(colors);
            // 1) 投放预览：半区填充 + 描边（VS 的蓝色落点预览）。
            if let (Some(r), Some(g)) = (rect, guide) {
                match g {
                    DockGuide::Tab { .. } => {
                        b.fill_rrect(r.0, r.1, r.2, r.3, 2.0, primary.with_alpha(0.14));
                        b.stroke_rrect(r.0, r.1, r.2, r.3, 2.0, primary.with_alpha(0.65), 1.0);
                    }
                    _ => {
                        let pv = dock_preview_rect(r, g.direction());
                        b.fill_rrect(pv.0, pv.1, pv.2, pv.3, 4.0, primary.with_alpha(0.14));
                        b.stroke_rrect(pv.0, pv.1, pv.2, pv.3, 4.0, primary.with_alpha(0.6), 1.5);
                    }
                }
            }
            // 2) VS 双十字——外层十字：贴在停靠容器四条边中点的 4 向指南，
            //    命中即停放到最外层 / 整窗分割。
            if root.2 > 0.0 && root.3 > 0.0 {
                for (dir, gr) in dock_outer_guides(root) {
                    let hot = guide == Some(DockGuide::Outer { dir });
                    draw_guide_box(b, gr, Some(dir), hot, colors, true);
                }
            }
            // 3) VS 双十字——内层小罗盘（悬停目标中心）：停靠到该目标组。
            //    命中 Tab 时隐藏内层以免标签插入光标与大十字混在一起。
            if let Some(r) = rect {
                if !matches!(guide, Some(DockGuide::Tab { .. })) {
                    let active = match guide {
                        Some(DockGuide::Inner { dir, .. }) => Some(dir),
                        _ => None,
                    };
                    for (dir, gr) in dock_inner_guides(r) {
                        draw_guide_box(b, gr, Some(dir), active == Some(dir), colors, false);
                    }
                }
            }
            // VS 双十字只画两层：外层大十字 + 内层小罗盘。旧的四边贴边方块
            // 会形成令人困惑的第三组十字，已从拖拽覆盖层移除（函数仅留作说明页图示）。
            // 4) 标签条插入光标。
            if let Some((cx, bar)) = caret {
                b.fill_rect(cx - 1.0, bar.1 + 2.0, 2.0, bar.3 - 4.0, primary);
                b.fill_path(
                    &[
                        (cx - 4.0, bar.1 + 2.0),
                        (cx + 4.0, bar.1 + 2.0),
                        (cx, bar.1 + 7.0),
                    ],
                    primary,
                );
                b.fill_path(
                    &[
                        (cx - 4.0, bar.1 + bar.3 - 2.0),
                        (cx + 4.0, bar.1 + bar.3 - 2.0),
                        (cx, bar.1 + bar.3 - 7.0),
                    ],
                    primary,
                );
            }
            // 5) 跟随光标的幽灵窗口（VS 拖拽时的半透明工具窗快照）。
            let (gx, gy, gw, gh) = (d.x + 14.0, d.y + 18.0, 210.0, 132.0);
            b.fill_rrect(gx + 4.0, gy + 6.0, gw, gh, 6.0, Color::BLACK.with_alpha(0.18));
            b.fill_rrect(gx, gy, gw, gh, 6.0, colors.Surface.with_alpha(0.94));
            b.stroke_rrect(gx, gy, gw, gh, 6.0, colors.outline.with_alpha(0.7), 1.0);
            b.fill_rrect(gx, gy, gw, 26.0, 6.0, primary.with_alpha(0.92));
            b.fill_rect(gx, gy + 16.0, gw, 10.0, primary.with_alpha(0.92));
            b.draw_text(
                gx + 10.0,
                gy + 6.0,
                &title,
                &tokens::label_medium(colors.on_primary),
                gw - 20.0,
            );
            for (i, lw) in [0.82f32, 0.62, 0.72, 0.45].iter().enumerate() {
                b.fill_rect(
                    gx + 12.0,
                    gy + 40.0 + i as f32 * 18.0,
                    (gw - 24.0) * lw,
                    8.0,
                    colors.on_surface_variant.with_alpha(0.22),
                );
            }
        },
    );
}

/// 单个指南格：圆角底 + 描边 + 方向图标；`outer` 为 VS 外圈「整窗分割」图标。
fn draw_guide_box(
    b: &mut DrawBuilder,
    r: DockRect,
    dir: Option<DockDirection>,
    hot: bool,
    colors: ColorScheme,
    outer: bool,
) {
    let (bx, by, s, _) = r;
    let fill = if hot {
        dock_accent(colors).with_alpha(0.95)
    } else {
        colors.Surface.with_alpha(0.94)
    };
    b.fill_rrect(bx, by, s, s, 5.0, fill);
    b.stroke_rrect(
        bx,
        by,
        s,
        s,
        5.0,
        if hot {
            dock_accent(colors)
        } else {
            colors.outline.with_alpha(0.6)
        },
        1.0,
    );
    let fg = if hot {
        colors.on_primary
    } else {
        colors.on_surface_variant
    };
    let (ax, ay) = (bx + s / 2.0, by + s / 2.0);
    if outer {
        // 外圈图标：一个小窗被切出一半靠边。
        let ix = bx + 7.0;
        let iy = by + 7.0;
        let is = s - 14.0;
        b.stroke_rrect(ix, iy, is, is, 2.0, fg, 1.4);
        match dir {
            Some(DockDirection::North) => b.fill_rect(ix, iy, is, is * 0.42, fg),
            Some(DockDirection::South) => b.fill_rect(ix, iy + is * 0.58, is, is * 0.42, fg),
            Some(DockDirection::West) => b.fill_rect(ix, iy, is * 0.42, is, fg),
            Some(DockDirection::East) => b.fill_rect(ix + is * 0.58, iy, is * 0.42, is, fg),
            _ => b.fill_rect(ix + 3.0, iy + 3.0, is - 6.0, is - 6.0, fg),
        }
        return;
    }
    let a = s * 0.26;
    match dir {
        Some(DockDirection::North) => {
            b.line(ax, ay + a, ax, ay - a, fg, 2.0);
            b.line(ax - a * 0.6, ay - a * 0.4, ax, ay - a, fg, 2.0);
            b.line(ax + a * 0.6, ay - a * 0.4, ax, ay - a, fg, 2.0);
        }
        Some(DockDirection::South) => {
            b.line(ax, ay - a, ax, ay + a, fg, 2.0);
            b.line(ax - a * 0.6, ay + a * 0.4, ax, ay + a, fg, 2.0);
            b.line(ax + a * 0.6, ay + a * 0.4, ax, ay + a, fg, 2.0);
        }
        Some(DockDirection::West) => {
            b.line(ax + a, ay, ax - a, ay, fg, 2.0);
            b.line(ax - a * 0.4, ay - a * 0.6, ax - a, ay, fg, 2.0);
            b.line(ax - a * 0.4, ay + a * 0.6, ax - a, ay, fg, 2.0);
        }
        Some(DockDirection::East) => {
            b.line(ax - a, ay, ax + a, ay, fg, 2.0);
            b.line(ax + a * 0.4, ay - a * 0.6, ax + a, ay, fg, 2.0);
            b.line(ax + a * 0.4, ay + a * 0.6, ax + a, ay, fg, 2.0);
        }
        _ => {
            b.stroke_rrect(bx + 8.0, by + 8.0, s - 16.0, s - 16.0, 2.0, fg, 2.0);
        }
    }
}

// ── 右键菜单 ──

const MENU_CLOSE: u8 = 0;
const MENU_MAXIMIZE: u8 = 1;
const MENU_AUTO_HIDE: u8 = 2;
const MENU_FLOAT: u8 = 3;
const MENU_CLOSE_OTHERS: u8 = 4;
const MENU_CLOSE_ALL: u8 = 5;
const MENU_SEPARATOR: u8 = 255;

fn menu_actions(state: DockState, id: PanelId) -> Vec<(&'static str, u8)> {
    let info = dock_panel_info(id).unwrap_or_default();
    let group = state.group_of(id);
    let mut v = Vec::new();
    if info.options.closable {
        v.push(("关闭", MENU_CLOSE));
        if group.len() > 1 {
            v.push(("关闭其他", MENU_CLOSE_OTHERS));
            v.push(("全部关闭", MENU_CLOSE_ALL));
        }
    }
    if !v.is_empty() {
        v.push(("", MENU_SEPARATOR));
    }
    if info.options.floatable {
        v.push((
            if state.is_floating(id) {
                "停靠回主窗口"
            } else {
                "浮动为窗口"
            },
            MENU_FLOAT,
        ));
    }
    if info.auto_hide_allowed {
        v.push((
            if state.is_hidden(id) {
                "取消自动隐藏"
            } else {
                "自动隐藏"
            },
            MENU_AUTO_HIDE,
        ));
    }
    if info.min_max_allowed {
        v.push((
            if state.is_maximized(id) {
                "还原"
            } else {
                "最大化"
            },
            MENU_MAXIMIZE,
        ));
    }
    while v.last().is_some_and(|(_, c)| *c == MENU_SEPARATOR) {
        v.pop();
    }
    v
}

/// VS2022 标签条「▼ 全部面板」下拉：列出活动面板所在标签组的全部成员，点选切换。
///
/// 浮层画在 DockLayout 根节点内（全屏遮罩 + 绝对定位列表），故四个方向的面板组、
/// 浮动窗口标签条都能共用，且不依赖外部 OverlayHost。
fn render_tab_dropdown(state: DockState, drop: DockTabDropdown, colors: ColorScheme) {
    let Some((lead, x, y)) = drop else { return };
    // 找到 lead 所在的标签组（主窗口树 / 浮动窗口树 / 自动隐藏组）。
    let items = state.read(|m| {
        let in_tree = m
            .root
            .as_ref()
            .and_then(|r| find_group(r, lead).map(|g| g.collect_ids()));
        in_tree
            .or_else(|| {
                m.floating
                    .iter()
                    .find(|f| f.contains(lead))
                    .and_then(|f| find_group(&f.root, lead).map(|g| g.collect_ids()))
            })
            .or_else(|| {
                m.hidden
                    .iter()
                    .find(|h| h.items.contains(&lead))
                    .map(|h| h.items.clone())
            })
            .unwrap_or_default()
    });
    if items.is_empty() {
        return;
    }
    let active = items
        .iter()
        .copied()
        .find(|&p| p == lead)
        .unwrap_or(lead);
    let height = items.len() as f32 * 30.0 + 8.0;
    let g = glass_spec();
    // 点击别处即关闭。
    Clickable(
        Modifier::new().fill_max_size().no_indication(),
        move || state.tab_dropdown.set(None),
        || {},
    );
    // `x/y` 是 `last_pointer_global()` 的**窗口坐标**，而本浮层挂在 DockLayout 根节点下
    // （根节点自身可能带 padding / 位于菜单栏之下）。必须用 `pin_at_global` 在落位期
    // 换算成父内偏移；用 `offset` 会把全局坐标当局部位移，靠右的点击再被硬 clamp
    // 就会把下拉拉到布局中间。
    Column(
        dock_surface(
            Modifier::new().pin_at_global(x, y).size(200.0, height),
            g,
            colors.surface_container_high,
            tokens::SHAPE_EXTRA_SMALL,
            1.0,
            colors.outline_variant,
        ),
        Arrangement::Center,
        CrossAxisAlignment::Stretch,
        move || {
            for id in items.iter().copied() {
                let on = id == active;
                let title = if on {
                    format!("✓ {}", panel_title(id))
                } else {
                    format!("   {}", panel_title(id))
                };
                Clickable(
                    Modifier::new()
                        .fill_max_width()
                        .height(th_header_h())
                        .padding_hv(10.0, 0.0)
                        .background(
                            dock_inner_color(
                                if on {
                                    dock_accent_container(colors)
                                } else {
                                    colors.surface_container_high
                                },
                                g,
                            ),
                            0.0,
                        )
                        .no_indication(),
                    move || {
                        state.switch_tab(id);
                        state.tab_dropdown.set(None);
                    },
                    move || {
                        Text(
                            title.clone(),
                            tokens::body_small(if on {
                                colors.on_primary_container
                            } else {
                                colors.on_surface
                            }),
                            Modifier::new(),
                        );
                    },
                );
            }
        },
    );
}

fn apply_menu_action(state: DockState, id: PanelId, code: u8) {
    match code {
        MENU_CLOSE => state.close(id),
        MENU_MAXIMIZE => state.toggle_maximize(id),
        MENU_AUTO_HIDE => state.toggle_auto_hide(id),
        MENU_FLOAT => state.toggle_float(id),
        MENU_CLOSE_OTHERS => state.close_others(id),
        MENU_CLOSE_ALL => state.close_all(id),
        _ => {}
    }
}

fn render_context_menu(
    state: DockState,
    id: PanelId,
    x: f32,
    y: f32,
    colors: ColorScheme,
) {
    let actions = menu_actions(state, id);
    let height: f32 = actions
        .iter()
        .map(|(_, c)| if *c == MENU_SEPARATOR { 9.0 } else { 32.0 })
        .sum::<f32>()
        + 8.0;
    let g = glass_spec();
    // 全屏遮罩：点击别处即关闭菜单。
    Clickable(
        Modifier::new().fill_max_size().no_indication(),
        move || state.menu.set(DockContextMenu::idle()),
        || {},
    );
    // 与标签条下拉同理：右键坐标是窗口全局坐标，需在落位期换算成父内偏移。
    Column(
        dock_surface(
            Modifier::new().pin_at_global(x, y).size(190.0, height),
            g,
            colors.surface_container_high,
            tokens::SHAPE_EXTRA_SMALL,
            1.0,
            colors.outline_variant,
        ),
        Arrangement::Center,
        CrossAxisAlignment::Stretch,
        move || {
            for (label, code) in actions.iter().copied() {
                if code == MENU_SEPARATOR {
                    Box(
                        Modifier::new().fill_max_width().height(9.0),
                        Alignment::Center,
                        move || {
                            Canvas(
                                Modifier::new().fill_max_width().height(1.0).no_hit(),
                                move |b: &mut DrawBuilder, x, y, w, _h| {
                                    b.fill_rect(x + 8.0, y, w - 16.0, 1.0, colors.outline_variant);
                                },
                            );
                        },
                    );
                    continue;
                }
                Clickable(
                    Modifier::new()
                        .fill_max_width()
                        .height(32.0)
                        .padding_hv(10.0, 0.0)
                        .no_indication(),
                    move || {
                        apply_menu_action(state, id, code);
                        state.menu.set(DockContextMenu::idle());
                    },
                    move || {
                        Text(
                            label,
                            tokens::body_small(colors.on_surface),
                            Modifier::new(),
                        );
                    },
                );
            }
        },
    );
}

// ── 自动隐藏边条 / peek ──

fn render_edge_strips(state: DockState, hidden: &[HiddenPanel], colors: ColorScheme) {
    let g = glass_spec();
    for edge in [
        DockEdge::Left,
        DockEdge::Right,
        DockEdge::Top,
        DockEdge::Bottom,
    ] {
        let tabs: Vec<(PanelId, String)> = hidden
            .iter()
            .filter(|h| h.edge == edge)
            .map(|h| (h.active, panel_title(h.active)))
            .collect();
        if tabs.is_empty() {
            continue;
        }
        if edge.is_side() {
            Column(
                dock_surface(
                    Modifier::new()
                        .align(edge.strip_align())
                        .width(docking_theme().strip_thickness)
                        .fill_max_height(),
                    g,
                    colors.surface_container,
                    0.0,
                    0.5,
                    colors.outline_variant,
                ),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    for (id, title) in tabs.iter().cloned() {
                        render_strip_tab(state, id, title, edge, colors);
                    }
                },
            );
        } else {
            Row(
                dock_surface(
                    Modifier::new()
                        .align(edge.strip_align())
                        .height(docking_theme().strip_thickness)
                        .fill_max_width(),
                    g,
                    colors.surface_container,
                    0.0,
                    0.5,
                    colors.outline_variant,
                ),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    for (id, title) in tabs.iter().cloned() {
                        render_strip_tab(state, id, title, edge, colors);
                    }
                },
            );
        }
    }
}

fn render_strip_tab(
    state: DockState,
    id: PanelId,
    title: String,
    edge: DockEdge,
    colors: ColorScheme,
) {
    let expanded = state.read(|m| m.expanded) == Some(id);
    if edge.is_side() {
        // VS：侧边自动隐藏条的标题竖排（中文逐字叠放，西文旋转观感由叠排近似）。
        let chars: Vec<char> = title.chars().collect();
        let h = (chars.len() as f32 * 15.0 + 20.0).min(190.0);
        let cid = add_node(
            Policy::Leaf,
            Modifier::new().fill_max_width().height(h),
        );
        with_rt_pub(|rt| {
            let n = &mut rt.nodes[cid.index()];
            n.hover_handler = Some(Box::new(move |enter| {
                HOVER_STRIP.with(|c| c.set(if enter { id } else { 0 }));
                invalidate_draw_node(cid);
                if enter {
                    state.expand_from_edge(id);
                }
            }));
            n.draw = Some(Box::new(
                move |b: &mut DrawBuilder, x, y, w: f32, h: f32| {
                    let hot = HOVER_STRIP.with(|c| c.get()) == id || expanded;
                    if hot {
                        b.fill_rect(x, y, w, h, dock_accent_container(colors).with_alpha(0.55));
                        b.fill_rect(x + w - 2.0, y, 2.0, h, dock_accent(colors));
                    }
                    let style = tokens::label_small(if hot {
                        colors.on_primary_container
                    } else {
                        colors.on_surface_variant
                    });
                    let n = ((h - 14.0) / 15.0).floor() as usize;
                    for (i, ch) in chars.iter().take(n).enumerate() {
                        b.draw_text(
                            x + w / 2.0 - 6.0,
                            y + 8.0 + i as f32 * 15.0,
                            &ch.to_string(),
                            &style,
                            14.0,
                        );
                    }
                },
            ));
        });
        with_rt_pub(|rt| {
            let n = &mut rt.nodes[cid.index()];
            n.on_click = Some(Box::new(move || {
                if state.read(|m| m.expanded) == Some(id) {
                    state.collapse_to_edge();
                } else {
                    state.expand_from_edge(id);
                }
            }));
        });
    } else {
        let label = title.clone();
        hoverable(
            Modifier::new().width(140.0).fill_max_height(),
            move |enter| {
                if enter {
                    state.expand_from_edge(id);
                }
            },
            move || {
                let label_inner = label.clone();
                Clickable(
                    Modifier::new()
                        .fill_max_size()
                        .no_indication()
                        .background(
                            if expanded {
                                dock_accent_container(colors).with_alpha(0.55)
                            } else {
                                Color::TRANSPARENT
                            },
                            0.0,
                        ),
                    move || {
                        if state.read(|m| m.expanded) == Some(id) {
                            state.collapse_to_edge();
                        } else {
                            state.expand_from_edge(id);
                        }
                    },
                    move || {
                        Text(
                            label_inner.clone(),
                            tokens::label_small(colors.on_surface_variant),
                            Modifier::new().padding(4.0),
                        );
                    },
                );
            },
        );
    }
}

fn render_peek(state: DockState, id: PanelId, colors: ColorScheme) {
    // 自动隐藏作用于整组（如「输出 / 错误列表」），peek 也必须拿到整组，
    // 否则只显示 active 那一个面板、标签条缺项。
    let (edge, items, active) = state.read(|m| {
        m.hidden
            .iter()
            .find(|h| h.items.contains(&id))
            .map(|h| (h.edge, h.items.clone(), h.active))
            .unwrap_or((DockEdge::Left, vec![id], id))
    });
    // 组合期 `dock_layout_rect()` 拿到的是本帧刚 add_node、尚未落位的根节点，尺寸为 0
    // ——用它算 peek 尺寸会把面板压成 0 高，标题与内容全挤在一起。
    // 实际尺寸交给布局约束（侧边 fill_max_height / 上下 fill_max_width），
    // 这里只按视口估算滑移距离与侧边宽度档。
    let (vw, vh) = viewport_size();
    let (w, h) = if edge.is_side() {
        ((vw * 0.30).clamp(240.0, 420.0), vh)
    } else {
        (vw, (vh * 0.32).clamp(180.0, 360.0))
    };
    // 入场滑移：key = 面板 id，切换预览对象即重播。
    let p = animate_progress_on(id, Tween::new(0.18));
    let slide = (1.0 - p) * if edge.is_side() { w } else { h };
    let (dx, dy) = match edge {
        DockEdge::Left => (-slide, 0.0),
        DockEdge::Right => (slide, 0.0),
        DockEdge::Top => (0.0, -slide),
        DockEdge::Bottom => (0.0, slide),
    };
    // 点击外部收起。
    Clickable(
        Modifier::new().fill_max_size().no_indication(),
        move || state.collapse_to_edge(),
        || {},
    );
    let size_mod = if edge.is_side() {
        Modifier::new().fill_max_height().width(w)
    } else {
        Modifier::new().fill_max_width().height(h)
    };
    Column(
        dock_surface(
            size_mod.align(edge.align()).offset(dx, dy),
            glass_spec(),
            colors.surface_container_high,
            tokens::SHAPE_SMALL,
            1.0,
            colors.outline,
        ),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            // 复用停靠态的整组渲染：标签条列出所有成员（输出 / 错误列表），
            // 内容与切换交互也完全一致；不再自绘只有单个标题的 header。
            render_panel_group(
                state,
                &tabs_group(&items, active),
                Modifier::new().fill_max_size(),
            );
        },
    );
}

// ── 浮动窗口 ──

fn render_floating(state: DockState, f: &FloatingPanel, colors: ColorScheme) {
    let lead = f.active();
    let active = lead;
    // 标签行展示活动面板所在标签组的成员（树形布局时即活动组，扁平组则整组）
    let items = find_group(&f.root, active)
        .map(|g| g.collect_ids())
        .unwrap_or_else(|| f.items());
    let (x, y, w, h) = (f.x, f.y, f.w, f.h);
    let title = panel_title(lead);
    let glass = glass_spec();
    let window = f.id;
    // VS 浮动工具窗的投影（两层软阴影）。用与窗框相同的 offset 落位路径铺底色节点，
    // 保证阴影与窗框逐帧同位（全屏 Canvas 绝对坐标在浮层重组时易残留旧位）。
    add_node(
        Policy::Box(Alignment::TopStart),
        Modifier::new()
            .offset(x + 6.0, y + 9.0)
            .size(w, h)
            .background(Color::BLACK.with_alpha(0.20), 8.0)
            .no_hit(),
    );
    add_node(
        Policy::Box(Alignment::TopStart),
        Modifier::new()
            .offset(x + 2.0, y + 4.0)
            .size(w, h)
            .background(Color::BLACK.with_alpha(0.12), 8.0)
            .no_hit(),
    );
    let root_id = add_node(
        Policy::Box(Alignment::TopStart),
        dock_surface(
            Modifier::new().offset(x, y).size(w, h),
            glass,
            colors.surface_container_high,
            6.0,
            0.5,
            colors.outline,
        ),
    );
    with_content_scope(root_id, move || {
        let title_c = title.clone();
        let items_c = items.clone();
        Column(
            Modifier::new().fill_max_size(),
            Arrangement::Start,
            CrossAxisAlignment::Stretch,
            move || {
                // 标题栏：拖动移动（落到主窗口指南上即回靠）；双击回靠保存位。
                let label = title_c.clone();
                let bar = add_node(
                    Policy::Row {
                        arrangement: Arrangement::Start,
                        cross: CrossAxisAlignment::Center,
                    },
                    Modifier::new()
                        .fill_max_width()
                        .height(th_header_h())
                        .background(dock_inner_color(dock_accent_container(colors), glass), 0.0),
                );
                with_rt_pub(|rt| {
                    let n = &mut rt.nodes[bar.index()];
                    n.on_click = Some(Box::new(move || {
                        let now = clock_nanos();
                        let (last_id, last_t) = LAST_HEADER_PRESS.with(|c| c.get());
                        LAST_HEADER_PRESS.with(|c| c.set((lead, now)));
                        if last_id == lead && last_t >= 0.0 && now - last_t < 400_000_000.0 {
                            state.unfloat_panel(lead);
                        }
                    }));
                    n.on_press = Some(Box::new(move || {
                        begin_floating_move(state, lead, window, x, y, w, h)
                    }));
                });
                let label_bar = label.clone();
                with_content_scope(bar, move || {
                    Text(
                        label_bar.clone(),
                        tokens::label_medium(colors.on_primary_container),
                        Modifier::new().padding_hv(10.0, 0.0),
                    );
                    Spacer(Modifier::new().weight(1.0, true));
                    dock_action_button(IconName::ChevronLeft, colors, move || {
                        state.unfloat_panel(lead)
                    });
                    dock_action_button(IconName::Close, colors, move || state.close(lead));
                });

                // 标签行：Always 变体强制单面板也显示；底部偏好置于内容之后（VS2022）。
                let pref = resolved_tab_preference(state.settings(), active);
                let items_c2 = items_c.clone();
                let show_tabs = items_c2.len() > 1 || pref.always_display_tabs();
                let content = |state: DockState, active: PanelId| {
                    Box(
                        Modifier::new().weight(1.0, true).fill_max_width(),
                        Alignment::TopStart,
                        move || call_dock_panel(active),
                    );
                };
                if tab_bar_on_bottom(state, active) {
                    content(state, active);
                    if show_tabs {
                        render_floating_tabs(state, &items_c2, active, colors);
                    }
                } else {
                    if show_tabs {
                        render_floating_tabs(state, &items_c2, active, colors);
                    }
                    content(state, active);
                }
            },
        );
        // 八向缩放手柄（VS 浮动窗四边 + 四角）。
        floating_resize_handles(state, lead, x, y, w, h, colors);
    });
    // ghosting 覆盖层：拖拽/缩放期间整窗半透明（VS 移动工具窗的反馈）。
    // 常驻节点 + 绘制闭包读 Cell，开关只失效本节点，避免拖拽结束还要重组整窗。
    let ghost_id = add_node(
        Policy::Box(Alignment::TopStart),
        Modifier::new().offset(x, y).size(w, h).no_hit(),
    );
    FLOAT_GHOST_NODE.with(|c| c.set(Some(ghost_id)));
    with_rt_pub(|rt| {
        rt.nodes[ghost_id.index()].draw = Some(Box::new(
            move |b: &mut DrawBuilder, gx, gy, gw, gh| {
                if FLOAT_GHOST.with(|c| c.get()) {
                    b.fill_rrect(gx, gy, gw, gh, 6.0, colors.Surface.with_alpha(0.5));
                }
            },
        ));
    });
}

/// 边/角缩放热区： invisible 命中条，悬停染主色提示可抓握。
fn floating_resize_handles(
    state: DockState,
    lead: PanelId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    colors: ColorScheme,
) {
    const L: u8 = 1;
    const R: u8 = 2;
    const T: u8 = 4;
    const B: u8 = 8;
    let specs: [(u8, Modifier, bool); 8] = [
        (L, Modifier::new().align(Alignment::CenterStart).width(6.0).fill_max_height(), true),
        (R, Modifier::new().align(Alignment::CenterEnd).width(6.0).fill_max_height(), true),
        (T, Modifier::new().align(Alignment::TopCenter).height(6.0).fill_max_width(), false),
        (B, Modifier::new().align(Alignment::BottomCenter).height(6.0).fill_max_width(), false),
        (L | T, Modifier::new().align(Alignment::TopStart).size(12.0, 12.0), true),
        (R | T, Modifier::new().align(Alignment::TopEnd).size(12.0, 12.0), true),
        (L | B, Modifier::new().align(Alignment::BottomStart).size(12.0, 12.0), true),
        (R | B, Modifier::new().align(Alignment::BottomEnd).size(12.0, 12.0), true),
    ];
    for (mask, m, vertical) in specs {
        let hid = add_node(Policy::Leaf, m);
        with_rt_pub(|rt| {
            let n = &mut rt.nodes[hid.index()];
            n.on_click = Some(Box::new(|| {}));
            n.on_press = Some(Box::new(move || {
                begin_floating_resize(state, lead, x, y, w, h, mask)
            }));
            n.hover_handler = Some(Box::new(move |enter| {
                HOVER_FLOAT_EDGE.with(|c| c.set(if enter { hid_raw(hid) } else { 0 }));
                set_cursor(if enter { edge_cursor(mask) } else { Cursor::Default });
                invalidate_draw_node(hid);
            }));
            n.draw = Some(Box::new(
                move |b: &mut DrawBuilder, x, y, w: f32, h: f32| {
                    if HOVER_FLOAT_EDGE.with(|c| c.get()) == hid_raw(hid) {
                        if vertical {
                            b.fill_rect(x + w / 2.0 - 1.0, y, 2.0, h, dock_accent(colors).with_alpha(0.7));
                        } else {
                            b.fill_rect(x, y + h / 2.0 - 1.0, w, 2.0, dock_accent(colors).with_alpha(0.7));
                        }
                    }
                },
            ));
        });
    }
}

/// NodeId → 稳定散列（hover Cell 用；arena 下标每趟组合会变）。
fn hid_raw(id: NodeId) -> u64 {
    id.index() as u64 + 1
}

/// 缩放热区 mask（L=1/R=2/T=4/B=8）→ 对应 OS 缩放光标。
fn edge_cursor(mask: u8) -> Cursor {
    match mask {
        1 | 2 => Cursor::ResizeEw,
        4 | 8 => Cursor::ResizeNs,
        5 | 10 => Cursor::ResizeNwse,
        6 | 9 => Cursor::ResizeNesw,
        _ => Cursor::Default,
    }
}

/// 开关浮动窗 ghosting：非响应式 Cell + 失效覆盖层节点（拖拽结束无需重组整窗）。
fn set_float_ghost(on: bool) {
    FLOAT_GHOST.with(|c| c.set(on));
    if let Some(id) = FLOAT_GHOST_NODE.with(|c| c.get()) {
        invalidate_draw_node(id);
    }
}

/// Esc：取消进行中的拖拽（分割条 / 标签 / 浮动窗，恢复拖拽前状态），并关闭上下文菜单 /
/// 标签下拉 / 滑出预览。由核心 `input_router` 在宿主收到 Escape 时转发。
pub fn InputEscape() {
    drag_2d_cancel_global();
    if let Some(st) = ACTIVE_DOCK.with(|c| c.get()) {
        if st.menu.get().panel.is_some() {
            st.menu.set(DockContextMenu::idle());
        }
        if st.tab_dropdown.get().is_some() {
            st.tab_dropdown.set(None);
        }
        if st.read(|m| m.expanded.is_some()) {
            st.collapse_to_edge();
        }
    }
}

fn render_floating_tabs(
    state: DockState,
    items: &[PanelId],
    active: PanelId,
    colors: ColorScheme,
) {
    let items = items.to_vec();
    let g = glass_spec();
    let bar = add_node(
        Policy::Row {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Center,
        },
        Modifier::new()
            .fill_max_width()
            .height(th_header_h())
            .background(dock_inner_color(colors.surface_variant, g), 0.0),
    );
    TAB_BARS.with(|t| {
        t.borrow_mut().push(TabBarRect {
            ids: items.clone(),
            active,
            node: bar,
        })
    });
    with_content_scope(bar, move || {
        for id in items.iter().copied() {
            render_tab(state, id, id == active, false, colors);
        }
        Spacer(Modifier::new().weight(1.0, true));
        dock_action_button(IconName::ChevronDown, colors, move || {
            let (x, y) = last_pointer_global();
            state.open_tab_dropdown(active, x, y);
        });
    });
}

/// 浮动窗口移动：限制在主窗口内；指针悬停主窗口指南时给出投放反馈，抬起即回靠。
fn begin_floating_move(
    state: DockState,
    lead: PanelId,
    window: WindowId,
    x0: f32,
    y0: f32,
    w0: f32,
    h0: f32,
) {
    let (sx, sy) = last_pointer_global();
    let root = dock_layout_rect();
    set_cursor(Cursor::Move);
    set_float_ghost(true);
    let on_move = move |x: f32, y: f32| {
        let (dx, dy) = (x - sx, y - sy);
        let nx = (x0 + dx).clamp(root.0, (root.0 + root.2 - w0).max(root.0));
        let ny = (y0 + dy).clamp(root.1, (root.1 + root.3 - h0).max(root.1));
        state.set_floating_rect(lead, nx, ny, w0, h0);
        let mut d = state.drag.get();
        d.source = Some(lead);
        d.moved = true;
        d.x = x;
        d.y = y;
        apply_drop_feedback(&mut d, dock_drop_target(x, y, lead), lead);
        state.drag.set(d);
    };
    let on_end = move |_x, _y| {
        set_float_ghost(false);
        set_cursor(Cursor::Default);
        let d = state.drag.get();
        state.drag.set(DockDragUi::idle());
        let Some(g) = d.guide else { return };
        let (target, dir, ratio) = match g {
            DockGuide::Inner { target, dir } => (Some(target), dir, 0.5),
            DockGuide::Outer { dir } => (None, dir, 0.25),
            DockGuide::Tab { lead, .. } => (Some(lead), DockDirection::Center, 0.5),
        };
        state.redock_floating(window, target, dir, ratio);
    };
    let on_cancel = move || {
        set_float_ghost(false);
        set_cursor(Cursor::Default);
        state.drag.set(DockDragUi::idle());
        state.set_floating_rect(lead, x0, y0, w0, h0);
    };
    begin_drag_2d_full(on_move, on_end, on_cancel);
}

/// 八向缩放：mask 位 L/R/T/B；保持最小尺寸并限制在主窗口内。
fn begin_floating_resize(
    state: DockState,
    lead: PanelId,
    x0: f32,
    y0: f32,
    w0: f32,
    h0: f32,
    mask: u8,
) {
    let (sx, sy) = last_pointer_global();
    let root = dock_layout_rect();
    set_cursor(edge_cursor(mask));
    set_float_ghost(true);
    let on_move = move |x: f32, y: f32| {
        let (dx, dy) = (x - sx, y - sy);
        let (mut nx, mut ny, mut nw, mut nh) = (x0, y0, w0, h0);
        if mask & 1 != 0 {
            nw = w0 - dx;
            nx = x0 + dx;
        }
        if mask & 2 != 0 {
            nw = w0 + dx;
        }
        if mask & 4 != 0 {
            nh = h0 - dy;
            ny = y0 + dy;
        }
        if mask & 8 != 0 {
            nh = h0 + dy;
        }
        if nw < DOCK_FLOATING_MIN_W {
            if mask & 1 != 0 {
                nx = x0 + w0 - DOCK_FLOATING_MIN_W;
            }
            nw = DOCK_FLOATING_MIN_W;
        }
        if nh < DOCK_FLOATING_MIN_H {
            if mask & 4 != 0 {
                ny = y0 + h0 - DOCK_FLOATING_MIN_H;
            }
            nh = DOCK_FLOATING_MIN_H;
        }
        nx = nx.clamp(root.0, (root.0 + root.2 - nw).max(root.0));
        ny = ny.clamp(root.1, (root.1 + root.3 - nh).max(root.1));
        nw = nw.min(root.0 + root.2 - nx);
        nh = nh.min(root.1 + root.3 - ny);
        state.set_floating_rect(lead, nx, ny, nw, nh);
    };
    let on_end = move |_x, _y| {
        set_float_ghost(false);
        set_cursor(Cursor::Default);
    };
    let on_cancel = move || {
        set_float_ghost(false);
        set_cursor(Cursor::Default);
        state.set_floating_rect(lead, x0, y0, w0, h0);
    };
    begin_drag_2d_full(on_move, on_end, on_cancel);
}
