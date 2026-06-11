//! 回归用例：拖拽会话（splitter 比例计算 / 二维拖拽释放回调）纯逻辑正确。

use std::cell::Cell;

use compose::layout::{
    begin_drag_2d_with_end, begin_drag_session, drag_2d_end_global, drag_2d_move_global,
    drag_end_global, drag_move_global, set_last_pointer_global,
};
use compose::runtime::mutableStateOf;

thread_local! {
    static END_XY: Cell<Option<(f32, f32)>> = const { Cell::new(None) };
    static MOVE_COUNT: Cell<usize> = const { Cell::new(0) };
}

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

#[test]
fn drag_2d_end_hook_receives_final_pointer() {
    // Docking 靠这个回调在「抬起那一刻」结算放置目标，必须收到最终全局坐标。
    END_XY.with(|c| c.set(None));
    MOVE_COUNT.with(|c| c.set(0));
    begin_drag_2d_with_end(
        |_x, _y| MOVE_COUNT.with(|c| c.set(c.get() + 1)),
        |x, y| END_XY.with(|c| c.set(Some((x, y)))),
    );
    assert!(drag_2d_move_global(10.0, 20.0));
    assert!(drag_2d_move_global(30.0, 40.0));
    assert_eq!(MOVE_COUNT.with(|c| c.get()), 2, "移动回调应逐次触发");
    set_last_pointer_global(77.0, 88.0);
    assert!(drag_2d_end_global(), "抬起应报告存在活动会话");
    assert_eq!(
        END_XY.with(|c| c.get()),
        Some((77.0, 88.0)),
        "释放回调应收到最终指针"
    );
    assert!(!drag_2d_move_global(0.0, 0.0), "结束后会话应清空");
    assert!(!drag_2d_end_global(), "重复抬起不应再触发回调");
}
