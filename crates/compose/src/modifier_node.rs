//! Modifier.Node — 对齐 Jetpack Compose `Modifier.Node` / `ModifierNodeElement`。
//!
//! Compose 的 Modifier 不是被动值链，而是一条**有身份、有生命周期**的节点链：
//! 每个 `ModifierNodeElement.create()` 产生一个 `Modifier.Node`，重组时 `update()`
//! 就地更新、`detach()` 时释放。本工程零 `Rc`/`unsafe`，故节点存中央 arena
//! （`MODIFIER_NODES`，句柄 `ModifierNodeId`），`Modifier` 链通过 `ModElem::Node`
//! 持有句柄——与 `NodeId` / `ScopeId` 的 arena 模型一致。
//!
//! 节点按 `kind()` 参与阶段：
//! - `Measure`：在 `apply_measure_mods` 中按链序收紧/放宽 `Constraints`；
//! - `Draw`：在 `draw_node` 的自定义节点趟中按链序追加绘制命令；
//! - `Hit`：在 `hit_test` 中参与命中覆盖（本阶段先提供接口，宿主可查询）。
//!
//! 内置一个对齐 Compose `padding` 包裹语义的 [`DeltaNode`]：测量时按四边增减约束，
//! 演示 `ModifierNodeElement.create/update` 与链序（后写覆盖先写）。

use std::any::Any;
use std::cell::RefCell;

use crate::foundation::Constraints;
use crate::layout::{DrawBuilder, Modifier};

/// 自定义 modifier 节点句柄（arena 索引）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModifierNodeId(pub u32);

/// 节点参与的阶段（对齐 Compose `ModifierNode` 的 NodeKind）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierNodeKind {
    Measure,
    Draw,
    Hit,
}

/// Modifier.Node 实现：按阶段提供钩子；`update` 用类型擦除值就地更新。
pub trait ModifierNodeImpl {
    fn kind(&self) -> ModifierNodeKind;
    /// 重组时就地更新（对齐 `ModifierNodeElement.update`）。
    fn update(&mut self, _payload: &dyn Any) {}
    /// 测量阶段改写约束（按链序调用）。
    fn on_measure(&self, incoming: Constraints) -> Constraints {
        incoming
    }
    /// 绘制阶段在节点自身内容之前追加命令。
    fn on_draw(&self, _b: &mut DrawBuilder, _x: f32, _y: f32, _w: f32, _h: f32) {}
    /// 身份探测（用于 `update` 前的 downcast）。
    fn as_any(&self) -> &dyn Any;
}

struct Slot {
    node: Box<dyn ModifierNodeImpl>,
    alive: bool,
}

thread_local! {
    static NODES: RefCell<Vec<Slot>> = const { RefCell::new(Vec::new()) };
}

/// 分配一个节点槽。
pub(crate) fn alloc(node: Box<dyn ModifierNodeImpl>) -> ModifierNodeId {
    NODES.with(|n| {
        let mut n = n.borrow_mut();
        // 优先复用释放槽。
        if let Some(i) = n.iter().position(|s| !s.alive) {
            n[i] = Slot { node, alive: true };
            ModifierNodeId(i as u32)
        } else {
            let id = ModifierNodeId(n.len() as u32);
            n.push(Slot { node, alive: true });
            id
        }
    })
}

/// 就地更新节点（组合期同一身份复用到新参数）。
pub fn update(id: ModifierNodeId, payload: &dyn Any) {
    NODES.with(|n| {
        if let Some(s) = n.borrow_mut().get_mut(id.0 as usize) {
            if s.alive {
                s.node.update(payload);
            }
        }
    });
}

/// 释放节点（作用域销毁 / 元素消失）。
pub fn detach(id: ModifierNodeId) {
    NODES.with(|n| {
        if let Some(s) = n.borrow_mut().get_mut(id.0 as usize) {
            s.alive = false;
        }
    });
}

/// 对单个节点执行测量钩子（供 layout 的 ModElem 链按序调用）。
pub(crate) fn measure_one(id: ModifierNodeId, incoming: Constraints) -> Constraints {
    NODES.with(|n| {
        let n = n.borrow();
        match n.get(id.0 as usize) {
            Some(s) if s.alive && s.node.kind() == ModifierNodeKind::Measure => {
                s.node.on_measure(incoming)
            }
            _ => incoming,
        }
    })
}

/// 按链序测量：依次应用所有 Measure 节点。
pub(crate) fn measure_chain(ids: &[ModifierNodeId], incoming: Constraints) -> Constraints {
    let mut c = incoming;
    NODES.with(|n| {
        let n = n.borrow();
        for id in ids {
            if let Some(s) = n.get(id.0 as usize) {
                if s.alive && s.node.kind() == ModifierNodeKind::Measure {
                    c = s.node.on_measure(c);
                }
            }
        }
    });
    c
}

/// 按链序绘制：依次调用所有 Draw 节点（节点自身内容之前）。
pub(crate) fn draw_chain(
    ids: &[ModifierNodeId],
    b: &mut DrawBuilder,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    NODES.with(|n| {
        let n = n.borrow();
        for id in ids {
            if let Some(s) = n.get(id.0 as usize) {
                if s.alive && s.node.kind() == ModifierNodeKind::Draw {
                    s.node.on_draw(b, x, y, w, h);
                }
            }
        }
    });
}

/// 收集链中某 kind 的节点句柄（命中阶段宿主查询）。
pub fn nodes_of_kind(ids: &[ModifierNodeId], kind: ModifierNodeKind) -> Vec<ModifierNodeId> {
    NODES.with(|n| {
        let n = n.borrow();
        ids.iter()
            .copied()
            .filter(|id| {
                n.get(id.0 as usize)
                    .map(|s| s.alive && s.node.kind() == kind)
                    .unwrap_or(false)
            })
            .collect()
    })
}

// ── 内置示例节点：padding 包裹（对齐 Compose padding modifier node） ────────

/// 测量时按四边增减约束的节点，演示 create/update 生命周期与链序。
pub struct DeltaNode {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl DeltaNode {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

impl ModifierNodeImpl for DeltaNode {
    fn kind(&self) -> ModifierNodeKind {
        ModifierNodeKind::Measure
    }
    fn update(&mut self, payload: &dyn Any) {
        if let Some((l, t, r, b)) = payload.downcast_ref::<(f32, f32, f32, f32)>() {
            self.left = *l;
            self.top = *t;
            self.right = *r;
            self.bottom = *b;
        }
    }
    fn on_measure(&self, incoming: Constraints) -> Constraints {
        // 水平/垂直各扣除 padding，结果与现有 padding_of 语义一致（求和覆盖）。
        let h = self.left + self.right;
        let v = self.top + self.bottom;
        Constraints::new(
            (incoming.min_width - h).max(0.0),
            (incoming.max_width - h).max(0.0),
            (incoming.min_height - v).max(0.0),
            (incoming.max_height - v).max(0.0),
        )
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// 便捷：构造一个 DeltaNode 并追加到 Modifier 链（对齐 Compose `Modifier.padding` 的 node 版）。
pub fn delta_modifier(
    modifier: Modifier,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> Modifier {
    let id = alloc(Box::new(DeltaNode::new(left, top, right, bottom)));
    modifier.with_node(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::Constraints;

    #[test]
    fn node_measure_chain_applies_in_order() {
        let m1 = delta_modifier(Modifier::new(), 10.0, 0.0, 10.0, 0.0);
        let ids = m1.node_ids();
        let out = measure_chain(&ids, Constraints::tight(100.0, 50.0));
        assert_eq!(out.max_width, 80.0);
    }

    #[test]
    fn node_update_in_place_preserves_handle() {
        let id = alloc(Box::new(DeltaNode::new(1.0, 0.0, 1.0, 0.0)));
        let m = Modifier::new().with_node(id);
        update(id, &(5.0f32, 0.0f32, 5.0f32, 0.0f32));
        let out = measure_chain(&m.node_ids(), Constraints::tight(100.0, 50.0));
        assert_eq!(out.max_width, 90.0, "update 应就地改节点参数");
    }

    #[test]
    fn detach_removes_node_from_chain() {
        let id = alloc(Box::new(DeltaNode::new(10.0, 0.0, 10.0, 0.0)));
        let m = Modifier::new().with_node(id);
        detach(id);
        let out = measure_chain(&m.node_ids(), Constraints::tight(100.0, 50.0));
        assert_eq!(out.max_width, 100.0, "detach 后节点不参与测量");
    }
}
