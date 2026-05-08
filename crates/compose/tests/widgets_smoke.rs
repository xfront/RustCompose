//! 控件集冒烟测试 — 无窗口环境验证浮层宿主与液态玻璃材质切换。

use compose::foundation::Constraints;
use compose::layout::{
    create_root_node, draw_tree, perform_layout, Alignment, DrawCmd, Modifier, TextStyle,
};
use compose::material::{MaterialTheme, SetLiquidGlass, ToggleLiquidGlass, ColorScheme};
use compose::runtime::{Composer, Composition, Recomposer};
use compose_material::{
    FilledCard, Overlay, OverlayHost, RememberOverlayKey, Text,
};

fn texts(cmds: &[DrawCmd]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn has_glass(cmds: &[DrawCmd]) -> bool {
    cmds.iter().any(|c| matches!(c, DrawCmd::GlassPanel { .. }))
}

#[test]
fn overlay_host_composes_registered_overlay() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                let key = RememberOverlayKey();
                Text("base", TextStyle::new(), Modifier::new());
                Overlay(key, true, move || {
                    Text("OVERLAY", TextStyle::new(), Modifier::new());
                });
            });
        });
    });

    perform_layout(root, Constraints::tight(400.0, 300.0));
    let cmds = draw_tree(root);
    let t = texts(&cmds);
    assert!(t.iter().any(|s| s == "base"), "基础内容未绘制: {t:?}");
    assert!(t.iter().any(|s| s == "OVERLAY"), "浮层内容未绘制: {t:?}");
}

#[test]
fn liquid_glass_toggle_switches_material() {
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            FilledCard(Modifier::new().size(200.0, 120.0), move || {
                Text("glass", TextStyle::new(), Modifier::new());
            });
        });
    });

    // 1) 默认关闭 → 无 GlassPanel
    SetLiquidGlass(false);
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    assert!(!has_glass(&draw_tree(root)), "默认不应使用玻璃材质");

    // 2) 一个函数切换 → 定向重组后走毛玻璃
    assert!(ToggleLiquidGlass(), "ToggleLiquidGlass 应返回 true（已开启）");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    assert!(has_glass(&draw_tree(root)), "开启后应产出 GlassPanel");

    // 3) 再切一次 → 回到普通底色
    assert!(!ToggleLiquidGlass(), "再次切换应返回 false（已关闭）");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(400.0, 300.0));
    assert!(!has_glass(&draw_tree(root)), "关闭后不应再有 GlassPanel");
}
