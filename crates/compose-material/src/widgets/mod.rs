//! Material 3 控件集 — 按功能分组（八大分类）。
//!
//! | 模块 | 分类 | 代表控件 |
//! |---|---|---|
//! | [`basic`] | basic | Text / Button / Card / Chip / Avatar / Badge / SearchBar |
//! | [`input`] | input | Checkbox / Radio / Switch / Slider / TextField / Dropdown |
//! | [`layout`] | layout | Scaffold / TopAppBar / FlowRow / Grid / Split / Expandable |
//! | [`data`] | data | List / DataTable / Tree / Chart / Progress / Stepper |
//! | [`feedback`] | feedback | Dialog / Snackbar / Tooltip / Popover / Skeleton |
//! | [`navigation`] | navigation | NavigationBar / Rail / Tabs / Breadcrumb / Pagination |
//! | [`advanced`] | advanced | ScrollView / Calendar / CodeBlock / Settings |
//! | [`animation`] | animation | AnimatedVisibility / Crossfade / SharedAxis / Shimmer |
//! | [`glass`] | — | 液态玻璃开关与壁纸 |
//! | [`overlay`] | root_window+overlay | 全局浮层宿主 |

pub mod advanced;
pub mod animation;
pub mod basic;
pub mod chart;
pub mod data;
pub mod dock_layout;
pub mod dock_model;
pub mod docking;
pub mod feedback;
pub mod glass;
pub mod grid_sheet;
pub mod input;
pub mod layout;
pub mod navigation;
pub mod file_dialog;
pub mod image;
pub mod overlay;
pub mod rich_text_editor;

use compose::foundation::Color;
use compose::layout::{DrawBuilder, GlassSpec, Modifier, NodeId, Policy, TextStyle, add_node};
use compose::material::{LocalContentColor, theme_colors};
use compose::runtime::{MutableState, NavKey, with_rt_pub};

// ══════════════════════════════ 共享基础 ══════════════════════════════

/// `Text(Text, style, modifier)` — Compose `Text`。
///
/// 未显式着色（默认 BLACK）的文本继承 `LocalContentColor`（容器给的前景色），
/// 容器未提供时回退主题 `on_surface` —— 暗色主题因此默认可读，按钮禁用态也只算一次。
pub fn Text(txt: impl Into<String>, style: TextStyle, modifier: Modifier) {
    let txt = txt.into();
    let style = if style.color == Color::BLACK {
        style.color(LocalContentColor::current())
    } else {
        style
    };
    let id = add_node(Policy::Leaf, modifier);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(
            move |b: &mut DrawBuilder, x, y, w: f32, _h: f32| {
                // 折行宽度 = 节点宽度，和测量阶段（leaf_intrinsic 传父级可用宽）一致，
                // 否则正文在卡片里横向溢出、在窄容器里又按单行量高。
                b.draw_text(x, y, &txt, &style, w);
            },
        ));
    });
}

/// 主题化文本色：`BLACK`（未指定）→ `on_surface`。
pub fn ThemedColor(color: Color) -> Color {
    if color == Color::BLACK {
        theme_colors().on_surface
    } else {
        color
    }
}

/// `RotatingChevron(up, size, color, modifier)` — M3 `AnimatedRotation` 风格的展开箭头。
///
/// `up` 从 0（向下）连续转到 1（向上）。引擎没有 rotate/graphicsLayer，故直接算
/// 旋转后的折线顶点：基础形状是绕中心的一个 V，按 `up·π` 旋转。
pub fn RotatingChevron(up: f32, size: f32, color: Color, modifier: Modifier) {
    let id = add_node(Policy::Leaf, modifier.size(size, size));
    let (a, c) = (up, color);
    with_rt_pub(move |rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
            let (si, co) = (a * std::f32::consts::PI).sin_cos();
            let (cx, cy) = (x + size * 0.5, y + size * 0.5);
            let mut pt = move |fx: f32, fy: f32| {
                let (px, py) = (fx * size, fy * size);
                (cx + px * co - py * si, cy + px * si + py * co)
            };
            let (x1, y1) = pt(-0.28, -0.08);
            let (x2, y2) = pt(0.0, 0.14);
            let (x3, y3) = pt(0.28, -0.08);
            let line = (size * 0.1).max(1.5);
            b.line(x1, y1, x2, y2, c, line);
            b.line(x2, y2, x3, y3, c, line);
        }));
    });
}

/// 树节点 / 分组用的「向右 → 向下」箭头（Flutter `AnimatedRotation(turns: 0.25)` 的等价物）。
///
/// `RotatingChevron` 的角度轴是「下(0) ⇄ 上(1)」，转半圈 = π。折叠态要停在「右」，
/// 即比「下」再倒退 90° ⇒ `up = frac·0.5 − 0.5`（0 ⇒ 右，1 ⇒ 下）。
pub fn RotatingExpander(frac: f32, size: f32, color: Color, modifier: Modifier) {
    RotatingChevron(frac * 0.5 - 0.5, size, color, modifier);
}

/// 液态玻璃 / 普通底色的统一入口：玻璃态用背景模糊材质，否则用不透明底色。
pub fn GlassOrBg(m: Modifier, g: GlassSpec, color: Color, radius: f32) -> Modifier {
    if g.enabled {
        m.glass(g.with_radius(radius))
    } else {
        m.background(color, radius)
    }
}

/// 节点在窗口内的绝对位置（浮层锚定用；沿父链累加，忽略滚动偏移）。
pub fn NodeAbsPos(id: NodeId) -> (f32, f32) {
    with_rt_pub(|rt| {
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut cur = Some(id);
        while let Some(n) = cur {
            x += rt.nodes[n.index()].x;
            y += rt.nodes[n.index()].y;
            cur = rt.nodes[n.index()].parent;
        }
        (x, y)
    })
}

/// 列表类控件的键 → 新条目下标（夹取到 `[0, len-1]`；空表返回 0）。
///
/// `ListView` / `TreeView` / story 手拼列表共用的条目切换语义（与 `Sidebar` 一致）：
/// ↑/↓ ±1、PageUp/PageDown ±8、Home/End 首尾；Left/Right/Delete 原位（横向语义由
/// 调用方自行处理，如 `GridView` 的二维步长、`TabRow` 的左右切换）。
pub(crate) fn nav_index(key: NavKey, cur: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    match key {
        NavKey::Up => cur.saturating_sub(1),
        NavKey::Down => (cur + 1).min(last),
        NavKey::Left | NavKey::Right | NavKey::Delete => cur,
        NavKey::PageUp => cur.saturating_sub(8),
        NavKey::PageDown => (cur + 8).min(last),
        NavKey::Home => 0,
        NavKey::End => last,
    }
}

/// 事件回调糖：按 [`nav_index`] 语义把 `sel` 移一格（外部手拼列表的 `on_key` 直接复用，
/// 如 story 的 List 演示；在组合外调用，不注册读取依赖）。
pub fn list_nav_on_key(sel: MutableState<usize>, len: usize, key: NavKey) {
    if len == 0 {
        return;
    }
    let cur = sel.read_unregistered();
    sel.set(nav_index(key, cur, len));
}

// ══════════════════════════════ 扁平 re-export ══════════════════════════════

pub use advanced::{
    AutoHideToolbar, Calendar, CodeBlock, DaysInMonth, HorizontalScroll, MediaSurface, RichText,
    SettingsItem, SettingsSection, VerticalScroll, WeekdayOf,
};
pub use animation::{
    AnimatedImage, AnimatedVisibility, AnimatedWidget, Crossfade, FadeScale, FadeThrough, Hero,
    OpenContainer, PullToRefresh, Pulse, SharedAxis, SlideIn,
};
pub use basic::{
    AssistChip, Avatar, AvatarGroup, Badge, BadgedBox, Button, ButtonEx, ButtonOpts, ButtonVariant,
    Card, CenteredText, Chip, Divider, ElevatedButton, ElevatedCard, ExpandableFab, ExtendedFab,
    Fab, FabEx, FabExpandType, FabSize, FilledButton, FilledCard, FilterChip, GroupBox, IconButton,
    ImagePlaceholder, InkWell,
    InkWellR, Kbd, Link, OutlinedButton, OutlinedCard, SearchBar, StatusDot, Surface, TextButton,
    TonalButton, VerticalDivider, button_icon, button_label,
};
pub use image::{Image, ImageSource};
pub use file_dialog::{FileFilter, open_file_dialog, open_image_dialog};
pub use chart::{
    Candle, Chart, ChartData, ChartOptions, ChartSeries, ChartType, DataPoint, FunnelSlice,
    PieSlice, SankeyLink, SankeyNode, ScaleBand, ScaleLinear, ScalePoint, WaterfallEntry,
    WaterfallKind, format_tick, labeled_items, linear_ticks, nice_linear_domain, nice_step,
    series_palette,
};
pub use data::{
    BarChart, Bubble, CircularProgress, DataTable, DescriptionList, DiffView, ExpansionTile,
    GridView, LineChart, LinearProgress, List, ListCustom, ListItem, ListItemR, ListView,
    ListViewCustom, Markdown, PieChart,
    SearchableList, Sparkline, Stepper, TreeNode, TreeView,
};
pub use dock_model::{
    AnchorId, DockableId, DockNodeId, DockRegion, DockTarget, DockableOptions, DockingStyle, NodeIdGenerator,
    TabPreference, region_is_allowed,
};
pub use dock_layout::{
    DOCK_LAYOUT_FORMAT, DockLayout, DockWindow, MaximizedState, WindowBounds, WindowId,
    WindowKind, WINDOW_MAIN, parse_dock_layout_text, serialize_dock_layout,
};
pub use docking::{
    DOCK_DRAG_THRESHOLD, DOCK_FLOATING_MIN_H, DOCK_FLOATING_MIN_W, DOCK_HEADER_H,
    DOCK_MIN_EXTENT, DOCK_RATIO_MAX, DOCK_RATIO_MIN, DOCK_SPLITTER_PX, DOCK_STRIP_THICKNESS,
    DOCK_ZONE_THRESHOLD, DockContextMenu, DockDirection, DockDragUi, DockEdge, DockLayout,
    DockTabDropdown, LayoutChangeKind,
    DockModel, DockNode, DockRect, DockState, DockablePanel, DockingEvent, FloatingPanel,
    HiddenPanel, PanelId, SavedTreePosition, SplitOrientation, call_dock_panel,
    clear_dock_panels, default_tab_preference, dock_direction_in_rect, dock_panel_exists,
    dock_panel_info, dock_preview_rect, dock_split_bounds, dock_state, drain_dock_events,
    parse_dock_layout, parse_dock_node, register_dock_panel, remember_dock_state,
    serialize_dock_node, set_default_tab_preference, unregister_dock_panel,
};
pub use feedback::{
    AlertDialog, AlertKind, Banner, BottomSheet, CommandPalette, Dialog, EmptyState, HoverCard,
    InlineAlert, ModalDrawer, Notification, NotificationStack, Popover, RememberSnackbarHostState,
    ShimmerPlaceholder, Skeleton, Snackbar, SnackbarHost, SnackbarHostState, Spinner, Tooltip,
};
pub use glass::{
    GlassRootBackground, GlassToggleButton, LiquidGlassWallpaper, SetLiquidGlass, ToggleLiquidGlass,
};
pub use grid_sheet::{
    CellValue, ConditionalFormatRule, FormatCondition, GridSheet, GridSheetColumn, GridSheetConfig,
    GridSheetRow, GridSheetTextAlign,
};
pub use input::{
    BareTextField, Checkbox, CheckboxRow, ColorPicker, ComboBox, ContextMenu, DatePicker, copy_text,
    Dropdown, DropdownMenu, FormField, InputBackspace, InputChar, InputCommit, InputCopy, InputCut,
    InputEnter, InputFindNext, InputFocused, InputKey, InputPaste, InputPreedit, InputRedo,
    InputScroll, InputSelectAll, InputUndo, OtpInput, OutlinedTextField, PasswordField,
    PopupMenuButton, RadioGroup, RatingBar, RichTextEdit, SetShiftState, Slider, SpinBox, Switch,
    SwitchRow, TextField, TimePicker, TokenInput, TriState, TriStateCheckbox,
    dropdown_menu_sections, menu_panel, menu_panel_auto_width, menu_panel_sections,
    run_menu_callback, set_menu_callback,
};
pub use rich_text_editor::{RichTextEditor, RichTextEditorHandle, rich_editor_focus};
pub use layout::{
    AppBarAction, AspectRatio, BottomAppBar, Expandable, FlowRow, Grid, Scaffold, SizedBox,
    SplitColumn, SplitRow, TopAppBar,
};
pub use navigation::{
    Accordion, Breadcrumb, Carousel, NavItem, NavigationBar, NavigationDrawer, NavigationRail,
    PageView, Pagination, Ribbon, Sidebar, SidebarHeader, SidebarSection, StatusBar, TabRow, Tabs,
    TitleBar, Toolbar,
};
pub use overlay::{
    ClearOverlays, NextOverlayKey, Overlay, OverlayHost, OverlaySlot, PutAction, PutIndexedAction,
    PutOverlayContent, RegisterOverlay, RememberOverlayKey, RemoveOverlay, RunAction,
    RunIndexedAction,
};
