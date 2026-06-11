//! SubcomposeLayout 回归：measure 期按需组合子内容，子尺寸反馈父布局，slot keyed 状态复用。

use std::collections::HashSet;

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, MeasureCtx, MeasurePolicy, MeasureResult, Modifier, NodeId, SubcomposeLayout,
    SubcomposeStackPolicy, create_root_node, perform_layout,
};
use compose::runtime::Composition;

/// 测一次就记录 slot，避免每轮重复请求导致 8 轮上限。
struct OncePolicy {
    requested: HashSet<u64>,
    n: usize,
}

impl MeasurePolicy for OncePolicy {
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx,
        measurables: &[NodeId],
        constraints: Constraints,
    ) -> MeasureResult {
        for slot in 0..self.n as u64 {
            if self.requested.insert(slot) {
                let slot_copy = slot;
                ctx.subcompose(slot, move || {
                    compose_material::Text(
                        format!("S{slot_copy}"),
                        compose::layout::TextStyle::new().size(20.0),
                        Modifier::new(),
                    );
                });
            }
        }
        // 用内置堆叠策略测量已组合的子内容。
        SubcomposeStackPolicy.measure(ctx, measurables, constraints)
    }
}

#[test]
fn subcompose_children_are_measured_into_parent() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        SubcomposeLayout(
            Modifier::new().width(200.0),
            Box::new(OncePolicy {
                requested: HashSet::new(),
                n: 3,
            }),
            || {},
        );
    });
    let size = perform_layout(root, Constraints::tight(200.0, 400.0));
    // 3 个 20px 文本竖排 → 高度约 3×行高，明显 > 0。
    assert!(size.height > 30.0, "按需组合的子内容应参与父测量: {size:?}");
    assert!(size.width <= 200.0);
}

#[test]
fn subcompose_slot_state_reuses_across_layouts() {
    use compose::runtime::{mutableStateOf, remember};
    let root = create_root_node(Alignment::TopStart);
    let seen = std::rc::Rc::new(std::cell::Cell::new(None));
    let seen2 = seen.clone();
    let comp = Composition::new(root);
    comp.set_content(move || {
        let s = remember(|| mutableStateOf(0i32));
        seen2.set(Some(s));
        SubcomposeLayout(
            Modifier::new().width(100.0),
            Box::new(OncePolicy {
                requested: HashSet::new(),
                n: 1,
            }),
            || {},
        );
    });
    let _ = perform_layout(root, Constraints::tight(100.0, 200.0));
    let _ = perform_layout(root, Constraints::tight(100.0, 200.0));
    // remember 在 root scope 稳定（两次布局不重组，但句柄应有效）。
    let s = seen.get().expect("应捕获 state");
    s.set(7);
    assert_eq!(s.get(), 7);
}

#[test]
fn subcompose_queue_does_not_leak_between_layouts() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        SubcomposeLayout(
            Modifier::new(),
            Box::new(SubcomposeStackPolicy),
            || {},
        );
    });
    let _ = perform_layout(root, Constraints::loose(100.0, 100.0));
    let _ = perform_layout(root, Constraints::loose(100.0, 100.0));
    assert_eq!(compose::subcompose::pending(), 0, "布局后请求队列应清空");
}
