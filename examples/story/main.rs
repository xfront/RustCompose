//! Storybook 高质量还原 —— 103 页 / 7 大分组结构（Demos / Basics / Data Display /
//! Layout / Navigation / Feedback / Advanced），外壳为「分组侧栏 + 标题栏/菜单栏 + 状态栏 +
//! 页面容器按需替换」，每页用 `compose_material` 的真实控件渲染。
//!
//! 运行：`cargo run --example story`
//!
//! 本文件仅为入口与模块声明，页面实现按分组拆到 `story/pages/*.rs`，
//! 外壳在 `story/shell.rs`，共享导入在 `story/prelude.rs`。

mod common;
mod prelude;
mod shell;
mod pages;

use crate::prelude::*;
use crate::shell::StoryApp;

fn main() {
    // 登记文本控件输入路由（键盘/字符/IME/剪贴板/滚动 → compose-material）。
    compose_material::install_router();
    run_app_with(
        WindowConfig::sized("Story — Compose Showcase", 1080.0, 800.0),
        StoryApp { dark: false, palette: 0 },
    );
}

// ══════════════════ 无头探针（cargo test --example story）══════════════════
#[cfg(test)]
mod probe_state {
    std::thread_local! {
        /// story_shell 每趟组合后镜像 open_menu / page，供探针断言。
        pub static OM: std::cell::Cell<i8> = const { std::cell::Cell::new(-1) };
        pub static PAGE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
}

#[cfg(test)]
mod probe {
    use crate::prelude::*;
    use crate::shell::{MENU_H, MENU_W, TITLE_H, story_shell};
    use compose::foundation::Constraints;
    use compose::layout::{
        Alignment, create_root_node, dispatch_click_at, hit_test, perform_layout,
    };
    use compose::runtime::{Composition, Recomposer, advance_frame, with_rt_pub};
    fn boot() -> compose::layout::NodeId {
        let root = create_root_node(Alignment::TopStart);
        let comp = Composition::new(root);
        comp.set_content(|| {
            let dark = remember(|| mutableStateOf(false));
            let palette = remember(|| mutableStateOf(0usize));
            // 与 StoryApp 相同层次：主题在宿主外层（浮层内容也要读主题 local）。
            MaterialTheme(ColorScheme::light(), move || {
                OverlayHost(Modifier::new().fill_max_size(), move || {
                    Box(
                        Modifier::new().fill_max_size(),
                        Alignment::TopStart,
                        move || story_shell(dark, palette),
                    );
                });
            });
        });
        perform_layout(root, Constraints::new(0.0, 1080.0, 0.0, 800.0));
        root
    }

    fn recomp(root: compose::layout::NodeId) {
        Recomposer::recompose_pending();
        perform_layout(root, Constraints::new(0.0, 1080.0, 0.0, 800.0));
    }

    /// 点击窗口坐标 (x,y)（等价 kernel.pointer_down 的命中+分发）。
    fn click(root: compose::layout::NodeId, x: f32, y: f32) -> bool {
        let Some((target, lx, ly)) = hit_test(root, x, y) else {
            println!("[probe] click({x},{y}) 未命中任何节点");
            return false;
        };
        dispatch_click_at(target, lx, ly)
    }

    /// 菜单栏第 i 项中心（标题栏 TITLE_H 之下）。
    fn menu_xy(i: usize) -> (f32, f32) {
        (i as f32 * MENU_W + MENU_W / 2.0, TITLE_H + MENU_H / 2.0)
    }

    /// 宿主（OverlayHost 节点）当前的孩子数（>1 即有浮层发射到树上）。
    /// 结构：root → MaterialTheme 透传节点 → OverlayHost → 内容 Box。
    fn host_kids(root: compose::layout::NodeId) -> usize {
        with_rt_pub(|rt| {
            let themed = rt.nodes[root.index()].children[0];
            let host = rt.nodes[themed.index()].children[0];
            rt.nodes[host.index()].children.len()
        })
    }

    fn om() -> i8 {
        super::probe_state::OM.with(|c| c.get())
    }
    fn page() -> usize {
        super::probe_state::PAGE.with(|c| c.get())
    }

    /// IDE 工作区：侧栏必须注册为 DockPanel，且与文档/工具箱/输出/错误列表/属性
    /// 同处一棵可拖拽停靠树；外壳只暴露一个共享 DockState。
    #[test]
    fn sidebar_is_a_dock_panel() {
        let _root = boot();
        let state = super::common::shell_dock().expect("外壳应共享 DockState");
        let ids = super::common::shell_dock_ids().expect("外壳应共享面板 id");

        // 侧栏是一个真实的可停靠面板（VS2022「解决方案资源管理器」）
        let info = compose_material::dock_panel_info(ids.sidebar)
            .expect("侧栏应注册进面板表");
        assert_eq!(info.title, "解决方案资源管理器");
        assert!(!info.options.closable, "导航骨架不应可被关闭");
        assert!(state.is_docked(ids.sidebar), "侧栏应停靠在布局树中");
        assert_eq!(state.window_of(ids.sidebar), Some(compose_material::WINDOW_MAIN));

        // 默认布局左半区是「侧栏 + 工具箱」标签组
        let m = state.model();
        let root = m.root.clone().expect("默认布局不应为空");
        let left = match &root {
            DockNode::Split { first, .. } => first.collect_ids(),
            _ => panic!("默认布局根应为左右分割"),
        };
        assert!(left.contains(&ids.sidebar), "侧栏应在左半区: {left:?}");
        assert!(left.contains(&ids.toolbox), "工具箱应与侧栏并组: {left:?}");

        // 六个面板都在布局里，且没有第二个 DockLayout 覆盖全局命中表
        let all = root.collect_ids();
        for id in [ids.sidebar, ids.documents, ids.toolbox, ids.output, ids.errors, ids.properties] {
            assert!(all.contains(&id), "面板 {id} 应进入默认工作区");
        }
        assert_eq!(all.len(), 6);

        // 保存 / 恢复 IDE 布局 roundtrip（含跨窗口停靠能力的状态通道）
        let text = state.save_full_state();
        let other = compose_material::dock_state(compose_material::DockModel::empty());
        assert!(other.restore_full_state(&text));
        assert_eq!(other.model(), m);

        // 侧栏「最小化」（收起为图标条）后，Sidebar 内部的图标胶囊仍须可见。
        // 回归：曾把 Sidebar 误放进 else 分支，导致 rail 模式整个图标条不渲染。
        let rail = super::common::shell_rail().expect("外壳应共享侧栏 rail 状态");
        rail.set(true);
        recomp(_root);
        let rail_cmds = compose::layout::draw_tree(_root);
        let rail_paths = rail_cmds
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    compose::layout::DrawCmd::FillPath { .. }
                        | compose::layout::DrawCmd::StrokePath { .. }
                )
            })
            .count();
        rail.set(false);
        recomp(_root);
        let full_cmds = compose::layout::draw_tree(_root);
        let full_paths = full_cmds
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    compose::layout::DrawCmd::FillPath { .. }
                        | compose::layout::DrawCmd::StrokePath { .. }
                )
            })
            .count();
        assert!(
            rail_paths > 0,
            "最小化后应仍能画出导航图标（图标胶囊），实际 0 条路径命令"
        );
        assert!(
            rail_paths >= full_paths / 2,
            "最小化后图标数量不应骤减: rail={rail_paths} full={full_paths}"
        );

        // View 菜单「重置窗口布局」：拖乱面板后能一键还原默认工作区
        state.close(ids.errors);
        state.toggle_auto_hide(ids.properties);
        assert_ne!(state.model(), m, "前置：布局确实被改动");
        let dark = mutableStateOf(false);
        let palette = mutableStateOf(0usize);
        let pg = mutableStateOf(0usize);
        crate::shell::run_menu_action(1, super::pages::PAGES.len(), dark, pg, palette);
        assert_eq!(state.model(), m, "重置窗口布局应还原默认工作区");
    }

    /// 回归：菜单栏下拉的完整打开/关闭/选择流程。
    /// 曾有两个框架级缺陷导致「点菜单不显示下拉、反而跳回 Spreadsheet」：
    /// 1) 祖先作用域重组时销毁后代作用域 → remember 槽（open_menu/page）丢失重建；
    /// 2) 已发射浮层被移除时不调度宿主重组 → 幽灵浮层残留并吞掉点击。
    #[test]
    fn menu_dropdown_flow() {
        let root = boot();
        let (vx, vy) = menu_xy(1); // "View"

        // 1) 点击 View → 菜单打开（浮层发射到宿主），页面不动
        assert!(click(root, vx, vy), "首次点击 View 应被消费");
        recomp(root);
        assert_eq!(host_kids(root), 2, "菜单打开后宿主应有 内容+浮层 两个孩子");
        assert_eq!(om(), 1, "View 菜单应处于打开态");
        assert_eq!(page(), 0, "打开菜单不应改变页面");

        // 2) 菜单打开时点击菜单栏位置 → 命中浮层背景 → 点外部关闭
        let Some((t2, _, _)) = hit_test(root, vx, vy) else {
            panic!("菜单打开后命中失败")
        };
        assert!(dispatch_click_at(t2, 0.0, 0.0));
        recomp(root);
        assert_eq!(om(), -1, "点击外部应关闭菜单");
        assert_eq!(host_kids(root), 1, "关闭后浮层应从宿主移除（无幽灵节点）");
        assert_eq!(page(), 0, "关闭菜单不应改变页面");

        // 3) 关闭后菜单可再次打开（幽灵背景曾让第二次点击失效）
        assert!(click(root, vx, vy));
        recomp(root);
        assert_eq!(om(), 1, "关闭后再次点击应重新打开");

        // 入场动画（menu_panel 的 max_height 生长）需逐帧推进到 t≈1，面板才有实际高度可命中。
        for _ in 0..30 {
            advance_frame(1.0 / 60.0);
            recomp(root);
        }

        // 4) 点击 View 菜单第 4 项（page 3）→ 执行动作并关闭，页面正确跳转
        // 几何由库 menu_panel 决定：面板左 = 锚点左（idx*MENU_W），顶 = 锚点下沿 + gap 4，
        // 上内边距 SPACE_MD 12，项高 MENU_ITEM_HEIGHT 48 → 第 4 项中心 y = 顶 + 12 + 3*48 + 24。
        let ix = 1.0 * MENU_W + 30.0; // 面板内左侧（项 fill_max_width）
        let iy = TITLE_H + MENU_H + 4.0 + 12.0 + 3.0 * 48.0 + 24.0; // 第 4 项中心
        assert!(click(root, ix, iy), "菜单项点击应被消费");
        recomp(root);
        assert_eq!(om(), -1, "选择菜单项后应关闭");
        assert_eq!(page(), 3, "应跳到所选页面");
        assert_eq!(host_kids(root), 1, "浮层应完全移除");
    }

    /// IDE 工作区的 DockPanel 应跟随全局液态玻璃开关切换材质（端到端）。
    #[test]
    fn dock_panels_follow_liquid_glass_toggle() {
        use compose::layout::{DrawCmd, draw_tree};
        SetLiquidGlass(false);
        let root = boot();
        let glass_count = |cmds: &[DrawCmd]| {
            cmds.iter()
                .filter(|c| matches!(c, DrawCmd::GlassPanel { .. }))
                .count()
        };
        assert_eq!(glass_count(&draw_tree(root)), 0, "关闭态不应产出 GlassPanel");
        SetLiquidGlass(true);
        recomp(root);
        let on = glass_count(&draw_tree(root));
        assert!(
            on >= 4,
            "IDE 各方向 DockPanel 应玻璃化，实际 {on} 个 GlassPanel"
        );
        SetLiquidGlass(false);
        recomp(root);
        assert_eq!(
            glass_count(&draw_tree(root)),
            0,
            "关闭后应恢复不透明底色"
        );
    }
}
