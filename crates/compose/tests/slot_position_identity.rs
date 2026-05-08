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
use compose::runtime::{advance_frame, mutableStateOf, remember, Composition, MutableState};
use std::cell::Cell;
use std::rc::Rc;

thread_local! {
    static TAB: Cell<Option<MutableState<usize>>> = const { Cell::new(None) };
    static TAB2: Cell<Option<MutableState<usize>>> = const { Cell::new(None) };
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
