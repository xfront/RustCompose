//! 动画控件的「静止即停驱 + 帧驱动不泄漏」回归（对齐 Compose 动画的两条性能不变式）。
//!
//! 覆盖迁移后的三个缺口：
//! ① 入场动画播完即停：消费作用域不再每帧重组（旧 `ensure_frames` 永久 `schedule_scope`）；
//! ② 控件自带帧驱动只注册一次，且随作用域销毁注销（旧实现从不 `remove_frame_driver`）；
//! ③ 下拉指示器弹簧收回到位后不再请求续帧，帧循环得以休眠。

use std::cell::Cell;
use std::rc::Rc;

use compose::animation::{animations_active, running_animations};
use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Box, Modifier, NodeId, TextStyle, create_root_node, dispatch_pull_down, draw_tree,
    hit_test, perform_layout, pull_end_global, pull_move_global,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{
    Composition, MutableState, Recomposer, advance_frame, frame_driver_count, mutableStateOf,
};
use compose_material::{
    Crossfade, LinearProgress, PullToRefresh, SharedAxis, Skeleton, Spinner, Text,
};

const DT: f32 = 1.0 / 60.0;

fn frames(n: u32) {
    for _ in 0..n {
        advance_frame(DT);
        Recomposer::recompose_pending();
    }
}

/// 逐帧读取宿主判据（`animations_active()` 读取即清除，必须每帧读）。
fn probe(n: u32) -> Vec<bool> {
    let mut out = Vec::new();
    for _ in 0..n {
        advance_frame(DT);
        Recomposer::recompose_pending();
        out.push(animations_active());
    }
    out
}

/// ① 入场动画：播完后作用域彻底静止。
#[test]
fn shared_axis_entrance_stops_recomposing() {
    let runs = Rc::new(Cell::new(0u32));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let runs_c = runs.clone();
    comp.set_content(move || {
        let runs_c = runs_c.clone();
        MaterialTheme(ColorScheme::light(), move || {
            runs_c.set(runs_c.get() + 1);
            SharedAxis(
                0,
                true,
                true,
                Modifier::new().fill_max_width().height(60.0),
                || Text("page", TextStyle::new(), Modifier::new()),
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));
    frames(30); // 0.5s > 0.3s 入场时长
    assert_eq!(running_animations(), 0, "入场动画应已退出注册表");
    let after = runs.get();
    assert!(after > 1, "入场期间确实发生了重组：{after}");
    frames(60);
    assert_eq!(
        runs.get(),
        after,
        "入场结束后不该再每帧重组：{} → {}",
        after,
        runs.get()
    );
}

/// ② 帧驱动：注册一次、重组不累加、卸载即注销。
#[test]
fn crossfade_driver_is_registered_once_and_removed_on_unmount() {
    let show = mutableStateOf(true);
    let root = compose_root(show);
    perform_layout(root, Constraints::tight(300.0, 300.0));
    let mounted = frame_driver_count();
    assert!(
        mounted >= 1,
        "Crossfade 应为其淡入注册一条帧驱动，实测 {mounted}"
    );
    frames(20); // 淡入 + 期间若干次本作用域重组
    assert_eq!(
        frame_driver_count(),
        mounted,
        "同一控件反复重组不应新增帧驱动"
    );

    show.set(false); // 整个子作用域销毁
    frames(3);
    assert_eq!(
        frame_driver_count(),
        0,
        "控件卸载后其帧驱动必须注销（旧实现永久驻留）"
    );
}

fn compose_root(show: MutableState<bool>) -> NodeId {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            // if 在 Box 之外：分支关闭时 Box 的子作用域被销毁，
            // 其内部注册的帧驱动应随之注销。
            if show.get() {
                Box(
                    Modifier::new().fill_max_width().height(80.0),
                    Alignment::TopStart,
                    move || {
                        Crossfade(1, Modifier::new().fill_max_width().height(60.0), || {
                            Text("x", TextStyle::new(), Modifier::new())
                        });
                    },
                );
            }
        });
    });
    root
}

/// ④ 纯绘制型无限相位（Spinner / Skeleton）：相位每帧落到绘制命令，
///    但**调用方作用域零重组**（旧实现每帧 `schedule_scope` 整页）。
#[test]
fn draw_phase_widgets_repaint_without_recomposing_caller() {
    let runs = Rc::new(Cell::new(0u32));
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    let runs_c = runs.clone();
    comp.set_content(move || {
        let runs_c = runs_c.clone();
        MaterialTheme(ColorScheme::light(), move || {
            runs_c.set(runs_c.get() + 1);
            Skeleton(120.0, 12.0, Modifier::new());
            Spinner(24.0, Modifier::new());
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));
    assert_eq!(runs.get(), 1);
    assert_eq!(frame_driver_count(), 2, "两个不确定态控件各注册一条帧驱动");
    let before = format!("{:?}", draw_tree(root));

    let act = probe(24); // ~0.4s
    assert!(act.iter().all(|b| *b), "不确定态应持续请求续帧：{act:?}");
    perform_layout(root, Constraints::tight(300.0, 300.0));
    let after = format!("{:?}", draw_tree(root));
    assert_ne!(before, after, "相位推进应落到绘制命令（呼吸/旋转可见）");
    assert_eq!(
        runs.get(),
        1,
        "draw 阶段相位不应触发调用方重组（实测 {} 次）",
        runs.get()
    );
}

/// ⑤ 确定态进度：无动画、不请求续帧，帧循环可休眠。
#[test]
fn determinate_progress_never_requests_frames() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            LinearProgress(Some(0.5), Modifier::new().fill_max_width());
        });
    });
    perform_layout(root, Constraints::tight(300.0, 300.0));
    let act = probe(10);
    assert!(act.iter().all(|b| !*b), "确定态不该请求续帧：{act:?}");
}

/// ④ 下拉刷新进入刷新态：旋转弧在 draw 阶段逐帧推进（不重组指示器作用域），
///    刷新结束收回后再次彻底静默。
#[test]
fn pull_to_refresh_spinner_rotates_in_draw_phase() {
    let refreshing = mutableStateOf(false);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            PullToRefresh(
                refreshing,
                Modifier::new().fill_max_width().height(200.0),
                || {},
                || Text("row", TextStyle::new(), Modifier::new()),
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 400.0));

    let (target, _, _) = hit_test(root, 10.0, 10.0).expect("应命中列表节点");
    assert!(dispatch_pull_down(target, 10.0));
    assert!(
        pull_move_global(200.0),
        "下拉 200px（阻尼后 100px，越阈值）"
    );
    assert!(pull_end_global());
    assert!(refreshing.get(), "越阈值松手应进入刷新态");
    assert_eq!(
        running_animations(),
        0,
        "指示器应走 draw-phase，不进动画注册表"
    );

    frames(2);
    perform_layout(root, Constraints::tight(300.0, 400.0));
    let a = format!("{:?}", draw_tree(root));
    frames(8);
    assert!(probe(1)[0], "刷新态应持续请求续帧（旋转）");
    perform_layout(root, Constraints::tight(300.0, 400.0));
    let b = format!("{:?}", draw_tree(root));
    assert_ne!(a, b, "刷新中的旋转弧应逐帧变化");

    refreshing.set(false);
    let act = probe(40);
    assert!(
        act.iter().rev().take(15).all(|b| !*b),
        "刷新结束收回后应彻底停止续帧：{act:?}"
    );
}

/// ③ 下拉刷新：未越阈值时指示器弹簧收回，到位后不再请求续帧。
#[test]
fn pull_to_refresh_collapses_and_sleeps() {
    let refreshing = mutableStateOf(false);
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            PullToRefresh(
                refreshing,
                Modifier::new().fill_max_width().height(200.0),
                || {},
                move || {
                    Text("row 1", TextStyle::new(), Modifier::new());
                    Text("row 2", TextStyle::new(), Modifier::new());
                },
            );
        });
    });
    perform_layout(root, Constraints::tight(300.0, 400.0));

    let (target, _, _) = hit_test(root, 10.0, 10.0).expect("应命中列表节点");
    assert!(dispatch_pull_down(target, 10.0));
    assert!(pull_move_global(60.0), "下拉 50px（阻尼后 25px）");
    assert!(pull_end_global());
    assert!(!refreshing.get(), "未越阈值不应进入刷新态");

    // 弹簧收回：前几帧需要续帧，0.05s 量级到位后即彻底静默
    // （旋转续帧只在刷新态发生，本例未越阈值）。
    let act = probe(30);
    assert_eq!(running_animations(), 0, "指示器不该注册补间动画");
    let tail = &act[act.len() - 20..];
    assert!(
        tail.iter().all(|b| !b),
        "收回到位后不应再请求续帧（否则帧循环永不休眠）：{act:?}"
    );
    assert!(
        act.iter().take(6).any(|b| *b),
        "收回过程中应请求续帧：{act:?}"
    );
    assert!(!refreshing.get());
}
