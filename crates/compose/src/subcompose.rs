//! SubcomposeLayout — 对齐 Jetpack Compose `SubcomposeLayout`：measure 期按需组合子内容。
//!
//! Compose 的 `SubcomposeLayout` 允许 measurePolicy 在测量阶段按「slot id」组合子内容
//! （如 LazyColumn 根据可用高度组合可见项），子组合的结果参与本轮测量。
//!
//! 本工程的 `perform_layout` 持有 `RUNTIME.borrow_mut()` 贯穿 measure/place，期间无法
//! 再进入组合（RefCell 重入 panic）。故采用**两趟**方案：
//! 1. measure 期 [`subcompose`] 只把「要组合的 slot + 内容闭包」排队（不立即组合）；
//! 2. [`drain_requests`] 在布局的无借用间隙执行组合（按 slot keyed 复用作用域），
//!    之后 `perform_layout` 重新测量一轮，使子内容尺寸反馈到父布局。
//!
//! 关键约束：请求队列必须每轮清空，避免重复组合；slot key 稳定 → keyed scope 复用。

use std::cell::RefCell;

use crate::runtime::ScopeId;

/// 一个子组合请求（在 measure 期排队）。
struct Request {
    parent: ScopeId,
    slot: u64,
    content: Box<dyn Fn() + 'static>,
}

thread_local! {
    static REQUESTS: RefCell<Vec<Request>> = const { RefCell::new(Vec::new()) };
}

/// measure 期请求按 slot 组合子内容（排队，不立即执行）。
pub fn subcompose(parent: ScopeId, slot: u64, content: impl Fn() + 'static) {
    REQUESTS.with(|r| {
        r.borrow_mut().push(Request {
            parent,
            slot,
            content: Box::new(content),
        })
    });
}

/// 当前排队请求数（供 perform_layout 判断是否需要第二趟）。
pub fn pending() -> usize {
    REQUESTS.with(|r| r.borrow().len())
}

/// 在无 RUNTIME 借用时执行所有排队请求：按 slot keyed 组合，返回是否有请求被处理。
pub fn drain_requests() -> bool {
    let requests: Vec<Request> = REQUESTS.with(|r| r.borrow_mut().drain(..).collect());
    if requests.is_empty() {
        return false;
    }
    for req in requests {
        crate::runtime::subcompose_scope(req.parent, req.slot, req.content);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_queue_drains_once() {
        subcompose(ScopeId(0), 1, || {});
        subcompose(ScopeId(0), 2, || {});
        assert_eq!(pending(), 2);
        // 不实际执行（ScopeId(0) 可能非真作用域），只验证队列语义：
        let n = REQUESTS.with(|r| r.borrow_mut().drain(..).count());
        assert_eq!(n, 2);
        assert_eq!(pending(), 0);
    }
}
