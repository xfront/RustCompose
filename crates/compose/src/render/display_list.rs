//! DrawCmd → valo DisplayList 转换（display_list_builder 模式）。

use std::collections::HashMap;

use compose::layout::{DrawCmd, TextStyle};
use compose::prelude::Color;
use valo::{
    Backdrop, Color as ValoColor, DisplayList, DisplayListBuilder, DrawParagraphExt, FillRule, Paint,
    PaintStyle, PathBuilder, Point as ValoPoint, Rect as ValoRect, Stroke, ClipOp,
    FontCollection, Paragraph, ParagraphBuilder, TextStyle as ValoTextStyle,
};

pub fn to_valo_color(c: &Color) -> ValoColor {
    ValoColor::rgba(c.r, c.g, c.b, c.a)
}

fn to_valo_rect(x: f32, y: f32, w: f32, h: f32) -> ValoRect {
    ValoRect::from_ltrb(x, y, x + w, y + h)
}

/// 渐变轴：`vertical` = 上→下，否则左→右。
fn gradient_axis(x: f32, y: f32, w: f32, h: f32, vertical: bool) -> (ValoPoint, ValoPoint) {
    if vertical {
        (ValoPoint::new(x, y), ValoPoint::new(x, y + h))
    } else {
        (ValoPoint::new(x, y), ValoPoint::new(x + w, y))
    }
}

/// 点列 → valo 路径（`closed` 时自动闭合）。
fn build_valo_path(points: &[(f32, f32)], closed: bool) -> std::sync::Arc<valo::Path> {
    let mut pb = PathBuilder::new();
    if let Some((x, y)) = points.first() {
        pb.move_to(ValoPoint::new(*x, *y));
        for (px, py) in &points[1..] {
            pb.line_to(ValoPoint::new(*px, *py));
        }
        if closed {
            pb.close();
        }
    }
    pb.build()
}

/// 段落缓存（key = 文本|字号|颜色|族|字重|斜体|行高|字距|折行宽度）。
pub struct TextContext {
    pub font_collection: FontCollection,
    para_cache: HashMap<String, (Paragraph, f32)>,
}

const PARA_CACHE_LIMIT: usize = 4096;

/// 折行宽度归一：无界用 `f32::MAX`（valo 按内容自适应），且和缓存 key 同语义。
fn layout_width(max_w: f32) -> f32 {
    if max_w.is_finite() && max_w > 0.0 { max_w } else { f32::MAX }
}

impl TextContext {
    pub fn new(font_collection: FontCollection) -> Self {
        Self { font_collection, para_cache: HashMap::new() }
    }

    /// valo 真实文本度量（布局阶段的 TextMeasurer 实现）。
    pub fn measure(&mut self, Text: &str, style: &TextStyle) -> (f32, f32) {
        self.measure_at(Text, style, f32::INFINITY)
    }

    /// 按 `max_w` 折行后的真实度量：`(最长行宽, 多行总高)`。
    pub fn measure_at(&mut self, Text: &str, style: &TextStyle, max_w: f32) -> (f32, f32) {
        if Text.is_empty() { return (0.0, 0.0); }
        let (para, _) = self.get_paragraph(Text, style, layout_width(max_w));
        (para.longest_line(), para.height())
    }

    fn get_paragraph(&mut self, Text: &str, style: &TextStyle, max_w: f32) -> &(Paragraph, f32) {
        let key = format!(
            "{}\u{1}{}\u{1}{:02x}{:02x}{:02x}{:02x}\u{1}{}\u{1}{}\u{1}{}\u{1}{:?}\u{1}{}\u{1}{}",
            Text, style.size,
            (style.color.r * 255.0) as u8, (style.color.g * 255.0) as u8,
            (style.color.b * 255.0) as u8, (style.color.a * 255.0) as u8,
            style.family, style.weight, style.italic as u8,
            style.line_height, style.letter_spacing,
            // 折行宽度必须进 key：同一句话在卡片正文和标题里宽度不同，段落不能复用。
            (max_w * 2.0) as i64,
        );
        if !self.para_cache.contains_key(&key) {
            // ParagraphBuilder 需要 &mut FontCollection — 短暂 take（缓存命中路径零成本）
            let mut fc = std::mem::take(&mut self.font_collection);
            let mut pb = ParagraphBuilder::new(&mut fc);
            let vstyle = ValoTextStyle {
                families: if style.family.is_empty() { Vec::new() } else { vec![style.family.clone()] },
                weight: style.weight,
                italic: style.italic,
                size: style.size,
                color: to_valo_color(&style.color),
                letter_spacing: style.letter_spacing,
                // valo 的 `height` 是 size 的倍数；我方按绝对像素给排版令牌，这里换算。
                height: style.line_height.map(|px| px / style.size),
                ..Default::default()
            };
            pb.add_text(Text, &vstyle);
            let mut paragraph = pb.build();
            paragraph.layout(max_w);
            let baseline = paragraph
                .lines()
                .first()
                .map(|line| line.baseline)
                .unwrap_or(0.0);
            if self.para_cache.len() >= PARA_CACHE_LIMIT {
                let drop_n = self.para_cache.len() / 4;
                let keys: Vec<String> = self.para_cache.keys().take(drop_n).cloned().collect();
                for k in keys {
                    self.para_cache.remove(&k);
                }
            }
            self.para_cache.insert(key.clone(), (paragraph, baseline));
            self.font_collection = fc;
        }
        self.para_cache.get(&key).unwrap()
    }
}

/// DrawCmd 序列 → DisplayList（绝对坐标，identity 变换）。
pub fn build_display_list(cmds: &[DrawCmd], bg: Color, tc: &mut TextContext) -> DisplayList {
    build_display_list_with(cmds, Some(bg), tc)
}

/// 同 `build_display_list`，但可省略整屏背景。
///
/// 分层重绘必须传 `None`：动态层叠在静态层之上，若再画一次背景就会把
/// 静态层（侧栏、标题栏）整块盖掉。
pub fn build_display_list_with(
    cmds: &[DrawCmd],
    bg: Option<Color>,
    tc: &mut TextContext,
) -> DisplayList {
    let mut b = DisplayListBuilder::new();
    if let Some(bg) = bg {
        draw_bg_rect(&mut b, &bg);
    }
    record_cmds(&mut b, cmds, tc);
    b.build()
}

/// 静态块 → 可跨帧复用的 DisplayList（不含背景：一块只负责自己的墨）。
pub fn build_cmds_display_list(cmds: &[DrawCmd], tc: &mut TextContext) -> DisplayList {
    let mut b = DisplayListBuilder::new();
    record_cmds(&mut b, cmds, tc);
    b.build()
}

/// 保留模式拼帧的起点：一条空 builder，可选整屏背景打底。
pub fn new_frame_builder(bg: Option<Color>) -> DisplayListBuilder {
    let mut b = DisplayListBuilder::new();
    if let Some(bg) = bg {
        draw_bg_rect(&mut b, &bg);
    }
    b
}

/// 把命令录进**已有的** builder：保留调用方此刻已建立的裁剪栈与 z 序。
///
/// 保留模式逐块拼一帧时靠它把现场块接在静态块之后，顺序与绘制树完全一致。
pub fn record_cmds(b: &mut DisplayListBuilder, cmds: &[DrawCmd], tc: &mut TextContext) {
    for cmd in cmds {
        match cmd {
            DrawCmd::FillRect { x, y, w, h, color } => {
                let p = Paint { color: to_valo_color(color), style: PaintStyle::Fill, ..Default::default() };
                b.draw_rect(to_valo_rect(*x, *y, *w, *h), &p);
            }
            DrawCmd::FillRRect { x, y, w, h, radius, color } => {
                let p = Paint { color: to_valo_color(color), style: PaintStyle::Fill, ..Default::default() };
                b.draw_rrect(to_valo_rect(*x, *y, *w, *h), *radius, &p);
            }
            DrawCmd::FillRectGradient { x, y, w, h, from, to, vertical } => {
                let (start, end) = gradient_axis(*x, *y, *w, *h, *vertical);
                let shader = valo::Shader::linear(start, end, to_valo_color(from), to_valo_color(to));
                b.draw_rect(to_valo_rect(*x, *y, *w, *h), &Paint::from_shader(shader));
            }
            DrawCmd::FillRRectGradient { x, y, w, h, radius, from, to, vertical } => {
                let (start, end) = gradient_axis(*x, *y, *w, *h, *vertical);
                let shader = valo::Shader::linear(start, end, to_valo_color(from), to_valo_color(to));
                b.draw_rrect(to_valo_rect(*x, *y, *w, *h), *radius, &Paint::from_shader(shader));
            }
            DrawCmd::StrokeRRect { x, y, w, h, radius, color, line } => {
                let p = Paint {
                    color: to_valo_color(color),
                    style: PaintStyle::Stroke(Stroke::new(*line)),
                    ..Default::default()
                };
                b.draw_rrect(to_valo_rect(*x, *y, *w, *h), *radius, &p);
            }
            DrawCmd::FillCircle { cx, cy, r, color } => {
                let p = Paint { color: to_valo_color(color), style: PaintStyle::Fill, ..Default::default() };
                b.draw_circle(ValoPoint::new(*cx, *cy), *r, &p);
            }
            DrawCmd::StrokeCircle { cx, cy, r, color, line } => {
                let p = Paint {
                    color: to_valo_color(color),
                    style: PaintStyle::Stroke(Stroke::new(*line)),
                    ..Default::default()
                };
                b.draw_circle(ValoPoint::new(*cx, *cy), *r, &p);
            }
            DrawCmd::Line { x0, y0, x1, y1, color, line } => {
                let p = Paint {
                    color: to_valo_color(color),
                    style: PaintStyle::Stroke(Stroke::new(*line)),
                    ..Default::default()
                };
                let mut pb = PathBuilder::new();
                pb.move_to(ValoPoint::new(*x0, *y0));
                pb.line_to(ValoPoint::new(*x1, *y1));
                b.draw_path(&pb.build(), FillRule::NonZero, &p);
            }
            DrawCmd::FillPath { points, color } => {
                let p = Paint { color: to_valo_color(color), style: PaintStyle::Fill, ..Default::default() };
                b.draw_path(&build_valo_path(points, true), FillRule::NonZero, &p);
            }
            DrawCmd::StrokePath { points, color, line } => {
                let p = Paint {
                    color: to_valo_color(color),
                    style: PaintStyle::Stroke(Stroke::new(*line)),
                    ..Default::default()
                };
                b.draw_path(&build_valo_path(points, false), FillRule::NonZero, &p);
            }
            DrawCmd::GlassPanel { x, y, w, h, radius, tint, blur, highlight, outline, line } => {
                let rect = to_valo_rect(*x, *y, *w, *h);
                b.save();
                b.clip_rrect(rect, *radius, ClipOp::Intersect);
                // 背景模糊层：以模糊后的场景为底，孩子（tint / 高光）画在其上
                b.save_layer_backdrop(Some(rect), &Paint::default(), Backdrop::blur(*blur));
                let tint_paint = Paint { color: to_valo_color(tint), style: PaintStyle::Fill, ..Default::default() };
                b.draw_rrect(rect, *radius, &tint_paint);
                // 镜面高光：上缘亮线 + 整圈内描边
                let spec = Paint {
                    color: to_valo_color(highlight),
                    style: PaintStyle::Stroke(Stroke::new(*line)),
                    ..Default::default()
                };
                b.draw_rrect(rect, *radius, &spec);
                b.draw_rrect(
                    ValoRect::from_ltrb(*x + *line, *y + *line,
                                        *x + *w - *line, *y + *h - *line),
                    (*radius - *line).max(0.0),
                    &Paint {
                        color: to_valo_color(&highlight.with_alpha(highlight.a * 0.45)),
                        style: PaintStyle::Stroke(Stroke::new(*line * 0.8)),
                        ..Default::default()
                    },
                );
                b.restore();
                b.restore();
                // 描边压在最上层，勾出玻璃轮廓
                if outline.a > 0.0 {
                    let op = Paint {
                        color: to_valo_color(outline),
                        style: PaintStyle::Stroke(Stroke::new(*line)),
                        ..Default::default()
                    };
                    b.draw_rrect(rect, *radius, &op);
                }
            }
            DrawCmd::Text { x, y, text, style, max_w } => {
                if text.is_empty() { continue; }
                let (para, _baseline) = tc.get_paragraph(text, style, layout_width(*max_w));
                // DrawCmd 的 y = 节点顶；valo draw_paragraph 原点 = 段落左上角
                // （首行基线 = 原点 + line.baseline），字形顶 ≈ 原点。
                b.draw_paragraph(para, ValoPoint::new(*x, *y));
            }
            DrawCmd::ClipPush { x, y, w, h, radius } => {
                b.save();
                b.clip_rrect(to_valo_rect(*x, *y, *w, *h), *radius, ClipOp::Intersect);
            }
            DrawCmd::ClipPop => {
                b.restore();
            }
        }
    }
}

/// 整屏背景。与呈现目标的清屏色同值，兜住没画到的像素。
fn draw_bg_rect(b: &mut DisplayListBuilder, bg: &Color) {
    let bg_paint = Paint { color: to_valo_color(bg), ..Default::default() };
    b.draw_rect(ValoRect::new(0.0, 0.0, 1_000_000.0, 1_000_000.0), &bg_paint);
}
