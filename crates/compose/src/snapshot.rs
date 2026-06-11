//! Snapshot — 对齐 Jetpack Compose `Snapshot`：事务性状态视图与观察。
//!
//! Compose 的 Snapshot 提供「时间点隔离读 + 批量写 + 提交时统一失效」：
//! - `Snapshot {}` 创建只读快照，读固定创建时刻的值，不受之后全局写入影响；
//! - `MutableSnapshot` 内写入隔离到写集合，`apply()` 提交后才写回全局并通知读者；
//! - `registerReadObserver` / `registerWriteObserver` 在组合之外提供状态观察。
//!
//! 本实现适配工程 arena 模型（零 `Rc`/`unsafe`、状态为 `StateId` 句柄）：
//! - 线程局部快照栈 `CURRENT`：嵌套快照后进先出；无快照时读写与改造前完全一致；
//! - 快照数据存 arena `SNAPSHOTS`：`read_cache`（时间点读快照）+ `writes`（待提交写）；
//! - snapshot 内的读先查写集合，再查读缓存，最后回落全局并缓存（隔离后续写）；
//! - snapshot 内的写只进写集合，`apply()` 时逐条写回全局并触发失效。
//!
//! 注意：Compose 用版本化 StateRecord 链实现无锁 MVCC；本工程单线程主线程模型下，
//! 以「读时缓存值」达到同样的隔离语义，且无需为每个 StateSlot 维护版本历史。

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::runtime::StateId;

/// 快照句柄（arena 索引）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SnapshotId(pub u32);

struct SnapshotData {
    /// 读缓存：StateId → 快照创建后首次读到的值（隔离后续全局写）。
    read_cache: HashMap<u32, Box<dyn Any>>,
    /// 写集合：StateId → 新值（apply 时提交）。
    writes: HashMap<u32, Box<dyn Any>>,
    /// 是否已 apply（apply 后句柄失效，对齐 Compose）。
    applied: bool,
    /// 只读快照（read-only 不允许写；对齐 Compose readonly snapshot）。
    read_only: bool,
}

impl SnapshotData {
    fn new(read_only: bool) -> Self {
        Self {
            read_cache: HashMap::new(),
            writes: HashMap::new(),
            applied: false,
            read_only,
        }
    }
}

thread_local! {
    /// 快照 arena（槽位永不回收，句柄稳定）。
    static SNAPSHOTS: RefCell<Vec<Option<SnapshotData>>> = const { RefCell::new(Vec::new()) };
    /// 当前快照栈（嵌套 snapshot 时后进先出）。
    static CURRENT: RefCell<Vec<SnapshotId>> = const { RefCell::new(Vec::new()) };
    /// 全局读观察者（组合外）：每次状态读触发；Option 槽支持 take-put 借用外调用。
    static READ_OBSERVERS: RefCell<Vec<Option<Box<dyn Fn(StateId)>>>> = const { RefCell::new(Vec::new()) };
    /// 全局写观察者：每次状态写（apply 提交也算）触发。
    static WRITE_OBSERVERS: RefCell<Vec<Option<Box<dyn Fn(StateId)>>>> = const { RefCell::new(Vec::new()) };
}

/// 当前是否处于快照上下文。
pub fn in_snapshot() -> bool {
    CURRENT.with(|c| !c.borrow().is_empty())
}

/// 当前快照句柄（栈顶）。
fn current_id() -> Option<SnapshotId> {
    CURRENT.with(|c| c.borrow().last().copied())
}

fn alloc(data: SnapshotData) -> SnapshotId {
    SNAPSHOTS.with(|s| {
        let mut s = s.borrow_mut();
        let id = SnapshotId(s.len() as u32);
        s.push(Some(data));
        id
    })
}

/// 创建一个只读快照：读固定首次读到的值，不受后续全局写影响；不可写。
pub fn take_snapshot() -> SnapshotId {
    alloc(SnapshotData::new(true))
}

/// 创建一个可变快照：写进隔离集合，`apply()` 后提交。
pub fn mutable_snapshot() -> SnapshotId {
    alloc(SnapshotData::new(false))
}

/// 在只读快照中执行 `f`；退出后快照仍可 `get` 读，但需手动 `release`。
pub fn snapshot<R>(f: impl FnOnce() -> R) -> (SnapshotId, R) {
    let id = take_snapshot();
    let result = enter(id, f);
    (id, result)
}

/// 在可变快照中执行 `f`；返回句柄供 `apply()` / `discard()`。
pub fn withMutableSnapshot<R>(f: impl FnOnce() -> R) -> (SnapshotId, R) {
    let id = mutable_snapshot();
    let result = enter(id, f);
    (id, result)
}

fn enter<R>(id: SnapshotId, f: impl FnOnce() -> R) -> R {
    CURRENT.with(|c| c.borrow_mut().push(id));
    let result = f();
    CURRENT.with(|c| {
        c.borrow_mut().pop();
    });
    result
}

/// 进入已存在的快照上下文执行 `f`（供测试/宿主控制边界）。
pub fn enter_snapshot<R>(id: SnapshotId, f: impl FnOnce() -> R) -> R {
    enter(id, f)
}

/// 提交快照的写集合到全局状态（触发写观察者与失效）。
///
/// 由 runtime 提供提交回调以避免 snapshot 反向依赖 runtime 私有失效函数。
pub fn apply_with<F>(id: SnapshotId, mut commit: F)
where
    F: FnMut(u32, Box<dyn Any>),
{
    let writes = SNAPSHOTS.with(|s| {
        let mut s = s.borrow_mut();
        let Some(Some(data)) = s.get_mut(id.0 as usize) else { return Vec::new() };
        if data.applied || data.read_only {
            return Vec::new();
        }
        data.applied = true;
        std::mem::take(&mut data.writes)
            .into_iter()
            .collect::<Vec<_>>()
    });
    for (key, value) in writes {
        commit(key, value);
        notify_write(StateId(key));
    }
}

/// 丢弃快照（未提交的写集合作废）。
pub fn discard(id: SnapshotId) {
    SNAPSHOTS.with(|s| {
        if let Some(Some(data)) = s.borrow_mut().get_mut(id.0 as usize) {
            data.applied = true;
            data.writes.clear();
            data.read_cache.clear();
        }
    });
}

/// 快照内读：命中写集合 → 读缓存 → 回落闭包（读取全局并缓存）。
pub fn read_or<F, R>(key: u32, f: F) -> R
where
    F: FnOnce() -> R,
    R: Clone + 'static,
{
    let Some(id) = current_id() else {
        return f();
    };
    // 先查写集合 / 读缓存；命中则 downcast clone 返回。
    let hit: Option<R> = SNAPSHOTS.with(|s| -> Option<R> {
        let s = s.borrow();
        let data = s.get(id.0 as usize).and_then(|d| d.as_ref())?;
        let value = data.writes.get(&key).or_else(|| data.read_cache.get(&key))?;
        value.downcast_ref::<R>().cloned()
    });
    if let Some(v) = hit {
        notify_read(StateId(key));
        return v;
    }
    let value = f();
    // 缓存本次读：后续全局写不影响本快照视图。
    SNAPSHOTS.with(|s| {
        if let Some(Some(data)) = s.borrow_mut().get_mut(id.0 as usize) {
            data.read_cache.insert(key, Box::new(value.clone()));
        }
    });
    notify_read(StateId(key));
    value
}

/// 快照内读克隆（非 Copy 类型）：命中则克隆。
pub fn read_clone_or<T: Clone + 'static>(key: u32, f: impl FnOnce() -> T) -> T {
    // 复用 read_or：T: Clone 满足 'static，Box<T> downcast 后 clone。
    read_or(key, f)
}

/// 快照内写：写进隔离集合；返回值表示是否已截获（false = 不在快照内或只读）。
pub fn write_capture(key: u32, value: Box<dyn Any>) -> bool {
    let Some(id) = current_id() else {
        return false;
    };
    let captured = SNAPSHOTS.with(|s| {
        let mut s = s.borrow_mut();
        let Some(Some(data)) = s.get_mut(id.0 as usize) else { return false };
        if data.applied || data.read_only {
            return false;
        }
        data.writes.insert(key, value);
        true
    });
    if captured {
        notify_write(StateId(key));
    }
    captured
}

/// 注册读观察者（组合外），返回注销句柄（下标）。
pub fn register_read_observer(f: impl Fn(StateId) + 'static) -> usize {
    READ_OBSERVERS.with(|o| {
        let mut o = o.borrow_mut();
        let id = o.len();
        o.push(Some(Box::new(f)));
        id
    })
}

/// 注册写观察者（组合外）。
pub fn register_write_observer(f: impl Fn(StateId) + 'static) -> usize {
    WRITE_OBSERVERS.with(|o| {
        let mut o = o.borrow_mut();
        let id = o.len();
        o.push(Some(Box::new(f)));
        id
    })
}

/// 注销观察者（take 出槽置 None）。
pub fn unregister_read_observer(id: usize) {
    READ_OBSERVERS.with(|o| {
        if let Some(slot) = o.borrow_mut().get_mut(id) {
            *slot = None;
        }
    });
}
pub fn unregister_write_observer(id: usize) {
    WRITE_OBSERVERS.with(|o| {
        if let Some(slot) = o.borrow_mut().get_mut(id) {
            *slot = None;
        }
    });
}

/// take-put 调用观察者：取出闭包（借用外执行），允许回调重入 runtime。
fn notify(list: &'static std::thread::LocalKey<RefCell<Vec<Option<Box<dyn Fn(StateId)>>>>>, id: StateId) {
    let n = list.with(|o| o.borrow().len());
    for i in 0..n {
        let f = list.with(|o| o.borrow_mut()[i].take());
        if let Some(f) = f {
            f(id);
            list.with(|o| o.borrow_mut()[i] = Some(f));
        }
    }
}

fn notify_read(id: StateId) {
    notify(&READ_OBSERVERS, id);
}

fn notify_write(id: StateId) {
    notify(&WRITE_OBSERVERS, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_read_isolated_from_later_writes() {
        let id = test_state_id();
        let (snap, first) = snapshot(|| crate::snapshot::read_or(id.0, || 1i32));
        assert_eq!(first, 1);
        // 全局写不影响快照内读。
        let isolated = enter_snapshot(snap, || crate::snapshot::read_or(id.0, || 999i32));
        assert_eq!(isolated, 1, "快照读应隔离后续全局写");
        discard(snap);
    }

    #[test]
    fn mutable_snapshot_write_only_visible_after_apply() {
        let id = test_state_id();
        let (snap, _) = withMutableSnapshot(|| {
            assert!(write_capture(id.0, Box::new(5i32)));
            assert_eq!(crate::snapshot::read_or(id.0, || 1i32), 5, "快照内读自己的写");
        });
        // 全局值仍是 1（未提交）。
        assert_eq!(crate::snapshot::read_or(id.0, || 1i32), 1);
        let mut applied = 0;
        apply_with(snap, |_, _| applied += 1);
        assert_eq!(applied, 1, "apply 应提交一次写");
    }

    #[test]
    fn nested_snapshots_restore_context() {
        assert!(!in_snapshot());
        let (outer, _) = snapshot(|| {
            assert!(in_snapshot());
            let (inner, _) = snapshot(|| assert!(in_snapshot()));
            discard(inner);
            assert!(in_snapshot(), "内层退出后仍在外层快照");
        });
        discard(outer);
        assert!(!in_snapshot());
    }

    #[test]
    fn observers_fire_on_write() {
        use std::cell::Cell;
        thread_local! {
            static HITS: Cell<usize> = const { Cell::new(0) };
        }
        let _h = register_write_observer(|_| HITS.with(|h| h.set(h.get() + 1)));
        let id = test_state_id();
        let (snap, _) = withMutableSnapshot(|| {
            write_capture(id.0, Box::new(7i32));
        });
        assert_eq!(HITS.with(|h| h.get()), 1, "快照写应通知观察者");
        apply_with(snap, |_, _| {});
    }

    fn test_state_id() -> StateId {
        // 测试只需一个 StateId 句柄；值由 read_or 的回落闭包提供。
        StateId(0)
    }
}
