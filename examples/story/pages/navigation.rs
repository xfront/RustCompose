//! Navigation 分组的页面实现。

use crate::common::*;
use crate::prelude::*;

pub(crate) fn page_ribbon() {
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

pub(crate) fn page_tabs() {
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

pub(crate) fn page_stepper() {
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

pub(crate) fn page_expansion() {
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

pub(crate) fn page_nav_rail() {
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
        Modifier::new(),
        None,
    );
    let _ = colors;
}

pub(crate) fn page_page_view() {
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

pub(crate) fn page_drawer() {
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

pub(crate) fn page_accordion() {
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

pub(crate) fn page_breadcrumb() {
    section("Breadcrumb");
    Breadcrumb(
        &[
            "Home".to_string(),
            "Library".to_string(),
            "Data".to_string(),
        ],
        Modifier::new().fill_max_width(),
        None,
    );
}

pub(crate) fn page_pagination() {
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

pub(crate) fn page_toolbar() {
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

pub(crate) fn page_status_bar_demo() {
    section("StatusBar");
    StatusBar(
        "Ready",
        "RustCompose",
        Modifier::new().fill_max_width().height(24.0),
    );
}

pub(crate) fn page_title_bar_demo() {
    section("TitleBar");
    TitleBar(
        "RustCompose Gallery",
        Modifier::new().fill_max_width().height(38.0),
    );
}

pub(crate) fn page_tab_bar() {
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

pub(crate) fn page_navigator() {
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
