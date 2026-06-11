//! 异步接入回归（三种 feature 组合共用同一份测试）：
//! - 默认（零依赖）：主线程任务池 + 墙钟帧唤醒；
//! - `--features tokio`：tokio 定时器 + 阻塞池；
//! - `--features async-std`：async-std 定时器 + 阻塞池。
//!
//! Future 在主线程 poll（可直接改 MutableState），外部执行器只负责就绪唤醒。
//! 两项断言放在同一测试中：全局跨线程投递队列 `MAIN_TASKS` 要求同线程顺序执行。

use compose::async_rt::{sleep, spawn, spawn_blocking};
use compose::runtime::{advance_frame, drain_main_tasks, mutableStateOf};
use std::time::Duration;

fn pump_until(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..400 {
        std::thread::sleep(Duration::from_millis(5));
        advance_frame(1.0 / 60.0);
        drain_main_tasks();
        if done() {
            return true;
        }
    }
    false
}

#[test]
fn async_sleep_and_blocking_result_resume_on_main() {
    let done = mutableStateOf(false);
    let d2 = done;
    spawn(async move {
        sleep(Duration::from_millis(30)).await;
        d2.set(true); // await 前后都可直写线程局部状态（证明 Future poll 在主线程）
    });
    assert!(
        pump_until(|| done.get()),
        "sleep 到期后应恢复执行并置位状态"
    );

    let result = mutableStateOf(0i32);
    let r2 = result;
    spawn_blocking(|| 42i32, move |value| r2.set(value));
    assert!(
        pump_until(|| result.get() == 42),
        "阻塞任务结果应经 post_to_main 回投主线程"
    );
}
