//! Applier — 对齐 Jetpack Compose `Applier<N>`：把组合产出的树变更应用到宿主节点树。
//!
//! Compose 的 `Applier` 抽象「如何把节点插进/移出/重排/清空树」，使组合器与具体树
//! （LayoutNode / SemanticsNode / 自定义树）解耦。本工程节点存于中央 arena（`NodeId`
//! 句柄），因此 `NodeApplier` 作用于 `RuntimeData`，并提供可替换的线程局部实现，
//! 让宿主可注入自定义树操作（对齐 Compose `AbstractApplier` / `LocalApplier`）。
//!
//! 默认实现 `ArenaApplier` 把变更写回 `rt.nodes[..].children` 与 `node.parent`，
//! 与改造前的直接 arena 操作语义一致：
//! - `insert_bottom_up`：先建子树再挂父（本工程 `add_node` 的模式）；
//! - `insert_top_down`：先挂父再填子树（Compose 默认，用于预先占位的容器）；
//! - `remove`：摘除索引处节点并把其子树标记待 GC（不立即释放，避免悬垂）；
//! - `clear`：摘除所有子节点（对齐 `run_scope` 重组前的 `take(children)`）；
//! - `move`：把 `[from, from+count)` 节点滑移到 `to`（Compose `move(from,to,count)`）。
//!
//! `begin_changes`/`end_changes` 界定一批变更：本工程的节点 GC 本就是延迟到空闲帧
//! （`node_gc_pending`），故 `end_changes` 只负责置该标记，不在中途触发回收。

use std::cell::RefCell;

use crate::layout::NodeId;
use crate::runtime::RuntimeData;

/// 树变更 applier：对齐 Compose `Applier<N>`，但操作 arena 索引而非显式 down/up 游标。
pub trait NodeApplier {
    /// 一批变更开始（对齐 `beginChanges`）。
    fn begin_changes(&self, rt: &mut RuntimeData) {
        let _ = rt;
    }
    /// 一批变更结束（对齐 `endChanges`）：标记节点 GC 待处理。
    fn end_changes(&self, rt: &mut RuntimeData) {
        rt.node_gc_pending = true;
    }

    /// 自底向上插入（子树已就绪再挂父），`index` 为插入位，`index == len` 表示追加。
    fn insert_bottom_up(&self, rt: &mut RuntimeData, parent: NodeId, index: usize, child: NodeId);
    /// 自顶向下插入（先挂父再填子树）。
    fn insert_top_down(&self, rt: &mut RuntimeData, parent: NodeId, index: usize, child: NodeId) {
        self.insert_bottom_up(rt, parent, index, child)
    }

    /// 删除：摘除 `[index, index+count)` 处子节点。
    fn remove(&self, rt: &mut RuntimeData, parent: NodeId, index: usize, count: usize);
    /// 移动：把 `[from, from+count)` 的节点滑移到 `to`（Compose `move` 语义）。
    fn move_nodes(&self, rt: &mut RuntimeData, parent: NodeId, from: usize, to: usize, count: usize);
    /// 清空：摘除 parent 的所有子节点（保留 arena 节点，交给 GC）。
    fn clear(&self, rt: &mut RuntimeData, parent: NodeId);
}

/// 默认实现：直接写 arena 的 children / parent。
pub struct ArenaApplier;

impl NodeApplier for ArenaApplier {
    fn insert_bottom_up(&self, rt: &mut RuntimeData, parent: NodeId, index: usize, child: NodeId) {
        rt.nodes[child.index()].parent = Some(parent);
        let kids = &mut rt.nodes[parent.index()].children;
        let at = index.min(kids.len());
        kids.insert(at, child);
    }

    fn remove(&self, rt: &mut RuntimeData, parent: NodeId, index: usize, count: usize) {
        // 与原 `retain` 语义一致：只从父的 children 摘除，保留子节点 parent（GC 安全网）。
        let kids = &mut rt.nodes[parent.index()].children;
        let end = (index + count).min(kids.len());
        if index >= end {
            return;
        }
        kids.drain(index..end);
    }

    fn move_nodes(&self, rt: &mut RuntimeData, parent: NodeId, from: usize, to: usize, count: usize) {
        let kids = &mut rt.nodes[parent.index()].children;
        if from >= kids.len() || count == 0 {
            return;
        }
        let end = (from + count).min(kids.len());
        let moved: Vec<NodeId> = kids.drain(from..end).collect();
        // `to` 为删除段后新列表中的插入位置（Compose `move` 采用移动前坐标，
        // 这里按删除后坐标，等价于 Compose 先校正 to 再调用）。
        let target = to.min(kids.len());
        for (i, child) in moved.into_iter().enumerate() {
            kids.insert(target + i, child);
        }
    }

    fn clear(&self, rt: &mut RuntimeData, parent: NodeId) {
        // 与原 `take(children)` 语义一致：只摘父的 children，**保留子节点的 parent**。
        // GC 会沿 parent 向上把仍被 scope 锚定的旧子树拉回存活（见 sweep_nodes）；
        // 若在此置 parent=None，会误回收活节点（list_custom_item 选区回归）。
        let _ = std::mem::take(&mut rt.nodes[parent.index()].children);
    }
}

thread_local! {
    /// 当前 applier：默认 `ArenaApplier`；宿主可 `[with_applier]` 临时替换（对齐 Compose 多 applier）。
    static CURRENT: RefCell<Option<Box<dyn NodeApplier>>> = const { RefCell::new(None) };
}

/// 当前 applier 是否已被自定义替换。
pub fn is_custom() -> bool {
    CURRENT.with(|c| c.borrow().is_some())
}

/// 在自定义 applier 下执行 `f`；嵌套时恢复上一层（对齐 Compose `with(LocalApplier)`）。
pub fn with_applier<A: NodeApplier + 'static, R>(applier: A, f: impl FnOnce() -> R) -> R {
    CURRENT.with(|c| {
        let prev = c.borrow_mut().replace(Box::new(applier));
        let result = f();
        *c.borrow_mut() = prev;
        result
    })
}

/// 以当前 applier 执行树变更；默认走 `ArenaApplier`。
/// 回调用 `&mut RuntimeData`，调用方负责 `with_rt_pub` 的借用边界。
pub(crate) fn apply<R>(f: impl FnOnce(&dyn NodeApplier, &mut RuntimeData) -> R, rt: &mut RuntimeData) -> R {
    CURRENT.with(|c| {
        let borrowed = c.borrow();
        match borrowed.as_ref() {
            Some(a) => f(a.as_ref(), rt),
            None => f(&ArenaApplier, rt),
        }
    })
}

/// 把 `parent` 的第 `from` 个起 `count` 个节点移到删除后的插入位 `to`。
///
/// 对齐 Compose `Applier.move(from, to, count)`：由当前 applier 决定如何应用，
/// 默认走 `ArenaApplier`。越界静默忽略（不破坏树）。
pub fn move_node(parent: NodeId, from: usize, to: usize, count: usize) {
    crate::runtime::with_rt_pub(|rt| {
        apply(|ap, rt| ap.move_nodes(rt, parent, from, to, count), rt);
        rt.node_gc_pending = true;
    });
}

/// 节点树快照断言辅助（测试用）：返回 parent 的直接子节点序列。
pub fn children_of(parent: NodeId) -> Vec<NodeId> {
    crate::runtime::with_rt_pub(|rt| rt.nodes[parent.index()].children.clone())
}

/// 便捷：节点父句柄。
pub fn parent_of(node: NodeId) -> Option<NodeId> {
    crate::runtime::with_rt_pub(|rt| rt.nodes[node.index()].parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Alignment, Modifier, Policy, add_node, create_root_node};
    use crate::runtime::with_rt_pub;

    fn leaf() -> NodeId {
        // parent_stack 为空 → 独立节点（测试断言的是 applier 改写后的 parent）。
        add_node(Policy::Box(Alignment::TopStart), Modifier::new())
    }

    fn kids(p: NodeId) -> Vec<u32> {
        children_of(p).iter().map(|n| n.index() as u32).collect()
    }

    #[test]
    fn insert_bottom_up_appends_in_order() {
        let root = create_root_node(Alignment::TopStart);
        let (a, b, c) = (leaf(), leaf(), leaf());
        with_rt_pub(|rt| {
            apply(
                |ap, rt| {
                    ap.begin_changes(rt);
                    ap.insert_bottom_up(rt, root, 0, a);
                    ap.insert_bottom_up(rt, root, 1, b);
                    ap.insert_bottom_up(rt, root, 2, c);
                    ap.end_changes(rt);
                },
                rt,
            )
        });
        assert_eq!(kids(root), vec![a.index() as u32, b.index() as u32, c.index() as u32]);
        assert!(with_rt_pub(|rt| rt.nodes[b.index()].parent == Some(root)));
    }

    #[test]
    fn remove_detaches_and_clears_parent() {
        let root = create_root_node(Alignment::TopStart);
        let (a, b, c) = (leaf(), leaf(), leaf());
        with_rt_pub(|rt| {
            apply(
                |ap, rt| {
                    ap.insert_bottom_up(rt, root, 0, a);
                    ap.insert_bottom_up(rt, root, 1, b);
                    ap.insert_bottom_up(rt, root, 2, c);
                    ap.remove(rt, root, 1, 1);
                },
                rt,
            )
        });
        assert_eq!(kids(root), vec![a.index() as u32, c.index() as u32]);
        // 语义：从父 children 摘除；保留子 parent 供 GC 可达性判据使用。
        assert_eq!(parent_of(b), Some(root));
    }

    #[test]
    fn move_reorders_without_losing_nodes() {
        let root = create_root_node(Alignment::TopStart);
        let ids: Vec<_> = (0..4).map(|_| leaf()).collect();
        with_rt_pub(|rt| {
            apply(
                |ap, rt| {
                    for (i, id) in ids.iter().enumerate() {
                        ap.insert_bottom_up(rt, root, i, *id);
                    }
                    // [a,b,c,d] → 把 [b,c] 移到 d 之后（删除 b,c 后 d 在 index 1，插到其后 index 2）。
                    ap.move_nodes(rt, root, 1, 2, 2);
                },
                rt,
            )
        });
        let expect = vec![
            ids[0].index() as u32,
            ids[3].index() as u32,
            ids[1].index() as u32,
            ids[2].index() as u32,
        ];
        assert_eq!(kids(root), expect);
    }

    #[test]
    fn clear_detaches_all_children() {
        let root = create_root_node(Alignment::TopStart);
        let ids: Vec<_> = (0..3).map(|_| leaf()).collect();
        with_rt_pub(|rt| {
            apply(
                |ap, rt| {
                    for (i, id) in ids.iter().enumerate() {
                        ap.insert_bottom_up(rt, root, i, *id);
                    }
                    ap.clear(rt, root);
                },
                rt,
            )
        });
        assert!(kids(root).is_empty());
        // parent 字段保留（GC 安全网），仅从父 children 摘除。
        assert!(ids.iter().all(|id| parent_of(*id) == Some(root)));
    }

    #[test]
    fn move_node_public_api_reorders() {
        let root = create_root_node(Alignment::TopStart);
        let ids: Vec<_> = (0..3).map(|_| leaf()).collect();
        with_rt_pub(|rt| {
            apply(
                |ap, rt| {
                    for (i, id) in ids.iter().enumerate() {
                        ap.insert_bottom_up(rt, root, i, *id);
                    }
                },
                rt,
            )
        });
        // 用公开 API 把首节点移到末尾（删除后插入位 = 2）。
        move_node(root, 0, 2, 1);
        assert_eq!(
            kids(root),
            vec![
                ids[1].index() as u32,
                ids[2].index() as u32,
                ids[0].index() as u32,
            ]
        );
    }

    #[test]
    fn custom_applier_intercepts_insert() {
        use std::cell::Cell;
        thread_local! {
            static HITS: Cell<usize> = const { Cell::new(0) };
        }
        struct Counting;
        impl NodeApplier for Counting {
            fn insert_bottom_up(&self, rt: &mut RuntimeData, p: NodeId, i: usize, c: NodeId) {
                HITS.with(|h| h.set(h.get() + 1));
                ArenaApplier.insert_bottom_up(rt, p, i, c);
            }
            fn remove(&self, rt: &mut RuntimeData, p: NodeId, i: usize, n: usize) {
                ArenaApplier.remove(rt, p, i, n);
            }
            fn move_nodes(&self, rt: &mut RuntimeData, p: NodeId, f: usize, t: usize, n: usize) {
                ArenaApplier.move_nodes(rt, p, f, t, n);
            }
            fn clear(&self, rt: &mut RuntimeData, p: NodeId) {
                ArenaApplier.clear(rt, p);
            }
        }
        let root = create_root_node(Alignment::TopStart);
        with_applier(Counting, || {
            let a = leaf();
            with_rt_pub(|rt| apply(|ap, rt| ap.insert_bottom_up(rt, root, 0, a), rt));
        });
        assert_eq!(HITS.with(|h| h.get()), 1);
        assert!(!is_custom(), "嵌套后应恢复默认 applier");
    }
}
