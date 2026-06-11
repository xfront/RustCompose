//! 跨分组共用的小组件与全局状态（各分组页面的公共依赖）。

use crate::pages::{GROUPS, PAGES};
use crate::prelude::*;

thread_local! {
    /// 顶部菜单栏各标签的锚点节点（按 `MENUS` 下标）：`menu_bar` 每次组合刷新，
    /// `dropdown_overlay` 据此把库的 `menu_panel` 钉在当前打开菜单标签正下方。
    pub(crate) static MENU_ANCHORS: std::cell::RefCell<Vec<compose::layout::NodeId>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// 当前 Snackbar 宿主句柄（`story_shell` 创建后写入）：`render` 统一为 `fn()`
    /// 签名，个别需要宿主的页（如 `d_snackbar`）从这里取同一句柄。
    pub(crate) static SNACK_HOST: std::cell::Cell<Option<SnackbarHostState>> =
        const { std::cell::Cell::new(None) };
    /// 外壳 IDE 停靠状态：全 story **只有一个** `DockLayout` 实例。
    /// docking 的命中表（GROUP_RECTS / ROOT_NODE）是 thread_local 单例，
    /// 页面里再嵌一个 DockLayout 会互相覆盖，故 Docking 页改为操控本句柄。
    pub(crate) static SHELL_DOCK: std::cell::Cell<Option<DockState>> =
        const { std::cell::Cell::new(None) };
    pub(crate) static SHELL_DOCK_IDS: std::cell::Cell<Option<ShellDockIds>> =
        const { std::cell::Cell::new(None) };
    /// 侧栏「收起为图标条」状态（共享出来供无头探针回归 rail 模式渲染）。
    pub(crate) static SHELL_RAIL: std::cell::Cell<Option<MutableState<bool>>> =
        const { std::cell::Cell::new(None) };
}

pub(crate) fn shell_rail() -> Option<MutableState<bool>> {
    SHELL_RAIL.with(|c| c.get())
}

/// VS2022 工作区各面板的 id（注册后写入 thread_local，供 Docking 页操控）。
#[derive(Clone, Copy)]
pub(crate) struct ShellDockIds {
    pub(crate) sidebar: PanelId,
    pub(crate) documents: PanelId,
    pub(crate) toolbox: PanelId,
    pub(crate) output: PanelId,
    pub(crate) errors: PanelId,
    pub(crate) properties: PanelId,
}

pub(crate) fn shell_dock() -> Option<DockState> {
    SHELL_DOCK.with(|c| c.get())
}

pub(crate) fn shell_dock_ids() -> Option<ShellDockIds> {
    SHELL_DOCK_IDS.with(|c| c.get())
}

pub(crate) fn set_shell_dock(state: DockState, ids: ShellDockIds) {
    SHELL_DOCK.with(|c| c.set(Some(state)));
    SHELL_DOCK_IDS.with(|c| c.set(Some(ids)));
}

pub(crate) fn set_shell_rail(rail: MutableState<bool>) {
    SHELL_RAIL.with(|c| c.set(Some(rail)));
}

/// VS2022 默认工作区：左（解决方案资源管理器 / 工具箱）｜
/// 中（文档 上 / 输出·错误列表 下）｜右（属性）。
pub(crate) fn ide_default_tree(i: ShellDockIds) -> DockNode {
    DockNode::split(
        SplitOrientation::Horizontal,
        0.20,
        DockNode::tabs(&[i.sidebar, i.toolbox]),
        DockNode::split(
            SplitOrientation::Horizontal,
            0.78,
            DockNode::split(
                SplitOrientation::Vertical,
                0.72,
                DockNode::tabs(&[i.documents]),
                DockNode::tabs(&[i.output, i.errors]),
            ),
            DockNode::tabs(&[i.properties]),
        ),
    )
}

// ════════════════════════════ App + 外壳 ════════════════════════════

pub(crate) fn nav_items() -> Vec<NavItem> {
    PAGES
        .iter()
        .map(|p| NavItem::new(p.icon, p.name))
        .collect()
}

/// 按 `GROUPS` 分组的侧栏 sections（folder）；分组连续覆盖页面，flat 序号 == 页下标。
pub(crate) fn nav_sections() -> Vec<SidebarSection> {
    let items = nav_items();
    GROUPS
        .iter()
        .map(|(g, r)| {
            let mut s = SidebarSection::new(*g);
            for i in r.clone() {
                if let Some(it) = items.get(i) {
                    s.items.push(it.clone());
                }
            }
            s
        })
        .collect()
}

pub(crate) fn section(title: &str) {
    let colors = theme_colors();
    Text(
        title.to_string(),
        TextStyle::new()
            .size(13.0)
            .weight(700)
            .color(colors.primary),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));
}

pub(crate) fn card_section(title: &str, build: impl Fn() + 'static) {
    let colors = theme_colors();
    let t = title.to_string();
    Card(
        Modifier::new()
            .padding(12.0)
            .border(1.0, colors.outline_variant, 12.0),
        move || {
            Text(
                t.clone(),
                TextStyle::new()
                    .size(13.0)
                    .weight(700)
                    .color(colors.primary),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(8.0));
            build();
        },
    );
    Spacer(Modifier::new().height(12.0));
}
