//! 停靠契约层 —— 对齐 compose-docking 的 `model/Enums.kt`、`model/Ids.kt`、
//! `model/DockableOptions.kt`、`state/DockTarget.kt`。
//!
//! 这些是**引擎契约**：区域、停靠风格、标签偏好、节点/锚点/停靠项 id、停靠目标。
//! 具体控件实现（`DockableSpec` 的可视部分）在 `docking.rs`，
//! 持久化布局（窗口列表）在 `dock_layout.rs`。

use crate::widgets::docking::PanelId;

/// 停靠项 id（KT `DockableId`）。
///
/// KT 用 `String` 值类保证**跨进程稳定**；本库用运行时分配的 `u64` 句柄
/// （面板注册表是 thread_local 单组合前提），语义等价于 KT 的"应用内唯一"。
pub type DockableId = PanelId;

/// 停靠锚点 id（KT `AnchorId`）：命名布局区域，成员全部关闭后仍保留槽位。
pub type AnchorId = u32;

/// 布局树节点 id（KT `DockNodeId`）。
///
/// 节点 id 随布局一起持久化，作为稳定 key 使用（分割条拖动、标签行滚动等
/// 每节点 UI 状态在树重组后仍能存活）。
pub type DockNodeId = u32;

/// 节点 id 生成器（KT `NodeIdGenerator`）。
///
/// KT 用**随机会话前缀**避免"本次新生成的 id"与"从存档恢复的 id"撞车；
/// 本库用会话基址 + 计数器达到同样效果（基址由启动时刻派生）。
#[derive(Clone, Debug)]
pub struct NodeIdGenerator {
    base: u32,
    counter: u32,
}

impl NodeIdGenerator {
    /// 以指定基址创建（测试用固定基址）。
    pub fn with_base(base: u32) -> Self {
        NodeIdGenerator { base, counter: 0 }
    }

    /// 以会话基址创建（默认：由启动时刻派生，避开存档中的旧 id）。
    pub fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u32)
            .unwrap_or(0);
        // 会话基址取高 16 位，低位留给计数器，避免与旧存档 id 相邻撞号。
        NodeIdGenerator::with_base((nanos & 0x0000_FFFF) << 16)
    }

    /// 下一个节点 id。
    pub fn next(&mut self) -> DockNodeId {
        self.counter += 1;
        self.base.wrapping_add(self.counter)
    }
}

impl Default for NodeIdGenerator {
    fn default() -> Self {
        NodeIdGenerator::new()
    }
}

/// 停靠区域（KT `DockRegion`）。
///
/// `Center` 表示并入/新建标签组；四个边缘区域表示对目标做分割。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockRegion {
    /// 中心：并入标签组。
    Center,
    /// 北：目标上方分割。
    North,
    /// 南：目标下方分割。
    South,
    /// 东：目标右侧分割。
    East,
    /// 西：目标左侧分割。
    West,
}

/// 旧名别名（区域即方向），保持源码兼容。
pub type DockDirection = DockRegion;

impl DockRegion {
    /// 持久化用的序号（沿用历史映射，保证旧存档可读）。
    pub fn index(self) -> u8 {
        match self {
            DockRegion::North => 0,
            DockRegion::South => 1,
            DockRegion::East => 2,
            DockRegion::West => 3,
            DockRegion::Center => 4,
        }
    }

    /// 由持久化序号反解。
    pub fn from_index(v: u8) -> Option<Self> {
        match v {
            0 => Some(DockRegion::North),
            1 => Some(DockRegion::South),
            2 => Some(DockRegion::East),
            3 => Some(DockRegion::West),
            4 => Some(DockRegion::Center),
            _ => None,
        }
    }

    /// 是否为边缘分割区域（`Center` 不是）。
    pub fn is_edge(self) -> bool {
        self != DockRegion::Center
    }

    /// KT 顺序下的序号（`Center` = 0）。
    pub fn kt_index(self) -> u8 {
        match self {
            DockRegion::Center => 0,
            DockRegion::North => 1,
            DockRegion::South => 2,
            DockRegion::East => 3,
            DockRegion::West => 4,
        }
    }
}

/// 停靠风格（KT `DockingStyle`）：一个停靠项允许参与哪些分割区域。
///
/// 校验是**双向**的：拖放两侧的风格都必须允许该方向。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockingStyle {
    /// 只参与南北分割（及中心）。
    Horizontal,
    /// 只参与东西分割（及中心）。
    Vertical,
    /// 任意区域。
    Both,
    /// 只做标签组，永不分割。
    CenterOnly,
}

impl DockingStyle {
    /// 该风格是否允许落在 `region`。
    pub fn allows_region(self, region: DockRegion) -> bool {
        match self {
            DockingStyle::Both => true,
            DockingStyle::CenterOnly => !region.is_edge(),
            DockingStyle::Horizontal => matches!(
                region,
                DockRegion::Center | DockRegion::North | DockRegion::South
            ),
            DockingStyle::Vertical => matches!(
                region,
                DockRegion::Center | DockRegion::East | DockRegion::West
            ),
        }
    }
}

/// 区域是否被允许（双向校验：目标风格与被拖风格都要允许）。
pub fn region_is_allowed(region: DockRegion, target: DockingStyle, dragged: DockingStyle) -> bool {
    target.allows_region(region) && dragged.allows_region(region)
}

/// 标签条位置偏好（KT `TabPreference`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabPreference {
    /// 跟随全局默认。
    Default,
    /// 顶部。
    Top,
    /// 底部。
    Bottom,
    /// 恒在顶部（并强制单面板也渲染成标签组）。
    TopAlways,
    /// 恒在底部（并强制单面板也渲染成标签组）。
    BottomAlways,
}

impl TabPreference {
    /// 是否为「恒显示」变体（对应 KT 的 `alwaysDisplayTabs`）。
    pub fn always_display_tabs(self) -> bool {
        matches!(self, TabPreference::TopAlways | TabPreference::BottomAlways)
    }

    /// 标签条是否应置于内容**下方**（KT `TabPreference.Bottom*`）。
    ///
    /// `Default` 由调用方解析为全局默认后再判定，故此处返回 `false`。
    pub fn is_bottom(self) -> bool {
        matches!(self, TabPreference::Bottom | TabPreference::BottomAlways)
    }
}

/// 停靠目标（KT `DockTarget`）：停靠到哪里去。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockTarget {
    /// 相对另一个停靠项停靠。
    OnDockable(DockableId),
    /// 相对某个具体树节点停靠（拖放解析用）。
    OnNode { window_id: u32, node_id: DockNodeId },
    /// 相对某个窗口的整棵根树停靠。
    Root { window_id: u32 },
    /// 停靠进命名锚点区域：替换占位符，或并入共享该锚点的停靠项。
    Anchor(AnchorId),
}

impl DockTarget {
    /// KT 的 `DockTarget.Root` 默认指向主窗口。
    pub fn root() -> Self {
        DockTarget::Root { window_id: 0 }
    }
}

/// 停靠项的静态停靠行为（KT `DockableOptions`）。
///
/// 只约束"引擎允许什么"；标题、图标、内容等可视部分在 `DockablePanel`。
#[derive(Clone, PartialEq, Debug)]
pub struct DockableOptions {
    /// 应用自定义类别（用于按类型查找/停靠）。
    pub r#type: Option<String>,
    /// 是否可关闭（是否显示关闭入口）。
    pub closable: bool,
    /// 是否可撕下为浮动窗口。
    pub floatable: bool,
    /// 是否被限制在当前窗口内（不可跨窗口移动）。
    pub limited_to_window: bool,
    /// 参与哪些分割区域（双向校验）。
    pub docking_style: DockingStyle,
    /// 是否提供最大化。
    pub maximizable: bool,
    /// 标签条位置偏好。
    pub tab_preference: TabPreference,
    /// 所属锚点：最后一个成员离开窗口时，锚点占位符被还原。
    pub anchor: Option<AnchorId>,
}

impl Default for DockableOptions {
    fn default() -> Self {
        DockableOptions {
            r#type: None,
            closable: true,
            floatable: true,
            limited_to_window: false,
            docking_style: DockingStyle::Both,
            maximizable: true,
            tab_preference: TabPreference::Default,
            anchor: None,
        }
    }
}
