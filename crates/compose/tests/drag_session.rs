//! 回归用例：拖拽会话（splitter 比例计算）纯逻辑正确。

use compose::layout::{begin_drag_session, drag_end_global, drag_move_global};
use compose::runtime::mutableStateOf;

#[test]
fn horizontal_drag_updates_ratio() {
    let ratio = mutableStateOf(0.5f32);
    begin_drag_session(0.0, 0.0, 100.0, 1.0, ratio, true, 0.05, 0.95);
    // 指针移到容器中线 → 比例 ~0.5
    assert!(drag_move_global(50.0, 0.0));
    assert!((ratio.get() - 0.5).abs() < 1e-3, "中线应为 0.5: {}", ratio.get());
    // 拖到 80% 宽处
    drag_move_global(80.0, 0.0);
    assert!((ratio.get() - 0.8).abs() < 1e-3, "80 宽应为 0.8: {}", ratio.get());
}

#[test]
fn drag_clamps_bounds() {
    let ratio = mutableStateOf(0.5f32);
    begin_drag_session(0.0, 0.0, 100.0, 1.0, ratio, true, 0.05, 0.95);
    drag_move_global(-50.0, 0.0);
    assert!(ratio.get() >= 0.05 - 1e-6, "下限钳制到 0.05: {}", ratio.get());
    drag_move_global(999.0, 0.0);
    assert!(ratio.get() <= 0.95 + 1e-6, "上限钳制到 0.95: {}", ratio.get());
}

#[test]
fn vertical_drag_uses_height() {
    let ratio = mutableStateOf(0.5f32);
    begin_drag_session(0.0, 0.0, 1.0, 200.0, ratio, false, 0.05, 0.95);
    drag_move_global(0.0, 60.0);
    assert!((ratio.get() - 0.3).abs() < 1e-3, "y=60/200 应为 0.3: {}", ratio.get());
}

#[test]
fn end_drag_clears_session() {
    let ratio = mutableStateOf(0.5f32);
    begin_drag_session(0.0, 0.0, 100.0, 1.0, ratio, true, 0.05, 0.95);
    assert!(drag_end_global());
    // 结束后 move 不再触发拖拽
    assert!(!drag_move_global(10.0, 0.0));
}
