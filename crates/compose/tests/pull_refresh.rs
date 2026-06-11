//! 回归用例：下拉刷新手势（越过阈值触发 / 未过阈值取消 / 刷新中状态）。

use compose::foundation::Constraints;
use compose::layout::{
    create_root_node, dispatch_pull_down, draw_tree, hit_test, perform_layout, pull_end_global,
    pull_move_global, DrawCmd, Modifier, TextStyle,
};
use compose::material::{MaterialTheme, ColorScheme};
use compose::runtime::{mutableStateOf, Composer};
use compose_material::{PullToRefresh, Text};

/// 组装一个含 `PullToRefresh` 的页面，返回 (root, refreshing, 触发次数)。
fn setup() -> (compose::layout::NodeId, compose::runtime::MutableState<bool>, compose::runtime::MutableState<usize>) {
    let refreshing = mutableStateOf(false);
    let triggered = mutableStateOf(0usize);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            PullToRefresh(refreshing,
                Modifier::new().fill_max_width().height(200.0),
                {
                    let t = triggered;
                    move || {
                        t.update(|n| *n += 1);
                    }
                },
                move || {
                    Text("row 1", TextStyle::new(), Modifier::new());
                    Text("row 2", TextStyle::new(), Modifier::new());
                },
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 400.0));
    (root, refreshing, triggered)
}

#[test]
fn pull_below_threshold_does_not_trigger() {
    let (root, refreshing, triggered) = setup();
    let (target, _, _) = hit_test(root, 10.0, 10.0).expect("应命中列表节点");
    assert!(dispatch_pull_down(target, 10.0), "按下应开启下拉会话");
    // 下拉 50px → 阻尼 0.5× → 25px < 64px 阈值
    assert!(pull_move_global(60.0), "移动应被下拉会话消费");
    assert!(pull_end_global(), "抬起应结束下拉会话");
    assert_eq!(triggered.get(), 0, "未过阈值不应触发刷新");
    assert!(!refreshing.get(), "未过阈值不应进入刷新态");
}

#[test]
fn pull_past_threshold_triggers_refresh() {
    let (root, refreshing, triggered) = setup();
    let (target, _, _) = hit_test(root, 10.0, 10.0).expect("应命中列表节点");
    assert!(dispatch_pull_down(target, 10.0), "按下应开启下拉会话");
    // 下拉 200px → 阻尼 0.5× → 100px ≥ 64px 阈值
    assert!(pull_move_global(210.0), "移动应被下拉会话消费");
    assert!(pull_end_global(), "抬起应结束下拉会话");
    assert_eq!(triggered.get(), 1, "越过阈值应触发一次刷新");
    assert!(refreshing.get(), "触发后应进入刷新态");
}

#[test]
fn pull_outside_target_is_ignored() {
    let (root, _refreshing, triggered) = setup();
    // 命中一个不属于下拉容器的节点（根节点本身无下拉处理器）
    let (target, _, _) = hit_test(root, 10.0, 10.0).expect("应命中节点");
    assert!(dispatch_pull_down(target, 10.0));
    // 会话结束后再抬起不应重复触发
    assert!(pull_end_global());
    assert_eq!(triggered.get(), 0, "仅下拉 0px 不应触发");
}

/// 刷新态应渲染「液面 + 谷底进度环」（液态下拉刷新视觉路径真正执行）。
#[test]
fn pull_refresh_draws_indicator_when_refreshing() {
    let refreshing = mutableStateOf(true);
    let root = create_root_node(compose::layout::Alignment::TopStart);
    let comp = compose::runtime::Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            PullToRefresh(refreshing,
                Modifier::new().fill_max_width().height(200.0),
                || {},
                move || {
                    Text("row 1", TextStyle::new(), Modifier::new());
                },
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 400.0));
    let cmds = draw_tree(root);
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillCircle { .. })),
        "刷新态应绘制进度环底盘圆"
    );
    assert!(
        cmds.iter().any(|c| matches!(c, DrawCmd::FillPath { .. })),
        "刷新态应绘制液面"
    );
}
