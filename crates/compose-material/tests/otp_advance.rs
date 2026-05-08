//! 诊断：OTP 输入自动前进的行为。

use compose::foundation::{Color, Constraints};
use compose::layout::{
    Alignment, DrawCmd, Modifier, create_root_node, dispatch_click_at, draw_tree, hit_test,
    perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{
    Composer, Composition, NavKey, Recomposer, advance_frame, mutableStateOf,
};
use compose_material::{InputBackspace, InputChar, InputKey, OtpInput};

fn setup(
    len: usize,
) -> (
    compose::layout::NodeId,
    compose::runtime::MutableState<String>,
) {
    let root = create_root_node(Alignment::TopStart);
    let code = mutableStateOf(String::new());
    let comp = Composition::new(root);
    let c2 = code;
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OtpInput(c2, len, Modifier::new());
        });
    });
    perform_layout(root, Constraints::tight(400.0, 120.0));
    (root, code)
}

/// 取出全部格子边框（按 x 排序）：`(x, 宽, 是否主色高亮)`。
///
/// 读屏前先泵一帧：OTP 的焦点格是非响应式 `Rc<Cell>`（边框在 draw 闭包里实时读取，不走重组），
/// 只有帧驱动（聚焦时 `caret_blink_driver` 失效节点 + 祖先链）才会让闭包重跑——与生产帧循环
/// 同一通路。无头测试若不泵帧，增量绘制缓存会回放上一次的高亮位置（方向键看起来「没生效」）。
fn cells(root: compose::layout::NodeId, primary: Color) -> Vec<(f32, f32, bool)> {
    advance_frame(1.0 / 60.0);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 120.0));
    let mut v: Vec<(f32, f32, bool)> = draw_tree(root)
        .iter()
        .filter_map(|c| match c {
            DrawCmd::StrokeRRect { x, w, color, line, .. }
                if (line - 1.0).abs() < 0.5 || (line - 2.0).abs() < 0.5 =>
            {
                Some((*x, *w, *color == primary))
            }
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

/// 光标所在格 = 主色边框在「按 x 排序的格子」里的序号（不写死格距，改尺寸不误判）。
fn focused_box(root: compose::layout::NodeId, primary: ColorScheme) -> usize {
    cells(root, primary.primary)
        .iter()
        .position(|(_, _, f)| *f)
        .expect("应存在主色边框的聚焦格")
}

fn click_box(root: compose::layout::NodeId, i: usize) {
    let cs = cells(root, Color::BLACK);
    let (x, w, _) = cs[i];
    click_at(root, x + w * 0.5);
}

fn click_at(root: compose::layout::NodeId, x: f32) {
    let y = 22.0;
    let (t, _, _) = hit_test(root, x, y).expect("应命中某格子");
    assert!(dispatch_click_at(t, x, y), "点击应被消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 120.0));
}

fn recomp(root: compose::layout::NodeId) {
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 120.0));
}

#[test]
fn otp_advances_auto() {
    let (root, code) = setup(6);
    let primary = ColorScheme::light();
    click_box(root, 0);
    assert_eq!(code.read_clone(), "");
    assert_eq!(focused_box(root, primary), 0);

    // 第一个框输入后立即跳到下一格（自动前进）
    InputChar('a');
    recomp(root);
    assert_eq!(code.read_clone(), "a");
    assert_eq!(focused_box(root, primary), 1);

    // 第二个框输入跳到第三格
    InputChar('b');
    recomp(root);
    assert_eq!(code.read_clone(), "ab");
    assert_eq!(focused_box(root, primary), 2);

    // Backspace：当前格为空 → 删前一格并后退到第二格
    InputBackspace();
    recomp(root);
    assert_eq!(code.read_clone(), "a");
    assert_eq!(focused_box(root, primary), 1);

    // 再 Backspace：当前格为空 → 删前一格（首个字符），光标停在第一格
    InputBackspace();
    recomp(root);
    assert_eq!(code.read_clone(), "");
    assert_eq!(focused_box(root, primary), 0);

    // 左右方向键移动光标
    InputChar('x');
    recomp(root);
    assert_eq!(code.read_clone(), "x");
    assert_eq!(focused_box(root, primary), 1);
    InputKey(NavKey::Left);
    recomp(root);
    assert_eq!(focused_box(root, primary), 0);
    InputKey(NavKey::Right);
    recomp(root);
    assert_eq!(focused_box(root, primary), 1);

    // 点击定位：点第三格后输入写入该格（覆盖/补位）
    click_box(root, 3);
    assert_eq!(focused_box(root, primary), 3);
    InputChar('z');
    recomp(root);
    assert_eq!(code.read_clone(), "xz"); // x + 补位到第三格 → "x?z" 但连续化为 "xz"
    println!("[dbg] ok");
}
