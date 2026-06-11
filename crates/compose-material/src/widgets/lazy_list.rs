//! 真正的虚拟列表 — `LazyColumn` / `LazyRow`。
//!
//! 与 `ListView`（全量组合 + 视口平移）不同，本控件**只组合可见区间内的 item**：
//! 组合期根据滚动偏移算出 `[first, last]`，用 `emit_keyed_scope(index)` 发射这些项，
//! 前后各放一个 `Spacer` 占位（`first * item_extent` / `(count - last) * item_extent`），
//! 使滚动容器总尺寸恒为 `count * item_extent`，滚动条比例与_offset 钳制照常工作。
//!
//! 约束：item 高度/宽度**固定**为 `item_extent`（对齐 Flutter `itemExtent` 构造器）。
//! 引擎不能在 measure 期组合（RUNTIME RefCell 重入），故暂不支持可变高度 item 的
//! 按需组合；需要可变高度时请用 `ListView`（全量组合）。
//!
//! 选中语义：与 `List`/`ListView` 一致——传入 `MutableState<usize>`，点击整行
//! （含 padding/留白）即选中，选中行以 M3 `secondary_container` 高亮。
//!
//! 状态身份：每个可见 item 经 `emit_keyed_scope(index)` 发射，滚动离开再回来时
//! item 内部 `remember` 会重建（keyed 语义：视口外即 dispose），与 Jetpack Compose
//! `LazyColumn` 的「离开视口释放」一致。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::Color;
use compose::layout::{
    Arrangement, CrossAxisAlignment, Modifier, NodeId, Policy, Spacer, add_node,
    with_content_scope,
};
use compose::material::theme_colors;
use compose::runtime::{
    MutableState, add_frame_driver, disposable_effect, emit_keyed_scope, mutableStateOf, remember,
    remove_frame_driver, with_rt_pub,
};

use super::advanced::{HorizontalScroll, VerticalScroll};

/// 视口外额外组合的 item 数（减少快速滚动时的空白闪烁）。
const OVERSCAN: usize = 2;

/// 纵向虚拟列表。只组合可见项，item 高度固定 `item_extent`。
///
/// - `count`：item 总数（可非常大，如 100000）。
/// - `selected`：选中项索引；点击整行即 `set(index)`，选中行以 M3 `secondary_container` 高亮。
/// - `item_extent`：每项固定高度（含 padding）；占位与滚动条据此计算。
/// - `item`：按索引组合单个 item 内容，经 keyed scope 隔离状态。
pub fn LazyColumn(
    modifier: Modifier,
    count: usize,
    selected: MutableState<usize>,
    item_extent: f32,
    item: impl Fn(usize) + 'static,
) -> NodeId {
    lazy_list(true, modifier, count, selected, item_extent, item)
}

/// 横向虚拟列表。只组合可见项，item 宽度固定 `item_extent`，选中语义同 [`LazyColumn`]。
pub fn LazyRow(
    modifier: Modifier,
    count: usize,
    selected: MutableState<usize>,
    item_extent: f32,
    item: impl Fn(usize) + 'static,
) -> NodeId {
    lazy_list(false, modifier, count, selected, item_extent, item)
}

fn lazy_list(
    vertical: bool,
    modifier: Modifier,
    count: usize,
    selected: MutableState<usize>,
    item_extent: f32,
    item: impl Fn(usize) + 'static,
) -> NodeId {
    let extent = item_extent.max(1.0);
    let first_state = remember(|| mutableStateOf(0usize));
    let id_cell = remember(|| Rc::new(Cell::new(None::<NodeId>)));
    // content 会被重组反复调用（Fn），item 是一次性移入的闭包；用 Rc 共享以克隆。
    let item: Rc<dyn Fn(usize)> = Rc::new(item);
    let id_cell_for_content = id_cell.clone();

    let content = move || {
        let first = first_state.get();
        let start = first.saturating_sub(OVERSCAN);
        // viewport 项数：从真实滚动节点读容器尺寸（组合期已建节点，可安全 with_rt_pub）。
        let viewport_items = if let Some(id) = id_cell_for_content.get() {
            with_rt_pub(|rt| {
                let n = &rt.nodes[id.index()];
                let vp = if vertical { n.height } else { n.width };
                (vp / extent).ceil() as usize
            })
        } else {
            8 // 首帧节点尚未落位，先组合一批兜底
        };
        let visible = viewport_items.max(1) + OVERSCAN * 2;
        let end = (start + visible).min(count);

        // 前置占位：维持总高度，保证滚动条 / offset 钳制正确。
        if start > 0 {
            spacer_before(start, extent, vertical);
        }
        // 只组合可见区间内的 item，每个用 keyed scope 隔离状态。
        let sel_now = selected.get();
        for i in start..end {
            let item_c = item.clone();
            let sel_c = selected;
            emit_keyed_scope(i as u64, move || {
                let inner = item_c.clone();
                let colors = theme_colors();
                // 整行节点：M3 选中高亮 + on_click 设选中（点击 padding/留白亦归属本行）。
                let base = if vertical {
                    Modifier::new().fill_max_width().height(extent)
                } else {
                    Modifier::new().fill_max_height().width(extent)
                };
                let row = add_node(
                    compose::layout::Policy::Box(if vertical {
                        compose::layout::Alignment::TopStart
                    } else {
                        compose::layout::Alignment::CenterStart
                    }),
                    base.background(
                        if i == sel_now {
                            colors.secondary_container
                        } else {
                            compose::foundation::Color::TRANSPARENT
                        },
                        0.0,
                    ),
                );
                with_rt_pub(|rt| {
                    rt.nodes[row.index()].on_click = Some(Box::new(move || sel_c.set(i)));
                });
                with_content_scope(row, move || inner(i));
            });
        }
        if end < count {
            spacer_after(count - end, extent, vertical);
        }
    };

    let id = if vertical {
        VerticalScroll(modifier, Arrangement::Start, CrossAxisAlignment::Stretch, content)
    } else {
        HorizontalScroll(modifier, Arrangement::Start, CrossAxisAlignment::Center, content)
    };
    id_cell.set(Some(id));

    // 帧驱动：从真实滚动节点同步 offset → 计算 first，仅在变化时 set（相同值 state 会跳过重组）。
    let first_c = first_state;
    let id_c = id_cell.clone();
    let drv = remember(|| {
        add_frame_driver(move || {
            let Some(id) = id_c.get() else { return };
            let offset = with_rt_pub(|rt| {
                rt.nodes[id.index()]
                    .scroll
                    .as_ref()
                    .map(|s| s.offset)
                    .unwrap_or(0.0)
            });
            let first = (offset / extent) as usize;
            first_c.set(first);
        })
    });
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));

    id
}

fn spacer_before(n: usize, extent: f32, vertical: bool) {
    let m = if vertical {
        Modifier::new().fill_max_width().height(n as f32 * extent)
    } else {
        Modifier::new().fill_max_height().width(n as f32 * extent)
    };
    Spacer(m);
}

fn spacer_after(n: usize, extent: f32, vertical: bool) {
    spacer_before(n, extent, vertical)
}
