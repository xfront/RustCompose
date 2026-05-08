//! 诊断：story 的 MenuBar 下拉浮层几何（是否落在菜单栏下方且可见）。

use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Arrangement, Box as CBox, Clickable, Column, CrossAxisAlignment, Modifier, NodeId,
    Row, TextStyle, create_root_node, dispatch_click_at, hit_test, perform_layout,
};
use compose::material::{ColorScheme, MaterialTheme};
use compose::runtime::{Composer, Composition, Recomposer, mutableStateOf, with_rt_pub};
use compose_material::{InkWell, Overlay, OverlayHost, Text};

const MENUS: &[&str] = &["File", "View", "Theme", "Help"];
const MENU_W: f32 = 76.0;
const MENU_H: f32 = 36.0;
const CLOSED: i8 = -1;
const TITLE_H: f32 = 36.0;

fn build() -> (
    NodeId,
    compose::runtime::MutableState<i8>,
    compose::runtime::MutableState<usize>,
) {
    let root = create_root_node(Alignment::TopStart);
    let open_menu = mutableStateOf(CLOSED);
    let page = mutableStateOf(0usize);
    let comp = Composition::new(root);
    let (om, pg) = (open_menu, page);
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                Column(
                    Modifier::new().fill_max_size(),
                    Arrangement::Start,
                    CrossAxisAlignment::Stretch,
                    move || {
                        // 标题栏
                        Column(
                            Modifier::new().fill_max_width().height(TITLE_H),
                            Arrangement::Start,
                            CrossAxisAlignment::Stretch,
                            move || {
                                Text("title".to_string(), TextStyle::new(), Modifier::new());
                            },
                        );
                        // 菜单栏
                        Row(
                            Modifier::new().fill_max_width().height(MENU_H),
                            Arrangement::Start,
                            CrossAxisAlignment::Stretch,
                            move || {
                                for (i, label) in MENUS.iter().enumerate() {
                                    let open = om.get() == i as i8;
                                    let om2 = om;
                                    InkWell(
                                        Modifier::new().width(MENU_W),
                                        move || om2.set(if open { CLOSED } else { i as i8 }),
                                        move || {
                                            Text(
                                                (*label).to_string(),
                                                TextStyle::new(),
                                                Modifier::new(),
                                            );
                                        },
                                    );
                                }
                            },
                        );
                        // 内容区（weight 1）
                        CBox(
                            Modifier::new().weight(1.0, true),
                            Alignment::TopStart,
                            move || {
                                Column(
                                    Modifier::new().fill_max_size(),
                                    Arrangement::Start,
                                    CrossAxisAlignment::Stretch,
                                    move || {
                                        Row(
                                            Modifier::new().fill_max_size().padding_hv(12.0, 8.0),
                                            Arrangement::Start,
                                            CrossAxisAlignment::Stretch,
                                            move || {
                                                Column(
                                                    Modifier::new().width(220.0).fill_max_height(),
                                                    Arrangement::Start,
                                                    CrossAxisAlignment::Stretch,
                                                    move || {
                                                        Text(
                                                            "sidebar".to_string(),
                                                            TextStyle::new(),
                                                            Modifier::new(),
                                                        );
                                                    },
                                                );
                                                Column(
                                                    Modifier::new()
                                                        .weight(1.0, true)
                                                        .fill_max_height(),
                                                    Arrangement::Start,
                                                    CrossAxisAlignment::Stretch,
                                                    move || {
                                                        Text(
                                                            "content".to_string(),
                                                            TextStyle::new(),
                                                            Modifier::new(),
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                        Overlay(0x4D45_4E55, om.get() != CLOSED, move || {
                                            let idx = om.get();
                                            // 下拉：绝对定位到菜单栏下方
                                            Column(
                                                Modifier::new()
                                                    .offset(
                                                        idx as f32 * MENU_W + 4.0,
                                                        TITLE_H + MENU_H + 2.0,
                                                    )
                                                    .width(220.0)
                                                    .background(
                                                        compose::foundation::Color::WHITE,
                                                        10.0,
                                                    )
                                                    .padding_hv(0.0, 4.0),
                                                Arrangement::Start,
                                                CrossAxisAlignment::Stretch,
                                                move || {
                                                    for i in 0..3 {
                                                        let (om3, pg3) = (om, pg);
                                                        InkWell(
                                                            Modifier::new().padding_hv(12.0, 8.0),
                                                            move || {
                                                                pg3.set(i);
                                                                om3.set(CLOSED);
                                                            },
                                                            move || {
                                                                Text(
                                                                    format!("item {i}"),
                                                                    TextStyle::new(),
                                                                    Modifier::new(),
                                                                );
                                                            },
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                    },
                                );
                            },
                        );
                    },
                );
            });
        });
    });
    perform_layout(root, Constraints::tight(800.0, 600.0));
    (root, open_menu, page)
}

/// 下拉浮层必须落在菜单栏正下方且可见（不能成为内容区的流式兄弟被挤出视口）。
#[test]
fn dropdown_is_visible_under_menu_bar() {
    let (root, open_menu, page) = build();
    // 点击 "View" 标签（第二个菜单）
    let x = MENU_W * 1.0 + MENU_W / 2.0;
    let y = TITLE_H + MENU_H / 2.0;
    let (target, _, _) = hit_test(root, x, y).expect("应命中菜单栏");
    println!("[dbg] hit menu target={:?}", target.0);
    assert!(
        compose::layout::dispatch_click_at(target, x, y),
        "点击应被消费"
    );
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(800.0, 600.0));
    println!("[dbg] open_menu={} page={}", open_menu.get(), page.get());

    // 打印下拉浮层及其首项的绝对位置
    with_rt_pub(|rt| {
        fn dump(rt: &compose::runtime::RuntimeData, id: NodeId, depth: usize, ax: f32, ay: f32) {
            let n = &rt.nodes[id.index()];
            let (x, y) = (ax + n.x, ay + n.y);
            println!(
                "{}node {:?} abs=({:.1},{:.1}) {}x{} kids={}",
                "  ".repeat(depth),
                id.0,
                x,
                y,
                n.width,
                n.height,
                n.children.len()
            );
            for ch in &n.children {
                dump(rt, *ch, depth + 1, x, y);
            }
        }
        dump(rt, root, 0, 0.0, 0.0);
    });

    // 浮层应落在菜单栏正下方、高度非零且在视口内
    let (dx, dy, dw, dh, kids) = with_rt_pub(|rt| {
        // 浮层内容 = OverlayHost 的最后一个孩子 → 其首个孩子即下拉 Column
        let host = rt.nodes[root.index()].children[0];
        let host = rt.nodes[host.index()].children[0];
        let overlay = *rt.nodes[host.index()].children.last().unwrap();
        // 浮层包装 → 其首个孩子即带 offset 的下拉 Column
        let col = rt.nodes[overlay.index()].children[0];
        let n = &rt.nodes[col.index()];
        (n.x, n.y, n.width, n.height, n.children.len())
    });
    println!("[dbg] dropdown abs=({dx},{dy}) {dw}x{dh} kids={kids}");
    assert!(dh > 10.0, "下拉浮层高度应可见，实际 {dh}");
    assert!(
        (dx - (MENU_W + 4.0)).abs() < 1.0,
        "应水平对齐第 2 个菜单：dx={dx}"
    );
    assert!(
        (dy - (TITLE_H + MENU_H + 2.0)).abs() < 1.0,
        "应紧贴在菜单栏下方：dy={dy}"
    );
    assert!(dy + dh < 600.0, "浮层应完整落在视口内");
    assert_eq!(kids, 3, "下拉列应含 3 个菜单项");

    // 点击第一个菜单项 → 执行动作并关闭
    let (it, _, _) = hit_test(root, MENU_W + 40.0, TITLE_H + MENU_H + 20.0).expect("应命中菜单项");
    assert!(
        compose::layout::dispatch_click_at(it, 0.0, 0.0),
        "菜单项点击应被消费"
    );
    Recomposer::recompose_pending();
    assert_eq!(page.get(), 0, "点击首项应执行动作");
    assert_eq!(open_menu.get(), CLOSED, "点击后应关闭菜单");
}

// ═══════════════════ gallery.rs 结构（无标题栏）═══════════════════

/// gallery.rs 的壳：菜单栏 + 内容区（`Row(fill_max_size)` 占满）+ 走 `Overlay` 的下拉。
fn build_gallery() -> (NodeId, compose::runtime::MutableState<i8>) {
    let root = create_root_node(Alignment::TopStart);
    let open_menu = mutableStateOf(CLOSED);
    let comp = Composition::new(root);
    let (om, pg) = (open_menu, mutableStateOf(0usize));
    comp.set_content(move || {
        MaterialTheme(ColorScheme::light(), move || {
            OverlayHost(Modifier::new().fill_max_size(), move || {
                Column(
                    Modifier::new().fill_max_size(),
                    Arrangement::Start,
                    CrossAxisAlignment::Stretch,
                    move || {
                        // 菜单栏
                        Row(
                            Modifier::new().fill_max_width().height(MENU_H),
                            Arrangement::Start,
                            CrossAxisAlignment::Stretch,
                            move || {
                                for (i, label) in MENUS.iter().enumerate() {
                                    let open = om.get() == i as i8;
                                    let om2 = om;
                                    InkWell(
                                        Modifier::new().width(MENU_W),
                                        move || om2.set(if open { CLOSED } else { i as i8 }),
                                        move || {
                                            Text(
                                                (*label).to_string(),
                                                TextStyle::new(),
                                                Modifier::new(),
                                            );
                                        },
                                    );
                                }
                            },
                        );
                        // 内容区（weight 1）
                        CBox(
                            Modifier::new().weight(1.0, true),
                            Alignment::TopStart,
                            move || {
                                let (om2, pg2) = (om, pg);
                                Column(
                                    Modifier::new().fill_max_size(),
                                    Arrangement::Start,
                                    CrossAxisAlignment::Stretch,
                                    move || {
                                        Row(
                                            Modifier::new().fill_max_size().padding_hv(12.0, 8.0),
                                            Arrangement::Start,
                                            CrossAxisAlignment::Stretch,
                                            move || {
                                                Text(
                                                    "content".to_string(),
                                                    TextStyle::new(),
                                                    Modifier::new(),
                                                );
                                            },
                                        );
                                        // 下拉走 Overlay → 叠在宿主最上层，不被内容 Row 挤压
                                        Overlay(0x4D45_4E55, om2.get() != CLOSED, move || {
                                            let idx = om2.get();
                                            let (om3, pg3) = (om2, pg2);
                                            Clickable(
                                                Modifier::new().fill_max_size(),
                                                {
                                                    let cl = om3.clone();
                                                    move || cl.set(CLOSED)
                                                },
                                                move || {
                                                    Column(
                                                        Modifier::new()
                                                            .width(200.0)
                                                            .background(
                                                                compose::foundation::Color::WHITE,
                                                                10.0,
                                                            )
                                                            .offset(
                                                                idx as f32 * MENU_W + 4.0,
                                                                MENU_H + 2.0,
                                                            )
                                                            .padding_hv(0.0, 4.0),
                                                        Arrangement::Start,
                                                        CrossAxisAlignment::Stretch,
                                                        move || {
                                                            for i in 0..3 {
                                                                let (om4, pg4) = (om3, pg3);
                                                                InkWell(
                                                                    Modifier::new()
                                                                        .padding_hv(12.0, 8.0),
                                                                    move || {
                                                                        pg4.set(i);
                                                                        om4.set(CLOSED);
                                                                    },
                                                                    move || {
                                                                        Text(
                                                                            format!("item {i}"),
                                                                            TextStyle::new(),
                                                                            Modifier::new(),
                                                                        );
                                                                    },
                                                                );
                                                            }
                                                        },
                                                    );
                                                },
                                            );
                                        });
                                    },
                                );
                            },
                        );
                    },
                );
            });
        });
    });
    perform_layout(root, Constraints::tight(800.0, 600.0));
    (root, open_menu)
}

/// 下拉必须真正可见：高度非零、落在菜单栏下方、完整位于视口内。
///
/// 回归缺陷：下拉若作为内容区 `Column` 的流式兄弟，会被同列的 `Row(fill_max_size)`
/// 挤到剩余空间为 0 —— 高度 0 且 y 落到视口外，表现为「点了菜单项没反应」。
#[test]
fn gallery_dropdown_is_not_squeezed_to_zero_height() {
    let (root, open_menu) = build_gallery();

    // 点击第 2 个菜单（View）
    let x = MENU_W + MENU_W / 2.0;
    let y = MENU_H / 2.0;
    let (target, _, _) = hit_test(root, x, y).expect("应命中菜单栏");
    assert!(dispatch_click_at(target, x, y), "点击应被消费");
    Recomposer::recompose_pending();
    perform_layout(root, Constraints::tight(800.0, 600.0));
    assert_eq!(open_menu.get(), 1, "菜单应已打开");

    // 浮层 = OverlayHost 的最后一个孩子（包装）→ clickable 背景 → 带 offset 的下拉列
    let (dx, dy, dw, dh, kids) = with_rt_pub(|rt| {
        let host = rt.nodes[root.index()].children[0];
        let host = rt.nodes[host.index()].children[0];
        let overlay = *rt.nodes[host.index()].children.last().unwrap();
        let backdrop = rt.nodes[overlay.index()].children[0];
        let col = rt.nodes[backdrop.index()].children[0];
        let n = &rt.nodes[col.index()];
        (n.x, n.y, n.width, n.height, n.children.len())
    });
    println!("[dbg] gallery dropdown abs=({dx},{dy}) {dw}x{dh} kids={kids}");
    assert!(dh > 10.0, "下拉高度应可见，实际 {dh}");
    assert!(dw > 10.0, "下拉宽度应可见，实际 {dw}");
    assert!(
        (dy - (MENU_H + 2.0)).abs() < 1.0,
        "应紧贴在菜单栏下方：dy={dy}"
    );
    assert!(
        (dx - (MENU_W + 4.0)).abs() < 1.0,
        "应水平对齐第 2 个菜单：dx={dx}"
    );
    assert!(dy + dh < 600.0, "下拉应完整落在视口内");
    assert_eq!(kids, 3, "下拉列应含 3 个菜单项");

    // 点击下拉之外 → 背景层关闭菜单
    let (bg, _, _) = hit_test(root, 700.0, 500.0).expect("应命中背景");
    assert!(dispatch_click_at(bg, 0.0, 0.0), "背景点击应被消费");
    Recomposer::recompose_pending();
    assert_eq!(open_menu.get(), CLOSED, "点击外部应关闭菜单");
}
