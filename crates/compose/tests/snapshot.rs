//! Snapshot 集成回归：对齐 Jetpack Compose Snapshot 的隔离读 / 批量写 / apply 提交 /
//! read-write observer。默认（无快照）路径行为不变由 runtime_semantics 覆盖。

use std::cell::Cell;

use compose::foundation::Constraints;
use compose::layout::{Alignment, DrawCmd, Modifier, create_root_node, draw_tree, perform_layout};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, apply_snapshot, mutableStateOf};
use compose::snapshot::{
    discard, in_snapshot, register_read_observer, register_write_observer, snapshot,
    unregister_read_observer, unregister_write_observer, withMutableSnapshot,
};

#[test]
fn mutable_snapshot_writes_are_isolated_until_apply() {
    let s = mutableStateOf(1i32);
    let (snap, _) = withMutableSnapshot(|| {
        s.set(42);
        assert_eq!(s.get(), 42, "快照内读应看到自己的写");
    });
    assert_eq!(s.get(), 1, "apply 前全局值不变");
    apply_snapshot(snap);
    assert_eq!(s.get(), 42, "apply 后全局值提交");
}

#[test]
fn snapshot_read_is_frozen_against_later_global_writes() {
    let s = mutableStateOf(1i32);
    let (snap, first) = snapshot(|| s.get());
    assert_eq!(first, 1);
    s.set(9); // 全局写
    // 同一快照内再读：命中读缓存，仍是快照时刻的值。
    let frozen = compose::snapshot::enter_snapshot(snap, || s.get());
    assert_eq!(frozen, 1, "快照读应隔离后续全局写");
    discard(snap);
}

#[test]
fn nested_snapshots_restore_and_isolate() {
    let s = mutableStateOf(0i32);
    let (outer, _) = withMutableSnapshot(|| {
        s.set(10);
        assert!(in_snapshot());
        let (inner, _) = withMutableSnapshot(|| {
            s.set(20);
            assert_eq!(s.get(), 20);
        });
        apply_snapshot(inner);
        // 外层已缓存自己写入的 10（快照读冻结语义），内层 apply 到全局不改变外层视图。
        assert_eq!(s.get(), 10, "外层读视图冻结在自己写入的值");
        assert!(in_snapshot(), "内层退出后仍在外层");
    });
    apply_snapshot(outer);
    assert_eq!(s.get(), 10, "外层 apply 后全局 = 外层写值");
}

#[test]
fn observers_fire_on_snapshot_writes() {
    thread_local! {
        static READS: Cell<usize> = const { Cell::new(0) };
        static WRITES: Cell<usize> = const { Cell::new(0) };
    }
    let r = register_read_observer(|_| READS.with(|c| c.set(c.get() + 1)));
    let w = register_write_observer(|_| WRITES.with(|c| c.set(c.get() + 1)));
    let s = mutableStateOf(0i32);
    let (snap, _) = withMutableSnapshot(|| {
        let _ = s.get();
        s.set(1);
    });
    assert_eq!(READS.with(|c| c.get()), 1, "快照读应通知读观察者");
    assert_eq!(WRITES.with(|c| c.get()), 1, "快照写应通知写观察者");
    apply_snapshot(snap);
    unregister_read_observer(r);
    unregister_write_observer(w);
}

#[test]
fn apply_snapshot_triggers_recomposition() {
    let s = mutableStateOf(0i32);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let label = format!("count={}", s.get());
            compose_material::Text(label, compose::layout::TextStyle::new(), Modifier::new());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 200.0));
    assert!(draw_tree(root).iter().any(|c| matches!(c, DrawCmd::Text { text, .. } if text == "count=0")));

    let (snap, _) = withMutableSnapshot(|| s.set(5));
    Recomposer::recompose_pending();
    assert!(
        !draw_tree(root).iter().any(|c| matches!(c, DrawCmd::Text { text, .. } if text == "count=5")),
        "apply 前不应重组"
    );
    apply_snapshot(snap);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 200.0));
    assert!(
        draw_tree(root).iter().any(|c| matches!(c, DrawCmd::Text { text, .. } if text == "count=5")),
        "apply 后应重组显示新值"
    );
}
