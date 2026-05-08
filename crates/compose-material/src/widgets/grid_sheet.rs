//! GridSheet — 可编辑数据网格。
//!
//! 核心观感与交互：
//!
//! * 表头（点击排序，数值列按数值比较，附 ▲/▼ 指示）+ 行号列；
//! * 固定列宽（末列自动占满剩余宽度）；
//! * 斑马纹、表头底线、行号列分隔线；
//! * 条件格式（按列规则着背景/前景/加粗）；
//! * 单元格点选（选中态主色高亮，选择状态由调用方持有）。
//!
//! 虚拟滚动 / 列钉住 / 单元格编辑为进阶能力，待后续接入。

use compose::foundation::Color;
use compose::layout::{Alignment, Arrangement, Box, Column, CrossAxisAlignment, Modifier, Row};
use compose::material::theme_colors;
use compose::runtime::{MutableState, mutableStateOf, remember};

use crate::tokens;
use crate::widgets::{InkWellR, Text, VerticalScroll};

// ══════════════════════════════ 数据模型 ══════════════════════════════

/// 单元格值。
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Text(String),
    Number(f64),
    Empty,
}

impl CellValue {
    /// 文本表示（整数不带小数点，浮点保留两位）。
    pub fn as_text(&self) -> String {
        match self {
            CellValue::Text(s) => s.clone(),
            CellValue::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    format!("{:.2}", n)
                }
            }
            CellValue::Empty => String::new(),
        }
    }

    /// 尝试解析为数值。
    pub fn as_number(&self) -> Option<f64> {
        match self {
            CellValue::Number(n) => Some(*n),
            CellValue::Text(s) => s.trim().parse().ok(),
            CellValue::Empty => None,
        }
    }
}

impl From<&str> for CellValue {
    fn from(s: &str) -> Self {
        CellValue::Text(s.to_string())
    }
}

impl From<String> for CellValue {
    fn from(s: String) -> Self {
        CellValue::Text(s)
    }
}

impl From<f64> for CellValue {
    fn from(n: f64) -> Self {
        CellValue::Number(n)
    }
}

impl From<i32> for CellValue {
    fn from(n: i32) -> Self {
        CellValue::Number(n as f64)
    }
}

/// 单元格文本对齐方式。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum GridSheetTextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// 列定义。
#[derive(Debug, Clone)]
pub struct GridSheetColumn {
    /// 列唯一键（条件格式规则按此匹配）。
    pub key: String,
    /// 表头标题。
    pub title: String,
    /// 列宽（末列自动占满剩余宽度）。
    pub width: f32,
    /// 文本对齐。
    pub align: GridSheetTextAlign,
    /// 是否可排序。
    pub sortable: bool,
}

impl GridSheetColumn {
    pub fn new(key: &str, title: &str, width: f32) -> Self {
        Self {
            key: key.to_string(),
            title: title.to_string(),
            width,
            align: GridSheetTextAlign::Left,
            sortable: true,
        }
    }

    pub fn with_align(mut self, align: GridSheetTextAlign) -> Self {
        self.align = align;
        self
    }

    pub fn with_sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }
}

/// 行数据（按列顺序）。
#[derive(Debug, Clone)]
pub struct GridSheetRow {
    pub key: String,
    pub data: Vec<CellValue>,
}

impl GridSheetRow {
    pub fn new(key: &str, data: Vec<CellValue>) -> Self {
        Self {
            key: key.to_string(),
            data,
        }
    }
}

// ══════════════════════════════ 条件格式 ══════════════════════════════

/// 条件格式条件类型。
#[derive(Debug, Clone)]
pub enum FormatCondition {
    TextContains(String),
    TextEquals(String),
    NumberGreaterThan(f64),
    NumberLessThan(f64),
}

/// 条件格式规则。
#[derive(Debug, Clone)]
pub struct ConditionalFormatRule {
    /// 应用列（`None` 表示所有列）。
    pub col_key: Option<String>,
    pub condition: FormatCondition,
    pub bg_color: Option<Color>,
    pub fg_color: Option<Color>,
    pub bold: bool,
}

impl ConditionalFormatRule {
    fn new(col_key: Option<&str>, condition: FormatCondition) -> Self {
        Self {
            col_key: col_key.map(|s| s.to_string()),
            condition,
            bg_color: None,
            fg_color: None,
            bold: false,
        }
    }

    pub fn text_contains(col_key: Option<&str>, text: &str) -> Self {
        Self::new(col_key, FormatCondition::TextContains(text.to_string()))
    }

    pub fn text_equals(col_key: Option<&str>, text: &str) -> Self {
        Self::new(col_key, FormatCondition::TextEquals(text.to_string()))
    }

    pub fn number_greater_than(col_key: Option<&str>, value: f64) -> Self {
        Self::new(col_key, FormatCondition::NumberGreaterThan(value))
    }

    pub fn number_less_than(col_key: Option<&str>, value: f64) -> Self {
        Self::new(col_key, FormatCondition::NumberLessThan(value))
    }

    pub fn with_bg_color(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }

    pub fn with_fg_color(mut self, color: Color) -> Self {
        self.fg_color = Some(color);
        self
    }

    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }

    /// 是否命中（列匹配 + 条件匹配）。
    pub fn matches(&self, col_key: &str, value: &CellValue) -> bool {
        if let Some(k) = &self.col_key {
            if k != col_key {
                return false;
            }
        }
        match &self.condition {
            FormatCondition::TextContains(t) => value.as_text().contains(t.as_str()),
            FormatCondition::TextEquals(t) => value.as_text() == t.as_str(),
            FormatCondition::NumberGreaterThan(v) => {
                value.as_number().map(|n| n > *v).unwrap_or(false)
            }
            FormatCondition::NumberLessThan(v) => {
                value.as_number().map(|n| n < *v).unwrap_or(false)
            }
        }
    }
}

// ══════════════════════════════ 配置 ══════════════════════════════

/// GridSheet 视觉配置。
#[derive(Debug, Clone, Copy)]
pub struct GridSheetConfig {
    pub row_height: f32,
    pub header_height: f32,
    pub row_number_width: f32,
    pub show_row_numbers: bool,
    pub zebra_stripes: bool,
    pub sortable: bool,
}

impl Default for GridSheetConfig {
    fn default() -> Self {
        Self {
            row_height: tokens::TABLE_ROW_HEIGHT,
            header_height: tokens::TABLE_HEADER_HEIGHT,
            row_number_width: tokens::TABLE_ROWNUM_WIDTH,
            show_row_numbers: true,
            zebra_stripes: true,
            sortable: true,
        }
    }
}

fn align_of(a: GridSheetTextAlign) -> Alignment {
    match a {
        GridSheetTextAlign::Left => Alignment::CenterStart,
        GridSheetTextAlign::Center => Alignment::Center,
        GridSheetTextAlign::Right => Alignment::CenterEnd,
    }
}

// ══════════════════════════════ 控件 ══════════════════════════════

/// `GridSheet(columns, rows, rules, config, selected, modifier)` — 数据网格。
///
/// * `modifier` 需给定高度（如 `.height(300.0)`）限定视口；表头固定在顶部，
///   数据区垂直滚动。
/// * 点击表头排序（再次点击反转方向）；点击单元格把 `(显示行, 列)` 写入
///   `selected`（`None` 表示未选中）。
/// * 条件格式规则按列 key 匹配，首条命中生效（背景/前景/加粗）。
pub fn GridSheet(
    columns: Vec<GridSheetColumn>,
    rows: Vec<GridSheetRow>,
    rules: Vec<ConditionalFormatRule>,
    config: GridSheetConfig,
    selected: MutableState<Option<(usize, usize)>>,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let cols = columns;
    let rows_st = remember(|| rows);
    let sort = remember(|| mutableStateOf((usize::MAX, true)));
    let ncols = cols.len();
    let (sort_col, asc) = sort.get();

    // 显示顺序（排序后；稳定排序保持未排序列的相对次序）
    let mut order: Vec<usize> = (0..rows_st.len()).collect();
    if sort_col < ncols {
        order.sort_by(|&a, &b| {
            let av = rows_st[a].data.get(sort_col);
            let bv = rows_st[b].data.get(sort_col);
            let ord = match (av, bv) {
                (Some(CellValue::Number(x)), Some(CellValue::Number(y))) => {
                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                }
                (Some(a), Some(b)) => a.as_text().cmp(&b.as_text()),
                _ => std::cmp::Ordering::Equal,
            };
            if asc { ord } else { ord.reverse() }
        });
    }

    let rn_w = if config.show_row_numbers {
        config.row_number_width
    } else {
        0.0
    };
    let hh = config.header_height;
    let rh = config.row_height;

    Column(
        modifier
            .fill_max_width()
            .background(colors.Surface, tokens::SHAPE_EXTRA_SMALL)
            .border(1.0, colors.outline_variant, tokens::SHAPE_EXTRA_SMALL),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            let cols_h = cols.clone();
            let sort_h = sort;
            // ── 表头 ──
            Row(
                Modifier::new()
                    .fill_max_width()
                    .height(hh)
                    .background(colors.surface_container, 0.0),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    if rn_w > 0.0 {
                        Box(Modifier::new().width(rn_w), Alignment::Center, move || {
                            Text(
                                "#",
                                tokens::label_small(colors.on_surface_variant),
                                Modifier::new(),
                            );
                        });
                        Box(
                            Modifier::new()
                                .width(1.0)
                                .background(colors.outline_variant.with_alpha(0.5), 0.0),
                            Alignment::TopStart,
                            move || {},
                        );
                    }
                    let last = cols_h.len().saturating_sub(1);
                    for (i, col) in cols_h.iter().enumerate() {
                        let (sc, sa) = sort_h.get();
                        let arrow = if sc == i {
                            if sa { " ▲" } else { " ▼" }
                        } else {
                            ""
                        };
                        let title = format!("{}{}", col.title, arrow);
                        let align = align_of(col.align);
                        let m = if i == last {
                            Modifier::new().weight(1.0, true)
                        } else {
                            Modifier::new().width(col.width)
                        };
                        let sortable = config.sortable && col.sortable;
                        let s = sort_h;
                        InkWellR(
                            m,
                            0.0,
                            move || {
                                if sortable {
                                    s.update(|v| {
                                        if v.0 == i {
                                            v.1 = !v.1;
                                        } else {
                                            *v = (i, true);
                                        }
                                    });
                                }
                            },
                            move || {
                                let t = title.clone();
                                Box(
                                    Modifier::new().fill_max_size().padding_ltrb(
                                        tokens::SPACE_LG,
                                        0.0,
                                        tokens::SPACE_LG,
                                        0.0,
                                    ),
                                    align,
                                    move || {
                                        Text(
                                            t.clone(),
                                            tokens::label_medium(colors.on_surface_variant),
                                            Modifier::new(),
                                        );
                                    },
                                );
                            },
                        );
                    }
                },
            );
            // 表头底线
            Box(
                Modifier::new()
                    .fill_max_width()
                    .height(1.0)
                    .background(colors.outline_variant, 0.0),
                Alignment::TopStart,
                move || {},
            );
            // ── 数据区（垂直滚动）──
            let cols_b = cols.clone();
            let rows_b = rows_st.clone();
            let rules_b = rules.clone();
            let order_b = order.clone();
            let sel_b = selected;
            VerticalScroll(
                Modifier::new().weight(1.0, true).fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let sel_b = sel_b;
                    let last = cols_b.len().saturating_sub(1);
                    for (di, &ri) in order_b.iter().enumerate() {
                        let cols_r = cols_b.clone();
                        let rules_r = rules_b.clone();
                        let row = rows_b[ri].clone();
                        let zebra = if config.zebra_stripes && di % 2 == 1 {
                            colors.outline_variant.with_alpha(0.18)
                        } else {
                            Color::TRANSPARENT
                        };
                        Row(
                            Modifier::new()
                                .fill_max_width()
                                .height(rh)
                                .background(zebra, 0.0),
                            Arrangement::Start,
                            CrossAxisAlignment::Stretch,
                            move || {
                                if rn_w > 0.0 {
                                    Box(
                                        Modifier::new().width(rn_w),
                                        Alignment::Center,
                                        move || {
                                            Text(
                                                (di + 1).to_string(),
                                                tokens::label_small(colors.on_surface_variant),
                                                Modifier::new(),
                                            );
                                        },
                                    );
                                    Box(
                                        Modifier::new().width(1.0).background(
                                            colors.outline_variant.with_alpha(0.5),
                                            0.0,
                                        ),
                                        Alignment::TopStart,
                                        move || {},
                                    );
                                }
                                for (i, val) in row.data.iter().enumerate() {
                                    let col_key =
                                        cols_r.get(i).map(|cc| cc.key.clone()).unwrap_or_default();
                                    let align = cols_r
                                        .get(i)
                                        .map(|cc| align_of(cc.align))
                                        .unwrap_or(Alignment::CenterStart);
                                    // 条件格式（首条命中生效）
                                    let mut bg = Color::TRANSPARENT;
                                    let mut fg = colors.on_surface;
                                    let mut bold = false;
                                    for rule in &rules_r {
                                        if rule.matches(&col_key, val) {
                                            if let Some(b) = rule.bg_color {
                                                bg = b;
                                            }
                                            if let Some(f) = rule.fg_color {
                                                fg = f;
                                            }
                                            bold = rule.bold;
                                            break;
                                        }
                                    }
                                    // 选中态
                                    let is_sel = sel_b.get() == Some((di, i));
                                    let cell_bg = if is_sel {
                                        tokens::state_layer(colors.primary, tokens::STATE_SELECTED)
                                    } else {
                                        bg
                                    };
                                    let m = if i == last {
                                        Modifier::new().weight(1.0, true)
                                    } else {
                                        Modifier::new().width(
                                            cols_r
                                                .get(i)
                                                .map(|cc| cc.width)
                                                .unwrap_or(tokens::TABLE_COL_WIDTH),
                                        )
                                    };
                                    let m = m.background(cell_bg, 0.0);
                                    let text = val.as_text();
                                    let s = sel_b;
                                    InkWellR(
                                        m,
                                        0.0,
                                        move || {
                                            s.set(Some((di, i)));
                                        },
                                        move || {
                                            let t = text.clone();
                                            Box(
                                                Modifier::new().fill_max_size().padding_ltrb(
                                                    tokens::SPACE_LG,
                                                    0.0,
                                                    tokens::SPACE_LG,
                                                    0.0,
                                                ),
                                                align,
                                                move || {
                                                    let mut ts = tokens::body_medium(if is_sel {
                                                        colors.primary
                                                    } else {
                                                        fg
                                                    });
                                                    if bold {
                                                        ts = ts.weight(700);
                                                    }
                                                    Text(t.clone(), ts, Modifier::new());
                                                },
                                            );
                                        },
                                    );
                                }
                            },
                        );
                    }
                },
            );
        },
    );
}

/// 供示例/测试构造单元格值的便捷宏式入口（`cell!("A1")` / `cell!(38000.0)`）。
#[macro_export]
macro_rules! cell {
    ($v:expr) => {
        $crate::CellValue::from($v)
    };
}
