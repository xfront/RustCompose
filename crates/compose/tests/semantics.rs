//! Semantics 回归：语义树构建、显式/派生属性、合并规则、查询/命中/动作/焦点/JSON 导出。

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Modifier, Policy, add_node, create_root_node, perform_layout, with_content_scope,
};
use compose::runtime::{Composition, with_rt_pub};
use compose::semantics::{
    CONTENT_DESCRIPTION, CHECKED, ROLE, Role, SemanticsAction, TEXT, build_tree, with_tree,
};

fn boot(content: impl Fn() + 'static) -> compose::layout::NodeId {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(content);
    perform_layout(root, Constraints::tight(300.0, 300.0));
    let _ = build_tree(root);
    root
}

#[test]
fn explicit_semantics_are_queryable() {
    boot(|| {
        let id = add_node(
            Policy::Leaf,
            Modifier::new()
                .size(40.0, 40.0)
                .content_description("Save button")
                .test_tag("save-btn")
                .role(Role::Button),
        );
        with_content_scope(id, || {});
    });
    with_tree(|tree| {
        let id = tree.find_by_test_tag("save-btn").expect("testTag 可查");
        let n = tree.get(id).unwrap();
        assert_eq!(n.config.get(CONTENT_DESCRIPTION).as_deref(), Some("Save button"));
        assert_eq!(n.config.get(ROLE), Some(Role::Button));
        assert_eq!(n.label().as_deref(), Some("Save button"));
    });
}

#[test]
fn clickable_derives_button_role_and_click_action() {
    let clicked = Rc::new(Cell::new(0));
    let c = clicked.clone();
    boot(move || {
        let id = add_node(Policy::Leaf, Modifier::new().size(40.0, 40.0).test_tag("clickable"));
        let c = c.clone();
        with_rt_pub(|rt| rt.nodes[id.index()].on_click = Some(Box::new(move || c.set(c.get() + 1))));
        with_content_scope(id, || {});
    });
    with_tree(|tree| {
        let id = tree.find_by_test_tag("clickable").expect("clickable 节点");
        let n = tree.get(id).expect("语义节点");
        assert_eq!(n.config.get(ROLE), Some(Role::Button));
        assert!(n.config.actions().contains(&SemanticsAction::Click));
    });
}

#[test]
fn text_from_draw_is_attached_to_semantics() {
    boot(|| {
        let id = add_node(Policy::Leaf, Modifier::new().size(100.0, 30.0).test_tag("text-node"));
        with_rt_pub(|rt| {
            rt.nodes[id.index()].draw = Some(Box::new(|b, x, y, _, _| {
                b.draw_text(x, y, "Hello Semantics", &compose::layout::TextStyle::new(), 0.0);
            }));
        });
        with_content_scope(id, || {});
    });
    with_tree(|tree| {
        let id = tree.find_by_test_tag("text-node").expect("text 节点");
        let n = tree.get(id).unwrap();
        assert_eq!(n.config.get(TEXT).as_deref(), Some("Hello Semantics"));
    });
}

#[test]
fn merge_descendants_combines_text_and_hides_children() {
    boot(|| {
        let parent = add_node(Policy::Box(Alignment::TopStart), Modifier::new().merge_descendants().test_tag("merged"));
        with_content_scope(parent, || {
            for label in ["Hello", "World"] {
                let child = add_node(Policy::Leaf, Modifier::new().size(40.0, 20.0));
                let l = label.to_string();
                with_rt_pub(|rt| {
                    rt.nodes[child.index()].draw = Some(Box::new(move |b, x, y, _, _| {
                        b.draw_text(x, y, &l, &compose::layout::TextStyle::new(), 0.0);
                    }));
                });
                with_content_scope(child, || {});
            }
        });
    });
    with_tree(|tree| {
        let id = tree.find_by_test_tag("merged").expect("merged 节点");
        let root = tree.get(id).unwrap();
        let text = root.config.get(TEXT).unwrap_or_default();
        assert!(text.contains("Hello") && text.contains("World"), "应合并后代文本: {text}");
        assert!(root.children.is_empty(), "合并后后代不单独暴露");
    });
}

#[test]
fn clear_and_set_semantics_cuts_subtree() {
    boot(|| {
        let parent = add_node(
            Policy::Box(Alignment::TopStart),
            Modifier::new()
                .test_tag("card")
                .clear_and_set_semantics(|c| {
                    c.set(CONTENT_DESCRIPTION, "Card".to_string());
                    c.set(CHECKED, true);
                }),
        );
        with_content_scope(parent, || {
            let child = add_node(Policy::Leaf, Modifier::new().size(40.0, 20.0).test_tag("hidden-child"));
            with_content_scope(child, || {});
        });
    });
    with_tree(|tree| {
        assert!(tree.find_by_test_tag("hidden-child").is_none(), "后代语义应被清除");
        let id = tree.find_by_test_tag("card").expect("card 节点");
        let root = tree.get(id).unwrap();
        assert_eq!(root.config.get(CHECKED), Some(true));
    });
}

#[test]
fn semantic_focus_and_hit_test_work() {
    boot(|| {
        for i in 0..3 {
            let id = add_node(
                Policy::Leaf,
                Modifier::new().size(30.0, 30.0).test_tag(format!("item-{i}")),
            );
            with_rt_pub(|rt| rt.nodes[id.index()].on_click = Some(Box::new(|| {})));
            with_content_scope(id, || {});
        }
    });
    compose::semantics::with_tree_mut(|tree| {
        let order = tree.linear_order();
        assert_eq!(order.len(), 3, "可交互节点应按 DFS 进入焦点顺序");
        let first = tree.focus_next().expect("首个焦点");
        assert_eq!(first, order[0]);
        let second = tree.focus_next().expect("下一个焦点");
        assert_eq!(second, order[1]);
        let hit = tree.hit_test(10.0, 10.0).expect("坐标命中");
        assert!(tree.get(hit).is_some());
    });
}

#[test]
fn json_snapshot_contains_semantic_properties() {
    boot(|| {
        let id = add_node(
            Policy::Leaf,
            Modifier::new().size(40.0, 40.0).content_description("Icon").role(Role::Image),
        );
        with_content_scope(id, || {});
    });
    let json = with_tree(|t| t.to_json());
    assert!(json.contains("\"contentDescription\":\""), "JSON 应含语义属性: {json}");
    assert!(json.contains("Icon"), "JSON 应含标签值");
    assert!(json.contains("\"role\":\"image\""), "JSON 应含角色: {json}");
}
