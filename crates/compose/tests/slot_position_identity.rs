//! 回归护栏：`remember` 的「位置即身份」在 remember 与子作用域交替出现时必须稳定。
//!
//! 背景（story.rs `d_tabs` 越界 panic，index len=3 but index=4）：同一作用域内先发射
//! 一个子作用域（`page_hero` 的 Box），再 `tab = remember` → Row → Box → `tab2 = remember`
//! → Row → Box，两个 remember 同为 `MutableState<usize>`。引擎若用**同一个游标**
//! 同时驱动 `slots`（remember）与 `child_scopes`（子作用域复用），而两者各自 `push`
//! 增长，则游标与两个向量的下标会失配；下一轮重组 `remember` 读 `slots[cursor]` 时会
//! 命中相邻另一个**同类型**句柄（tab 拿到 tab2 的句柄）→ tab 读到 tab2 的越界值 4 → panic。
//!
//! 修复：`ScopeData` 用两个独立游标 `run_cursor`（remember/slots）与 `child_cursor`
//! （子作用域/child_scopes），各自与对应向量严格 1:1。本测试锁定该不变式。

use compose::foundation::Constraints;
use compose::layout::{create_root_node, Box, perform_layout, Modifier, Row};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{
    advance_frame, emit_keyed_scope, mutableStateOf, remember, Composition, MutableState,
};
use std::cell::Cell;
use std::rc::Rc;

thread_local! {
    static TAB: Cell<Option<MutableState<usize>>> = const { Cell::new(None) };
    static TAB2: Cell<Option<MutableState<usize>>> = const { Cell::new(None) };
    /// 键控作用域探针：分支状态句柄与分支 B 读到的值。
    static KEYED_PAGE: Cell<Option<MutableState<usize>>> = const { Cell::new(None) };
    static KEYED_SEEN: Cell<usize> = const { Cell::new(usize::MAX) };
}

#[test]
fn remember_identities_stable_across_child_scope_interleave() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);

    let seen_tab = Rc::new(Cell::new(usize::MAX));
    let seen_tab2 = Rc::new(Cell::new(usize::MAX));
    let tab_probe = seen_tab.clone();
    let tab2_probe = seen_tab2.clone();

    comp.set_content(move || {
        let ts = seen_tab.clone();
        let ts2 = seen_tab2.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let ts = ts.clone();
            let ts2 = ts2.clone();
            // 先发射一个子作用域，把 remember 游标推到 >0（对齐 page_body → page_hero）。
            Box(
                Modifier::new(),
                compose::layout::Alignment::TopStart,
                move || {},
            );
            // 对齐 d_tabs：tab=remember → Row → Box → tab2=remember → Row → Box。
            let tab = remember(|| mutableStateOf(0usize));
            TAB.with(|c| c.set(Some(tab)));
            Row(
                Modifier::new(),
                compose::layout::Arrangement::Start,
                compose::layout::CrossAxisAlignment::Center,
                move || {},
            );
            Box(
                Modifier::new(),
                compose::layout::Alignment::Center,
                move || {},
            );

            let tab2 = remember(|| mutableStateOf(0usize));
            TAB2.with(|c| c.set(Some(tab2)));
            Row(
                Modifier::new(),
                compose::layout::Arrangement::Start,
                compose::layout::CrossAxisAlignment::Center,
                move || {},
            );
            Box(
                Modifier::new(),
                compose::layout::Alignment::Center,
                move || {},
            );

            // 用 get() 注册依赖：tab / tab2 变化都会重组本作用域（触发跨轮游标复用）。
            ts.set(tab.get());
            ts2.set(tab2.get());
        });
    });

    perform_layout(root, Constraints::tight(400.0, 400.0));
    advance_frame(1.0 / 60.0);
    assert_eq!((tab_probe.get(), tab2_probe.get()), (0, 0), "首帧应都是 0");

    // 模拟把 tab2 设成越界值 4，触发本作用域重组。
    TAB2.with(|c| c.get().unwrap().set(4));
    advance_frame(1.0 / 60.0);
    assert!(
        tab_probe.get() < 3,
        "set(tab2=4) 后重组：tab 串位读到越界值 {}（复现 d_tabs panic）",
        tab_probe.get()
    );
    assert_eq!(tab2_probe.get(), 4, "tab2 应保持自身身份值 4");

    // 反复改 tab / tab2 触发定向重组，累积任何潜在漂移后仍须严格稳定。
    for round in 0..6 {
        let want_tab = round % 3; // 合法下标 0..2
        TAB.with(|c| c.get().unwrap().set(want_tab));
        advance_frame(1.0 / 60.0);
        assert_eq!(
            tab_probe.get(),
            want_tab,
            "round {round}: tab 身份不稳定，读到别的 remember 的值（{}）",
            tab_probe.get()
        );
    }

    // tab2 身份同样独立稳定。
    TAB2.with(|c| c.get().unwrap().set(7));
    advance_frame(1.0 / 60.0);
    assert_eq!(tab2_probe.get(), 7, "tab2 身份应稳定保持最新写入值");
    let _ = &comp;
}

/// 页面切换（同一位置渲染不同分支）时，普通位置复用会把上一分支的同类型
/// `MutableState<usize>` 继承给新分支 → 新分支的 3 标签数组越界 panic。
/// `emit_keyed_scope` 以 key 隔离：key 变化即清空 remember 槽，新分支重新初始化。
#[test]
fn keyed_scope_resets_remember_when_branch_changes() {
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = Composition::new(root);

    comp.set_content(move || {
        let page = remember(|| mutableStateOf(0usize));
        KEYED_PAGE.with(|c| c.set(Some(page)));
        let key = page.get();
        emit_keyed_scope(key as u64, move || {
            if key == 0 {
                // 分支 A：第一个 remember 槽存一个越界值 3。
                let _stale = remember(|| mutableStateOf(3usize));
            } else {
                // 分支 B：同样第一个 remember 槽；若继承 A 的状态就读到 3 → 越界 panic。
                let tab = remember(|| mutableStateOf(0usize));
                KEYED_SEEN.with(|c| c.set(tab.get()));
                let _ = ["a", "b", "c"][tab.get()];
            }
        });
    });

    perform_layout(root, Constraints::tight(400.0, 400.0));
    advance_frame(1.0 / 60.0);

    // 切到分支 B：keyed scope 必须重建，tab 读到自己的初始值 0。
    KEYED_PAGE.with(|c| c.get().unwrap().set(1));
    advance_frame(1.0 / 60.0);
    assert_eq!(
        KEYED_SEEN.with(|c| c.get()),
        0,
        "键控作用域未隔离：分支 B 继承上一分支的状态值（应为 0）"
    );

    // 再切回 A、再切回 B：来回切换不得残留对方状态。
    KEYED_PAGE.with(|c| c.get().unwrap().set(0));
    advance_frame(1.0 / 60.0);
    KEYED_PAGE.with(|c| c.get().unwrap().set(1));
    advance_frame(1.0 / 60.0);
    assert_eq!(KEYED_SEEN.with(|c| c.get()), 0, "往返切换后状态仍应独立");
    let _ = &comp;
}
