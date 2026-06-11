//! 液态玻璃壁纸回归测试（与 cpp `story_test.cpp` 9e (E) 块同源）。
//!
//! 锁住三处观感结论——它们一旦回退，「毛玻璃背景」就会重新变难看：
//! 1) 每枚光斑是**同心环近似径向衰减**（不是硬边实心圆）：半径单调递减、中心累计不透明度 = 峰值；
//! 2) 壁纸**裁到自身矩形**（`ClipPush`）：半径 `max(w,h)·0.45` 远大于节点，不裁会溢到窗口
//!    留白形成一圈「彩色画框」；
//! 3) 配色为高饱和桌面色板 + 玻璃 tint 减淡档：M3 的 `secondary`/`tertiary` 是低饱和灰调，
//!    铺底后模糊里只剩灰紫雾；厚白/灰面纱又会把彩色压成牛乳。

use compose::foundation::Constraints;
use compose::layout::{
    Box as LayoutBox, create_root_node, draw_tree, perform_layout, Alignment, DrawCmd, Modifier,
};
use compose::material::{ColorScheme, MaterialTheme, SetLiquidGlass, glass_spec};
use compose::runtime::Composition;
use compose_material::widgets::glass::GLASS_BLOB_RINGS;
use compose_material::LiquidGlassWallpaper;

/// 实际发射的环数：`i = 0`（`f = 1`）时 `want = 0`，该盘不发射。
const RINGS: usize = GLASS_BLOB_RINGS - 1;
const PEAKS: [f32; 4] = [0.55, 0.48, 0.42, 0.30];
const SIZES: [f32; 4] = [1.00, 0.85, 0.95, 0.60];
const CENTERS: [(f32, f32); 4] = [(0.18, 0.22), (0.82, 0.30), (0.55, 0.86), (0.30, 0.72)];

const W: f32 = 1200.0;
const H: f32 = 800.0;

fn wallpaper(dark: bool) -> Vec<DrawCmd> {
    let scheme = if dark {
        ColorScheme::dark()
    } else {
        ColorScheme::light()
    };
    let root = create_root_node(Alignment::TopStart);
    let comp = Composition::new(root);
    comp.set_content(move || {
        MaterialTheme(scheme, move || {
            LiquidGlassWallpaper(scheme);
        });
    });
    perform_layout(root, Constraints::tight(W, H));
    draw_tree(root)
}

fn circles(cmds: &[DrawCmd]) -> Vec<(f32, f32, f32, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            DrawCmd::FillCircle { cx, cy, r, color } => Some((*cx, *cy, *r, color.a)),
            _ => None,
        })
        .collect()
}

/// src-over 叠加后的累计不透明度（同心环的每层 alpha 就是按这个式子反推的）。
fn accumulate(alphas: &[f32]) -> f32 {
    let mut acc = 0.0f32;
    for &a in alphas {
        acc += a * (1.0 - acc);
    }
    acc
}

#[test]
fn wallpaper_is_soft_clipped_blobs() {
    let cmds = wallpaper(false);
    let cs = circles(&cmds);
    assert_eq!(
        cs.len(),
        4 * RINGS,
        "四枚光斑 × {} 层同心环（最外环 want=0 不发射）",
        RINGS
    );

    // 裁切：ClipPush 必须在圆之前（壁纸自身矩形裁掉溢出）
    let clip = cmds
        .iter()
        .position(|c| matches!(c, DrawCmd::ClipPush { .. }))
        .expect("壁纸必须裁到自身矩形，否则沿窗缘出现彩色画框");
    let first_circle = cmds
        .iter()
        .position(|c| matches!(c, DrawCmd::FillCircle { .. }))
        .unwrap();
    assert!(clip < first_circle, "ClipPush 应先于光斑（{clip} < {first_circle}）");
    assert_eq!(
        cmds.iter()
            .filter(|c| matches!(c, DrawCmd::ClipPop))
            .count(),
        1,
        "ClipPop 应与 ClipPush 配平"
    );

    let r = W.max(H) * 0.45;
    for b in 0..4 {
        let g = &cs[b * RINGS..(b + 1) * RINGS];
        // 圆心按分数位摆放（同 cpp 探针断言）
        assert!(
            (g[0].0 - W * CENTERS[b].0).abs() < 0.5 && (g[0].1 - H * CENTERS[b].1).abs() < 0.5,
            "第 {} 枚光斑圆心位错：{:?}",
            b + 1,
            g[0]
        );
        let mut prev_r = f32::INFINITY;
        let mut run = 0.0f32;
        for &(cx, cy, radius, a) in g {
            assert!(radius < prev_r, "同心环半径必须递减");
            assert!(a > 0.0 && a < 1.0, "单层增量 alpha 应在 (0,1)：{a}");
            assert_eq!((cx, cy), (g[0].0, g[0].1), "同一枚光斑同心环共用圆心");
            let before = run;
            run += a * (1.0 - run);
            assert!(run > before, "累计不透明度应由边缘向中心单调上升");
            prev_r = radius;
        }
        // 消硬边的判据是「累计」而非「单层」：最外环累计必须接近透明，
        // 否则缝隙里露出一条实心圆弧边。（单层增量反而越靠内越大——分母
        // `1 - acc` 在收缩，拿它做单调断言是错的。）
        assert!(
            g[0].3 < PEAKS[b] * 0.1,
            "最外环几乎不盖色（平滑收尾），实际 {}",
            g[0].3
        );
        // 最外发射环在 f = 1 - 1/GLASS_BLOB_RINGS
        let r_out = r * SIZES[b] * (1.0 - 1.0 / GLASS_BLOB_RINGS as f32);
        assert!(
            (g[0].2 - r_out).abs() < 1.0,
            "最外环半径应为 r·尺寸比 = {r_out}，实际 {}",
            g[0].2
        );
        assert!(
            (run - PEAKS[b]).abs() < PEAKS[b] * 0.03,
            "中心累计不透明度应收敛到峰值 {}，实际 {run}",
            PEAKS[b]
        );
    }
}

#[test]
fn wallpaper_brightens_in_dark_theme() {
    let acc_light = accumulate(
        &circles(&wallpaper(false))
            .iter()
            .take(RINGS)
            .map(|&(_, _, _, a)| a)
            .collect::<Vec<_>>(),
    );
    let cmds = wallpaper(true);
    let cs = circles(&cmds);
    assert_eq!(cs.len(), 4 * RINGS, "暗色只调亮度/浓度，不改环结构");
    let acc_dark = accumulate(&cs.iter().take(RINGS).map(|&(_, _, _, a)| a).collect::<Vec<_>>());
    // gain = 1.35（上限 0.92）：暗色主题下壁纸必须更亮，否则被 40% 深灰 tint 压成一片墨
    let want = (PEAKS[0] * 1.35).min(0.92);
    assert!(
        (acc_dark - want).abs() < want * 0.03,
        "暗色首枚峰值应为 {want}，实际 {acc_dark}"
    );
    assert!(acc_dark > acc_light, "暗色壁纸应比浅色更浓（{acc_dark} > {acc_light}）");
}

#[test]
fn glass_spec_tint_stays_translucent() {
    for (dark, want_tint) in [(false, 0.34f32), (true, 0.40)] {
        let scheme = if dark {
            ColorScheme::dark()
        } else {
            ColorScheme::light()
        };
        let root = create_root_node(Alignment::TopStart);
        let comp = Composition::new(root);
        SetLiquidGlass(true);
        comp.set_content(move || {
            MaterialTheme(scheme, move || {
                let g = glass_spec();
                LayoutBox(
                    Modifier::new().size(200.0, 120.0).glass(g),
                    Alignment::TopStart,
                    || {},
                );
            });
        });
        perform_layout(root, Constraints::tight(W, H));
        let cmds = draw_tree(root);
        let panel = cmds.iter().find_map(|c| match c {
            DrawCmd::GlassPanel {
                tint,
                highlight,
                outline,
                blur,
                line,
                radius,
                ..
            } => Some((*tint, *highlight, *outline, *blur, *line, *radius)),
            _ => None,
        });
        SetLiquidGlass(false);
        let (tint, highlight, outline, blur, line, radius) =
            panel.expect("玻璃态应产出 GlassPanel");
        assert!(
            (tint.a - want_tint).abs() < 0.01,
            "暗/浅 tint alpha 应为 {want_tint}，实际 {}",
            tint.a
        );
        // 其余材质基线与 cpp `theme.cpp::glass_spec()` 一致
        assert!((blur - 16.0).abs() < 0.01 && (line - 1.2).abs() < 0.01);
        assert!((radius - 18.0).abs() < 0.01);
        assert!(highlight.a > 0.0 && outline.a > 0.0, "高光与描边不能为全透明");
    }
}
