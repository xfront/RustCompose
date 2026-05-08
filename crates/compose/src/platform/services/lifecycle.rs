//! 应用生命周期 — 对齐 Flutter `AppLifecycleState` / `WidgetsBindingObserver`。
//!
//! 纯 Rust 实现，跨平台可用：维护当前 [`AppLifecycleState`] 与观察者列表。
//! 各平台事件循环在对应生命周期事件里调用 [`set_state`]（如 Android
//! `InitWindow` → `Resumed`、`TerminateWindow` → `Paused`），业务侧经
//! [`add_observer`] 订阅。无宿主时默认 `Resumed`。

use std::sync::{Mutex, OnceLock};

/// 应用生命周期状态（对齐 Flutter `AppLifecycleState`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLifecycleState {
    /// 应用可见且可交互（前台）。
    Resumed,
    /// 前台但不可交互（如 iOS 来电、画中画）。
    Inactive,
    /// 后台（用户切走）。
    Paused,
    /// 隐藏（如 Android 多窗口被遮挡）。
    Hidden,
    /// 即将销毁（进程回收前）。
    Detached,
}

impl AppLifecycleState {
    /// 从宿主字符串解析（Android `onTrimMemory` / iOS `applicationDidBecomeActive` 等映射）。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "resumed" | "active" => Some(Self::Resumed),
            "inactive" => Some(Self::Inactive),
            "paused" | "background" => Some(Self::Paused),
            "hidden" => Some(Self::Hidden),
            "detached" | "destroyed" => Some(Self::Detached),
            _ => None,
        }
    }
}

type Observer = Box<dyn Fn(AppLifecycleState) + Send + Sync>;

static OBSERVERS: OnceLock<Mutex<Vec<Observer>>> = OnceLock::new();
static STATE: OnceLock<Mutex<AppLifecycleState>> = OnceLock::new();

fn observers() -> &'static Mutex<Vec<Observer>> {
    OBSERVERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn state_cell() -> &'static Mutex<AppLifecycleState> {
    STATE.get_or_init(|| Mutex::new(AppLifecycleState::Resumed))
}

/// 订阅生命周期状态变化（可多次调用，按注册顺序触发）。
pub fn add_observer(observer: Observer) {
    observers().lock().unwrap().push(observer);
}

/// 读取当前生命周期状态。
pub fn current() -> AppLifecycleState {
    *state_cell().lock().unwrap()
}

/// 设置生命周期状态并通知所有观察者（平台事件循环调用）。
pub fn set_state(state: AppLifecycleState) {
    *state_cell().lock().unwrap() = state;
    for ob in observers().lock().unwrap().iter() {
        ob(state);
    }
}

/// 经 [`crate::platform::services::channel`] 的 `lifecycleSet` 同步宿主状态串。
pub fn set_state_from_host(s: &str) {
    if let Some(st) = AppLifecycleState::from_str(s) {
        set_state(st);
    }
}
