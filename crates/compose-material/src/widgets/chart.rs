//! 通用图表引擎（通用 `Chart` + `plot` 基元）。
//!
//! 设计取舍：不同于 builder + setter（`Rc<RefCell<..>>` 持有可变状态）模式，本框架是
//! 「组合函数 + 不可变数据」模型，故**数据以结构体传入、选项用 `ChartOptions` 链式构建**。
//!
//! 覆盖图型：
//! * 折线族 — `Line` / `Area` / `Spline` / `Step` / `StackedArea100`
//! * 柱族 — `Bar` / `StackedBar` / `GroupedBar` / `HorizontalBar` / `RangeBar` /
//!   `Histogram` / `Waterfall`
//! * 饼族 — `Pie` / `Donut` / `Funnel`
//! * 点族 — `Scatter` / `Bubble`
//! * 其它 — `Radar` / `RadialBar` / `Candlestick` / `Sankey`
//!
//! 全部图型共享：网格线、Y 轴刻度、X 轴类目标签、数据标签、底部图例。

use compose::foundation::Color;
use compose::layout::{Canvas, DrawBuilder, Modifier, measure_text_with_measurer};
use compose::material::{ColorScheme, theme_colors};

use crate::tokens;

// ══════════════════════ plot 基元（比例尺 / 刻度 / 命中） ══════════════════════

/// 线性比例尺：值域 → 像素域。
#[derive(Clone, Copy, Debug)]
pub struct ScaleLinear {
    domain_start: f32,
    domain_diff: f32,
    range_start: f32,
    range_diff: f32,
}

impl ScaleLinear {
    /// `domain` = [最小值, 最大值]；`range` = [起点, 终点]（像素）。
    pub fn new(domain: [f32; 2], range: [f32; 2]) -> Self {
        let diff = domain[1] - domain[0];
        Self {
            domain_start: domain[0],
            domain_diff: if diff.abs() < 1e-6 { 1.0 } else { diff },
            range_start: range[0],
            range_diff: range[1] - range[0],
        }
    }

    /// 值 → 位置。
    pub fn position_of(&self, v: f32) -> f32 {
        self.range_start + (v - self.domain_start) / self.domain_diff * self.range_diff
    }

    /// 位置 → 值。
    pub fn value_at(&self, p: f32) -> f32 {
        if self.range_diff.abs() < 1e-6 {
            return self.domain_start;
        }
        self.domain_start + (p - self.range_start) / self.range_diff * self.domain_diff
    }

    /// 刻度值序列。
    pub fn ticks(&self, count: usize) -> Vec<f32> {
        linear_ticks(
            self.domain_start,
            self.domain_start + self.domain_diff,
            count,
        )
    }
}

/// 频带比例尺（类目轴）：每个类目占一段频带。
#[derive(Clone, Copy, Debug)]
pub struct ScaleBand {
    range_start: f32,
    range_diff: f32,
    count: usize,
    padding_inner: f32,
    padding_outer: f32,
    max_band_width: Option<f32>,
}

impl ScaleBand {
    pub fn new(count: usize, range: [f32; 2]) -> Self {
        Self {
            range_start: range[0],
            range_diff: range[1] - range[0],
            count,
            padding_inner: 0.1,
            padding_outer: 0.1,
            max_band_width: None,
        }
    }

    /// 类目之间留白比例（0~1）。
    pub fn padding_inner(mut self, p: f32) -> Self {
        self.padding_inner = p.clamp(0.0, 0.95);
        self
    }

    /// 两端留白比例（0~1）。
    pub fn padding_outer(mut self, p: f32) -> Self {
        self.padding_outer = p.clamp(0.0, 0.95);
        self
    }

    /// 单段最大宽度。
    pub fn max_band_width(mut self, m: f32) -> Self {
        self.max_band_width = Some(m);
        self
    }

    /// 类目步距。
    pub fn step(&self) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let n = self.count as f32;
        let s = self.range_diff / (n - self.padding_inner + 2.0 * self.padding_outer).max(0.001);
        match self.max_band_width {
            Some(m) => s.min(m),
            None => s,
        }
    }

    /// 单段宽度。
    pub fn band_width(&self) -> f32 {
        (self.step() * (1.0 - self.padding_inner)).max(0.0)
    }

    /// 第 i 段起点。
    pub fn band_start(&self, i: usize) -> f32 {
        self.range_start + self.padding_outer * self.step() + i as f32 * self.step()
    }

    /// 第 i 段中心。
    pub fn band_center(&self, i: usize) -> f32 {
        self.band_start(i) + self.band_width() * 0.5
    }

    /// 命中：像素位置 → 类目下标。
    pub fn index_for(&self, pos: f32) -> usize {
        if self.count == 0 {
            return 0;
        }
        let s = self.step();
        if s <= 0.0 {
            return 0;
        }
        let raw = ((pos - self.range_start - self.padding_outer * s) / s).floor() as i64;
        raw.clamp(0, self.count as i64 - 1) as usize
    }
}

/// 点比例尺（等距类目，用于折线类）。
#[derive(Clone, Copy, Debug)]
pub struct ScalePoint {
    range_start: f32,
    range_diff: f32,
    count: usize,
}

impl ScalePoint {
    pub fn new(count: usize, range: [f32; 2]) -> Self {
        Self {
            range_start: range[0],
            range_diff: range[1] - range[0],
            count,
        }
    }

    /// 第 i 个点的位置。
    pub fn position_of(&self, i: usize) -> f32 {
        if self.count <= 1 {
            return self.range_start + self.range_diff * 0.5;
        }
        self.range_start + self.range_diff * (i as f32 / (self.count - 1) as f32)
    }

    /// 最近点下标（命中用）。
    pub fn nearest_index(&self, pos: f32) -> usize {
        if self.count == 0 {
            return 0;
        }
        if self.count == 1 || self.range_diff.abs() < 1e-6 {
            return 0;
        }
        let t = ((pos - self.range_start) / self.range_diff) * (self.count - 1) as f32;
        (t.round().max(0.0) as usize).min(self.count - 1)
    }
}

/// 「漂亮」的刻度步长（1/2/5 × 10ⁿ）。
pub fn nice_step(rough: f32) -> f32 {
    if rough <= 0.0 || !rough.is_finite() {
        return 1.0;
    }
    let exp = rough.log10().floor();
    let base = 10f32.powf(exp);
    let f = rough / base;
    let m = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    m * base
}

/// 线性刻度值（min..max 均分 count 段）。
pub fn linear_ticks(min: f32, max: f32, count: usize) -> Vec<f32> {
    let n = count.max(1);
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..=n {
        out.push(min + (max - min) * (i as f32 / n as f32));
    }
    out
}

/// 取整后的「漂亮」值域 `(min, max)`。
pub fn nice_linear_domain(min: f32, max: f32, count: usize) -> (f32, f32) {
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    let (lo, hi) = if min > max { (max, min) } else { (min, max) };
    if (hi - lo).abs() < 1e-6 {
        return (lo, lo + 1.0);
    }
    let step = nice_step((hi - lo) / count.max(1) as f32);
    let nlo = (lo / step).floor() * step;
    let nhi = (hi / step).ceil() * step;
    (nlo, nhi)
}

/// 刻度文本（整数去小数；大数用 k 缩写）。
pub fn format_tick(v: f32) -> String {
    if !v.is_finite() {
        return "—".to_string();
    }
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else if v.abs() >= 1000.0 {
        format!("{:.1}k", v / 1000.0)
    } else {
        format!("{:.1}", v)
    }
}

/// 稀疏标注的类目下标（等距抽稀）。
pub fn labeled_items(len: usize, label_count: usize) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let want = label_count.max(1).min(len);
    let mut out = Vec::with_capacity(want);
    for i in 0..want {
        out.push(i * (len - 1) / (want - 1).max(1));
    }
    out.dedup();
    out
}

/// 系列调色板（Material 3 色调）。
pub fn series_palette(index: usize) -> Color {
    let p = [
        Color::hex(0x6750A4),
        Color::hex(0x00639B),
        Color::hex(0x8F4C38),
        Color::hex(0x3F6837),
        Color::hex(0x7D5260),
        Color::hex(0x625B71),
        Color::hex(0xB58310),
        Color::hex(0x006A6A),
    ];
    p[index % p.len()]
}

// ══════════════════════ 图表数据结构 ══════════════════════

/// 图型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChartType {
    /// 折线。
    Line,
    /// 面积（半透明填充 + 描边）。
    Area,
    /// 平滑曲线（Catmull-Rom 近似）。
    Spline,
    /// 阶梯线。
    Step,
    /// 100% 堆叠面积。
    StackedArea100,
    /// 柱状。
    Bar,
    /// 堆叠柱。
    StackedBar,
    /// 分组柱。
    GroupedBar,
    /// 横向柱。
    HorizontalBar,
    /// 区间柱（低—高）。
    RangeBar,
    /// 直方图（按 bin 聚合原始样本）。
    Histogram,
    /// 瀑布图。
    Waterfall,
    /// 饼图。
    Pie,
    /// 环形图。
    Donut,
    /// 漏斗图。
    Funnel,
    /// 散点图。
    Scatter,
    /// 气泡图（点大小随 size 变化）。
    Bubble,
    /// 雷达图。
    Radar,
    /// 径向柱图。
    RadialBar,
    /// K 线图（OHLC）。
    Candlestick,
    /// 桑基图。
    Sankey,
}

/// 数据点（`x` 为类目下标或数值；`size` 为气泡因子）。
#[derive(Clone, Copy, Debug)]
pub struct DataPoint {
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

impl DataPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y, size: 1.0 }
    }

    /// 气泡点（`size` 越大点越大）。
    pub fn bubble(x: f32, y: f32, size: f32) -> Self {
        Self { x, y, size }
    }
}

/// 数据系列。
#[derive(Clone, Debug)]
pub struct ChartSeries {
    pub label: String,
    /// 颜色；`Color::TRANSPARENT` 表示按调色板自动取色。
    pub color: Color,
    pub points: Vec<DataPoint>,
}

impl ChartSeries {
    pub fn new(label: impl Into<String>, color: Color, points: Vec<DataPoint>) -> Self {
        Self {
            label: label.into(),
            color,
            points,
        }
    }

    /// 自动配色系列。
    pub fn auto(label: impl Into<String>, points: Vec<DataPoint>) -> Self {
        Self {
            label: label.into(),
            color: Color::TRANSPARENT,
            points,
        }
    }

    /// 由 `(x, y)` 列表构造自动配色系列。
    pub fn of(label: impl Into<String>, points: Vec<(f32, f32)>) -> Self {
        Self::auto(
            label,
            points
                .into_iter()
                .map(|(x, y)| DataPoint::new(x, y))
                .collect(),
        )
    }
}

/// 饼 / 环切片。
#[derive(Clone, Debug)]
pub struct PieSlice {
    pub label: String,
    pub value: f32,
    pub color: Color,
}

impl PieSlice {
    pub fn new(label: impl Into<String>, value: f32, color: Color) -> Self {
        Self {
            label: label.into(),
            value,
            color,
        }
    }
}

/// K 线（OHLC）。
#[derive(Clone, Debug)]
pub struct Candle {
    pub label: String,
    pub open: f32,
    pub close: f32,
    pub high: f32,
    pub low: f32,
}

impl Candle {
    pub fn new(label: impl Into<String>, open: f32, close: f32, high: f32, low: f32) -> Self {
        Self {
            label: label.into(),
            open,
            close,
            high,
            low,
        }
    }
}

/// 瀑布条目类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaterfallKind {
    Increase,
    Decrease,
    Total,
}

/// 瀑布条目。
#[derive(Clone, Debug)]
pub struct WaterfallEntry {
    pub label: String,
    pub value: f32,
    pub kind: WaterfallKind,
}

impl WaterfallEntry {
    pub fn new(label: impl Into<String>, value: f32, kind: WaterfallKind) -> Self {
        Self {
            label: label.into(),
            value,
            kind,
        }
    }
}

/// 漏斗切片。
#[derive(Clone, Debug)]
pub struct FunnelSlice {
    pub label: String,
    pub value: f32,
    pub color: Color,
}

/// 桑基节点。
#[derive(Clone, Debug)]
pub struct SankeyNode {
    pub label: String,
    pub value: f32,
}

/// 桑基连线。
#[derive(Clone, Debug)]
pub struct SankeyLink {
    pub source: usize,
    pub target: usize,
    pub value: f32,
}

/// 图表数据载荷。
#[derive(Clone, Debug)]
pub enum ChartData {
    /// 系列（折线 / 柱 / 散点 / 面积 / 雷达 …）。
    Series(Vec<ChartSeries>),
    /// 饼 / 环切片。
    Pie(Vec<PieSlice>),
    /// K 线。
    Candle(Vec<Candle>),
    /// 瀑布条目。
    Waterfall(Vec<WaterfallEntry>),
    /// 漏斗切片。
    Funnel(Vec<FunnelSlice>),
    /// 桑基（节点 + 连线）。
    Sankey {
        nodes: Vec<SankeyNode>,
        links: Vec<SankeyLink>,
    },
}

impl ChartData {
    pub fn series(s: Vec<ChartSeries>) -> Self {
        ChartData::Series(s)
    }

    pub fn pie(s: Vec<PieSlice>) -> Self {
        ChartData::Pie(s)
    }

    pub fn candle(s: Vec<Candle>) -> Self {
        ChartData::Candle(s)
    }

    pub fn waterfall(s: Vec<WaterfallEntry>) -> Self {
        ChartData::Waterfall(s)
    }

    pub fn funnel(s: Vec<FunnelSlice>) -> Self {
        ChartData::Funnel(s)
    }

    pub fn sankey(nodes: Vec<SankeyNode>, links: Vec<SankeyLink>) -> Self {
        ChartData::Sankey { nodes, links }
    }
}

/// 图表选项（链式构建）。
#[derive(Clone, Debug)]
pub struct ChartOptions {
    /// 标题（绘制在顶部）。
    pub title: Option<String>,
    /// 网格线。
    pub show_grid: bool,
    /// 底部图例。
    pub show_legend: bool,
    /// 数据标签（数值标注）。
    pub show_data_labels: bool,
    /// 环形图内半径比例（0~0.95）。
    pub donut_ratio: f32,
    /// 柱/块圆角。
    pub corner_radius: f32,
    /// 类目间留白比例（0~0.9）。
    pub band_padding: f32,
    /// Y 轴下界（`None` = 自动）。
    pub y_min: Option<f32>,
    /// Y 轴上界（`None` = 自动）。
    pub y_max: Option<f32>,
    /// Y 轴刻度段数。
    pub y_tick_count: usize,
    /// X 轴类目标签（类目型图使用）。
    pub x_labels: Vec<String>,
    /// 自定义调色板（为空则用内置调色板）。
    pub palette: Vec<Color>,
    /// 直方图分箱数。
    pub histogram_bins: usize,
    /// 图表高度（px）。
    pub height: f32,
}

impl Default for ChartOptions {
    fn default() -> Self {
        Self {
            title: None,
            show_grid: true,
            show_legend: true,
            show_data_labels: false,
            donut_ratio: 0.55,
            corner_radius: 3.0,
            band_padding: 0.25,
            y_min: None,
            y_max: None,
            y_tick_count: 4,
            x_labels: Vec::new(),
            palette: Vec::new(),
            histogram_bins: 8,
            height: 240.0,
        }
    }
}

impl ChartOptions {
    /// 标题。
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = Some(t.into());
        self
    }

    /// 网格线开关。
    pub fn grid(mut self, on: bool) -> Self {
        self.show_grid = on;
        self
    }

    /// 图例开关。
    pub fn legend(mut self, on: bool) -> Self {
        self.show_legend = on;
        self
    }

    /// 数据标签开关。
    pub fn data_labels(mut self, on: bool) -> Self {
        self.show_data_labels = on;
        self
    }

    /// 环形图内半径比例。
    pub fn donut(mut self, ratio: f32) -> Self {
        self.donut_ratio = ratio.clamp(0.0, 0.95);
        self
    }

    /// X 轴类目标签。
    pub fn x_labels(mut self, labels: Vec<String>) -> Self {
        self.x_labels = labels;
        self
    }

    /// 自定义调色板。
    pub fn palette(mut self, p: Vec<Color>) -> Self {
        self.palette = p;
        self
    }

    /// 直方图分箱数。
    pub fn bins(mut self, n: usize) -> Self {
        self.histogram_bins = n.max(1);
        self
    }

    /// 图表高度（px）。
    pub fn height(mut self, h: f32) -> Self {
        self.height = h.max(40.0);
        self
    }
}

// ══════════════════════ 通用 Chart 控件 ══════════════════════

/// `Chart(chart_type, data, options, modifier)` — 通用图表。
///
/// 一个控件覆盖 21 种图型；`ChartOptions` 控制标题 / 网格 / 图例 / 数据标签 / 配色。
///
/// ```ignore
/// Chart(ChartType::Bar, ChartData::series(vec![ChartSeries::of("线上", vec![(0.0, 32.0)])]),
///       ChartOptions::default().title("月度销量").data_labels(true),
///       Modifier::new().fill_max_width().height(240.0));
/// ```
pub fn Chart(chart_type: ChartType, data: ChartData, opts: ChartOptions, modifier: Modifier) {
    let colors = theme_colors();
    let h = opts.height;
    Canvas(modifier.fill_max_width().height(h), move |b, x, y, w, h| {
        draw_chart(b, x, y, w, h, chart_type, &data, &opts, &colors);
    });
}

// ══════════════════════ 引擎内部 ══════════════════════

/// 坐标系框架（笛卡尔绘图区）。
struct Frame {
    x0: f32,
    pw: f32,
    ph: f32,
    base: f32,
    yscale: ScaleLinear,
    n_cat: usize,
}

/// 取色（系列显式色优先，否则调色板）。
fn pick_color(c: Color, index: usize, opts: &ChartOptions) -> Color {
    if c.a > 0.0 {
        return c;
    }
    if opts.palette.is_empty() {
        series_palette(index)
    } else {
        opts.palette[index % opts.palette.len()]
    }
}

/// 应用 Y 轴上下界覆盖。
fn apply_y_opts(lo: f32, hi: f32, opts: &ChartOptions) -> (f32, f32) {
    let lo = opts.y_min.unwrap_or(lo);
    let hi = opts.y_max.unwrap_or(hi);
    if (hi - lo).abs() < 1e-6 {
        (lo, lo + 1.0)
    } else {
        (lo, hi)
    }
}

/// 系列值域。
fn series_domain(series: &[ChartSeries]) -> (f32, f32) {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for s in series {
        for p in &s.points {
            lo = lo.min(p.y);
            hi = hi.max(p.y);
        }
    }
    if lo > hi { (0.0, 1.0) } else { (lo, hi) }
}

/// 主绘制分发。
fn draw_chart(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ctype: ChartType,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let mut top = y;
    if let Some(t) = &opts.title {
        b.draw_text(x, top, t, &tokens::title_medium(colors.on_surface), w);
        top += tokens::SPACE_XL;
    }
    let legend_h = if opts.show_legend {
        tokens::CHART_LEGEND_ROW_H
    } else {
        0.0
    };
    let plot_h = (h - (top - y) - legend_h).max(24.0);
    match ctype {
        ChartType::Pie | ChartType::Donut => {
            draw_pie(b, x, top, w, plot_h, ctype, data, opts, colors)
        }
        ChartType::Funnel => draw_funnel(b, x, top, w, plot_h, data, opts, colors),
        ChartType::Radar => draw_radar(b, x, top, w, plot_h, data, opts, colors),
        ChartType::RadialBar => draw_radial_bar(b, x, top, w, plot_h, data, opts, colors),
        ChartType::Sankey => draw_sankey(b, x, top, w, plot_h, data, opts, colors),
        ChartType::Scatter | ChartType::Bubble => {
            draw_xy(b, x, top, w, plot_h, ctype, data, opts, colors)
        }
        ChartType::HorizontalBar => draw_hbar(b, x, top, w, plot_h, data, opts, colors),
        ChartType::Candlestick => draw_candlestick(b, x, top, w, plot_h, data, opts, colors),
        ChartType::Waterfall => draw_waterfall(b, x, top, w, plot_h, data, opts, colors),
        ChartType::Histogram => draw_histogram(b, x, top, w, plot_h, data, opts, colors),
        _ => draw_cartesian(b, x, top, w, plot_h, ctype, data, opts, colors),
    }
    if opts.show_legend {
        let items = legend_items(data, opts);
        draw_legend(b, x, top + plot_h, w, &items, colors);
    }
}

/// 图例条目。
fn legend_items(data: &ChartData, opts: &ChartOptions) -> Vec<(String, Color)> {
    match data {
        ChartData::Series(s) => s
            .iter()
            .enumerate()
            .map(|(i, x)| (x.label.clone(), pick_color(x.color, i, opts)))
            .collect(),
        ChartData::Pie(s) => s
            .iter()
            .enumerate()
            .map(|(i, x)| (x.label.clone(), pick_color(x.color, i, opts)))
            .collect(),
        ChartData::Funnel(s) => s
            .iter()
            .enumerate()
            .map(|(i, x)| (x.label.clone(), pick_color(x.color, i, opts)))
            .collect(),
        ChartData::Candle(_) => vec![("OHLC".to_string(), series_palette(0))],
        ChartData::Waterfall(_) => vec![
            ("增加".to_string(), series_palette(3)),
            ("减少".to_string(), series_palette(2)),
            ("合计".to_string(), series_palette(0)),
        ],
        ChartData::Sankey { nodes, .. } => nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.label.clone(), series_palette(i)))
            .collect(),
    }
}

/// 底部图例。
fn draw_legend(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    items: &[(String, Color)],
    colors: &ColorScheme,
) {
    if items.is_empty() {
        return;
    }
    let style = tokens::label_small(colors.on_surface_variant);
    let sw = tokens::CHART_LEGEND_SWATCH;
    let row_h = tokens::CHART_LEGEND_ROW_H;
    let measured: Vec<(&String, Color, f32, f32)> = items
        .iter()
        .map(|(l, c)| {
            let (tw, th) = measure_text_with_measurer(l, &style);
            (l, *c, tw, th)
        })
        .collect();
    let mut total = 0.0;
    for (_, _, tw, _) in &measured {
        total += sw + tokens::SPACE_XS + tw + tokens::SPACE_LG;
    }
    let mut cx = x + (w - total).max(0.0) * 0.5;
    for (label, c, tw, th) in &measured {
        b.fill_rrect(
            cx,
            y + (row_h - sw) * 0.5,
            sw,
            sw,
            tokens::SHAPE_EXTRA_SMALL,
            *c,
        );
        b.draw_text(
            cx + sw + tokens::SPACE_XS,
            y + (row_h - th) * 0.5,
            label,
            &style,
            w,
        );
        cx += sw + tokens::SPACE_XS + tw + tokens::SPACE_LG;
    }
}

/// 绘制网格 + 坐标轴 + X 标签，返回绘图框架。
fn draw_frame(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ymin: f32,
    ymax: f32,
    n_cat: usize,
    labels: &[String],
    opts: &ChartOptions,
    colors: &ColorScheme,
) -> Frame {
    let gutter = 36.0;
    let bottom_pad = 20.0;
    let x0 = x + gutter;
    let y0 = y + 6.0;
    let pw = (w - gutter - 10.0).max(1.0);
    let ph = (h - 6.0 - bottom_pad).max(1.0);
    let base = y0 + ph;
    let yscale = ScaleLinear::new([ymin, ymax], [base, y0]);

    if opts.show_grid {
        let n = opts.y_tick_count.max(1);
        for i in 0..=n {
            let tv = ymin + (ymax - ymin) * (i as f32 / n as f32);
            let gy = yscale.position_of(tv);
            b.line(x0, gy, x0 + pw, gy, colors.outline_variant, 1.0);
            b.draw_text(
                x + 2.0,
                gy + 3.0,
                &format_tick(tv),
                &tokens::label_small(colors.on_surface_variant),
                gutter - 4.0,
            );
        }
    }
    b.line(x0, y0, x0, base, colors.outline, 1.0);
    b.line(x0, base, x0 + pw, base, colors.outline, 1.0);

    if !labels.is_empty() && n_cat > 0 {
        let step = pw / n_cat as f32;
        let want = ((pw / 64.0).floor() as usize + 1).max(1);
        for i in labeled_items(n_cat, want) {
            if i >= labels.len() {
                break;
            }
            b.draw_text(
                x0 + i as f32 * step,
                base + 4.0,
                &labels[i],
                &tokens::label_small(colors.on_surface_variant),
                step,
            );
        }
    }

    Frame {
        x0,
        pw,
        ph,
        base,
        yscale,
        n_cat,
    }
}

/// 笛卡尔图型（折线 / 面积 / 柱 / 堆叠 / 分组 / 区间柱 / 100% 堆叠面积）。
fn draw_cartesian(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ctype: ChartType,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let series: &[ChartSeries] = match data {
        ChartData::Series(s) => s,
        _ => &[],
    };
    if series.is_empty() {
        return;
    }
    let n_cat = series.iter().map(|s| s.points.len()).max().unwrap_or(0);
    if n_cat == 0 {
        return;
    }
    let labels = opts.x_labels.clone();

    // 值域（堆叠类按累加值）
    let (ymin, ymax) = match ctype {
        ChartType::StackedBar | ChartType::StackedArea100 => {
            let (lo, hi) = stacked_domain(series);
            if ctype == ChartType::StackedArea100 {
                (0.0, 1.0)
            } else {
                nice_linear_domain(lo, hi, opts.y_tick_count)
            }
        }
        // 柱族：值域必须包含 0，柱子才立在坐标轴上（dmin=0）
        ChartType::Bar | ChartType::GroupedBar | ChartType::RangeBar => {
            let (lo, hi) = series_domain(series);
            let (lo, hi) = (lo.min(0.0), hi.max(0.0));
            let (lo, hi) = nice_linear_domain(lo, hi, opts.y_tick_count);
            apply_y_opts(lo, hi, opts)
        }
        _ => {
            let (lo, hi) = series_domain(series);
            let (lo, hi) = nice_linear_domain(lo, hi, opts.y_tick_count);
            apply_y_opts(lo, hi, opts)
        }
    };
    let f = draw_frame(b, x, y, w, h, ymin, ymax, n_cat, &labels, opts, colors);

    match ctype {
        ChartType::Line | ChartType::Spline | ChartType::Step => {
            draw_line_family(b, &f, series, ctype, opts, colors)
        }
        ChartType::Area => draw_area(b, &f, series, opts, colors, false),
        ChartType::StackedArea100 => draw_area(b, &f, series, opts, colors, true),
        ChartType::RangeBar => draw_range_bar(b, &f, series, opts, colors),
        _ => draw_bars(b, &f, series, ctype, opts, colors),
    }
}

/// 堆叠值域。
fn stacked_domain(series: &[ChartSeries]) -> (f32, f32) {
    let n = series.iter().map(|s| s.points.len()).max().unwrap_or(0);
    let mut lo = 0.0f32;
    let mut hi = 0.0f32;
    for i in 0..n {
        let mut pos = 0.0f32;
        let mut neg = 0.0f32;
        for s in series {
            if let Some(p) = s.points.get(i) {
                if p.y >= 0.0 {
                    pos += p.y;
                } else {
                    neg += p.y;
                }
            }
        }
        hi = hi.max(pos);
        lo = lo.min(neg);
    }
    (lo, hi)
}

/// 折线族（Line / Spline / Step）。
fn draw_line_family(
    b: &mut DrawBuilder,
    f: &Frame,
    series: &[ChartSeries],
    ctype: ChartType,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let ps = ScalePoint::new(f.n_cat, [f.x0, f.x0 + f.pw]);
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        let raw: Vec<(f32, f32)> = s
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| (ps.position_of(i), f.yscale.position_of(p.y)))
            .collect();
        if raw.len() < 2 {
            if let Some(p) = raw.first() {
                b.fill_circle(p.0, p.1, 2.5, c);
            }
            continue;
        }
        let path = match ctype {
            ChartType::Spline => spline_points(&raw, 12),
            ChartType::Step => step_points(&raw),
            _ => raw.clone(),
        };
        b.stroke_path(&path, c, 2.0);
        if opts.show_data_labels {
            for (i, p) in s.points.iter().enumerate() {
                b.draw_text(
                    raw[i].0 - 12.0,
                    raw[i].1 - 13.0,
                    &format_tick(p.y),
                    &tokens::label_small(colors.on_surface),
                    40.0,
                );
            }
        }
    }
}

/// 阶梯折线点集。
fn step_points(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(pts.len() * 2);
    let mut prev_y = pts[0].1;
    for (i, p) in pts.iter().enumerate() {
        if i == 0 {
            out.push(*p);
        } else {
            out.push((p.0, prev_y));
            out.push(*p);
        }
        prev_y = p.1;
    }
    out
}

/// Catmull-Rom 平滑采样。
fn spline_points(pts: &[(f32, f32)], per_seg: usize) -> Vec<(f32, f32)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(pts.len() * per_seg);
    for i in 0..pts.len() - 1 {
        let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 < pts.len() {
            pts[i + 2]
        } else {
            pts[pts.len() - 1]
        };
        for s in 0..per_seg {
            let t = s as f32 / per_seg as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let px = 0.5
                * (2.0 * p1.0
                    + (-p0.0 + p2.0) * t
                    + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * t2
                    + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * t3);
            let py = 0.5
                * (2.0 * p1.1
                    + (-p0.1 + p2.1) * t
                    + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * t2
                    + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * t3);
            out.push((px, py));
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// 面积图（`stack100` = 100% 堆叠面积）。
fn draw_area(
    b: &mut DrawBuilder,
    f: &Frame,
    series: &[ChartSeries],
    opts: &ChartOptions,
    _colors: &ColorScheme,
    stack100: bool,
) {
    let ps = ScalePoint::new(f.n_cat, [f.x0, f.x0 + f.pw]);
    let mut cum = vec![0.0f32; f.n_cat];
    let mut totals = vec![0.0f32; f.n_cat];
    if stack100 {
        for s in series {
            for (i, p) in s.points.iter().enumerate() {
                if i < totals.len() {
                    totals[i] += p.y.abs();
                }
            }
        }
    }
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        let mut top: Vec<(f32, f32)> = Vec::with_capacity(s.points.len());
        let mut bottom: Vec<(f32, f32)> = Vec::with_capacity(s.points.len());
        for (i, p) in s.points.iter().enumerate() {
            let px = ps.position_of(i);
            let (yv, base_v) = if stack100 {
                let total = totals.get(i).copied().unwrap_or(1.0).max(1e-6);
                let frac = p.y.abs() / total;
                let base_frac = cum.get(i).copied().unwrap_or(0.0);
                cum[i] = base_frac + frac;
                (base_frac + frac, base_frac)
            } else {
                (p.y, 0.0)
            };
            let py = f.yscale.position_of(yv);
            let by = f.yscale.position_of(base_v);
            top.push((px, py));
            bottom.push((px, by));
        }
        let mut poly = top.clone();
        for p in bottom.iter().rev() {
            poly.push(*p);
        }
        if poly.len() >= 3 {
            b.fill_path(&poly, c.with_alpha(0.22));
        }
        b.stroke_path(&top, c, 2.0);
    }
}

/// 柱族（Bar / StackedBar / GroupedBar）。
fn draw_bars(
    b: &mut DrawBuilder,
    f: &Frame,
    series: &[ChartSeries],
    ctype: ChartType,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let band = ScaleBand::new(f.n_cat, [f.x0, f.x0 + f.pw])
        .padding_inner(opts.band_padding)
        .padding_outer(0.08);
    let stacked = ctype == ChartType::StackedBar;
    let n_ser = series.len().max(1);
    let bw = if stacked {
        band.band_width()
    } else {
        (band.band_width() / n_ser as f32).max(1.0)
    };
    let mut cum = vec![0.0f32; f.n_cat];
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        for (i, p) in s.points.iter().enumerate() {
            let (v0, v1) = if stacked {
                let base = cum[i];
                let top = base + p.y;
                cum[i] = top;
                (base, top)
            } else {
                (0.0, p.y)
            };
            let bx = if stacked {
                band.band_start(i)
            } else {
                band.band_start(i) + si as f32 * bw
            };
            let y_top = f.yscale.position_of(v1.max(v0));
            let y_bot = f.yscale.position_of(v0.min(v1));
            let bh = (y_bot - y_top).abs();
            if bh > 0.0 {
                b.fill_rrect(bx, y_top, bw, bh, opts.corner_radius, c);
            }
            if opts.show_data_labels {
                b.draw_text(
                    bx - 4.0,
                    y_top - 12.0,
                    &format_tick(p.y),
                    &tokens::label_small(colors.on_surface),
                    bw + 8.0,
                );
            }
        }
    }
}

/// 区间柱（`DataPoint.y` = 低值，`DataPoint.size` = 高值）。
fn draw_range_bar(
    b: &mut DrawBuilder,
    f: &Frame,
    series: &[ChartSeries],
    opts: &ChartOptions,
    _colors: &ColorScheme,
) {
    let band = ScaleBand::new(f.n_cat, [f.x0, f.x0 + f.pw])
        .padding_inner(opts.band_padding)
        .padding_outer(0.08);
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        for (i, p) in s.points.iter().enumerate() {
            let hi = p.size.max(p.y);
            let y_hi = f.yscale.position_of(hi);
            let y_lo = f.yscale.position_of(p.y);
            b.fill_rrect(
                band.band_start(i),
                y_hi,
                band.band_width(),
                (y_lo - y_hi).abs(),
                opts.corner_radius,
                c.with_alpha(0.75),
            );
        }
    }
}

/// 散点 / 气泡（连续 X 轴）。
fn draw_xy(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ctype: ChartType,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let series: &[ChartSeries] = match data {
        ChartData::Series(s) => s,
        _ => return,
    };
    let mut xlo = f32::MAX;
    let mut xhi = f32::MIN;
    let mut ylo = f32::MAX;
    let mut yhi = f32::MIN;
    for s in series {
        for p in &s.points {
            xlo = xlo.min(p.x);
            xhi = xhi.max(p.x);
            ylo = ylo.min(p.y);
            yhi = yhi.max(p.y);
        }
    }
    if xlo > xhi {
        return;
    }
    let (xlo, xhi) = nice_linear_domain(xlo, xhi, opts.y_tick_count);
    let (ylo, yhi) = series_domain(series);
    let (ylo, yhi) = nice_linear_domain(ylo, yhi, opts.y_tick_count);
    let (ylo, yhi) = apply_y_opts(ylo, yhi, opts);
    let f = draw_frame(b, x, y, w, h, ylo, yhi, 0, &[], opts, colors);
    let xs = ScaleLinear::new([xlo, xhi], [f.x0, f.x0 + f.pw]);
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        for p in &s.points {
            let px = xs.position_of(p.x);
            let py = f.yscale.position_of(p.y);
            let r = if ctype == ChartType::Bubble {
                (2.0 + p.size * 2.5).min(18.0)
            } else {
                3.0
            };
            b.fill_circle(px, py, r, c.with_alpha(0.85));
        }
    }
}

/// 横向柱状图。
fn draw_hbar(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let series: &[ChartSeries] = match data {
        ChartData::Series(s) => s,
        _ => return,
    };
    let n_cat = series.iter().map(|s| s.points.len()).max().unwrap_or(0);
    if n_cat == 0 {
        return;
    }
    let (lo, hi) = series_domain(series);
    let (_, vmax) = nice_linear_domain(lo.max(0.0), hi, opts.y_tick_count);
    let gutter = 56.0;
    let x0 = x + gutter;
    let pw = (w - gutter - 44.0).max(1.0);
    let row_h = h / n_cat as f32;
    let labels = opts.x_labels.clone();
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        for (i, p) in s.points.iter().enumerate() {
            let cy = y + i as f32 * row_h + row_h * 0.5;
            let bw = ((p.y / vmax.max(1e-6)).clamp(0.0, 1.0)) * pw;
            b.fill_rrect(
                x0,
                cy - row_h * 0.28,
                bw,
                row_h * 0.56,
                opts.corner_radius,
                c,
            );
            if let Some(l) = labels.get(i) {
                b.draw_text(
                    x,
                    cy - 6.0,
                    l,
                    &tokens::label_small(colors.on_surface_variant),
                    gutter - 6.0,
                );
            }
            b.draw_text(
                x0 + bw + 4.0,
                cy - 6.0,
                &format_tick(p.y),
                &tokens::label_small(colors.on_surface),
                40.0,
            );
        }
    }
}

/// 直方图（系列点为原始样本，按 bin 聚合）。
fn draw_histogram(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let series: &[ChartSeries] = match data {
        ChartData::Series(s) => s,
        _ => return,
    };
    let mut samples: Vec<f32> = Vec::new();
    for s in series {
        for p in &s.points {
            samples.push(p.y);
        }
    }
    if samples.is_empty() {
        return;
    }
    let lo = samples.iter().cloned().fold(f32::MAX, f32::min);
    let hi = samples.iter().cloned().fold(f32::MIN, f32::max);
    let bins = opts.histogram_bins.max(1);
    let span = ((hi - lo).abs()).max(1e-6);
    let mut counts = vec![0usize; bins];
    for v in &samples {
        let t = (((v - lo) / span) * (bins - 1) as f32).round().max(0.0) as usize;
        counts[t.min(bins - 1)] += 1;
    }
    let max_c = counts.iter().cloned().max().unwrap_or(1) as f32;
    let (_, vmax) = nice_linear_domain(0.0, max_c, opts.y_tick_count);
    let f = draw_frame(b, x, y, w, h, 0.0, vmax, bins, &[], opts, colors);
    let band = ScaleBand::new(bins, [f.x0, f.x0 + f.pw])
        .padding_inner(0.08)
        .padding_outer(0.04);
    for (i, c) in counts.iter().enumerate() {
        let bh = (*c as f32 / vmax.max(1e-6)) * f.ph;
        b.fill_rrect(
            band.band_start(i),
            f.base - bh,
            band.band_width(),
            bh,
            opts.corner_radius,
            series_palette(1),
        );
        b.draw_text(
            band.band_start(i),
            f.base - bh - 12.0,
            &format!("{}", c),
            &tokens::label_small(colors.on_surface),
            band.band_width(),
        );
    }
}

/// 瀑布图。
fn draw_waterfall(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let entries: &[WaterfallEntry] = match data {
        ChartData::Waterfall(e) => e,
        _ => return,
    };
    if entries.is_empty() {
        return;
    }
    // 计算累加区间
    let mut cum = 0.0f32;
    let mut lows = Vec::new();
    let mut highs = Vec::new();
    for e in entries {
        let (lo, hi) = match e.kind {
            WaterfallKind::Total => (0.0, e.value),
            WaterfallKind::Increase => (cum, cum + e.value),
            WaterfallKind::Decrease => (cum + e.value, cum),
        };
        if e.kind != WaterfallKind::Total {
            cum = hi.max(lo);
        } else {
            cum = e.value;
        }
        lows.push(lo);
        highs.push(hi);
    }
    let lo = lows.iter().cloned().fold(f32::MAX, f32::min);
    let hi = highs.iter().cloned().fold(f32::MIN, f32::max);
    let (lo, hi) = nice_linear_domain(lo.min(0.0), hi, opts.y_tick_count);
    let labels: Vec<String> = entries.iter().map(|e| e.label.clone()).collect();
    let f = draw_frame(b, x, y, w, h, lo, hi, entries.len(), &labels, opts, colors);
    let band = ScaleBand::new(entries.len(), [f.x0, f.x0 + f.pw])
        .padding_inner(opts.band_padding)
        .padding_outer(0.08);
    for (i, e) in entries.iter().enumerate() {
        let c = match e.kind {
            WaterfallKind::Total => series_palette(0),
            WaterfallKind::Increase => series_palette(3),
            WaterfallKind::Decrease => series_palette(2),
        };
        let y_top = f.yscale.position_of(highs[i]);
        let y_bot = f.yscale.position_of(lows[i]);
        b.fill_rrect(
            band.band_start(i),
            y_top,
            band.band_width(),
            (y_bot - y_top).abs(),
            opts.corner_radius,
            c,
        );
        if opts.show_data_labels {
            b.draw_text(
                band.band_start(i) - 4.0,
                y_top - 12.0,
                &format_tick(e.value),
                &tokens::label_small(colors.on_surface),
                band.band_width() + 8.0,
            );
        }
    }
}

/// K 线图。
fn draw_candlestick(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let candles: &[Candle] = match data {
        ChartData::Candle(c) => c,
        _ => return,
    };
    if candles.is_empty() {
        return;
    }
    let lo = candles.iter().map(|c| c.low).fold(f32::MAX, f32::min);
    let hi = candles.iter().map(|c| c.high).fold(f32::MIN, f32::max);
    let (lo, hi) = nice_linear_domain(lo, hi, opts.y_tick_count);
    let labels: Vec<String> = candles.iter().map(|c| c.label.clone()).collect();
    let f = draw_frame(b, x, y, w, h, lo, hi, candles.len(), &labels, opts, colors);
    let band = ScaleBand::new(candles.len(), [f.x0, f.x0 + f.pw])
        .padding_inner(opts.band_padding)
        .padding_outer(0.08);
    let up = series_palette(3);
    let down = series_palette(2);
    for (i, cd) in candles.iter().enumerate() {
        let cx = band.band_center(i);
        let bw = (band.band_width() * 0.6).max(1.5);
        let color = if cd.close >= cd.open { up } else { down };
        b.line(
            cx,
            f.yscale.position_of(cd.high),
            cx,
            f.yscale.position_of(cd.low),
            color,
            1.2,
        );
        let yo = f.yscale.position_of(cd.open);
        let yc = f.yscale.position_of(cd.close);
        let top = yo.min(yc);
        let bh = (yc - yo).abs().max(1.0);
        b.fill_rrect(cx - bw * 0.5, top, bw, bh, 1.0, color);
    }
}

/// 饼 / 环形图。
fn draw_pie(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    ctype: ChartType,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let slices: &[PieSlice] = match data {
        ChartData::Pie(s) => s,
        _ => return,
    };
    let total: f32 = slices.iter().map(|s| s.value.max(0.0)).sum();
    if total <= 0.0 {
        return;
    }
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let r = (h.min(w) * 0.42).max(8.0);
    let r_inner = if ctype == ChartType::Donut {
        r * opts.donut_ratio
    } else {
        0.0
    };
    let mut a = -std::f32::consts::FRAC_PI_2;
    for (i, s) in slices.iter().enumerate() {
        let frac = (s.value.max(0.0) / total).clamp(0.0, 1.0);
        let a1 = a + frac * std::f32::consts::TAU;
        let c = pick_color(s.color, i, opts);
        if r_inner > 0.0 {
            annular_sector(b, cx, cy, r, r_inner, a, a1, c);
        } else {
            let mut pts = vec![(cx, cy)];
            for k in 0..=24 {
                let t = a + (a1 - a) * (k as f32 / 24.0);
                pts.push((cx + t.cos() * r, cy + t.sin() * r));
            }
            b.fill_path(&pts, c);
        }
        if frac >= 0.05 && r - r_inner > 24.0 {
            let mid = a + (a1 - a) * 0.5;
            let rr = (r + r_inner) * 0.5;
            let txt = format!("{:.0}%", frac * 100.0);
            b.draw_text(
                cx + mid.cos() * rr - 14.0,
                cy + mid.sin() * rr - 6.0,
                &txt,
                &tokens::label_medium(colors.on_surface),
                40.0,
            );
        }
        a = a1;
    }
}

/// 环形扇区（外弧 + 内弧反向）。
fn annular_sector(
    b: &mut DrawBuilder,
    cx: f32,
    cy: f32,
    r_out: f32,
    r_in: f32,
    a0: f32,
    a1: f32,
    color: Color,
) {
    let steps = 24;
    let mut pts = Vec::with_capacity(steps * 2 + 2);
    for k in 0..=steps {
        let t = a0 + (a1 - a0) * (k as f32 / steps as f32);
        pts.push((cx + t.cos() * r_out, cy + t.sin() * r_out));
    }
    for k in (0..=steps).rev() {
        let t = a0 + (a1 - a0) * (k as f32 / steps as f32);
        pts.push((cx + t.cos() * r_in, cy + t.sin() * r_in));
    }
    b.fill_path(&pts, color);
}

/// 漏斗图。
fn draw_funnel(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let slices: &[FunnelSlice] = match data {
        ChartData::Funnel(s) => s,
        _ => return,
    };
    if slices.is_empty() {
        return;
    }
    let total: f32 = slices.iter().map(|s| s.value.max(0.0)).sum();
    if total <= 0.0 {
        return;
    }
    let row_h = h / slices.len() as f32;
    let max_w = w * 0.72;
    let cx = x + w * 0.5;
    for (i, s) in slices.iter().enumerate() {
        let frac = (s.value.max(0.0) / total).clamp(0.0, 1.0);
        let w1 = max_w * frac;
        let next = slices
            .get(i + 1)
            .map(|n| (n.value.max(0.0) / total).clamp(0.0, 1.0))
            .unwrap_or(frac);
        let w2 = max_w * next;
        let y0 = y + i as f32 * row_h + row_h * 0.12;
        let hh = row_h * 0.76;
        let c = pick_color(s.color, i, opts);
        let pts = [
            (cx - w1 * 0.5, y0),
            (cx + w1 * 0.5, y0),
            (cx + w2 * 0.5, y0 + hh),
            (cx - w2 * 0.5, y0 + hh),
        ];
        b.fill_path(&pts, c);
        b.draw_text(
            x + 2.0,
            y0 + hh * 0.5 - 6.0,
            &s.label,
            &tokens::label_small(colors.on_surface),
            w * 0.26,
        );
        b.draw_text(
            x + w - 54.0,
            y0 + hh * 0.5 - 6.0,
            &format!("{:.0}", s.value),
            &tokens::label_medium(colors.on_surface),
            52.0,
        );
    }
}

/// 雷达图。
fn draw_radar(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let series: &[ChartSeries] = match data {
        ChartData::Series(s) => s,
        _ => return,
    };
    let n_axis = series.iter().map(|s| s.points.len()).max().unwrap_or(0);
    if n_axis < 3 {
        return;
    }
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let r = (h.min(w) * 0.36).max(10.0);
    let vmax = series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.y))
        .fold(0.0f32, f32::max)
        .max(1e-6);
    // 网格环
    for lv in 1..=4 {
        let rr = r * lv as f32 / 4.0;
        let mut pts = Vec::new();
        for i in 0..n_axis {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n_axis as f32 * std::f32::consts::TAU;
            pts.push((cx + a.cos() * rr, cy + a.sin() * rr));
        }
        b.stroke_path(&pts, colors.outline_variant, 1.0);
    }
    // 轴线
    for i in 0..n_axis {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n_axis as f32 * std::f32::consts::TAU;
        b.line(
            cx,
            cy,
            cx + a.cos() * r,
            cy + a.sin() * r,
            colors.outline_variant,
            1.0,
        );
    }
    // 数据多边形
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        let mut pts = Vec::new();
        for (i, p) in s.points.iter().enumerate() {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n_axis as f32 * std::f32::consts::TAU;
            let rr = r * (p.y / vmax).clamp(0.0, 1.0);
            pts.push((cx + a.cos() * rr, cy + a.sin() * rr));
        }
        if pts.len() >= 3 {
            b.fill_path(&pts, c.with_alpha(0.18));
            b.stroke_path(&pts, c, 2.0);
        }
    }
    // 轴标签
    for i in 0..n_axis {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n_axis as f32 * std::f32::consts::TAU;
        let lx = cx + a.cos() * (r + 14.0);
        let ly = cy + a.sin() * (r + 14.0);
        let label = series
            .iter()
            .find_map(|s| s.points.get(i).map(|_| ()))
            .and_then(|_| opts.x_labels.get(i).cloned())
            .unwrap_or_default();
        b.draw_text(
            lx - 24.0,
            ly - 6.0,
            &label,
            &tokens::label_small(colors.on_surface_variant),
            48.0,
        );
    }
}

/// 径向柱图。
fn draw_radial_bar(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    opts: &ChartOptions,
    _colors: &ColorScheme,
) {
    let series: &[ChartSeries] = match data {
        ChartData::Series(s) => s,
        _ => return,
    };
    let n = series.iter().map(|s| s.points.len()).max().unwrap_or(0);
    if n == 0 {
        return;
    }
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let r_out = (h.min(w) * 0.40).max(10.0);
    let r_in = r_out * 0.42;
    let vmax = series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.y))
        .fold(0.0f32, f32::max)
        .max(1e-6);
    for (si, s) in series.iter().enumerate() {
        let c = pick_color(s.color, si, opts);
        for (i, p) in s.points.iter().enumerate() {
            let frac = (p.y / vmax).clamp(0.0, 1.0);
            let a0 = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
            let a1 = a0 + (std::f32::consts::TAU / n as f32) * 0.78;
            let r1 = r_in + (r_out - r_in) * frac;
            let mut pts = Vec::new();
            for k in 0..=16 {
                let t = a0 + (a1 - a0) * (k as f32 / 16.0);
                pts.push((cx + t.cos() * r_out, cy + t.sin() * r_out));
            }
            for k in (0..=16).rev() {
                let t = a0 + (a1 - a0) * (k as f32 / 16.0);
                pts.push((cx + t.cos() * r1, cy + t.sin() * r1));
            }
            b.fill_path(&pts, c);
        }
    }
}

/// 桑基图（节点分层 + 连线）。
/// 桑基图节点宽度与列内节点间隔。
const SANKEY_NODE_W: f32 = 16.0;
const SANKEY_NODE_PAD: f32 = 12.0;

/// 线性插值。
fn sankey_lerp(x0: f32, x1: f32, t: f32) -> f32 {
    x0 + (x1 - x0) * t
}

/// 桑基图实现：
/// 1) 入度/出边 → 最长路径松弛求分层列数；2) 各列按总流量等比例缩放高度；
/// 3) 列内堆叠后整体垂直居中；4) 缎带用 smoothstep 连接；5) 标签画在节点外侧。
fn draw_sankey(
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    data: &ChartData,
    _opts: &ChartOptions,
    colors: &ColorScheme,
) {
    let (nodes, links) = match data {
        ChartData::Sankey { nodes, links } => (nodes, links),
        _ => return,
    };
    let n = nodes.len();
    if n == 0 {
        return;
    }
    // 分层：最长路径松弛（至多 n+1 轮）
    let mut depth = vec![0usize; n];
    for _ in 0..=n {
        let mut changed = false;
        for l in links {
            if l.source < n && l.target < n && depth[l.target] < depth[l.source] + 1 {
                depth[l.target] = depth[l.source] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let cols = depth.iter().cloned().max().unwrap_or(0) + 1;
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); cols];
    for i in 0..n {
        columns[depth[i]].push(i);
    }
    let mut col_sum = vec![0.0f32; cols];
    for i in 0..n {
        col_sum[depth[i]] += nodes[i].value.max(0.0);
    }
    // 绘图区（左右各留 12 内边距）
    let content_x = x + 12.0;
    let content_w = (w - 24.0).max(0.0);
    let content_top = y;
    let content_bottom = y + h;
    // 缩放：取各列「可用高度 / 列总流量」的最小值（齐平列高）
    let mut scale = f32::INFINITY;
    for c in 0..cols {
        let cnt = columns[c].len();
        if cnt == 0 || col_sum[c] <= 0.0 {
            continue;
        }
        let avail = (content_bottom - content_top) - (cnt as f32 - 1.0) * SANKEY_NODE_PAD;
        if avail > 0.0 {
            scale = scale.min(avail / col_sum[c]);
        }
    }
    if !scale.is_finite() {
        scale = 1.0;
    }
    let col_x = |c: usize| -> f32 {
        if cols <= 1 {
            content_x
        } else {
            content_x + (content_w - SANKEY_NODE_W) * c as f32 / (cols - 1) as f32
        }
    };
    // 各节点高度（至少 2px）
    let heights: Vec<f32> = (0..n)
        .map(|i| (nodes[i].value.max(0.0) * scale).max(2.0))
        .collect();
    // 列内堆叠，再整体垂直居中
    let mut pos: Vec<(f32, f32)> = vec![(0.0, 0.0); n];
    for c in 0..cols {
        let mut cur = content_top;
        for &i in &columns[c] {
            pos[i].0 = col_x(c);
            pos[i].1 = cur;
            cur += heights[i] + SANKEY_NODE_PAD;
        }
        let last = cur - SANKEY_NODE_PAD;
        let offset = (content_bottom - last) / 2.0;
        if offset > 0.0 {
            for &i in &columns[c] {
                pos[i].1 += offset;
            }
        }
    }
    // 缎带：按节点流量累计偏移，用 smoothstep 连接上下边
    let mut src_cur = vec![0.0f32; n];
    let mut dst_cur = vec![0.0f32; n];
    for i in 0..n {
        src_cur[i] = pos[i].1;
        dst_cur[i] = pos[i].1;
    }
    let steps = 24usize;
    for l in links {
        if l.source >= n || l.target >= n {
            continue;
        }
        let lw = (l.value.max(0.0) * scale).max(1.0);
        let sx = pos[l.source].0 + SANKEY_NODE_W;
        let tx = pos[l.target].0;
        let sy = src_cur[l.source];
        let ty = dst_cur[l.target];
        src_cur[l.source] += lw;
        dst_cur[l.target] += lw;
        let mut poly = Vec::with_capacity(steps * 2 + 2);
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let by = sy + (ty - sy) * (3.0 * t * t - 2.0 * t * t * t);
            poly.push((sankey_lerp(sx, tx, t), by));
        }
        for k in (0..=steps).rev() {
            let t = k as f32 / steps as f32;
            let by = sy + (ty - sy) * (3.0 * t * t - 2.0 * t * t * t) + lw;
            poly.push((sankey_lerp(sx, tx, t), by));
        }
        b.fill_path(&poly, series_palette(l.source).with_alpha(0.35));
    }
    // 节点 + 标签（标签画在节点外侧：左列右侧、右列左侧）
    for i in 0..n {
        b.fill_rrect(
            pos[i].0,
            pos[i].1,
            SANKEY_NODE_W,
            heights[i],
            2.0,
            colors.on_surface_variant,
        );
        let mid = pos[i].1 + heights[i] / 2.0;
        let on_right = depth[i] < cols / 2;
        let label_style = tokens::label_small(colors.on_surface);
        if on_right {
            b.draw_text(
                pos[i].0 + SANKEY_NODE_W + 4.0,
                mid,
                &nodes[i].label,
                &label_style,
                f32::INFINITY,
            );
        } else {
            let (tw, _) = measure_text_with_measurer(&nodes[i].label, &label_style);
            b.draw_text(
                pos[i].0 - 4.0 - tw,
                mid,
                &nodes[i].label,
                &label_style,
                f32::INFINITY,
            );
        }
    }
}
