//! Storybook 高质量还原 —— 114 页 / 10 大分组结构（Demos / Basics / Data Display /
//! Layout / Navigation / Feedback / Advanced / Compose 对齐 / 多媒体 / Platform），外壳为「分组侧栏 + 标题栏/菜单栏 + 状态栏 +
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
#[cfg(all(
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
use crate::shell::StoryApp;

#[cfg(all(
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
fn main() {
    // 登记文本控件输入路由（键盘/字符/IME/剪贴板/滚动 → compose-material）。
    compose_material::install_router();
    run_app_with(
        WindowConfig::sized("Story — Compose Showcase", 1080.0, 800.0),
        StoryApp { dark: false, palette: 0 },
    );
}

// Android 的 NativeActivity 入口由 story_android cdylib（story/android_lib.rs）导出，
// 本 bin 仅作为桌面/Web 入口；Android 下仍需空 main 满足 rustc 链接。
#[cfg(target_os = "android")]
fn main() {}

// ══════════════════ 无头探针（cargo test --example story）══════════════════
#[cfg(test)]
mod probe_state {
    use crate::prelude::MutableState;

    std::thread_local! {
        /// story_shell 每趟组合后镜像 open_menu / page，供探针断言。
        pub static OM: std::cell::Cell<i8> = const { std::cell::Cell::new(-1) };
        pub static PAGE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        /// 外壳页状态句柄（仅测试构建）：供探针切换页面复现 remember 串位。
        pub static PAGE_ST: std::cell::Cell<Option<MutableState<usize>>> =
            const { std::cell::Cell::new(None) };
    }
}

#[cfg(test)]
mod probe {
    use crate::prelude::*;
    use crate::shell::{MENU_H, MENU_W, TITLE_H, story_shell};
    use compose::foundation::Constraints;
    use compose::layout::{
        Alignment, DrawCmd, create_root_node, dispatch_click_at, dispatch_hover_at, hit_test,
        perform_layout,
    };
    use compose::runtime::{
        Composition, Recomposer, advance_frame, frame_driver_count, with_rt_pub,
    };
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

    /// Compose 对齐演示页必须能逐个渲染而不 panic（覆盖所有新增页面）。
    #[test]
    fn compose_alignment_pages_render_without_panic() {
        let root = boot();
        let ps = super::probe_state::PAGE_ST.with(|c| c.get()).expect("页状态");
        for name in [
            "SlotTable",
            "Applier",
            "Snapshot",
            "Effects",
            "Modifier.Node & GraphicsLayer",
            "Subcompose & Semantics",
            "pointerInput 手势",
            "AnimatedTransition",
            "Video",
        ] {
            let idx = super::pages::PAGES
                .iter()
                .position(|p| p.name == name)
                .unwrap_or_else(|| panic!("页面 {name} 应注册"));
            ps.set(idx);
            recomp(root);
            let _ = compose::layout::draw_tree(root);
        }
    }

    /// 视频页每帧都会因时钟状态重组：channel/帧驱动必须只注册一次，
    /// 否则重复驱动 + 失效 Sender 会让「打开文件」永远收不到路径并无尽续帧（闪烁）。
    #[test]
    fn video_page_recomposition_does_not_grow_frame_drivers() {
        let root = boot();
        let ps = super::probe_state::PAGE_ST.with(|c| c.get()).expect("页状态");
        let idx = super::pages::PAGES
            .iter()
            .position(|p| p.name == "Video")
            .expect("Video 页应注册");
        ps.set(idx);
        recomp(root);
        let before = frame_driver_count();
        for _ in 0..12 {
            advance_frame(0.016);
            recomp(root);
        }
        assert_eq!(
            frame_driver_count(),
            before,
            "视频页重组不应累积帧驱动"
        );
    }

    /// 按钮点击后必须**当次重组**刷新统计文本，不能等鼠标离开触发 hover 重绘。
    #[test]
    fn slot_table_button_click_recomposes_without_pointer_move() {
        let root = boot();
        let ps = super::probe_state::PAGE_ST.with(|c| c.get()).expect("页状态");
        let idx = super::pages::PAGES
            .iter()
            .position(|p| p.name == "SlotTable")
            .expect("SlotTable 页应注册");
        ps.set(idx);
        recomp(root);

        let stat = |cmds: &[DrawCmd]| -> String {
            cmds.iter()
                .find_map(|c| match c {
                    DrawCmd::Text { text, .. } if text.starts_with("逻辑槽 =") => Some(text.clone()),
                    _ => None,
                })
                .expect("统计文本应绘制")
        };
        let before = stat(&compose::layout::draw_tree(root));
        let (x, y) = compose::layout::draw_tree(root)
            .iter()
            .find_map(|c| match c {
                DrawCmd::Text { text, x, y, .. } if text.contains("追加 slot") => Some((*x + 12.0, *y + 8.0)),
                _ => None,
            })
            .expect("按钮文字应绘制");

        assert!(click(root, x, y), "应命中追加按钮");
        recomp(root); // 没有 pointer_move / 没有离开按钮
        let after = stat(&compose::layout::draw_tree(root));
        assert_ne!(before, after, "点击后统计文本应立即刷新（无需移开鼠标）");
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

    /// 菜单已打开时，悬停其它菜单标签直接切换浮层；关闭态悬停不打开。
    #[test]
    fn menu_hover_switches_while_open() {
        let root = boot();
        let (vx, vy) = menu_xy(1); // View
        assert!(click(root, vx, vy), "点击 View 应打开菜单");
        recomp(root);
        assert_eq!(om(), 1);

        // 浮层 scrim 顶部穿透后，hover 应命中 Theme 标签而非 scrim。
        let (tx, ty) = menu_xy(2); // Theme
        let Some((t, _, _)) = hit_test(root, tx, ty) else {
            panic!("菜单打开后标签应可命中");
        };
        dispatch_hover_at(t);
        recomp(root);
        assert_eq!(om(), 2, "悬停 Theme 应切换到 Theme 菜单");

        let (hx, hy) = menu_xy(3); // Help
        let (ht, _, _) = hit_test(root, hx, hy).expect("Help 标签应可命中");
        dispatch_hover_at(ht);
        recomp(root);
        assert_eq!(om(), 3, "继续悬停 Help 应切换到 Help 菜单");

        // 再点当前标签关闭；关闭态 hover 不应打开菜单。
        assert!(click(root, hx, hy), "点击已打开标签应关闭");
        recomp(root);
        assert_eq!(om(), -1);
        let (vt, _, _) = hit_test(root, vx, vy).expect("View 标签应可命中");
        dispatch_hover_at(vt);
        recomp(root);
        assert_eq!(om(), -1, "关闭态悬停不应打开菜单");
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

    /// 回归：右侧 peek 曾在组合期读取尚未落位的根节点几何，高度算成 0，
    /// 标题与内容全部挤在一起（用户截图「弹出内容乱了」）。
    #[test]
    fn right_peek_panel_keeps_full_height_and_content_layout() {
        let root = boot();
        let state = super::common::shell_dock().unwrap();
        let ids = super::common::shell_dock_ids().unwrap();
        state.hide_to_edge(ids.properties, DockEdge::Right);
        state.expand_from_edge(ids.properties);
        recomp(root);
        let cmds = compose::layout::draw_tree(root);
        let peek = cmds.iter().find_map(|c| match c {
            compose::layout::DrawCmd::FillRRect { x, w, h, .. }
            | compose::layout::DrawCmd::GlassPanel { x, w, h, .. }
                if *x > 900.0 && *w > 150.0 && *w < 500.0 =>
            {
                Some((*w, *h))
            }
            _ => None,
        });
        let (pw, ph) = peek.expect("应绘制右侧 peek 面板");
        assert!(ph > 400.0, "peek 应贴边全高，实际 {pw}×{ph}");
        let ys: Vec<f32> = cmds
            .iter()
            .filter_map(|c| match c {
                compose::layout::DrawCmd::Text { text, y, .. }
                    if text == "属性"
                        || text.starts_with("页面:")
                        || text.starts_with("分组:") =>
                {
                    Some(*y)
                }
                _ => None,
            })
            .collect();
        assert_eq!(ys.len(), 3, "peek 应有标题与两行内容，实际 {ys:?}");
        assert!(
            ys[0] < ys[1] && ys[1] < ys[2],
            "peek 内容应依次纵向排布而不是重叠，实际 {ys:?}"
        );
    }

    /// 左侧解决方案资源管理器最小化必须收进左边条，不能判成 Top。
    #[test]
    fn sidebar_auto_hide_uses_left_edge() {
        let root = boot();
        let state = super::common::shell_dock().expect("外壳应共享 DockState");
        let ids = super::common::shell_dock_ids().expect("外壳应共享面板 id");
        // 模拟页面上的「自动隐藏侧栏」按钮：指针在文档区中间，不在侧栏上。
        compose::layout::set_last_pointer_global(500.0, 400.0);
        state.toggle_auto_hide(ids.sidebar);
        recomp(root);
        let model = state.model();
        let h = model
            .hidden
            .iter()
            .find(|h| h.items.contains(&ids.sidebar))
            .expect("侧栏应进入自动隐藏组");
        assert_eq!(
            h.edge,
            DockEdge::Left,
            "左侧资源管理器应收进左边条，实际 {:?}",
            h.edge
        );
    }

    /// 集成 smoke：真实外壳中反复切页（含 Tabs 页）应始终正常渲染。
    /// 状态隔离的精确回归护栏在 compose/examples/slot_position_identity.rs。
    #[test]
    fn switching_pages_isolates_page_remember_state() {
        let root = boot();
        let page_state = super::probe_state::PAGE_ST.with(|c| c.get()).expect("外壳应暴露页状态句柄");
        let texts = || -> Vec<String> {
            compose::layout::draw_tree(root)
                .iter()
                .filter_map(|c| match c {
                    compose::layout::DrawCmd::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };

        // Tabs 页：3 标签 TabRow 的内容应稳定显示首页，不得越界 panic。
        // 按页名查下标，避免增删演示页后硬编码下标失效。
        let tabs_page = super::pages::PAGES.iter().position(|p| p.name == "Tabs").expect("Tabs 页");
        page_state.set(tabs_page);
        recomp(root);
        let t = texts();
        assert!(t.iter().any(|x| x == "Home page"), "Tabs 页应显示 Home page: {t:?}");

        // 切到别的页再切回：键控作用域重建后仍应正常渲染，不继承残留状态。
        for other in [0usize, 33, 72] {
            page_state.set(other);
            recomp(root);
        }
        page_state.set(tabs_page);
        recomp(root);
        let t = texts();
        assert!(t.iter().any(|x| x == "Home page"), "切回 Tabs 页应正常显示: {t:?}");
    }

    /// 页面各段内容必须垂直排列而非堆叠：键控作用域的 Box 组节点内需再包 Column。
    /// 用 RichTextEditor 页验证：页面说明 → 编辑器正文 → 页尾查找框的 y 严格递增。
    #[test]
    fn page_content_is_stacked_vertically_not_overlapping() {
        let root = boot();
        let page_state = super::probe_state::PAGE_ST.with(|c| c.get()).unwrap();
        let rte_page = super::pages::PAGES
            .iter()
            .position(|p| p.name == "RichTextEditor")
            .expect("RichTextEditor 页");
        page_state.set(rte_page);
        recomp(root);

        let y_of = |needle: &str| -> f32 {
            compose::layout::draw_tree(root)
                .iter()
                .find_map(|c| match c {
                    compose::layout::DrawCmd::Text { text, y, .. } if text.contains(needle) => {
                        Some(*y)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("未找到文本 {needle}"))
        };
        let intro_y = y_of("工具栏：");
        let editor_y = y_of("Quill-like Editor");
        let find_y = y_of("查找:");
        assert!(
            editor_y > intro_y + 20.0,
            "编辑器应在页面说明下方（说明 {intro_y:.1}, 编辑器 {editor_y:.1}）"
        );
        assert!(
            find_y > editor_y + 20.0,
            "页尾查找框应在编辑器下方而非堆在一起（编辑器 {editor_y:.1}, 查找 {find_y:.1}）"
        );
    }

    /// 底部「输出 / 错误列表」是一组：peek 必须列出两个 tab，不能只剩 active 一个。
    #[test]
    fn bottom_peek_shows_all_group_tabs() {
        let root = boot();
        let state = super::common::shell_dock().expect("外壳应共享 DockState");
        let ids = super::common::shell_dock_ids().expect("外壳应共享面板 id");
        state.hide_to_edge(ids.output, DockEdge::Bottom);
        state.expand_from_edge(ids.output);
        recomp(root);
        let texts: Vec<String> = compose::layout::draw_tree(root)
            .iter()
            .filter_map(|c| match c {
                compose::layout::DrawCmd::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t == "输出"), "peek 应显示「输出」tab: {texts:?}");
        assert!(
            texts.iter().any(|t| t == "错误列表"),
            "peek 应显示同组的「错误列表」tab: {texts:?}"
        );
    }

    /// 在 peek 内切到「错误列表」再钉住恢复，不应残留左侧 peek 浮层遮住文档。
    #[test]
    fn switching_hidden_tab_before_restore_does_not_leave_stale_peek() {
        let root = boot();
        let state = super::common::shell_dock().expect("外壳应共享 DockState");
        let ids = super::common::shell_dock_ids().expect("外壳应共享面板 id");
        state.hide_to_edge(ids.output, DockEdge::Bottom);
        state.expand_from_edge(ids.output);
        assert_eq!(state.model().expanded, Some(ids.output));
        // 模拟在 peek 标签条点「错误列表」：active 变了，但 expanded 仍指向 output。
        state.switch_tab(ids.errors);
        assert_eq!(state.model().expanded, Some(ids.output));
        // 再点 tab 条的「钉住」：传入的是当前 active（errors）。
        state.toggle_auto_hide(ids.errors);
        assert!(
            state.model().expanded.is_none(),
            "恢复后不应残留 peek，实际 {:?}",
            state.model().expanded
        );
        recomp(root);
        assert!(!state.is_hidden(ids.errors), "错误列表应已恢复停靠");
    }

    /// 最右「属性」面板自动隐藏后恢复，必须回到最右区域而不是跑到左侧/中间。
    #[test]
    fn auto_hidden_properties_restores_to_right_side() {
        let _root = boot();
        let state = super::common::shell_dock().expect("外壳应共享 DockState");
        let ids = super::common::shell_dock_ids().expect("外壳应共享面板 id");
        let before = state
            .model()
            .root
            .clone()
            .expect("默认 IDE 布局不应为空");
        state.hide_to_edge(ids.properties, DockEdge::Right);
        assert!(state.is_hidden(ids.properties));
        state.restore_from_edge(ids.properties);
        let after = state.model().root.expect("恢复后布局不应为空");
        assert_eq!(
            after, before,
            "最小化再恢复应逐节点还原原工作区（含最右列与底部输出区）: {after:?}"
        );
        match &after {
            DockNode::Split { first, second, .. } => {
                assert!(
                    second.collect_ids().contains(&ids.properties),
                    "属性面板应回到根右侧区域，实际: {after:?}"
                );
                assert!(
                    !first.collect_ids().contains(&ids.properties),
                    "属性面板不应被塞进左侧导航组，实际: {after:?}"
                );
            }
            _ => panic!("默认 IDE 布局根应为 Split，实际: {after:?}"),
        }
    }

    /// 底部输出面板最小化→恢复后应仍在文档区下方，而不是翻到顶部。
    #[test]
    fn bottom_panel_hide_and_restore_stays_below() {
        fn is_below(node: &DockNode, id: PanelId) -> bool {
            match node {
                DockNode::Split {
                    orientation: SplitOrientation::Vertical,
                    second,
                    ..
                } if second.collect_ids().contains(&id) => true,
                DockNode::Split { first, second, .. } => {
                    is_below(first, id) || is_below(second, id)
                }
                _ => false,
            }
        }
        let root = boot();
        let state = super::common::shell_dock().unwrap();
        let ids = super::common::shell_dock_ids().unwrap();
        state.toggle_auto_hide(ids.output);
        recomp(root);
        let model = state.model();
        let h = model
            .hidden
            .iter()
            .find(|h| h.items.contains(&ids.output))
            .expect("输出面板应进入自动隐藏组");
        assert_eq!(h.edge, DockEdge::Bottom, "应隐藏到底部边缘");
        state.restore_from_edge(ids.output);
        recomp(root);
        let after = state.model().root.clone().expect("恢复后布局不应为空");
        assert!(
            is_below(&after, ids.output),
            "输出面板应恢复到文档区下方，实际: {after:?}"
        );
    }
}
