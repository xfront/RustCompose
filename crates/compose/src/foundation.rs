//! Compose foundation — 单位、几何、颜色、约束。
//!
//! 对齐 `androidx.compose.ui.geometry` / `unit` / `Constraints`。

use std::fmt;

// ── Dp ──────────────────────────────────────────────────────────────────────

/// 密度无关像素（Compose `Dp`）。运行时按 `Density` 缩放为 px。
#[derive(Clone, Copy, PartialEq, PartialOrd, Default, Debug)]
pub struct Dp(pub f32);

pub fn dp(v: f32) -> Dp {
    Dp(v)
}
pub fn sp(v: f32) -> Dp {
    Dp(v)
} // 文本字号同 Dp 处理（desktop 单一 density）

impl Dp {
    pub fn to_px(self, density: f32) -> f32 {
        self.0 * density
    }
}

// ── Color ───────────────────────────────────────────────────────────────────

/// 线性 sRGBA，各分量 0..1（对齐 Compose `Color`，非打包 u32）。
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// `f32` 没有 `Hash`（NaN 不等于自身，会破坏 hash/eq 契约）。渲染层要对绘制命令做
/// 内容寻址（静态/动态分区），故这里按位哈希。
///
/// 与 `PartialEq` 唯一的不一致处是 `0.0 == -0.0` 和 NaN：两者位模式不同即哈希不同，
/// 后果只是把本来相同的内容多判一帧动态，不会把不同的内容误判成静态。
#[inline]
pub(crate) fn hash_f32<H: std::hash::Hasher>(h: &mut H, v: f32) {
    std::hash::Hash::hash(&v.to_bits(), h);
}

impl std::hash::Hash for Color {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        hash_f32(state, self.r);
        hash_f32(state, self.g);
        hash_f32(state, self.b);
        hash_f32(state, self.a);
    }
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }
    pub const WHITE: Color = Self::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Color = Self::rgb(0.0, 0.0, 0.0);
    pub const TRANSPARENT: Color = Self::rgba(0.0, 0.0, 0.0, 0.0);

    /// 0xRRGGBB / 0xAARRGGBB 便捷构造。
    pub const fn hex(v: u32) -> Color {
        let (a, r, g, b) = if v <= 0xFF_FFFF {
            (255, (v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF)
        } else {
            (
                (v >> 24) & 0xFF,
                (v >> 16) & 0xFF,
                (v >> 8) & 0xFF,
                v & 0xFF,
            )
        };
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    pub fn with_alpha(self, a: f32) -> Color {
        Self { a, ..self }
    }

    /// 线性插值（`t = 0` 取 `self`，`t = 1` 取 `other`，含 alpha）。
    pub fn mix(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        Color {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// 相对亮度（Rec.709 权重，sRGB 空间近似）：用于挑选 on-color 与判断暗色主题。
    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// 与白色/黑色混合（用于 state layer，如 ripple 底）。
    pub fn composite_over(self, other: Color) -> Color {
        let a = self.a + other.a * (1.0 - self.a);
        if a <= 0.0 {
            return Color::TRANSPARENT;
        }
        Color {
            r: (self.r * self.a + other.r * other.a * (1.0 - self.a)) / a,
            g: (self.g * self.a + other.g * other.a * (1.0 - self.a)) / a,
            b: (self.b * self.a + other.b * other.a * (1.0 - self.a)) / a,
            a,
        }
    }
}

// ── 几何 ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub const ZERO: Offset = Offset { x: 0.0, y: 0.0 };
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn is_zero(self) -> bool {
        self.x == 0.0 && self.y == 0.0
    }
    pub fn add(self, o: Offset) -> Offset {
        Offset::new(self.x + o.x, self.y + o.y)
    }
    pub fn sub(self, o: Offset) -> Offset {
        Offset::new(self.x - o.x, self.y - o.y)
    }
}

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            width: w,
            height: h,
        }
    }
    pub fn infinite() -> Self {
        Self {
            width: f32::INFINITY,
            height: f32::INFINITY,
        }
    }
    pub fn contains(self, p: Offset) -> bool {
        p.x >= 0.0 && p.y >= 0.0 && p.x <= self.width && p.y <= self.height
    }
}

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn from_ltwh(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        }
    }
    pub fn width(&self) -> f32 {
        self.right - self.left
    }
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }
    pub fn contains(&self, p: Offset) -> bool {
        p.x >= self.left && p.x < self.right && p.y >= self.top && p.y < self.bottom
    }
}

// ── Constraints（Compose `Constraints`）─────────────────────────────────────

/// 双向 [min, max] 测量约束。`max = INFINITY` 表示无界。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Constraints {
    pub const INFINITY: f32 = f32::INFINITY;

    pub fn new(min_w: f32, max_w: f32, min_h: f32, max_h: f32) -> Self {
        Self {
            min_width: min_w,
            max_width: max_w,
            min_height: min_h,
            max_height: max_h,
        }
    }
    pub fn tight(w: f32, h: f32) -> Self {
        Self::new(w, w, h, h)
    }
    pub fn loose(max_w: f32, max_h: f32) -> Self {
        Self::new(0.0, max_w, 0.0, max_h)
    }

    pub fn has_bounded_width(&self) -> bool {
        self.max_width.is_finite()
    }
    pub fn has_bounded_height(&self) -> bool {
        self.max_height.is_finite()
    }
    pub fn is_tight(&self) -> bool {
        self.min_width == self.max_width && self.min_height == self.max_height
    }

    pub fn constrain(&self, w: f32, h: f32) -> Size {
        Size::new(self.constrain_width(w), self.constrain_height(h))
    }
    pub fn constrain_width(&self, w: f32) -> f32 {
        w.clamp(self.min_width, self.max_width)
    }
    pub fn constrain_height(&self, h: f32) -> f32 {
        h.clamp(self.min_height, self.max_height)
    }

    /// 收紧 min 边界（Compose `copy(minWidth = …)` 组合）。
    pub fn with_min(&self, min_w: f32, min_h: f32) -> Constraints {
        Self::new(
            min_w.max(self.min_width),
            self.max_width.max(min_w.max(self.min_width)),
            min_h.max(self.min_height),
            self.max_height.max(min_h.max(self.min_height)),
        )
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Self::new(0.0, Self::INFINITY, 0.0, Self::INFINITY)
    }
}

impl fmt::Display for Constraints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Constraints(w {}..{}, h {}..{})",
            self.min_width, self.max_width, self.min_height, self.max_height
        )
    }
}

// ── Density ─────────────────────────────────────────────────────────────────

/// 密度上下文（Compose `Density`），desktop 默认 1.0。
#[derive(Clone, Copy, Debug)]
pub struct Density {
    pub density: f32,
}

impl Density {
    pub fn dp_to_px(&self, dp: Dp) -> f32 {
        dp.0 * self.density
    }
    pub fn px_to_dp(&self, px: f32) -> Dp {
        Dp(px / self.density)
    }
}

impl Default for Density {
    fn default() -> Self {
        Self { density: 1.0 }
    }
}
