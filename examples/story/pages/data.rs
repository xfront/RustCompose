//! Data Display 分组的页面实现。

use crate::common::*;
use crate::prelude::*;

pub(crate) fn page_searchable_list() {
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

pub(crate) fn page_grid_sheet() {
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

pub(crate) fn page_cards() {
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

pub(crate) fn page_chips() {
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

pub(crate) fn page_list() {
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
        sel2,
        Modifier::new().fill_max_width(),
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
    );
}

pub(crate) fn page_list_view() {
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
        sel2,
        Modifier::new()
            .fill_max_width()
            .height(200.0)
            .border(1.0, colors.outline_variant, 12.0),
        move |i| {
            Text(
                &items2[i],
                TextStyle::new(),
                Modifier::new().padding_ltrb(16.0, 8.0, 16.0, 8.0),
            );
        },
    );
}

pub(crate) fn page_grid() {
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

pub(crate) fn page_tree() {
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

pub(crate) fn page_table() {
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

pub(crate) fn page_color() {
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

pub(crate) fn page_bubble() {
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

pub(crate) fn page_charts() {
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

pub(crate) fn page_description() {
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

pub(crate) fn page_diff() {
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

pub(crate) fn page_markdown() {
    section("Markdown");
    Markdown(
        "# Title\n\n- item **bold**\n- item *italic*\n\n```\ncode block\n```",
        Modifier::new().fill_max_width(),
    );
}

// ════════════════════════════ Layout ════════════════════════════
