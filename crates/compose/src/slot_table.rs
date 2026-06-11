//! SlotTable — 对标 Jetpack Compose `SlotTable` 的 gap-buffer 槽存储。
//!
//! Compose 把 group 标记与 data slot 交错存进**一个**线性数组，用一段可移动的
//! 「空隙」(gap) 承接条件分支的插入/删除：插入即吞掉 gap 一端，删除即扩大 gap，
//! 只有跨越 gap 时才批量搬移数据（`arraycopy`）。位置即身份：重组时按顺序走
//! group，key 匹配则复用原槽，不匹配则删除旧 group 并插入新的。
//!
//! 本模块实现这一数据结构本体（不依赖 runtime），供 `ScopeData.slots` 接入：
//! - [`SlotTable`]：线性槽数组 + gap + group 树 + 游标遍历。
//! - [`Anchor`]：稳定槽引用，gap 移动 / group 增删后仍指向同一逻辑槽。
//! - [`GroupKey`]：group 身份（数值或对象键的 u64 句柄），对应 Compose `Key`。
//!
//! 注意：`Box<dyn Any>` 不可 `Clone`，gap 搬移全部用原地 `rotate` / `swap`，
//! 不做克隆；这与 Compose 在 `Array<Any?>` 上做 `arraycopy` 等价。

use std::any::Any;

/// group 身份键（对齐 Compose `GroupKey`：编译期调用点 key / 显式 key）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    /// 位置组：由调用序号隐式给出（Compose 的默认 call-site key）。
    Position(u64),
    /// 显式 key（`emit_keyed_scope` 的 u64）。
    Explicit(u64),
}

/// 一个线性槽：group 头 / data 槽 / gap 占位。
enum Slot {
    /// group 起始标记；`data_size` 是本 group 直接持有的 data 槽数。
    Group {
        key: GroupKey,
        data_size: usize,
        active: bool,
    },
    /// 一个 remember 值（类型擦除）。
    Data(Box<dyn Any>),
    /// gap 占位（逻辑上不存在，遍历时跳过）。
    Gap,
}

/// 稳定槽锚点：记录**逻辑 data 下标**（不含 gap / group），由 [`SlotTable::resolve`] 转物理。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Anchor {
    logical: usize,
    generation: u32,
}

/// 线性 gap-buffer 槽表。
pub struct SlotTable {
    data: Vec<Slot>,
    /// `[gap_start, gap_end)` 为 gap；两者恒满足 `0 <= gap_start <= gap_end <= data.len()`。
    gap_start: usize,
    gap_end: usize,
    /// 结构变更计数：变更即使锚点失效（对齐 Compose 删除 group 时 Anchor 失效）。
    generation: u32,
}

impl Default for SlotTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotTable {
    /// 空表（gap 为空，首次插入时按需扩容）。
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            gap_start: 0,
            gap_end: 0,
            generation: 0,
        }
    }

    /// 逻辑 data 槽数（不含 gap / group 头）。
    pub fn data_len(&self) -> usize {
        self.data
            .iter()
            .filter(|s| matches!(s, Slot::Data(_)))
            .count()
    }

    /// 物理数组长度（含 gap 与 group 头），调试 / 单测用。
    pub fn physical_len(&self) -> usize {
        self.data.len()
    }

    /// 当前 gap 的物理区间 `[start, end)`。
    pub fn gap(&self) -> (usize, usize) {
        (self.gap_start, self.gap_end)
    }

    /// 确保 gap 至少 `n` 格：不够时在 gap 位置插入 `Slot::Gap` 扩容。
    fn ensure_gap(&mut self, n: usize) {
        let have = self.gap_end - self.gap_start;
        if have < n {
            for _ in 0..(n - have) {
                self.data.insert(self.gap_start, Slot::Gap);
            }
            self.gap_end = self.gap_start + n;
        }
    }

    /// 把 gap 搬到物理下标 `index`：原地 rotate，不 Clone 槽内容。
    ///
    /// - `index <= gap_start`：把 `[index, gap_start)` 数据右移到 gap 之后。
    /// - `index >= gap_end`：把 `[gap_end, index)` 数据左移到 gap 之前。
    pub fn move_gap_to(&mut self, index: usize) {
        let index = index.min(self.data.len());
        if index < self.gap_start {
            let size = self.gap_end - self.gap_start;
            self.data[index..self.gap_end].rotate_right(size);
            self.gap_start = index;
            self.gap_end = index + size;
        } else if index > self.gap_end {
            let size = self.gap_end - self.gap_start;
            self.data[self.gap_start..index].rotate_left(size);
            self.gap_start = index - size;
            self.gap_end = index;
        }
    }

    /// 第 `logical` 个 data 槽的物理下标；越界（`logical == data_len`）返回插入位（gap 起点）。
    fn data_physical(&self, logical: usize) -> Option<usize> {
        let mut count = 0usize;
        let mut i = 0usize;
        while i < self.data.len() {
            match self.data[i] {
                Slot::Data(_) => {
                    if count == logical {
                        return Some(i);
                    }
                    count += 1;
                    i += 1;
                }
                Slot::Group { .. } => i += 1,
                Slot::Gap => i = self.gap_end,
            }
        }
        if count == logical {
            Some(self.gap_start)
        } else {
            None
        }
    }

    /// 吞掉 gap 一端写入一个槽，返回写入的物理位置。
    fn write_at_gap(&mut self, slot: Slot, at_end: bool) -> usize {
        self.ensure_gap(1);
        let p = if at_end { self.gap_end - 1 } else { self.gap_start };
        self.data[p] = slot;
        if at_end {
            self.gap_end -= 1;
        } else {
            self.gap_start += 1;
        }
        self.generation = self.generation.wrapping_add(1);
        p
    }

    /// 在逻辑 data 位置 `at` 插入 data 槽；`at == data_len()` 追加。
    pub fn insert_data(&mut self, at: usize, value: Box<dyn Any>) {
        let p = self.data_physical(at).unwrap_or(self.gap_start);
        self.move_gap_to(p);
        self.write_at_gap(Slot::Data(value), false);
    }

    /// 覆盖第 `logical` 个 data 槽；越界视为插入。
    pub fn set_data(&mut self, logical: usize, value: Box<dyn Any>) {
        let p = self.data_physical(logical).unwrap_or(self.gap_start);
        if p < self.data.len() && matches!(self.data[p], Slot::Data(_)) {
            self.data[p] = Slot::Data(value);
        } else {
            self.insert_data(logical, value);
        }
    }

    /// 读取第 `logical` 个 data 槽。
    pub fn get_data(&self, logical: usize) -> Option<&Box<dyn Any>> {
        let p = self.data_physical(logical)?;
        match self.data.get(p) {
            Some(Slot::Data(v)) => Some(v),
            _ => None,
        }
    }

    /// 删除第 `logical` 个 data 槽：把 gap 移过来圈住该槽（逻辑删除，物理槽复用）。
    pub fn remove_data(&mut self, logical: usize) -> bool {
        let Some(p) = self.data_physical(logical) else {
            return false;
        };
        if p >= self.data.len() || !matches!(self.data[p], Slot::Data(_)) {
            return false;
        }
        // 把 gap 右端搬到 p+1，再让 gap 左端吞掉 p：等价于删除该槽并扩大 gap。
        self.move_gap_to(p + 1);
        self.gap_start -= 1;
        self.data[self.gap_start] = Slot::Gap;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// 在逻辑位置 `at` 插入 group 头，返回其物理下标。
    pub fn start_group(&mut self, at: usize, key: GroupKey) -> usize {
        let p = self.data_physical(at).unwrap_or(self.gap_start);
        self.move_gap_to(p);
        self.write_at_gap(
            Slot::Group {
                key,
                data_size: 0,
                active: true,
            },
            false,
        )
    }

    /// 结束 group：记录其直接 data 槽数。
    pub fn end_group(&mut self, group_physical: usize, data_size: usize) {
        if let Some(Slot::Group { data_size: s, .. }) = self.data.get_mut(group_physical) {
            *s = data_size;
        }
    }

    /// 查第 `at` 个 active group 的 key（按物理出现顺序，跳过 gap / data）。
    pub fn group_key_at(&self, at: usize) -> Option<GroupKey> {
        let mut count = 0usize;
        for slot in &self.data {
            if let Slot::Group { key, active: true, .. } = slot {
                if count == at {
                    return Some(*key);
                }
                count += 1;
            }
        }
        None
    }

    /// 回收第 `keep` 个之后的 data 槽到 gap（条件组合分支消失时调用，
    /// 等价 Compose `SlotWriter.removeCurrentGroup` 把退出组合的槽圈回 gap 复用）。
    pub fn truncate_data(&mut self, keep: usize) {
        while self.data_len() > keep {
            let last = self.data_len() - 1;
            self.remove_data(last);
        }
    }

    /// 创建指向第 `logical` 个 data 槽的锚点。
    pub fn anchor(&self, logical: usize) -> Anchor {
        Anchor {
            logical,
            generation: self.generation,
        }
    }

    /// 解析锚点到物理下标；结构变更后返回 `None`（对齐 Compose 失效锚点）。
    pub fn resolve(&self, a: Anchor) -> Option<usize> {
        if a.generation != self.generation {
            return None;
        }
        self.data_physical(a.logical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Box<dyn Any> {
        Box::new(v.to_string())
    }
    fn get(table: &SlotTable, i: usize) -> &str {
        table
            .get_data(i)
            .and_then(|b| b.downcast_ref::<String>())
            .map(|x| x.as_str())
            .unwrap_or("<none>")
    }

    #[test]
    fn append_and_read_stable() {
        let mut t = SlotTable::new();
        t.insert_data(0, s("a"));
        t.insert_data(1, s("b"));
        t.insert_data(2, s("c"));
        assert_eq!(t.data_len(), 3);
        assert_eq!(get(&t, 0), "a");
        assert_eq!(get(&t, 1), "b");
        assert_eq!(get(&t, 2), "c");
    }

    #[test]
    fn gap_shrinks_on_insert_and_grows_on_remove() {
        let mut t = SlotTable::new();
        t.insert_data(0, s("x"));
        let before_remove = t.physical_len();
        assert!(t.remove_data(0));
        assert_eq!(t.data_len(), 0);
        assert_eq!(t.physical_len(), before_remove, "删除应把槽圈回 gap 不缩数组");
        t.insert_data(0, s("y"));
        assert_eq!(t.physical_len(), before_remove, "gap 复用避免扩容");
        assert_eq!(get(&t, 0), "y");
    }

    #[test]
    fn insert_middle_moves_gap_not_all_data() {
        let mut t = SlotTable::new();
        for (i, v) in ["a", "b", "c"].iter().enumerate() {
            t.insert_data(i, s(v));
        }
        t.insert_data(1, s("X"));
        assert_eq!(get(&t, 0), "a");
        assert_eq!(get(&t, 1), "X");
        assert_eq!(get(&t, 2), "b");
        assert_eq!(get(&t, 3), "c");
    }

    #[test]
    fn group_keys_and_reuse_probe() {
        let mut t = SlotTable::new();
        let g = t.start_group(0, GroupKey::Explicit(7));
        t.insert_data(0, s("inside"));
        t.end_group(g, 1);
        assert_eq!(t.group_key_at(0), Some(GroupKey::Explicit(7)));
        assert_eq!(get(&t, 0), "inside", "group 头不应占 data 逻辑下标");
    }

    #[test]
    fn anchor_invalidates_after_mutation() {
        let mut t = SlotTable::new();
        t.insert_data(0, s("a"));
        let a = t.anchor(0);
        assert_eq!(t.resolve(a), Some(0));
        t.insert_data(0, s("b"));
        assert_eq!(t.resolve(a), None, "结构变化后锚点应失效");
    }

    #[test]
    fn truncate_recycles_slots_into_gap() {
        let mut t = SlotTable::new();
        for (i, v) in ["a", "b", "c", "d"].iter().enumerate() {
            t.insert_data(i, s(v));
        }
        let capacity = t.physical_len();
        t.truncate_data(2); // 条件分支退出：丢弃 c/d
        assert_eq!(t.data_len(), 2);
        assert_eq!(get(&t, 1), "b");
        // 分支重新进入：复用 gap 中的槽，不增长物理数组（Compose gap buffer 核心收益）。
        t.insert_data(2, s("c2"));
        t.insert_data(3, s("d2"));
        assert_eq!(t.physical_len(), capacity);
        assert_eq!(get(&t, 2), "c2");
        assert_eq!(get(&t, 3), "d2");
    }

    #[test]
    fn gap_move_preserves_all_data() {
        let mut t = SlotTable::new();
        for (i, v) in ["a", "b", "c", "d"].iter().enumerate() {
            t.insert_data(i, s(v));
        }
        t.move_gap_to(0);
        assert_eq!((get(&t, 0), get(&t, 3)), ("a", "d"));
        t.move_gap_to(t.physical_len());
        assert_eq!((get(&t, 0), get(&t, 3)), ("a", "d"));
        assert_eq!(t.data_len(), 4);
    }
}
