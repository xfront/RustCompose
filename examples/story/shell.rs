//! 应用外壳：主题 / 顶部标题栏与菜单栏 / 侧栏导航 / 状态栏 / 页面容器。

use crate::common::*;
use crate::pages::{GROUPS, PAGES, build_page_body};
use crate::prelude::*;

/// 探针状态（仅测试构建）：外壳每趟组合后镜像 open_menu / page 供断言。
#[cfg(test)]
use crate::probe_state;

const MENUS: &[&str] = &["File", "View", "Theme", "Help"];

pub(crate) const MENU_W: f32 = 76.0;

pub(crate) const MENU_H: f32 = 36.0;

pub(crate) const TITLE_H: f32 = 48.0; // 标题栏高度（下拉按窗口坐标定位到菜单栏下方）

const CLOSED: i8 = -1;

/// 菜单栏下拉浮层的注册键。
const MENU_KEY: u64 = 0x4D45_4E55;

#[derive(Clone, Copy)]
pub(crate) struct StoryApp {
    pub(crate) dark: bool,
    pub(crate) palette: usize,
}

/// 色板 0 保持 M3 baseline；其余按种子色生成（品牌色替换、中性面带轻色调）。
pub(crate) fn scheme_for(palette: usize, dark: bool) -> ColorScheme {
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
pub(crate) fn initial_page() -> usize {
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

pub(crate) fn story_shell(dark: MutableState<bool>, palette: MutableState<usize>) {
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
    set_shell_rail(sidebar_rail);

    // ── VS2022 IDE 工作区：侧栏本身注册为可停靠面板 ──────────────────────
    // 注册只做一次（面板 id 与内容闭包跨重组稳定），随后把句柄共享给 Docking 页。
    let ids = remember(|| {
        clear_dock_panels();
        let sidebar = register_dock_panel(
            DockablePanel::with_options(
                "解决方案资源管理器",
                DockableOptions {
                    closable: false, // 导航是应用骨架，不允许关闭（可自动隐藏 / 浮动）
                    ..Default::default()
                },
            ),
            move || {
                let colors = theme_colors();
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .background(colors.surface_container_low, 0.0)
                        .padding(4.0),
                    Arrangement::Start,
                    CrossAxisAlignment::Stretch,
                    move || {
                        if sidebar_rail.get() {
                            Row(
                                Modifier::new().fill_max_width().height(52.0),
                                Arrangement::Center,
                                CrossAxisAlignment::Center,
                                move || {
                                    IconButton(
                                        IconName::ChevronRight,
                                        Modifier::new(),
                                        move || sidebar_rail.set(false),
                                    );
                                },
                            );
                        } else {
                            SidebarHeader("Storybook", false, Modifier::new(), move || {
                                sidebar_rail.set(true);
                            });
                            Spacer(Modifier::new().height(4.0));
                        }
                        // 两种模式都必须渲染 Sidebar：rail 模式下由它内部绘制图标胶囊
                        // （此前误放进 else，导致「最小化」后整个图标条不可见）。
                        Sidebar(
                            &nav_sections(),
                            page,
                            sidebar_collapsed,
                            sidebar_rail,
                            Modifier::new().fill_max_height(),
                        );
                    },
                );
            },
        );
        let toolbox = register_dock_panel(DockablePanel::new("工具箱"), move || {
            let colors = theme_colors();
            Column(
                Modifier::new().fill_max_size().padding(10.0),
                Arrangement::Start,
                CrossAxisAlignment::Start,
                move || {
                    for name in ["Button", "Text", "CheckBox", "Slider", "Card", "Chart"] {
                        Text(
                            name.to_string(),
                            TextStyle::new().size(12.0).color(colors.on_surface_variant),
                            Modifier::new().padding_hv(0.0, 4.0),
                        );
                    }
                },
            );
        });
        let documents = register_dock_panel(
            DockablePanel::with_options(
                "文档",
                DockableOptions {
                    closable: false, // 关掉就看不到任何页面了，用「重置窗口布局」还原
                    tab_preference: TabPreference::TopAlways,
                    ..Default::default()
                },
            ),
            move || {
                VerticalScroll(
                    Modifier::new().fill_max_size().padding_hv(4.0, 2.0),
                    Arrangement::Start,
                    CrossAxisAlignment::Stretch,
                    move || build_page_body(page),
                );
            },
        );
        let output = register_dock_panel(
            DockablePanel::with_options(
                "输出",
                DockableOptions {
                    tab_preference: TabPreference::BottomAlways,
                    ..Default::default()
                },
            ),
            move || {
                let colors = theme_colors();
                Text(
                    "> cargo build --workspace\n   Compiling compose v0.1.0\n    Finished dev profile",
                    TextStyle::new().size(11.5).color(colors.on_surface_variant),
                    Modifier::new().padding(10.0),
                );
            },
        );
        let errors = register_dock_panel(
            DockablePanel::with_options(
                "错误列表",
                DockableOptions {
                    tab_preference: TabPreference::BottomAlways,
                    ..Default::default()
                },
            ),
            move || {
                let colors = theme_colors();
                Text(
                    "0 个错误 · 0 个警告 · 0 条消息",
                    TextStyle::new().size(11.5).color(colors.on_surface_variant),
                    Modifier::new().padding(10.0),
                );
            },
        );
        let properties = register_dock_panel(DockablePanel::new("属性"), move || {
            let colors = theme_colors();
            let idx = page.get();
            Column(
                Modifier::new().fill_max_size().padding(10.0),
                Arrangement::Start,
                CrossAxisAlignment::Start,
                move || {
                    Text(
                        format!("页面: {}", PAGES[idx].name),
                        TextStyle::new().size(12.0).color(colors.on_surface),
                        Modifier::new(),
                    );
                    Text(
                        format!("分组: {} · 第 {} / {} 页", page_group(idx), idx + 1, PAGES.len()),
                        TextStyle::new().size(11.5).color(colors.on_surface_variant),
                        Modifier::new().padding_hv(0.0, 4.0),
                    );
                },
            );
        });
        ShellDockIds {
            sidebar,
            toolbox,
            documents,
            output,
            errors,
            properties,
        }
    });
    let dock_state = remember_dock_state(DockModel::new(ide_default_tree(ids)));
    set_shell_dock(dock_state, ids);

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
                        Modifier::new()
                            .fill_max_size()
                            .padding_ltrb(10.0, 8.0, 10.0, 8.0),
                        Arrangement::Start,
                        CrossAxisAlignment::Stretch,
                        move || {
                            let sn = snack;
                            // VS2022 工作区：侧栏（解决方案资源管理器）就是左侧 DockPanel，
                            // 文档 / 工具箱 / 输出 / 错误列表 / 属性 同属这一棵可拖拽停靠树。
                            // 全 story 只此一个 DockLayout 实例（命中表是 thread_local 单例）。
                            // padding 放外层：ROOT_NODE 矩形精确等于工作区，5 区判定不偏移。
                            DockLayout(dock_state, Modifier::new().fill_max_size());
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

pub(crate) fn page_group(idx: usize) -> &'static str {
    GROUPS
        .iter()
        .find(|g| g.1.contains(&idx))
        .map(|g| g.0)
        .unwrap_or("")
}

pub(crate) fn story_title_bar(dark: MutableState<bool>) {
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
pub(crate) fn ribbon_bar(dark: MutableState<bool>, palette: MutableState<usize>) {
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
pub(crate) fn mode_segment(icon_name: IconName, selected: bool, on_click: impl FnMut() + 'static) {
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

pub(crate) fn menu_bar(open_menu: MutableState<i8>) {
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

pub(crate) fn dropdown_overlay(
    open_menu: MutableState<i8>,
    dark: MutableState<bool>,
    page: MutableState<usize>,
    palette: MutableState<usize>,
) {
    let idx = open_menu.get();
    let items: Vec<String> = match idx {
        0 => vec!["New".into(), "Open".into(), "Exit".into()],
        1 => {
            // VS2022「视图」菜单末尾提供「重置窗口布局」：面板被拖乱/关闭后可一键还原。
            let mut v: Vec<String> = PAGES.iter().map(|p| p.name.to_string()).collect();
            v.push("重置窗口布局".into());
            v
        }
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

pub(crate) fn run_menu_action(
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
            } else if let (Some(state), Some(ids)) = (shell_dock(), shell_dock_ids()) {
                // 「重置窗口布局」：恢复 VS2022 默认工作区（面板重新归位）
                state.reset(ide_default_tree(ids));
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

pub(crate) fn story_status_bar(page: MutableState<usize>, palette: MutableState<usize>, dark: MutableState<bool>) {
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
