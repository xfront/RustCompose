//! Compose runtime — arena 化：状态、组合器、重组作用域、槽位、CompositionLocal、重组调度。
//!
//! 严格对齐 `androidx.compose.runtime` 的核心模型，同时遵守工程约束：
//! - **零 `Rc` / `Arc` / `unsafe`**：节点 / 状态 / 作用域 / 任务全部存中央 arena，
//!   以 `u32` 句柄引用；回调 `Box<dyn FnMut>` 以 take-put 模式防重入借用
//! - `MutableState` — 可观察状态：读取注册读者，写入失效读者
//! - `RecomposeScope` — 状态变更只重跑**读到该状态**的作用域（定向重组）
//! - 子作用域按「父作用域内位置」复用（对齐 SlotTable 组身份语义），
//!   `remember` 槽位跨重组存活
//! - `CompositionLocal` — 沿作用域祖先链查找的隐式环境
//! - `Recomposer` / `advance_frame` — 重组与帧调度

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::foundation::Constraints;
use crate::layout::{NodeId, NodeRef};

// ══════════════════════════════ 句柄 ══════════════════════════════

/// 重组作用域句柄。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ScopeId(pub u32);

/// 可观察状态句柄（arena 索引）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StateId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ReaderKey {
    Scope(ScopeId),
    Observer(u64),
}

#[derive(Clone, Copy)]
enum ReadCtx {
    Scope(ScopeId),
    Observer(u64),
}

/// 协程任务句柄。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TaskId(pub u32);

// ══════════════════════════════ Arena ══════════════════════════════

pub(crate) struct StateSlot {
    value: Box<dyn Any>,
    // 类型擦除的同值比较：`set`/`update` 在「值未变」时跳过失效，
    // 避免同值反复触发重组（对齐 Compose 快照系统的「结构相等才通知」策略）。
    eq: Box<dyn Fn(&dyn Any, &dyn Any) -> bool>,
}

struct ObserverSlot {
    f: Box<dyn FnMut()>,
}

/// 协程任务（取消 = 置空 future，下次 drain 移除）。
pub struct TaskSlot {
    fut: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>>,
}

pub(crate) struct ScopeData {
    /// 内容发射到的容器节点。
    pub(crate) node: Option<NodeId>,
    /// remember 槽位（位置即身份，跨重组存活）。
    pub(crate) slots: Vec<Box<dyn Any>>,
    /// 子作用域槽（父作用域内位置复用 — SlotTable 组身份）。
    pub(crate) child_scopes: Vec<ScopeId>,
    /// remember 槽位游标：每遇到一个 `remember`/`keyed_remember` 前进 1，
    /// 与 `slots` 严格 1:1（游标 == 该记忆在本作用域内出现的序号 == `slots` 下标）。
    run_cursor: usize,
    /// 子作用域游标：每 `emit_scope*` 一个子作用域前进 1，与 `child_scopes` 严格 1:1。
    /// **必须与 `run_cursor` 分离**：`slots` 与 `child_scopes` 是两个各自 `push` 增长的
    /// 独立向量，若共用一个游标，则 remember 与子作用域交替出现时两者会与游标失配，
    /// 下一轮重组 `slots[cursor]` 会读到相邻另一个同类型句柄（如两个 `MutableState<usize>`）
    /// 造成「位置即身份」串位（story d_tabs 的 tab 串到 tab2 → 越界 panic）。分离后各自对齐。
    child_cursor: usize,
    content: Option<Box<dyn Fn() + 'static>>,
    /// CompositionLocal 覆盖（provides 写入本层）。
    pub(crate) locals: HashMap<TypeId, Box<dyn Any>>,
    pub(crate) disposed: bool,
    /// DisposableEffect 清理器（作用域销毁时执行）。
    pub(crate) disposables: Vec<Box<dyn FnMut()>>,
}

pub struct RuntimeData {
    pub nodes: Vec<NodeRef>,
    /// 已释放节点槽位（LIFO 复用；重组整树重建时不至于让 arena 无限膨胀）。
    pub(crate) node_free: Vec<u32>,
    /// 组合根节点（GC 可达性扫描的起点；多个 Composition 各有一份）。
    pub(crate) root_nodes: Vec<NodeId>,
    /// 有子树被摘除 / 作用域销毁 → 下次空闲时做一次节点 GC。
    pub(crate) node_gc_pending: bool,
    pub(crate) scopes: Vec<Option<ScopeData>>,
    pub(crate) states: Vec<StateSlot>,
    pub(crate) readers: HashMap<u32, HashSet<ReaderKey>>,
    pub(crate) scope_reads: HashMap<u32, HashSet<u32>>,
    pub(crate) observers: Vec<Option<ObserverSlot>>,
    pub(crate) observer_reads: HashMap<u64, HashSet<u32>>,
    pub(crate) tasks: Vec<Option<TaskSlot>>,
    pub(crate) scope_stack: Vec<ScopeId>,
    pub(crate) parent_stack: Vec<NodeId>,
    pub(crate) read_stack: Vec<ReadCtx>,
    pub(crate) pending: Vec<ScopeId>,
    pub(crate) pending_set: HashSet<ScopeId>,
    /// 待处理的失效读者（借用外统一消费）。
    pub(crate) invalidations: Vec<ReaderKey>,
    /// 待执行的作用域清理器（借用外统一执行）。
    pub(crate) disposal_queue: Vec<Box<dyn FnMut()>>,
    /// 本轮组合注册的 DisposableEffect 清理器（按组合帧栈，嵌套作用域隔离）。
    pub(crate) disposable_stack: Vec<Vec<Box<dyn FnMut()>>>,
    /// 帧驱动（动画每帧调度其作用域重组），归属组合作用域、随作用域销毁注销。
    pub(crate) frame_drivers: HashMap<u64, FrameDriver>,
    /// 帧驱动自增 id 序号（用于 remove_frame_driver）。
    frame_driver_seq: u64,
    /// 重组监听（状态变化 → 请求重绘）。
    pub(crate) recompose_listeners: Vec<Box<dyn FnMut()>>,
    pub(crate) frame_delta: f32,
    pub(crate) frame: u64,
    /// 单调帧时钟（纳秒）— 对齐 Compose `MonotonicFrameClock`：动画按墙钟计时，
    /// 与真实刷新率/掉帧无关（旧实现按「帧数 ÷ 假定 60fps」，120Hz 上动效快一倍）。
    pub(crate) frame_time_nanos: f64,
    /// 当前键盘焦点作用域（按 scope 身份持久化，跨重组不丢）。
    pub(crate) focused_scope: Option<ScopeId>,
    /// 当前悬停节点（最近一次 mouse move 命中的、带 hover_handler 的祖先）。
    pub(crate) hovered_node: Option<usize>,
    /// 在飞的「按下」（抬起时结算点击用）。
    pub(crate) press: Option<crate::layout::Press>,
    /// 在飞的波纹（按下期间生长、抬手后淡出）。
    pub(crate) ripples: Vec<crate::layout::Ripple>,
    /// 上一帧 perform_layout 使用的约束（用于检测窗口尺寸变化 → 整体失效重布局）。
    pub(crate) last_constraints: Option<Constraints>,
}

thread_local! {
    static RUNTIME: RefCell<RuntimeData> = RefCell::new(RuntimeData::new());
    /// `run_frame_drivers` 的 (id, 归属作用域) 快照缓冲：逐帧复用，避免每帧一次 `Vec` 分配。
    /// 归属作用域在注册时确定、之后不变，故快照即权威 —— 省掉逐驱动一次哈希查找。
    static DRIVER_IDS: RefCell<Vec<(u64, Option<ScopeId>)>> = const { RefCell::new(Vec::new()) };
    /// `run_scope` 的祖先链缓冲（定向重组每帧都会用到，逐次分配浪费）。
    static SCOPE_CHAIN: RefCell<Vec<ScopeId>> = const { RefCell::new(Vec::new()) };
}

impl RuntimeData {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            node_free: Vec::new(),
            root_nodes: Vec::new(),
            node_gc_pending: false,
            scopes: Vec::new(),
            states: Vec::new(),
            readers: HashMap::new(),
            scope_reads: HashMap::new(),
            observers: Vec::new(),
            observer_reads: HashMap::new(),
            tasks: Vec::new(),
            scope_stack: Vec::new(),
            parent_stack: Vec::new(),
            read_stack: Vec::new(),
            pending: Vec::new(),
            pending_set: HashSet::new(),
            invalidations: Vec::new(),
            disposal_queue: Vec::new(),
            disposable_stack: Vec::new(),
            frame_drivers: HashMap::new(),
            frame_driver_seq: 0,
            recompose_listeners: Vec::new(),
            frame_delta: 0.0,
            frame: 0,
            frame_time_nanos: 0.0,
            focused_scope: None,
            hovered_node: None,
            press: None,
            ripples: Vec::new(),
            last_constraints: None,
        }
    }
}

fn with_rt<R>(f: impl FnOnce(&mut RuntimeData) -> R) -> R {
    RUNTIME.with(|rt| f(&mut rt.borrow_mut()))
}

/// crate 内通用 runtime 访问（material / layout 使用）。
pub fn with_rt_pub<R>(f: impl FnOnce(&mut RuntimeData) -> R) -> R {
    with_rt(f)
}

/// 公开 runtime 访问（测试 / 平台后端事件注入用）。
pub fn with_runtime<R>(f: impl FnOnce(&mut RuntimeData) -> R) -> R {
    with_rt(f)
}

/// 主线程任务队列：后台线程（如文件对话框的阻塞 `rfd` 调用）把结果回调投递到这里，
/// 由 `Kernel::frame` 在「运行时 `RefCell` 借用尚未建立」时排空，从而安全调用
/// `MutableState::set` 等需 `with_rt` 的操作。等价于 rui `spawn_blocking` 后把结果
/// 送回 GUI 线程——因为 `MutableState` 写入只能在主线程运行时里进行。
static MAIN_TASKS: Mutex<Vec<Box<dyn FnOnce() + Send>>> = Mutex::new(Vec::new());

/// 把一个任务投递到主线程（GUI 运行时线程）执行。供后台线程回传结果。
pub fn post_to_main(f: impl FnOnce() + Send + 'static) {
    MAIN_TASKS.lock().unwrap().push(Box::new(f));
}

/// 排空主线程任务队列。`Kernel::frame` 在重组前调用——此时运行时 `RefCell` 借用尚未建立，
/// 任务内可安全再入 `with_rt`（如 `MutableState::set`）。
pub fn drain_main_tasks() {
    // 取出整个队列（留下空 Vec）并立即释放锁，避免持锁期间运行回调（回调内可能再次 post_to_main）。
    let mut tasks = std::mem::take(&mut *MAIN_TASKS.lock().unwrap());
    for t in tasks.drain(..) {
        t();
    }
}

/// 当前组合作用域（浮层注册 / 动画帧驱动注册用）。
pub fn current_scope_id() -> Option<ScopeId> {
    with_rt(|rt| rt.scope_stack.last().copied())
}

/// 键盘导航方向键（与 winit 解耦，核心库不依赖窗口后端）。
#[derive(Clone, Copy, Debug)]
pub enum NavKey {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Delete,
}

/// 设置键盘焦点作用域（按 scope 身份持久化，跨重组不丢）。
/// 焦点变化时失效新旧作用域（读取 `focused_scope()` 的 UI 如聚焦外框得以重组）。
pub fn set_focus(scope: ScopeId) {
    with_rt(|rt| {
        if rt.focused_scope == Some(scope) {
            return;
        }
        let old = rt.focused_scope.replace(scope);
        if let Some(o) = old {
            schedule_scope_rt(rt, o);
        }
        schedule_scope_rt(rt, scope);
    });
}

/// 读取当前键盘焦点作用域。
pub fn focused_scope() -> Option<ScopeId> {
    with_rt(|rt| rt.focused_scope)
}

/// 清除键盘焦点（同时失效原焦点作用域）。
pub fn clear_focus() {
    with_rt(|rt| {
        if let Some(o) = rt.focused_scope.take() {
            schedule_scope_rt(rt, o);
        }
    });
}

// ══════════════════════════════ State ══════════════════════════════

/// 可写可观察状态 — Compose `MutableState<T>`（句柄 Copy，克隆即复制索引）。
pub struct MutableState<T: 'static> {
    id: StateId,
    _marker: std::marker::PhantomData<fn() -> T>,
}

// 句柄无条件 Copy/Clone（不要求 T: Copy）——arena 索引即身份
impl<T: 'static> Clone for MutableState<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: 'static> Copy for MutableState<T> {}

/// 只读状态 — Compose `State<T>`。
pub type State<T> = MutableState<T>;

/// `mutableStateOf(initial)`。
///
/// 要求 `T: PartialEq`：`MutableState::set`/`update` 据此在值未变化时不触发失效，
/// 从而避免「同值反复写 → 重组无限不收敛」之类的问题。
pub fn mutableStateOf<T: PartialEq + 'static>(value: T) -> MutableState<T> {
    let id = with_rt(|rt| {
        let id = rt.states.len() as u32;
        rt.states.push(StateSlot {
            value: Box::new(value),
            eq: Box::new(|a: &dyn Any, b: &dyn Any| a.downcast_ref::<T>() == b.downcast_ref::<T>()),
        });
        StateId(id)
    });
    MutableState {
        id,
        _marker: std::marker::PhantomData,
    }
}

impl<T: 'static> MutableState<T> {
    fn key(&self) -> u32 {
        self.id.0
    }

    pub fn as_state(&self) -> State<T> {
        *self
    }

    /// 读非 Copy 类型（借阅窗口）。
    pub fn with_value<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        register_read(self.key());
        let key = self.key();
        with_rt(|rt| f(rt.states[key as usize].value.downcast_ref::<T>().unwrap()))
    }

    /// 原地借阅写（不触发失效，用于批量更新后手动 `invalidate`）。
    pub fn peek_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let key = self.key();
        with_rt(|rt| f(rt.states[key as usize].value.downcast_mut::<T>().unwrap()))
    }
}

impl<T: 'static + PartialEq> MutableState<T> {
    /// 写入并在值变化时失效读者（Compose `value = x`）。
    ///
    /// 与 Compose `Snapshot` 一致：仅当新值与旧值不相等时才通知读者；
    /// 同值写入不会触发重组，从根上消除「组合期写自己读的 state」类死循环。
    pub fn set(&self, value: T) {
        let key = self.key();
        with_rt(|rt| {
            let slot = &mut rt.states[key as usize];
            if (slot.eq)(slot.value.as_ref(), &value) {
                return;
            }
            *slot.value.downcast_mut::<T>().unwrap() = value;
            invalidate_state(rt, key);
        });
        process_invalidations();
    }

    /// 原地更新并在值变化时失效读者（Compose `value = value.apply(...)`）。
    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: Clone,
    {
        let key = self.key();
        with_rt(|rt| {
            let slot = &mut rt.states[key as usize];
            let old = slot.value.downcast_ref::<T>().unwrap().clone();
            f(slot.value.downcast_mut::<T>().unwrap());
            if !(slot.eq)(slot.value.as_ref(), &old) {
                invalidate_state(rt, key);
            }
        });
        process_invalidations();
    }
}

impl<T: Copy> MutableState<T> {
    /// 读并注册依赖（Compose `value` getter）。
    pub fn get(&self) -> T {
        register_read(self.key());
        let key = self.key();
        with_rt(|rt| *rt.states[key as usize].value.downcast_ref::<T>().unwrap())
    }

    /// 读但不注册依赖（Compose `unreadValue`，绘制/事件读取用）。
    pub fn read_unregistered(&self) -> T {
        let key = self.key();
        with_rt(|rt| *rt.states[key as usize].value.downcast_ref::<T>().unwrap())
    }
}

impl<T: Clone + 'static> MutableState<T> {
    /// 读克隆（非 Copy 类型，注册依赖）。
    pub fn read_clone(&self) -> T {
        register_read(self.id.0);
        with_rt(|rt| {
            rt.states[self.id.0 as usize]
                .value
                .downcast_ref::<T>()
                .unwrap()
                .clone()
        })
    }
}

// ── 依赖注册与失效 ──────────────────────────────────────────────────────────

fn register_read(key: u32) {
    with_rt(|rt| {
        if let Some(&ctx) = rt.read_stack.last() {
            match ctx {
                ReadCtx::Scope(sid) => {
                    rt.readers
                        .entry(key)
                        .or_default()
                        .insert(ReaderKey::Scope(sid));
                    rt.scope_reads.entry(sid.0).or_default().insert(key);
                }
                ReadCtx::Observer(oid) => {
                    rt.readers
                        .entry(key)
                        .or_default()
                        .insert(ReaderKey::Observer(oid));
                    rt.observer_reads.entry(oid).or_default().insert(key);
                }
            }
        }
    });
}

fn unsubscribe_scope(rt: &mut RuntimeData, sid: ScopeId) {
    if let Some(old) = rt.scope_reads.remove(&sid.0) {
        for k in old {
            if let Some(set) = rt.readers.get_mut(&k) {
                set.remove(&ReaderKey::Scope(sid));
                if set.is_empty() {
                    rt.readers.remove(&k);
                }
            }
        }
    }
}

fn unsubscribe_observer(rt: &mut RuntimeData, oid: u64) {
    if let Some(old) = rt.observer_reads.remove(&oid) {
        for k in old {
            if let Some(set) = rt.readers.get_mut(&k) {
                set.remove(&ReaderKey::Observer(oid));
                if set.is_empty() {
                    rt.readers.remove(&k);
                }
            }
        }
    }
}

fn invalidate_state(rt: &mut RuntimeData, key: u32) {
    let affected: Vec<ReaderKey> = rt
        .readers
        .get(&key)
        .map(|s| s.iter().copied().collect())
        .unwrap_or_default();
    if affected.is_empty() {
        return;
    }
    // 仅收集，处理放到借用外（观察者闭包需要再入 runtime）
    rt.invalidations.extend(affected);
}

/// 失效传播 / 重组收敛的最大轮数 —— 超过即强制退出，把「必然冻死」降级为「可定位告警」。
const MAX_INVALIDATION_ROUNDS: usize = 64;
const MAX_RECOMPOSE_ROUNDS: usize = 64;

static INVALIDATION_WARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static RECOMPOSE_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 循环未收敛告警（同名只打印一次，避免每帧刷屏）。
fn warn_unconverged(what: &str, rounds: usize) {
    eprintln!(
        "[compose] {what} 连续 {rounds} 轮仍未收敛，已强制退出以避免死循环。\n\
         \x20       常见原因：① 组合期写入自己正在读的 state（自失效作用域）；\n\
         \x20       ② 观察者互相失效；③ 状态在重组中被无条件重写。"
    );
}

/// 消费本轮失效（在 with_rt 借用外调用）。
fn process_invalidations() {
    let mut rounds = 0usize;
    loop {
        let batch: Vec<ReaderKey> = with_rt(|rt| std::mem::take(&mut rt.invalidations));
        if batch.is_empty() {
            break;
        }
        rounds += 1;
        if rounds > MAX_INVALIDATION_ROUNDS {
            if !INVALIDATION_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                warn_unconverged("process_invalidations", MAX_INVALIDATION_ROUNDS);
            }
            break;
        }
        let mut any = false;
        for r in batch {
            match r {
                ReaderKey::Scope(sid) => {
                    schedule_scope(sid);
                    any = true;
                }
                ReaderKey::Observer(oid) => {
                    run_observer(oid);
                    any = true;
                }
            }
        }
        if any {
            run_recompose_listeners();
        }
    }
}

fn run_recompose_listeners() {
    let mut listeners = with_rt(|rt| std::mem::take(&mut rt.recompose_listeners));
    for l in &mut listeners {
        l();
    }
    with_rt(|rt| rt.recompose_listeners = listeners);
}

/// 观察者运行（借用外执行闭包，take-put）。
fn run_observer(oid: u64) {
    let mut f = with_rt(|rt| {
        unsubscribe_observer(rt, oid);
        rt.read_stack.push(ReadCtx::Observer(oid));
        match rt.observers.get_mut(oid as usize).and_then(|s| s.as_mut()) {
            Some(slot) => Some(std::mem::replace(&mut slot.f, Box::new(|| {}))),
            None => None,
        }
    });
    if let Some(mut f) = f.take() {
        f();
        with_rt(|rt| {
            rt.read_stack.pop();
            if let Some(slot) = rt.observers.get_mut(oid as usize).and_then(|s| s.as_mut()) {
                slot.f = f;
            }
        });
    } else {
        with_rt(|rt| {
            rt.read_stack.pop();
        });
    }
}

fn schedule_scope_rt(rt: &mut RuntimeData, sid: ScopeId) {
    if rt.pending_set.insert(sid) {
        rt.pending.push(sid);
    }
}

/// 读取状态句柄的 arena id（测试 / 调试用：验证 remember 身份）。
pub fn state_id<T: 'static>(st: MutableState<T>) -> u32 {
    st.id.0
}

/// 调度作用域重组（幂等）。
pub fn schedule_scope(sid: ScopeId) {
    with_rt(|rt| schedule_scope_rt(rt, sid));
}

// ══════════════════════════════ 作用域 ══════════════════════════════

/// Composer 句柄 — 组合函数第一个参数。
///
/// 状态在线程局部 Runtime（对齐 Compose 编译器隐式穿线 Composer 的模型）。
pub struct Composer {
    _priv: (),
}

/// 分配新作用域（无 c 版本，供 `emit_scope_impl` 在 root 路径使用）。
fn alloc_scope() -> ScopeId {
    with_rt(|rt| {
        let sid = ScopeId(rt.scopes.len() as u32);
        rt.scopes.push(Some(ScopeData {
            node: None,
            slots: Vec::new(),
            child_scopes: Vec::new(),
            run_cursor: 0,
            child_cursor: 0,
            content: None,
            locals: HashMap::new(),
            disposed: false,
            disposables: Vec::new(),
        }));
        sid
    })
}

/// 发射重组作用域：内容作为 `content` 运行，其发射的 widget 挂到当前父节点。
///
/// 子作用域按父作用域内位置复用（槽位语义），父重组时立即重跑。
/// 父作用域从线程局部 `scope_stack` 读取（不再需要显式传入 `c`）。
pub fn emit_scope(content: impl Fn() + 'static) -> ScopeId {
    emit_scope_impl(None, None, content)
}

/// 内容发射到指定容器节点（widget 容器实现用：Column/Row/box 的 content 作用域）。
pub(crate) fn emit_scope_into(
    node: crate::layout::NodeId,
    content: impl Fn() + 'static,
) -> ScopeId {
    emit_scope_impl(None, Some(node), content)
}

fn emit_scope_impl(
    seed_locals: Option<(TypeId, Box<dyn Any>)>,
    node_override: Option<crate::layout::NodeId>,
    content: impl Fn() + 'static,
) -> ScopeId {
    let parent = with_rt(|rt| rt.scope_stack.last().copied());

    let scope: ScopeId = match parent {
        Some(p) => with_rt(|rt| {
            // 子作用域按自身游标 `child_cursor` 复用（与 `child_scopes` 严格 1:1），
            // 不得与 remember 的 `run_cursor` 混用（否则交替出现时位置失配）。
            let cursor = rt.scopes[p.0 as usize].as_ref().unwrap().child_cursor;
            rt.scopes[p.0 as usize].as_mut().unwrap().child_cursor += 1;
            let reuse = {
                let cs = &rt.scopes[p.0 as usize].as_ref().unwrap().child_scopes;
                cs.get(cursor).copied().filter(|s| {
                    rt.scopes
                        .get(s.0 as usize)
                        .and_then(|o| o.as_ref())
                        .map(|sd| !sd.disposed)
                        .unwrap_or(false)
                })
            };
            match reuse {
                Some(s) => {
                    rt.scopes[s.0 as usize].as_mut().unwrap().content = Some(Box::new(content));
                    s
                }
                None => {
                    let sid = ScopeId(rt.scopes.len() as u32);
                    rt.scopes.push(Some(ScopeData {
                        node: None,
                        slots: Vec::new(),
                        child_scopes: Vec::new(),
                        run_cursor: 0,
                        child_cursor: 0,
                        content: Some(Box::new(content)),
                        locals: HashMap::new(),
                        disposed: false,
                        disposables: Vec::new(),
                    }));
                    let cs = &mut rt.scopes[p.0 as usize].as_mut().unwrap().child_scopes;
                    if cursor < cs.len() {
                        cs[cursor] = sid;
                    } else {
                        cs.push(sid);
                    }
                    sid
                }
            }
        }),
        None => {
            let sid = alloc_scope();
            with_rt(|rt| {
                rt.scopes[sid.0 as usize].as_mut().unwrap().content = Some(Box::new(content))
            });
            sid
        }
    };

    if let Some((tid, v)) = seed_locals {
        with_rt(|rt| {
            rt.scopes[scope.0 as usize]
                .as_mut()
                .unwrap()
                .locals
                .insert(tid, v);
        });
    }

    // 作用域容器节点：容器型 widget 传入既有节点；纯作用域（provides 等）
    // 自建透传 Box 组节点（对齐 Compose 的 Group 节点语义）。
    let node = match node_override {
        Some(n) => n,
        None => crate::layout::add_node(
            crate::layout::Policy::Box(crate::layout::Alignment::TopStart),
            crate::layout::Modifier::new(),
        ),
    };
    with_rt(|rt| {
        rt.scopes[scope.0 as usize].as_mut().unwrap().node = Some(node);
        rt.nodes[node.index()].owner_scope = Some(scope);
    });

    run_scope(scope);
    scope
}

/// （重）运行作用域：解除旧依赖 → 清子树 → 重置游标 → 跑内容 → GC 陈旧子作用域。
/// 节点 arena 可达性 GC：从组合根（及无父节点的独立根）出发标记存活节点，
/// 其余已脱离树的节点归还自由表。
///
/// 为什么不能「摘除即释放」：作用域树与节点树会错位 —— 某个活作用域的节点物理上挂在
/// 另一个作用域的节点下，祖先重组摘除子节点引用时它仍然有效。只有可达性判据同时对
/// 两类情况成立：真孤儿不可达被回收，活节点总能从根走到。
pub(crate) fn collect_node_garbage() {
    let pending = with_rt(|rt| std::mem::replace(&mut rt.node_gc_pending, false));
    if !pending {
        return;
    }
    crate::layout::sweep_nodes();
}

fn run_scope(sid: ScopeId) {
    // 准备：解订阅、摘子树、取内容（take-put 防重入）
    let (node, content) = with_rt(|rt| {
        let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
        s.run_cursor = 0;
        s.child_cursor = 0;
        unsubscribe_scope(rt, sid);
        let node = rt.scopes[sid.0 as usize].as_ref().unwrap().node;
        if let Some(n) = node {
            // 仅摘除子节点引用，**不销毁后代作用域**：
            // 内容重跑会按位置复用子作用域（其 remember 槽随之存活）；
            // 本轮未触达的陈旧子作用域由末尾 GC 统一销毁。
            // 此前这里 dispose_subtree 会把后代 scope 置 disposed，
            // 导致位置复用永远失败、remember 状态随祖先重组整体丢失。
            // 节点身份不跨重组：旧子树只摘不立即回收 —— 作用域树与节点树可能错位
            // （某个活作用域的节点仍挂在别处），直接释放会误伤活节点。统一交给 GC：
            // 从根做可达性扫描，回收真正无人引用的节点（见 `collect_node_garbage`）。
            let _ = std::mem::take(&mut rt.nodes[n.index()].children);
            rt.node_gc_pending = true;
            // 增量渲染：作用域重组 → 整个子树（含内联子节点，其 draw 闭包被重跑更新）
            // 标记脏；同时标记祖先，保证布局遍历不会因祖先被跳过而漏掉本子树。
            crate::layout::mark_dirty_subtree(rt, n);
            crate::layout::mark_dirty_path(rt, n);
        }
        (
            node,
            rt.scopes[sid.0 as usize].as_mut().unwrap().content.take(),
        )
    });
    drain_disposal_queue();

    if let Some(content) = content {
        // 祖先链缓冲跨调用复用：state 动画每帧都会定向重组作用域，逐个分配 `Vec` 没必要。
        let mut chain = SCOPE_CHAIN.with(|c| std::mem::take(&mut *c.borrow_mut()));
        let extra_ancestors = with_rt(|rt| {
            // 定向重组时祖先作用域不在栈上——从节点父链重建（CompositionLocal 沿祖先链查找）。
            // emit 路径中祖先已在栈上，跳过重复压入。
            let mut extra = 0usize;
            if let Some(n) = node {
                chain.clear();
                let mut cur = rt.nodes[n.index()].parent;
                while let Some(p) = cur {
                    if let Some(owner) = rt.nodes[p.index()].owner_scope {
                        chain.push(owner);
                    }
                    cur = rt.nodes[p.index()].parent;
                }
                // 收集顺序是 leaf→root，压栈需要 root→leaf。
                for s in chain.iter().rev() {
                    if !rt.scope_stack.contains(s) {
                        rt.scope_stack.push(*s);
                        extra += 1;
                    }
                }
            }
            rt.scope_stack.push(sid);
            rt.read_stack.push(ReadCtx::Scope(sid));
            if let Some(n) = node {
                rt.parent_stack.push(n);
            }
            // disposables 帧栈：本作用域内容注册的清理器归属本作用域
            rt.disposable_stack.push(Vec::new());
            extra
        });
        content();
        with_rt(|rt| {
            rt.parent_stack.pop();
            rt.read_stack.pop();
            rt.scope_stack.pop();
            for _ in 0..extra_ancestors {
                rt.scope_stack.pop();
            }
            let frame = rt.disposable_stack.pop().unwrap_or_default();
            if let Some(s) = rt.scopes[sid.0 as usize].as_mut() {
                s.disposables = frame;
            }
            rt.scopes[sid.0 as usize].as_mut().unwrap().content = Some(content);
        });
        SCOPE_CHAIN.with(|c| *c.borrow_mut() = chain);
    }

    // GC：本轮未触达的子作用域槽截断丢弃（按子作用域游标 `child_cursor`）
    let (emitted, stale) = with_rt(|rt| {
        let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
        let emitted = s.child_cursor;
        let stale: Vec<ScopeId> = s
            .child_scopes
            .drain(emitted.min(s.child_scopes.len())..)
            .collect();
        (emitted, stale)
    });
    let _ = emitted;
    for sid in stale {
        with_rt(|rt| dispose_scope_rt(rt, sid));
    }
    drain_disposal_queue();
}

/// 执行排队的清理器（借用外）。
pub(crate) fn drain_disposal_queue() {
    let queue = with_rt(|rt| std::mem::take(&mut rt.disposal_queue));
    for mut d in queue {
        d();
    }
}

/// 释放作用域（纯函数：排队清理器，不执行；调用方负责 drain）。
pub(crate) fn dispose_scope_rt(rt: &mut RuntimeData, sid: ScopeId) {
    let s = match rt.scopes.get_mut(sid.0 as usize) {
        Some(Some(s)) => s,
        _ => return,
    };
    if s.disposed {
        return;
    }
    s.disposed = true;
    let children = std::mem::take(&mut s.child_scopes);
    let disposables = std::mem::take(&mut s.disposables);
    // 作用域独占的节点子树随之回收（父作用域重跑时通常已摘除并回收，`alive` 兜底防重）；
    // 挂在本作用域节点上的帧驱动一并注销 —— 对齐 Compose「组合取消即取消 withFrameNanos」。
    if let Some(n) = s.node.take() {
        if let Some(p) = rt.nodes[n.index()].parent {
            rt.nodes[p.index()].children.retain(|c| *c != n);
        }
        rt.node_gc_pending = true;
    }
    rt.frame_drivers.retain(|_, d| d.owner != Some(sid));
    unsubscribe_scope(rt, sid);
    rt.scopes[sid.0 as usize] = None;
    rt.disposal_queue.extend(disposables);
    for ch in children {
        dispose_scope_rt(rt, ch);
    }
}

/// 作用域销毁（借用外入口）。
pub(crate) fn dispose_scope_pub(sid: ScopeId) {
    with_rt(|rt| dispose_scope_rt(rt, sid));
    drain_disposal_queue();
}

// ══════════════════════════════ remember ══════════════════════════════

/// `remember { init }` — 作用域内位置记忆（位置即身份，对齐 SlotTable 槽位语义）。
pub fn remember<T: Clone + 'static>(init: impl FnOnce() -> T) -> T {
    let sid = current_scope_id().expect("remember called outside composition scope");
    let cursor = alloc_slot(sid);
    // 短借查槽；init 在借用外执行（init 可能再入 runtime，如 mutableStateOf）
    let existing: Option<T> = with_rt(|rt| {
        let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
        if cursor < s.slots.len() {
            s.slots[cursor].downcast_ref::<T>().map(|v| v.clone())
        } else {
            None
        }
    });
    match existing {
        Some(v) => v,
        None => {
            let v = init();
            with_rt(|rt| {
                let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
                if cursor < s.slots.len() {
                    s.slots[cursor] = Box::new(v.clone());
                } else {
                    s.slots.push(Box::new(v.clone()));
                }
            });
            v
        }
    }
}

/// `remember(key) { init }` — 键控记忆：key 变化时重新初始化。
pub fn keyed_remember<T: Clone + 'static>(key: u64, init: impl FnOnce() -> T) -> T {
    let sid = current_scope_id().expect("keyed_remember outside scope");
    let cursor = alloc_slot(sid);
    enum Slot<T> {
        Hit(T),
        Miss,
        KeyChange,
    }
    let probe: Slot<T> = with_rt(|rt| {
        let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
        if cursor < s.slots.len() {
            if let Some((k, v)) = s.slots[cursor].downcast_ref::<(u64, T)>() {
                if *k == key {
                    return Slot::Hit(v.clone());
                }
            }
            Slot::KeyChange
        } else {
            Slot::Miss
        }
    });
    match probe {
        Slot::Hit(v) => v,
        Slot::Miss | Slot::KeyChange => {
            let v = init();
            with_rt(|rt| {
                let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
                let entry = Box::new((key, v.clone()));
                if cursor < s.slots.len() {
                    s.slots[cursor] = entry;
                } else {
                    s.slots.push(entry);
                }
            });
            v
        }
    }
}

/// remember / 子作用域发射共用位置游标（SlotTable 中 data槽 与 group槽
/// 交错的位置编码——同一作用域内按代码顺序交替推进同一游标）。
fn alloc_slot(sid: ScopeId) -> usize {
    with_rt(|rt| {
        let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
        let cur = s.run_cursor;
        s.run_cursor += 1;
        cur
    })
}

// ══════════════════════════════ Recomposer ══════════════════════════════

/// 重组调度器 — Compose `Recomposer`。
pub struct Recomposer;

impl Recomposer {
    pub fn has_pending() -> bool {
        with_rt(|rt| !rt.pending.is_empty())
    }

    /// 运行全部待重组作用域（一帧的 composition pass）。
    ///
    /// 有轮数上限：若某作用域在重组中又把自己排回 pending（写了自己正在读的
    /// state），这里原本会无界循环 → 单帧卡死 + CPU 100% + 窗口空白。
    /// 超出上限则告警并退出，保证帧一定能走完。
    pub fn recompose_pending() {
        let mut rounds = 0usize;
        loop {
            // 批次缓冲跨轮/跨帧复用：`mem::take` 出去处理完再还回去。
            // 旧实现在每轮结束时丢弃缓冲，动画期间每帧都要为新调度的作用域重新分配一次。
            let mut batch: Vec<ScopeId> = with_rt(|rt| {
                let b = std::mem::take(&mut rt.pending);
                rt.pending_set.clear();
                b
            });
            if batch.is_empty() {
                with_rt(|rt| rt.pending = batch);
                collect_node_garbage();
                return;
            }
            rounds += 1;
            if rounds > MAX_RECOMPOSE_ROUNDS {
                if !RECOMPOSE_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    warn_unconverged("Recomposer::recompose_pending", MAX_RECOMPOSE_ROUNDS);
                }
                return;
            }
            for &sid in &batch {
                let alive = with_rt(|rt| {
                    rt.scopes
                        .get(sid.0 as usize)
                        .and_then(|s| s.as_ref())
                        .map(|s| !s.disposed)
                        .unwrap_or(false)
                });
                if alive {
                    run_scope(sid);
                }
            }
            // 本轮重组中新调度的作用域接回同一块缓冲，下一轮（下一帧）继续复用其容量。
            with_rt(|rt| {
                let fresh = std::mem::take(&mut rt.pending);
                batch.clear();
                batch.extend(fresh);
                rt.pending = batch;
            });
        }
    }

    /// 注册重组监听（状态变化 → 请求重绘/帧调度）。
    pub fn on_recompose(listener: impl FnMut() + 'static) {
        with_rt(|rt| rt.recompose_listeners.push(Box::new(listener)));
    }
}

// ══════════════════════════════ 帧时钟 ══════════════════════════════

/// 平台帧推进：每帧调用（渲染循环 / 测试手动 tick）。
/// 依次：注入帧时长 → 消费重组 → 动画帧时钟 + 帧驱动（调度重组）→ 再消费重组 → 推进协程。
pub fn advance_frame(delta: f32) {
    with_rt(|rt| {
        rt.frame_delta = delta;
        rt.frame += 1;
        // 帧时钟只前进不倒退（delta 为负/异常时保持单调）
        rt.frame_time_nanos += (delta.max(0.0) as f64) * 1_000_000_000.0;
    });
    Recomposer::recompose_pending();
    // 活跃动画集合驱动（对齐 Recomposer：只调度在跑的那些作用域）
    crate::animation::tick_animations();
    // 按压波纹推进（生长/淡出/剪枝）——与动画同处，不进组合。
    crate::layout::tick_press_state();
    run_frame_drivers();
    Recomposer::recompose_pending();
    drain_tasks();
    collect_node_garbage();
}

fn run_frame_drivers() {
    let mut ids = DRIVER_IDS.with(|c| std::mem::take(&mut *c.borrow_mut()));
    with_rt(|rt| ids.extend(rt.frame_drivers.iter().map(|(&id, d)| (id, d.owner))));
    for &(id, owner) in &ids {
        // take-put 防重入：回调里可能注销自身、销毁作用域或注册新驱动
        // 归属作用域随快照带入：注册后不变，省掉逐驱动一次 `get` 哈希查找。
        let taken = with_rt(|rt| {
            if let Some(owner) = owner {
                if !owner_alive_in(rt, owner) {
                    rt.frame_drivers.remove(&id);
                    return None;
                }
            }
            rt.frame_drivers.get_mut(&id)?.run.take().map(|f| (id, f))
        });
        if let Some((id, mut f)) = taken {
            f();
            with_rt(|rt| {
                if let Some(d) = rt.frame_drivers.get_mut(&id) {
                    d.run = Some(f);
                }
            });
        }
    }
    ids.clear();
    DRIVER_IDS.with(|c| *c.borrow_mut() = ids);
}

/// 帧驱动记录 —— 组合期注册的驱动归属当前作用域：作用域销毁即自动注销
/// （对齐 Compose 协程随 composition 取消），修掉「驱动只增不减 → 静止页仍 60fps」。
///
/// 以 id 为键存在 `HashMap` 里：每帧都要按 id take-put，线性 `Vec` 在驱动多时是 O(N²)。
pub(crate) struct FrameDriver {
    pub(crate) owner: Option<ScopeId>,
    /// 运行期被取出（take-put），空表示正在执行或已注销。
    pub(crate) run: Option<Box<dyn FnMut()>>,
}

/// 帧驱动句柄 —— 由 `add_frame_driver` 返回，用于 `remove_frame_driver` 注销。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameDriverHandle(pub(crate) u64);

/// 注册帧驱动（动画：每帧调度其作用域重组）。返回句柄以便作用域销毁时注销。
pub fn add_frame_driver(f: impl FnMut() + 'static) -> FrameDriverHandle {
    let owner = current_scope_id();
    with_rt(|rt| {
        let id = rt.frame_driver_seq;
        rt.frame_driver_seq += 1;
        rt.frame_drivers.insert(
            id,
            FrameDriver {
                owner,
                run: Some(Box::new(f)),
            },
        );
        FrameDriverHandle(id)
    })
}

/// 注销帧驱动（作用域销毁时调用，避免驱动随作用域消失后继续驻留）。
pub fn remove_frame_driver(handle: FrameDriverHandle) {
    with_rt(|rt| {
        rt.frame_drivers.remove(&handle.0);
    });
}

pub fn current_frame() -> u64 {
    with_rt(|rt| rt.frame)
}

/// 在册帧驱动数量（诊断 / 回归用：控件注销后应回落，不随重组增长）。
pub fn frame_driver_count() -> usize {
    with_rt(|rt| rt.frame_drivers.len())
}

/// 单调帧时钟（纳秒）— Compose `MonotonicFrameClock` 的 `withFrameNanos` 时间戳。
pub fn frame_time_nanos() -> f64 {
    with_rt(|rt| rt.frame_time_nanos)
}

/// 帧时钟 + 当前组合作用域一次读出（动画采样每帧都要两者，合并省一次借用）。
pub(crate) fn frame_time_and_scope() -> (f64, Option<ScopeId>) {
    with_rt(|rt| (rt.frame_time_nanos, rt.scope_stack.last().copied()))
}

/// 上一帧的帧间隔（秒）。
pub fn frame_delta() -> f32 {
    with_rt(|rt| rt.frame_delta)
}

/// 作用域是否仍存活（动画注册表剪枝用：宿主作用域销毁后其动画不再续帧）。
pub fn scope_alive(sid: ScopeId) -> bool {
    with_rt(|rt| owner_alive_in(rt, sid))
}

fn owner_alive_in(rt: &RuntimeData, sid: ScopeId) -> bool {
    rt.scopes
        .get(sid.0 as usize)
        .and_then(|s| s.as_ref())
        .map(|s| !s.disposed)
        .unwrap_or(false)
}

/// `delay_frames(n)` — 挂起 n 帧（协程演示 / 动画等待用）。
pub async fn delay_frames(n: u64) {
    let start = current_frame();
    loop {
        if current_frame() >= start + n {
            return;
        }
        std::future::pending::<()>().await;
    }
}

// ══════════════════════════════ 协程执行器 ══════════════════════════════

/// 生成协程任务，返回句柄（可取消）。
pub fn spawn_task(future: impl std::future::Future<Output = ()> + 'static) -> TaskId {
    with_rt(|rt| {
        let id = TaskId(rt.tasks.len() as u32);
        rt.tasks.push(Some(TaskSlot {
            fut: Some(Box::pin(future)),
        }));
        id
    })
}

pub fn cancel_task(task: TaskId) {
    with_rt(|rt| {
        if let Some(slot) = rt.tasks.get_mut(task.0 as usize).and_then(|s| s.as_mut()) {
            slot.fut = None;
        }
    });
}

/// 单帧内任务推进的最大轮数（防止原地空转）。
const MAX_DRAIN_ROUNDS: usize = 8;

/// 推进全部就绪任务（每帧调用；await 点挂起的任务下帧再 poll）。
///
/// 不能无界 `loop`：Pending 任务会被放回 `rt.tasks`，下一轮又被取出来 poll，
/// 只要存在任一挂起任务就永不退出 —— 表现为单帧死循环、CPU 100%、首帧不呈现、
/// 事件循环被堵死（窗口空白且无响应）。
/// 这里改为「有进展才继续」的有限轮：本轮全是 Pending 就直接返回，下帧再 poll。
pub fn drain_tasks() {
    for _ in 0..MAX_DRAIN_ROUNDS {
        // take-put：poll 期间任务内部可能再 spawn / 读写状态
        let mut batch: Vec<(
            TaskId,
            std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
        )> = with_rt(|rt| {
            let mut out = Vec::new();
            for (i, slot) in rt.tasks.iter_mut().enumerate() {
                if let Some(s) = slot {
                    if let Some(f) = s.fut.take() {
                        out.push((TaskId(i as u32), f));
                    }
                }
            }
            out
        });
        if batch.is_empty() {
            return;
        }
        let waker = noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        let mut leftovers = Vec::new();
        let mut progressed = false;
        for (id, mut f) in batch.drain(..) {
            match std::future::Future::poll(f.as_mut(), &mut cx) {
                std::task::Poll::Ready(()) => {
                    with_rt(|rt| rt.tasks[id.0 as usize] = None);
                    progressed = true;
                }
                std::task::Poll::Pending => {
                    leftovers.push((id, f));
                }
            }
        }
        with_rt(|rt| {
            for (id, f) in leftovers {
                if let Some(slot) = rt.tasks.get_mut(id.0 as usize).and_then(|s| s.as_mut()) {
                    slot.fut = Some(f);
                }
            }
        });
        if !progressed {
            return;
        }
    }
}

fn noop_waker() -> std::task::Waker {
    use std::task::{RawWaker, RawWakerVTable, Waker};
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    fn noop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    // 唯一的 unsafe：标准库 Waker::from_raw 惯用法（vtable 全 noop，无内存操作）
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

// ══════════════════════════════ CompositionLocal ══════════════════════════════

/// 组合内隐式环境 — Compose `CompositionLocal<T>`。
pub struct CompositionLocal<T: 'static> {
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: 'static> Default for CompositionLocal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> CompositionLocal<T> {
    pub const fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Clone + 'static> CompositionLocal<T> {
    /// 提供值并组合内容（Compose `LocalX provides value { .. }`）。
    pub fn provides(&self, value: T, content: impl Fn() + 'static) {
        emit_scope_impl(Some((TypeId::of::<T>(), Box::new(value))), None, content);
    }

    /// 同 [`Self::provides`]，但内容作用域直接挂到既有容器节点（省掉透传组节点）。
    ///
    /// 控件容器（按钮、卡片…）用它把前景色下发给 `content`，而不必再套一层作用域。
    pub fn provides_in(
        &self,
        node: crate::layout::NodeId,
        value: T,
        content: impl Fn() + 'static,
    ) {
        emit_scope_impl(
            Some((TypeId::of::<T>(), Box::new(value))),
            Some(node),
            content,
        );
    }

    /// 读取当前值（Compose `LocalX.current`）。
    pub fn current(&self) -> T {
        self.try_current()
            .unwrap_or_else(|| panic!("CompositionLocal not provided"))
    }

    /// 沿作用域栈查找；未提供时返回 `None`（用于有合理默认值的本地，如内容色）。
    pub fn try_current(&self) -> Option<T> {
        let tid = TypeId::of::<T>();
        with_rt(|rt| {
            for sid in rt.scope_stack.iter().rev() {
                if let Some(s) = rt.scopes[sid.0 as usize].as_ref() {
                    if let Some(v) = s.locals.get(&tid) {
                        return Some(v.downcast_ref::<T>().unwrap().clone());
                    }
                }
            }
            None
        })
    }
}

// ══════════════════════════════ Effects ══════════════════════════════

/// `SideEffect { block }` — 组合期间立即执行。
pub fn side_effect(_c: &mut Composer, block: impl FnOnce()) {
    block();
}

/// `LaunchedEffect(key) { block }` — key 变化时取消旧协程、启动新协程。
pub fn launched_effect(key: u64, block: impl std::future::Future<Output = ()> + 'static) {
    let sid = current_scope_id().expect("launched_effect outside scope");
    let cursor = alloc_slot(sid);
    with_rt(|rt| {
        let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
        if cursor < s.slots.len() {
            if let Some((k, old)) = s.slots[cursor].downcast_ref::<(u64, TaskId)>() {
                if *k == key {
                    return;
                }
                let old = *old;
                if let Some(slot) = rt.tasks.get_mut(old.0 as usize).and_then(|t| t.as_mut()) {
                    slot.fut = None;
                }
            }
        }
        let task_id = TaskId(rt.tasks.len() as u32);
        rt.tasks.push(Some(TaskSlot {
            fut: Some(Box::pin(block)),
        }));
        let entry = Box::new((key, task_id));
        if cursor < s.slots.len() {
            s.slots[cursor] = entry;
        } else {
            s.slots.push(entry);
        }
    });
}

/// `DisposableEffect(key)` — 返回守卫，`guard.on_dispose(f)` 注册清理函数；
/// key 变化 / 作用域销毁时执行。
pub fn disposable_effect(key: u64) -> DisposableEffectGuard {
    let sid = current_scope_id().expect("disposable_effect outside scope");
    let cursor = alloc_slot(sid);
    with_rt(|rt| {
        // 清理器挂到当前组合帧栈（run_scope 出栈时归属本作用域）
        let frame = match rt.disposable_stack.last_mut() {
            Some(f) => f,
            None => {
                rt.disposable_stack.push(Vec::new());
                rt.disposable_stack.last_mut().unwrap()
            }
        };
        let s = rt.scopes[sid.0 as usize].as_mut().unwrap();
        if cursor < s.slots.len() {
            if let Some((k, old)) = s.slots[cursor].downcast_mut::<(u64, Box<dyn FnMut()>)>() {
                if *k == key {
                    // 复用：旧清理器重新挂到本轮帧栈
                    let old = std::mem::replace(old, Box::new(|| {}));
                    let idx = frame.len();
                    frame.push(old);
                    s.slots[cursor] = Box::new((key, idx));
                    return DisposableEffectGuard { idx, fresh: false };
                }
                // key 变化：先跑旧清理
                let mut old = std::mem::replace(old, Box::new(|| {}));
                old();
            }
        }
        let idx = frame.len();
        frame.push(Box::new(|| {}));
        let entry = Box::new((key, idx));
        if cursor < s.slots.len() {
            s.slots[cursor] = entry;
        } else {
            s.slots.push(entry);
        }
        DisposableEffectGuard { idx, fresh: true }
    })
}

/// DisposableEffect 守卫。
pub struct DisposableEffectGuard {
    idx: usize,
    fresh: bool,
}

impl DisposableEffectGuard {
    pub fn on_dispose(&self, cleanup: impl FnMut() + 'static) {
        if !self.fresh {
            return;
        }
        with_rt(|rt| {
            if let Some(frame) = rt.disposable_stack.last_mut() {
                if self.idx < frame.len() {
                    frame[self.idx] = Box::new(cleanup);
                }
            }
        });
    }
}

// ══════════════════════════════ Composition ══════════════════════════════

/// 组合 — 内容函数 → 节点树的运行容器。
pub struct Composition {
    root_scope: ScopeId,
    root_node: NodeId,
}

impl Composition {
    /// 创建组合，内容发射到 `root_node`。
    pub fn new(root_node: NodeId) -> Self {
        let root_scope = with_rt(|rt| {
            let sid = ScopeId(rt.scopes.len() as u32);
            rt.scopes.push(Some(ScopeData {
                node: Some(root_node),
                slots: Vec::new(),
                child_scopes: Vec::new(),
                run_cursor: 0,
                child_cursor: 0,
                content: None,
                locals: HashMap::new(),
                disposed: false,
                disposables: Vec::new(),
            }));
            rt.nodes[root_node.index()].owner_scope = Some(sid);
            if !rt.root_nodes.contains(&root_node) {
                rt.root_nodes.push(root_node);
            }
            sid
        });
        Self {
            root_scope,
            root_node,
        }
    }

    /// `setContent { content }` — 首次组合。
    pub fn set_content(&self, content: impl Fn() + 'static) {
        with_rt(|rt| {
            rt.scopes[self.root_scope.0 as usize]
                .as_mut()
                .unwrap()
                .content = Some(Box::new(content));
        });
        run_scope(self.root_scope);
        Recomposer::recompose_pending();
    }

    pub fn root_node(&self) -> NodeId {
        self.root_node
    }
}

/// derivedStateOf — 派生状态：任一来源变化时重算并失效下游读者。
pub fn derivedStateOf<T: Clone + PartialEq + 'static>(
    mut calc: impl FnMut() -> T + 'static,
) -> State<T> {
    let oid = with_rt(|rt| {
        let oid = rt.observers.len() as u64;
        rt.observers.push(None);
        oid
    });
    // 首算：读注册到 Observer 上下文（建立依赖）
    with_rt(|rt| rt.read_stack.push(ReadCtx::Observer(oid)));
    let first = (&mut calc)();
    with_rt(|rt| rt.read_stack.pop());
    let out = mutableStateOf(first);
    // 注册持久观察者（借用外运行，内部可再入 runtime）
    with_rt(|rt| {
        rt.observers[oid as usize] = Some(ObserverSlot {
            f: Box::new(move || {
                let v = calc();
                out.set(v);
            }),
        });
    });
    out.as_state()
}
