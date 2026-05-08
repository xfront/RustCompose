//! 回归用例：OTP 光标与定位输入。
//! - 输入一位后光标自动前移；
//! - 点击已输入的格子可定位改写该位；
//! - 退格删除选中位。
//!
//! 焦点格通过「主色边框」判定（每帧 draw 闭包实时刷新，稳定且不受光标闪烁影响）。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, DrawCmd, Modifier, create_root_node, dispatch_click_at, draw_tree, hit_test,
    perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition, Recomposer, mutableStateOf};
use compose_material::{InputBackspace, InputChar, OtpInput};

fn setup() -> (
    compose::layout::NodeId,
    compose::runtime::MutableState<String>,
) {
    let root = create_root_node(Alignment::TopStart);
    let code = mutableStateOf(String::new());
    let c2 = code;
    Composition::new(root).set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OtpInput(c2, 4, Modifier::new());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 200.0));
    (root, code)
}

/// 第 i 格中心 x（格子 38 宽 + 8 间距，行从 x=0 起）。
fn cell_center(i: usize) -> f32 {
    i as f32 * 46.0 + 19.0
}

/// 通过主色边框（line≈2）判定聚焦格下标。
fn focused_box(root: compose::layout::NodeId, primary: compose::foundation::Color) -> usize {
    for c in draw_tree(root) {
        if let DrawCmd::StrokeRRect { x, color, line, .. } = c {
            if color == primary && (line - 2.0).abs() < 0.5 {
                return (x / 46.0).round() as usize;
            }
        }
    }
    panic!("应存在主色边框的聚焦格");
}

fn click_cell(root: compose::layout::NodeId, i: usize) {
    let x = cell_center(i);
    let (target, _, _) = hit_test(root, x, 30.0).expect("应命中格子");
    assert!(dispatch_click_at(target, x, 30.0), "点击应被消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 200.0));
}

fn recomp(root: compose::layout::NodeId) {
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 200.0));
}

#[test]
fn otp_caret_advances_and_supports_positional_edit() {
    let (root, code) = setup();
    let primary = ColorScheme::light().primary;

    click_cell(root, 0);
    assert_eq!(focused_box(root, primary), 0);

    // 输入一位 → 光标自动前移到第二格
    InputChar('7');
    recomp(root);
    assert_eq!(code.read_clone(), "7");
    assert_eq!(focused_box(root, primary), 1);

    // 再输两位，共 3 位
    InputChar('8');
    InputChar('9');
    recomp(root);
    assert_eq!(code.read_clone(), "789");

    // 点击第 1 格（已有 '7'）→ 定位改写
    click_cell(root, 0);
    assert_eq!(focused_box(root, primary), 0);
    InputChar('1');
    recomp(root);
    assert_eq!(code.read_clone(), "189", "应改写第 1 位而非追加");

    // 退格：删除选中位（此时焦点已前移到第 1 格 '8'）
    InputBackspace();
    recomp(root);
    assert_eq!(code.read_clone(), "19", "退格应删除选中位");
}
