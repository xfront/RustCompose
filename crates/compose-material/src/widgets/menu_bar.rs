//! 库级 MenuBar（对齐 VS / Jetpack Compose 顶部菜单栏交互）：
//! - 点击标签打开浮层；
//! - **已打开时悬停其它标签直接切换菜单**（不再需要再点一次）；
//! - 点击菜单外区域关闭；点击已打开标签关闭；
//! - 菜单项选择经 `on_select(menu_index, item_index)` 回传，浮层复用库 `menu_panel`
//!   的滚动封顶、边缘重定位、悬停高亮与入场动画。
//!
//! 交互格与浮层注册都在本控件内完成，业务侧只需提供标签/项和回调。

use std::cell::RefCell;
use std::rc::Rc;

use compose::layout::{
    Alignment, Arrangement, Box as LayoutBox, Clickable, Column, CrossAxisAlignment, Modifier,
    NodeId, Row, TextStyle, hoverable, viewport_size,
};
use compose::material::theme_colors;
use compose::runtime::{MutableState, remember};

use crate::tokens;
use crate::widgets::overlay::RememberOverlayKey;
use crate::widgets::{InkWellR, Overlay, Text, menu_panel_auto_width, set_menu_callback};

/// 菜单栏单个菜单：标签 + 下拉项。
#[derive(Clone, Debug, PartialEq)]
pub struct MenuBarEntry {
    pub label: String,
    pub items: Vec<String>,
}

impl MenuBarEntry {
    pub fn new(label: impl Into<String>, items: Vec<String>) -> Self {
        Self {
            label: label.into(),
            items,
        }
    }
}

/// 菜单关闭态哨兵（与 story/业务侧的 `-1` 约定一致）。
pub const MENU_BAR_CLOSED: i8 = -1;
const CLOSED: i8 = MENU_BAR_CLOSED;
const MENU_LABEL_W: f32 = 76.0;
const MENU_BAR_H: f32 = 36.0;

/// `MenuBar(entries, modifier) { |menu, item| … }`。
///
/// `on_select` 在 GUI 主线程执行，可直接更新界面状态。
pub fn MenuBar(
    open: MutableState<i8>,
    entries: &[MenuBarEntry],
    modifier: Modifier,
    on_select: impl Fn(usize, usize) + 'static,
) {
    let colors = theme_colors();
    let key = RememberOverlayKey();
    let entries = entries.to_vec();
    let callback = Rc::new(on_select);
    let anchors: Rc<RefCell<Vec<NodeId>>> = remember(|| Rc::new(RefCell::new(Vec::new())));

    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            let entries = entries.clone();
            let anchors = anchors.clone();
            let entries_row = entries.clone();
            let anchors_row = anchors.clone();
            let anchors_overlay = anchors.clone();
            Row(
                Modifier::new().fill_max_width().height(MENU_BAR_H),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    anchors.borrow_mut().clear();
                    for (i, entry) in entries_row.iter().enumerate() {
                        let label = entry.label.clone();
                        let is_open = open.get() == i as i8;
                        let om = open;
                        let target = i as i8;
                        let mut m = Modifier::new()
                            .width(MENU_LABEL_W)
                            .padding_hv(2.0, 3.0);
                        if is_open {
                            m = m.background(colors.primary_container, 8.0);
                        }
                        let anchors_cell = anchors.clone();
                        hoverable(
                            Modifier::new().size(MENU_LABEL_W, MENU_BAR_H),
                            move |entered| {
                                // 已打开时悬停其它标签切换；关闭态悬停不打开。
                                if entered && om.get() != CLOSED {
                                    om.set(target);
                                }
                            },
                            move || {
                                let om = om;
                                let is_open = is_open;
                                let label = label.clone();
                                let mut m = m.clone();
                                if is_open {
                                    m = m.background(colors.primary_container, 8.0);
                                }
                                let id = InkWellR(
                                    m,
                                    8.0,
                                    move || om.set(if is_open { CLOSED } else { target }),
                                    move || {
                                        Text(
                                            label.clone(),
                                            TextStyle::new()
                                                .size(13.0)
                                                .weight(if is_open { 650 } else { 500 })
                                                .color(if is_open {
                                                    colors.on_primary_container
                                                } else {
                                                    colors.on_surface_variant
                                                }),
                                            Modifier::new().align(Alignment::Center),
                                        );
                                    },
                                );
                                anchors_cell.borrow_mut().push(id);
                            },
                        );
                    }
                },
            );

            // 浮层：菜单外点击关闭；顶部交互格承接 hover 切换（浮层在遮罩之上，
            // 底层标签命中会被遮罩吃掉，故交互格必须在浮层内）。
            let idx = open.get();
            let menu_idx = idx.max(0) as usize;
            let open_for_overlay = open;
            let key_overlay = key;
            let entries_overlay = entries.clone();
            let cb1 = callback.clone();
            Overlay(key_overlay, idx != CLOSED, move || {
                let cb2 = cb1.clone();
                let anchors = anchors_overlay.borrow().clone();
                let items = entries_overlay
                    .get(menu_idx)
                    .map(|e| e.items.clone())
                    .unwrap_or_default();
                let anchor = anchors.get(menu_idx).copied();
                let (_, vh) = viewport_size();
                let top_h = anchor
                    .map(|a| {
                        let (x, y, _, h) = compose::layout::node_abs_rect(a);
                        {
                            let _ = x;
                            y + h // 菜单栏下沿（遮罩从此往下）
                        }
                    })
                    .unwrap_or(MENU_BAR_H);

                // 与每个标签等位的交互格：hover 切换 + 点击切换/关闭。
                for (i, entry) in entries_overlay.iter().enumerate() {
                    let anchor_cell = anchors.get(i).copied();
                    // 布局期锚定到标签下方再上移一个栏高 = 与标签完全重合；
                    // 不能用组合期的 node_abs_rect（重组后新节点坐标尚未布局）。
                    let cell_mod = match anchor_cell {
                        Some(a) => Modifier::new()
                            .pin_to(a, 0.0, -MENU_BAR_H)
                            .size(MENU_LABEL_W, MENU_BAR_H),
                        None => Modifier::new()
                            .offset(i as f32 * MENU_LABEL_W, 0.0)
                            .size(MENU_LABEL_W, MENU_BAR_H),
                    };
                    let cell_state = open_for_overlay;
                    let click_state = open_for_overlay;
                    let target = i as i8;
                    let label = entry.label.clone();
                    hoverable(
                        cell_mod,
                        move |entered| {
                            if entered && cell_state.get() != CLOSED {
                                cell_state.set(target);
                            }
                        },
                        move || {
                            let label = label.clone();
                            Clickable(
                                Modifier::new().fill_max_size().no_indication(),
                                move || {
                                    click_state.set(if click_state.get() == target {
                                        CLOSED
                                    } else {
                                        target
                                    })
                                },
                                move || {
                                    // 交互格本体不绘制：真实文字/高亮由底层菜单栏负责。
                                    let _ = &label;
                                },
                            );
                        },
                    );
                }

                // 遮罩主体：点击下方区域关闭。菜单面板挂在其中按锚点定位。
                let items_cell = items.clone();
                let scrim_close = open_for_overlay;
                let scrim_mod = match anchor {
                    Some(a) => Modifier::new()
                        .fill_max_width()
                        .height((vh - top_h).max(0.0))
                        .pin_below(a, 0.0),
                    None => Modifier::new()
                        .fill_max_width()
                        .height((vh - top_h).max(0.0))
                        .offset(0.0, top_h),
                };
                Clickable(
                    scrim_mod.no_indication(),
                    move || scrim_close.set(CLOSED),
                    move || {
                        if let Some(a) = anchor {
                            let cb3 = cb2.clone();
                            set_menu_callback(key_overlay, move |i| {
                                let cb = cb3.clone();
                                cb(menu_idx, i);
                                open_for_overlay.set(CLOSED);
                            });
                            menu_panel_auto_width(key_overlay, a, &items_cell);
                        }
                    },
                );
            });
        },
    );
}
