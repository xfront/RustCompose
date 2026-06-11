//! 手势 — 对齐 Jetpack Compose `pointerInput` / `PointerInputScope` / 手势检测器。
//!
//! Compose 的 `Modifier.pointerInput(key) { detectTapGestures { ... } }` 把指针事件流
//! 暴露给挂在该节点上的协程作用域，检测器用 `awaitPointerEvent` 消费事件。
//! 本工程零 `Rc`/`unsafe`，故采用同步状态机：节点上挂一个 `PointerInputHandler`
//! （`FnMut(PointerEvent)`），由分发入口把 Down/Move/Up/Cancel 喂给沿祖先链注册的处理器。
//!
//! 提供的检测器：
//! - [`detect_tap_gestures`]：tap / doubleTap / longPress（带 tap slop 与超时）
//! - [`detect_drag_gestures`]：drag start / move / end（相对按下点偏移，带 touch slop）
//!
//! 时间由 `now_nanos()` 注入（测试可确定性驱动），与 Compose 的 `MonotonicFrameClock` 对应。

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

/// 指针事件类型（对齐 Compose `PointerEventType`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerEventType {
    Press,
    Move,
    Release,
    Cancel,
}

/// 统一指针事件（对齐 Compose `PointerEvent`）。
#[derive(Clone, Copy, Debug)]
pub struct PointerEvent {
    pub kind: PointerEventType,
    /// 屏幕全局坐标。
    pub x: f32,
    pub y: f32,
    /// 相对按下点的偏移（Press 时为 0）。
    pub dx: f32,
    pub dy: f32,
    /// 单调时间戳（纳秒）。
    pub time_ns: u64,
}

impl PointerEvent {
    pub fn press(x: f32, y: f32) -> Self {
        Self { kind: PointerEventType::Press, x, y, dx: 0.0, dy: 0.0, time_ns: now_nanos() }
    }
    pub fn move_to(x: f32, y: f32, start_x: f32, start_y: f32) -> Self {
        Self { kind: PointerEventType::Move, x, y, dx: x - start_x, dy: y - start_y, time_ns: now_nanos() }
    }
    pub fn release(x: f32, y: f32, start_x: f32, start_y: f32) -> Self {
        Self { kind: PointerEventType::Release, x, y, dx: x - start_x, dy: y - start_y, time_ns: now_nanos() }
    }
}

pub fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// 节点上的指针输入处理器（由 `Modifier.pointerInput` 注册）。
pub type PointerInputHandler = Box<dyn FnMut(PointerEvent)>;

thread_local! {
    /// 各节点的 pointerInput 处理器（按 NodeId 索引；与 Node 分离避免污染布局结构）。
    static HANDLERS: RefCell<Vec<(u32, PointerInputHandler)>> = const { RefCell::new(Vec::new()) };
    /// 每个检测器实例的按压起点（按节点 id）。
    static STARTS: RefCell<Vec<(u32, f32, f32)>> = const { RefCell::new(Vec::new()) };
}

/// 在节点上注册 pointerInput 处理器（组合期调用；同节点重复注册覆盖）。
pub fn register(node: u32, handler: PointerInputHandler) {
    HANDLERS.with(|h| {
        let mut h = h.borrow_mut();
        if let Some(slot) = h.iter_mut().find(|(id, _)| *id == node) {
            slot.1 = handler;
        } else {
            h.push((node, handler));
        }
    });
}

/// 移除节点上的处理器（作用域销毁时可调用）。
pub fn unregister(node: u32) {
    HANDLERS.with(|h| h.borrow_mut().retain(|(id, _)| *id != node));
}

/// 是否有节点注册了 pointerInput（供分发入口快速判断）。
pub fn has_handlers() -> bool {
    HANDLERS.with(|h| !h.borrow().is_empty())
}

/// 向指定节点派发一个事件（take-put 防重入）。
pub fn dispatch_to(node: u32, event: PointerEvent) {
    let handler = HANDLERS.with(|h| {
        let mut h = h.borrow_mut();
        h.iter_mut().find(|(id, _)| *id == node).map(|(_, f)| std::mem::replace(f, Box::new(|_| {})))
    });
    if let Some(mut f) = handler {
        f(event);
        HANDLERS.with(|h| {
            let mut h = h.borrow_mut();
            if let Some((_, slot)) = h.iter_mut().find(|(id, _)| *id == node) {
                *slot = f;
            }
        });
    }
}

/// 记录/读取按压起点（检测器内部用）。
pub(crate) fn set_start(node: u32, x: f32, y: f32) {
    STARTS.with(|s| {
        let mut s = s.borrow_mut();
        if let Some(slot) = s.iter_mut().find(|(id, _, _)| *id == node) {
            slot.1 = x;
            slot.2 = y;
        } else {
            s.push((node, x, y));
        }
    });
}
pub(crate) fn take_start(node: u32) -> Option<(f32, f32)> {
    STARTS.with(|s| {
        let mut s = s.borrow_mut();
        s.iter().position(|(id, _, _)| *id == node).map(|i| {
            let (_, x, y) = s.remove(i);
            (x, y)
        })
    })
}

///  tap 手势回调集合（对齐 Compose `detectTapGestures` 的参数）。
pub struct TapGestureConfig<OnTap, OnDouble, OnLong> {
    pub on_tap: OnTap,
    pub on_double_tap: OnDouble,
    pub on_long_press: OnLong,
    /// 判定为 tap 的最大位移（Compose `touchSlop`）。
    pub slop: f32,
    /// 长按判定时间（毫秒）。
    pub long_press_ms: u64,
    /// 双击判定间隔（毫秒）。
    pub double_tap_ms: u64,
}

/// 构造默认 tap 配置（slop=8px, longPress=500ms, doubleTap=300ms）。
pub fn tap_config<OnTap, OnDouble, OnLong>(
    on_tap: OnTap,
    on_double_tap: OnDouble,
    on_long_press: OnLong,
) -> TapGestureConfig<OnTap, OnDouble, OnLong>
where
    OnTap: Fn(f32, f32) + 'static,
    OnDouble: Fn(f32, f32) + 'static,
    OnLong: Fn(f32, f32) + 'static,
{
    TapGestureConfig {
        on_tap,
        on_double_tap,
        on_long_press,
        slop: 8.0,
        long_press_ms: 500,
        double_tap_ms: 300,
    }
}

/// 生成一个 tap 手势处理器（返回闭包，供 `Modifier.pointerInput` 注册）。
pub fn detect_tap_gestures<OnTap, OnDouble, OnLong>(
    config: TapGestureConfig<OnTap, OnDouble, OnLong>,
) -> impl FnMut(PointerEvent)
where
    OnTap: Fn(f32, f32) + 'static,
    OnDouble: Fn(f32, f32) + 'static,
    OnLong: Fn(f32, f32) + 'static,
{
    let TapGestureConfig { on_tap, on_double_tap, on_long_press, slop, long_press_ms, double_tap_ms } = config;
    let mut down: Option<(f32, f32, u64)> = None;
    let mut last_tap_ns = 0u64;
    move |e: PointerEvent| match e.kind {
        PointerEventType::Press => {
            down = Some((e.x, e.y, e.time_ns));
        }
        PointerEventType::Release => {
            if let Some((sx, sy, t0)) = down.take() {
                let moved = (e.x - sx).hypot(e.y - sy);
                if moved > slop {
                    return; // 移动超阈值不算 tap
                }
                let now = e.time_ns.max(t0);
                if now.saturating_sub(t0) >= long_press_ms * 1_000_000 {
                    on_long_press(e.x, e.y);
                } else if last_tap_ns != 0 && now.saturating_sub(last_tap_ns) <= double_tap_ms * 1_000_000 {
                    on_double_tap(e.x, e.y);
                    last_tap_ns = 0;
                } else {
                    on_tap(e.x, e.y);
                    last_tap_ns = now;
                }
            }
        }
        PointerEventType::Cancel => down = None,
        PointerEventType::Move => {}
    }
}

/// drag 手势回调配置（对齐 Compose `detectDragGestures`）。
pub struct DragGestureConfig<OnStart, OnDrag, OnEnd> {
    pub on_drag_start: OnStart,
    pub on_drag: OnDrag,
    pub on_drag_end: OnEnd,
    pub slop: f32,
}

pub fn drag_config<OnStart, OnDrag, OnEnd>(
    on_start: OnStart,
    on_drag: OnDrag,
    on_end: OnEnd,
) -> DragGestureConfig<OnStart, OnDrag, OnEnd>
where
    OnStart: Fn() + 'static,
    OnDrag: Fn(f32, f32) + 'static,
    OnEnd: Fn() + 'static,
{
    DragGestureConfig { on_drag_start: on_start, on_drag, on_drag_end: on_end, slop: 8.0 }
}

/// 生成 drag 手势处理器。
pub fn detect_drag_gestures<OnStart, OnDrag, OnEnd>(
    config: DragGestureConfig<OnStart, OnDrag, OnEnd>,
) -> impl FnMut(PointerEvent)
where
    OnStart: Fn() + 'static,
    OnDrag: Fn(f32, f32) + 'static,
    OnEnd: Fn() + 'static,
{
    let DragGestureConfig { on_drag_start, on_drag, on_drag_end, slop } = config;
    let mut down: Option<(f32, f32)> = None;
    let mut dragging = false;
    move |e: PointerEvent| match e.kind {
        PointerEventType::Press => {
            down = Some((e.x, e.y));
            dragging = false;
        }
        PointerEventType::Move => {
            if let Some((sx, sy)) = down {
                let dx = e.x - sx;
                let dy = e.y - sy;
                if !dragging && dx.hypot(dy) >= slop {
                    dragging = true;
                    on_drag_start();
                }
                if dragging {
                    on_drag(dx, dy);
                }
            }
        }
        PointerEventType::Release | PointerEventType::Cancel => {
            if dragging {
                on_drag_end();
            }
            down = None;
            dragging = false;
        }
    }
}

/// `Modifier.pointerInput(key) { ... }` 对齐入口（组合函数形式）。
///
/// 创建一个带 pointerInput 的容器节点：handler 由 `factory` 生成（每次组合重建），
/// `content` 在该节点内组合。指针事件经 `dispatch_pointer_*` 派发到此节点的处理器。
pub fn PointerInput(
    modifier: crate::layout::Modifier,
    content: impl Fn() + 'static,
    handler: impl FnMut(PointerEvent) + 'static,
) -> crate::layout::NodeId {
    let id = crate::layout::add_node(
        crate::layout::Policy::Box(crate::layout::Alignment::TopStart),
        modifier,
    );
    register(id.index() as u32, Box::new(handler));
    crate::layout::with_content_scope(id, content);
    id
}

/// 派发 Move 事件（供 Kernel / 测试在指针移动时调用）。
pub fn dispatch_pointer_move(target: crate::layout::NodeId, gx: f32, gy: f32) -> bool {
    let e = PointerEvent::move_to(gx, gy, gx, gy); // dx/dy 由检测器内部起点计算
    dispatch_pointer_event(target, e)
}

/// 向节点树派发指针事件（沿 target 祖先链找最近的 pointerInput 处理器）。
/// 返回是否命中处理器。
pub fn dispatch_pointer_event(
    target: crate::layout::NodeId,
    event: PointerEvent,
) -> bool {
    if !has_handlers() {
        return false;
    }
    // 从 target 沿 parent 向上找第一个注册了 handler 的节点（closest wins）。
    let mut cur = Some(target);
    while let Some(id) = cur {
        let has = HANDLERS.with(|h| h.borrow().iter().any(|(n, _)| *n == id.index() as u32));
        if has {
            dispatch_to(id.index() as u32, event);
            return true;
        }
        cur = crate::runtime::with_rt_pub(|rt| rt.nodes[id.index()].parent);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn event(kind: PointerEventType, x: f32, y: f32, t: u64) -> PointerEvent {
        PointerEvent { kind, x, y, dx: 0.0, dy: 0.0, time_ns: t }
    }

    #[test]
    fn tap_detector_fires_on_release() {
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        let mut d = detect_tap_gestures(tap_config(
            move |_, _| h.set(h.get() + 1),
            |_, _| {},
            |_, _| {},
        ));
        d(event(PointerEventType::Press, 10.0, 10.0, 0));
        d(event(PointerEventType::Release, 10.0, 10.0, 100_000_000));
        assert_eq!(hits.get(), 1);
    }

    #[test]
    fn tap_detector_ignores_dragged_release() {
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        let mut d = detect_tap_gestures(tap_config(
            move |_, _| h.set(h.get() + 1),
            |_, _| {},
            |_, _| {},
        ));
        d(event(PointerEventType::Press, 10.0, 10.0, 0));
        d(event(PointerEventType::Release, 30.0, 10.0, 100_000_000)); // 移动 20px > slop
        assert_eq!(hits.get(), 0);
    }

    #[test]
    fn drag_detector_respects_slop() {
        let starts = Rc::new(Cell::new(0));
        let moves = Rc::new(Cell::new(0));
        let (s, m) = (starts.clone(), moves.clone());
        let mut d = detect_drag_gestures(drag_config(
            move || s.set(s.get() + 1),
            move |_, _| m.set(m.get() + 1),
            || {},
        ));
        d(event(PointerEventType::Press, 10.0, 10.0, 0));
        d(event(PointerEventType::Move, 14.0, 10.0, 10_000_000)); // 4px < slop
        assert_eq!(starts.get(), 0);
        d(event(PointerEventType::Move, 20.0, 10.0, 20_000_000)); // 10px >= slop
        assert_eq!(starts.get(), 1);
        assert_eq!(moves.get(), 1);
    }

    #[test]
    fn long_press_and_double_tap() {
        let long = Rc::new(Cell::new(0));
        let dbl = Rc::new(Cell::new(0));
        let (l, db) = (long.clone(), dbl.clone());
        let mut d = detect_tap_gestures(tap_config(
            |_, _| {},
            move |_, _| db.set(db.get() + 1),
            move |_, _| l.set(l.get() + 1),
        ));
        // 长按
        d(event(PointerEventType::Press, 5.0, 5.0, 0));
        d(event(PointerEventType::Release, 5.0, 5.0, 600_000_000));
        assert_eq!(long.get(), 1);
        // 双击（两次短按，间隔 100ms）
        d(event(PointerEventType::Press, 5.0, 5.0, 1_000_000_000));
        d(event(PointerEventType::Release, 5.0, 5.0, 1_050_000_000));
        d(event(PointerEventType::Press, 5.0, 5.0, 1_150_000_000));
        d(event(PointerEventType::Release, 5.0, 5.0, 1_200_000_000));
        assert_eq!(dbl.get(), 1);
    }
}
