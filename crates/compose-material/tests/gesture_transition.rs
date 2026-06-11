//! 手势 pointerInput 与 AnimatedVisibility/AnimatedContent 回归。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::Constraints;
use compose::gesture::{
    PointerEvent, PointerEventType, PointerInput, detect_tap_gestures, detect_drag_gestures,
    dispatch_pointer_event, drag_config, tap_config,
};
use compose::layout::{Alignment, Modifier, create_root_node, perform_layout};
use compose::runtime::Composition;
use compose_material::{AnimatedContent, AnimatedVisibilityEx, ContentTransform, Text, VisibilityTransition};

#[test]
fn pointer_input_tap_via_dispatch_chain() {
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let h = h.clone();
        PointerInput(Modifier::new().size(200.0, 200.0), || {}, detect_tap_gestures(tap_config(
            move |_, _| h.set(h.get() + 1),
            |_, _| {},
            |_, _| {},
        )));
    });
    perform_layout(root, Constraints::tight(200.0, 200.0));

    // pointerInput 节点是 root 的子节点；用 root 作为 target 沿祖先链找不到 handler，
    // 需命中该节点。这里用 hit_test 在 (0,0) 命中（节点 fill 区域）。
    let (target, _, _) = compose::layout::hit_test(root, 5.0, 5.0).expect("应命中 pointerInput 节点");
    let base = 1_000_000_000u64;
    dispatch_pointer_event(target, PointerEvent { kind: PointerEventType::Press, x: 10.0, y: 10.0, dx: 0.0, dy: 0.0, time_ns: base });
    dispatch_pointer_event(target, PointerEvent { kind: PointerEventType::Release, x: 10.0, y: 10.0, dx: 0.0, dy: 0.0, time_ns: base + 100_000_000 });
    assert_eq!(hits.get(), 1, "点击应触发 tap 回调");
}

#[test]
fn pointer_input_drag_via_dispatch_chain() {
    let drags = Rc::new(Cell::new(0));
    let d = drags.clone();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let d = d.clone();
        PointerInput(Modifier::new().size(200.0, 200.0), || {}, detect_drag_gestures(drag_config(
            || {},
            move |_, _| d.set(d.get() + 1),
            || {},
        )));
    });
    perform_layout(root, Constraints::tight(200.0, 200.0));
    let (target, _, _) = compose::layout::hit_test(root, 5.0, 5.0).expect("应命中");
    dispatch_pointer_event(target, PointerEvent { kind: PointerEventType::Press, x: 10.0, y: 10.0, dx: 0.0, dy: 0.0, time_ns: 0 });
    dispatch_pointer_event(target, PointerEvent { kind: PointerEventType::Move, x: 30.0, y: 10.0, dx: 0.0, dy: 0.0, time_ns: 10_000_000 });
    assert_eq!(drags.get(), 1, "超过 slop 的拖动应触发 onDrag");
}

#[test]
fn animated_visibility_ex_renders_and_hides() {
    let visible = compose::runtime::mutableStateOf(true);
    let v = visible;
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let v = v.get();
        AnimatedVisibilityEx(v, VisibilityTransition::Fade, VisibilityTransition::Fade, Modifier::new(), || {
            Text("Hello", compose::layout::TextStyle::new(), Modifier::new());
        });
    });
    perform_layout(root, Constraints::tight(200.0, 200.0));
    let cmds = compose::layout::draw_tree(root);
    assert!(cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::Text { .. })), "visible 时应绘制内容");

    // 隐藏：退出动画完成后内容应被 dispose（多帧推进）。
    visible.set(false);
    for _ in 0..30 {
        compose::runtime::Recomposer::recompose_pending();
        compose::runtime::advance_frame(1.0 / 60.0);
        perform_layout(root, Constraints::tight(200.0, 200.0));
    }
    let cmds = compose::layout::draw_tree(root);
    assert!(
        !cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::Text { .. })),
        "exit 完成后内容应从语义/绘制树移除"
    );
}

#[test]
fn animated_content_renders_new_state() {
    let state = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let s = state.clone();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let st = s.get();
        AnimatedContent(st, ContentTransform::FadeIn, Modifier::new(), move || {
            Text(format!("State {st}"), compose::layout::TextStyle::new(), Modifier::new());
        });
    });
    perform_layout(root, Constraints::tight(200.0, 200.0));
    let cmds = compose::layout::draw_tree(root);
    assert!(cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::Text { text, .. } if text == "State 0")));

    state.set(1);
    compose::runtime::advance_frame(1.0 / 60.0);
    perform_layout(root, Constraints::tight(200.0, 200.0));
    let cmds = compose::layout::draw_tree(root);
    assert!(cmds.iter().any(|c| matches!(c, compose::layout::DrawCmd::Text { text, .. } if text == "State 1")));
}
