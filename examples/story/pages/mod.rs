//! 页面注册表与分发器：`PAGES` 是「标题/图标/说明/渲染函数」的唯一真源。

use crate::prelude::*;
use crate::shell::page_group;

mod advanced;
mod basics;
mod data;
mod demos;
mod feedback;
mod layout;
mod navigation;

use self::advanced::*;
use self::basics::*;
use self::data::*;
use self::demos::*;
use self::feedback::*;
use self::layout::*;
use self::navigation::*;

/// 单页元数据：标题 + 侧栏图标 + 页头一句话说明。
///
/// 过去 `PAGES` / `PAGE_ICONS` / `PAGE_BLURBS` 是三条按下标并行的数组，极易走样
/// （曾出现 `PAGE_ICONS` 比 `PAGES` 少两项，末两页从侧栏 `zip` 中丢失、选中即越界）。
/// 合并为一条按页对齐的记录后，三者天然同序、不可能再错位。
pub(crate) struct PageInfo {
    pub(crate) name: &'static str,
    pub(crate) icon: IconName,
    pub(crate) blurb: &'static str,
    /// 该页正文渲染函数引用：PAGES 因此成为「标题/图标/说明/渲染」的唯一真源，
    /// 不再需要按下标分发的 `dispatch_demo` 大 match（曾出现下标与函数错位的隐患）。
    pub(crate) builder: fn(),
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

pub(crate) const PAGES: &[PageInfo] = &[
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

pub(crate) const GROUPS: &[(&str, std::ops::Range<usize>)] = &[
    ("Demos", 0..10),
    ("Basics", 10..28),
    ("Data Display", 28..44),
    ("Layout", 44..57),
    ("Navigation", 57..73),
    ("Feedback", 73..89),
    ("Advanced", 89..103),
];

pub(crate) fn page_blurb(idx: usize) -> &'static str {
    PAGES.get(idx).map(|p| p.blurb).unwrap_or("")
}

pub(crate) fn build_page_body(page: MutableState<usize>) {
    let idx = page.get();
    build_page_title(idx);
    Spacer(Modifier::new().height(16.0));
    (PAGES[idx].builder)();
}

/// 页头 hero：渐变底 + 大号水印图标 + 分组徽标 / 标题 / 一句话说明。
pub(crate) fn build_page_title(idx: usize) {
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
