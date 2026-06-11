//! 光标请求与二维拖拽取消（Esc）语义回归。

use std::cell::Cell;

use compose::cursor::{Cursor, current_cursor, set_cursor, take_cursor_change};
use compose::layout::{
    begin_drag_2d_full, drag_2d_cancel_global, drag_2d_end_global, drag_2d_move_global,
    set_last_pointer_global,
};

thread_local! {
    static ENDED: Cell<usize> = const { Cell::new(0) };
    static CANCELED: Cell<usize> = const { Cell::new(0) };
}

#[test]
fn cursor_change_reported_once_then_none() {
    set_cursor(Cursor::Default);
    assert_eq!(take_cursor_change(), None, "未变化不应重复上报");
    set_cursor(Cursor::ResizeEw);
    assert_eq!(current_cursor(), Cursor::ResizeEw);
    assert_eq!(take_cursor_change(), Some(Cursor::ResizeEw));
    assert_eq!(take_cursor_change(), None, "同一形状只上报一次");
    set_cursor(Cursor::Default);
    assert_eq!(take_cursor_change(), Some(Cursor::Default));
}

#[test]
fn cancel_invokes_on_cancel_not_on_end() {
    begin_drag_2d_full(
        |_, _| {},
        |_, _| ENDED.with(|c| c.set(c.get() + 1)),
        || CANCELED.with(|c| c.set(c.get() + 1)),
    );
    assert!(drag_2d_move_global(10.0, 10.0));
    assert!(drag_2d_cancel_global(), "取消应消费活动会话");
    assert!(!drag_2d_end_global(), "取消后会话已丢弃");
    assert_eq!(CANCELED.with(|c| c.get()), 1);
    assert_eq!(ENDED.with(|c| c.get()), 0, "取消 ≠ 提交");
}

#[test]
fn end_invokes_on_end_not_cancel() {
    begin_drag_2d_full(
        |_, _| {},
        |_, _| ENDED.with(|c| c.set(c.get() + 1)),
        || CANCELED.with(|c| c.set(c.get() + 1)),
    );
    set_last_pointer_global(5.0, 5.0);
    assert!(drag_2d_end_global());
    assert_eq!(ENDED.with(|c| c.get()), 1);
    assert_eq!(CANCELED.with(|c| c.get()), 0);
}