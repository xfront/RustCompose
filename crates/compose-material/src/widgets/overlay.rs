//! 浮层宿主 — 对话框 / 抽屉 / 菜单 / Tooltip / Snackbar 的统一最上层通道。
//!
//! 全局浮层栈（`Overlay`）：树内任意深度的控件都能把内容叠到最上层，
//! 且不会像内联 `Box` 那样把父容器撑大、被父级裁剪。
//!
//! 机制：组合期 `Overlay(..)` 注册内容 → `OverlayHost` 在内容之后统一组合；
//! 注册/注销发生在宿主组合之外时会自动调度宿主重组（因此开关浮层无需手动刷新）。
//!
//! **零 `Rc`**：内容闭包由 `emit_scope` 派生的作用域持有（arena 化），宿主只保存一趟组合期内的临时表。

use std::cell::{Cell, RefCell};

use compose::layout::{Alignment, Modifier, Policy, add_node, with_content_scope};
use compose::runtime::{ScopeId, current_scope_id, emit_scope, schedule_scope};

type OverlayFn = Box<dyn Fn()>;

thread_local! {
    /// 本趟组合期内注册的浮层（按注册顺序）。
    static OVERLAYS: RefCell<Vec<(u64, OverlayFn)>> = const { RefCell::new(Vec::new()) };
    /// 宿主内容作用域（注册变化时需要重新组合它）。
    static HOST_SCOPE: Cell<Option<ScopeId>> = const { Cell::new(None) };
    /// 是否正处于宿主组合中（此时注册不需要再调度，避免自激循环）。
    static IN_HOST: Cell<bool> = const { Cell::new(false) };
    /// 上一轮宿主实际发射到树上的浮层 key（关闭时据此判定是否需要重组宿主）。
    static EMITTED: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// 宿主正在组合 → 注册不触发额外调度。
pub fn in_host_composition() -> bool {
    IN_HOST.with(|h| h.get())
}

thread_local! {
    static KEY_SEED: Cell<u64> = const { Cell::new(0x5A5A_0000) };
    /// 按 key 延迟调用的内容表（把 `Fn` 闭包搬进 `Fn` 浮层闭包的桥梁，零 `Rc`）。
    static CONTENT_CB: RefCell<std::collections::HashMap<u64, Box<dyn Fn()>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// 登记按 key 调用的内容（浮层内容需要跨闭包复用时使用）。
pub fn PutOverlayContent(key: u64, f: impl Fn() + 'static) {
    CONTENT_CB.with(|m| {
        m.borrow_mut().insert(key, Box::new(f));
    });
}

/// 调用 [`PutOverlayContent`] 登记的内容。
pub fn call_overlay_content(key: u64) {
    CONTENT_CB.with(|m| {
        if let Some(f) = m.borrow().get(&key) {
            f();
        }
    });
}

/// 转发型浮层：内容为「调用 `key` 登记的内容」，因此自身只需捕获 Copy 的 key。
pub fn OverlaySlot(key: u64, open: bool) {
    Overlay(key, open, move || call_overlay_content(key));
}

thread_local! {
    /// 零次性动作回调表（在 `Fn` 闭包里复用 `FnOnce` 语义的回调）。
    static ACTION_CB: RefCell<std::collections::HashMap<u64, Box<dyn Fn()>>> =
        RefCell::new(std::collections::HashMap::new());
    /// 带索引的动作回调表（列表项关闭 / 选中）。
    static IDX_CB: RefCell<std::collections::HashMap<u64, Box<dyn Fn(usize)>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// 登记动作回调（`Fn` 闭包内需要调用一次性回调时使用）。
pub fn PutAction(key: u64, f: impl Fn() + 'static) {
    ACTION_CB.with(|m| {
        m.borrow_mut().insert(key, Box::new(f));
    });
}

/// 触发 [`PutAction`] 登记的动作。
pub fn RunAction(key: u64) {
    ACTION_CB.with(|m| {
        if let Some(f) = m.borrow().get(&key) {
            f();
        }
    });
}

/// 登记带索引的动作回调。
pub fn PutIndexedAction(key: u64, f: impl Fn(usize) + 'static) {
    IDX_CB.with(|m| {
        m.borrow_mut().insert(key, Box::new(f));
    });
}

/// 触发 [`PutIndexedAction`] 登记的动作。
pub fn RunIndexedAction(key: u64, index: usize) {
    IDX_CB.with(|m| {
        if let Some(f) = m.borrow().get(&key) {
            f(index);
        }
    });
}

/// 生成稳定的浮层 key（配合 `remember` 使用 → 跨重组不变，注销才找得到）。
pub fn NextOverlayKey() -> u64 {
    KEY_SEED.with(|k| {
        let v = k.get() + 1;
        k.set(v);
        v
    })
}

/// 记住一个稳定浮层 key（控件内首选：`let key = RememberOverlayKey();`）。
pub fn RememberOverlayKey() -> u64 {
    compose::runtime::remember(NextOverlayKey)
}

fn schedule_host() {
    if IN_HOST.with(|h| h.get()) {
        return;
    }
    if let Some(sid) = HOST_SCOPE.with(|h| h.get()) {
        schedule_scope(sid);
    }
}

/// 注册浮层内容（同一 `key` 覆盖旧内容）。
pub fn RegisterOverlay(key: u64, content: impl Fn() + 'static) {
    let fresh = OVERLAYS.with(|o| {
        let mut o = o.borrow_mut();
        if let Some((_, f)) = o.iter_mut().find(|(k, _)| *k == key) {
            *f = Box::new(content);
            false
        } else {
            o.push((key, Box::new(content)));
            true
        }
    });
    if fresh {
        schedule_host();
    }
}

/// 注销浮层。返回是否真的移除了一个条目（含上一轮已发射到树上的）。
pub fn RemoveOverlay(key: u64) -> bool {
    let removed = OVERLAYS.with(|o| {
        let mut o = o.borrow_mut();
        let before = o.len();
        o.retain(|(k, _)| *k != key);
        o.len() != before
    }) | EMITTED.with(|e| {
        let mut e = e.borrow_mut();
        let before = e.len();
        e.retain(|k| *k != key);
        e.len() != before
    });
    if removed {
        schedule_host();
    }
    removed
}

/// 条件浮层：开关由调用方每趟组合给出，翻转时自动调度宿主重组。
///
/// ```ignore
/// Overlay(c, KEY_DIALOG, open.get(), move || { dialog_body(); });
/// ```
pub fn Overlay(key: u64, open: bool, content: impl Fn() + 'static) {
    if open {
        RegisterOverlay(key, content);
    } else {
        RemoveOverlay(key);
    }
}

/// 清空全部浮层（切换页面 / 退出时使用）。
pub fn ClearOverlays() {
    OVERLAYS.with(|o| o.borrow_mut().clear());
    EMITTED.with(|e| e.borrow_mut().clear());
}

/// 浮层宿主：先组合内容，再把本趟注册的浮层依次叠在最上层。
///
/// 应用根（或 `Scaffold`）包一层即可让全树任意位置弹出对话框/菜单。
pub fn OverlayHost(modifier: Modifier, content: impl Fn() + 'static) {
    let id = add_node(Policy::Box(Alignment::TopStart), modifier);
    with_content_scope(id, move || {
        OVERLAYS.with(|o| o.borrow_mut().clear());
        HOST_SCOPE.with(|h| h.set(current_scope_id()));
        IN_HOST.with(|h| h.set(true));
        content();
        // 嵌套浮层（对话框里再开下拉）也一并组合，限深防病态递归。
        for _ in 0..8 {
            let batch: Vec<(u64, OverlayFn)> =
                OVERLAYS.with(|o| std::mem::take(&mut *o.borrow_mut()));
            if batch.is_empty() {
                break;
            }
            for (k, f) in batch {
                emit_scope(f);
                // 记录实际发射的 key：关闭时 RemoveOverlay 据此判定需重组宿主
                EMITTED.with(|e| {
                    let mut e = e.borrow_mut();
                    if !e.contains(&k) {
                        e.push(k);
                    }
                });
            }
        }
        IN_HOST.with(|h| h.set(false));
    });
}
