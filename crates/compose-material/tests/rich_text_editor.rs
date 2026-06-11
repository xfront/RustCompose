//! Quill 风格富文本编辑器的无头回归测试：
//! 输入路由、撤销/重做、HTML/Markdown 导出、查找高亮。

use std::rc::Rc;
use std::cell::RefCell;

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Modifier, create_root_node, dispatch_click_at, hit_test, perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme, theme_colors};
use compose::runtime::Composition;
use compose_material::widgets::rich_text_editor::{
    Ed, EdState, parse_html, rich_char, rich_editor_focus, rich_find_next, rich_find_set,
    to_html, to_markdown,
};
use compose_material::{InputUndo, RichTextEditor, RichTextEditorHandle};

/// 构造一份文档并返回共享 `Ed`（供 focus + direct 调用）。
fn make_ed(html: &str) -> Ed {
    Rc::new(RefCell::new(EdState::from_blocks(parse_html(html))))
}

#[test]
fn routing_char_inserts_into_focused_editor() {
    let ed = make_ed("<p>hi</p>");
    rich_editor_focus(ed.clone());
    let n0 = ed.borrow().blocks[0].flat_text().chars().count();
    rich_char('Z');
    let n1 = ed.borrow().blocks[0].flat_text().chars().count();
    assert_eq!(n1, n0 + 1, "聚焦编辑器应收到并插入字符");
    assert!(ed.borrow().blocks[0].flat_text().contains('Z'));
}

#[test]
fn routing_undo_via_input_handler() {
    let ed = make_ed("<p>hi</p>");
    rich_editor_focus(ed.clone());
    rich_char('A');
    let after = ed.borrow().blocks[0].flat_text().to_string();
    // 经平台路由入口 Ctrl+Z → 应撤销上一步插入
    InputUndo();
    let reverted = ed.borrow().blocks[0].flat_text().to_string();
    assert_ne!(after, reverted, "InputUndo 应撤销上一步编辑");
    assert_eq!(reverted, "hi");
}

#[test]
fn routing_find_highlights_matches() {
    let ed = make_ed("<p>hello world hello</p>");
    rich_editor_focus(ed.clone());
    rich_find_set("hello");
    assert_eq!(
        ed.borrow().find_matches.len(),
        2,
        "应命中两处 'hello'"
    );
    // F3：跳到下一个命中，命中数不变
    rich_find_next();
    assert_eq!(ed.borrow().find_matches.len(), 2);
}

#[test]
fn export_html_and_markdown_roundtrip() {
    let blocks = parse_html(
        "<h1>Title</h1>\
         <p>a <b>bold</b> and <i>it</i> <span style=\"color:#ff0000\">red</span></p>\
         <ul><li>x</li><li>y</li></ul>\
         <ol><li>one</li></ol>\
         <blockquote>q</blockquote>\
         <pre>code</pre>",
    );
    let html = to_html(&blocks);
    assert!(html.contains("<h1>Title</h1>"), "h1 导出");
    assert!(html.contains("<b>bold</b>"), "加粗导出");
    assert!(html.contains("<i>it</i>"), "斜体导出");
    assert!(html.contains("#ff0000") || html.contains("#FF0000"), "颜色导出");
    assert!(html.contains("<ul><li>x</li><li>y</li></ul>"), "无序列表合并");
    assert!(html.contains("<ol><li>one</li></ol>"), "有序列表");
    assert!(html.contains("<blockquote>q</blockquote>"), "引用块");

    let md = to_markdown(&blocks);
    assert!(md.contains("# Title"), "h1→# ");
    assert!(md.contains("**bold**"), "加粗→**");
    assert!(md.contains("*it*"), "斜体→*");
    assert!(md.contains("- x") && md.contains("- y"), "无序列表→- ");
    assert!(md.contains("> q"), "引用→> ");
}

#[test]
fn handle_undo_redo_across_set_html() {
    let cap: Rc<RefCell<Option<RichTextEditorHandle>>> = Rc::new(RefCell::new(None));
    let cap2 = cap.clone();
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        let cap_inner = cap2.clone();
        MaterialTheme(ColorScheme::light(), move || {
            let h = RichTextEditor(Modifier::new());
            cap_inner.borrow_mut().replace(h.clone());
            h.set_html("<p>first</p>");
        });
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));

    let h = cap.borrow().clone().unwrap();
    let a = h.to_html();
    h.set_html("<p>second</p>");
    let b = h.to_html();
    assert_ne!(a, b);
    assert!(h.can_undo(), "set_html 后应可撤销");
    h.undo();
    assert_eq!(h.to_html(), a, "undo 应回到 first");
    assert!(h.can_redo());
    h.redo();
    assert_eq!(h.to_html(), b, "redo 应回到 second");

    // 查找经句柄
    h.set_find("second");
    assert_eq!(h.find_match_count(), 1);
}

#[test]
fn build_and_render_editor_tree() {
    // 确保 RichTextEditor 能正常组合并产出绘制命令（不 panic）。
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let h = RichTextEditor(Modifier::new().fill_max_width().height(300.0));
            h.set_html(
                "<h2>Quill</h2><p><b>bold</b> <i>italic</i> <u>u</u></p>\
                 <ul><li>one</li><li>two</li></ul>",
            );
        });
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));
    let _cmds = compose::layout::draw_tree(root);
    assert!(!_cmds.is_empty(), "编辑器应产生绘制命令");
}

/// 回归（用户崩溃堆栈）：点击编辑区 → `dispatch_click_at` 在 runtime 借用外执行
/// `on_click`，此时作用域栈为空，回调里若再读 `theme_colors()` 就会
/// panic `CompositionLocal not provided`。
#[test]
fn clicking_editor_body_outside_composition_does_not_panic() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            let h = RichTextEditor(Modifier::new().fill_max_width().height(300.0));
            h.set_html("<p>hello</p>");
        });
    });
    perform_layout(root, Constraints::loose(800.0, 600.0));
    // 工具栏之下的编辑区：命中其（或子）节点后沿祖先链找到编辑区 on_click。
    let (x, y) = (60.0, 80.0);
    let hit = hit_test(root, x, y).expect("应命中编辑器区域");
    assert!(dispatch_click_at(hit.0, x, y), "编辑区点击应被消费");
    std::mem::forget(comp);
}

/// 组合外读主题色（点击/悬停/绘制/输入路由回调）应回退到全局主题镜像，而不是 panic。
#[test]
fn theme_colors_outside_composition_uses_global_mirror() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(|| {
        MaterialTheme(ColorScheme::dark(), || {});
    });
    perform_layout(root, Constraints::loose(100.0, 100.0));
    // 组合结束后 scope_stack 为空：此处正是崩溃堆栈里回调所处的上下文。
    let c = theme_colors();
    assert!(c.is_dark(), "组合外应回退到全局主题镜像（暗色）");
    std::mem::forget(comp);
}
