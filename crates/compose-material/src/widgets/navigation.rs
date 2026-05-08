//! 导航控件 — NavigationBar / Rail / Tabs / Breadcrumb / Pagination …

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose::animation::{animate_color_as_state, animate_float_as_state};
use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Box, Column, CrossAxisAlignment, DrawBuilder, Modifier, NodeId, Policy,
    Row, ScrollInfo, Spacer, add_node, focusable_full, measure_text_with_measurer,
    with_content_scope,
};
use compose::material::{IconName, glass_spec, icon, theme_colors};
use compose::runtime::{
    MutableState, NavKey, add_frame_driver, current_scope_id, disposable_effect, focused_scope,
    mutableStateOf, remember, remove_frame_driver, with_rt_pub,
};

use crate::tokens;
use crate::widgets::overlay::{PutOverlayContent, RememberOverlayKey, call_overlay_content};
use crate::widgets::{GlassOrBg, HorizontalScroll, InkWell, Text, dropdown_menu_sections};

/// 导航条目（图标 + 标签）。
#[derive(Clone, Debug)]
pub struct NavItem {
    pub icon: IconName,
    pub label: String,
}

impl NavItem {
    pub fn new(icon: IconName, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
        }
    }
}

// ══════════════════════════════ 导航栏 / 侧栏 ══════════════════════════════

/// `NavigationBar(items, selected)` — M3 底部导航栏。
///
/// 80dp 高 `surface_container`；每个目的地 = 64×32 指示胶囊（选中 `secondary_container`，
/// 图标 24dp）+ `label_medium` 标签（选中 `on_surface`，未选 `on_surface_variant`）。
pub fn NavigationBar(items: &[NavItem], selected: MutableState<usize>, modifier: Modifier) {
    let colors = theme_colors();
    let g = glass_spec();
    let list: Vec<NavItem> = items.to_vec();
    let cur = selected.get();
    let m = GlassOrBg(modifier.fill_max_width(), g, colors.surface_container, 0.0)
        .height(tokens::NAVBAR_HEIGHT);
    // M3 用「色调高度」而非投影：NavigationBar 靠 surface_container 加深表达 elevation 3，
    // 再加矩形投影会在整条栏四周糊出一片灰。
    Row(
        m,
        Arrangement::SpaceEvenly,
        CrossAxisAlignment::Center,
        move || {
            for (i, it) in list.clone().into_iter().enumerate() {
                let on = i == cur;
                let st = selected;
                let id = add_node(
                    Policy::Column {
                        arrangement: Arrangement::Center,
                        cross: CrossAxisAlignment::Center,
                    },
                    Modifier::new().weight(1.0, false),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(i)));
                });
                with_content_scope(id, move || {
                    // M3：胶囊从 60% 长到满宽（Flutter `NavigationBar` 的 indicator 缩放动画），
                    // 底色与图标/标签颜色一起淡入。
                    let spec = tokens::state_spec();
                    let t = animate_float_as_state(if on { 1.0 } else { 0.0 }, spec);
                    let scale = tokens::NAV_INDICATOR_FROM
                        + (1.0 - tokens::NAV_INDICATOR_FROM) * t;
                    let pill = animate_color_as_state(
                        if on {
                            colors.secondary_container
                        } else {
                            Color::TRANSPARENT
                        },
                        spec,
                    );
                    let tint = animate_color_as_state(
                        if on {
                            colors.on_secondary_container
                        } else {
                            colors.on_surface_variant
                        },
                        spec,
                    );
                    let label = animate_color_as_state(
                        if on {
                            colors.on_surface
                        } else {
                            colors.on_surface_variant
                        },
                        spec,
                    );
                    let pid = add_node(
                        Policy::Box(Alignment::Center),
                        Modifier::new()
                            .width(tokens::NAV_INDICATOR_W * scale)
                            .height(tokens::NAV_INDICATOR_H)
                            .background(pill, tokens::full_radius(tokens::NAV_INDICATOR_H)),
                    );
                    with_content_scope(pid, move || {
                        icon(it.icon, tokens::ICON_DEFAULT, tint, Modifier::new());
                    });
                    Text(
                        it.label.clone(),
                        tokens::label_medium(label),
                        Modifier::new().padding_ltrb(
                            0.0,
                            tokens::SPACE_XXS,
                            0.0,
                            tokens::SPACE_XXS,
                        ),
                    );
                });
            }
        },
    );
}

/// `NavigationRail(items, selected, header)` — M3 侧边导航条（80dp 宽，56×32 胶囊）。
pub fn NavigationRail(
    items: &[NavItem],
    selected: MutableState<usize>,
    header: Option<Box<dyn Fn()>>,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let g = glass_spec();
    let list: Vec<NavItem> = items.to_vec();
    let cur = selected.get();
    let mut m = GlassOrBg(
        modifier.width(tokens::NAVBAR_HEIGHT),
        g,
        colors.surface_container,
        0.0,
    )
    .padding_ltrb(0.0, tokens::SPACE_LG, 0.0, tokens::SPACE_LG);
    if !g.enabled {
        m = m.background(colors.surface_container, 0.0);
    }
    Column(
        m.fill_max_height(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            if let Some(h) = header.as_ref() {
                h();
                Spacer(Modifier::new().height(tokens::SPACE_LG));
            }
            for (i, it) in list.clone().into_iter().enumerate() {
                let on = i == cur;
                let st = selected;
                let id = add_node(
                    Policy::Column {
                        arrangement: Arrangement::Center,
                        cross: CrossAxisAlignment::Center,
                    },
                    Modifier::new()
                        .width(tokens::NAVBAR_HEIGHT - 8.0)
                        .min_height(tokens::LIST_ONE_LINE),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(i)));
                });
                with_content_scope(id, move || {
                    // 与 NavigationBar 同一套胶囊动画（缩放淡入），指示胶囊 56×32。
                    let spec = tokens::state_spec();
                    let rail_t = animate_float_as_state(if on { 1.0 } else { 0.0 }, spec);
                    let rail_scale = tokens::NAV_INDICATOR_FROM
                        + (1.0 - tokens::NAV_INDICATOR_FROM) * rail_t;
                    let rail_pill = animate_color_as_state(
                        if on {
                            colors.secondary_container
                        } else {
                            Color::TRANSPARENT
                        },
                        spec,
                    );
                    let rail_tint = animate_color_as_state(
                        if on {
                            colors.on_secondary_container
                        } else {
                            colors.on_surface_variant
                        },
                        spec,
                    );
                    let rail_label = animate_color_as_state(
                        if on {
                            colors.on_surface
                        } else {
                            colors.on_surface_variant
                        },
                        spec,
                    );
                    let pid = add_node(
                        Policy::Box(Alignment::Center),
                        Modifier::new()
                            .width(tokens::RAIL_INDICATOR_W * rail_scale)
                            .height(tokens::NAV_INDICATOR_H)
                            .background(rail_pill, tokens::full_radius(tokens::NAV_INDICATOR_H)),
                    );
                    with_content_scope(pid, move || {
                        icon(it.icon, tokens::ICON_DEFAULT, rail_tint, Modifier::new());
                    });
                    Text(
                        it.label.clone(),
                        tokens::label_medium(rail_label),
                        Modifier::new().padding_ltrb(
                            0.0,
                            tokens::SPACE_XXS,
                            0.0,
                            tokens::SPACE_XXS,
                        ),
                    );
                });
                if i + 1 < list.len() {
                    Spacer(Modifier::new().height(tokens::SPACE_SM));
                }
            }
        },
    );
}

/// `NavigationDrawer(items, selected)` — M3 抽屉：256dp `surface_container`，
/// 56dp 胶囊条目（选中 `secondary_container` + 24dp 图标 + `label_large`）。
pub fn NavigationDrawer(
    title: &str,
    items: &[NavItem],
    selected: MutableState<usize>,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let g = glass_spec();
    let list: Vec<NavItem> = items.to_vec();
    let cur = selected.get();
    let t = title.to_string();
    let m = GlassOrBg(
        modifier.width(tokens::DRAWER_WIDTH),
        g,
        colors.surface_container,
        0.0,
    )
    .padding_ltrb(
        tokens::SPACE_XS,
        tokens::SPACE_SM,
        tokens::SPACE_XS,
        tokens::SPACE_SM,
    )
    .fill_max_height();
    Column(
        m,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            Text(
                t.clone(),
                tokens::title_medium(colors.on_surface),
                Modifier::new().padding_ltrb(
                    tokens::SPACE_MD,
                    tokens::SPACE_XS,
                    0.0,
                    tokens::SPACE_LG,
                ),
            );
            for (i, it) in list.clone().into_iter().enumerate() {
                let on = i == cur;
                let st = selected;
                // 抽屉条目是整宽胶囊：只做配色淡入（Flutter `NavigationDrawer` 无缩放动画）。
                let dspec = tokens::state_spec();
                let d_bg = animate_color_as_state(
                    if on {
                        colors.secondary_container
                    } else {
                        Color::TRANSPARENT
                    },
                    dspec,
                );
                let d_tint = animate_color_as_state(
                    if on {
                        colors.on_secondary_container
                    } else {
                        colors.on_surface_variant
                    },
                    dspec,
                );
                let d_label = animate_color_as_state(
                    if on {
                        colors.on_secondary_container
                    } else {
                        colors.on_surface
                    },
                    dspec,
                );
                let row_mod = Modifier::new()
                    .fill_max_width()
                    .height(tokens::MENU_ITEM_HEIGHT + 8.0)
                    .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0)
                    .background(
                        d_bg,
                        tokens::full_radius(tokens::MENU_ITEM_HEIGHT + 8.0),
                    );
                let id = add_node(
                    Policy::Row {
                        arrangement: Arrangement::Start,
                        cross: CrossAxisAlignment::Center,
                    },
                    row_mod,
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(i)));
                });
                with_content_scope(id, move || {
                    icon(it.icon, tokens::ICON_DEFAULT, d_tint, Modifier::new());
                    Spacer(Modifier::new().width(tokens::SPACE_LG));
                    Text(it.label.clone(), tokens::label_large(d_label), Modifier::new());
                });
                Spacer(Modifier::new().height(tokens::SPACE_XXS));
            }
        },
    );
}

/// 侧栏分组（folder）。
#[derive(Clone, Debug, Default)]
pub struct SidebarSection {
    /// 组标题（`None` 为无标题组）。
    pub title: Option<String>,
    pub items: Vec<NavItem>,
}

impl SidebarSection {
    /// 有标题分组。
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            items: Vec::new(),
        }
    }
    /// 无标题分组（条目直接平铺）。
    pub fn untitled() -> Self {
        Self {
            title: None,
            items: Vec::new(),
        }
    }
    /// 追加条目（链式）。
    pub fn item(mut self, icon: IconName, label: impl Into<String>) -> Self {
        self.items.push(NavItem::new(icon, label));
        self
    }
}

thread_local! {
    /// type-ahead 缓冲（Sidebar 聚焦时逐字前缀搜索，1s 超时清空）。
    static TA_BUF: RefCell<String> = const { RefCell::new(String::new()) };
    static TA_LAST_MS: Cell<u128> = const { Cell::new(0) };
}

fn ta_now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 定位包含全局序号 `flat` 的分组下标。
fn section_of(secs: &[SidebarSection], flat: usize) -> Option<usize> {
    let mut f = 0usize;
    for (si, sec) in secs.iter().enumerate() {
        if flat < f + sec.items.len() {
            return Some(si);
        }
        f += sec.items.len();
    }
    None
}

/// 有效折叠态：**含选中项的分组强制展开**（保证选中项始终可见）。
fn effective_collapsed(
    collapsed: &[bool],
    secs: &[SidebarSection],
    sel_sec: Option<usize>,
) -> Vec<bool> {
    (0..secs.len())
        .map(|si| collapsed.get(si).copied().unwrap_or(false) && Some(si) != sel_sec)
        .collect()
}

/// 在 `si` 组之外挑一个可见条目（折叠含选中项的分组前迁移选中用）。
fn pick_visible_outside(
    secs: &[SidebarSection],
    eff: &[bool],
    si: usize,
    sel: usize,
) -> Option<usize> {
    let mut f = 0usize;
    let mut before: Option<usize> = None;
    let mut after: Option<usize> = None;
    for (i, sec) in secs.iter().enumerate() {
        let start = f;
        let end = f + sec.items.len();
        f = end;
        if i == si || eff.get(i).copied().unwrap_or(false) {
            continue;
        }
        if end <= sel {
            before = Some(end - 1);
        } else if after.is_none() {
            after = Some(start);
        }
    }
    after.or(before)
}

/// `Sidebar(sections, selected, collapsed, rail)` — 分组侧栏（folder）。
///
/// * `rail = true` 时进入**图标条（最小化）**模式：分组标题退化为细分隔线，条目只显示居中
///   图标（隐藏文字标签），所有分组强制展开；容器宽度由宿主收窄。
///
/// - 分组标题行：左箭头 + 组名 + 右侧数量徽标；点击折叠/展开，折叠组内条目隐藏。
/// - 条目：图标 + 标签缩进绘制；点击选中（跨组连续序号，即页面下标）。
/// - **选中项始终可见**：含选中项的分组强制展开；折叠该分组时先把选中迁移到其它可见条目；
///   内容区滚动偏移在布局期自动校正，保证选中项进入视口（不影响用户手动滚动）。
/// - 内容区可鼠标滚动；↑/↓/PageUp/PageDown 在**可见**条目间移动（折叠组自动跳过）；
///   聚焦时敲字符逐字搜索（type-ahead），跳到首个前缀匹配条目（1s 超时重置）。
/// - 聚焦时绘制主色外框；点击内部条目不抢焦点（整栏持焦）。
pub fn Sidebar(
    sections: &[SidebarSection],
    selected: MutableState<usize>,
    collapsed: MutableState<Vec<bool>>,
    rail: MutableState<bool>,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let g = glass_spec();
    let secs: Vec<SidebarSection> = sections.to_vec();
    let cur = selected.get();
    let collapsed_now = collapsed.read_clone();
    let rail_on = rail.get();
    let st_sel = selected;
    let st_col = collapsed;

    // 含选中项的分组强制展开 → 选中项始终可见
    let sel_sec = section_of(&secs, cur);
    let eff = effective_collapsed(&collapsed_now, &secs, sel_sec);

    // 可见条目的全局连续序号（折叠组跳过）+ 对应小写标签（type-ahead 用）
    let mut vis: Vec<usize> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut flat = 0usize;
    for (si, sec) in secs.iter().enumerate() {
        // 图标条模式：所有分组强制展开（条目都以图标呈现）
        let col = !rail_on && eff.get(si).copied().unwrap_or(false);
        for it in &sec.items {
            if !col {
                vis.push(flat);
                labels.push(it.label.to_lowercase());
            }
            flat += 1;
        }
    }
    let vis_key = vis;
    let vis_ta = vis_key.clone();
    let labels_ta = labels;

    let m = GlassOrBg(modifier, g, colors.surface_container, 0.0).fill_max_height();

    focusable_full(
        m,
        || {},
        move |key| {
            if vis_key.is_empty() {
                return;
            }
            let cur = st_sel.get();
            let pos = vis_key
                .iter()
                .position(|&i| i == cur)
                .or_else(|| vis_key.iter().position(|&i| i > cur))
                .unwrap_or(0);
            let last = vis_key.len() - 1;
            let np = match key {
                NavKey::Up => pos.saturating_sub(1),
                NavKey::Down => (pos + 1).min(last),
                NavKey::Left | NavKey::Right => pos,
                NavKey::PageUp => pos.saturating_sub(8),
                NavKey::PageDown => (pos + 8).min(last),
                NavKey::Home => 0,
                NavKey::End => last,
                NavKey::Delete => pos,
            };
            st_sel.set(vis_key[np]);
        },
        Some(move |ch: char| {
            let now = ta_now_ms();
            let q = TA_BUF.with(|b| {
                let mut buf = b.borrow_mut();
                if now.saturating_sub(TA_LAST_MS.with(|l| l.get())) > 1000 {
                    buf.clear();
                }
                buf.push(ch);
                TA_LAST_MS.with(|l| l.set(now));
                buf.to_lowercase()
            });
            if vis_ta.is_empty() {
                return;
            }
            // 从当前选中之后找前缀匹配，找不到再从头循环
            let cur = st_sel.get();
            let start = vis_ta
                .iter()
                .position(|&i| i == cur)
                .map(|p| p + 1)
                .unwrap_or(0);
            let n = vis_ta.len();
            for k in 0..n {
                let p = (start + k) % n;
                if labels_ta[p].starts_with(&q) {
                    st_sel.set(vis_ta[p]);
                    break;
                }
            }
        }),
        move || {
            let focused = focused_scope()
                .zip(current_scope_id())
                .map(|(a, b)| a == b)
                .unwrap_or(false);
            // 内层 'static 闭包所需数据克隆（外层 Fn 闭包捕获不可 move 出）
            let secs = secs.clone();
            let eff = eff.clone();
            // 滚动容器（内联而非 VerticalScroll：需在内容组合期登记 sticky 目标节点）
            // 滚动偏移跨重组保留：节点每帧重建，偏移存在节点上会随重组（如点选 item）
            // 被清零 → 已滚动的项被判到视口外、被 sticky 校正误拉到底部。
            // 故用 remember 保住偏移，并注册一次性帧驱动把实时节点偏移同步回来。
            let scroll_off = remember(|| Rc::new(Cell::new(0.0f32)));
            let scroll_id_cell = remember(|| Rc::new(Cell::new(None::<NodeId>)));
            let scroll_id = add_node(
                Policy::Column {
                    arrangement: Arrangement::Start,
                    cross: CrossAxisAlignment::Stretch,
                },
                Modifier::new()
                    .fill_max_width()
                    .fill_max_height()
                    .VerticalScroll(),
            );
            with_rt_pub(|rt| {
                rt.nodes[scroll_id.index()].scroll = Some(ScrollInfo {
                    vertical: true,
                    offset: scroll_off.get(),
                    content_size: 0.0,
                    sticky: None,
                });
            });
            scroll_id_cell.set(Some(scroll_id));
            // 一次性注册：每帧把实时滚动节点的偏移写回 remember（手动滚动 → 重组不丢位）
            let off_c = scroll_off.clone();
            let id_c = scroll_id_cell.clone();
            let scroll_drv = remember(|| {
                add_frame_driver(move || {
                    if let Some(id) = id_c.get() {
                        let v = with_rt_pub(|rt| {
                            rt.nodes[id.index()]
                                .scroll
                                .as_ref()
                                .map(|s| s.offset)
                                .unwrap_or(0.0)
                        });
                        off_c.set(v);
                    }
                })
            });
            // 作用域销毁时注销帧驱动，避免驱动随滚动控件消失后继续驻留
            disposable_effect(0).on_dispose(move || remove_frame_driver(scroll_drv));
            with_content_scope(scroll_id, move || {
                let secs_h = secs.clone();
                let eff_h = eff.clone();
                let mut flat = 0usize;
                for (si, sec) in secs.iter().enumerate() {
                    let col = !rail_on && eff.get(si).copied().unwrap_or(false);
                    if rail_on {
                        // 图标条模式：分组标题退化为一条细分隔线
                        if sec.title.is_some() {
                            Spacer(Modifier::new().height(tokens::SPACE_MD));
                            add_node(
                                Policy::Leaf,
                                Modifier::new().fill_max_width().height(1.0).background(
                                    colors.outline_variant,
                                    0.0,
                                ),
                            );
                            Spacer(Modifier::new().height(tokens::SPACE_MD));
                        }
                    } else if let Some(title) = &sec.title {
                        let t2 = title.clone();
                        let st2 = st_col;
                        let si2 = si;
                        let n_items = sec.items.len();
                        let sh = secs_h.clone();
                        let eh = eff_h.clone();
                        let sel_sec_h = sel_sec;
                        let st_sel_h = st_sel;
                        // 分组之间留间隔，避免相邻 folder 色带紧贴（首个分组上方已有栏头，不加）
                        if si > 0 {
                            Spacer(Modifier::new().height(tokens::SPACE_SM));
                        }
                        InkWell(
                            Modifier::new()
                                .fill_max_width()
                                .height(tokens::SPACE_XXXL)
                                // 分组标题（folder）用主题色半透明色带，与中性色条目（item）明显区分
                                .background(colors.primary.with_alpha(0.35), 8.0)
                                .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0),
                            move || {
                                // 折叠含选中项的分组：先把选中迁移到其它可见条目
                                //（保证选中项始终可见，折叠动作也能生效）
                                if sel_sec_h == Some(si2) {
                                    if let Some(t) =
                                        pick_visible_outside(&sh, &eh, si2, st_sel_h.get())
                                    {
                                        st_sel_h.set(t);
                                    }
                                }
                                st2.update(|v| {
                                    while v.len() <= si2 {
                                        v.push(false);
                                    }
                                    v[si2] = !v[si2];
                                });
                            },
                            move || {
                                let t3 = t2.clone();
                                let Badge = n_items.to_string();
                                Row(
                                    Modifier::new().fill_max_width(),
                                    Arrangement::Start,
                                    CrossAxisAlignment::Center,
                                    move || {
                                        icon(
                                            if col {
                                                IconName::ChevronRight
                                            } else {
                                                IconName::ChevronDown
                                            },
                                            tokens::ICON_SMALL,
                                            colors.on_surface_variant,
                                            Modifier::new(),
                                        );
                                        Spacer(Modifier::new().width(tokens::SPACE_XS));
                                        Text(
                                            t3.clone(),
                                            // 分组标题（folder）文字加粗，与条目（item）进一步区分
                                            tokens::label_large(colors.on_surface_variant).weight(700),
                                            Modifier::new().weight(1.0, false),
                                        );
                                        // 数量徽标
                                        let Badge = Badge.clone();
                                        let bid = add_node(
                                            Policy::Box(Alignment::Center),
                                            Modifier::new().padding_ltrb(
                                                tokens::SPACE_XS,
                                                0.0,
                                                tokens::SPACE_XS,
                                                0.0,
                                            ),
                                        );
                                        with_content_scope(bid, move || {
                                            Text(
                                                Badge.as_str(),
                                                tokens::label_small(colors.on_surface_variant),
                                                Modifier::new(),
                                            );
                                        });
                                    },
                                );
                            },
                        );
                    }
                    if !col {
                        for it in &sec.items {
                            let gi = flat;
                            flat += 1;
                            let on = gi == cur;
                            let st3 = st_sel;
                            let lbl = it.label.clone();
                            let ic = it.icon;
                            // M3 抽屉条目：4dp 侧边距 + 56dp 胶囊（选中 secondary_container）
                            let iid = add_node(
                                Policy::Box(Alignment::CenterStart),
                                Modifier::new().fill_max_width().padding_ltrb(
                                    tokens::SPACE_XS,
                                    tokens::SPACE_XXS,
                                    tokens::SPACE_XS,
                                    tokens::SPACE_XXS,
                                ),
                            );
                            with_content_scope(iid, move || {
                                if rail_on {
                                    // 图标条：固定宽度胶囊水平居中，图标在胶囊内居中
                                    // （InkWell 内容按 TopStart 落位，若胶囊 fill_max_width
                                    //  则图标会贴左；故收窄为定宽 + align(Center) 居中）。
                                    let pill = Modifier::new()
                                        .width(40.0)
                                        .height(tokens::MENU_ITEM_HEIGHT)
                                        .align(Alignment::Center)
                                        .background(
                                            if on {
                                                colors.secondary_container
                                            } else {
                                                Color::TRANSPARENT
                                            },
                                            tokens::full_radius(tokens::MENU_ITEM_HEIGHT),
                                        );
                                    InkWell(
                                        pill,
                                        move || st3.set(gi),
                                        move || {
                                            let bid = add_node(
                                                Policy::Box(Alignment::Center),
                                                Modifier::new().fill_max_size(),
                                            );
                                            with_content_scope(bid, move || {
                                                icon(
                                                    ic,
                                                    tokens::LIST_ICON_SIZE,
                                                    if on {
                                                        colors.on_secondary_container
                                                    } else {
                                                        colors.on_surface_variant
                                                    },
                                                    Modifier::new(),
                                                );
                                            });
                                        },
                                    );
                                    return;
                                }
                                let l2 = lbl.clone();
                                let pill = Modifier::new()
                                    .fill_max_width()
                                    .height(tokens::MENU_ITEM_HEIGHT + 8.0)
                                    .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0)
                                    .background(
                                        if on {
                                            colors.secondary_container
                                        } else {
                                            Color::TRANSPARENT
                                        },
                                        tokens::full_radius(tokens::MENU_ITEM_HEIGHT + 8.0),
                                    );
                                InkWell(
                                    pill,
                                    move || st3.set(gi),
                                    move || {
                                        let l3 = l2.clone();
                                        Row(
                                            // 撑满胶囊高度：InkWell 内容按 TopStart 落位，
                                            // 若 Row 只取内容高度会被钉在顶部 → 图标/文字无法垂直居中。
                                            Modifier::new().fill_max_height(),
                                            Arrangement::Start,
                                            CrossAxisAlignment::Center,
                                            move || {
                                                icon(
                                                    ic,
                                                    tokens::LIST_ICON_SIZE,
                                                    if on {
                                                        colors.on_secondary_container
                                                    } else {
                                                        colors.on_surface_variant
                                                    },
                                                    Modifier::new(),
                                                );
                                                Spacer(Modifier::new().width(tokens::SPACE_MD));
                                                Text(
                                                    l3.clone(),
                                                    tokens::label_large(if on {
                                                        colors.on_secondary_container
                                                    } else {
                                                        colors.on_surface
                                                    }),
                                                    Modifier::new(),
                                                );
                                            },
                                        );
                                    },
                                );
                            });
                            // 选中项登记为 sticky：仅在越出视口时最小幅度滚动到刚好可见，已在视口内则不动
                            if on {
                                with_rt_pub(|rt| {
                                    if let Some(sc) = rt.nodes[scroll_id.index()].scroll.as_mut() {
                                        sc.sticky = Some(iid);
                                    }
                                });
                            }
                        }
                    } else {
                        flat += sec.items.len();
                    }
                }
            });
            // 聚焦外框：覆盖整栏，不参与命中
            if focused {
                add_node(
                    Policy::Leaf,
                    Modifier::new()
                        .fill_max_size()
                        .border(2.0, colors.primary, 0.0)
                        .no_hit(),
                );
            }
        },
    );
}

// ══════════════════════════════ Tabs ══════════════════════════════

/// 标签条（横向滚动区 + 右侧"更多"下拉溢出菜单）。
///
/// * `more = true` 时右侧渲染 `⋮` 按钮，**仅当有标签溢出当前视口才出现**；点击弹出下拉，
///   只列出**不在视口内**的标签：右侧溢出的在上、左侧溢出的在下，中间一条分隔条（对齐 rui
///   `tab_overflow` 语义）。全部标签都装得下时连 `⋮` 都不画。
/// * 溢出成员由一次性帧驱动在布局就绪后按各标签几何（`node.x` / 滚动偏移 / 视口宽）计算，
///   滚动 / 窗口缩放 / 选中（sticky 把选中项带入视口）都会触发重算；`MutableState::set`
///   同值 no-op，故静止即停驱、不空转。
/// * M3 `PrimaryTabRow`：标签 48dp 高、水平内边距 16dp、文字 `label_large`，
///   选中指示条 3dp（宽度 = 标签文字宽）贴底，未选中不绘制但保留高度（避免跳动）。
fn tab_strip(titles: &[String], selected: MutableState<usize>, modifier: Modifier, more: bool) {
    let colors = theme_colors();
    let list: Vec<String> = titles.to_vec();
    let cur = selected.get();
    // 各标签节点句柄（每次组合刷新，供帧驱动在组合外读几何）+ 溢出索引 `(右溢出, 左溢出)`。
    let tab_ids: Rc<RefCell<Vec<NodeId>>> = remember(|| Rc::new(RefCell::new(Vec::new())));
    let overflow: MutableState<(Vec<usize>, Vec<usize>)> =
        remember(|| mutableStateOf((Vec::new(), Vec::new())));
    // 上一次生效的选中下标：仅当选择**变化**时才把选中标签标为滚动 sticky（避免每帧
    // 重组都校正而干扰用户手动横向滚动）。
    let last_sel: Rc<Cell<usize>> = remember(|| Rc::new(Cell::new(usize::MAX)));
    // 待滚入视口的选中下标：组合期只登记，帧驱动在树稳定后用当前节点设置 sticky
    // （组合期一次变更可能多次重建节点，直接拿 NodeId 设 sticky 会被后续重建孤立）。
    let pending_sel: MutableState<Option<usize>> = remember(|| mutableStateOf(None));
    // 键导航：聚焦标签条后用 ↑/↓/←/→ ±1、PageUp/Down ±5、Home/End 切换选中；
    // 选中变化后既有 sticky 逻辑（下方 content 内）自动把选中标签滚入视口。
    let tab_count = list.len();
    focusable_full(
        modifier,
        || {},
        move |key| {
            if tab_count == 0 {
                return;
            }
            let last = tab_count - 1;
            let cur = selected.get();
            let np = match key {
                NavKey::Up | NavKey::Left => cur.saturating_sub(1),
                NavKey::Down | NavKey::Right => (cur + 1).min(last),
                NavKey::PageUp => cur.saturating_sub(5),
                NavKey::PageDown => (cur + 5).min(last),
                NavKey::Home => 0,
                NavKey::End => last,
                NavKey::Delete => cur,
            };
            selected.set(np);
        },
        None::<fn(char)>,
        move || {
            // focusable_full 的 content 为 `Fn`：下面 Row 的内容闭包需 `move` 拥有
            // list/tab_ids/last_sel，故先在本帧克隆为局部所有权，再交给内层闭包。
            let list = list.clone();
            let tab_ids = tab_ids.clone();
            let last_sel = last_sel.clone();
            Row(
                Modifier::new()
                    .fill_max_width()
                    .height(tokens::TAB_HEIGHT)
                    .background(colors.Surface, 0.0),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
            // 横向可滚动标签区（不显滚动条，选中项靠 sticky 自动滚入视口）
            let scroll_list = list.clone();
            let ids_cell = tab_ids.clone();
            let last_sel_c = last_sel.clone();
            let pend_c = pending_sel;
            HorizontalScroll(
                Modifier::new()
                    .weight(1.0, false)
                    .fill_max_height()
                    .hide_scrollbar(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    ids_cell.borrow_mut().clear();
                    for (i, t) in scroll_list.clone().into_iter().enumerate() {
                        let on = i == cur;
                        let st = selected;
                        let id = add_node(
                            Policy::Box(Alignment::Center),
                            Modifier::new().fill_max_height().padding_ltrb(
                                tokens::SPACE_XXL,
                                0.0,
                                tokens::SPACE_XXL,
                                0.0,
                            ),
                        );
                        ids_cell.borrow_mut().push(id);
                        with_rt_pub(|rt| {
                            rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(i)));
                        });
                        with_content_scope(id, move || {
                            // M3：指示条从 0 长到标签宽（Flutter `PrimaryTabRowIndicator` 的
                            // `animateFloatAsState`），标签颜色同时淡入 on_surface。
                            let spec = tokens::state_spec();
                            let ind_t = animate_float_as_state(if on { 1.0 } else { 0.0 }, spec);
                            let fg = animate_color_as_state(
                                if on {
                                    colors.on_surface
                                } else {
                                    colors.on_surface_variant
                                },
                                spec,
                            );
                            let style = tokens::label_large(fg);
                            let tw = measure_text_with_measurer(&t, &style).0;
                            Text(t.clone(), style, Modifier::new().align(Alignment::Center));
                            add_node(
                                Policy::Leaf,
                                Modifier::new()
                                    .align(Alignment::BottomCenter)
                                    .width(tw * ind_t)
                                    .height(tokens::TAB_INDICATOR)
                                    .background(
                                        colors.primary,
                                        tokens::full_radius(tokens::TAB_INDICATOR),
                                    ),
                            );
                        });
                    }
                    // 选中项始终可见：选择**变化**时登记待滚动下标，由下方帧驱动在
                    // **树稳定后**用当前 `tab_ids` 设置滚动 sticky。组合期一次变更可能
                    // 多次重建节点，直接在此拿 NodeId 设 sticky 会被后续重建孤立，
                    // 导致（尤其从下拉菜单）选中的标签不滚入视口。
                    if last_sel_c.replace(cur) != cur {
                        pend_c.set(Some(cur));
                        compose::animation::request_anim_frames();
                    }
                },
            );
            // 帧驱动：树稳定后把 pending 的选中标签设为滚动 sticky，布局阶段
            // `update_scroll_content` 会最小幅度校正偏移使其进入视口（一次性）。
            let pend_drv = {
                let ids_drv = tab_ids.clone();
                remember(|| {
                    add_frame_driver(move || {
                        let Some(i) = pend_c.peek_mut(|p| *p) else {
                            return;
                        };
                        let ids = ids_drv.borrow().clone();
                        if let Some(tid) = ids.get(i).copied() {
                            with_rt_pub(|rt| {
                                if let Some(sc) = rt.nodes[tid.index()].parent {
                                    if let Some(s) = rt.nodes[sc.index()].scroll.as_mut() {
                                        s.sticky = Some(tid);
                                    }
                                }
                            });
                        }
                        pend_c.set(None);
                        compose::animation::request_anim_frames();
                    })
                })
            };
            disposable_effect(0).on_dispose(move || remove_frame_driver(pend_drv));
            // 右侧"更多"下拉（仅在有标签溢出视口时出现）
            if more {
                let key = RememberOverlayKey();
                let open = remember(|| mutableStateOf(false));
                // 一次性帧驱动：读布局几何算溢出 → 变化才写回，并追加一帧让「⋮ 出现后
                // 视口收缩→更多标签溢出」这一步重新收敛（monotone，几步后即稳定停驱）。
                let ids2 = tab_ids.clone();
                let ov2 = overflow;
                let drv = remember(|| {
                    add_frame_driver(move || {
                        let ids = ids2.borrow().clone();
                        if ids.is_empty() {
                            return;
                        }
                        match tab_overflow(&ids) {
                            None => compose::animation::request_anim_frames(),
                            Some(nv) => {
                                let changed = ov2.peek_mut(|c| *c != nv);
                                if changed {
                                    ov2.set(nv);
                                    compose::animation::request_anim_frames();
                                }
                            }
                        }
                    })
                });
                disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
                let (right, left) = overflow.read_clone();
                if !right.is_empty() || !left.is_empty() {
                    let more_id = add_node(
                        Policy::Box(Alignment::Center),
                        Modifier::new()
                            .width(tokens::CONTROL_HEIGHT)
                            .fill_max_height(),
                    );
                    with_rt_pub(|rt| {
                        rt.nodes[more_id.index()].on_click =
                            Some(Box::new(move || open.update(|v| *v = !*v)));
                    });
                    with_content_scope(more_id, move || {
                        icon(
                            IconName::MoreVert,
                            tokens::ICON_DEFAULT,
                            colors.on_surface_variant,
                            Modifier::new(),
                        );
                    });
                    // 溢出菜单：右段在前（上）、左段在后（下），段间由 `Sep` 分隔。
                    // `on_selected` 收到跨段扁平下标，映射回原标签索引再 `selected.set`。
                    let rcnt = right.len();
                    let rmap = right.clone();
                    let lmap = left.clone();
                    let sections = vec![
                        right.iter().map(|&i| list[i].clone()).collect::<Vec<String>>(),
                        left.iter().map(|&i| list[i].clone()).collect::<Vec<String>>(),
                    ];
                    dropdown_menu_sections(more_id, key, &sections, open, move |flat| {
                        let tab = if flat < rcnt { rmap[flat] } else { lmap[flat - rcnt] };
                        selected.set(tab);
                    });
                }
            }
        },
    );
        },
    );
}

/// 读各标签几何算出溢出当前视口的标签索引 `(右溢出升序, 左溢出降序)`。
///
/// 判定以滚动容器的视口 `[offset, offset+viewport]` 为准：完全位于右界之外为「右溢出」，
/// 完全位于左界之前为「左溢出」（反向，使靠内的左溢出紧邻分隔条），与视口有交叠者视为
/// 可见、不入菜单。返回 `None` = 布局尚未就绪（视口或标签尺寸还没测出），调用方据此再要一帧。
fn tab_overflow(ids: &[NodeId]) -> Option<(Vec<usize>, Vec<usize>)> {
    with_rt_pub(|rt| {
        let sc = rt.nodes[ids[0].index()].parent?;
        let sn = &rt.nodes[sc.index()];
        let viewport = sn.width;
        let offset = sn.scroll.as_ref().map(|s| s.offset).unwrap_or(0.0);
        if viewport <= 1.0 {
            return None;
        }
        let vis_l = offset;
        let vis_r = offset + viewport;
        let mut right = Vec::new();
        let mut left = Vec::new();
        let mut measured = false;
        for (i, &id) in ids.iter().enumerate() {
            let t = &rt.nodes[id.index()];
            if t.width <= 0.0 {
                continue;
            }
            measured = true;
            if t.x >= vis_r - 0.5 {
                right.push(i);
            } else if t.x + t.width <= vis_l + 0.5 {
                left.push(i);
            }
        }
        if !measured {
            return None;
        }
        left.reverse();
        Some((right, left))
    })
}

/// `TabRow(titles, selected)` — Compose `TabRow`（选中下划线指示 + 溢出下拉）。
pub fn TabRow(titles: &[String], selected: MutableState<usize>, modifier: Modifier) {
    tab_strip(titles, selected, modifier, true);
}

/// `Carousel(page, count, indicator) { content }` — 分页轮播。
pub fn Carousel(
    page: MutableState<usize>,
    count: usize,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let colors = theme_colors();
    let cur = page.get();
    let ck = RememberOverlayKey();
    PutOverlayContent(ck, content);
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Box(
                Modifier::new().fill_max_width().weight(1.0, true),
                Alignment::TopStart,
                move || call_overlay_content(ck),
            );
            Spacer(Modifier::new().height(tokens::SPACE_LG));
            Row(
                Modifier::new(),
                Arrangement::Center,
                CrossAxisAlignment::Center,
                move || {
                    for i in 0..count {
                        let on = i == cur;
                        let st = page;
                        let id = add_node(Policy::Leaf, Modifier::new().size(16.0, 16.0));
                        with_rt_pub(|rt| {
                            rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(i)));
                            rt.nodes[id.index()].draw =
                                Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
                                    b.fill_circle(
                                        x + 8.0,
                                        y + 8.0,
                                        if on { 5.0 } else { 3.5 },
                                        if on {
                                            colors.primary
                                        } else {
                                            colors.outline_variant
                                        },
                                    );
                                }));
                        });
                    }
                },
            );
        },
    );
}

// ══════════════════════════════ 面包屑 / 分页 / 栏 ══════════════════════════════

/// `Breadcrumb(items, onNavigate)` — M3 面包屑：`label_large`，当前页 `on_surface`，
/// 其余 `primary` 可点，分隔符 18dp `on_surface_variant`。
pub fn Breadcrumb(items: &[String], on_navigate: Option<Box<dyn Fn(usize)>>, modifier: Modifier) {
    let colors = theme_colors();
    let list: Vec<String> = items.to_vec();
    let key = crate::widgets::overlay::RememberOverlayKey();
    if let Some(f) = on_navigate {
        crate::widgets::overlay::PutIndexedAction(key, f);
    }
    Row(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for (i, it) in list.clone().into_iter().enumerate() {
                if i > 0 {
                    icon(
                        IconName::ChevronRight,
                        tokens::ICON_SMALL,
                        colors.on_surface_variant,
                        Modifier::new().padding_ltrb(tokens::SPACE_XS, 0.0, tokens::SPACE_XS, 0.0),
                    );
                }
                let last = i + 1 == list.len();
                let id = add_node(
                    Policy::Leaf,
                    Modifier::new().padding_ltrb(
                        tokens::SPACE_XS,
                        tokens::SPACE_XS,
                        tokens::SPACE_XS,
                        tokens::SPACE_XS,
                    ),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || {
                        crate::widgets::overlay::RunIndexedAction(key, i)
                    }));
                    let s = it.clone();
                    rt.nodes[id.index()].draw =
                        Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
                            b.draw_text(
                                x,
                                y,
                                &s,
                                &tokens::label_large(if last {
                                    colors.on_surface
                                } else {
                                    colors.primary
                                }),
                                f32::INFINITY,
                            );
                        }));
                });
            }
        },
    );
}

/// `Pagination(page, pageCount)` — 分页控件：40dp 圆形触控目标，选中 `secondary_container`。
pub fn Pagination(page: MutableState<usize>, page_count: usize, modifier: Modifier) {
    let colors = theme_colors();
    let cur = page.get();
    Row(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let prev = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new().size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
            );
            with_rt_pub(|rt| {
                rt.nodes[prev.index()].on_click =
                    Some(Box::new(move || page.update(|v| *v = v.saturating_sub(1))));
            });
            with_content_scope(prev, move || {
                icon(
                    IconName::ChevronLeft,
                    tokens::ICON_DEFAULT,
                    colors.on_surface_variant,
                    Modifier::new(),
                );
            });
            for i in 0..page_count.min(9) {
                let on = i == cur;
                let id = add_node(
                    Policy::Box(Alignment::Center),
                    Modifier::new()
                        .size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT)
                        .background(
                            if on {
                                colors.secondary_container
                            } else {
                                Color::TRANSPARENT
                            },
                            tokens::full_radius(tokens::CONTROL_HEIGHT),
                        ),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || page.set(i)));
                    let s = format!("{}", i + 1);
                    rt.nodes[id.index()].draw =
                        Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                            crate::widgets::basic::CenteredText(
                                b,
                                x,
                                y,
                                w,
                                h,
                                &s,
                                &tokens::label_large(if on {
                                    colors.on_secondary_container
                                } else {
                                    colors.on_surface_variant
                                }),
                            );
                        }));
                });
            }
            let next = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new().size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
            );
            with_rt_pub(|rt| {
                rt.nodes[next.index()].on_click = Some(Box::new(move || {
                    page.update(|v| *v = (*v + 1).min(page_count - 1))
                }));
            });
            with_content_scope(next, move || {
                icon(
                    IconName::ChevronRight,
                    tokens::ICON_DEFAULT,
                    colors.on_surface_variant,
                    Modifier::new(),
                );
            });
        },
    );
}

/// `Toolbar(actions)` — 工具条（动作所有权移入，回调经索引动作表派发）。
pub fn Toolbar(actions: Vec<crate::widgets::layout::AppBarAction>, modifier: Modifier) {
    let colors = theme_colors();
    let tk = crate::widgets::overlay::RememberOverlayKey();
    let icons: Vec<IconName> = actions.iter().map(|a| a.icon).collect();
    crate::widgets::overlay::PutIndexedAction(tk, move |i: usize| {
        if let Some(a) = actions.get(i) {
            (a.on_click)();
        }
    });
    Row(
        modifier
            .fill_max_width()
            .height(tokens::CONTROL_HEIGHT + 4.0)
            .background(colors.surface_container, 0.0)
            .padding_ltrb(tokens::SPACE_XS, 0.0, tokens::SPACE_XS, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for (i, ic) in icons.clone().into_iter().enumerate() {
                let id = add_node(
                    Policy::Box(Alignment::Center),
                    Modifier::new().size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || {
                        crate::widgets::overlay::RunIndexedAction(tk, i)
                    }));
                });
                with_content_scope(id, move || {
                    icon(
                        ic,
                        tokens::ICON_DEFAULT,
                        colors.on_surface_variant,
                        Modifier::new(),
                    );
                });
            }
        },
    );
}

/// `TitleBar(title)` — 窗口标题条。
pub fn TitleBar(title: &str, modifier: Modifier) {
    let colors = theme_colors();
    let t = title.to_string();
    Row(
        modifier
            .fill_max_width()
            .height(tokens::TITLEBAR_HEIGHT)
            .background(colors.surface_container, 0.0)
            .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text(
                t.clone(),
                tokens::label_large(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

/// `StatusBar(left, right)` — 底部状态条。
pub fn StatusBar(left: &str, right: &str, modifier: Modifier) {
    let colors = theme_colors();
    let l = left.to_string();
    let r = right.to_string();
    Row(
        modifier
            .fill_max_width()
            .height(tokens::STATUSBAR_HEIGHT)
            .background(colors.surface_container, 0.0)
            .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_MD, 0.0),
        Arrangement::SpaceBetween,
        CrossAxisAlignment::Center,
        move || {
            Text(
                l.clone(),
                tokens::label_small(colors.on_surface_variant),
                Modifier::new(),
            );
            Text(
                r.clone(),
                tokens::label_small(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

// ════════════════════ Tabs 控制器 / Accordion（navigation 补齐） ════════════════════

/// `Tabs(titles, selected, page)` — 标签栏 + 页面视图（`TabBar`+`TabBarView`）。
///
/// 页面内容只在选中页组合（`page(index)` 单次调用），切换即重组换页。
pub fn Tabs(
    titles: &[String],
    selected: MutableState<usize>,
    page: impl Fn(usize) + 'static,
    modifier: Modifier,
) {
    let titles: Vec<String> = titles.to_vec();
    let cur = selected.get();
    let st = selected;
    Column(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            // 标签栏（横向滚动 + 右侧"更多"下拉）
            tab_strip(&titles, st, Modifier::new(), true);
            // 页面视图
            let idx = cur.min(titles.len().saturating_sub(1));
            page(idx);
        },
    );
}

/// `Accordion(titles, expanded, page)` — 手风琴（多开，点击标题切换）。
pub fn Accordion(
    titles: &[String],
    expanded: MutableState<Vec<usize>>,
    page: impl Fn(usize) + 'static,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let titles: Vec<String> = titles.to_vec();
    let open = expanded.read_clone();
    let st = expanded;
    // 每段的内容作用域闭包都要能调 `page` → 用 `Rc` 共享（compose-material 允许 `Rc`）。
    let page = Rc::new(page);
    Column(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            for (i, t) in titles.iter().enumerate() {
                let on = open.contains(&i);
                let s = st;
                let idx = i;
                let tt = t.clone();
                let s_click = s;
                // M3 `AnimatedVisibility(expandVertically)`：内容按自然高度测量，
                // 外层节点高度按进度折算并裁剪；箭头同时连续旋转 180°。
                let spec = if on {
                    tokens::enter_spec()
                } else {
                    tokens::exit_spec()
                };
                let frac = animate_float_as_state(if on { 1.0 } else { 0.0 }, spec);
                let pg = page.clone();
                InkWell(
                    Modifier::new()
                        .fill_max_width()
                        .min_height(tokens::LIST_ONE_LINE)
                        .padding_ltrb(
                            tokens::LIST_PAD_H,
                            tokens::SPACE_MD,
                            tokens::LIST_PAD_H,
                            tokens::SPACE_MD,
                        )
                        .background(colors.surface_container, 0.0),
                    move || {
                        s_click.update(|v| {
                            if v.contains(&idx) {
                                v.retain(|x| *x != idx);
                            } else {
                                v.push(idx);
                            }
                        });
                    },
                    move || {
                        let tt2 = tt.clone();
                        Row(
                            Modifier::new(),
                            Arrangement::SpaceBetween,
                            CrossAxisAlignment::Center,
                            move || {
                                Text(
                                    tt2.clone(),
                                    tokens::body_large(colors.on_surface),
                                    Modifier::new().weight(1.0, false),
                                );
                                crate::widgets::RotatingChevron(
                                    frac,
                                    tokens::ICON_DEFAULT,
                                    colors.on_surface_variant,
                                    Modifier::new(),
                                );
                            },
                        );
                    },
                );
                if on || frac > 0.001 {
                    let k = i;
                    let wid = add_node(
                        Policy::Column {
                            arrangement: Arrangement::Start,
                            cross: CrossAxisAlignment::Stretch,
                        },
                        Modifier::new()
                            .fill_max_width()
                            .reveal_height(frac)
                            .clip(0.0),
                    );
                    with_content_scope(wid, move || pg(k));
                }
            }
        },
    );
}

// ════════════════════ PageView / Ribbon / SidebarHeader（navigation 补齐） ════════════════════

/// `PageView(current, count, auto_play_ms, page) { }` — 页面视图。
///
/// 一次只显示一页（按 `current` 索引）；底部圆点指示器可点击切换；
/// `auto_play_ms > 0` 时按间隔自动轮播。
pub fn PageView(
    current: MutableState<usize>,
    count: usize,
    auto_play_ms: f32,
    modifier: Modifier,
    page: impl Fn(usize) + 'static,
) {
    let idx = current.get();
    let st = current;
    // 自动轮播：用 `Instant` 计间隔（单调墙钟），并**只在下一次切换时定点唤醒**。
    // 旧实现按帧号累计（随帧率漂移）且每帧 `request_anim_frames`（开着轮播即满帧率重绘）；
    // 对齐 Compose `LaunchedEffect { while (true) { delay(interval); … } }` 的节奏。
    if auto_play_ms > 0.0 && count > 1 {
        let cur2 = st;
        let period = std::time::Duration::from_secs_f32((auto_play_ms / 1000.0).max(0.05));
        let last = remember(|| std::rc::Rc::new(std::cell::Cell::new(std::time::Instant::now())));
        let drv = remember(move || {
            add_frame_driver(move || {
                let now = std::time::Instant::now();
                let elapsed = now.duration_since(last.get());
                if elapsed >= period {
                    last.set(now);
                    let n = cur2.get();
                    cur2.set((n + 1) % count);
                }
                let remain = period.saturating_sub(now.duration_since(last.get()));
                compose::animation::request_anim_frame_after(remain.as_secs_f32());
            })
        });
        disposable_effect(0).on_dispose(move || remove_frame_driver(drv));
    }
    let colors = theme_colors();
    // 页面内容直接绘制在根盒（裁剪到整框）；圆点指示器叠加在底部。
    Box(
        modifier.fill_max_size().clip(0.0),
        Alignment::TopStart,
        move || {
            page(idx);
            // 圆点指示器（底部居中叠加）
            let idx1 = idx;
            let st1 = st;
            let cnt = count;
            let colors1 = colors;
            let did = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new()
                    .align(Alignment::BottomCenter)
                    .fill_max_width()
                    .height(20.0),
            );
            with_content_scope(did, move || {
                Row(
                    Modifier::new(),
                    Arrangement::Center,
                    CrossAxisAlignment::Center,
                    move || {
                        for i in 0..cnt {
                            let on = i == idx1;
                            let s = st1;
                            let di = i;
                            let id = add_node(
                                Policy::Leaf,
                                Modifier::new().size(12.0, 12.0).background(
                                    if on {
                                        colors1.primary
                                    } else {
                                        colors1.outline_variant
                                    },
                                    tokens::full_radius(12.0),
                                ),
                            );
                            with_rt_pub(|rt| {
                                rt.nodes[id.index()].on_click = Some(Box::new(move || s.set(di)));
                            });
                            Spacer(Modifier::new().width(tokens::SPACE_SM));
                        }
                    },
                );
            });
        },
    );
}

/// `Ribbon(Tabs, selected, panel) { }` — 功能区带。
///
/// 顶部标签条（选中标签下方主色指示条）+ 下面板区（选中标签对应的内容）。
pub fn Ribbon(
    Tabs: &[String],
    selected: MutableState<usize>,
    modifier: Modifier,
    panel: impl Fn(usize) + 'static,
) {
    let colors = theme_colors();
    let Tabs: Vec<String> = Tabs.to_vec();
    let cur = selected.get();
    let st = selected;
    Column(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            // 标签条
            let tabs1 = Tabs.clone();
            let st1 = st;
            let cur1 = cur;
            let colors1 = colors;
            Row(
                Modifier::new()
                    .fill_max_width()
                    .height(tokens::CONTROL_HEIGHT)
                    .background(colors.surface_container, 0.0),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    for (i, t) in tabs1.clone().into_iter().enumerate() {
                        let on = i == cur1;
                        let s = st1;
                        let idx = i;
                        let id = add_node(
                            Policy::Column {
                                arrangement: Arrangement::Center,
                                cross: CrossAxisAlignment::Center,
                            },
                            Modifier::new().padding_ltrb(
                                tokens::SPACE_XXL,
                                0.0,
                                tokens::SPACE_XXL,
                                0.0,
                            ),
                        );
                        with_rt_pub(|rt| {
                            rt.nodes[id.index()].on_click = Some(Box::new(move || s.set(idx)));
                        });
                        with_content_scope(id, move || {
                            Text(
                                t.clone(),
                                tokens::label_large(if on {
                                    colors1.primary
                                } else {
                                    colors1.on_surface_variant
                                }),
                                Modifier::new(),
                            );
                        });
                    }
                },
            );
            // 指示条
            add_node(
                Policy::Leaf,
                Modifier::new()
                    .fill_max_width()
                    .height(2.0)
                    .background(colors.primary, 0.0),
            );
            // 面板（直接在组合闭包内调用，使其内容成为本列子节点；根节点自填剩余空间）
            let idx = cur.min(Tabs.len().saturating_sub(1));
            panel(idx);
        },
    );
}

/// `SidebarHeader(title, collapsed, on_toggle) { }` — 侧栏固定标题栏。
///
/// 置于滚动区**之外**（如侧栏 Column 首行）：左侧标题、右侧折叠/展开切换按钮（«/»）；
/// 点击调用 `on_toggle`（宿主据此折叠/展开全部分组）。`collapsed` 仅用于决定按钮字形。
pub fn SidebarHeader(
    title: &str,
    collapsed: bool,
    on_toggle: impl Fn() + 'static,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let t = title.to_string();
    let m = GlassOrBg(
        modifier
            .fill_max_width()
            .height(tokens::CONTROL_HEIGHT + 4.0)
            .padding_ltrb(tokens::SPACE_MD, 0.0, tokens::SPACE_XS, 0.0),
        glass_spec(),
        colors.surface_container,
        0.0,
    );
    let id = add_node(
        Policy::Row {
            arrangement: Arrangement::SpaceBetween,
            cross: CrossAxisAlignment::Center,
        },
        m,
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(on_toggle));
    });
    with_content_scope(id, move || {
        Text(
            t.clone(),
            tokens::title_medium(colors.on_surface),
            Modifier::new().weight(1.0, false),
        );
        // 切换按钮（40dp 触控目标 + 24dp 图标）
        let bid = add_node(
            Policy::Box(Alignment::Center),
            Modifier::new().size(tokens::CONTROL_HEIGHT, tokens::CONTROL_HEIGHT),
        );
        with_content_scope(bid, move || {
            icon(
                if collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronLeft
                },
                tokens::ICON_DEFAULT,
                colors.on_surface_variant,
                Modifier::new(),
            );
        });
    });
}
