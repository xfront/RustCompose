//! 停靠布局的**持久化模型** —— 对齐 compose-docking 的 `DockLayout.kt`。
//!
//! 参考实现：
//! `compose-docking/docking-core/src/commonMain/kotlin/com/seanproctor/docking/model/DockLayout.kt`
//!
//! 与 KT 逐条对齐：
//! * [`DockLayout`] = **窗口列表**（主窗口 + 任意浮动窗口），是唯一被持久化的值；
//!   瞬时 UI 状态（拖拽中、菜单、边条 peek）留在 [`DockState`](super::DockState) 里。
//! * [`DockWindow`] 携带窗口 id、窗口种类、根树、最大化状态、窗口边界。
//! * [`MaximizedState`] 把「被最大化的面板」与「最大化前的根树」打包在一起，
//!   还原最大化因此是一次**纯值替换**（KT 语义），可安全跨进程持久化。
//! * 不变式与 KT 一致：窗口列表非空、`windows[0]` 必须是主窗口、
//!   其余只能是浮动窗口、窗口 id 不重复。
//!
//! 与运行时模型的关系：运行时仍是 [`DockModel`](super::DockModel)（含自动隐藏边条等
//! Rust 扩展），[`DockLayout`] 是它的**窗口视图**——[`DockLayout::from_model`] 投影出
//! 窗口列表，[`DockLayout::apply_to`] 再写回运行时模型。

use super::docking::{DockModel, DockNode, PanelId};

/// 停靠窗口 id（主窗口固定为 [`WINDOW_MAIN`]，浮动窗口自 1 递增）。
pub type WindowId = u32;

/// 应用主窗口的窗口 id（对齐 KT `WindowId.MAIN`）。
pub const WINDOW_MAIN: WindowId = 0;

/// 窗口种类（对齐 KT `WindowKind`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowKind {
    /// 应用主窗口。
    Main,
    /// 浮动窗口（可独立定位、拖动、关闭）。
    Floating,
}

impl WindowKind {
    /// 序列化标签（`Main` / `Floating`）。
    pub fn tag(self) -> &'static str {
        match self {
            WindowKind::Main => "Main",
            WindowKind::Floating => "Floating",
        }
    }

    /// 由标签反解。
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "Main" => Some(WindowKind::Main),
            "Floating" => Some(WindowKind::Floating),
            _ => None,
        }
    }
}

/// 窗口边界（屏幕坐标，dp）—— 对齐 KT `WindowBounds`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WindowBounds {
    /// 左上角 x。
    pub x: f32,
    /// 左上角 y。
    pub y: f32,
    /// 宽。
    pub width: f32,
    /// 高。
    pub height: f32,
}

/// 最大化状态 —— 对齐 KT `MaximizedState`。
///
/// `saved_root` 是最大化**之前**的根树；还原时整棵树被换回，
/// 因此最大化/还原是可持久化的纯值变换。
#[derive(Clone, PartialEq, Debug)]
pub struct MaximizedState {
    /// 被最大化的面板。
    pub dockable_id: PanelId,
    /// 最大化前的根树（`None` = 最大化前窗口是空的）。
    pub saved_root: Option<DockNode>,
}

/// 一个停靠窗口 —— 对齐 KT `DockWindow`。
#[derive(Clone, PartialEq, Debug)]
pub struct DockWindow {
    /// 窗口 id（主窗口为 [`WINDOW_MAIN`]）。
    pub id: WindowId,
    /// 窗口种类。
    pub kind: WindowKind,
    /// 窗口的布局树（`None` = 空窗口）。
    pub root: Option<DockNode>,
    /// 最大化状态（仅主窗口可能非空）。
    pub maximized: Option<MaximizedState>,
    /// 窗口边界（浮动窗口有，主窗口为 `None`）。
    pub bounds: Option<WindowBounds>,
}

/// 完整停靠布局 —— 对齐 KT `DockLayout`。
#[derive(Clone, PartialEq, Debug)]
pub struct DockLayout {
    /// 窗口列表：`windows[0]` 必须是主窗口，其余是浮动窗口。
    pub windows: Vec<DockWindow>,
}

impl DockLayout {
    /// 只含一个空主窗口的布局（对齐 KT `DockLayout.empty()`）。
    pub fn empty() -> Self {
        DockLayout {
            windows: vec![DockWindow {
                id: WINDOW_MAIN,
                kind: WindowKind::Main,
                root: None,
                maximized: None,
                bounds: None,
            }],
        }
    }

    /// 主窗口（`windows[0]`）。
    pub fn main_window(&self) -> &DockWindow {
        &self.windows[0]
    }

    /// 主窗口（可变）。
    pub fn main_window_mut(&mut self) -> &mut DockWindow {
        &mut self.windows[0]
    }

    /// 全部浮动窗口。
    pub fn floating_windows(&self) -> &[DockWindow] {
        &self.windows[1..]
    }

    /// 按 id 查窗口。
    pub fn window(&self, id: WindowId) -> Option<&DockWindow> {
        self.windows.iter().find(|w| w.id == id)
    }

    /// 按 id 查窗口，缺失则 panic（对齐 KT `requireWindow`）。
    pub fn require_window(&self, id: WindowId) -> &DockWindow {
        self.window(id)
            .unwrap_or_else(|| panic!("no window with id '{id}' in layout"))
    }

    /// 替换指定 id 的窗口，返回新布局（对齐 KT `replaceWindow`）。
    pub fn replace_window(&self, window: DockWindow) -> DockLayout {
        DockLayout {
            windows: self
                .windows
                .iter()
                .map(|w| if w.id == window.id { window.clone() } else { w.clone() })
                .collect(),
        }
    }

    /// 不变式检查（对齐 KT `DockLayout.init` 的 `require`）。
    ///
    /// * 窗口列表非空；
    /// * `windows[0]` 必须是主窗口；
    /// * 其余窗口只能是浮动窗口；
    /// * 窗口 id 不重复。
    pub fn is_valid(&self) -> bool {
        if self.windows.is_empty() {
            return false;
        }
        if self.windows[0].kind != WindowKind::Main {
            return false;
        }
        if self.windows[1..].iter().any(|w| w.kind != WindowKind::Floating) {
            return false;
        }
        for i in 0..self.windows.len() {
            for j in (i + 1)..self.windows.len() {
                if self.windows[i].id == self.windows[j].id {
                    return false;
                }
            }
        }
        true
    }

    /// 面板所在的窗口 id（`None` = 该面板不在任何窗口里）。
    pub fn window_of(&self, panel: PanelId) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|w| {
                w.root
                    .as_ref()
                    .is_some_and(|r| r.collect_ids().contains(&panel))
            })
            .map(|w| w.id)
    }

    /// 从运行时模型投影出窗口列表。
    ///
    /// * 主窗口 = 根树 + 最大化状态；
    /// * 每个浮动组 = 一个浮动窗口（带边界）。
    ///
    /// 自动隐藏边条是 Rust 扩展、KT 模型无对应概念，故不进入窗口列表，
    /// 由 [`DockModel`] 自身保留（见 [`DockLayout::apply_to`]）。
    pub fn from_model(m: &DockModel) -> DockLayout {
        let mut windows = Vec::with_capacity(1 + m.floating.len());
        windows.push(DockWindow {
            id: WINDOW_MAIN,
            kind: WindowKind::Main,
            root: m.root.clone(),
            maximized: m.maximized.map(|id| MaximizedState {
                dockable_id: id,
                saved_root: m.saved_before_maximize.clone(),
            }),
            bounds: None,
        });
        for f in &m.floating {
            windows.push(DockWindow {
                id: f.id,
                kind: WindowKind::Floating,
                // 浮动窗口与主窗口同构：完整保留布局树（KT 语义），
                // 跨窗口停靠产生的窗口内分割因此可被持久化。
                root: Some(f.root.clone()),
                maximized: None,
                bounds: Some(WindowBounds {
                    x: f.x,
                    y: f.y,
                    width: f.w,
                    height: f.h,
                }),
            });
        }
        DockLayout { windows }
    }

    /// 把窗口列表写回运行时模型。
    ///
    /// 主窗口覆盖根树与最大化状态；浮动窗口重建浮动组（按窗口 id 对齐）。
    /// 自动隐藏边条与 peek 预览等 Rust 扩展状态保持不变。
    pub fn apply_to(&self, m: &mut DockModel) {
        let main = self.main_window();
        m.root = main.root.clone();
        match &main.maximized {
            Some(mx) => {
                m.maximized = Some(mx.dockable_id);
                m.saved_before_maximize = mx.saved_root.clone();
            }
            None => {
                m.maximized = None;
                m.saved_before_maximize = None;
            }
        }
        let mut floating = Vec::new();
        for w in self.floating_windows() {
            let Some(root) = w.root.clone() else {
                continue;
            };
            if root.collect_ids().is_empty() {
                continue;
            }
            let b = w.bounds.unwrap_or(WindowBounds {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            });
            floating.push(super::docking::FloatingPanel {
                id: w.id,
                root,
                x: b.x,
                y: b.y,
                w: b.width,
                h: b.height,
                saved: None,
            });
        }
        m.floating = floating;
    }
}

/// 持久化格式版本头（`dl1`）。
pub const DOCK_LAYOUT_FORMAT: &str = "dl1";

/// 序列化为 `dl1` 文本（每窗口一行）。
///
/// 格式（字段以 `;` 分隔——布局树文本里含 `|`，故不能用 `|` 做分隔符）：
///
/// ```text
/// dl1
/// w=0;Main;-;-;-;S(V,0.5000,T(1|1),T(2|2))
/// w=1;Floating;10.0,20.0,300.0,200.0;-;-;T(3|3)
/// ```
///
/// 各字段依次为：窗口 id、窗口种类、边界 `x,y,w,h`（`-` = 无）、
/// 被最大化的面板（`-` = 无）、最大化前的根树（`-` = 无）、根树（`-` = 空窗口）。
pub fn serialize_dock_layout(layout: &DockLayout) -> String {
    let mut out = String::from(DOCK_LAYOUT_FORMAT);
    out.push('\n');
    for w in &layout.windows {
        let bounds = match w.bounds {
            Some(b) => format!("{},{},{},{}", b.x, b.y, b.width, b.height),
            None => "-".to_string(),
        };
        let max = match &w.maximized {
            Some(mx) => mx.dockable_id.to_string(),
            None => "-".to_string(),
        };
        let saved = match w.maximized.as_ref().and_then(|m| m.saved_root.as_ref()) {
            Some(r) => super::docking::serialize_dock_node(r),
            None => "-".to_string(),
        };
        let root = match &w.root {
            Some(r) => super::docking::serialize_dock_node(r),
            None => "-".to_string(),
        };
        out.push_str(&format!(
            "w={};{};{};{};{};{}\n",
            w.id,
            w.kind.tag(),
            bounds,
            max,
            saved,
            root
        ));
    }
    out
}

/// 解析 `dl1` 文本 → 窗口列表。
///
/// 解析成功后仍需调用 [`DockLayout::is_valid`] 校验不变式（KT 的 `init` 会在
/// 反序列化时抛错；Rust 侧改为显式校验，避免 panic）。
pub fn parse_dock_layout_text(text: &str) -> Option<DockLayout> {
    let mut lines = text.lines();
    if lines.next()? != DOCK_LAYOUT_FORMAT {
        return None;
    }
    let mut windows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let body = line.strip_prefix("w=")?;
        let parts: Vec<&str> = body.split(';').collect();
        if parts.len() != 6 {
            return None;
        }
        let id: WindowId = parts[0].parse().ok()?;
        let kind = WindowKind::from_tag(parts[1])?;
        let bounds = if parts[2] == "-" {
            None
        } else {
            let nums: Vec<f32> = parts[2]
                .split(',')
                .map(|s| s.parse::<f32>().ok())
                .collect::<Option<Vec<f32>>>()?;
            if nums.len() != 4 {
                return None;
            }
            Some(WindowBounds {
                x: nums[0],
                y: nums[1],
                width: nums[2],
                height: nums[3],
            })
        };
        let maximized = if parts[3] == "-" {
            None
        } else {
            let dockable_id: PanelId = parts[3].parse().ok()?;
            let saved_root = if parts[4] == "-" {
                None
            } else {
                Some(super::docking::parse_dock_layout(parts[4])?)
            };
            Some(MaximizedState {
                dockable_id,
                saved_root,
            })
        };
        let root = if parts[5] == "-" {
            None
        } else {
            Some(super::docking::parse_dock_layout(parts[5])?)
        };
        windows.push(DockWindow {
            id,
            kind,
            root,
            maximized,
            bounds,
        });
    }
    Some(DockLayout { windows })
}
