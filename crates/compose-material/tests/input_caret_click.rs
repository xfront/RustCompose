//! 回归用例：输入框点击可定位光标到任意字符位置。

use std::time::{Duration, Instant};

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, DrawCmd, Modifier, create_root_node, dispatch_click_at, draw_tree, hit_test,
    perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition, Recomposer, advance_frame, mutableStateOf};
use compose_material::{InputBackspace, InputChar, TextField};

fn setup(
    text: &str,
) -> (
    compose::layout::NodeId,
    compose::runtime::MutableState<String>,
) {
    let root = create_root_node(Alignment::TopStart);
    let value = mutableStateOf(text.to_string());
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

/// 光标竖线（2 宽、primary 色）的 x；530ms 闪烁，轮询一个周期。
///
/// 每轮先 `advance_frame` 泵帧驱动：聚焦字段的 `caret_blink_driver` 会把该节点连同祖先链
/// 标为绘制失效，绘制闭包才会重跑并重新采样闪烁相位——与生产帧循环同一通路。无头测试若
/// 不泵帧，增量绘制缓存会把首帧采到的相位冻结（亮/灭全看第一次采样撞到哪一半）。
fn caret_x(root: compose::layout::NodeId, primary: Color) -> Option<f32> {
    let deadline = Instant::now() + Duration::from_millis(1300);
    loop {
        advance_frame(1.0 / 60.0);
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

fn click_at(root: compose::layout::NodeId, x: f32) {
    let (t, _, _) = hit_test(root, x, 22.0).expect("应命中输入框");
    assert!(dispatch_click_at(t, x, 22.0), "点击应被消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 200.0));
}

#[test]
fn click_positions_caret_between_chars() {
    let (root, value) = setup("abcd");
    let primary = ColorScheme::light().primary;

    // 点击最左（'a' 之前）→ 光标索引 0
    click_at(root, 14.0);
    let at0 = caret_x(root, primary);
    // 点击靠右（末尾）→ 光标索引 4
    click_at(root, 200.0);
    let at_end = caret_x(root, primary);
    println!("caret@0={at0:?} caret@end={at_end:?}");
    assert!(at0.is_some() && at_end.is_some(), "两处都应显示光标");
    assert!(
        at0.unwrap() < at_end.unwrap() - 8.0,
        "点击左侧时光标应更靠左"
    );

    // 点击中间（约第 2 个字符后）→ 输入应插入该处而非末尾
    // 16dp 左内边距 + `bodyLarge` 16px（启发式字宽 ≈ 8.8）⇒ 第 2 字边界落在 ~33.6
    click_at(root, 34.0);
    InputChar('X');
    Recomposer::recompose_pending();
    let s = value.read_clone();
    println!("after insert: {s:?}");
    assert_ne!(s, "abcdX", "不应追加到末尾");
    assert!(
        s.starts_with("ab") && s.contains('X'),
        "X 应插在点击处：{s:?}"
    );

    // 退格删除光标前一位（即刚插入的 X）
    InputBackspace();
    Recomposer::recompose_pending();
    assert_eq!(value.read_clone(), "abcd", "退格应删除光标前一位");
}
