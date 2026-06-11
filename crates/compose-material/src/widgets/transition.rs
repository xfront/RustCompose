//! AnimatedVisibility / AnimatedContent — 对齐 Jetpack Compose 的可见性与内容切换转场。
//!
//! Compose 的 `AnimatedVisibility` 在 visible=false 时先播 ExitTransition，完成后才把
//! 内容从组合中移除；`AnimatedContent` 在 targetState 变化时让内容交叉过渡。
//! 本实现用工程已有的 `animate_float_as_state` 驱动进度，用 `remember` 状态机跟踪生命周期，
//! alpha/scale/slide 走 `Modifier.graphics_layer`（真实图层，非遮罩近似）。
//!
//! 与旧 `animation::AnimatedVisibility`（高度折叠 + 同色遮罩近似）并存，本模块提供
//! 带完整生命周期语义的版本，命名带 `Ex` 以区分，避免破坏既有调用方。

use compose::animation::Tween;
use compose::layout::{Alignment, Box as LayoutBox, Modifier};
use compose::runtime::{mutableStateOf, remember};

/// 可见性转场规格（对齐 Compose `EnterTransition` / `ExitTransition` 的简化集）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VisibilityTransition {
    /// 仅淡入淡出。
    Fade,
    /// 淡入淡出 + 从上方滑入（位移按 extent 比例）。
    SlideDown(f32),
    /// 淡入淡出 + 缩放（enter 从 scale 到 1，exit 反向）。
    Scale(f32),
}

impl VisibilityTransition {
    fn apply(self, progress: f32) -> (f32, f32, f32) {
        match self {
            VisibilityTransition::Fade => (progress, 1.0, 0.0),
            VisibilityTransition::SlideDown(dist) => (progress, 1.0, -dist * (1.0 - progress)),
            VisibilityTransition::Scale(from) => {
                (progress, from + (1.0 - from) * progress, 0.0)
            }
        }
    }
}

/// `AnimatedVisibilityEx(visible, enter, exit, content)` — 带 Enter/Exit 生命周期的可见性。
///
/// `present` 表示内容是否留在组合中：visible=false 时先播 exit，
/// 进度归零后下一轮重组才移除内容（dispose）；visible=true 时立即重新组合并播 enter。
pub fn AnimatedVisibilityEx(
    visible: bool,
    enter: VisibilityTransition,
    exit: VisibilityTransition,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let present = remember(|| mutableStateOf(visible));
    if visible {
        present.set(true);
    }
    let target = remember(|| mutableStateOf(if visible { 1.0f32 } else { 0.0 }));
    target.set(if visible { 1.0 } else { 0.0 });

    let p = compose::animation::animate_float_as_state(target.get(), Tween::new(0.25));

    if present.get() {
        let (alpha, scale, offset_y) = (if visible { enter } else { exit }).apply(p);
        LayoutBox(
            modifier.graphics_layer(alpha, scale, scale, 0.0, 0.0, offset_y, 0.5, 0.5),
            Alignment::TopStart,
            content,
        );
        // exit 完成后移除内容（下一轮重组生效，避免在本轮 content() 内 dispose 自身）。
        if !visible && p <= 0.001 {
            present.set(false);
        }
    }
}

/// 内容切换转场（对齐 Compose `ContentTransform` 的简化集）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContentTransform {
    /// 新内容淡入（交叉淡出的简化：旧内容在切换时立即 dispose）。
    FadeIn,
    /// 新内容从下方滑入 + 淡入。
    SlideInVertically(f32),
    /// 新内容缩放淡入。
    ScaleIn(f32),
}

/// `AnimatedContent(target_state, transform, content)` — key 切换时的入场转场。
///
/// key 变化时 `animate_progress_on` 重播 0→1，新内容从 progress=0 播入场动画。
/// 与 Compose 的差异：旧内容不保留做并行 exit（引擎不能在 measure 期组合，
/// 双内容并行需要更深重构），但入场转场语义一致。
pub fn AnimatedContent(
    target_state: u64,
    transform: ContentTransform,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    // key 变化触发 animate_progress_on 重播 0→1。
    let p = compose::animation::animate_progress_on(target_state, Tween::new(0.3));
    let (alpha, scale, offset_y) = match transform {
        ContentTransform::FadeIn => (p, 1.0, 0.0),
        ContentTransform::SlideInVertically(d) => (p, 1.0, d * (1.0 - p)),
        ContentTransform::ScaleIn(from) => (p, from + (1.0 - from) * p, 0.0),
    };
    LayoutBox(
        modifier.graphics_layer(alpha, scale, scale, 0.0, 0.0, offset_y, 0.5, 0.5),
        Alignment::TopStart,
        content,
    );
}
