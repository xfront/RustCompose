//! 回归用例：TextField 输入文字后光标应仍在（且不因重组/闪烁而消失）。

use std::time::{Duration, Instant};

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, DrawCmd, Modifier, create_root_node, dispatch_click_at, draw_tree, hit_test,
    perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition, Recomposer, mutableStateOf};
use compose_material::{InputChar, TextField};

fn setup() -> (
    compose::layout::NodeId,
    compose::runtime::MutableState<String>,
) {
    let root = create_root_node(Alignment::TopStart);
    let value = mutableStateOf(String::new());
    let comp = Composition::new(root);
    let v2 = value;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            TextField(v2, "type…", Modifier::new().fill_max_width());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 200.0));
    (root, value)
}

/// 光标竖线（2 宽、primary 色）的 x；530ms 闪烁，轮询采样一个周期。
///
/// 轮询期间按生产帧循环泵帧：光标只在闪烁相位翻转时被失效重绘，
/// 不泵帧就可能在「灭」相位定格整段轮询（无头测试曾在相位恰好为灭时偶发失败）。
fn caret_x(root: compose::layout::NodeId, primary: Color) -> Option<f32> {
    let deadline = Instant::now() + Duration::from_millis(1300);
    loop {
        compose::runtime::advance_frame(1.0 / 60.0);
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::tight(400.0, 200.0));
        for c in draw_tree(root) {
            if let DrawCmd::FillRect { x, w, h, color, .. } = c {
                if (w - 2.0).abs() < 0.01 && h > 5.0 && color == primary {
                    return Some(x);
                }
            }
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(60));
    }
}

#[test]
fn caret_visible_and_follows_text() {
    let (root, value) = setup();
    let primary = ColorScheme::light().primary;

    let (t, _, _) = hit_test(root, 100.0, 22.0).expect("应命中输入框");
    assert!(dispatch_click_at(t, 100.0, 22.0), "点击应被消费");
    let empty = caret_x(root, primary);
    println!("caret (empty): {empty:?}");
    assert!(empty.is_some(), "聚焦后应显示光标");
    // 16dp 左内边距 + 1px 偏移
    assert!(
        (empty.unwrap() - 17.0).abs() < 1.0,
        "空文本时光标应在左内边距处：{empty:?}"
    );

    InputChar('h');
    InputChar('i');
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 200.0));
    assert_eq!(value.read_clone(), "hi");

    let filled = caret_x(root, primary);
    println!("caret (after \"hi\"): {filled:?}");
    assert!(
        filled.is_some(),
        "输入后光标仍应显示（不应只在重新点击后出现）"
    );
    assert!(
        filled.unwrap() > empty.unwrap(),
        "光标应随文本右移：empty={empty:?} filled={filled:?}"
    );
}
