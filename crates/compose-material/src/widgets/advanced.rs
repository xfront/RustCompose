//! 高级控件 — ScrollView / Calendar / CodeBlock / Settings …

use std::cell::Cell;
use std::rc::Rc;

use compose::layout::{
    Alignment, Arrangement, Column, CrossAxisAlignment, DrawBuilder, Modifier, NodeId, Policy, Row,
    ScrollInfo, Spacer, TextStyle, add_node, with_content_scope,
};
use compose::material::theme_colors;
use compose::runtime::{
    MutableState, add_frame_driver, disposable_effect, remember, remove_frame_driver, with_rt_pub,
};

use crate::tokens;
use crate::widgets::Text;

// ══════════════════════════════ 滚动容器 ══════════════════════════════

/// `VerticalScroll { content }` — 可纵向滚动的 Column。
pub fn VerticalScroll(
    modifier: Modifier,
    arrangement: Arrangement,
    cross: CrossAxisAlignment,
    content: impl Fn() + 'static,
) {
    scroll_container(
        modifier.VerticalScroll(),
        true,
        Policy::Column { arrangement, cross },
        content,
    );
}

/// `HorizontalScroll { content }` — 可横向滚动的 Row。
pub fn HorizontalScroll(
    modifier: Modifier,
    arrangement: Arrangement,
    cross: CrossAxisAlignment,
    content: impl Fn() + 'static,
) {
    scroll_container(
        modifier.HorizontalScroll(),
        false,
        Policy::Row { arrangement, cross },
        content,
    );
}

/// 滚动容器公共实现：节点每帧重建，把滚动偏移存在节点上会随重组（如内容变化、命中交互）
/// 被清零。故用 `remember` 保住偏移，并注册一次性帧驱动把实时节点偏移同步回来。
fn scroll_container(
    modifier: Modifier,
    vertical: bool,
    policy: Policy,
    content: impl Fn() + 'static,
) {
    let off = remember(|| Rc::new(Cell::new(0.0f32)));
    let id_cell = remember(|| Rc::new(Cell::new(None::<NodeId>)));
    let id = add_node(policy, modifier);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].scroll = Some(ScrollInfo {
            vertical,
            offset: off.get(),
            content_size: 0.0,
            sticky: None,
        });
    });
    id_cell.set(Some(id));
    let off_c = off.clone();
    let id_c = id_cell.clone();
    let drv = remember(|| {
        add_frame_driver(move || {
            if let Some(id) = id_c.get() {
                let v = with_rt_pub(|rt| {
                    rt.nodes[id.index()]
                        .scroll
                        .as_ref()
                        .map(|s| s.offset)
                        .unwrap_or(0.0)
                });
                off_c.set(v);
            }
        })
    });
    // 作用域销毁时注销帧驱动，避免驱动随滚动控件消失后继续驻留
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
    with_content_scope(id, content);
}

// ══════════════════════════════ 日历 ══════════════════════════════

const WEEK: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];

/// 月天数。
pub fn DaysInMonth(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Zeller 公式：返回 0=周日 .. 6=周六。
pub fn WeekdayOf(year: i32, month: u32, day: u32) -> u32 {
    let (y, m) = if month < 3 {
        (year - 1, month + 12)
    } else {
        (year, month)
    };
    let k = y % 100;
    let j = y / 100;
    let h = (day as i32 + 13 * (m as i32 + 1) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    ((h + 6) % 7) as u32
}

/// `Calendar(year, month, selectedDay)` — 月历（点击选日）。
pub fn Calendar(year: i32, month: u32, selected: MutableState<u32>, modifier: Modifier) {
    let colors = theme_colors();
    let cur = selected.get();
    let total = DaysInMonth(year, month);
    let first = WeekdayOf(year, month, 1);
    Column(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            // 星期表头：每个标题用 CenteredText 在 CALENDAR_CELL 宽度内水平居中，
            // 与日期网格（同样 CenteredText 居中）的列精确对齐。
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    for w in WEEK {
                        let s = w.to_string();
                        let id = add_node(
                            Policy::Leaf,
                            Modifier::new().size(tokens::CALENDAR_CELL, 20.0),
                        );
                        with_rt_pub(|rt| {
                            let st = tokens::label_small(colors.on_surface_variant);
                            rt.nodes[id.index()].draw =
                                Some(Box::new(move |b: &mut DrawBuilder, x, y, cw, ch| {
                                    crate::widgets::basic::CenteredText(b, x, y, cw, ch, &s, &st);
                                }));
                        });
                    }
                },
            );
            // 日期网格
            let cells = first as usize + total as usize;
            let rows = cells.div_ceil(7);
            for r in 0..rows {
                Row(
                    Modifier::new(),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        for col in 0..7 {
                            let idx = r * 7 + col;
                            let day = if idx >= first as usize && idx < cells {
                                Some((idx - first as usize + 1) as u32)
                            } else {
                                None
                            };
                            let Some(d) = day else {
                                Spacer(
                                    Modifier::new()
                                        .size(tokens::CALENDAR_CELL, tokens::CALENDAR_CELL),
                                );
                                continue;
                            };
                            let on = d == cur;
                            let id = add_node(
                                Policy::Box(Alignment::Center),
                                Modifier::new()
                                    .size(tokens::CALENDAR_CELL, tokens::CALENDAR_CELL)
                                    .background(
                                        if on {
                                            colors.primary
                                        } else {
                                            colors.Surface.with_alpha(0.0)
                                        },
                                        tokens::full_radius(tokens::CALENDAR_CELL),
                                    ),
                            );
                            with_rt_pub(|rt| {
                                rt.nodes[id.index()].on_click =
                                    Some(Box::new(move || selected.set(d)));
                                let s = d.to_string();
                                let st = if on {
                                    tokens::label_medium(colors.on_primary)
                                } else {
                                    tokens::label_medium(colors.on_surface)
                                };
                                rt.nodes[id.index()].draw =
                                    Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                                        crate::widgets::basic::CenteredText(b, x, y, w, h, &s, &st);
                                    }));
                            });
                        }
                    },
                );
            }
        },
    );
}

// ══════════════════════════════ 代码 / 富文本 ══════════════════════════════

/// 代码块（带行号，等宽近似）。
pub fn CodeBlock(code: &str, modifier: Modifier) {
    let colors = theme_colors();
    let lines: Vec<String> = code.lines().map(|l| l.to_string()).collect();
    let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32
        * tokens::CODE_CHAR_WIDTH
        + 2.0 * tokens::CODE_GUTTER;
    let id = add_node(
        Policy::Leaf,
        modifier
            .width(w)
            .background(colors.surface_variant, tokens::SHAPE_SMALL)
            .padding_ltrb(
                tokens::SPACE_MD,
                tokens::SPACE_SM,
                tokens::SPACE_MD,
                tokens::SPACE_SM,
            ),
    );
    with_rt_pub(|rt| {
        let num = tokens::label_small(colors.outline);
        let body = tokens::body_small(colors.on_surface_variant);
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
            for (i, l) in lines.iter().enumerate() {
                let ly = y + i as f32 * tokens::CODE_LINE_HEIGHT;
                b.draw_text(x, ly, &format!("{:>3}", i + 1), &num, f32::INFINITY);
                b.draw_text(x + tokens::CODE_GUTTER, ly, l, &body, f32::INFINITY);
            }
        }));
    });
}

/// 富文本：按 span 顺序排版（`(\u{6587}\u{672c}, \u{6837}\u{5f0f})` 列表）。
pub fn RichText(spans: &[(String, TextStyle)], modifier: Modifier) {
    let ss: Vec<(String, TextStyle)> = spans.to_vec();
    let id = add_node(Policy::Leaf, modifier);
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
            let mut cx = x;
            for (s, st) in &ss {
                b.draw_text(cx, y, s, st, f32::INFINITY);
                cx += s.chars().count() as f32 * st.size * 0.55;
            }
        }));
    });
}

// ══════════════════════════════ 设置项 ══════════════════════════════

/// 设置分区标题 + 内容。
pub fn SettingsSection(title: &str, modifier: Modifier, content: impl Fn() + 'static) {
    let colors = theme_colors();
    let t = title.to_string();
    Column(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            Text(
                t.clone(),
                tokens::title_small(colors.primary),
                Modifier::new().padding_ltrb(
                    tokens::LIST_PAD_H,
                    tokens::SPACE_LG,
                    0.0,
                    tokens::SPACE_SM,
                ),
            );
            content();
        },
    );
}

/// 设置条目：标题 + 副标题 + 右侧控件（开关 / 复选框 / 滑块…）。
pub fn SettingsItem(
    title: &str,
    subtitle: Option<&str>,
    modifier: Modifier,
    trailing: impl Fn() + 'static,
) {
    let colors = theme_colors();
    let t = title.to_string();
    let s = subtitle.map(|x| x.to_string());
    Row(
        modifier
            .fill_max_width()
            .padding_ltrb(
                tokens::LIST_PAD_H,
                tokens::SPACE_MD,
                tokens::LIST_PAD_H,
                tokens::SPACE_MD,
            )
            .min_height(tokens::LIST_ONE_LINE),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            let t2 = t.clone();
            let s2 = s.clone();
            Column(
                Modifier::new().weight(1.0, false),
                Arrangement::Center,
                CrossAxisAlignment::Start,
                move || {
                    Text(
                        t2.clone(),
                        tokens::body_large(colors.on_surface),
                        Modifier::new(),
                    );
                    if let Some(sub) = s2.clone() {
                        Text(
                            sub,
                            tokens::body_small(colors.on_surface_variant),
                            Modifier::new(),
                        );
                    }
                },
            );
            Spacer(Modifier::new().width(tokens::SPACE_MD));
            trailing();
        },
    );
}

/// 媒体占位（相机预览 / 视频表面：无平台解码器时的可视替身）。
pub fn MediaSurface(label: &str, modifier: Modifier) {
    crate::widgets::basic::ImagePlaceholder(label, modifier);
}

// ════════════════════ 补齐（advanced） ════════════════════

/// `AutoHideToolbar(hidden, height) { content }` — 隐藏时高度动画收缩到 0 并裁剪。
///
/// `hidden` 由调用方驱动（如滚动方向检测：下滚隐藏 / 上滚显示）。
pub fn AutoHideToolbar(
    hidden: MutableState<bool>,
    height: f32,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    use compose::animation::Tween;
    use compose::animation::animate_float_as_state;
    let h = hidden.get();
    let t = animate_float_as_state(if h { 0.0 } else { 1.0 }, Tween::new(0.2));
    compose::layout::Box(
        modifier.fill_max_width().height(t * height).clip(0.0),
        Alignment::TopStart,
        content,
    );
}
