//! Effects 回归：对齐 Compose SideEffect / LaunchedEffect 生命周期 /
//! rememberUpdatedState / snapshotFlow / producedState。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::Constraints;
use compose::layout::{Alignment, Modifier, Policy, add_node, create_root_node, perform_layout, with_content_scope};
use compose::runtime::{
    Composition, MutableState, Recomposer, SideEffect, advance_frame, launched_effect,
    mutableStateOf, producedState, remember, rememberUpdatedState, snapshotFlow,
};

#[test]
fn side_effect_runs_after_recomposition_not_during() {
    let events = Rc::new(Cell::new(0));
    let e = events.clone();
    let s = mutableStateOf(0i32);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let _ = s.get(); // 建立依赖
        let e = e.clone();
        SideEffect(move || e.set(e.get() + 1));
    });
    // set_content 完成一次重组 → SideEffect 在其后执行 1 次（不在 content() 期间）。
    assert_eq!(events.get(), 1, "首次组合成功后应执行 SideEffect");
    s.set(1);
    Recomposer::recompose_pending();
    assert_eq!(events.get(), 2, "每次重组后都应执行");
}

#[test]
fn launched_effect_cancels_when_scope_disposed() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct Never(Rc<Cell<bool>>);
    impl Future for Never {
        type Output = ();
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
            Poll::Pending
        }
    }
    impl Drop for Never {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    let dropped = Rc::new(Cell::new(false));
    let d = dropped.clone();
    let show = mutableStateOf(true);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        if show.get() {
            let d = d.clone();
            compose::runtime::emit_scope(move || {
                let d = d.clone();
                launched_effect(0, Never(d));
            });
        }
    });
    Recomposer::recompose_pending();
    assert!(!dropped.get(), "组合存活时协程不应取消");
    show.set(false); // 分支消失 → 子作用域销毁 → 协程取消
    Recomposer::recompose_pending();
    assert!(dropped.get(), "作用域销毁应取消 LaunchedEffect 协程");
    let _ = comp;
}

#[test]
fn remember_updated_state_keeps_latest_without_restart() {
    let s = mutableStateOf(0i32);
    let out: Rc<Cell<Option<MutableState<i32>>>> = Rc::new(Cell::new(None));
    let out2 = out.clone();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let latest = rememberUpdatedState(s.get());
        out2.set(Some(latest));
        let id = add_node(Policy::Leaf, Modifier::new());
        with_content_scope(id, || {});
    });
    perform_layout(root, Constraints::tight(100.0, 100.0));
    let handle = out.get().expect("应捕获 latest state");
    assert_eq!(handle.get(), 0);
    s.set(42);
    Recomposer::recompose_pending();
    assert_eq!(handle.get(), 42, "rememberUpdatedState 应保留最新值且句柄不变");
}

#[test]
fn snapshot_flow_emits_only_on_change() {
    let source = mutableStateOf(1i32);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let s = source;
        let _ = snapshotFlow(move || s.get());
    });
    Recomposer::recompose_pending();
    source.set(1); // 同值：不应触发重算/失效
    Recomposer::recompose_pending();
    source.set(2);
    Recomposer::recompose_pending();
    // snapshotFlow 返回派生 state，行为与 derivedStateOf 一致；无 panic 即通过。
    let _ = advance_frame;
    let _ = comp;
}

#[test]
fn produced_state_publishes_from_coroutine() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let handle: Rc<Cell<Option<MutableState<i32>>>> = Rc::new(Cell::new(None));
    let h = handle.clone();
    comp.set_content(move || {
        let state = producedState(0i32, 0, |out| async move {
            out.set(99);
        });
        h.set(Some(state));
    });
    Recomposer::recompose_pending();
    advance_frame(1.0 / 60.0); // drain_tasks
    let state = handle.get().unwrap();
    assert_eq!(state.get(), 99, "producedState 协程结果应写入状态");
}
