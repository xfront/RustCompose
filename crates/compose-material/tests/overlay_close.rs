//! BottomSheet / Snackbar 关闭路径回归：
//! 1) 点击 scrim 应关闭 BottomSheet；
//! 2) 点击 Snackbar 本体应关闭（dismiss）。
//!
//! 曾共同失败的根因（引擎级）：`hit_node` 对所有容器按 bounds 剪枝，而
//! `Modifier.offset` 不参与测量、落位可越出父 bounds ⇒ offset 定位的浮层
//! （snackbar 挂在视口底部、对话框动画位移）永远点不到。修复 = 只有
//! clip/滚动容器才按 bounds 剪枝（对齐 Compose：无 clip 不裁命中）。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Modifier, TextStyle, create_root_node, dispatch_click_at, hit_test, perform_layout,
    set_viewport,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composition, Recomposer, advance_frame, mutableStateOf, remember};
use compose_material::{BottomSheet, OverlayHost, SnackbarHost, RememberSnackbarHostState, Text};

fn pump(root: compose::layout::NodeId) {
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 600.0));
}

#[test]
fn bottom_sheet_closes_on_scrim_click() {
    set_viewport(400.0, 600.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let open_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let s = open_slot.clone();
    comp.set_content(move || {
        let s = s.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let open = remember(|| mutableStateOf(false));
            *s.borrow_mut() = Some(open);
            OverlayHost(Modifier::new().fill_max_size(), move || {
                BottomSheet(open, 160.0, || {
                    Text("sheet body", TextStyle::new(), Modifier::new());
                });
            });
        });
    });
    pump(root);
    let open = open_slot.borrow().clone().unwrap();
    assert!(!open.get(), "初始应关闭");

    // 打开
    open.set(true);
    pump(root);
    assert!(open.get(), "应处于打开态");

    // 点击 scrim（左上角，避开底部 sheet）
    let (target, _, _) = hit_test(root, 10.0, 10.0).expect("scrim 应可命中");
    assert!(dispatch_click_at(target, 10.0, 10.0), "scrim 点击应被消费");
    pump(root);
    assert!(!open.get(), "点击 scrim 应回关闭 BottomSheet");
}

#[test]
fn snackbar_click_dismisses() {
    set_viewport(400.0, 600.0);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let host_slot: std::rc::Rc<std::cell::RefCell<Option<compose_material::SnackbarHostState>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let s = host_slot.clone();
    comp.set_content(move || {
        let s = s.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let host = RememberSnackbarHostState();
            *s.borrow_mut() = Some(host);
            OverlayHost(Modifier::new().fill_max_size(), move || {
                SnackbarHost(host);
            });
        });
    });
    pump(root);
    let host = host_slot.borrow().clone().unwrap();

    host.show("hello");
    pump(root);
    assert!(host.is_visible(), "显示后应可见");

    // 泵帧让入场动画收敛（t→1）：条子滑入视口内 y = vh - SNACKBAR_BOTTOM(88)。
    for _ in 0..60 {
        advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(400.0, 600.0));
    }
    // 位于底部：x=24(SPACE_XXL) 起，宽 = vw - 2*24，高 44。
    let (mx, my) = (24.0 + 100.0, 600.0 - 88.0 + 16.0);
    let (target, _, _) = hit_test(root, mx, my).expect("snackbar 应可命中");
    assert!(dispatch_click_at(target, mx, my), "snackbar 点击应被消费");
    pump(root);
    assert!(!host.is_visible(), "点击 snackbar 应回关闭");
}
