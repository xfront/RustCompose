//! 异步执行器接入（feature 方式，对齐 Jetpack Compose `rememberCoroutineScope`
//! + `CoroutineScope` 的调度职责）：
//!
//! - **零依赖默认**：`spawn` 复用主线程任务池（每帧 poll），`sleep` 用墙钟帧唤醒；
//! - `--features tokio`：定时器与阻塞池由 tokio current-thread runtime 提供
//!   （后台线程驱动 runtime，Future 仍 poll 在主线程）；
//! - `--features async-std`：定时器与阻塞池由 async-std 提供；
//! - 同时开启两者时 **tokio 优先**。
//!
//! 为什么 Future 必须在主线程 poll：节点、状态、作用域全在线程局部 `RUNTIME` arena
//! 中（`!Send`）。外部执行器只负责「何时就绪」（定时器 / IO / 阻塞任务），
//! 就绪后经 `MainWake` → `post_to_main` → 宿主续帧 → 主线程 poll，
//! 从而可以直接在 `async` 块里读写 `MutableState`、发事件、操作 Composition。

use std::future::Future;
use std::time::Duration;

use crate::animation::{request_anim_frame_after, request_anim_frames};
use crate::runtime::{TaskId, frame_time_nanos, post_to_main, spawn_task};

/// 在 GUI 主线程poll 一个 Future，返回可取消的任务句柄（arena 化 `TaskId`）。
/// 与 `crate::runtime::spawn_task` 等价，提供与执行器实现无关的入口。
pub fn spawn(future: impl Future<Output = ()> + 'static) -> TaskId {
    spawn_task(future)
}

/// 可取消的延时（对齐 `kotlinx.coroutines.delay`）。
///
/// - tokio：`tokio::time::sleep` + 后台 runtime 驱动定时器；
/// - async-std：`async_std::task::sleep`；
/// - 默认：墙钟帧轮询 + `request_anim_frame_after` 定点唤醒
///   （不用帧号计时，宿主休眠时不会停摆）。
#[cfg(feature = "tokio")]
pub async fn sleep(duration: Duration) {
    let _guard = tokio_handle().enter();
    tokio::time::sleep(duration).await;
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
pub async fn sleep(duration: Duration) {
    async_std::task::sleep(duration).await;
}

#[cfg(not(any(feature = "tokio", feature = "async-std")))]
pub async fn sleep(duration: Duration) {
    ClockDelay {
        end: frame_time_nanos() + duration.as_nanos() as f64,
    }
    .await
}

/// 零依赖延时 Future：每次 poll 检查墙钟帧时钟（不是帧号），
/// 未到期时预约宿主定点唤醒；不能用 `pending().await` 配循环——一旦挂起
/// 就再也不会回到循环体检查时间（首版实现踩过此坑）。
#[cfg(not(any(feature = "tokio", feature = "async-std")))]
struct ClockDelay {
    end: f64,
}

#[cfg(not(any(feature = "tokio", feature = "async-std")))]
impl std::future::Future for ClockDelay {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        let now = frame_time_nanos();
        if now >= self.end {
            std::task::Poll::Ready(())
        } else {
            let remain = ((self.end - now) / 1e9) as f32;
            request_anim_frame_after(remain);
            std::task::Poll::Pending
        }
    }
}

/// 阻塞任务（IO / CPU 密集）：在执行器的阻塞池执行，结果经主线程队列回传。
///
/// `done` 在 GUI 线程执行，可直接更新 `MutableState`。
#[cfg(feature = "tokio")]
pub fn spawn_blocking<F, R>(f: F, done: impl FnOnce(R) + Send + 'static)
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio_handle().spawn_blocking(move || {
        let result = f();
        post_to_main(move || done(result));
    });
}

#[cfg(all(feature = "async-std", not(feature = "tokio")))]
pub fn spawn_blocking<F, R>(f: F, done: impl FnOnce(R) + Send + 'static)
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    async_std::task::spawn_blocking(move || {
        let result = f();
        post_to_main(move || done(result));
    });
}

#[cfg(not(any(feature = "tokio", feature = "async-std")))]
pub fn spawn_blocking<F, R>(f: F, done: impl FnOnce(R) + Send + 'static)
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    std::thread::spawn(move || {
        let result = f();
        post_to_main(move || done(result));
    });
}

/// 请求宿主继续泵帧（任务内部等待自定义 IO 时可显式调用）。
pub fn wake_main() {
    request_anim_frames();
}

#[cfg(feature = "tokio")]
fn tokio_handle() -> &'static tokio::runtime::Handle {
    use std::sync::OnceLock;
    static HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();
    HANDLE.get_or_init(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("compose: create tokio runtime");
        let handle = rt.handle().clone();
        // 后台线程驱动 runtime 的定时器 reactor；任务本体仍在主线程被 poll。
        std::thread::spawn(move || {
            let _ = rt.block_on(std::future::pending::<()>());
        });
        handle
    })
}
