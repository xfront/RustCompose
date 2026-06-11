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

use compose::animation::clock_nanos;
use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Box, Canvas, Clickable, Column, CrossAxisAlignment, DrawBuilder,
    GlassSpec, Modifier, NodeId, Policy, Row, Spacer, add_node, begin_drag_2d_with_end, hoverable,
    invalidate_draw_node, last_pointer_global, node_abs_rect, with_content_scope,
};
use compose::material::{ColorScheme, IconName, glass_spec, icon, theme_colors};
use compose::runtime::{MutableState, mutableStateOf, remember, with_rt_pub};

use crate::tokens;
use crate::widgets::{GlassOrBg, Text};

use super::dock_layout::{DockLayout, WINDOW_MAIN, WindowBounds, WindowId};
pub use super::dock_model::{
    AnchorId, DockDirection, DockNodeId, DockRegion, DockTarget, DockableOptions, DockingStyle,
    NodeIdGenerator, TabPreference, region_is_allowed,
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

/// 面板 ID（全局注册时分配，稳定且可序列化）。
pub type PanelId = u64;
/// 矩形 `(x, y, w, h)`，一律为窗口绝对像素坐标。
pub type DockRect = (f32, f32, f32, f32);

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

/// 解析面板的标签条位置：`Default` 跟随全局，其余用自身偏好。
fn resolved_tab_preference(id: PanelId) -> TabPreference {
    let own = dock_panel_info(id)
        .map(|i| i.options.tab_preference)
        .unwrap_or(TabPreference::Default);
    if own == TabPreference::Default {
        default_tab_preference()
    } else {
        own
    }
}

/// 该面板组的标签条是否应渲染在内容**下方**（VS2022 底部工具窗口语义）。
fn tab_bar_on_bottom(active: PanelId) -> bool {
    resolved_tab_preference(active).is_bottom()
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
    if path.is_empty() {
        return;
    }
    if let DockNode::Split {
        proportion: r,
        first,
        second,
        ..
    } = node
    {
        if path.len() == 1 {
            *r = ratio.clamp(DOCK_RATIO_MIN, DOCK_RATIO_MAX);
            return;
        }
        let child = if path[0] == 0 { first } else { second };
        set_ratio_by_path(child, &path[1..], ratio);
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

/// 记录面板在树中的位置（兄弟 + 方向 + 比例），供浮动/自动隐藏后精确恢复。
fn find_tree_position(node: &DockNode, id: PanelId) -> Option<SavedTreePosition> {
    match node {
        DockNode::Tabs { .. } | DockNode::Leaf { .. } | DockNode::Anchor { .. } => None,
        DockNode::Split {
            orientation,
            proportion,
            first,
            second,
            ..
        } => {
            if first.collect_ids().contains(&id) {
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
                return find_tree_position(first, id);
            }
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
            find_tree_position(second, id)
        }
    }
}

/// 几何单一真源：给定矩形与分割节点，算出两个子矩形（渲染/预览/命中一致）。
pub fn dock_split_bounds(
    orientation: SplitOrientation,
    ratio: f32,
    rect: DockRect,
) -> (DockRect, DockRect) {
    let (x, y, w, h) = rect;
    match orientation {
        SplitOrientation::Horizontal => {
            let fw = (w * ratio - DOCK_SPLITTER_PX / 2.0).max(0.0);
            let sw = (w * (1.0 - ratio) - DOCK_SPLITTER_PX / 2.0).max(0.0);
            ((x, y, fw, h), (x + fw + DOCK_SPLITTER_PX, y, sw, h))
        }
        SplitOrientation::Vertical => {
            let fh = (h * ratio - DOCK_SPLITTER_PX / 2.0).max(0.0);
            let sh = (h * (1.0 - ratio) - DOCK_SPLITTER_PX / 2.0).max(0.0);
            ((x, y, w, fh), (x, y + fh + DOCK_SPLITTER_PX, w, sh))
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

// ══════════════════════════════ 状态模型 ══════════════════════════════

/// 恢复位置：兄弟面板 + 相对方向 + 当时的分割比例。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SavedTreePosition {
    pub sibling: PanelId,
    pub direction: DockDirection,
    pub ratio: f32,
}

/// 自动隐藏到某边缘的整组面板。
#[derive(Clone, PartialEq, Debug)]
pub struct HiddenPanel {
    pub edge: DockEdge,
    pub items: Vec<PanelId>,
    pub active: PanelId,
    pub saved: Option<SavedTreePosition>,
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
    /// DockLayout 根节点（窗口绝对矩形）。
    static ROOT_NODE: Cell<Option<NodeId>> = const { Cell::new(None) };
    /// 当前 hover 的分割条路径散列（0 = 无）。
    static HOVER_SPLIT: Cell<u64> = const { Cell::new(0) };
    /// 双击检测：上次按压的面板与时间（ns）。
    static LAST_HEADER_PRESS: Cell<(PanelId, f64)> = const { Cell::new((0, -1.0)) };
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
    pub root_direction: Option<DockDirection>,
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
            root_direction: None,
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
}

/// 在组合外（测试 / 初始化）创建状态句柄。
pub fn dock_state(initial: DockModel) -> DockState {
    DockState {
        model: mutableStateOf(initial),
        drag: mutableStateOf(DockDragUi::idle()),
        menu: mutableStateOf(DockContextMenu::idle()),
        tab_dropdown: mutableStateOf(None),
    }
}

/// 组合期创建并记住状态句柄（跨重组保持）。
pub fn remember_dock_state(initial: DockModel) -> DockState {
    remember(move || dock_state(initial))
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
            let saved = find_tree_position(root, id).map(|mut p| {
                p.direction = edge.direction();
                p.ratio = 0.2;
                p
            });
            let items = group.collect_ids();
            let active = group.active_panel().unwrap_or(id);
            for gid in &items {
                Self::detach(m, *gid);
            }
            m.hidden.push(HiddenPanel {
                edge,
                items,
                active,
                saved,
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
            if m.expanded == Some(id) {
                m.expanded = None;
            }
            let group = tabs_group(&h.items, h.active);
            let root = m.root.take();
            m.root = Some(match (root, h.saved) {
                (Some(r), Some(s)) if r.collect_ids().contains(&s.sibling) => {
                    insert_relative(r, s.sibling, group, s.direction, s.ratio)
                }
                (Some(r), _) => match r.first_id() {
                    Some(t) => insert_relative(r, t, group, DockDirection::Center, 0.3),
                    None => group,
                },
                (None, _) => group,
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
        let edge = nearest_edge_for(id);
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
    pub fn float_panel(&self, id: PanelId) {
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
        let m = self.model();
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
        let mut lines = text.trim().lines();
        let version = match lines.next() {
            Some("mdl2") => 2,
            Some("mdl1") => 1,
            _ => return false,
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
                "tree" => tree = parse_dock_layout(v).and_then(|n| prune_node(n, &dock_panel_exists)),
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
        self.mutate(|m| {
            m.root = tree;
            m.maximized = max;
            m.saved_before_maximize = saved_max;
            m.hidden = hidden;
            m.floating = floating;
            m.expanded = None;
        });
        true
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
fn nearest_edge_for(id: PanelId) -> DockEdge {
    let root = dock_layout_rect();
    let groups: Vec<GroupRect> = GROUP_RECTS.with(|g| g.borrow().clone());
    let center = groups
        .iter()
        .find(|g| g.ids.contains(&id))
        .map(|g| {
            let (x, y, w, h) = node_abs_rect(g.node);
            (x + w / 2.0, y + h / 2.0)
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
    target: Option<PanelId>,
    direction: DockDirection,
    rect: DockRect,
    root_direction: Option<DockDirection>,
    outside: bool,
}

/// 释放点归属：优先命中面板 body（排除源所在组），否则根级 5 区，再否则布局外。
fn dock_drop_target(gx: f32, gy: f32, source: PanelId) -> DockDrop {
    let groups: Vec<GroupRect> = GROUP_RECTS.with(|g| g.borrow().clone());
    for g in groups.iter().rev() {
        if g.ids.contains(&source) {
            continue;
        }
        let rect = node_abs_rect(g.node);
        if point_in(rect, gx, gy) {
            return DockDrop {
                target: Some(g.active),
                direction: dock_direction_in_rect(rect, gx, gy),
                rect,
                root_direction: None,
                outside: false,
            };
        }
    }
    let root = dock_layout_rect();
    if root.2 <= 0.0 || !point_in(root, gx, gy) {
        return DockDrop {
            target: None,
            direction: DockDirection::Center,
            rect: root,
            root_direction: None,
            outside: true,
        };
    }
    let dir = dock_direction_in_rect(root, gx, gy);
    DockDrop {
        target: None,
        direction: dir,
        rect: root,
        root_direction: Some(dir),
        outside: false,
    }
}

/// 标签栏内横向重排：返回插入索引。
fn tab_reorder_target(gx: f32, gy: f32, source: PanelId) -> Option<usize> {
    let tabs: Vec<TabRect> = TAB_RECTS.with(|t| t.borrow().to_vec());
    if tabs.is_empty() {
        return None;
    }
    let source_tab = tabs.iter().find(|t| t.id == source)?;
    let sr = node_abs_rect(source_tab.node);
    if gy < sr.1 - 4.0 || gy > sr.1 + sr.3 + 4.0 {
        return None;
    }
    let mut before = 0usize;
    for t in &tabs {
        if t.id == source {
            continue;
        }
        let r = node_abs_rect(t.node);
        if gx > r.0 + r.2 / 2.0 {
            before += 1;
        }
    }
    Some(before)
}

fn begin_tab_drag(state: DockState, id: PanelId) {
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
        root_direction: None,
    });
    let on_move = move |x: f32, y: f32| {
        let mut d = state.drag.get();
        if !d.moved
            && (x - d.start_x).abs() + (y - d.start_y).abs() > DOCK_DRAG_THRESHOLD
        {
            d.moved = true;
        }
        d.x = x;
        d.y = y;
        if d.moved {
            let drop = dock_drop_target(x, y, id);
            d.target = drop.target;
            d.direction = Some(drop.direction);
            d.target_rect = if drop.outside { None } else { Some(drop.rect) };
            d.root_direction = drop.root_direction;
        }
        state.drag.set(d);
    };
    let on_end = move |x: f32, y: f32| {
        let d = state.drag.get();
        state.drag.set(DockDragUi::idle());
        if !d.moved {
            return;
        }
        let drop = dock_drop_target(x, y, id);
        // 1) 同组标签栏横向拖拽 → 重排
        if let Some(idx) = tab_reorder_target(x, y, id) {
            let same_group = GROUP_RECTS.with(|g| {
                g.borrow().iter().any(|gr| {
                    gr.ids.contains(&id) && {
                        let r = node_abs_rect(gr.node);
                        point_in(r, x, y)
                    }
                })
            });
            if same_group {
                state.reorder_tab(id, idx);
                return;
            }
        }
        // 2) 拖出布局外 → 浮起（参考实现声明了该优先级但未真正接线）
        if drop.outside {
            if dock_panel_info(id).is_some_and(|p| p.options.floatable) {
                state.float_panel(id);
            }
            return;
        }
        // 3) 目标面板 5 区 / 4) 根级 5 区
        state.dock(id, drop.target, drop.direction, 0.5);
    };
    begin_drag_2d_with_end(on_move, on_end);
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
    let lo = (DOCK_MIN_EXTENT / total).clamp(DOCK_RATIO_MIN, 0.49);
    let hi = 1.0 - lo;
    let on_move = move |x: f32, y: f32| {
        let cur = if horizontal { x } else { y };
        let nr = ((cur - origin) / total).clamp(lo, hi);
        state.set_split_ratio(&path, nr);
    };
    begin_drag_2d_with_end(on_move, |_x, _y| {});
}

// ══════════════════════════════ 渲染 ══════════════════════════════

/// 停靠布局主入口。
///
/// 面板由 [`register_dock_panel`] 注册；`modifier` 一般应带 `fill_max_size()`。
/// 所有交互（分割条、标签头、边条、浮动窗口）都在本控件的组合树里完成。
#[allow(non_snake_case)] // composable 命名与全库控件一致
pub fn DockLayout(state: DockState, modifier: Modifier) {
    // 注：本函数（值命名空间）与 [`DockLayout`](super::dock_layout::DockLayout) 结构体
    // （类型命名空间）同名但语义不同——前者是停靠 UI 的渲染入口，后者是对齐
    // compose-docking `DockLayout.kt` 的**持久化窗口列表模型**。
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

    let root_id = add_node(Policy::Box(Alignment::TopStart), modifier);
    ROOT_NODE.with(|r| r.set(Some(root_id)));

    let tree = m.root.clone();
    let maximized = m.maximized;
    let hidden = m.hidden.clone();
    let floating = m.floating.clone();
    let expanded = m.expanded;

    with_content_scope(root_id, move || {
        if let Some(mid) = maximized {
            render_panel_group(
                state,
                &DockNode::tabs(&[mid]),
                Modifier::new().fill_max_size(),
                true,
            );
        } else if let Some(root) = &tree {
            render_node(state, root, &[], Modifier::new().fill_max_size());
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
        DockNode::Tabs { .. } => render_panel_group(state, node, modifier, false),
        DockNode::Leaf { .. } => render_panel_group(state, node, modifier, false),
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
        Modifier::new().width(DOCK_SPLITTER_PX).fill_max_height()
    } else {
        Modifier::new().height(DOCK_SPLITTER_PX).fill_max_width()
    };
    let rid = add_node(Policy::Leaf, m);
    let p = path.to_vec();
    with_rt_pub(|rt| {
        let n = &mut rt.nodes[rid.index()];
        // `dispatch_press_down` 只认「最近的 on_click 祖先」，故 on_press 必须成对挂 on_click。
        n.on_click = Some(Box::new(|| {}));
        n.on_press = Some(Box::new(move || {
            begin_splitter_drag(rid, p.clone(), horizontal, state)
        }));
        n.hover_handler = Some(Box::new(move |enter| {
            HOVER_SPLIT.with(|c| c.set(if enter { key } else { 0 }));
            invalidate_draw_node(rid);
        }));
        n.draw = Some(Box::new(
            move |b: &mut DrawBuilder, x, y, w: f32, h: f32| {
                let c = if HOVER_SPLIT.with(|c| c.get()) == key {
                    colors.primary.with_alpha(0.6)
                } else {
                    colors.outline_variant
                };
                b.fill_rect(x, y, w, h, c);
            },
        ));
    });
}

fn render_panel_group(
    state: DockState,
    node: &DockNode,
    modifier: Modifier,
    maximized: bool,
) {
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
    with_content_scope(id, move || {
        let bottom = tab_bar_on_bottom(active);
        if bottom {
            // VS2022 底部工具窗口：标签条在内容下方（输出 / 错误列表）。
            Box(
                Modifier::new().weight(1.0, true).fill_max_width(),
                Alignment::TopStart,
                move || call_dock_panel(active),
            );
            render_tab_bar(state, &items, active, maximized, colors);
        } else {
            render_tab_bar(state, &items, active, maximized, colors);
            Box(
                Modifier::new().weight(1.0, true).fill_max_width(),
                Alignment::TopStart,
                move || call_dock_panel(active),
            );
        }
    });
}

/// 渲染命名锚点占位（KT `DockNode.Anchor`）：空槽位提示，仍是合法投放目标。
fn render_anchor(state: DockState, anchor_id: AnchorId, modifier: Modifier) {
    let colors = theme_colors();
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
    });
}

fn render_tab_bar(
    state: DockState,
    items: &[PanelId],
    active: PanelId,
    maximized: bool,
    colors: ColorScheme,
) {
    let items = items.to_vec();
    let g = glass_spec();
    Row(
        Modifier::new()
            .fill_max_width()
            .height(DOCK_HEADER_H)
            .background(dock_inner_color(colors.surface_variant, g), 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for id in items.iter().copied() {
                render_tab(state, id, id == active, colors);
            }
            Spacer(Modifier::new().weight(1.0, true));
            let lead = active;
            // VS2022 标签条右侧「▼」：下拉列出该组全部面板（与 Tabs 控件的「更多」同语义）。
            // 四个方向的面板组都走 render_tab_bar，因此自动覆盖全部停靠区域。
            dock_action_button(IconName::ChevronDown, colors, move || {
                let (x, y) = last_pointer_global();
                state.open_tab_dropdown(lead, x, y);
            });
            let info = dock_panel_info(lead).unwrap_or_default();
            if info.min_max_allowed {
                dock_action_button(
                    if maximized {
                        IconName::ZoomOut
                    } else {
                        IconName::ZoomIn
                    },
                    colors,
                    move || state.toggle_maximize(lead),
                );
            }
            if info.auto_hide_allowed {
                dock_action_button(IconName::Bookmark, colors, move || {
                    state.toggle_auto_hide(lead)
                });
            }
            if info.options.floatable {
                dock_action_button(IconName::Layers, colors, move || {
                    state.toggle_float(lead)
                });
            }
            if info.options.closable {
                dock_action_button(IconName::Close, colors, move || state.close(lead));
            }
        },
    );
}

fn render_tab(state: DockState, id: PanelId, active: bool, colors: ColorScheme) {
    let title = panel_title(id);
    let info = dock_panel_info(id).unwrap_or_default();
    let g = glass_spec();
    let cid = add_node(
        Policy::Row {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Center,
        },
        Modifier::new()
            .height(DOCK_HEADER_H)
            .padding_hv(10.0, 0.0)
            .background(
                dock_inner_color(
                    if active {
                        colors.Surface
                    } else {
                        colors.surface_variant
                    },
                    g,
                ),
                0.0,
            ),
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
        n.on_press = Some(Box::new(move || begin_tab_drag(state, id)));
        n.on_secondary_click = Some(Box::new(move || {
            let (x, y) = last_pointer_global();
            state.menu.set(DockContextMenu {
                panel: Some(id),
                x,
                y,
            });
        }));
        if active {
            n.draw = Some(Box::new(
                move |b: &mut DrawBuilder, x, y, w: f32, h: f32| {
                    b.fill_rect(x, y + h - 2.0, w, 2.0, colors.primary);
                },
            ));
        }
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
        if info.options.closable {
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
    let dir = d.direction;
    let rect = d.target_rect;
    Canvas(
        Modifier::new().fill_max_size().no_hit(),
        move |b: &mut DrawBuilder, _x, _y, _w, _h| {
            let primary = colors.primary;
            if let (Some(r), Some(dir)) = (rect, dir) {
                let pv = dock_preview_rect(r, dir);
                b.fill_rrect(
                    pv.0,
                    pv.1,
                    pv.2,
                    pv.3,
                    4.0,
                    primary.with_alpha(0.12),
                );
                b.stroke_rrect(pv.0, pv.1, pv.2, pv.3, 4.0, primary.with_alpha(0.5), 1.0);
                draw_compass(b, r, dir, colors);
            }
            // 跟随光标的幽灵标签头。
            b.fill_rrect(
                d.x,
                d.y,
                160.0,
                DOCK_HEADER_H,
                4.0,
                primary.with_alpha(0.85),
            );
            b.draw_text(
                d.x + 10.0,
                d.y + 7.0,
                &title,
                &tokens::label_medium(colors.on_primary),
                140.0,
            );
        },
    );
}

fn draw_compass(b: &mut DrawBuilder, r: DockRect, dir: DockDirection, colors: ColorScheme) {
    let (x, y, w, h) = r;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let s = 26.0;
    let gap = 4.0;
    let spots = [
        (DockDirection::North, cx - s / 2.0, cy - s * 1.5 - gap),
        (DockDirection::South, cx - s / 2.0, cy + s * 0.5 + gap),
        (DockDirection::West, cx - s * 1.5 - gap, cy - s / 2.0),
        (DockDirection::East, cx + s * 0.5 + gap, cy - s / 2.0),
        (DockDirection::Center, cx - s / 2.0, cy - s / 2.0),
    ];
    for (dd, bx, by) in spots {
        let fill = if dd == dir {
            colors.primary.with_alpha(0.92)
        } else {
            colors.Surface.with_alpha(0.92)
        };
        b.fill_rrect(bx, by, s, s, 4.0, fill);
        b.stroke_rrect(bx, by, s, s, 4.0, colors.outline.with_alpha(0.5), 1.0);
        let fg = if dd == dir {
            colors.on_primary
        } else {
            colors.on_surface_variant
        };
        let (ax, ay) = (bx + s / 2.0, by + s / 2.0);
        let a = s * 0.28;
        match dd {
            DockDirection::North => {
                b.line(ax, ay + a, ax, ay - a, fg, 2.0);
                b.line(ax - a * 0.6, ay - a * 0.4, ax, ay - a, fg, 2.0);
                b.line(ax + a * 0.6, ay - a * 0.4, ax, ay - a, fg, 2.0);
            }
            DockDirection::South => {
                b.line(ax, ay - a, ax, ay + a, fg, 2.0);
                b.line(ax - a * 0.6, ay + a * 0.4, ax, ay + a, fg, 2.0);
                b.line(ax + a * 0.6, ay + a * 0.4, ax, ay + a, fg, 2.0);
            }
            DockDirection::West => {
                b.line(ax + a, ay, ax - a, ay, fg, 2.0);
                b.line(ax - a * 0.4, ay - a * 0.6, ax - a, ay, fg, 2.0);
                b.line(ax - a * 0.4, ay + a * 0.6, ax - a, ay, fg, 2.0);
            }
            DockDirection::East => {
                b.line(ax - a, ay, ax + a, ay, fg, 2.0);
                b.line(ax + a * 0.4, ay - a * 0.6, ax + a, ay, fg, 2.0);
                b.line(ax + a * 0.4, ay + a * 0.6, ax + a, ay, fg, 2.0);
            }
            DockDirection::Center => {
                b.stroke_rrect(bx + 6.0, by + 6.0, s - 12.0, s - 12.0, 2.0, fg, 2.0);
            }
        }
    }
}

// ── 右键菜单 ──

const MENU_CLOSE: u8 = 0;
const MENU_MAXIMIZE: u8 = 1;
const MENU_AUTO_HIDE: u8 = 2;
const MENU_FLOAT: u8 = 3;

fn menu_actions(state: DockState, id: PanelId) -> Vec<(&'static str, u8)> {
    let info = dock_panel_info(id).unwrap_or_default();
    let mut v = Vec::new();
    if info.options.closable {
        v.push(("关闭", MENU_CLOSE));
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
                        .height(30.0)
                        .padding_hv(10.0, 0.0)
                        .background(
                            dock_inner_color(
                                if on {
                                    colors.primary_container
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
    let height = actions.len() as f32 * 32.0 + 8.0;
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
            Modifier::new().pin_at_global(x, y).size(180.0, height),
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
                        .width(DOCK_STRIP_THICKNESS)
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
                        .height(DOCK_STRIP_THICKNESS)
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
    let m = if edge.is_side() {
        Modifier::new().fill_max_width().height(120.0)
    } else {
        Modifier::new().width(140.0).fill_max_height()
    };
    hoverable(
        m,
        move |enter| {
            if enter {
                state.expand_from_edge(id);
            }
        },
        move || {
            let label = title.clone();
            Clickable(
                Modifier::new().fill_max_size().no_indication(),
                move || state.restore_from_edge(id),
                move || {
                    Text(
                        label.clone(),
                        tokens::label_small(colors.on_surface_variant),
                        Modifier::new().padding(4.0),
                    );
                },
            );
        },
    );
}

fn render_peek(state: DockState, id: PanelId, colors: ColorScheme) {
    let edge = state
        .read(|m| {
            m.hidden
                .iter()
                .find(|h| h.items.contains(&id))
                .map(|h| h.edge)
        })
        .unwrap_or(DockEdge::Left);
    let title = panel_title(id);
    // 点击外部收起。
    Clickable(
        Modifier::new().fill_max_size().no_indication(),
        move || state.collapse_to_edge(),
        || {},
    );
    Column(
        dock_surface(
            Modifier::new()
                .align(edge.align())
                .fill_max_width_frac(0.32)
                .fill_max_height_frac(0.32),
            glass_spec(),
            colors.surface_container_high,
            tokens::SHAPE_SMALL,
            1.0,
            colors.outline,
        ),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            let label = title.clone();
            Row(
                Modifier::new().fill_max_width().height(DOCK_HEADER_H),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Text(
                        label.clone(),
                        tokens::label_medium(colors.on_surface),
                        Modifier::new().padding_hv(10.0, 0.0),
                    );
                    Spacer(Modifier::new().weight(1.0, true));
                    dock_action_button(IconName::Bookmark, colors, move || {
                        state.restore_from_edge(id)
                    });
                },
            );
            Box(
                Modifier::new().weight(1.0, true).fill_max_width(),
                Alignment::TopStart,
                move || call_dock_panel(id),
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
    Column(
        dock_surface(
            Modifier::new().offset(x, y).size(w, h),
            glass,
            colors.surface_container_high,
            6.0,
            0.5,
            colors.outline,
        ),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            // 标题栏：拖动移动窗口。
            let label = title.clone();
            let bar = add_node(
                Policy::Row {
                    arrangement: Arrangement::Start,
                    cross: CrossAxisAlignment::Center,
                },
                Modifier::new()
                    .fill_max_width()
                    .height(DOCK_HEADER_H)
                    .background(dock_inner_color(colors.primary_container, glass), 0.0),
            );
            with_rt_pub(|rt| {
                let n = &mut rt.nodes[bar.index()];
                n.on_click = Some(Box::new(|| {}));
                n.on_press = Some(Box::new(move || {
                    begin_floating_drag(state, lead, x, y, w, h, false)
                }));
            });
            with_content_scope(bar, move || {
                Text(
                    label.clone(),
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
            let pref = resolved_tab_preference(active);
            let show_tabs = items.len() > 1 || pref.always_display_tabs();
            let content = |state: DockState, active: PanelId| {
                Box(
                    Modifier::new().weight(1.0, true).fill_max_width(),
                    Alignment::TopStart,
                    move || call_dock_panel(active),
                );
            };
            if pref.is_bottom() {
                content(state, active);
                if show_tabs {
                    render_floating_tabs(state, &items, active, colors);
                }
            } else {
                if show_tabs {
                    render_floating_tabs(state, &items, active, colors);
                }
                content(state, active);
            }
            // 右下角缩放把手。
            let grip = add_node(
                Policy::Leaf,
                Modifier::new()
                    .size(16.0, 16.0)
                    .align(Alignment::BottomEnd)
                    .background(dock_inner_color(colors.outline_variant, glass), 2.0),
            );
            with_rt_pub(|rt| {
                let n = &mut rt.nodes[grip.index()];
                n.on_click = Some(Box::new(|| {}));
                n.on_press = Some(Box::new(move || {
                    begin_floating_drag(state, lead, x, y, w, h, true)
                }));
            });
        },
    );
}

fn render_floating_tabs(
    state: DockState,
    items: &[PanelId],
    active: PanelId,
    colors: ColorScheme,
) {
    let items = items.to_vec();
    let g = glass_spec();
    Row(
        Modifier::new()
            .fill_max_width()
            .height(DOCK_HEADER_H)
            .background(dock_inner_color(colors.surface_variant, g), 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for id in items.iter().copied() {
                let label = panel_title(id);
                Clickable(
                    Modifier::new()
                        .height(DOCK_HEADER_H)
                        .padding_hv(10.0, 0.0)
                        .background(
                            dock_inner_color(
                                if id == active {
                                    colors.Surface
                                } else {
                                    colors.surface_variant
                                },
                                g,
                            ),
                            0.0,
                        )
                        .no_indication(),
                    move || state.switch_tab(id),
                    move || {
                        Text(
                            label.clone(),
                            tokens::label_medium(colors.on_surface),
                            Modifier::new(),
                        );
                    },
                );
            }
            Spacer(Modifier::new().weight(1.0, true));
            dock_action_button(IconName::ChevronDown, colors, move || {
                let (x, y) = last_pointer_global();
                state.open_tab_dropdown(active, x, y);
            });
        },
    );
}

fn begin_floating_drag(
    state: DockState,
    lead: PanelId,
    x0: f32,
    y0: f32,
    w0: f32,
    h0: f32,
    resize: bool,
) {
    let (sx, sy) = last_pointer_global();
    let on_move = move |x: f32, y: f32| {
        let (dx, dy) = (x - sx, y - sy);
        if resize {
            state.set_floating_rect(
                lead,
                x0,
                y0,
                w0 + dx,
                h0 + dy,
            );
        } else {
            state.set_floating_rect(lead, x0 + dx, y0 + dy, w0, h0);
        }
    };
    begin_drag_2d_with_end(on_move, |_x, _y| {});
}
