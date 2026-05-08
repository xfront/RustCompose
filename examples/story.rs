//! Storybook 高质量还原 —— 103 页 / 7 大分组结构（Demos / Basics / Data Display /
//! Layout / Navigation / Feedback / Advanced），外壳为「分组侧栏 + 标题栏/菜单栏 + 状态栏 +
//! 页面容器按需替换」，每页用 `compose_material` 的真实控件渲染。
//!
//! 运行：`cargo run --example story`

use compose::animation::{
    Spring, Tween, animate_float_as_state, ease_out_cubic, request_anim_frames,
};
use compose::layout::{
    Alignment, Arrangement, Box, Canvas, Clickable, Column, CrossAxisAlignment, DrawBuilder,
    Modifier, Row, Spacer, TextStyle,
};
use compose::material::{
    ColorScheme, IconName, MaterialTheme, THEME_PRESETS, draw_icon, icon, liquid_glass_state,
    theme_colors,
};
use compose::prelude::Color;
use compose::runtime::{
    MutableState, add_frame_driver, current_scope_id, frame_delta, mutableStateOf, remember,
    schedule_scope,
};
use compose_material::widgets::{set_menu_callback, SidebarSection, menu_panel_auto_width};
use compose_material::*;
use compose::{App, WindowConfig, run_app_with};

// ════════════════════════════ 页面注册表（103 页） ════════════════════════════

/// 单页元数据：标题 + 侧栏图标 + 页头一句话说明。
///
/// 过去 `PAGES` / `PAGE_ICONS` / `PAGE_BLURBS` 是三条按下标并行的数组，极易走样
/// （曾出现 `PAGE_ICONS` 比 `PAGES` 少两项，末两页从侧栏 `zip` 中丢失、选中即越界）。
/// 合并为一条按页对齐的记录后，三者天然同序、不可能再错位。
struct PageInfo {
    name: &'static str,
    icon: IconName,
    blurb: &'static str,
    /// 该页正文渲染函数引用：PAGES 因此成为「标题/图标/说明/渲染」的唯一真源，
    /// 不再需要按下标分发的 `dispatch_demo` 大 match（曾出现下标与函数错位的隐患）。
    builder: fn(),
}

const fn page(
    name: &'static str,
    icon: IconName,
    blurb: &'static str,
    render: fn(),
) -> PageInfo {
    PageInfo {
        name,
        icon,
        blurb,
        builder: render,
    }
}

const PAGES: &[PageInfo] = &[
    // Demos (10)
    page("Spreadsheet", IconName::Table, "带公式栏与统计摘要的网格，演示 DataTable 的对齐与密度。", page_spreadsheet),
    page("Chat", IconName::Send, "由消息气泡、时间戳与输入区组成的聊天界面。", page_chat),
    page("Animation", IconName::Play, "缓动、弹簧、关键帧与序列动画的实时演示。", page_animation),
    page("Material Motion", IconName::Layers, "SharedAxis / FadeThrough / ContainerTransform 三种 M3 转场。", page_material_motion),
    page("Pull To Refresh", IconName::Refresh, "下拉越过阈值触发刷新，进度弧随拉动实时增长。", page_pull_refresh),
    page("Hero Animation", IconName::Star, "共享元素在两页之间飞行，落地时尺寸与圆角连续过渡。", page_hero),
    page("Cross Fade", IconName::Brush, "内容切换时淡入淡出，布局槽位保持稳定。", page_cross_fade),
    page("Custom Widget", IconName::Terminal, "用 Canvas 与状态自己组装一个仪表盘控件。", page_custom_widget),
    page("Async", IconName::Cloud, "异步任务的进度、加载态与完成提示。", page_async),
    page("Reactive & DSL", IconName::Code, "状态变化自动驱动重组的最小响应式示例。", page_reactive),
    // Basics (18)
    page("Buttons", IconName::Star, "Filled / Outlined / Text / Icon / FAB 五种按钮形态。", page_buttons),
    page("Toggles", IconName::Check, "Checkbox、Switch 与 RadioGroup 的选择语义。", page_toggles),
    page("Value Input", IconName::Volume, "Slider、线性进度与 SpinBox 的数值输入。", page_sliders),
    page("Text Input", IconName::Edit, "Filled / Outlined / Password / OTP / Token 输入框全览。", page_text_input),
    page("Icon", IconName::Star, "矢量图标全集与尺寸变体。", page_icons),
    page("AvatarGroup", IconName::User, "头像、徽标与溢出计数的组合。", page_avatars),
    page("ButtonGroup", IconName::Layers, "单选 / 多选按钮组的分段选择。", page_buttons),
    page("GroupBox", IconName::Folder, "带标题的分组容器。", page_group_box),
    page("Kbd", IconName::Terminal, "键盘按键样式与组合键排版。", page_kbd),
    page("Link", IconName::Link, "行内链接与访问态。", page_link),
    page("SearchBar", IconName::Search, "搜索输入与一键清空。", page_text_input),
    page("ComboBox", IconName::List, "下拉选择器与选项浮层。", page_text_input),
    page("ContextMenu", IconName::MoreVert, "右键上下文菜单。", page_context_menu),
    page("MenuBar", IconName::Menu, "多级菜单栏与浮层定位。", page_menu_demo),
    page("OtpInput", IconName::Lock, "验证码输入与自动跳格。", page_text_input),
    page("TokenInput", IconName::Tag, "标签式多值输入。", page_text_input),
    page("Form / FormField", IconName::Note, "表单标签、校验与错误提示。", page_text_input),
    page("ExpandableFab", IconName::Add, "可展开的浮动操作按钮。", page_buttons),
    // Data Display (16)
    page("Cards", IconName::Layers, "Filled / Elevated / Outlined 三种卡片。", page_cards),
    page("Badges & Avatars", IconName::Bell, "徽标与头像组。", page_avatars),
    page("Chips & Tags", IconName::Tag, "Assist / Filter / Input 芯片。", page_chips),
    page("List", IconName::List, "列表项与分组；聚焦后可用 ↑/↓、PageUp/Down、Home/End 切换。", page_list),
    page("ListView", IconName::List, "虚拟滚动列表；聚焦后键切换条目，选中自动滚入视口。", page_list_view),
    page("Grid", IconName::Grid, "自适应网格布局。", page_grid),
    page("Tree", IconName::Tree, "可折叠树形结构；↑/↓ 选择、←/→ 折叠展开。", page_tree),
    page("Table", IconName::Table, "表头、斑马纹与网格线。", page_table),
    page("Color", IconName::Palette, "颜色模型与色板浏览。", page_color),
    page("Bubble", IconName::Mail, "聊天气泡样式。", page_bubble),
    page("Chart", IconName::Chart, "折线 / 柱状 / 饼图与图例。", page_charts),
    page("DescriptionList", IconName::Note, "键值描述列表。", page_description),
    page("DiffView", IconName::Code, "并排差异对比。", page_diff),
    page("Markdown", IconName::Type, "轻量 Markdown 渲染。", page_markdown),
    page("SearchableList", IconName::Search, "带过滤的搜索列表。", page_searchable_list),
    page("GridSheet", IconName::Grid, "带表头与选区的电子表格。", page_grid_sheet),
    // Layout (13)
    page("Row & Column", IconName::Layout, "主轴 / 交叉轴排列。", page_row_col),
    page("Expanded / Flexible", IconName::Layout, "按权重分配剩余空间。", page_flex),
    page("Wrap", IconName::Layers, "自动换行布局。", page_wrap),
    page("Stack", IconName::Layers, "叠层定位。", page_stack),
    page("Align / Spacer", IconName::ExpandMore, "对齐与占位。", page_align),
    page("Modifier", IconName::Brush, "链式修饰符组合。", page_modifier),
    page("Extended Modifiers", IconName::Brush, "尺寸、边框、阴影、偏移等扩展修饰。", page_modifier),
    page("Splitter", IconName::Grid, "可拖拽分隔条。", page_splitter),
    page("Zoomable", IconName::ZoomIn, "缩放与平移画布。", page_zoomable),
    page("Card", IconName::Layers, "卡片容器与内容排布。", page_cards),
    page("Divider", IconName::Remove, "分隔线。", page_divider),
    page("SizedBox", IconName::Image, "固定尺寸盒子。", page_sized_box),
    page("Spacer", IconName::ExpandMore, "弹性留白。", page_align),
    // Navigation (16)
    page("Tabs", IconName::Layers, "标签页与内容切换；聚焦后 ←/→ 切换标签。", page_tabs),
    page("Stepper", IconName::Check, "步骤指示器。", page_stepper),
    page("ExpansionTile", IconName::ExpandMore, "可展开列表项。", page_expansion),
    page("NavigationRail", IconName::Menu, "侧边导航栏。", page_nav_rail),
    page("PageView", IconName::Play, "分页滑动视图。", page_page_view),
    page("Drawer & BottomSheet", IconName::Menu, "抽屉与底部弹层。", page_drawer),
    page("Accordion", IconName::ExpandMore, "手风琴折叠面板。", page_accordion),
    page("Breadcrumb", IconName::ChevronRight, "面包屑导航。", page_breadcrumb),
    page("Carousel", IconName::Play, "轮播卡片。", page_page_view),
    page("Pagination", IconName::Bookmark, "分页器。", page_pagination),
    page("Ribbon", IconName::Star, "功能区工具条。", page_ribbon),
    page("StatusBar", IconName::Info, "底部状态栏。", page_status_bar_demo),
    page("TitleBar", IconName::Home, "窗口标题栏。", page_title_bar_demo),
    page("Toolbar", IconName::Settings, "工具按钮条。", page_toolbar),
    page("TabBar / TabController", IconName::Layers, "可编程标签控制器。", page_tab_bar),
    page("Navigator / Overlay / Router", IconName::Layers, "导航栈与浮层路由。", page_navigator),
    // Feedback (16)
    page("Search", IconName::Search, "搜索页样式。", page_text_input),
    page("Rating", IconName::Star, "星级评分。", page_rating),
    page("ColorPicker", IconName::Palette, "颜色选择器。", page_color_picker),
    page("TimePicker", IconName::Clock, "时间选择。", page_time_picker),
    page("DatePicker", IconName::Calendar, "日期选择。", page_date_picker),
    page("SnackBar & Banner", IconName::Bell, "轻提示与横幅。", page_snackbar),
    page("Alert", IconName::Warning, "警告对话框。", page_alert),
    page("CommandPalette", IconName::Terminal, "命令面板。", page_command_palette),
    page("EmptyState", IconName::Info, "空状态占位。", page_empty_state),
    page("HoverCard", IconName::Info, "悬停卡片。", page_hover_card),
    page("Notification", IconName::Bell, "通知中心。", page_notification),
    page("ProgressCircle", IconName::Refresh, "圆形进度。", page_progress_circle),
    page("Skeleton", IconName::Layers, "骨架屏。", page_skeleton),
    page("Spinner", IconName::Refresh, "加载动画。", page_sliders),
    page("Tooltip", IconName::Info, "工具提示。", page_tooltip),
    page("Dialog", IconName::Warning, "模态对话框。", page_dialog),
    // Advanced (14)
    page("InkWell", IconName::Star, "点击波纹与状态层。", page_ink_well),
    page("PopupMenuButton", IconName::MoreVert, "弹出菜单按钮。", page_popup_menu),
    page("AnimatedWidget", IconName::Play, "显式动画组件。", page_animated),
    page("Images", IconName::Image, "图片与裁剪。", page_images),
    page("Animated GIF", IconName::Video, "动图播放。", page_animated_gif),
    page("Native Dialogs", IconName::Warning, "原生文件对话框。", page_native_dialogs),
    page("ScrollArea", IconName::List, "滚动容器与滚动条。", page_scroll_area),
    page("Builder Helpers", IconName::Code, "构建辅助函数。", page_builders),
    page("Calendar", IconName::Grid, "月历与日期选择。", page_calendar),
    page("CodeEditor", IconName::Type, "代码块与等宽排版。", page_code_editor),
    page("Docking", IconName::Settings, "可拖拽停靠面板。", page_docking),
    page("RichTextEditor", IconName::Settings, "富文本编辑。", page_rich_text),
    page("Settings", IconName::Settings, "设置页布局。", page_settings),
    page("AutoHideToolbar", IconName::Menu, "自动隐藏工具条。", page_auto_hide),
];

const GROUPS: &[(&str, std::ops::Range<usize>)] = &[
    ("Demos", 0..10),
    ("Basics", 10..28),
    ("Data Display", 28..44),
    ("Layout", 44..57),
    ("Navigation", 57..73),
    ("Feedback", 73..89),
    ("Advanced", 89..103),
];

fn page_blurb(idx: usize) -> &'static str {
    PAGES.get(idx).map(|p| p.blurb).unwrap_or("")
}

const MENUS: &[&str] = &["File", "View", "Theme", "Help"];
const MENU_W: f32 = 76.0;
const MENU_H: f32 = 36.0;
const TITLE_H: f32 = 48.0; // 标题栏高度（下拉按窗口坐标定位到菜单栏下方）
const CLOSED: i8 = -1;
/// 菜单栏下拉浮层的注册键。
const MENU_KEY: u64 = 0x4D45_4E55;

thread_local! {
    /// 顶部菜单栏各标签的锚点节点（按 `MENUS` 下标）：`menu_bar` 每次组合刷新，
    /// `dropdown_overlay` 据此把库的 `menu_panel` 钉在当前打开菜单标签正下方。
    static MENU_ANCHORS: std::cell::RefCell<Vec<compose::layout::NodeId>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// 当前 Snackbar 宿主句柄（`story_shell` 创建后写入）：`render` 统一为 `fn()`
    /// 签名，个别需要宿主的页（如 `d_snackbar`）从这里取同一句柄。
    static SNACK_HOST: std::cell::Cell<Option<SnackbarHostState>> =
        const { std::cell::Cell::new(None) };
}

// ════════════════════════════ App + 外壳 ════════════════════════════

#[derive(Clone, Copy)]
struct StoryApp {
    dark: bool,
    palette: usize,
}

/// 色板 0 保持 M3 baseline；其余按种子色生成（品牌色替换、中性面带轻色调）。
fn scheme_for(palette: usize, dark: bool) -> ColorScheme {
    if palette == 0 {
        if dark { ColorScheme::dark() } else { ColorScheme::light() }
    } else {
        let seed = THEME_PRESETS
            .get(palette)
            .map(|p| p.seed)
            .unwrap_or(THEME_PRESETS[0].seed);
        ColorScheme::from_seed(seed, dark)
    }
}

impl App for StoryApp {
    fn theme(&self) -> ColorScheme {
        scheme_for(self.palette, self.dark)
    }

    fn content(&self) {
        let dark = remember(|| mutableStateOf(self.dark));
        let palette = remember(|| mutableStateOf(self.palette));
        MaterialTheme(scheme_for(palette.get(), dark.get()), move || {
            let colors = theme_colors();
            OverlayHost(Modifier::new().fill_max_size(), move || {
                Box(
                    Modifier::new().fill_max_size(),
                    Alignment::TopStart,
                    move || {
                        if liquid_glass_state().get() {
                            LiquidGlassWallpaper(colors);
                        }
                        story_shell(dark, palette);
                    },
                );
            });
        });
    }
}

/// 调试入口：`STORY_PAGE=<下标|页名>` 直接打开指定页（性能定位 / 无头复现用）。
fn initial_page() -> usize {
    match std::env::var("STORY_PAGE") {
        Ok(v) => v
            .parse::<usize>()
            .ok()
            .filter(|i| *i < PAGES.len())
            .or_else(|| PAGES.iter().position(|p| p.name.eq_ignore_ascii_case(&v)))
            .unwrap_or(0),
        Err(_) => 0,
    }
}

fn story_shell(dark: MutableState<bool>, palette: MutableState<usize>) {
    let page = remember(|| mutableStateOf(initial_page()));
    let open_menu = remember(|| mutableStateOf(CLOSED));
    #[cfg(test)]
    {
        probe_state::OM.with(|c| c.set(open_menu.get()));
        probe_state::PAGE.with(|c| c.set(page.get()));
    }
    let snack = RememberSnackbarHostState();
    SNACK_HOST.with(|c| c.set(Some(snack)));
    let sidebar_collapsed = remember(|| mutableStateOf(Vec::<bool>::new()));
    let sidebar_rail = remember(|| mutableStateOf(false));
    Column(
        Modifier::new().fill_max_size(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            story_title_bar(dark);
            menu_bar(open_menu);
            ribbon_bar(dark, palette);
            Box(
                Modifier::new().weight(1.0, true),
                Alignment::TopStart,
                move || {
                    let om = open_menu;
                    Column(
                        Modifier::new().fill_max_size(),
                        Arrangement::Start,
                        CrossAxisAlignment::Stretch,
                        move || {
                            let sn = snack;
                            // 右内边距 0：让内容区 VerticalScroll 节点外框贴窗口右缘，
                            // 引擎把滚动条画在节点外框右缘（n.width - thickness - margin），
                            // 于是滚动条落到窗口右缘（红框处）。右 gutter 改由 VerticalScroll 内部 padding 提供。
                            Row(
                                Modifier::new().fill_max_size().padding_ltrb(14.0, 10.0, 0.0, 10.0),
                                Arrangement::Start,
                                CrossAxisAlignment::Stretch,
                                move || {
                                    let colors = theme_colors();
                                    // 图标条（最小化）⇄ 展开：宽度随状态平滑动画
                                    // 84：让居中的图标胶囊（40）与右侧滚动条（占视口右缘 ~10px）
                                    // 之间留出 gutter，避免胶囊压在滚动条上（56 时二者重叠）。
                                    let rail_w = animate_float_as_state(
                                        if sidebar_rail.get() { 84.0 } else { 228.0 },
                                        Tween::new(0.28),
                                    );
                                    Column(
                                        Modifier::new()
                                            .width(rail_w)
                                            .fill_max_height()
                                            .background(colors.surface_container_low, 16.0)
                                            .border(1.0, colors.outline_variant, 16.0)
                                            .padding(8.0),
                                        Arrangement::Start,
                                        CrossAxisAlignment::Stretch,
                                        move || {
                                            if sidebar_rail.get() {
                                                // 图标条：仅一个居中「展开」按钮（»）
                                                Row(
                                                    Modifier::new()
                                                        .fill_max_width()
                                                        .height(52.0),
                                                    Arrangement::Center,
                                                    CrossAxisAlignment::Center,
                                                    move || {
                                                        let st = sidebar_rail;
                                                        IconButton(
                                                            IconName::ChevronRight,
                                                            Modifier::new(),
                                                            move || st.set(false),
                                                        );
                                                    },
                                                );
                                            } else {
                                                // 展开：标题 + 「收起为图标条」按钮（«）
                                                SidebarHeader(
                                                    "Storybook",
                                                    false,
                                                    move || {
                                                        sidebar_rail.set(true);
                                                    },
                                                    Modifier::new(),
                                                );
                                                Spacer(Modifier::new().height(4.0));
                                            }
                                            Sidebar(
                                                &nav_sections(),
                                                page,
                                                sidebar_collapsed,
                                                sidebar_rail,
                                                Modifier::new().fill_max_height(),
                                            );
                                        },
                                    );
                                    Spacer(Modifier::new().width(14.0));
                                    VerticalScroll(
                                        Modifier::new()
                                            .weight(1.0, true)
                                            .fill_max_height()
                                            .padding_hv(4.0, 2.0),
                                        Arrangement::Start,
                                        CrossAxisAlignment::Stretch,
                                        move || build_page_body(page),
                                    );
                                },
                            );
                            SnackbarHost(sn);
                        },
                    );
                    // 下拉浮层：注册到根 OverlayHost，绝对浮在内容之上
                    // （作为内容区 Column 的流式兄弟会被占满高度的 Row 挤到视口外且压成 0 高）。
                    Overlay(MENU_KEY, om.get() != CLOSED, move || {
                        dropdown_overlay(om, dark, page, palette);
                    });
                },
            );
            story_status_bar(page, palette, dark);
        },
    );
}

fn nav_items() -> Vec<NavItem> {
    PAGES
        .iter()
        .map(|p| NavItem::new(p.icon, p.name))
        .collect()
}

/// 按 `GROUPS` 分组的侧栏 sections（folder）；分组连续覆盖页面，flat 序号 == 页下标。
fn nav_sections() -> Vec<SidebarSection> {
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

fn page_group(idx: usize) -> &'static str {
    GROUPS
        .iter()
        .find(|g| g.1.contains(&idx))
        .map(|g| g.0)
        .unwrap_or("")
}

fn story_title_bar(dark: MutableState<bool>) {
    let colors = theme_colors();
    Row(
        Modifier::new()
            .fill_max_width()
            .height(48.0)
            .background(colors.surface_container_low, 0.0)
            .padding_ltrb(14.0, 0.0, 8.0, 0.0),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    // 品牌标：primary → tertiary 渐变方块
                    Canvas(Modifier::new().size(28.0, 28.0), move |b, x, y, w, h| {
                        b.fill_rrect_gradient(
                            x,
                            y,
                            w,
                            h,
                            8.0,
                            colors.primary,
                            colors.tertiary,
                            false,
                        );
                        draw_icon(b, IconName::Layers, x + 5.0, y + 5.0, 18.0, colors.on_primary);
                    });
                    Spacer(Modifier::new().width(10.0));
                    Column(
                        Modifier::new(),
                        Arrangement::Center,
                        CrossAxisAlignment::Start,
                        move || {
                            Text(
                                "RustCompose",
                                TextStyle::new()
                                    .size(15.0)
                                    .weight(750)
                                    .color(colors.on_surface),
                                Modifier::new(),
                            );
                            Text(
                                "Jetpack Compose architecture · GPU accelerated",
                                TextStyle::new()
                                    .size(10.5)
                                    .color(colors.on_surface_variant),
                                Modifier::new(),
                            );
                        },
                    );
                },
            );
            Row(
                Modifier::new(),
                Arrangement::End,
                CrossAxisAlignment::Center,
                move || {
                    IconButton(IconName::Sun, Modifier::new(), move || dark.set(false));
                    IconButton(IconName::Moon, Modifier::new(), move || dark.set(true));
                    IconButton(IconName::Brush, Modifier::new(), || {
                        ToggleLiquidGlass();
                    });
                },
            );
        },
    );
}

/// 标题栏下的「外观」工具条：色板圆点、明暗分段控件、玻璃开关。
fn ribbon_bar(dark: MutableState<bool>, palette: MutableState<usize>) {
    let colors = theme_colors();
    let current = palette.get();
    let preset_name = THEME_PRESETS
        .get(current)
        .map(|p| p.name)
        .unwrap_or("Violet");
    Row(
        Modifier::new()
            .fill_max_width()
            .height(52.0)
            .background(colors.Surface, 0.0)
            .padding_ltrb(16.0, 0.0, 16.0, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                "APPEARANCE",
                TextStyle::new()
                    .size(10.0)
                    .weight(700)
                    .letter_spacing(1.4)
                    .color(colors.on_surface_variant),
                Modifier::new(),
            );
            Spacer(Modifier::new().width(12.0));
            for (i, preset) in THEME_PRESETS.iter().enumerate() {
                let selected = i == current;
                let seed = preset.seed;
                Clickable(
                    Modifier::new().size(30.0, 30.0),
                    move || palette.set(i),
                    move || {
                        Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                            let cx = x + w * 0.5;
                            let cy = y + h * 0.5;
                            if selected {
                                b.stroke_circle(cx, cy, 12.0, colors.on_surface, 2.0);
                            }
                            b.fill_circle(cx, cy, 8.5, seed);
                            if selected {
                                draw_icon(b, IconName::Check, cx - 6.0, cy - 6.0, 12.0, Color::WHITE);
                            }
                        });
                    },
                );
            }
            Spacer(Modifier::new().width(14.0));
            VerticalDivider(Modifier::new().height(26.0));
            Spacer(Modifier::new().width(14.0));
            Text(
                "MODE",
                TextStyle::new()
                    .size(10.0)
                    .weight(700)
                    .letter_spacing(1.4)
                    .color(colors.on_surface_variant),
                Modifier::new(),
            );
            Spacer(Modifier::new().width(8.0));
            mode_segment(IconName::Sun, !dark.get(), move || dark.set(false));
            mode_segment(IconName::Moon, dark.get(), move || dark.set(true));
            Spacer(Modifier::new().width(14.0));
            VerticalDivider(Modifier::new().height(26.0));
            Spacer(Modifier::new().width(14.0));
            FilterChip("Liquid glass", liquid_glass_state(), Modifier::new());
            Spacer(Modifier::new().weight(1.0, true));
            Text(
                format!(
                    "{} · {}",
                    preset_name,
                    if dark.get() { "Dark" } else { "Light" }
                ),
                TextStyle::new()
                    .size(11.5)
                    .weight(600)
                    .color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

/// 明暗模式分段控件里的一格（选中 = primary_container 底）。
fn mode_segment(icon_name: IconName, selected: bool, on_click: impl FnMut() + 'static) {
    let colors = theme_colors();
    let m = if selected {
        Modifier::new()
            .size(44.0, 30.0)
            .background(colors.primary_container, 9.0)
    } else {
        Modifier::new().size(44.0, 30.0).background(colors.surface_container_high, 9.0)
    };
    Clickable(m, on_click, move || {
        icon(
            icon_name,
            17.0,
            if selected {
                colors.on_primary_container
            } else {
                colors.on_surface_variant
            },
            Modifier::new().align(Alignment::Center),
        );
    });
}

fn menu_bar(open_menu: MutableState<i8>) {
    let colors = theme_colors();
    Row(
        Modifier::new()
            .fill_max_width()
            .height(MENU_H)
            .background(colors.surface_container, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            MENU_ANCHORS.with(|a| a.borrow_mut().clear());
            for (i, label) in MENUS.iter().enumerate() {
                let open = open_menu.get() == i as i8;
                let mut m = Modifier::new().width(MENU_W).padding_hv(2.0, 3.0);
                if open {
                    m = m.background(colors.primary_container, 8.0);
                }
                let om = open_menu;
                let id = InkWellR(
                    m,
                    8.0,
                    move || om.set(if open { CLOSED } else { i as i8 }),
                    move || {
                        Text(
                            *label,
                            TextStyle::new()
                                .size(13.0)
                                .weight(if open { 650 } else { 500 })
                                .color(if open {
                                    colors.on_primary_container
                                } else {
                                    colors.on_surface_variant
                                }),
                            Modifier::new().align(Alignment::Center),
                        );
                    },
                );
                MENU_ANCHORS.with(|a| a.borrow_mut().push(id));
            }
        },
    );
}

fn dropdown_overlay(
    open_menu: MutableState<i8>,
    dark: MutableState<bool>,
    page: MutableState<usize>,
    palette: MutableState<usize>,
) {
    let idx = open_menu.get();
    let items: Vec<String> = match idx {
        0 => vec!["New".into(), "Open".into(), "Exit".into()],
        1 => PAGES.iter().map(|p| p.name.to_string()).collect(),
        2 => {
            let mut v: Vec<String> = THEME_PRESETS
                .iter()
                .map(|p| format!("Palette: {}", p.name))
                .collect();
            v.push("Light theme".into());
            v.push("Dark theme".into());
            v.push("Liquid glass".into());
            v
        }
        _ => vec!["About storybook".into()],
    };
    // 锚点 = 当前打开菜单标签的节点（menu_bar 本次组合已记录）；缺失则不画浮层。
    let anchor = MENU_ANCHORS.with(|a| a.borrow().get(idx as usize).copied());
    let Some(anchor) = anchor else { return };
    Clickable(
        Modifier::new().fill_max_size().no_indication(),
        {
            let cl = open_menu.clone();
            move || cl.set(CLOSED)
        },
        move || {
            // 复用库的 menu_panel：钉在标签正下方、按内容自然宽展开、项数超出屏幕底部
            // 自动纵向滚动、靠边自动重定位、悬停高亮与入场动画全部由库统一提供。
            // 点项经 `fire_menu_cb` 触发下方回调（执行动作并关闭）。
            set_menu_callback(MENU_KEY, move |i| {
                run_menu_action(idx, i, dark, page, palette);
                open_menu.set(CLOSED);
            });
            menu_panel_auto_width(MENU_KEY, anchor, &items);
        },
    );
}

fn run_menu_action(
    idx: i8,
    i: usize,
    dark: MutableState<bool>,
    page: MutableState<usize>,
    palette: MutableState<usize>,
) {
    match idx {
        0 => {
            if i == 2 {
                std::process::exit(0);
            }
        }
        1 => {
            if i < PAGES.len() {
                page.set(i);
            } else {
                ToggleLiquidGlass();
            }
        }
        2 => {
            if i < THEME_PRESETS.len() {
                palette.set(i);
            } else {
                match i - THEME_PRESETS.len() {
                    0 => dark.set(false),
                    1 => dark.set(true),
                    _ => {
                        ToggleLiquidGlass();
                    }
                }
            }
        }
        _ => {}
    }
}

fn story_status_bar(page: MutableState<usize>, palette: MutableState<usize>, dark: MutableState<bool>) {
    let idx = page.get();
    let group = page_group(idx);
    let preset = THEME_PRESETS
        .get(palette.get())
        .map(|p| p.name)
        .unwrap_or("Violet");
    StatusBar(
        &format!("{}  ·  {}", group, PAGES[idx].name),
        &format!(
            "{} · {}  ·  {} components  ·  RustCompose",
            preset,
            if dark.get() { "Dark" } else { "Light" },
            PAGES.len()
        ),
        Modifier::new().fill_max_width().height(26.0),
    );
}

// ════════════════════════════ 页面分发 ════════════════════════════

fn build_page_body(page: MutableState<usize>) {
    let idx = page.get();
    build_page_title(idx);
    Spacer(Modifier::new().height(16.0));
    (PAGES[idx].builder)();
}

/// 页头 hero：渐变底 + 大号水印图标 + 分组徽标 / 标题 / 一句话说明。
fn build_page_title(idx: usize) {
    let colors = theme_colors();
    let group = page_group(idx).to_uppercase();
    let title = PAGES[idx].name;
    let blurb = page_blurb(idx);
    let icon_name = PAGES[idx].icon;
    let dark = colors.is_dark();
    let (from, to, fg, sub) = if dark {
        (
            colors.primary_container.mix(colors.surface_container, 0.42),
            colors.surface_container,
            colors.on_surface,
            colors.on_surface_variant,
        )
    } else {
        (
            colors.primary_container,
            colors.secondary_container,
            colors.on_primary_container,
            colors.on_primary_container.mix(colors.on_surface_variant, 0.42),
        )
    };
    let chip_bg = fg.with_alpha(if dark { 0.10 } else { 0.08 });
    Box(
        Modifier::new()
            .fill_max_width()
            .height(150.0)
            .background_gradient(from, to, 20.0)
            .clip(20.0),
        Alignment::TopStart,
        move || {
            let group_inner = group.clone();
            Canvas(Modifier::new().fill_max_size().no_hit(), move |b, x, y, w, h| {
                let cx = x + w - 96.0;
                let cy = y + h * 0.5;
                b.fill_circle(cx, cy, 78.0, fg.with_alpha(0.045));
                b.fill_circle(cx + 30.0, cy - 38.0, 34.0, fg.with_alpha(0.06));
                if w > 360.0 {
                    draw_icon(
                        b,
                        icon_name,
                        x + w - 128.0,
                        cy - 42.0,
                        84.0,
                        fg.with_alpha(0.15),
                    );
                }
            });
            Row(
                Modifier::new()
                    .fill_max_size()
                    .padding_ltrb(26.0, 18.0, 26.0, 18.0),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    let group_row = group_inner.clone();
                    Column(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Start,
                        move || {
                            let group_leaf = group_row.clone();
                            Row(
                                Modifier::new(),
                                Arrangement::Start,
                                CrossAxisAlignment::Center,
                                move || {
                                    let group_text = group_leaf.clone();
                                    Row(
                                        Modifier::new()
                                            .height(22.0)
                                            .padding_hv(9.0, 0.0)
                                            .background(chip_bg, 11.0),
                                        Arrangement::Center,
                                        CrossAxisAlignment::Center,
                                        move || {
                                            Text(
                                                group_text.clone(),
                                                TextStyle::new()
                                                    .size(9.5)
                                                    .weight(700)
                                                    .letter_spacing(1.2)
                                                    .color(fg),
                                                Modifier::new(),
                                            );
                                        },
                                    );
                                    Spacer(Modifier::new().width(10.0));
                                    Text(
                                        format!("{:02} / {}", idx + 1, PAGES.len()),
                                        TextStyle::new()
                                            .size(10.5)
                                            .weight(600)
                                            .color(sub),
                                        Modifier::new(),
                                    );
                                },
                            );
                            Spacer(Modifier::new().height(10.0));
                            Text(
                                title,
                                TextStyle::new().size(27.0).weight(800).color(fg),
                                Modifier::new(),
                            );
                            Spacer(Modifier::new().height(6.0));
                            Text(
                                blurb,
                                TextStyle::new().size(12.5).color(sub),
                                Modifier::new(),
                            );
                        },
                    );
                },
            );
        },
    );
}

// ════════════════════════════ 通用助手 ════════════════════════════

fn section(title: &str) {
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

fn card_section(title: &str, build: impl Fn() + 'static) {
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

fn page_progress_circle() {
    let colors = theme_colors();
    section("ProgressCircle（CircularProgress / Spinner）");
    let v = remember(|| mutableStateOf(0.65_f32));
    let vc = v.clone();
    Row(
        Modifier::new().fill_max_width().height(80.0),
        Arrangement::SpaceEvenly,
        CrossAxisAlignment::Center,
        move || {
            let p = vc.get();
            CircularProgress(Some(p), 56.0, Modifier::new());
            CircularProgress(None, 56.0, Modifier::new());
            Spinner(40.0, Modifier::new());
        },
    );
    Spacer(Modifier::new().height(12.0));
    Text(
        format!("Progress: {:.0}%", v.get() * 100.0),
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
    Slider(v.clone(), Modifier::new().fill_max_width());
}

fn page_searchable_list() {
    let colors = theme_colors();
    section("SearchableList（SearchBar + 过滤 ListItem）");
    let q = remember(|| mutableStateOf(String::new()));
    let all = [
        "Inbox",
        "Sent",
        "Draft",
        "Archive",
        "Spam",
        "Trash",
        "Starred",
        "Snoozed",
        "Important",
        "All Mail",
        "Social",
        "Promotions",
    ];
    SearchBar(q.clone(), "Search…", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    let key = q.read_clone().to_lowercase();
    let list: Vec<String> = all
        .iter()
        .filter(|s| s.to_lowercase().contains(&key))
        .map(|s| s.to_string())
        .collect();
    if list.is_empty() {
        Text(
            "No matches",
            TextStyle::new().color(colors.on_surface_variant),
            Modifier::new(),
        );
    } else {
        let items = list.clone();
        VerticalScroll(
            Modifier::new().fill_max_width().height(220.0).border(
                1.0,
                colors.outline_variant,
                12.0,
            ),
            Arrangement::Start,
            CrossAxisAlignment::Stretch,
            move || {
                for it in &items {
                    ListItem(
                        it,
                        None,
                        Some(IconName::Mail),
                        Some("›"),
                        Modifier::new(),
                        None,
                    );
                }
            },
        );
    }
}

fn page_grid_sheet() {
    let colors = theme_colors();
    section("GridSheet（可编辑数据网格）");
    Text(
        "GridSheet with Conditional Formatting",
        TextStyle::new()
            .size(13.0)
            .weight(600)
            .color(colors.on_surface),
        Modifier::new(),
    );
    Text(
        "Features: conditional formatting (salary > 30k green, < 20k orange, On Leave gray) • sorting • zebra stripes",
        TextStyle::new().size(11.5).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    let columns = vec![
        GridSheetColumn::new("id", "ID", 36.0),
        GridSheetColumn::new("name", "Name", 90.0),
        GridSheetColumn::new("dept", "Department", 110.0),
        GridSheetColumn::new("salary", "Salary", 80.0).with_align(GridSheetTextAlign::Right),
        GridSheetColumn::new("status", "Status", 90.0),
    ];
    let rows: Vec<GridSheetRow> = [
        ("1", "Alice", "Engineering", 38000.0, "Active"),
        ("2", "Bob", "Product", 32000.0, "Active"),
        ("3", "Carol", "Design", 22000.0, "Active"),
        ("4", "David", "Engineering", 45000.0, "Active"),
        ("5", "Eva", "Marketing", 18500.0, "On Leave"),
        ("6", "Frank", "Engineering", 15000.0, "Probation"),
        ("7", "Grace", "HR", 26000.0, "Active"),
        ("8", "Henry", "Finance", 24000.0, "Active"),
        ("9", "Ivy", "Engineering", 31000.0, "Active"),
        ("10", "Jack", "Sales", 17500.0, "Probation"),
        ("11", "Kate", "Product", 29500.0, "Active"),
        ("12", "Liam", "Design", 33500.0, "Active"),
        ("13", "Mia", "HR", 19800.0, "On Leave"),
        ("14", "Noah", "Marketing", 41000.0, "Active"),
    ]
    .iter()
    .map(|(id, name, dept, salary, status)| {
        GridSheetRow::new(
            id,
            vec![
                CellValue::from(*id),
                CellValue::from(*name),
                CellValue::from(*dept),
                CellValue::from(*salary),
                CellValue::from(*status),
            ],
        )
    })
    .collect();
    let rules = vec![
        ConditionalFormatRule::number_greater_than(Some("salary"), 30000.0)
            .with_bg_color(Color::rgba(0.30, 0.69, 0.31, 0.20))
            .with_fg_color(Color::rgba(0.13, 0.45, 0.15, 1.0)),
        ConditionalFormatRule::number_less_than(Some("salary"), 20000.0)
            .with_bg_color(Color::rgba(1.0, 0.66, 0.0, 0.22))
            .with_fg_color(Color::rgba(0.72, 0.42, 0.0, 1.0)),
        ConditionalFormatRule::text_equals(Some("status"), "On Leave")
            .with_bg_color(Color::rgba(0.55, 0.55, 0.55, 0.18))
            .with_fg_color(colors.on_surface_variant),
    ];
    let sel = remember(|| mutableStateOf(None::<(usize, usize)>));
    GridSheet(
        columns,
        rows,
        rules,
        GridSheetConfig::default(),
        sel,
        Modifier::new().fill_max_width().height(320.0),
    );
    Spacer(Modifier::new().height(8.0));
    let footer = match sel.get() {
        Some((r, col)) => format!("Selected cell: ({}, {})", r, col),
        None => "Selected cell: none（点击单元格选择，点击表头排序）".to_string(),
    };
    Text(
        footer,
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

fn page_ribbon() {
    let colors = theme_colors();
    section("Ribbon（TabRow + GroupBox 分组命令）");
    let tab = remember(|| mutableStateOf(0usize));
    let Tabs = ["Home".to_string(), "Insert".to_string(), "View".to_string()];
    TabRow(&Tabs, tab.clone(), Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    let t = tab.get();
    Row(
        Modifier::new()
            .fill_max_width()
            .border(1.0, colors.outline_variant, 8.0)
            .padding(8.0),
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            if t == 0 {
                GroupBox("Clipboard", Modifier::new(), move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            IconButton(IconName::Copy, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Save, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Edit, Modifier::new().size(32.0, 32.0), || {});
                        },
                    );
                });
                Spacer(Modifier::new().width(8.0));
                GroupBox("Font", Modifier::new(), move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            IconButton(IconName::Type, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Brush, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Palette, Modifier::new().size(32.0, 32.0), || {});
                        },
                    );
                });
            } else if t == 1 {
                GroupBox("Tables", Modifier::new(), move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            IconButton(IconName::Table, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Grid, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::List, Modifier::new().size(32.0, 32.0), || {});
                        },
                    );
                });
                Spacer(Modifier::new().width(8.0));
                GroupBox("Media", Modifier::new(), move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            IconButton(IconName::Image, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Music, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Video, Modifier::new().size(32.0, 32.0), || {});
                        },
                    );
                });
            } else {
                GroupBox("Zoom", Modifier::new(), move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            IconButton(IconName::ZoomIn, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::ZoomOut, Modifier::new().size(32.0, 32.0), || {});
                        },
                    );
                });
                Spacer(Modifier::new().width(8.0));
                GroupBox("Layout", Modifier::new(), move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            IconButton(IconName::Layout, Modifier::new().size(32.0, 32.0), || {});
                            IconButton(IconName::Eye, Modifier::new().size(32.0, 32.0), || {});
                        },
                    );
                });
            }
        },
    );
}

fn page_animated_gif() {
    let colors = theme_colors();
    let t = Pulse(2.0);
    section("Animated GIF（模拟帧动画播放器）");
    Box(
        Modifier::new().fill_max_width().height(200.0),
        Alignment::Center,
        move || {
            let colors = theme_colors();
            let t = t;
            Canvas(
                Modifier::new().fill_max_size().clip(12.0),
                move |b, x, y, w, h| {
                    b.fill_rrect(x, y, w, h, 12.0, colors.surface_variant);
                    let bounce = 1.0 - (2.0 * t - 1.0).abs();
                    let by = y + h - 30.0 - bounce * (h - 60.0);
                    b.fill_circle(x + w * 0.5, by, 14.0, colors.primary);
                    let ang = t * std::f32::consts::TAU;
                    let cx = x + 56.0;
                    let cy = y + 56.0;
                    let mut pts: Vec<(f32, f32)> = Vec::new();
                    for i in 0..6 {
                        let a = ang + i as f32 * std::f32::consts::TAU / 6.0;
                        pts.push((cx + a.cos() * 24.0, cy + a.sin() * 24.0));
                    }
                    b.stroke_path(&pts, colors.secondary, 4.0);
                },
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    Text(
        "模拟 GIF：循环帧动画（弹跳小球 + 旋转六边形）",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(4.0));
    Text(
        "（接入 GIF 解码器即可播放真实动图；此处以矢量动画演示播放效果）",
        TextStyle::new().size(11.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
}

// ════════════════════════════ Demos ════════════════════════════

fn page_spreadsheet() {
    let colors = theme_colors();
    section("Quarterly report · Q4 2026");
    Row(
        Modifier::new().fill_max_width().height(104.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            stat_card("REVENUE", "$128,400", "+12.4%", true);
            Spacer(Modifier::new().width(10.0));
            stat_card("ORDERS", "1,284", "+8.1%", true);
            Spacer(Modifier::new().width(10.0));
            stat_card("MARGIN", "42.6%", "-1.2%", false);
        },
    );
    Spacer(Modifier::new().height(12.0));
    Row(
        Modifier::new()
            .fill_max_width()
            .height(34.0)
            .background(colors.surface_container_low, 8.0)
            .border(1.0, colors.outline_variant, 8.0)
            .padding_hv(10.0, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                "fx",
                TextStyle::new()
                    .size(11.0)
                    .weight(750)
                    .italic(true)
                    .color(colors.primary),
                Modifier::new()
                    .padding_hv(7.0, 3.0)
                    .background(colors.primary_container, 6.0),
            );
            Spacer(Modifier::new().width(10.0));
            Text(
                "=SUM(D2:D7)  ·  毛利率 = (收入 − 成本) / 收入  ·  数据每 5 分钟刷新",
                TextStyle::new()
                    .size(11.5)
                    .color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(12.0));
    DataTable(
        &[
            "Item".to_string(),
            "Qty".to_string(),
            "Unit price".to_string(),
            "Total".to_string(),
        ],
        &[
            vec![
                "Mechanical keyboard".to_string(),
                "128".to_string(),
                "$89.00".to_string(),
                "$11,392".to_string(),
            ],
            vec![
                "Studio monitor".to_string(),
                "64".to_string(),
                "$249.00".to_string(),
                "$15,936".to_string(),
            ],
            vec![
                "USB-C dock".to_string(),
                "210".to_string(),
                "$119.00".to_string(),
                "$24,990".to_string(),
            ],
            vec![
                "Webcam Pro".to_string(),
                "96".to_string(),
                "$159.00".to_string(),
                "$15,264".to_string(),
            ],
            vec![
                "Standing desk".to_string(),
                "42".to_string(),
                "$399.00".to_string(),
                "$16,758".to_string(),
            ],
            vec![
                "Total".to_string(),
                "540".to_string(),
                "—".to_string(),
                "$84,340".to_string(),
            ],
        ],
        true,
        true,
        Modifier::new().border(1.0, colors.outline_variant, 10.0),
    );
}

fn stat_card(label: &'static str, value: &'static str, delta: &'static str, positive: bool) {
    let colors = theme_colors();
    let chip_fg = if positive {
        Color::rgb(0.13, 0.53, 0.30)
    } else {
        colors.error
    };
    let chip_bg = chip_fg.with_alpha(0.12);
    Column(
        Modifier::new()
            .weight(1.0, true)
            .padding(14.0)
            .background(colors.surface_container_low, 14.0)
            .border(1.0, colors.outline_variant, 14.0),
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            Text(
                label,
                TextStyle::new()
                    .size(9.5)
                    .weight(700)
                    .letter_spacing(1.2)
                    .color(colors.on_surface_variant),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(6.0));
            Text(
                value,
                TextStyle::new()
                    .size(24.0)
                    .weight(780)
                    .color(colors.on_surface),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(4.0));
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Text(
                        delta,
                        TextStyle::new().size(10.5).weight(700).color(chip_fg),
                        Modifier::new()
                            .padding_hv(6.0, 2.0)
                            .background(chip_bg, 8.0),
                    );
                    Spacer(Modifier::new().width(6.0));
                    Text(
                        "vs last quarter",
                        TextStyle::new()
                            .size(10.0)
                            .color(colors.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        },
    );
}

fn page_chat() {
    let input = remember(|| mutableStateOf(String::new()));
    let rich = remember(|| mutableStateOf(String::new()));
    let msgs = remember(|| {
        mutableStateOf(vec![
            ("Hey! The new story build looks sharp.".to_string(), false, "09:41".to_string()),
            ("Thanks — gradients and the new hero header just landed.".to_string(), true, "09:42".to_string()),
            ("Can you ship the hero transition demo too?".to_string(), false, "09:42".to_string()),
            ("Already in. Replay it from the Material Motion page.".to_string(), true, "09:43".to_string()),
        ])
    });
    let colors = theme_colors();
    section("Chat（list + Bubble + composer）");
    Column(
        Modifier::new()
            .fill_max_width()
            .padding(14.0)
            .background(colors.surface_container_low, 16.0)
            .border(1.0, colors.outline_variant, 16.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Avatar("AL", 38.0, Modifier::new());
                    Spacer(Modifier::new().width(10.0));
                    Column(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Start,
                        move || {
                            Text(
                                "Ada Lovelace",
                                TextStyle::new()
                                    .size(13.5)
                                    .weight(700)
                                    .color(colors.on_surface),
                                Modifier::new(),
                            );
                            Row(
                                Modifier::new(),
                                Arrangement::Start,
                                CrossAxisAlignment::Center,
                                move || {
                                    StatusDot(Color::rgb(0.18, 0.66, 0.38), Modifier::new());
                                    Spacer(Modifier::new().width(5.0));
                                    Text(
                                        "online · design systems",
                                        TextStyle::new()
                                            .size(10.5)
                                            .color(colors.on_surface_variant),
                                        Modifier::new(),
                                    );
                                },
                            );
                        },
                    );
                    Spacer(Modifier::new().weight(1.0, true));
                    IconButton(IconName::Search, Modifier::new(), || {});
                    IconButton(IconName::MoreVert, Modifier::new(), || {});
                },
            );
            Spacer(Modifier::new().height(12.0));
            Divider(Modifier::new());
            Spacer(Modifier::new().height(10.0));
            let current = msgs.read_clone();
            for (text, mine, at) in &current {
                Row(
                    Modifier::new().fill_max_width(),
                    if *mine {
                        Arrangement::End
                    } else {
                        Arrangement::Start
                    },
                    CrossAxisAlignment::Center,
                    {
                        let (text, mine, at) = (text.clone(), *mine, at.clone());
                        move || {
                            if mine {
                                Text(
                                    at.clone(),
                                    TextStyle::new()
                                        .size(9.5)
                                        .color(colors.on_surface_variant),
                                    Modifier::new().padding_hv(0.0, 6.0),
                                );
                                Spacer(Modifier::new().width(6.0));
                            }
                            Bubble(&text, mine, Modifier::new());
                            if !mine {
                                Spacer(Modifier::new().width(6.0));
                                Text(
                                    at.clone(),
                                    TextStyle::new()
                                        .size(9.5)
                                        .color(colors.on_surface_variant),
                                    Modifier::new().padding_hv(0.0, 6.0),
                                );
                            }
                        }
                    },
                );
                Spacer(Modifier::new().height(6.0));
            }
            Spacer(Modifier::new().height(6.0));
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    TextField(
                        input.clone(),
                        "Write a message…",
                        Modifier::new().weight(1.0, true),
                    );
                    Spacer(Modifier::new().width(8.0));
                    let st = msgs;
                    let inp = input;
                    IconButton(IconName::Send, Modifier::new(), move || {
                        let text = inp.read_clone();
                        if text.trim().is_empty() {
                            return;
                        }
                        st.update(|v| v.push((text, true, "now".to_string())));
                        inp.set(String::new());
                    });
                },
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    section("RichTextEdit（多行 · 选区 · 复制/粘贴/剪切）");
    RichTextEdit(
        rich,
        "Multi-line text…  Enter 换行，↑↓ 跨行，Home/End 行首末，Shift+方向键 选区，Ctrl+A/C/V/X 全选/复制/粘贴/剪切",
        Modifier::new().fill_max_width().height(140.0),
    );
}

// ── Animation 演示页（pages_animation 1–4 节）──

/// 立体感小球：投影 + 球体 + 高光。
fn anim_ball(b: &mut DrawBuilder, x: f32, y: f32, r: f32, col: Color) {
    b.fill_circle(x + 1.5, y + 2.0, r, Color::rgba(0.0, 0.0, 0.0, 0.15));
    b.fill_circle(x, y, r, col);
    b.fill_circle(
        x - r * 0.25,
        y - r * 0.3,
        r * 0.35,
        Color::rgba(1.0, 1.0, 1.0, 0.35),
    );
}

fn lerp3(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> Color {
    Color::rgba(
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
        1.0,
    )
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn ease_in_out_back(t: f32) -> f32 {
    let c2 = 1.70158 * 1.525;
    if t < 0.5 {
        ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
    } else {
        ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
    }
}

/// 弹簧仿真解析解（目标 1、初值 0、初速 0）：x'' = k(1-x) - c·x'。
fn spring_value(t: f32, k: f32, c: f32) -> f32 {
    let disc = c * c - 4.0 * k;
    if disc.abs() < 1e-3 {
        // 临界阻尼
        let a = c / 2.0;
        1.0 - (-a * t).exp() * (1.0 + a * t)
    } else if disc > 0.0 {
        // 过阻尼
        let s = disc.sqrt();
        let r1 = (-c + s) / 2.0;
        let r2 = (-c - s) / 2.0;
        1.0 - (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / (r2 - r1)
    } else {
        // 欠阻尼
        let s = (-disc).sqrt();
        1.0 - (-c * t / 2.0).exp() * ((s * t / 2.0).cos() + (c / s) * (s * t / 2.0).sin())
    }
}

/// LCG 伪随机（0..1）。
fn next_rand(seed: &MutableState<u64>) -> f32 {
    let s = seed.get().wrapping_add(1);
    seed.set(s);
    let x = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (x >> 33) as f32 / (u32::MAX as f32)
}

/// 简易动画控制器（`SharedAnimation` 的最小子集）。
///
/// progress 0→1，支持循环 / 暂停 / 恢复 / 重置；时间源用 `Instant`，
/// 暂停期间扣除累计时长，恢复后从原进度继续。
#[derive(Clone, Copy)]
struct AnimCtrl {
    /// 起始时刻（`None` = 未开始 / 已重置）
    start: MutableState<Option<std::time::Instant>>,
    /// 暂停累计时长（秒）
    paused_total: MutableState<f32>,
    /// 暂停起始时刻（`Some` = 处于暂停）
    paused_at: MutableState<Option<std::time::Instant>>,
    /// 初速度（fling 用）
    velocity: MutableState<f32>,
    /// 单轮时长（秒）
    duration: f32,
    /// 是否循环播放
    looping: bool,
}

impl AnimCtrl {
    fn new(duration: f32, looping: bool) -> Self {
        Self {
            start: mutableStateOf(None),
            paused_total: mutableStateOf(0.0),
            paused_at: mutableStateOf(None),
            velocity: mutableStateOf(0.0),
            duration,
            looping,
        }
    }

    /// 从头开始播放。
    fn start(&self) {
        self.start.set(Some(std::time::Instant::now()));
        self.paused_total.set(0.0);
        self.paused_at.set(None);
    }

    /// 以给定初速度重新发射（fling）。
    fn launch(&self, velocity: f32) {
        self.velocity.set(velocity);
        self.start();
    }

    fn pause(&self) {
        if self.paused_at.get().is_none() && self.start.get().is_some() {
            self.paused_at.set(Some(std::time::Instant::now()));
        }
    }

    fn resume(&self) {
        if let Some(p) = self.paused_at.get() {
            self.paused_total
                .set(self.paused_total.get() + p.elapsed().as_secs_f32());
            self.paused_at.set(None);
        }
    }

    /// 回到起点并停止。
    fn reset(&self) {
        self.start.set(None);
        self.paused_total.set(0.0);
        self.paused_at.set(None);
    }

    fn is_paused(&self) -> bool {
        self.paused_at.get().is_some()
    }

    fn velocity(&self) -> f32 {
        self.velocity.get()
    }

    /// 已播放时长（秒，已扣除暂停）。
    fn elapsed(&self) -> f32 {
        let Some(t0) = self.start.get() else {
            return 0.0;
        };
        let mut e = t0.elapsed().as_secs_f32() - self.paused_total.get();
        if let Some(p) = self.paused_at.get() {
            e -= p.elapsed().as_secs_f32();
        }
        e.max(0.0)
    }

    /// 归一化进度 0..1（循环时取小数部分）。
    fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        let t = self.elapsed() / self.duration;
        if self.looping { t.fract() } else { t.min(1.0) }
    }

    /// 是否仍在推进（决定是否继续请求帧）。
    fn active(&self) -> bool {
        if self.start.get().is_none() || self.is_paused() {
            return false;
        }
        self.looping || self.elapsed() < self.duration
    }
}

/// 26 条缓动曲线（`Easing` 枚举 25 条 + `Spring`）。
const EASINGS: [(&str, u8); 26] = [
    ("Linear", 0),
    ("EaseInQuad", 1),
    ("EaseOutQuad", 2),
    ("EaseInOutQuad", 3),
    ("EaseInCubic", 4),
    ("EaseOutCubic", 5),
    ("EaseInOutCubic", 6),
    ("EaseInQuart", 7),
    ("EaseOutQuart", 8),
    ("EaseInOutQuart", 9),
    ("EaseInQuint", 10),
    ("EaseOutQuint", 11),
    ("EaseInOutQuint", 12),
    ("EaseInSine", 13),
    ("EaseOutSine", 14),
    ("EaseInOutSine", 15),
    ("EaseInExpo", 16),
    ("EaseOutExpo", 17),
    ("EaseInOutExpo", 18),
    ("EaseInCirc", 19),
    ("EaseOutCirc", 20),
    ("EaseInOutCirc", 21),
    ("EaseInBack", 22),
    ("EaseOutBack", 23),
    ("EaseInOutBack", 24),
    ("Spring", 25),
];

/// 按 `apply_easing` 实现的 26 条缓动曲线。
fn easing(kind: u8, t: f32) -> f32 {
    match kind {
        0 => t,
        1 => t * t,
        2 => t * (2.0 - t),
        3 => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        4 => t * t * t,
        5 => {
            let t = t - 1.0;
            t * t * t + 1.0
        }
        6 => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let t = t - 1.0;
                4.0 * t * t * t + 1.0
            }
        }
        7 => t * t * t * t,
        8 => {
            let t = t - 1.0;
            1.0 - t * t * t * t
        }
        9 => {
            if t < 0.5 {
                8.0 * t * t * t * t
            } else {
                let t = t - 1.0;
                1.0 - 8.0 * t * t * t * t
            }
        }
        10 => t * t * t * t * t,
        11 => {
            let t = t - 1.0;
            1.0 + t * t * t * t * t
        }
        12 => {
            if t < 0.5 {
                16.0 * t * t * t * t * t
            } else {
                let t = t - 1.0;
                1.0 + 16.0 * t * t * t * t * t
            }
        }
        13 => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        14 => (t * std::f32::consts::FRAC_PI_2).sin(),
        15 => 0.5 * (1.0 - (std::f32::consts::PI * t).cos()),
        16 => {
            if t == 0.0 {
                0.0
            } else {
                2.0f32.powf(10.0 * (t - 1.0))
            }
        }
        17 => {
            if (t - 1.0).abs() < f32::EPSILON {
                1.0
            } else {
                1.0 - 2.0f32.powf(-10.0 * t)
            }
        }
        18 => {
            if t == 0.0 {
                return 0.0;
            }
            if (t - 1.0).abs() < f32::EPSILON {
                return 1.0;
            }
            if t < 0.5 {
                0.5 * 2.0f32.powf(20.0 * t - 10.0)
            } else {
                1.0 - 0.5 * 2.0f32.powf(-20.0 * t + 10.0)
            }
        }
        19 => 1.0 - (1.0 - t * t).sqrt(),
        20 => (1.0 - (t - 1.0) * (t - 1.0)).sqrt(),
        21 => {
            if t < 0.5 {
                0.5 * (1.0 - (1.0 - 4.0 * t * t).sqrt())
            } else {
                0.5 * ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0)
            }
        }
        22 => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        }
        23 => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            let t = t - 1.0;
            1.0 + c3 * t * t * t + c1 * t * t
        }
        24 => {
            let c1 = 1.70158;
            let c2 = c1 * 1.525;
            if t < 0.5 {
                0.5 * ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2))
            } else {
                let t = t - 1.0;
                0.5 * ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t + c2) + 2.0)
            }
        }
        _ => {
            // Spring：过冲后回弹
            let c4 = std::f32::consts::PI / 3.0;
            if t == 0.0 {
                return 0.0;
            }
            if (t - 1.0).abs() < f32::EPSILON {
                return 1.0;
            }
            2.0f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
        }
    }
}

/// `Interval::map`：全局进度 → 区间内局部缓动进度。
fn interval_map(t: f32, begin: f32, end: f32, kind: u8) -> f32 {
    if t <= begin {
        return 0.0;
    }
    if t >= end {
        return 1.0;
    }
    let local = ((t - begin) / (end - begin)).clamp(0.0, 1.0);
    easing(kind, local)
}

/// `TweenSequence::value_at`：多阶段等权路径插值。
fn sequence_value(t: f32, segs: &[(f32, f32, u8)]) -> f32 {
    let n = segs.len();
    if n == 0 {
        return 0.0;
    }
    let mut acc = 0.0;
    for &(from, to, kind) in segs.iter() {
        let end = acc + 1.0 / n as f32;
        if t <= end {
            let local = ((t - acc) / (end - acc)).clamp(0.0, 1.0);
            return from + (to - from) * easing(kind, local);
        }
        acc = end;
    }
    segs[n - 1].1
}

/// `TweenSequence<Color>::value_at`：多阶段等权颜色插值。
fn sequence_color(t: f32, segs: &[(Color, Color)]) -> Color {
    let n = segs.len();
    if n == 0 {
        return Color::rgba(0.0, 0.0, 0.0, 1.0);
    }
    let mut acc = 0.0;
    for &(from, to) in segs.iter() {
        let end = acc + 1.0 / n as f32;
        if t <= end {
            let local = ((t - acc) / (end - acc)).clamp(0.0, 1.0);
            return Color::rgba(
                from.r + (to.r - from.r) * local,
                from.g + (to.g - from.g) * local,
                from.b + (to.b - from.b) * local,
                from.a + (to.a - from.a) * local,
            );
        }
        acc = end;
    }
    segs[n - 1].1
}

/// 构造 `anim_card` 的按钮项。
fn btn(label: &'static str, f: impl Fn() + 'static) -> (&'static str, std::rc::Rc<dyn Fn()>) {
    (label, std::rc::Rc::new(f) as std::rc::Rc<dyn Fn()>)
}

/// 注册「动画控制器 → 每帧调度作用域重组」的帧驱动。
///
/// 只在首次组合时注册（`remember` 保证），控制器活跃期间持续请求帧，
/// 动画结束 / 暂停后自动停止，帧循环得以休眠。
fn register_anim_driver(ctrl: AnimCtrl) {
    let sid = current_scope_id();
    remember(|| {
        if let Some(sid) = sid {
            add_frame_driver(move || {
                if ctrl.active() {
                    request_anim_frames();
                    schedule_scope(sid);
                }
            });
        }
        true
    });
}

/// 演示卡片：画布在上，编号说明与按钮在下（`make_section`）。
fn anim_card(
    caption: &str,
    buttons: Vec<(&'static str, std::rc::Rc<dyn Fn()>)>,
    demo: impl Fn() + 'static,
) {
    let colors = theme_colors();
    let cap = caption.to_string();
    Card(
        Modifier::new()
            .padding(12.0)
            .border(1.0, colors.outline_variant, 12.0),
        move || {
            demo();
            Spacer(Modifier::new().height(10.0));
            Text(
                cap.clone(),
                TextStyle::new().size(12.5).color(colors.on_surface),
                Modifier::new(),
            );
            if !buttons.is_empty() {
                Spacer(Modifier::new().height(8.0));
                let btns = buttons.clone();
                Row(
                    Modifier::new(),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        for (i, (label, on_click)) in btns.iter().enumerate() {
                            if i > 0 {
                                Spacer(Modifier::new().width(8.0));
                            }
                            let l = label.to_string();
                            let f = on_click.clone();
                            Button(
                                Modifier::new(),
                                move || f(),
                                move || {
                                    Text(
                                        l.clone(),
                                        TextStyle::new().size(12.5).color(colors.on_primary),
                                        Modifier::new(),
                                    );
                                },
                            );
                        }
                    },
                );
            }
        },
    );
    Spacer(Modifier::new().height(12.0));
}

fn page_animation() {
    section("Animation");
    Spacer(Modifier::new().height(8.0));

    // ── 1 · Tween — 一个控制器同时驱动位置 + 颜色 + 尺寸（自动循环）──
    // 绘制闭包只读取「组合阶段捕获的值」，绝不在绘制时访问 MutableState（避免 with_rt 重入）。
    {
        let run = remember(|| mutableStateOf(true));
        let t0 = remember(|| mutableStateOf(Some(std::time::Instant::now())));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() && t0.get().is_some() {
                        request_anim_frames();
                        schedule_scope(sid);
                    }
                });
            }
            true
        });
        let (run_b, t0_b) = (run, t0);
        let demo = move || {
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            let t = ease_in_out_cubic((e % 2.5) / 2.5);
            Box(
                Modifier::new().fill_max_width().height(90.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        let bx = x + 20.0 + t * (w - 40.0);
                        let col = lerp3((0.25, 0.55, 1.0), (1.0, 0.35, 0.2), t);
                        let r = 10.0 + 14.0 * t;
                        for i in (1..=3).rev() {
                            let gt = (t - i as f32 * 0.04).max(0.0);
                            let gx = x + 20.0 + gt * (w - 40.0);
                            let gr = r * (1.0 - i as f32 * 0.15);
                            b.fill_circle(
                                gx,
                                y + h / 2.0,
                                gr,
                                col.with_alpha(0.12 - i as f32 * 0.03),
                            );
                        }
                        anim_ball(b, bx, y + h / 2.0, r, col);
                        b.fill_rect(
                            x + 10.0,
                            y + h - 6.0,
                            w - 20.0,
                            3.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        b.fill_rect(x + 10.0, y + h - 6.0, (w - 20.0) * t, 3.0, col);
                    });
                },
            );
        };
        anim_card(
            "1 · Tween — one controller drives position + color + size",
            vec![btn("Start Tween", move || {
                run_b.set(true);
                t0_b.set(Some(std::time::Instant::now()));
            })],
            demo,
        );
    }

    // ── 2 · SpringSimulation — underdamped / critical / overdamped ──
    {
        let run = remember(|| mutableStateOf(false));
        let t0 = remember(|| mutableStateOf(None::<std::time::Instant>));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() {
                        request_anim_frames();
                        if let Some(t0v) = t0.get() {
                            if t0v.elapsed().as_secs_f32() < 6.0 {
                                schedule_scope(sid);
                            } else {
                                run.set(false);
                            }
                        }
                    }
                });
            }
            true
        });
        let (run_b, t0_b) = (run, t0);
        let demo = move || {
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            Box(
                Modifier::new().fill_max_width().height(100.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, _h| {
                        let x0 = x + 24.0;
                        let x1 = x + w - 90.0;
                        let springs: [(&str, f32, (f32, f32, f32), f32); 3] = [
                            ("Under", 22.0, (0.2, 0.75, 0.35), 6.0),
                            ("Critical", 48.0, (0.25, 0.55, 1.0), 22.0),
                            ("Over", 74.0, (0.95, 0.55, 0.15), 55.0),
                        ];
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        for (label, ry, rgb, damping) in springs {
                            b.fill_rect(
                                x0,
                                y + ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.05),
                            );
                            let val = spring_value(e, 120.0, damping);
                            let bx = x0 + val * (x1 - x0);
                            anim_ball(b, bx, y + ry, 7.0, lerp3(rgb, rgb, 0.0));
                            b.draw_text(x1 + 8.0, y + ry + 4.0, label, &gray, 60.0);
                        }
                    });
                },
            );
        };
        anim_card(
            "2 · SpringSimulation — underdamped / critical / overdamped",
            vec![btn("Release Spring", move || {
                run_b.set(true);
                t0_b.set(Some(std::time::Instant::now()));
            })],
            demo,
        );
    }

    // ── 3 · CurvedAnimation — 同一控制器，四种曲线（自动循环）──
    {
        let run = remember(|| mutableStateOf(true));
        let t0 = remember(|| mutableStateOf(Some(std::time::Instant::now())));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() && t0.get().is_some() {
                        request_anim_frames();
                        schedule_scope(sid);
                    }
                });
            }
            true
        });
        let (run_b, t0_b) = (run, t0);
        let demo = move || {
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            let t = (e % 2.0) / 2.0;
            Box(
                Modifier::new().fill_max_width().height(110.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        let row_h = (h - 10.0) / 4.0;
                        let x0 = x + 16.0;
                        let x1 = x + w - 100.0;
                        let curves: [(&str, (f32, f32, f32), u8); 4] = [
                            ("Linear", (0.6, 0.6, 0.6), 0),
                            ("EaseIn", (0.9, 0.3, 0.3), 1),
                            ("EaseOut", (0.3, 0.6, 0.9), 2),
                            ("EaseInOutBack", (0.85, 0.4, 0.9), 3),
                        ];
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        for (i, (label, rgb, kind)) in curves.into_iter().enumerate() {
                            let ry = y + 12.0 + row_h * i as f32 + row_h / 2.0;
                            let prog = match kind {
                                1 => t * t * t,
                                2 => 1.0 - (1.0 - t).powi(3),
                                3 => ease_in_out_back(t),
                                _ => t,
                            };
                            let bx = x0 + prog * (x1 - x0);
                            b.fill_rect(
                                x0,
                                ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.04),
                            );
                            anim_ball(b, bx, ry, 7.0, lerp3(rgb, rgb, 0.0));
                            b.draw_text(x1 + 8.0, ry + 4.0, label, &gray, 90.0);
                        }
                    });
                },
            );
        };
        anim_card(
            "3 · CurvedAnimation — same controller, four different curves",
            vec![btn("Start Curves", move || {
                run_b.set(true);
                t0_b.set(Some(std::time::Instant::now()));
            })],
            demo,
        );
    }

    // ── 4 · AnimatedValue — 隐式动画（随机新目标，0.5s 缓动过去）──
    {
        type BallState = (f32, f32, f32, (f32, f32, f32));
        let cur: MutableState<BallState> =
            remember(|| mutableStateOf((0.2f32, 30.0f32, 12.0f32, (0.2f32, 0.6f32, 1.0f32))));
        let tgt: MutableState<BallState> =
            remember(|| mutableStateOf((0.2f32, 30.0f32, 12.0f32, (0.2f32, 0.6f32, 1.0f32))));
        let t0 = remember(|| mutableStateOf(None::<std::time::Instant>));
        let run = remember(|| mutableStateOf(false));
        let seed = remember(|| mutableStateOf(1u64));
        let sid = current_scope_id();
        remember(|| {
            if let Some(sid) = sid {
                let (run, t0) = (run, t0);
                add_frame_driver(move || {
                    if run.get() {
                        request_anim_frames();
                        if let Some(t0v) = t0.get() {
                            if t0v.elapsed().as_secs_f32() < 0.7 {
                                schedule_scope(sid);
                            } else {
                                run.set(false);
                            }
                        }
                    }
                });
            }
            true
        });
        let (cur_b, tgt_b, t0_b, run_b, seed_b) = (cur, tgt, t0, run, seed);
        let demo = move || {
            let cur_v = cur_b.get();
            let tgt_v = tgt_b.get();
            let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.6);
            let te = ease_in_out_cubic((e / 0.5).min(1.0));
            Box(
                Modifier::new().fill_max_width().height(80.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        let (fx, cy, cs, cc) = cur_v;
                        let (tfx, ty, ts, tc) = tgt_v;
                        let bx = x + 20.0 + (fx + (tfx - fx) * te) * (w - 40.0);
                        let by = y + (cy + (ty - cy) * te).min(h - 20.0);
                        let r = cs + (ts - cs) * te;
                        let col = lerp3(cc, tc, te);
                        b.fill_rect(bx, y + 4.0, 1.0, h - 8.0, Color::rgba(0.0, 0.0, 0.0, 0.08));
                        b.fill_rect(
                            x + 10.0,
                            by,
                            w - 20.0,
                            1.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.08),
                        );
                        anim_ball(b, bx, by, r, col);
                    });
                },
            );
        };
        anim_card(
            "4 · AnimatedValue — implicit animation (click to randomize)",
            vec![btn("Randomize Target", move || {
                let e = t0_b.get().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.6);
                let te = ease_in_out_cubic((e / 0.5).min(1.0));
                let (fx, cy, cs, cc) = cur_b.get();
                let (tfx, ty, ts, tc) = tgt_b.get();
                cur_b.set((
                    fx + (tfx - fx) * te,
                    cy + (ty - cy) * te,
                    cs + (ts - cs) * te,
                    (
                        cc.0 + (tc.0 - cc.0) * te,
                        cc.1 + (tc.1 - cc.1) * te,
                        cc.2 + (tc.2 - cc.2) * te,
                    ),
                ));
                tgt_b.set((
                    0.05 + next_rand(&seed_b) * 0.9,
                    15.0 + next_rand(&seed_b) * 35.0,
                    10.0 + next_rand(&seed_b) * 14.0,
                    (
                        0.15 + next_rand(&seed_b) * 0.85,
                        0.15 + next_rand(&seed_b) * 0.85,
                        0.15 + next_rand(&seed_b) * 0.85,
                    ),
                ));
                t0_b.set(Some(std::time::Instant::now()));
                run_b.set(true);
            })],
            demo,
        );
    }

    // ── 5 · Animation::fling — inertial motion ──
    {
        let ctrl = remember(|| AnimCtrl::new(3.0, false));
        register_anim_driver(ctrl);
        let demo = move || {
            let elapsed = ctrl.elapsed();
            let vel = ctrl.velocity();
            Box(
                Modifier::new().fill_max_width().height(60.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        // 测量探测阶段 w/h 为 0，需早退（`if w < 10.0 { return; }`）
                        if w < 10.0 {
                            return;
                        }
                        let track_y = y + h / 2.0;
                        b.fill_rrect(
                            x + 15.0,
                            track_y - 2.0,
                            w - 30.0,
                            4.0,
                            2.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        // 摩擦衰减：位移 = v·(1−e^(−friction·t)) / friction
                        let friction = 1.8f32;
                        let decay = (-friction * elapsed).exp();
                        let displacement = vel * (1.0 - decay) / friction;
                        let lo = x + 15.0;
                        let hi = (x + w - 15.0).max(lo);
                        let pos = (x + 20.0 + displacement * w * 0.3).clamp(lo, hi);
                        anim_ball(b, pos, track_y, 10.0, Color::rgba(0.55, 0.3, 0.9, 1.0));
                    });
                },
            );
        };
        anim_card(
            "5 · Animation::fling — inertial motion",
            vec![
                btn("Slow Fling", {
                    let c = ctrl;
                    move || c.launch(0.5)
                }),
                btn("Medium Fling", {
                    let c = ctrl;
                    move || c.launch(1.5)
                }),
                btn("Fast Fling", {
                    let c = ctrl;
                    move || c.launch(3.0)
                }),
            ],
            demo,
        );
    }

    // ── 6 · TweenSequence — multi-stage path (right→down←left↑up) ──
    {
        let ctrl = remember(|| AnimCtrl::new(3.0, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let t = ctrl.progress();
            let segs_x: [(f32, f32, u8); 4] = [
                (40.0, 260.0, 5),
                (260.0, 260.0, 0),
                (260.0, 40.0, 5),
                (40.0, 40.0, 0),
            ];
            let segs_y: [(f32, f32, u8); 4] = [
                (20.0, 20.0, 0),
                (20.0, 55.0, 5),
                (55.0, 55.0, 0),
                (55.0, 20.0, 5),
            ];
            let cols: [(Color, Color); 4] = [
                (
                    Color::rgba(0.25, 0.55, 1.0, 1.0),
                    Color::rgba(0.2, 0.8, 0.4, 1.0),
                ),
                (
                    Color::rgba(0.2, 0.8, 0.4, 1.0),
                    Color::rgba(1.0, 0.6, 0.1, 1.0),
                ),
                (
                    Color::rgba(1.0, 0.6, 0.1, 1.0),
                    Color::rgba(0.7, 0.3, 0.9, 1.0),
                ),
                (
                    Color::rgba(0.7, 0.3, 0.9, 1.0),
                    Color::rgba(0.25, 0.55, 1.0, 1.0),
                ),
            ];
            let bx = sequence_value(t, &segs_x);
            let by = sequence_value(t, &segs_y);
            let col = sequence_color(t, &cols);
            let seg = ((t * 4.0) as i32).clamp(0, 3) as usize;
            let labels = ["Move Right", "Move Down", "Move Left", "Move Up"];
            Box(
                Modifier::new().fill_max_width().height(80.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        b.stroke_rrect(
                            x + 39.5,
                            y + 19.5,
                            221.0,
                            36.0,
                            0.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                            1.0,
                        );
                        b.fill_circle(x + bx, y + by, 14.0, col.with_alpha(0.15));
                        anim_ball(b, x + bx, y + by, 9.0, col);
                        b.draw_text(x + w - 100.0, y + h / 2.0 + 4.0, labels[seg], &gray, 90.0);
                        b.fill_rrect(
                            x + 10.0,
                            y + h - 6.0,
                            w - 20.0,
                            3.0,
                            1.5,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        b.fill_rrect(x + 10.0, y + h - 6.0, (w - 20.0) * t, 3.0, 1.5, col);
                    });
                },
            );
        };
        anim_card(
            "6 · TweenSequence — multi-stage path (right→down←left↑up)",
            vec![btn("Start Sequence", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 7 · Interval + ReverseAnimation — staggered properties ──
    {
        let ctrl = remember(|| AnimCtrl::new(2.5, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let raw = ctrl.progress();
            let opacity = interval_map(raw, 0.0, 0.3, 5);
            let scale = 0.5 + 0.5 * interval_map(raw, 0.2, 0.7, 24);
            let slide = interval_map(raw, 0.5, 1.0, 6);
            // ReverseAnimation：进度反转 1.0 → 0.0
            let rev = 1.0 - raw;
            Box(
                Modifier::new().fill_max_width().height(80.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(9.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let gray2 = TextStyle::new()
                            .size(11.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let x0_bar = x + 30.0;
                        let x1_bar = x + w - 80.0;
                        let span = x1_bar - x0_bar;
                        let intervals: [(f32, f32, Color, &str); 3] = [
                            (0.0, 0.3, Color::rgba(0.2, 0.7, 0.4, 1.0), "Opacity"),
                            (0.2, 0.7, Color::rgba(0.9, 0.4, 0.2, 1.0), "Scale"),
                            (0.5, 1.0, Color::rgba(0.3, 0.5, 0.95, 1.0), "Slide"),
                        ];
                        for (i, (begin, end, c_col, label)) in intervals.iter().enumerate() {
                            let bar_y = y + 8.0 + 14.0 * i as f32;
                            b.fill_rrect(
                                x0_bar,
                                bar_y,
                                span,
                                6.0,
                                3.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.04),
                            );
                            b.fill_rrect(
                                x0_bar + begin * span,
                                bar_y,
                                (end - begin) * span,
                                6.0,
                                3.0,
                                c_col.with_alpha(0.3),
                            );
                            b.fill_circle(x0_bar + raw * span, bar_y + 3.0, 4.0, *c_col);
                            b.draw_text(x1_bar + 6.0, bar_y + 5.0, label, &gray, 60.0);
                        }
                        let ball_y = y + h - 18.0;
                        let slide_x = x + 30.0 + 200.0 * slide;
                        b.fill_circle(
                            slide_x,
                            ball_y,
                            12.0 * scale,
                            Color::rgba(0.25, 0.55, 1.0, opacity * 0.2),
                        );
                        b.fill_circle(
                            slide_x,
                            ball_y,
                            8.0 * scale,
                            Color::rgba(0.25, 0.55, 1.0, opacity),
                        );
                        b.draw_text(
                            x + w - 60.0,
                            ball_y + 4.0,
                            &format!("{:.2}", rev),
                            &gray2,
                            60.0,
                        );
                    });
                },
            );
        };
        anim_card(
            "7 · Interval + ReverseAnimation — staggered properties from one controller",
            vec![btn("Start Staggered", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 8 · Spring Presets — Default / Stiff / Bouncy / Smooth / Gentle ──
    {
        // (名称, 刚度 k, 阻尼 颜色) —— 质量均为 1（`SpringPreset`）
        const PRESETS: [(&str, f32, f32, (f32, f32, f32)); 5] = [
            ("Default", 100.0, 10.0, (0.25, 0.55, 1.0)),
            ("Stiff", 180.0, 26.0, (0.9, 0.35, 0.2)),
            ("Bouncy", 120.0, 8.0, (0.85, 0.4, 0.9)),
            ("Smooth", 150.0, 24.5, (0.2, 0.75, 0.4)),
            ("Gentle", 80.0, 14.0, (1.0, 0.65, 0.1)),
        ];
        let ctrl = remember(|| AnimCtrl::new(4.0, false));
        register_anim_driver(ctrl);
        let demo = move || {
            let elapsed = ctrl.elapsed();
            let running = elapsed < 4.0;
            Box(
                Modifier::new().fill_max_width().height(110.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let x0 = x + 24.0;
                        let x1 = x + w - 90.0;
                        let row_h = (h - 10.0) / PRESETS.len() as f32;
                        for (i, (name, k, damp, rgb)) in PRESETS.iter().enumerate() {
                            let ry = y + 12.0 + row_h * i as f32 + row_h / 2.0;
                            let val = if running {
                                spring_value(elapsed, *k, *damp)
                            } else {
                                0.0
                            };
                            let bx = x0 + val * (x1 - x0);
                            b.fill_rect(
                                x0,
                                ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.04),
                            );
                            anim_ball(b, bx, ry, 6.0, Color::rgba(rgb.0, rgb.1, rgb.2, 1.0));
                            b.draw_text(x1 + 8.0, ry + 4.0, name, &gray, 60.0);
                        }
                    });
                },
            );
        };
        anim_card(
            "8 · Spring Presets — Default / Stiff / Bouncy / Smooth / Gentle",
            vec![btn("Release Springs", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 9 · Easing Showcase — all 25 easing curves ──
    {
        let ctrl = remember(|| AnimCtrl::new(3.0, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let raw = ctrl.progress();
            Box(
                Modifier::new().fill_max_width().height(12.0 * 26.0 + 20.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, _h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(8.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let x0 = x + 16.0;
                        let x1 = x + w - 130.0;
                        for (i, (label, kind)) in EASINGS.iter().enumerate() {
                            let ry = y + 10.0 + 12.0 * i as f32 + 6.0;
                            // 轨道
                            b.fill_rect(
                                x0,
                                ry - 0.5,
                                x1 - x0,
                                1.0,
                                Color::rgba(0.0, 0.0, 0.0, 0.03),
                            );
                            b.draw_text(x1 + 6.0, ry + 3.0, label, &gray, 120.0);
                            // 曲线圆点
                            let prog = easing(*kind, raw);
                            b.fill_circle(
                                x0 + prog * (x1 - x0),
                                ry,
                                3.5,
                                Color::rgba(0.3, 0.6, 0.9, 1.0),
                            );
                        }
                    });
                },
            );
        };
        anim_card(
            "9 · Easing Showcase — all 25 easing curves",
            vec![btn("Start Easing", {
                let c = ctrl;
                move || c.start()
            })],
            demo,
        );
    }

    // ── 10 · Animation Control — pause / resume / reset ──
    {
        let ctrl = remember(|| AnimCtrl::new(2.0, true));
        register_anim_driver(ctrl);
        let demo = move || {
            let t = easing(6, ctrl.progress());
            let running = ctrl.active();
            Box(
                Modifier::new().fill_max_width().height(70.0),
                Alignment::TopStart,
                move || {
                    Canvas(Modifier::new().fill_max_size(), move |b, x, y, w, h| {
                        if w < 10.0 {
                            return;
                        }
                        let gray = TextStyle::new()
                            .size(10.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0));
                        let bx = x + 20.0 + t * (w - 40.0);
                        let by = y + h / 2.0;
                        b.fill_rrect(
                            x + 20.0,
                            by - 2.0,
                            w - 40.0,
                            4.0,
                            2.0,
                            Color::rgba(0.0, 0.0, 0.0, 0.06),
                        );
                        anim_ball(b, bx, by, 12.0, Color::rgba(0.2, 0.7, 0.4, 1.0));
                        b.draw_text(
                            x + w - 70.0,
                            by + 4.0,
                            if running { "Running" } else { "Stopped" },
                            &gray,
                            60.0,
                        );
                    });
                },
            );
        };
        anim_card(
            "10 · Animation Control — pause / resume / reset",
            vec![
                btn("Pause", {
                    let c = ctrl;
                    move || c.pause()
                }),
                btn("Resume", {
                    let c = ctrl;
                    move || c.resume()
                }),
                btn("Reset", {
                    let c = ctrl;
                    move || c.reset()
                }),
            ],
            demo,
        );
    }
}

/// 转场面板的容器色 / 前景色（四种内容轮换）。
fn motion_tone(colors: ColorScheme, i: usize) -> (Color, Color) {
    match i % 4 {
        0 => (colors.primary_container, colors.on_primary_container),
        1 => (colors.secondary_container, colors.on_secondary_container),
        2 => (colors.tertiary_container, colors.on_tertiary_container),
        _ => (colors.surface_variant, colors.on_surface_variant),
    }
}

/// SharedAxis / FadeScale 的演示内容：一整块「列表项」卡片。
fn motion_panel(
    bg: Color,
    fg: Color,
    glyph: IconName,
    title: &'static str,
    sub: &'static str,
    idx: usize,
) {
    Box(
        Modifier::new()
            .fill_max_width()
            .height(92.0)
            .background(bg, 14.0)
            .clip(14.0),
        Alignment::Center,
        move || {
            Row(
                Modifier::new().fill_max_width().padding(16.0),
                Arrangement::SpaceBetween,
                CrossAxisAlignment::Center,
                move || {
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            Box(
                                Modifier::new()
                                    .size(38.0, 38.0)
                                    .background(fg.mix(bg, 0.82), 19.0)
                                    .clip(19.0),
                                Alignment::Center,
                                move || {
                                    Canvas(Modifier::new().size(21.0, 21.0).no_hit(), move |b, x, y, w, h| {
                                        draw_icon(b, glyph, x, y, w.min(h), fg);
                                    });
                                },
                            );
                            Spacer(Modifier::new().width(12.0));
                            Column(
                                Modifier::new(),
                                Arrangement::Start,
                                CrossAxisAlignment::Start,
                                move || {
                                    Text(
                                        format!("{} · {:02}", title, idx + 1),
                                        TextStyle::new().size(14.5).weight(700).color(fg),
                                        Modifier::new(),
                                    );
                                    Spacer(Modifier::new().height(3.0));
                                    Text(
                                        sub,
                                        TextStyle::new()
                                            .size(12.0)
                                            .color(fg.mix(bg, 0.26)),
                                        Modifier::new(),
                                    );
                                },
                            );
                        },
                    );
                    Canvas(Modifier::new().size(18.0, 18.0).no_hit(), move |b, x, y, w, h| {
                        draw_icon(b, IconName::ChevronRight, x, y, w.min(h), fg);
                    });
                },
            );
        },
    );
}

fn page_material_motion() {
    let colors = theme_colors();
    section("Material Motion（SharedAxis / FadeScale）");
    Text(
        "M3 三条转场曲线：X 轴滑动（层级导航）、Y 轴滑动（分组切换）、淡入缩放（同级内容切换）"
            .to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    // ── SharedAxis · X ──
    let hkey = remember(|| mutableStateOf(0u64));
    let hfwd = remember(|| mutableStateOf(true));
    {
        let (k, f) = (hkey, hfwd);
        let prev = btn("Previous", move || {
            f.set(false);
            k.update(|n| *n += 1);
        });
        let (k, f) = (hkey, hfwd);
        let next = btn("Next", move || {
            f.set(true);
            k.update(|n| *n += 1);
        });
        anim_card(
            "SharedAxis · X — 列表进入详情时页面左右滑动，返回时反向",
            vec![prev, next],
            move || {
                let k = hkey.get();
                let fwd = hfwd.get();
                SharedAxis(k, true, fwd, Modifier::new().fill_max_width(), move || {
                    let (bg, fg) = motion_tone(colors, k as usize);
                    let (glyph, title, sub) = match k as usize % 4 {
                        0 => (IconName::Mail, "Inbox", "12 封未读邮件"),
                        1 => (IconName::File, "Drafts", "3 份草稿待发送"),
                        2 => (IconName::Star, "Starred", "你收藏的会话"),
                        _ => (IconName::Settings, "Settings", "工作区偏好设置"),
                    };
                    motion_panel(bg, fg, glyph, title, sub, k as usize % 4);
                });
            },
        );
    }

    // ── SharedAxis · Y ──
    let vkey = remember(|| mutableStateOf(0u64));
    let vfwd = remember(|| mutableStateOf(true));
    {
        let (k, f) = (vkey, vfwd);
        let prev = btn("Previous", move || {
            f.set(false);
            k.update(|n| *n += 1);
        });
        let (k, f) = (vkey, vfwd);
        let next = btn("Next", move || {
            f.set(true);
            k.update(|n| *n += 1);
        });
        anim_card(
            "SharedAxis · Y — 同一层级内切换分组（Inbox ↔ Drafts ↔ Starred）",
            vec![prev, next],
            move || {
                let k = vkey.get();
                let fwd = vfwd.get();
                SharedAxis(k, false, fwd, Modifier::new().fill_max_width(), move || {
                    let (bg, fg) = motion_tone(colors, (k as usize + 2) % 4);
                    motion_panel(bg, fg, IconName::Grid, "Group", "切换分组时的纵向位移", k as usize);
                });
            },
        );
    }

    // ── FadeScale ──
    let fkey = remember(|| mutableStateOf(0u64));
    {
        let k = fkey;
        let replay = btn("Replay", move || k.update(|n| *n += 1));
        anim_card(
            "FadeScale — 同级内容切换：透明度 + 缩放同时插值",
            vec![replay],
            move || {
                let k = fkey.get();
                FadeScale(k, Modifier::new().fill_max_width(), move || {
                    let (bg, fg) = motion_tone(colors, (k as usize + 1) % 4);
                    motion_panel(bg, fg, IconName::Chart, "Insight", "每周活跃度 · 同比 +18%", k as usize);
                });
            },
        );
    }
}

fn page_pull_refresh() {
    let colors = theme_colors();
    let refreshing = remember(|| mutableStateOf(false));
    let stamp = remember(|| mutableStateOf(0usize));
    let tick = remember(|| mutableStateOf(0u32));
    // 模拟 ~0.5s 网络延迟后完成刷新。驱动只在组合时注册一次（回调里注册会
    // 随刷新次数累积泄漏）；结束写 false 仅在 true→false 时刻发生一次。
    remember(|| {
        let (tick, refreshing) = (tick, refreshing);
        add_frame_driver(move || {
            if !refreshing.get() {
                return;
            }
            request_anim_frames();
            let n = tick.get() + 1;
            tick.set(n);
            if n >= 30 {
                refreshing.set(false);
            }
        });
        true
    });
    section("Pull To Refresh（下拉刷新）");
    Text(
        "在列表内按住向下拖动：进度弧随拉动增长，越过阈值松手即刷新".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));
    PullToRefresh(
        refreshing,
        {
            let stamp = stamp;
            let tick = tick;
            move || {
                stamp.update(|n| *n += 1);
                tick.set(0);
            }
        },
        Modifier::new()
            .fill_max_width()
            .height(240.0)
            .border(1.0, colors.outline_variant, 12.0),
        move || {
            let stamp = stamp;
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    for i in 0..12 {
                        Card(Modifier::new().padding(12.0), move || {
                            Row(
                                Modifier::new().fill_max_width(),
                                Arrangement::SpaceBetween,
                                CrossAxisAlignment::Center,
                                move || {
                                    Text(
                                        format!("Item {}", i + 1),
                                        TextStyle::new(),
                                        Modifier::new(),
                                    );
                                    Text(
                                        format!("刷新 #{}", stamp.get()),
                                        TextStyle::new()
                                            .size(12.0)
                                            .color(colors.on_surface_variant),
                                        Modifier::new(),
                                    );
                                },
                            );
                        });
                        Spacer(Modifier::new().height(8.0));
                    }
                },
            );
        },
    );
}

fn mini_stat(label: &'static str, value: &'static str, colors: ColorScheme) {
    Column(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                value,
                TextStyle::new().size(17.0).weight(700).color(colors.primary),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(2.0));
            Text(
                label,
                TextStyle::new().size(11.0).color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

fn page_hero() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    // 单一进度 t 驱动头图高度 / 头像尺寸 / 详情透明度，展开收拢天然同步。
    let t = animate_float_as_state(
        if open.get() { 1.0 } else { 0.0 },
        Tween::new(0.42).with_easing(ease_out_cubic),
    );
    section("Hero Animation（共享元素展开）");
    Text(
        "点击卡片：头图、头像与详情由同一个进度 t 插值，展开收拢保持同步".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    Clickable(
        Modifier::new().fill_max_width(),
        move || open.set(!open.get()),
        move || {
            Card(Modifier::new().fill_max_width(), move || {
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(64.0 + t * 66.0)
                        .background_gradient(colors.primary, colors.tertiary, 14.0)
                        .clip(14.0),
                    Alignment::Center,
                    move || {
                        Canvas(Modifier::new().fill_max_size().no_hit(), move |b, x, y, w, h| {
                            b.fill_circle(
                                x + w * 0.84,
                                y + h * 0.30,
                                44.0,
                                Color::rgba(1.0, 1.0, 1.0, 0.10),
                            );
                            b.fill_circle(
                                x + w * 0.66,
                                y + h * 0.82,
                                26.0,
                                Color::rgba(1.0, 1.0, 1.0, 0.08),
                            );
                            draw_icon(
                                b,
                                IconName::Star,
                                x + w - 86.0,
                                y + h * 0.5 - 18.0,
                                36.0,
                                Color::rgba(1.0, 1.0, 1.0, 0.30),
                            );
                        });
                    },
                );
                Spacer(Modifier::new().height(12.0));
                Row(
                    Modifier::new().fill_max_width(),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        Avatar("AL", 42.0 + t * 10.0, Modifier::new());
                        Spacer(Modifier::new().width(12.0));
                        Column(
                            Modifier::new(),
                            Arrangement::Start,
                            CrossAxisAlignment::Start,
                            move || {
                                Text(
                                    "Ada Lovelace",
                                    TextStyle::new()
                                        .size(16.0)
                                        .weight(700)
                                        .color(colors.on_surface),
                                    Modifier::new(),
                                );
                                Spacer(Modifier::new().height(2.0));
                                Text(
                                    "Staff Engineer · Design Systems",
                                    TextStyle::new()
                                        .size(12.0)
                                        .color(colors.on_surface_variant),
                                    Modifier::new(),
                                );
                            },
                        );
                    },
                );
                // 高度与透明度都随 t 增长：t=0 时整块被裁掉，不占位。
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(t * 84.0)
                        .clip(0.0),
                    Alignment::TopStart,
                    move || {
                        Column(
                            Modifier::new().fill_max_width(),
                            Arrangement::Start,
                            CrossAxisAlignment::Stretch,
                            move || {
                                Spacer(Modifier::new().height(12.0));
                                Divider(Modifier::new());
                                Spacer(Modifier::new().height(10.0));
                                Row(
                                    Modifier::new().fill_max_width(),
                                    Arrangement::SpaceEvenly,
                                    CrossAxisAlignment::Center,
                                    move || {
                                        mini_stat("Projects", "24", colors);
                                        mini_stat("Followers", "1.2k", colors);
                                        mini_stat("Stars", "318", colors);
                                    },
                                );
                            },
                        );
                    },
                );
            });
        },
    );
    Spacer(Modifier::new().height(10.0));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Button(
                Modifier::new(),
                move || open.set(!open.get()),
                move || {
                    Text(
                        if open.get() { "Collapse" } else { "Expand" },
                        TextStyle::new().color(colors.on_primary),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().width(10.0));
            Text(
                format!("progress t = {:.2}", t),
                TextStyle::new().size(12.0).color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

fn fade_panel(
    bg1: Color,
    bg2: Color,
    fg: Color,
    glyph: IconName,
    title: &'static str,
    sub: &'static str,
) {
    Box(
        Modifier::new()
            .fill_max_width()
            .height(148.0)
            .background_gradient(bg1, bg2, 18.0)
            .clip(18.0),
        Alignment::TopStart,
        move || {
            Row(
                Modifier::new().fill_max_size().padding(22.0),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Box(
                        Modifier::new()
                            .size(54.0, 54.0)
                            .background(Color::rgba(1.0, 1.0, 1.0, 0.18), 27.0)
                            .clip(27.0),
                        Alignment::Center,
                        move || {
                            Canvas(Modifier::new().size(28.0, 28.0).no_hit(), move |b, x, y, w, h| {
                                draw_icon(b, glyph, x, y, w.min(h), fg);
                            });
                        },
                    );
                    Spacer(Modifier::new().width(16.0));
                    Column(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Start,
                        move || {
                            Text(
                                title,
                                TextStyle::new().size(20.0).weight(750).color(fg),
                                Modifier::new(),
                            );
                            Spacer(Modifier::new().height(5.0));
                            Text(
                                sub,
                                TextStyle::new().size(12.5).color(fg.mix(bg2, 0.22)),
                                Modifier::new(),
                            );
                        },
                    );
                },
            );
        },
    );
}

fn page_cross_fade() {
    let colors = theme_colors();
    let sel = remember(|| mutableStateOf(0usize));
    section("Cross Fade（内容渐隐渐显）");
    Text(
        "切换选项：旧内容淡出、新内容淡入，同一容器内交叉过渡".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    Crossfade(
        sel.get() as u64,
        Modifier::new().fill_max_width(),
        move || {
            let i = sel.get() % 3;
            let (bg1, bg2, fg, glyph, title, sub) = match i {
                0 => (
                    colors.primary_container,
                    colors.secondary_container,
                    colors.on_primary_container,
                    IconName::Play,
                    "Morning focus",
                    "深度工作 · 25 分钟专注 + 5 分钟休息",
                ),
                1 => (
                    colors.secondary_container,
                    colors.tertiary_container,
                    colors.on_secondary_container,
                    IconName::Star,
                    "Evening review",
                    "回顾今天完成的 6 个任务，整理明日清单",
                ),
                _ => (
                    colors.tertiary_container,
                    colors.primary_container,
                    colors.on_tertiary_container,
                    IconName::Chart,
                    "Weekly retro",
                    "团队 12 人 · 交付 38 个变更 · 0 次回滚",
                ),
            };
            fade_panel(bg1, bg2, fg, glyph, title, sub);
        },
    );
    Spacer(Modifier::new().height(14.0));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for (i, label) in ["Focus", "Review", "Retro"].iter().enumerate() {
                if i > 0 {
                    Spacer(Modifier::new().width(8.0));
                }
                let s = sel;
                let selected = sel.get() == i;
                Chip(label.to_string(), None, selected, Modifier::new(), move || s.set(i));
            }
        },
    );
}

fn page_custom_widget() {
    let colors = theme_colors();
    let v = remember(|| mutableStateOf(0.62f32));
    let shown = animate_float_as_state(v.get(), Spring::bouncy());
    section("Custom Widget（Canvas 自绘仪表盘）");
    Text(
        "组合基础图元（圆弧 / 刻度 / 指针）自绘控件；滑块驱动 Spring 弹性跟随".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    let gauge = if shown < 0.5 {
        colors.primary
    } else if shown < 0.8 {
        colors.tertiary
    } else {
        colors.error
    };
    let label = format!("{:.0}%", shown * 100.0);
    let value_style = TextStyle::new().size(30.0).weight(800).color(gauge);
    let tick_style = TextStyle::new().size(10.0).color(colors.on_surface_variant);
    Card(Modifier::new().fill_max_width(), move || {
        let label = label.clone();
        let value_style = value_style.clone();
        let tick_style = tick_style.clone();
        Canvas(
            Modifier::new().fill_max_width().height(260.0).no_hit(),
            move |b, x, y, w, h| {
                let cx = x + w * 0.5;
                let cy = y + h * 0.60;
                let r = (w * 0.36).min(h * 0.52);
                let a0 = std::f32::consts::PI * 0.78;
                let span = std::f32::consts::PI * 1.44;

                let mut track = Vec::with_capacity(49);
                for i in 0..=48 {
                    let a = a0 + span * i as f32 / 48.0;
                    track.push((cx + r * a.cos(), cy + r * a.sin()));
                }
                b.stroke_path(&track, colors.surface_variant, 13.0);

                let steps = ((48.0 * shown).round() as usize).max(1);
                let mut arc = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let a = a0 + span * shown * i as f32 / steps as f32;
                    arc.push((cx + r * a.cos(), cy + r * a.sin()));
                }
                b.stroke_path(&arc, gauge, 13.0);
                b.fill_circle(cx + r * a0.cos(), cy + r * a0.sin(), 6.5, gauge);

                for i in 0..=10 {
                    let a = a0 + span * i as f32 / 10.0;
                    let (s, c) = (a.sin(), a.cos());
                    b.line(
                        cx + (r + 11.0) * c,
                        cy + (r + 11.0) * s,
                        cx + (r + 18.0) * c,
                        cy + (r + 18.0) * s,
                        colors.outline,
                        2.0,
                    );
                }

                let a = a0 + span * shown;
                b.line(
                    cx,
                    cy,
                    cx + (r - 26.0) * a.cos(),
                    cy + (r - 26.0) * a.sin(),
                    colors.on_surface,
                    3.0,
                );
                b.fill_circle(cx, cy, 7.0, colors.on_surface);
                b.fill_circle(cx, cy, 3.0, colors.surface_container_low);

                // 读数放在轴心下方的开口区，避免与指针重叠。
                let label_w = label.chars().count() as f32 * 8.2;
                b.draw_text(cx - label_w, cy + 16.0, &label, &value_style, 160.0);
                b.draw_text(cx - r + 24.0, cy + 14.0, "0", &tick_style, 40.0);
                b.draw_text(cx + r - 34.0, cy + 14.0, "100", &tick_style, 40.0);
            },
        );
        Spacer(Modifier::new().height(6.0));
        Slider(v, Modifier::new().fill_max_width());
        Spacer(Modifier::new().height(10.0));
        Row(
            Modifier::new(),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            move || {
                for (i, pct) in [0.25f32, 0.5, 0.75, 1.0].iter().enumerate() {
                    if i > 0 {
                        Spacer(Modifier::new().width(8.0));
                    }
                    let p = *pct;
                    OutlinedButton(
                        Modifier::new(),
                        move || v.set(p),
                        move || {
                            Text(
                                format!("{:.0}%", p * 100.0),
                                TextStyle::new().size(12.0).color(colors.primary),
                                Modifier::new(),
                            );
                        },
                    );
                }
            },
        );
    });
}

fn status_pill(text: &'static str, bg: Color, fg: Color) {
    Box(
        Modifier::new().background(bg, 999.0).padding_ltrb(10.0, 4.0, 10.0, 4.0),
        Alignment::Center,
        move || {
            Text(
                text,
                TextStyle::new().size(11.0).weight(600).color(fg),
                Modifier::new(),
            );
        },
    );
}

/// 异步任务的步骤行：圆点（待办 / 进行中 / 完成）+ 名称 + 状态。
fn async_step(label: &'static str, state: u8, colors: ColorScheme) {
    let (dot, text) = match state {
        0 => (colors.outline_variant, colors.on_surface_variant),
        1 => (colors.primary, colors.on_surface),
        _ => (colors.primary, colors.on_surface),
    };
    let status = match state {
        0 => "waiting",
        1 => "running",
        _ => "done",
    };
    Row(
        Modifier::new().fill_max_width().padding_ltrb(0.0, 5.0, 0.0, 5.0),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    Canvas(Modifier::new().size(16.0, 16.0).no_hit(), move |b, x, y, w, h| {
                        let cx = x + w * 0.5;
                        let cy = y + h * 0.5;
                        if state == 2 {
                            b.fill_circle(cx, cy, 7.0, dot);
                            draw_icon(
                                b,
                                IconName::Check,
                                cx - 5.0,
                                cy - 5.0,
                                10.0,
                                colors.surface_container_low,
                            );
                        } else if state == 1 {
                            b.stroke_circle(cx, cy, 6.0, dot, 2.0);
                            b.fill_circle(cx, cy, 2.5, dot);
                        } else {
                            b.stroke_circle(cx, cy, 6.0, dot, 1.5);
                        }
                    });
                    Spacer(Modifier::new().width(10.0));
                    Text(
                        label,
                        TextStyle::new().size(12.5).color(text),
                        Modifier::new(),
                    );
                },
            );
            Text(
                status,
                TextStyle::new()
                    .size(11.0)
                    .color(if state == 0 {
                        colors.outline
                    } else {
                        colors.primary
                    }),
                Modifier::new(),
            );
        },
    );
}

fn page_async() {
    let colors = theme_colors();
    // 0 = idle / 1 = running / 2 = done
    let phase = remember(|| mutableStateOf(0u8));
    let prog = remember(|| mutableStateOf(0.0f32));
    // 组合期注册一次：只有运行中才请求续帧（结束即停驱，保持空闲休眠）。
    remember(|| {
        let (phase, prog) = (phase, prog);
        add_frame_driver(move || {
            if phase.get() != 1 {
                return;
            }
            request_anim_frames();
            let p = (prog.get() + frame_delta() * 0.85).min(1.0);
            prog.set(p);
            if p >= 1.0 {
                phase.set(2);
            }
        });
        true
    });
    let p = prog.get();
    let running = phase.get() == 1;
    let done = phase.get() == 2;
    section("Async（异步任务模拟）");
    Text(
        "帧驱动推进的模拟任务：进度条 · 步骤清单 · 状态徽标随状态机同步".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    Card(Modifier::new().fill_max_width(), move || {
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            CrossAxisAlignment::Center,
            move || {
                Column(
                    Modifier::new(),
                    Arrangement::Start,
                    CrossAxisAlignment::Start,
                    move || {
                        Text(
                            "Workspace sync",
                            TextStyle::new()
                                .size(15.0)
                                .weight(700)
                                .color(colors.on_surface),
                            Modifier::new(),
                        );
                        Spacer(Modifier::new().height(3.0));
                        Text(
                            if done {
                                "同步完成 · 1,284 条记录已更新"
                            } else if running {
                                "正在拉取远端变更…"
                            } else {
                                "等待开始 · 预计 1.2s"
                            },
                            TextStyle::new().size(12.0).color(colors.on_surface_variant),
                            Modifier::new(),
                        );
                    },
                );
                Row(
                    Modifier::new(),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        if running {
                            Spinner(18.0, Modifier::new());
                            Spacer(Modifier::new().width(8.0));
                        }
                        if done {
                            status_pill(
                                "DONE",
                                colors.secondary_container,
                                colors.on_secondary_container,
                            );
                        } else if running {
                            status_pill(
                                "RUNNING",
                                colors.primary_container,
                                colors.on_primary_container,
                            );
                        } else {
                            status_pill(
                                "IDLE",
                                colors.surface_variant,
                                colors.on_surface_variant,
                            );
                        }
                    },
                );
            },
        );
        Spacer(Modifier::new().height(14.0));
        LinearProgress(Some(p), Modifier::new().fill_max_width());
        Spacer(Modifier::new().height(6.0));
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceBetween,
            CrossAxisAlignment::Center,
            move || {
                Text(
                    if done { "全部完成" } else { "同步进度" },
                    TextStyle::new().size(11.0).color(colors.on_surface_variant),
                    Modifier::new(),
                );
                Text(
                    format!("{:.0}%", p * 100.0),
                    TextStyle::new().size(11.0).weight(600).color(colors.primary),
                    Modifier::new(),
                );
            },
        );
        Spacer(Modifier::new().height(10.0));
        Divider(Modifier::new());
        async_step("Connect to workspace", if done { 2 } else if running { 1 } else { 0 }, colors);
        async_step(
            "Fetch remote records",
            if p > 0.30 || done { 2 } else if running { 1 } else { 0 },
            colors,
        );
        async_step(
            "Merge local changes",
            if p > 0.65 || done { 2 } else if running { 1 } else { 0 },
            colors,
        );
        async_step("Write index", if done { 2 } else if p > 0.92 { 1 } else { 0 }, colors);
        Spacer(Modifier::new().height(12.0));
        Row(
            Modifier::new(),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            move || {
                let (ph, pg) = (phase, prog);
                Button(
                    Modifier::new(),
                    move || {
                        pg.set(0.0);
                        ph.set(1);
                    },
                    move || {
                        Text(
                            if done { "Run again" } else { "Run task" },
                            TextStyle::new().color(colors.on_primary),
                            Modifier::new(),
                        );
                    },
                );
                Spacer(Modifier::new().width(8.0));
                let (ph, pg) = (phase, prog);
                TextButton(
                    "Reset",
                    Modifier::new(),
                    move || {
                        pg.set(0.0);
                        ph.set(0);
                    },
                );
            },
        );
    });
}

fn metric_tile(label: &'static str, value: String, color: Color, colors: ColorScheme) {
    Column(
        Modifier::new().padding_ltrb(20.0, 8.0, 20.0, 8.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                value.clone(),
                TextStyle::new().size(30.0).weight(800).color(color),
                Modifier::new(),
            );
            Spacer(Modifier::new().height(2.0));
            Text(
                label,
                TextStyle::new().size(11.0).color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

fn page_reactive() {
    let colors = theme_colors();
    let count = remember(|| mutableStateOf(3i32));
    let step = remember(|| mutableStateOf(1i32));
    section("Reactive & DSL（state → UI 自动刷新）");
    Text(
        "只修改状态，界面由组合自动推导；派生值在组合期直接计算，无手动刷新".to_string(),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(12.0));

    let n = count.get();
    let s = step.get();
    card_section("派生状态", move || {
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::SpaceEvenly,
            CrossAxisAlignment::Center,
            move || {
                metric_tile("count", format!("{}", n), colors.primary, colors);
                metric_tile("count × 2", format!("{}", n * 2), colors.tertiary, colors);
                metric_tile(
                    "count²",
                    format!("{}", n * n),
                    colors.secondary,
                    colors,
                );
                metric_tile(
                    "parity",
                    if n % 2 == 0 { "even" } else { "odd" }.to_string(),
                    colors.on_surface_variant,
                    colors,
                );
            },
        );
        Spacer(Modifier::new().height(12.0));
        Row(
            Modifier::new().fill_max_width(),
            Arrangement::Start,
            CrossAxisAlignment::Center,
            move || {
                let c = count;
                OutlinedButton(
                    Modifier::new(),
                    move || c.update(|v| *v -= s),
                    move || {
                        Text(
                            format!("− {}", s),
                            TextStyle::new().color(colors.primary),
                            Modifier::new(),
                        );
                    },
                );
                Spacer(Modifier::new().width(10.0));
                for (i, opt) in [1i32, 5, 10].iter().enumerate() {
                    if i > 0 {
                        Spacer(Modifier::new().width(6.0));
                    }
                    let st = step;
                    let chosen = s == *opt;
                    let val = *opt;
                    Chip(
                        format!("step {}", val),
                        None,
                        chosen,
                        Modifier::new(),
                        move || st.set(val),
                    );
                }
                Spacer(Modifier::new().width(10.0));
                let c = count;
                Button(
                    Modifier::new(),
                    move || c.update(|v| *v += s),
                    move || {
                        Text(
                            format!("+ {}", s),
                            TextStyle::new().color(colors.on_primary),
                            Modifier::new(),
                        );
                    },
                );
                Spacer(Modifier::new().width(8.0));
                let c = count;
                TextButton("Reset", Modifier::new(), move || c.set(0));
            },
        );
    });

    card_section("声明式组合", move || {
        CodeBlock(
            "let count = remember(|| mutableStateOf(3));\nlet step  = remember(|| mutableStateOf(1));\n\nText(format!(\"count = {}\", count.get()), ..);\n\nButton(Modifier::new(), move || {\n    count.update(|v| *v += step.get());\n}, ..);",
            Modifier::new(),
        );
    });
}

// ════════════════════════════ Basics ════════════════════════════

fn page_buttons() {
    let colors = theme_colors();
    let clicks = remember(|| mutableStateOf(0));
    section("Buttons");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Button(
                Modifier::new(),
                move || clicks.set(clicks.get() + 1),
                move || {
                    Text(
                        "Button()",
                        TextStyle::new().color(colors.on_primary),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().width(8.0));
            OutlinedButton(
                Modifier::new(),
                || {},
                move || {
                    Text(
                        "Outlined",
                        TextStyle::new().color(colors.primary),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().width(8.0));
            TextButton("Text", Modifier::new(), || {});
            Spacer(Modifier::new().width(8.0));
            IconButton(IconName::Add, Modifier::new(), || {});
            Spacer(Modifier::new().width(8.0));
            ExtendedFab(IconName::Add, "Extended", Modifier::new(), || {});
        },
    );
    Spacer(Modifier::new().height(8.0));
    Text(
        format!("clicks = {}", clicks.get()),
        TextStyle::new(),
        Modifier::new(),
    );
}

fn page_toggles() {
    let a = remember(|| mutableStateOf(true));
    let b = remember(|| mutableStateOf(false));
    let sel = remember(|| mutableStateOf(1usize));
    section("Toggles（Checkbox / Switch / radio）");
    CheckboxRow(a.clone(), "Enable feature", Modifier::new());
    SwitchRow(b.clone(), "Dark mode", Modifier::new());
    RadioGroup(
        &[
            "Small".to_string(),
            "Medium".to_string(),
            "Large".to_string(),
        ],
        sel.clone(),
        Modifier::new(),
    );
}

fn page_sliders() {
    let colors = theme_colors();
    let v = remember(|| mutableStateOf(0.4f32));
    let n = remember(|| mutableStateOf(3));
    section("Value Input（Slider / progress / spinbox）");
    Slider(v.clone(), Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    LinearProgress(Some(v.get()), Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    CircularProgress(Some(v.get()), 48.0, Modifier::new());
    Spacer(Modifier::new().height(8.0));
    SpinBox(n.clone(), 0, 10, Modifier::new());
    let _ = colors;
}

fn page_text_input() {
    let colors = theme_colors();
    let t = remember(|| mutableStateOf(String::from("Hello")));
    let p = remember(|| mutableStateOf(String::new()));
    let o = remember(|| mutableStateOf(String::new()));
    let q = remember(|| mutableStateOf(String::new()));
    let code = remember(|| mutableStateOf(String::new()));
    let tokens = remember(|| mutableStateOf(vec!["rust".to_string(), "gui".to_string()]));
    let draft = remember(|| mutableStateOf(String::new()));
    section("Text Input（filled / outlined / password / search / otp / token）");
    TextField(t.clone(), "Filled", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    OutlinedTextField(p.clone(), "Outlined", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    PasswordField(o.clone(), "Password", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    SearchBar(q.clone(), "Search…", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    OtpInput(code.clone(), 4, Modifier::new());
    Spacer(Modifier::new().height(8.0));
    TokenInput(
        tokens.clone(),
        draft.clone(),
        Modifier::new().fill_max_width(),
    );
    let _ = colors;
}

fn page_icons() {
    let colors = theme_colors();
    section("Icon（全套矢量图标）");
    let names = [
        IconName::Home,
        IconName::Star,
        IconName::Heart,
        IconName::Settings,
        IconName::User,
        IconName::Search,
        IconName::Bell,
        IconName::Camera,
    ];
    FlowRow(Modifier::new().fill_max_width(), 8.0, move || {
        for n in names {
            icon(n, 28.0, colors.primary, Modifier::new().padding(6.0));
        }
    });
}

fn page_avatars() {
    let colors = theme_colors();
    section("AvatarGroup & Badges");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Avatar("AB", 40.0, Modifier::new());
            Spacer(Modifier::new().width(8.0));
            Avatar("CD", 40.0, Modifier::new());
            Spacer(Modifier::new().width(8.0));
            AvatarGroup(
                &["AB".to_string(), "CD".to_string(), "EF".to_string()],
                40.0,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(8.0));
            BadgedBox("3", Modifier::new(), move || {
                Avatar("!", 40.0, Modifier::new())
            });
        },
    );
    let _ = colors;
}

fn page_group_box() {
    let colors = theme_colors();
    section("GroupBox");
    GroupBox("Options", Modifier::new().fill_max_width(), move || {
        Text(
            "Grouped controls go here",
            TextStyle::new().color(colors.on_surface_variant),
            Modifier::new(),
        );
    });
}

fn page_kbd() {
    section("Kbd（快捷键提示）");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Kbd(&["Ctrl".to_string(), "K".to_string()], Modifier::new());
        },
    );
}

fn page_link() {
    let colors = theme_colors();
    section("Link");
    Link("https://example.com", Modifier::new(), || {});
    Spacer(Modifier::new().height(8.0));
    Text(
        "(Link 自带主题色下划线，点击触发回调)",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

fn page_context_menu() {
    let colors = theme_colors();
    section("ContextMenu（右键）");
    ContextMenu(
        &["Cut".to_string(), "Copy".to_string(), "Paste".to_string()],
        Modifier::new()
            .fill_max_width()
            .height(60.0)
            .border(1.0, colors.outline_variant, 8.0),
        move |_i| {},
        move || {
            Text(
                "Right-click here",
                TextStyle::new(),
                Modifier::new().align(Alignment::Center),
            )
        },
    );
}

fn page_menu_demo() {
    let sel = remember(|| mutableStateOf(0usize));
    section("MenuBar / Dropdown");
    Dropdown(
        &[
            "Option A".to_string(),
            "Option B".to_string(),
            "Option C".to_string(),
        ],
        sel.clone(),
        Modifier::new(),
    );
}

// ════════════════════════════ Data Display ════════════════════════════

fn page_cards() {
    let colors = theme_colors();
    section("Cards");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Card(Modifier::new().width(160.0).padding(12.0), move || {
                Text(
                    "Filled",
                    TextStyle::new().weight(700).color(colors.on_surface),
                    Modifier::new(),
                );
            });
            Spacer(Modifier::new().width(8.0));
            ElevatedCard(Modifier::new().width(160.0).padding(12.0), move || {
                Text("Elevated", TextStyle::new().weight(700), Modifier::new());
            });
            Spacer(Modifier::new().width(8.0));
            OutlinedCard(Modifier::new().width(160.0).padding(12.0), move || {
                Text("Outlined", TextStyle::new().weight(700), Modifier::new());
            });
        },
    );
}

fn page_chips() {
    let colors = theme_colors();
    section("Chips & Tags");
    let checked = remember(|| mutableStateOf(false));
    FlowRow(Modifier::new().fill_max_width(), 8.0, move || {
        Chip("Primary", None, false, Modifier::new(), || {});
        Chip("Selected", None, true, Modifier::new(), || {});
        FilterChip("Filter", checked.clone(), Modifier::new());
        AssistChip("Assist", IconName::Add, Modifier::new(), || {});
        let _ = colors;
    });
}

fn page_list() {
    section("List（开箱即用：整行点击选择）");
    // 只需 items + selected：行节点自带 on_click，点击中间文字、左右 padding、
    // 上下留白都会选中该行；选中行 `secondary_container` 高亮，聚焦后
    // ↑/↓、PageUp/Down、Home/End 也能切换（与 ListView 同一套语义）。
    let sel = remember(|| mutableStateOf(0usize));
    let items: Vec<String> = ["Inbox", "Sent", "Draft", "Archive"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    List(&items, sel, Modifier::new().fill_max_width());

    section("List（ListItem 自定义行内容，改用 ListCustom）");
    // 原本需手拼 `ListItem` + 外包 `focusable_full` 才能整行选中/键盘导航；
    // 改用 `ListCustom` 后，行外壳（整行点击选中 + `secondary_container` 高亮 +
    // 键盘导航）统一由它提供，调用方只画内容（leading 图标 + 两行文字 + trailing 箭头）。
    let sel2 = remember(|| mutableStateOf(0usize));
    let rows: [(&str, &str, IconName); 3] = [
        ("Inbox", "12 new", IconName::Mail),
        ("Sent", "3 items", IconName::Send),
        ("Draft", "saved", IconName::Edit),
    ];
    let n2 = rows.len();
    let c2 = theme_colors();
    ListCustom(
        n2,
        move |i| {
            let (title, sub, ic) = rows[i];
            Row(
                Modifier::new()
                    .fill_max_width()
                    .padding_ltrb(16.0, 8.0, 16.0, 8.0),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    icon(ic, 24.0, c2.on_surface_variant, Modifier::new());
                    Spacer(Modifier::new().width(12.0));
                    Column(
                        Modifier::new().weight(1.0, false),
                        Arrangement::Center,
                        CrossAxisAlignment::Start,
                        move || {
                            Text(title, TextStyle::new().size(16.0), Modifier::new());
                            Text(
                                sub,
                                TextStyle::new().size(14.0).color(c2.on_surface_variant),
                                Modifier::new(),
                            );
                        },
                    );
                    Text(
                        "›",
                        TextStyle::new().size(16.0).color(c2.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        },
        sel2,
        Modifier::new().fill_max_width(),
    );
}

fn page_list_view() {
    let colors = theme_colors();
    section("ListView（virtual scroll）");
    // 直接用 `ListView` 控件：自带点击选中 + `secondary_container` 高亮。
    let sel = remember(|| mutableStateOf(0usize));
    let items: Vec<String> = (0..50).map(|i| format!("Row {}", i)).collect();
    ListView(
        &items,
        sel,
        Modifier::new()
            .fill_max_width()
            .height(200.0)
            .border(1.0, colors.outline_variant, 12.0),
    );

    section("ListViewCustom（可滚动 + 自定义 item）");
    // 与 `ListView` 共用滚动/sticky，但每行内容由 `item_content(i)` 绘制。
    let sel2 = remember(|| mutableStateOf(0usize));
    let items2: Vec<String> = (0..50).map(|i| format!("Row {}", i)).collect();
    ListViewCustom(
        items2.len(),
        move |i| {
            Text(
                &items2[i],
                TextStyle::new(),
                Modifier::new().padding_ltrb(16.0, 8.0, 16.0, 8.0),
            );
        },
        sel2,
        Modifier::new()
            .fill_max_width()
            .height(200.0)
            .border(1.0, colors.outline_variant, 12.0),
    );
}

fn page_grid() {
    let colors = theme_colors();
    section("Grid（Grid）");
    Grid(3usize, 8.0, Modifier::new().fill_max_width(), move || {
        for i in 0..9 {
            Card(Modifier::new().padding(12.0), move || {
                Text(
                    format!("{}", i),
                    TextStyle::new().color(colors.primary),
                    Modifier::new().align(Alignment::Center),
                );
            });
        }
    });
}

fn page_tree() {
    let colors = theme_colors();
    section("Tree（TreeView）");
    let expanded = remember(|| mutableStateOf(vec![String::new()]));
    // 选中节点路径 key（根=label，子=`父/label`）：初始选中根节点，点击/键导航均可切换。
    let sel = remember(|| mutableStateOf(String::from("Root")));
    let root = TreeNode::with_children(
        "Root",
        vec![
            TreeNode::with_children("Child A", vec![TreeNode::new("Leaf A1")]),
            TreeNode::new("Child B"),
        ],
    );
    TreeView(
        &[root],
        expanded,
        sel,
        Modifier::new()
            .fill_max_width()
            .border(1.0, colors.outline_variant, 8.0),
    );
}

fn page_table() {
    let colors = theme_colors();
    section("Table（DataTable，排序/斑马纹）");
    DataTable(
        &["ID".to_string(), "Name".to_string(), "Qty".to_string()],
        &[
            vec!["1".to_string(), "Apple".to_string(), "10".to_string()],
            vec!["2".to_string(), "Banana".to_string(), "20".to_string()],
            vec!["3".to_string(), "Cherry".to_string(), "15".to_string()],
        ],
        true,
        true,
        Modifier::new().border(1.0, colors.outline_variant, 8.0),
    );
}

fn page_color() {
    let colors = theme_colors();
    section("Color（swatches）");
    let palette = [
        colors.primary,
        colors.secondary,
        colors.tertiary,
        colors.error,
        colors.surface_variant,
    ];
    FlowRow(Modifier::new().fill_max_width(), 8.0, move || {
        for col in palette {
            // 用 Box（无自带底色层）直接铺 `col`；Card 会在调用方 background 之上
            // 再叠一层 surface_container，把色板颜色盖住 → 色块显示不出颜色。
            Box(
                Modifier::new()
                    .size(48.0, 48.0)
                    .background(col, 6.0)
                    .border(1.0, colors.outline_variant, 6.0),
                Alignment::TopStart,
                || {},
            );
        }
    });
}

fn page_color_picker() {
    let colors = theme_colors();
    section("ColorPicker（HSV 取色器）");
    let picked = remember(|| mutableStateOf(colors.primary));
    ColorPicker(picked, Modifier::new());
    Spacer(Modifier::new().height(12.0));
    let c = picked.get();
    Text(
        format!(
            "当前颜色  R{} G{} B{}",
            (c.r * 255.0).round() as i32,
            (c.g * 255.0).round() as i32,
            (c.b * 255.0).round() as i32
        ),
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

fn page_bubble() {
    let colors = theme_colors();
    section("Bubble（聊天气泡）");
    Bubble(
        "Hi there!",
        false,
        Modifier::new()
            .padding(10.0)
            .background(colors.surface_variant, 12.0),
    );
    Spacer(Modifier::new().height(8.0));
    Bubble(
        "Reply message",
        true,
        Modifier::new()
            .padding(10.0)
            .background(colors.primary_container, 12.0),
    );
}

fn page_charts() {
    let colors = theme_colors();
    section("Chart（bar / line / pie）");
    let labels = vec![
        "A".to_string(),
        "B".to_string(),
        "C".to_string(),
        "D".to_string(),
    ];
    card_section("Bar", move || {
        BarChart(
            &[3.0, 7.0, 5.0, 9.0],
            &labels,
            Modifier::new().fill_max_width(),
        );
    });
    card_section("Line", move || {
        LineChart(&[1.0, 3.0, 2.0, 5.0], Modifier::new().fill_max_width());
    });
    card_section("Pie", move || {
        PieChart(
            &[1.0, 2.0, 1.5],
            &[colors.primary, colors.secondary, colors.error],
            Some(&["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()]),
            false,
            Modifier::new(),
        );
    });

    // ── 通用 Chart（一个控件覆盖 21 种图型）──
    section("通用 Chart（Chart + ChartData / ChartOptions）");
    card_section(
        "分组柱 GroupedBar（多系列 + 数据标签）",
        move || {
            Chart(
                ChartType::GroupedBar,
                ChartData::series(vec![
                    ChartSeries::of(
                        "线上",
                        vec![(0.0, 32.0), (1.0, 41.0), (2.0, 38.0), (3.0, 52.0)],
                    ),
                    ChartSeries::of(
                        "门店",
                        vec![(0.0, 20.0), (1.0, 28.0), (2.0, 34.0), (3.0, 30.0)],
                    ),
                ]),
                ChartOptions::default()
                    .title("季度销量")
                    .data_labels(true)
                    .height(220.0)
                    .x_labels(vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()]),
                Modifier::new().fill_max_width(),
            );
        },
    );
    card_section(
        "折线 + 面积 Line / Area（多系列 + 图例）",
        move || {
            Chart(
                ChartType::Area,
                ChartData::series(vec![
                    ChartSeries::of(
                        "PV",
                        vec![(0.0, 12.0), (1.0, 28.0), (2.0, 22.0), (3.0, 45.0)],
                    ),
                    ChartSeries::of(
                        "UV",
                        vec![(0.0, 8.0), (1.0, 16.0), (2.0, 30.0), (3.0, 26.0)],
                    ),
                ]),
                ChartOptions::default().title("访问趋势").height(220.0),
                Modifier::new().fill_max_width(),
            );
        },
    );
    card_section("堆叠柱 StackedBar", move || {
        Chart(
            ChartType::StackedBar,
            ChartData::series(vec![
                ChartSeries::of("Android", vec![(0.0, 30.0), (1.0, 45.0), (2.0, 38.0)]),
                ChartSeries::of("iOS", vec![(0.0, 20.0), (1.0, 26.0), (2.0, 33.0)]),
                ChartSeries::of("Web", vec![(0.0, 10.0), (1.0, 18.0), (2.0, 15.0)]),
            ]),
            ChartOptions::default().title("平台占比").height(220.0),
            Modifier::new().fill_max_width(),
        );
    });
    card_section("环形 Donut + 漏斗 Funnel", move || {
        Chart(
            ChartType::Donut,
            ChartData::pie(vec![
                PieSlice::new("线上", 35.0, Color::TRANSPARENT),
                PieSlice::new("门店", 28.0, Color::TRANSPARENT),
                PieSlice::new("其它", 22.0, Color::TRANSPARENT),
            ]),
            ChartOptions::default()
                .title("渠道结构")
                .donut(0.6)
                .height(220.0),
            Modifier::new().fill_max_width(),
        );
        Chart(
            ChartType::Funnel,
            ChartData::funnel(vec![
                FunnelSlice {
                    label: "访问".into(),
                    value: 100.0,
                    color: Color::TRANSPARENT,
                },
                FunnelSlice {
                    label: "注册".into(),
                    value: 62.0,
                    color: Color::TRANSPARENT,
                },
                FunnelSlice {
                    label: "下单".into(),
                    value: 28.0,
                    color: Color::TRANSPARENT,
                },
                FunnelSlice {
                    label: "复购".into(),
                    value: 12.0,
                    color: Color::TRANSPARENT,
                },
            ]),
            ChartOptions::default()
                .title("转化漏斗")
                .legend(false)
                .height(200.0),
            Modifier::new().fill_max_width(),
        );
    });
    card_section("雷达 Radar + 气泡 Bubble", move || {
        Chart(
            ChartType::Radar,
            ChartData::series(vec![
                ChartSeries::of(
                    "本季",
                    vec![
                        (0.0, 80.0),
                        (1.0, 65.0),
                        (2.0, 90.0),
                        (3.0, 55.0),
                        (4.0, 72.0),
                    ],
                ),
                ChartSeries::of(
                    "上季",
                    vec![
                        (0.0, 60.0),
                        (1.0, 70.0),
                        (2.0, 75.0),
                        (3.0, 50.0),
                        (4.0, 68.0),
                    ],
                ),
            ]),
            ChartOptions::default()
                .title("能力雷达")
                .height(220.0)
                .x_labels(vec![
                    "性能".into(),
                    "稳定".into(),
                    "易用".into(),
                    "生态".into(),
                    "文档".into(),
                ]),
            Modifier::new().fill_max_width(),
        );
        Chart(
            ChartType::Bubble,
            ChartData::series(vec![ChartSeries::auto(
                "样本",
                vec![
                    DataPoint::bubble(1.0, 24.0, 1.5),
                    DataPoint::bubble(2.0, 38.0, 2.6),
                    DataPoint::bubble(3.0, 55.0, 3.4),
                    DataPoint::bubble(4.0, 71.0, 2.1),
                    DataPoint::bubble(5.0, 88.0, 4.2),
                ],
            )]),
            ChartOptions::default().title("气泡散点").height(220.0),
            Modifier::new().fill_max_width(),
        );
    });
    card_section("瀑布 Waterfall + 直方 Histogram", move || {
        Chart(
            ChartType::Waterfall,
            ChartData::waterfall(vec![
                WaterfallEntry::new("期初", 40.0, WaterfallKind::Total),
                WaterfallEntry::new("新增", 15.0, WaterfallKind::Increase),
                WaterfallEntry::new("流失", 8.0, WaterfallKind::Decrease),
                WaterfallEntry::new("期末", 47.0, WaterfallKind::Total),
            ]),
            ChartOptions::default().title("用户变化").height(220.0),
            Modifier::new().fill_max_width(),
        );
        Chart(
            ChartType::Histogram,
            ChartData::series(vec![ChartSeries::auto(
                "样本",
                vec![
                    DataPoint::new(0.0, 12.0),
                    DataPoint::new(0.0, 15.0),
                    DataPoint::new(0.0, 22.0),
                    DataPoint::new(0.0, 24.0),
                    DataPoint::new(0.0, 31.0),
                    DataPoint::new(0.0, 38.0),
                    DataPoint::new(0.0, 44.0),
                    DataPoint::new(0.0, 51.0),
                ],
            )]),
            ChartOptions::default()
                .title("分布直方")
                .bins(5)
                .height(200.0),
            Modifier::new().fill_max_width(),
        );
    });
    card_section(
        "K 线 Candlestick + 横向柱 HorizontalBar + 径向 RadialBar",
        move || {
            Chart(
                ChartType::Candlestick,
                ChartData::candle(vec![
                    Candle::new("D1", 10.0, 12.5, 13.2, 9.4),
                    Candle::new("D2", 12.5, 11.8, 13.6, 11.2),
                    Candle::new("D3", 11.8, 14.2, 14.8, 11.5),
                    Candle::new("D4", 14.2, 15.6, 16.1, 13.9),
                ]),
                ChartOptions::default().title("K 线").height(200.0),
                Modifier::new().fill_max_width(),
            );
            Chart(
                ChartType::HorizontalBar,
                ChartData::series(vec![ChartSeries::of(
                    "耗时",
                    vec![(0.0, 32.0), (1.0, 18.0), (2.0, 47.0), (3.0, 25.0)],
                )]),
                ChartOptions::default()
                    .title("横向柱")
                    .height(180.0)
                    .x_labels(vec![
                        "编译".into(),
                        "链接".into(),
                        "测试".into(),
                        "打包".into(),
                    ]),
                Modifier::new().fill_max_width(),
            );
            Chart(
                ChartType::RadialBar,
                ChartData::series(vec![ChartSeries::of(
                    "达成",
                    vec![(0.0, 82.0), (1.0, 64.0), (2.0, 91.0), (3.0, 55.0)],
                )]),
                ChartOptions::default()
                    .title("径向柱")
                    .legend(false)
                    .height(200.0),
                Modifier::new().fill_max_width(),
            );
        },
    );
    card_section("桑基 Sankey", move || {
        Chart(
            ChartType::Sankey,
            ChartData::sankey(
                vec![
                    SankeyNode {
                        label: "首页".into(),
                        value: 100.0,
                    },
                    SankeyNode {
                        label: "文档".into(),
                        value: 60.0,
                    },
                    SankeyNode {
                        label: "注册".into(),
                        value: 40.0,
                    },
                ],
                vec![
                    SankeyLink {
                        source: 0,
                        target: 1,
                        value: 60.0,
                    },
                    SankeyLink {
                        source: 0,
                        target: 2,
                        value: 40.0,
                    },
                ],
            ),
            ChartOptions::default().title("流量桑基").height(220.0),
            Modifier::new().fill_max_width(),
        );
    });
}

fn page_description() {
    section("DescriptionList");
    DescriptionList(
        &[
            ("Name".to_string(), "RustCompose".to_string()),
            ("Version".to_string(), "0.1.0".to_string()),
            ("License".to_string(), "MIT".to_string()),
        ],
        Modifier::new().fill_max_width(),
    );
}

fn page_diff() {
    let colors = theme_colors();
    let lines: Vec<(char, String)> = vec![
        (' ', "line one".to_string()),
        ('-', "line two".to_string()),
        (' ', "line three".to_string()),
        ('+', "line 2".to_string()),
        ('+', "line four".to_string()),
    ];
    section("DiffView（LCS 差分）");
    DiffView(
        &lines,
        Modifier::new()
            .fill_max_width()
            .border(1.0, colors.outline_variant, 8.0),
    );
}

fn page_markdown() {
    section("Markdown");
    Markdown(
        "# Title\n\n- item **bold**\n- item *italic*\n\n```\ncode block\n```",
        Modifier::new().fill_max_width(),
    );
}

// ════════════════════════════ Layout ════════════════════════════

fn page_row_col() {
    let colors = theme_colors();
    section("Row & Column");
    Column(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                CrossAxisAlignment::Center,
                move || {
                    Text("Left", TextStyle::new(), Modifier::new());
                    Text(
                        "Right",
                        TextStyle::new().color(colors.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().height(8.0));
            Row(
                Modifier::new(),
                Arrangement::Center,
                CrossAxisAlignment::Center,
                move || {
                    for _ in 0..3 {
                        icon(IconName::Star, 20.0, colors.primary, Modifier::new());
                    }
                },
            );
        },
    );
}

fn page_flex() {
    let colors = theme_colors();
    section("Expanded / Flexible（weight）");
    Row(
        Modifier::new().fill_max_width().height(60.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Box(
                Modifier::new()
                    .weight(1.0, true)
                    .background(colors.primary_container, 8.0),
                Alignment::Center,
                move || {
                    Text("weight 1", TextStyle::new(), Modifier::new());
                },
            );
            Spacer(Modifier::new().width(8.0));
            Box(
                Modifier::new()
                    .weight(2.0, true)
                    .background(colors.secondary_container, 8.0),
                Alignment::Center,
                move || {
                    Text("weight 2", TextStyle::new(), Modifier::new());
                },
            );
        },
    );
}

fn page_wrap() {
    section("Wrap（FlowRow）");
    FlowRow(Modifier::new().fill_max_width(), 8.0, move || {
        for w in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"] {
            Chip(w, None, false, Modifier::new(), || {});
        }
    });
}

fn page_stack() {
    let colors = theme_colors();
    section("Stack（Box + Alignment）");
    Box(
        Modifier::new()
            .size(160.0, 120.0)
            .background(colors.surface_variant, 12.0),
        Alignment::Center,
        move || {
            Text("Centered", TextStyle::new(), Modifier::new());
        },
    );
}

fn page_align() {
    let colors = theme_colors();
    section("Align / Spacer");
    Box(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(colors.surface_variant, 8.0),
        Alignment::BottomEnd,
        move || {
            Text("BottomEnd", TextStyle::new(), Modifier::new().padding(8.0));
        },
    );
}

fn page_modifier() {
    let colors = theme_colors();
    section("Modifier（padding / background / border / radius）");
    Card(
        Modifier::new()
            .padding(16.0)
            .background(colors.primary_container, 16.0)
            .border(1.0, colors.outline, 16.0),
        move || {
            Text("Styled box", TextStyle::new(), Modifier::new());
        },
    );
}

fn page_splitter() {
    let colors = theme_colors();
    section("Splitter（SplitRow）");
    SplitRow(
        Modifier::new().fill_max_width().height(160.0),
        0.5,
        move || {
            Box(
                Modifier::new().background(colors.primary_container, 0.0),
                Alignment::Center,
                move || Text("Pane A", TextStyle::new(), Modifier::new()),
            )
        },
        move || {
            Box(
                Modifier::new().background(colors.secondary_container, 0.0),
                Alignment::Center,
                move || Text("Pane B", TextStyle::new(), Modifier::new()),
            )
        },
    );
}

fn page_zoomable() {
    let colors = theme_colors();
    section("Zoomable（MediaSurface 占位）");
    Box(
        Modifier::new()
            .fill_max_width()
            .height(160.0)
            .background(colors.surface_variant, 12.0),
        Alignment::Center,
        move || {
            Text(
                "Pinch / double-tap to zoom",
                TextStyle::new().color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

fn page_divider() {
    let colors = theme_colors();
    section("Divider（horizontal / vertical）");
    Divider(Modifier::new());
    Spacer(Modifier::new().height(8.0));
    Row(
        Modifier::new().height(40.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Text("A", TextStyle::new(), Modifier::new());
            VerticalDivider(Modifier::new());
            Text("B", TextStyle::new(), Modifier::new());
            let _ = colors;
        },
    );
}

fn page_sized_box() {
    let colors = theme_colors();
    section("SizedBox");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            SizedBox(
                60.0,
                60.0,
                Modifier::new().background(colors.primary_container, 8.0),
                || {},
            );
            Spacer(Modifier::new().width(8.0));
            SizedBox(
                100.0,
                40.0,
                Modifier::new().background(colors.secondary_container, 8.0),
                || {},
            );
        },
    );
}

// ════════════════════════════ Navigation ════════════════════════════

fn page_tabs() {
    let colors = theme_colors();
    let tab = remember(|| mutableStateOf(0usize));
    section("TabRow（少标签）");
    TabRow(
        &[
            "Home".to_string(),
            "Profile".to_string(),
            "Settings".to_string(),
        ],
        tab.clone(),
        Modifier::new().fill_max_width(),
    );
    Spacer(Modifier::new().height(8.0));
    Box(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(colors.surface_variant, 8.0),
        Alignment::Center,
        move || {
            Text(
                ["Home page", "Profile page", "Settings page"][tab.get()].to_string(),
                TextStyle::new(),
                Modifier::new(),
            );
        },
    );

    Spacer(Modifier::new().height(16.0));
    let tab2 = remember(|| mutableStateOf(0usize));
    section("TabRow（多标签 → 右侧 ▾ 下拉 / 横向滚动）");
    let many: Vec<String> = (0..12).map(|i| format!("Tab {}", i + 1)).collect();
    TabRow(&many, tab2.clone(), Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    Box(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(colors.surface_variant, 8.0),
        Alignment::Center,
        move || {
            Text(
                format!("当前：{}", many[tab2.get()]),
                TextStyle::new(),
                Modifier::new(),
            );
        },
    );
}

fn page_stepper() {
    let colors = theme_colors();
    let step = remember(|| mutableStateOf(0usize));
    section("Stepper");
    Stepper(
        &[
            "Step 1".to_string(),
            "Step 2".to_string(),
            "Step 3".to_string(),
            "Step 4".to_string(),
        ],
        step.clone(),
        Modifier::new().fill_max_width(),
    );
    Spacer(Modifier::new().height(8.0));
    Button(
        Modifier::new(),
        move || step.set((step.get() + 1) % 4),
        move || {
            Text(
                "Next",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
}

fn page_expansion() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(true));
    section("ExpansionTile / Accordion（Expandable）");
    Expandable(
        Modifier::new().fill_max_width(),
        open.clone(),
        60.0,
        move || {
            Text(
                "Details",
                TextStyle::new().weight(700),
                Modifier::new().padding(12.0),
            );
        },
        move || {
            Text(
                "Expanded content",
                TextStyle::new().color(colors.on_surface_variant),
                Modifier::new().padding(12.0),
            );
        },
    );
}

fn page_nav_rail() {
    let colors = theme_colors();
    let sel = remember(|| mutableStateOf(0usize));
    section("NavigationRail");
    NavigationRail(
        &[
            NavItem::new(IconName::Home, "Home"),
            NavItem::new(IconName::Search, "Search"),
            NavItem::new(IconName::Settings, "Settings"),
        ],
        sel.clone(),
        None,
        Modifier::new(),
    );
    let _ = colors;
}

fn page_page_view() {
    let idx = remember(|| mutableStateOf(0usize));
    section("PageView / Carousel（自动轮播）");
    Carousel(
        idx.clone(),
        3usize,
        Modifier::new().fill_max_width().height(140.0),
        move || {
            let colors = theme_colors();
            Card(Modifier::new().padding(16.0), move || {
                Text(
                    format!("Page {}", idx.get()),
                    TextStyle::new().color(colors.on_primary),
                    Modifier::new().align(Alignment::Center),
                );
            });
        },
    );
}

fn page_drawer() {
    let open = remember(|| mutableStateOf(false));
    section("Drawer & BottomSheet（ModalDrawer / BottomSheet）");
    Button(
        Modifier::new(),
        {
            let o = open.clone();
            move || o.set(!o.get())
        },
        move || {
            let colors = theme_colors();
            Text(
                "Toggle Drawer",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    if open.get() {
        ModalDrawer(open.clone(), 260.0, move || {
            let items = nav_items();
            Column(
                Modifier::new().fill_max_size(),
                Arrangement::Start,
                CrossAxisAlignment::Start,
                move || {
                    for it in items.iter() {
                        ListItem(&it.label, None, Some(it.icon), None, Modifier::new(), None);
                    }
                },
            );
        });
        BottomSheet(open.clone(), 160.0, move || {
            let colors = theme_colors();
            Text(
                "Bottom sheet",
                TextStyle::new().color(colors.on_surface),
                Modifier::new().padding(16.0),
            );
        });
    }
}

fn page_accordion() {
    let colors = theme_colors();
    let a = remember(|| mutableStateOf(false));
    let b = remember(|| mutableStateOf(false));
    section("Accordion（多开）");
    Expandable(
        Modifier::new().fill_max_width(),
        a.clone(),
        50.0,
        move || {
            Text(
                "Section A",
                TextStyle::new().weight(700),
                Modifier::new().padding(12.0),
            );
        },
        move || {
            Text(
                "Content A",
                TextStyle::new().color(colors.on_surface_variant),
                Modifier::new().padding(12.0),
            );
        },
    );
    Expandable(
        Modifier::new().fill_max_width(),
        b.clone(),
        50.0,
        move || {
            Text(
                "Section B",
                TextStyle::new().weight(700),
                Modifier::new().padding(12.0),
            );
        },
        move || {
            Text(
                "Content B",
                TextStyle::new().color(colors.on_surface_variant),
                Modifier::new().padding(12.0),
            );
        },
    );
}

fn page_breadcrumb() {
    section("Breadcrumb");
    Breadcrumb(
        &[
            "Home".to_string(),
            "Library".to_string(),
            "Data".to_string(),
        ],
        None,
        Modifier::new().fill_max_width(),
    );
}

fn page_pagination() {
    let colors = theme_colors();
    let page = remember(|| mutableStateOf(1usize));
    section("Pagination");
    Pagination(page.clone(), 10usize, Modifier::new());
    Spacer(Modifier::new().height(8.0));
    Text(
        format!("Page {}", page.get()),
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

fn page_toolbar() {
    section("Toolbar / Ribbon（AppBarAction）");
    Toolbar(
        vec![
            AppBarAction::new(IconName::Save, || {}),
            AppBarAction::new(IconName::Edit, || {}),
            AppBarAction::new(IconName::Delete, || {}),
        ],
        Modifier::new().fill_max_width(),
    );
}

fn page_status_bar_demo() {
    section("StatusBar");
    StatusBar(
        "Ready",
        "RustCompose",
        Modifier::new().fill_max_width().height(24.0),
    );
}

fn page_title_bar_demo() {
    section("TitleBar");
    TitleBar(
        "RustCompose Gallery",
        Modifier::new().fill_max_width().height(38.0),
    );
}

fn page_tab_bar() {
    let colors = theme_colors();
    let tab = remember(|| mutableStateOf(0usize));
    section("TabBar / TabController（多标签溢出）");
    TabRow(
        &[
            "Tab 1".to_string(),
            "Tab 2".to_string(),
            "Tab 3".to_string(),
            "Tab 4".to_string(),
            "Tab 5".to_string(),
        ],
        tab.clone(),
        Modifier::new().fill_max_width(),
    );
    let _ = colors;
}

fn page_navigator() {
    let colors = theme_colors();
    section("Navigator / Overlay / Router");
    Text(
        "Declarative routing demo (OverlayHost active at root)",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));
    Button(
        Modifier::new(),
        || {},
        move || {
            Text(
                "Push route",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
}

// ════════════════════════════ Feedback ════════════════════════════

fn page_rating() {
    let colors = theme_colors();
    let r = remember(|| mutableStateOf(3i32));
    section("Rating（RatingBar）");
    RatingBar(r.clone(), 5, Modifier::new());
    Spacer(Modifier::new().height(8.0));
    Text(
        format!("{}/5", r.get()),
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

fn page_time_picker() {
    let colors = theme_colors();
    let h = remember(|| mutableStateOf(9));
    let m = remember(|| mutableStateOf(30));
    section("TimePicker");
    TimePicker(h.clone(), m.clone(), Modifier::new().fill_max_width());
    let _ = colors;
}

fn page_date_picker() {
    let colors = theme_colors();
    let sel = remember(|| mutableStateOf(31u32));
    section("DatePicker");
    DatePicker(2026, 10, sel.clone(), Modifier::new().fill_max_width());
    let _ = colors;
}

fn page_snackbar() {
    // 与壳层 `SnackbarHost(sn)` 共用同一宿主句柄（`story_shell` 写入 `SNACK_HOST`）。
    let snack = SNACK_HOST.with(|c| c.get()).expect("snackbar host");
    let colors = theme_colors();
    section("SnackBar & Banner");
    Banner(
        "This is an informational Banner.",
        IconName::Info,
        Some(Box::new(|| {})),
    );
    Spacer(Modifier::new().height(8.0));
    Button(
        Modifier::new(),
        move || {
            snack.show("Saved successfully");
        },
        move || {
            Text(
                "Show Snackbar",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
}

fn page_alert() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    section("Alert（AlertDialog / InlineAlert）");
    InlineAlert(
        AlertKind::Info,
        "Informational alert",
        "This is an informational alert.",
    );
    Spacer(Modifier::new().height(8.0));
    Button(
        Modifier::new(),
        move || open.set(true),
        move || {
            Text(
                "Open AlertDialog",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    if open.get() {
        AlertDialog(
            open.clone(),
            "Confirm",
            "Are you sure?",
            "OK",
            "Cancel",
            move || open.set(false),
        );
    }
}

fn page_command_palette() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    let q = remember(|| mutableStateOf(String::new()));
    section("CommandPalette（Ctrl+K 占位）");
    Button(
        Modifier::new(),
        move || open.set(true),
        move || {
            Text(
                "Open Palette",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    CommandPalette(
        open.clone(),
        q.clone(),
        &[
            "New File".to_string(),
            "Open".to_string(),
            "Save".to_string(),
            "Quit".to_string(),
        ],
        move |_i| {},
    );
}

fn page_empty_state() {
    section("EmptyState");
    EmptyState(
        IconName::Mail,
        "No items yet",
        "Add your first item to get started.",
        None,
        None,
    );
}

fn page_hover_card() {
    let colors = theme_colors();
    section("HoverCard");
    HoverCard(
        220.0,
        move || Text("Hover me", TextStyle::new(), Modifier::new().padding(8.0)),
        move || {
            Text(
                "Tooltip-like hover Card",
                TextStyle::new().color(colors.on_surface_variant),
                Modifier::new().padding(8.0),
            )
        },
    );
}

fn page_notification() {
    let show = remember(|| mutableStateOf(false));
    let colors = theme_colors();
    section("Notification");
    Button(
        Modifier::new(),
        {
            let s = show.clone();
            move || s.set(!s.get())
        },
        move || {
            Text(
                "Toggle Notification",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    if show.get() {
        Notification(
            "New message",
            "You have 1 new message",
            AlertKind::Info,
            None,
        );
    }
}

fn page_skeleton() {
    let colors = theme_colors();
    section("Skeleton（loading placeholder）");
    Skeleton(
        200.0,
        80.0,
        Modifier::new().border(1.0, colors.outline_variant, 8.0),
    );
    Spacer(Modifier::new().height(8.0));
    ShimmerPlaceholder(
        200.0,
        40.0,
        Modifier::new().border(1.0, colors.outline_variant, 8.0),
    );
    let _ = colors;
}

fn page_tooltip() {
    let colors = theme_colors();
    section("Tooltip");
    Tooltip("Tooltip Text", move || {
        Text(
            "Hover",
            TextStyle::new(),
            Modifier::new()
                .padding(8.0)
                .border(1.0, colors.outline_variant, 8.0),
        )
    });
}

fn page_dialog() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    section("Dialog（Dialog）");
    Button(
        Modifier::new(),
        move || open.set(true),
        move || {
            Text(
                "Open Dialog",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    Dialog(open.clone(), 320.0, 200.0, move || {
        Text(
            "Dialog content",
            TextStyle::new().color(colors.on_surface),
            Modifier::new().align(Alignment::Center),
        );
    });
}

// ════════════════════════════ Advanced ════════════════════════════

fn page_ink_well() {
    let colors = theme_colors();
    section("InkWell（ripple 静态近似）");
    InkWell(
        Modifier::new()
            .fill_max_width()
            .height(56.0)
            .background(colors.primary_container, 12.0),
        || {},
        move || {
            Text(
                "Tappable Surface",
                TextStyle::new().color(colors.on_primary_container),
                Modifier::new().align(Alignment::Center),
            );
        },
    );
}

fn page_popup_menu() {
    section("PopupMenuButton");
    PopupMenuButton(
        &[
            "Edit".to_string(),
            "Duplicate".to_string(),
            "Delete".to_string(),
        ],
        Modifier::new(),
        move |_i| {},
    );
}

fn page_animated() {
    let colors = theme_colors();
    let on = remember(|| mutableStateOf(true));
    section("AnimatedWidget（Crossfade）");
    Crossfade(if on.get() { 1 } else { 0 }, Modifier::new(), move || {
        Text(
            if on.get() { "Visible" } else { "Hidden" },
            TextStyle::new().color(colors.primary),
            Modifier::new(),
        );
    });
    Spacer(Modifier::new().height(8.0));
    Spinner(24.0, Modifier::new());
}

fn page_images() {
    let colors = theme_colors();
    section("Images（ImagePlaceholder）");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            ImagePlaceholder(
                "img",
                Modifier::new()
                    .size(80.0, 80.0)
                    .background(colors.surface_variant, 12.0),
            );
            Spacer(Modifier::new().width(8.0));
            ImagePlaceholder(
                "img",
                Modifier::new()
                    .size(80.0, 80.0)
                    .background(colors.primary_container, 40.0),
            );
        },
    );
}

fn page_native_dialogs() {
    let open = remember(|| mutableStateOf(false));
    section("Native Dialogs（Dialog）");
    Button(
        Modifier::new(),
        {
            let o = open.clone();
            move || o.set(true)
        },
        move || {
            let colors = theme_colors();
            Text(
                "Open Dialog",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    Dialog(open.clone(), 300.0, 160.0, move || {
        let colors = theme_colors();
        Text(
            "Native-style Dialog",
            TextStyle::new().color(colors.on_surface),
            Modifier::new().align(Alignment::Center),
        );
    });
}

fn page_scroll_area() {
    let colors = theme_colors();
    section("ScrollArea（VerticalScroll）");
    VerticalScroll(
        Modifier::new()
            .fill_max_width()
            .height(160.0)
            .border(1.0, colors.outline_variant, 12.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            for i in 0..20 {
                Text(
                    format!("Line {}", i),
                    TextStyle::new(),
                    Modifier::new().padding(4.0),
                );
            }
        },
    );
}

fn page_builders() {
    let colors = theme_colors();
    section("Builder Helpers（Box 自绘占位）");
    Box(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(colors.primary_container, 12.0),
        Alignment::Center,
        move || {
            icon(IconName::Brush, 32.0, colors.primary, Modifier::new());
        },
    );
}

fn page_calendar() {
    let colors = theme_colors();
    let sel = remember(|| mutableStateOf(31u32));
    section("Calendar");
    Calendar(2026, 10, sel.clone(), Modifier::new().fill_max_width());
    let _ = colors;
}

fn page_code_editor() {
    let colors = theme_colors();
    section("CodeEditor（CodeBlock）");
    CodeBlock(
        "fn main() {\n    println!(\"Hello, RustCompose!\");\n}",
        Modifier::new()
            .fill_max_width()
            .border(1.0, colors.outline_variant, 8.0),
    );
}

fn page_docking() {
    let colors = theme_colors();
    section("Docking（可拖拽分隔）");
    Text(
        "SplitRow / SplitColumn 支持拖拽中间分隔条实时调整比例（Layout 页也有演示）。",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));
    SplitRow(
        Modifier::new().fill_max_width().height(160.0),
        0.5,
        move || {
            Box(
                Modifier::new().background(colors.primary_container, 0.0),
                Alignment::Center,
                move || Text("Pane A", TextStyle::new(), Modifier::new()),
            )
        },
        move || {
            Box(
                Modifier::new().background(colors.secondary_container, 0.0),
                Alignment::Center,
                move || Text("Pane B", TextStyle::new(), Modifier::new()),
            )
        },
    );
}

fn page_rich_text() {
    let colors = theme_colors();
    section("RichTextEditor（RichText）");
    RichText(
        &[
            ("Rich ".to_string(), TextStyle::new().size(14.0).weight(700)),
            (
                "Text ".to_string(),
                TextStyle::new().size(14.0).color(colors.primary),
            ),
            ("with ".to_string(), TextStyle::new().size(14.0)),
            (
                "styles".to_string(),
                TextStyle::new().size(14.0).weight(700),
            ),
        ],
        Modifier::new().fill_max_width(),
    );
}

fn page_settings() {
    let a = remember(|| mutableStateOf(true));
    let b = remember(|| mutableStateOf(false));
    section("Settings（SettingsSection / SettingsItem）");
    SettingsSection("General", Modifier::new().fill_max_width(), move || {
        SettingsItem(
            "Wi-Fi",
            Some("Toggle wireless"),
            Modifier::new().fill_max_width(),
            move || Switch(a.clone(), Modifier::new()),
        );
        SettingsItem(
            "Bluetooth",
            Some("Toggle bluetooth"),
            Modifier::new().fill_max_width(),
            move || Switch(b.clone(), Modifier::new()),
        );
    });
}

fn page_auto_hide() {
    let colors = theme_colors();
    section("AutoHideToolbar（peek 2px）");
    Toolbar(
        vec![
            AppBarAction::new(IconName::Home, || {}),
            AppBarAction::new(IconName::Search, || {}),
        ],
        Modifier::new().fill_max_width(),
    );
    Spacer(Modifier::new().height(8.0));
    Text(
        "Toolbar auto-hides on scroll (demo).",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

// ════════════════════════════ 入口 ════════════════════════════

fn main() {
    // 登记文本控件输入路由（键盘/字符/IME/剪贴板/滚动 → compose-material）。
    compose_material::install_router();
    run_app_with(
        WindowConfig::sized("Story — Compose Showcase", 1080.0, 800.0),
        StoryApp { dark: false, palette: 0 },
    );
}

// ══════════════════ 无头探针（cargo test --example story）══════════════════
#[cfg(test)]
mod probe_state {
    std::thread_local! {
        /// story_shell 每趟组合后镜像 open_menu / page，供探针断言。
        pub static OM: std::cell::Cell<i8> = const { std::cell::Cell::new(-1) };
        pub static PAGE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    use compose::foundation::Constraints;
    use compose::layout::{
        Alignment, create_root_node, dispatch_click_at, hit_test, perform_layout,
    };
    use compose::runtime::{Composition, Recomposer, advance_frame, with_rt_pub};
    fn boot() -> compose::layout::NodeId {
        let root = create_root_node(Alignment::TopStart);
        let comp = Composition::new(root);
        comp.set_content(|| {
            let dark = remember(|| mutableStateOf(false));
            let palette = remember(|| mutableStateOf(0usize));
            // 与 StoryApp 相同层次：主题在宿主外层（浮层内容也要读主题 local）。
            MaterialTheme(ColorScheme::light(), move || {
                OverlayHost(Modifier::new().fill_max_size(), move || {
                    Box(
                        Modifier::new().fill_max_size(),
                        Alignment::TopStart,
                        move || story_shell(dark, palette),
                    );
                });
            });
        });
        perform_layout(root, Constraints::new(0.0, 1080.0, 0.0, 800.0));
        root
    }

    fn recomp(root: compose::layout::NodeId) {
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::new(0.0, 1080.0, 0.0, 800.0));
    }

    /// 点击窗口坐标 (x,y)（等价 kernel.pointer_down 的命中+分发）。
    fn click(root: compose::layout::NodeId, x: f32, y: f32) -> bool {
        let Some((target, lx, ly)) = hit_test(root, x, y) else {
            println!("[probe] click({x},{y}) 未命中任何节点");
            return false;
        };
        dispatch_click_at(target, lx, ly)
    }

    /// 菜单栏第 i 项中心（标题栏 TITLE_H 之下）。
    fn menu_xy(i: usize) -> (f32, f32) {
        (i as f32 * MENU_W + MENU_W / 2.0, TITLE_H + MENU_H / 2.0)
    }

    /// 宿主（OverlayHost 节点）当前的孩子数（>1 即有浮层发射到树上）。
    /// 结构：root → MaterialTheme 透传节点 → OverlayHost → 内容 Box。
    fn host_kids(root: compose::layout::NodeId) -> usize {
        with_rt_pub(|rt| {
            let themed = rt.nodes[root.index()].children[0];
            let host = rt.nodes[themed.index()].children[0];
            rt.nodes[host.index()].children.len()
        })
    }

    fn om() -> i8 {
        super::probe_state::OM.with(|c| c.get())
    }
    fn page() -> usize {
        super::probe_state::PAGE.with(|c| c.get())
    }

    /// 回归：菜单栏下拉的完整打开/关闭/选择流程。
    /// 曾有两个框架级缺陷导致「点菜单不显示下拉、反而跳回 Spreadsheet」：
    /// 1) 祖先作用域重组时销毁后代作用域 → remember 槽（open_menu/page）丢失重建；
    /// 2) 已发射浮层被移除时不调度宿主重组 → 幽灵浮层残留并吞掉点击。
    #[test]
    fn menu_dropdown_flow() {
        let root = boot();
        let (vx, vy) = menu_xy(1); // "View"

        // 1) 点击 View → 菜单打开（浮层发射到宿主），页面不动
        assert!(click(root, vx, vy), "首次点击 View 应被消费");
        recomp(root);
        assert_eq!(host_kids(root), 2, "菜单打开后宿主应有 内容+浮层 两个孩子");
        assert_eq!(om(), 1, "View 菜单应处于打开态");
        assert_eq!(page(), 0, "打开菜单不应改变页面");

        // 2) 菜单打开时点击菜单栏位置 → 命中浮层背景 → 点外部关闭
        let Some((t2, _, _)) = hit_test(root, vx, vy) else {
            panic!("菜单打开后命中失败")
        };
        assert!(dispatch_click_at(t2, 0.0, 0.0));
        recomp(root);
        assert_eq!(om(), -1, "点击外部应关闭菜单");
        assert_eq!(host_kids(root), 1, "关闭后浮层应从宿主移除（无幽灵节点）");
        assert_eq!(page(), 0, "关闭菜单不应改变页面");

        // 3) 关闭后菜单可再次打开（幽灵背景曾让第二次点击失效）
        assert!(click(root, vx, vy));
        recomp(root);
        assert_eq!(om(), 1, "关闭后再次点击应重新打开");

        // 入场动画（menu_panel 的 max_height 生长）需逐帧推进到 t≈1，面板才有实际高度可命中。
        for _ in 0..30 {
            advance_frame(1.0 / 60.0);
            recomp(root);
        }

        // 4) 点击 View 菜单第 4 项（page 3）→ 执行动作并关闭，页面正确跳转
        // 几何由库 menu_panel 决定：面板左 = 锚点左（idx*MENU_W），顶 = 锚点下沿 + gap 4，
        // 上内边距 SPACE_MD 12，项高 MENU_ITEM_HEIGHT 48 → 第 4 项中心 y = 顶 + 12 + 3*48 + 24。
        let ix = 1.0 * MENU_W + 30.0; // 面板内左侧（项 fill_max_width）
        let iy = TITLE_H + MENU_H + 4.0 + 12.0 + 3.0 * 48.0 + 24.0; // 第 4 项中心
        assert!(click(root, ix, iy), "菜单项点击应被消费");
        recomp(root);
        assert_eq!(om(), -1, "选择菜单项后应关闭");
        assert_eq!(page(), 3, "应跳到所选页面");
        assert_eq!(host_kids(root), 1, "浮层应完全移除");
    }
}
