//! 回归用例：通用图表引擎（plot 基元 + 21 种图型渲染）。
//!
//! 注：`Chart` 取数据所有权，故测试用 **fn 指针构造器**（Copy，可在嵌套 `move`
//! 闭包中按拷贝捕获）把数据造在最内层闭包里。

use compose::foundation::Constraints;
use compose::layout::{create_root_node, draw_tree, Alignment, DrawCmd, Modifier};
use compose::material::{MaterialTheme, ColorScheme};
use compose::runtime::Composer;
use compose_material::{
    labeled_items, linear_ticks, nice_linear_domain, nice_step, series_palette, Candle, Chart,
    ChartData, ChartOptions, ChartSeries, ChartType, DataPoint, FunnelSlice, PieSlice,
    SankeyLink, SankeyNode, ScaleBand, ScaleLinear, ScalePoint, WaterfallEntry, WaterfallKind,
};

// ══════════════════════ plot 基元 ══════════════════════

#[test]
fn scale_linear_maps_value_to_pixel_and_back() {
    let s = ScaleLinear::new([0.0, 100.0], [200.0, 0.0]);
    assert!((s.position_of(0.0) - 200.0).abs() < 1e-3);
    assert!((s.position_of(100.0) - 0.0).abs() < 1e-3);
    assert!((s.position_of(50.0) - 100.0).abs() < 1e-3);
    assert!((s.value_at(100.0) - 50.0).abs() < 1e-3);
    // 退化值域不应产生 NaN
    let d = ScaleLinear::new([5.0, 5.0], [0.0, 10.0]);
    assert!(d.position_of(5.0).is_finite());
}

#[test]
fn scale_band_hits_categories() {
    let b = ScaleBand::new(4, [0.0, 400.0]).padding_inner(0.0).padding_outer(0.0);
    assert!(b.band_width() > 0.0);
    assert_eq!(b.index_for(0.0), 0);
    assert_eq!(b.index_for(150.0), 1);
    assert_eq!(b.index_for(399.0), 3);
    // 越界钳制
    assert_eq!(b.index_for(-50.0), 0);
    assert_eq!(b.index_for(9999.0), 3);
}

#[test]
fn scale_point_nearest_index() {
    let p = ScalePoint::new(5, [0.0, 400.0]);
    assert!((p.position_of(0) - 0.0).abs() < 1e-3);
    assert!((p.position_of(4) - 400.0).abs() < 1e-3);
    assert_eq!(p.nearest_index(190.0), 2);
    assert_eq!(p.nearest_index(-100.0), 0);
    assert_eq!(p.nearest_index(9999.0), 4);
}

#[test]
fn tick_helpers_behave() {
    assert!(nice_step(7.0) >= 5.0 && nice_step(7.0) <= 10.0);
    assert!(nice_step(0.0) >= 1.0);
    let (lo, hi) = nice_linear_domain(3.0, 97.0, 4);
    assert!(lo <= 3.0, "lo={lo}");
    assert!(hi >= 97.0, "hi={hi}");
    assert!(hi > lo);
    assert_eq!(linear_ticks(0.0, 10.0, 5).len(), 6);
    assert_eq!(compose_material::format_tick(12.0), "12");
    assert_eq!(labeled_items(10, 3).len(), 3);
    assert!(labeled_items(0, 3).is_empty());
    // 调色板循环取色
    assert_eq!(series_palette(0), series_palette(8));
}

// ══════════════════════ 图表渲染 ══════════════════════

/// 数据构造器（fn 指针 → Copy，可在最内层闭包中调用）。
type Builder = fn() -> (ChartData, ChartOptions);

fn series_data() -> ChartData {
    ChartData::series(vec![
        ChartSeries::of("线上", vec![(0.0, 32.0), (1.0, 41.0), (2.0, 38.0), (3.0, 52.0)]),
        ChartSeries::of("门店", vec![(0.0, 20.0), (1.0, 28.0), (2.0, 34.0), (3.0, 30.0)]),
    ])
}

fn b_series() -> (ChartData, ChartOptions) {
    (series_data(), ChartOptions::default())
}

fn b_series_labeled() -> (ChartData, ChartOptions) {
    (
        series_data(),
        ChartOptions::default()
            .title("季度销量")
            .data_labels(true)
            .x_labels(vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()]),
    )
}

fn b_hbar() -> (ChartData, ChartOptions) {
    (
        ChartData::series(vec![ChartSeries::of("耗时", vec![(0.0, 32.0), (1.0, 18.0)])]),
        ChartOptions::default(),
    )
}

fn b_range() -> (ChartData, ChartOptions) {
    (
        ChartData::series(vec![ChartSeries::auto(
            "区间",
            vec![
                DataPoint { x: 0.0, y: 10.0, size: 30.0 },
                DataPoint { x: 1.0, y: 20.0, size: 45.0 },
            ],
        )]),
        ChartOptions::default(),
    )
}

fn b_hist() -> (ChartData, ChartOptions) {
    (
        ChartData::series(vec![ChartSeries::auto(
            "样本",
            vec![
                DataPoint::new(0.0, 12.0),
                DataPoint::new(0.0, 25.0),
                DataPoint::new(0.0, 33.0),
                DataPoint::new(0.0, 47.0),
            ],
        )]),
        ChartOptions::default().bins(4),
    )
}

fn b_waterfall() -> (ChartData, ChartOptions) {
    (
        ChartData::waterfall(vec![
            WaterfallEntry::new("期初", 40.0, WaterfallKind::Total),
            WaterfallEntry::new("新增", 15.0, WaterfallKind::Increase),
            WaterfallEntry::new("流失", 8.0, WaterfallKind::Decrease),
        ]),
        ChartOptions::default(),
    )
}

fn b_pie() -> (ChartData, ChartOptions) {
    (
        ChartData::pie(vec![
            PieSlice::new("A", 35.0, compose::foundation::Color::TRANSPARENT),
            PieSlice::new("B", 28.0, compose::foundation::Color::TRANSPARENT),
        ]),
        ChartOptions::default(),
    )
}

fn b_funnel() -> (ChartData, ChartOptions) {
    (
        ChartData::funnel(vec![
            FunnelSlice {
                label: "访问".into(),
                value: 100.0,
                color: compose::foundation::Color::TRANSPARENT,
            },
            FunnelSlice {
                label: "注册".into(),
                value: 62.0,
                color: compose::foundation::Color::TRANSPARENT,
            },
        ]),
        ChartOptions::default(),
    )
}

fn b_scatter() -> (ChartData, ChartOptions) {
    (
        ChartData::series(vec![ChartSeries::of(
            "点",
            vec![(1.0, 24.0), (2.0, 38.0), (3.0, 55.0)],
        )]),
        ChartOptions::default(),
    )
}

fn b_bubble() -> (ChartData, ChartOptions) {
    (
        ChartData::series(vec![ChartSeries::auto(
            "泡",
            vec![
                DataPoint::bubble(1.0, 24.0, 1.5),
                DataPoint::bubble(2.0, 38.0, 2.6),
            ],
        )]),
        ChartOptions::default(),
    )
}

fn b_radar() -> (ChartData, ChartOptions) {
    (
        ChartData::series(vec![ChartSeries::of(
            "本季",
            vec![(0.0, 80.0), (1.0, 65.0), (2.0, 90.0), (3.0, 55.0)],
        )]),
        ChartOptions::default(),
    )
}

fn b_radial() -> (ChartData, ChartOptions) {
    (
        ChartData::series(vec![ChartSeries::of("达成", vec![(0.0, 82.0), (1.0, 64.0)])]),
        ChartOptions::default(),
    )
}

fn b_candle() -> (ChartData, ChartOptions) {
    (
        ChartData::candle(vec![
            Candle::new("D1", 10.0, 12.5, 13.2, 9.4),
            Candle::new("D2", 12.5, 11.8, 13.6, 11.2),
        ]),
        ChartOptions::default(),
    )
}

fn b_sankey() -> (ChartData, ChartOptions) {
    (
        ChartData::sankey(
            vec![
                SankeyNode { label: "首页".into(), value: 100.0 },
                SankeyNode { label: "文档".into(), value: 60.0 },
            ],
            vec![SankeyLink { source: 0, target: 1, value: 60.0 }],
        ),
        ChartOptions::default(),
    )
}

fn b_empty() -> (ChartData, ChartOptions) {
    (ChartData::series(vec![]), ChartOptions::default())
}

fn render(ctype: ChartType, build: Builder) -> Vec<DrawCmd> {
    let root = create_root_node(Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            // 数据在最内层闭包内构造（fn 指针为 Copy，可按拷贝捕获）
            let (d, o) = build();
            Chart(ctype, d, o, Modifier::new().fill_max_width());
        });
    });
    compose::layout::perform_layout(root, Constraints::tight(360.0, 300.0));
    draw_tree(root)
}

#[test]
fn every_chart_type_renders_without_panic() {
    let cases: Vec<(ChartType, Builder)> = vec![
        (ChartType::Line, b_series),
        (ChartType::Spline, b_series),
        (ChartType::Step, b_series),
        (ChartType::Area, b_series),
        (ChartType::StackedArea100, b_series),
        (ChartType::Bar, b_series),
        (ChartType::StackedBar, b_series),
        (ChartType::GroupedBar, b_series),
        (ChartType::HorizontalBar, b_hbar),
        (ChartType::RangeBar, b_range),
        (ChartType::Histogram, b_hist),
        (ChartType::Waterfall, b_waterfall),
        (ChartType::Pie, b_pie),
        (ChartType::Donut, b_pie),
        (ChartType::Funnel, b_funnel),
        (ChartType::Scatter, b_scatter),
        (ChartType::Bubble, b_bubble),
        (ChartType::Radar, b_radar),
        (ChartType::RadialBar, b_radial),
        (ChartType::Candlestick, b_candle),
        (ChartType::Sankey, b_sankey),
    ];
    assert_eq!(cases.len(), 21, "应覆盖 21 种图型");
    for (t, b) in cases {
        let cmds = render(t, b);
        assert!(!cmds.is_empty(), "{t:?} 应产生绘制指令");
    }
}

#[test]
fn chart_renders_axis_and_category_labels() {
    let cmds = render(ChartType::Bar, b_series_labeled);
    let texts: Vec<&str> = cmds
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| *t == "季度销量"), "应绘制标题: {texts:?}");
    assert!(texts.iter().any(|t| *t == "Q1"), "应绘制 X 轴类目标签: {texts:?}");
    assert!(texts.iter().any(|t| *t == "线上"), "应绘制图例: {texts:?}");
}

#[test]
fn empty_data_does_not_panic() {
    let cmds = render(ChartType::Bar, b_empty);
    let _ = cmds;
}
