//! 数据展示控件 — List / Table / Tree / Chart / Progress / Stepper …

use compose::animation::animate_float_as_state;
use compose::foundation::Color;
use compose::layout::{
    Alignment, Arrangement, Box, Canvas, Column, CrossAxisAlignment, DrawBuilder, Modifier, NodeId,
    Policy, Row, Spacer, add_node, focusable_full, measure_text_with_measurer, with_content_scope,
};
use compose::material::{IconName, glass_spec, icon, theme_colors};
use compose::runtime::{MutableState, NavKey, mutableStateOf, remember, with_rt_pub};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::tokens;
use crate::widgets::{GlassOrBg, InkWell, Text, TextField, VerticalScroll, nav_index};

// ══════════════════════════════ 列表 ══════════════════════════════

/// `ListItem(headline, supporting, leading, trailing, onClick)` — Compose `ListItem`。
pub fn ListItem(
    headline: &str,
    supporting: Option<&str>,
    leading: Option<IconName>,
    trailing: Option<&str>,
    modifier: Modifier,
    on_click: Option<Box<dyn Fn()>>,
) {
    ListItemR(headline, supporting, leading, trailing, modifier, on_click);
}

/// [`ListItem`] 的带句柄版本（先例：`InkWell` / `InkWellR`）：返回行节点 `NodeId`，
/// 供列表容器把它登记为滚动 sticky 目标（选中项自动滚入视口）。
pub fn ListItemR(
    headline: &str,
    supporting: Option<&str>,
    leading: Option<IconName>,
    trailing: Option<&str>,
    modifier: Modifier,
    on_click: Option<Box<dyn Fn()>>,
) -> NodeId {
    let colors = theme_colors();
    let head = headline.to_string();
    let sup = supporting.map(|s| s.to_string());
    let tail = trailing.map(|s| s.to_string());
    let two_line = sup.is_some();
    // M3 `ListItem`：单行 56dp / 双行 64dp，左右 16dp，前后各 24dp 图标（40dp 容器）
    let m = modifier
        .fill_max_width()
        .padding_ltrb(tokens::LIST_PAD_H, 0.0, tokens::LIST_PAD_H, 0.0)
        .min_height(if two_line {
            tokens::LIST_TWO_LINE
        } else {
            tokens::LIST_ONE_LINE
        });
    let id = add_node(
        Policy::Row {
            arrangement: Arrangement::Start,
            cross: CrossAxisAlignment::Center,
        },
        m,
    );
    if let Some(f) = on_click {
        with_rt_pub(|rt| {
            rt.nodes[id.index()].on_click = Some(f);
        });
    }
    with_content_scope(id, move || {
        let h2 = head.clone();
        let sp2 = sup.clone();
        if let Some(n) = leading {
            // 40dp 图标容器（M3 `ListItem` 的 leading 槽）
            let box_id = add_node(
                Policy::Box(Alignment::Center),
                Modifier::new()
                    .size(tokens::LIST_ICON_BOX, tokens::LIST_ICON_BOX)
                    .padding_ltrb(tokens::SPACE_XS, 0.0, tokens::SPACE_XS, 0.0),
            );
            with_content_scope(box_id, move || {
                icon(
                    n,
                    tokens::LIST_ICON_SIZE,
                    colors.on_surface_variant,
                    Modifier::new(),
                );
            });
            Spacer(Modifier::new().width(tokens::SPACE_LG));
        }
        Column(
            Modifier::new().weight(1.0, false),
            Arrangement::Center,
            CrossAxisAlignment::Start,
            move || {
                Text(
                    h2.clone(),
                    tokens::body_large(colors.on_surface),
                    Modifier::new(),
                );
                if let Some(s) = sp2.clone() {
                    Text(
                        s,
                        tokens::body_medium(colors.on_surface_variant),
                        Modifier::new(),
                    );
                }
            },
        );
        if let Some(t) = tail.clone() {
            Spacer(Modifier::new().width(tokens::SPACE_LG));
            Text(
                t,
                tokens::body_small(colors.on_surface_variant),
                Modifier::new(),
            );
        }
    });
    id
}

/// `List(items, selected)` — 非滚动纵向列表，开箱即用的「整行点击选择」。
///
/// 与 [`ListView`] 共用选中语义（`secondary_container` 高亮）和键盘步长
/// （↑/↓ ±1、PageUp/Down ±8、Home/End），但**不带滚动容器**：适合条目较少、
/// 由外层负责滚动的场景。调用方只需传入 `MutableState<usize>`，不必自己手拼
/// [`ListItem`] 再逐个挂 `on_click`。
///
/// 「整行」包括内边距：[`ListItemR`] 的行节点是 `fill_max_width` + 左右
/// padding，`on_click` 挂在**行节点**而非文字子节点上；`dispatch_click_at`
/// 沿祖先链找最近的 `on_click`，所以点击中间文字、左右 padding、上下留白
/// 都会归属同一行并选中它（行内文字叶子没有自己的点击处理器）。
pub fn List(items: &[String], selected: MutableState<usize>, modifier: Modifier) {
    let items: Vec<String> = items.to_vec();
    let cur = selected.get();
    let n = items.len();
    let nav_sel = selected;
    focusable_full(
        modifier,
        || {},
        move |key| {
            let c = nav_sel.read_unregistered();
            nav_sel.set(nav_index(key, c, n));
        },
        None::<fn(char)>,
        move || {
            let list = items.clone();
            // focusable_full 是 Box 语义（子树重叠），行必须收进 Column 依次排布。
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let colors = theme_colors();
                    for (i, it) in list.clone().into_iter().enumerate() {
                        let on = i == cur;
                        let st = selected;
                        ListItemR(
                            &it,
                            None,
                            None,
                            None,
                            Modifier::new().background(
                                // M3 选中列表项 = `secondary_container`
                                if on {
                                    colors.secondary_container
                                } else {
                                    Color::TRANSPARENT
                                },
                                tokens::SHAPE_SMALL,
                            ),
                            Some(Box::new(move || st.set(i))),
                        );
                    }
                },
            );
        },
    );
}

/// 行外壳：整行填充宽 + 选中高亮（`secondary_container`）+ `on_click` 设 `selected`，
/// 行内内容由 `content` 闭包绘制（可含图标/副标题/任意 widget）。`ListCustom` /
/// `ListViewCustom` 共用，保证「整行（含 padding 与留白）点击选中」语义一致。
///
/// `on_click` 挂在**行节点**而非内容子节点上；`dispatch_click_at` 沿祖先链找最近的
/// `on_click`，所以点击中间文字、左右 padding、上下留白都归属同一行。
fn list_row(index: usize, selected: MutableState<usize>, content: impl Fn() + 'static) -> NodeId {
    let on = index == selected.get();
    let st = selected;
    let idx = index;
    let colors = theme_colors();
    let id = add_node(
        Policy::Box(Alignment::TopStart),
        Modifier::new().fill_max_width().background(
            if on {
                colors.secondary_container
            } else {
                Color::TRANSPARENT
            },
            tokens::SHAPE_SMALL,
        ),
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(idx)));
    });
    with_content_scope(id, content);
    id
}

/// `ListCustom(count, selected, modifier, item_content)` — Jetpack Compose 风格的可自定义列表。
///
/// 与 [`List`] 共用选中语义（`secondary_container` 高亮）与键盘步长，但每行内容由
/// `item_content(index)` 绘制，调用方可放任意 widget（图标、副标题、自定义布局），
/// 不再受 `String` 限制。例如：
///
/// ```ignore
/// ListCustom(items.len(), sel, Modifier::new().fill_max_width(), |i| {
///     let it = &items[i];
///     Row(Modifier::new().fill_max_width(), Start, Center, move || {
///         icon(it.icon, 24.0, colors.on_surface_variant, Modifier::new());
///         Text(it.label, tokens::body_large(colors.on_surface), Modifier::new());
///     });
/// });
/// ```
///
/// 行外壳（整行点击选中 + 高亮）由 [`list_row`] 统一提供，故点击行内任意位置都归属同一行。
/// 不带滚动容器：适合条目较少、由外层负责滚动的场景。
pub fn ListCustom(
    count: usize,
    selected: MutableState<usize>,
    modifier: Modifier,
    item_content: impl Fn(usize) + 'static,
) {
    let n = count;
    // `item_content` 是 `Fn`（可多次 & 调用），但 `list_row` 的 content 参数要求 `'static`；
    // 用 `Rc` 包裹后每个行外壳各自持有一份共享引用，闭包即满足 `'static`。
    let rc = Rc::new(item_content);
    let nav_sel = selected;
    focusable_full(
        modifier,
        || {},
        move |key| {
            let c = nav_sel.read_unregistered();
            nav_sel.set(nav_index(key, c, n));
        },
        None::<fn(char)>,
        move || {
            let rc = rc.clone();
            // focusable_full 是 Box 语义（子树重叠），行需收进 Column 依次排布。
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    for i in 0..n {
                        let rc2 = rc.clone();
                        list_row(i, selected, move || rc2(i));
                    }
                },
            );
        },
    );
}

// ══════════════════════════════ 表格 ══════════════════════════════

/// `DataTable(headers, rows, zebra, sortable, modifier)` — 数据表格。
///
/// - `zebra=true`：隔行 `surface_container` 底色，形成斑马纹。
/// - `sortable=true`：点击表头按该列排序（再次点击切换升/降序；数字列按数值，否则按字典序），
///   表头显示 ▲/▼ 指示当前排序列。
/// - 数字列（整列可解析为 `f64`）单元格右对齐，便于阅读金额/数量。
pub fn DataTable(
    headers: &[String],
    rows: &[Vec<String>],
    zebra: bool,
    sortable: bool,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let g = glass_spec();
    let hs: Vec<String> = headers.to_vec();
    let rs_in: Vec<Vec<String>> = rows.to_vec();
    let cols = hs.len().max(1);

    // 排序状态：(列下标, 升序)，初始 usize::MAX 表示不排序
    let sort = remember(|| mutableStateOf((usize::MAX, true)));
    let (sort_col, asc) = sort.get();

    // 整列是否数字 → 右对齐
    let numeric_cols: Vec<bool> = (0..cols)
        .map(|col| {
            rs_in.iter().all(|r| {
                r.get(col)
                    .map(|s| s.trim().parse::<f64>().is_ok())
                    .unwrap_or(false)
            })
        })
        .collect();

    // 排序后的行
    let mut rs = rs_in.clone();
    if sort_col < cols {
        let col = sort_col;
        let numeric = numeric_cols[col];
        rs.sort_by(|a, b| {
            let av = a.get(col).map(|s| s.as_str()).unwrap_or("");
            let bv = b.get(col).map(|s| s.as_str()).unwrap_or("");
            let ord = if numeric {
                let af = av.trim().parse::<f64>().unwrap_or(0.0);
                let bf = bv.trim().parse::<f64>().unwrap_or(0.0);
                af.partial_cmp(&bf).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                av.cmp(bv)
            };
            if asc { ord } else { ord.reverse() }
        });
    }

    // M3 `DataTable`：12dp 圆角外壳，表头 52dp / `label_medium`，正文行 52dp / `body_medium`，
    // 单元格左右 16dp，行间 `outline_variant` 分隔线。
    let mut m = GlassOrBg(
        modifier.fill_max_width(),
        g,
        colors.Surface,
        tokens::SHAPE_MEDIUM,
    );
    if !g.enabled {
        m = m.border(1.0, colors.outline_variant, tokens::SHAPE_MEDIUM);
    }
    Column(
        m.padding_hv(0.0, 0.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            let hs1 = hs.clone();
            let rs1 = rs.clone();
            let nc1 = numeric_cols.clone();
            let nc_h = nc1.clone();
            let nc_d = nc1.clone();
            let sort1 = sort;
            // 表头（可排序）
            Row(
                Modifier::new()
                    .fill_max_width()
                    .height(tokens::TABLE_HEADER_HEIGHT)
                    .background(colors.surface_container, 0.0)
                    .padding_ltrb(tokens::SPACE_XXL, 0.0, tokens::SPACE_LG, 0.0),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    for (i, h) in hs1.clone().into_iter().enumerate() {
                        let nc_h_local = nc_h.clone();
                        let right = nc_h_local[i];
                        let arrow = if sort1.get().0 == i {
                            if sort1.get().1 { "▲" } else { "▼" }
                        } else {
                            ""
                        };
                        let txt = if arrow.is_empty() {
                            h.clone()
                        } else {
                            format!("{} {}", h, arrow)
                        };
                        let id = add_node(
                            Policy::Box(if right {
                                Alignment::CenterEnd
                            } else {
                                Alignment::CenterStart
                            }),
                            Modifier::new()
                                .weight(1.0 / cols as f32, false)
                                .fill_max_height()
                                .padding_ltrb(
                                    if i == 0 { 0.0 } else { tokens::SPACE_LG },
                                    0.0,
                                    0.0,
                                    0.0,
                                ),
                        );
                        with_rt_pub(|rt| {
                            if sortable {
                                let s2 = sort1;
                                rt.nodes[id.index()].on_click = Some(Box::new(move || {
                                    s2.update(|s| {
                                        if s.0 == i {
                                            s.1 = !s.1;
                                        } else {
                                            *s = (i, true);
                                        }
                                    });
                                }));
                            }
                            let t = txt;
                            let end = right;
                            rt.nodes[id.index()].draw =
                                Some(Box::new(move |b: &mut DrawBuilder, x, y, w, hgt| {
                                    let style = tokens::label_medium(colors.on_surface_variant);
                                    let (tw, th) = measure_text_with_measurer(&t, &style);
                                    let dx = if end { x + w - tw } else { x };
                                    let dy = y + (hgt - th) * 0.5;
                                    b.draw_text(dx, dy, &t, &style, f32::INFINITY);
                                }));
                        });
                    }
                },
            );
            // 数据行
            for (ri, r) in rs1.clone().into_iter().enumerate() {
                let nc_d_local = nc_d.clone();
                let r1 = r.clone();
                let bg = if zebra && ri % 2 == 1 {
                    colors.outline_variant.with_alpha(0.18)
                } else {
                    Color::TRANSPARENT
                };
                Row(
                    Modifier::new()
                        .fill_max_width()
                        .height(tokens::TABLE_ROW_HEIGHT)
                        .background(bg, 0.0)
                        .padding_ltrb(tokens::SPACE_XXL, 0.0, tokens::SPACE_LG, 0.0),
                    Arrangement::Start,
                    CrossAxisAlignment::Center,
                    move || {
                        for (i, cell) in r1.clone().into_iter().enumerate() {
                            let right = nc_d_local[i];
                            Box(
                                Modifier::new()
                                    .weight(1.0 / cols as f32, false)
                                    .fill_max_height()
                                    .padding_ltrb(
                                        if i == 0 { 0.0 } else { tokens::SPACE_LG },
                                        0.0,
                                        0.0,
                                        0.0,
                                    ),
                                if right {
                                    Alignment::CenterEnd
                                } else {
                                    Alignment::CenterStart
                                },
                                move || {
                                    Text(
                                        cell.clone(),
                                        tokens::body_medium(colors.on_surface),
                                        Modifier::new(),
                                    );
                                },
                            );
                        }
                    },
                );
                if ri + 1 < rs1.len() {
                    add_node(
                        Policy::Leaf,
                        Modifier::new()
                            .fill_max_width()
                            .height(1.0)
                            .background(colors.outline_variant, 0.0),
                    );
                }
            }
        },
    );
}

/// `DescriptionList(items)` — 键值描述列表。
pub fn DescriptionList(items: &[(String, String)], modifier: Modifier) {
    let colors = theme_colors();
    let list: Vec<(String, String)> = items.to_vec();
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            for (k, v) in list.clone() {
                Row(
                    Modifier::new().fill_max_width().padding_ltrb(
                        0.0,
                        tokens::SPACE_SM,
                        0.0,
                        tokens::SPACE_SM,
                    ),
                    Arrangement::Start,
                    CrossAxisAlignment::Start,
                    move || {
                        Text(
                            k.clone(),
                            tokens::label_large(colors.on_surface_variant),
                            Modifier::new().width(120.0),
                        );
                        Text(
                            v.clone(),
                            tokens::body_medium(colors.on_surface),
                            Modifier::new().weight(1.0, false),
                        );
                    },
                );
            }
        },
    );
}

// ══════════════════════════════ 树 ══════════════════════════════

/// 树节点。
#[derive(Clone, Debug)]
pub struct TreeNode {
    pub label: String,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            children: Vec::new(),
        }
    }
    pub fn with_children(label: impl Into<String>, children: Vec<TreeNode>) -> Self {
        Self {
            label: label.into(),
            children,
        }
    }
}

/// 树节点路径 key：根 = `label`，子 = `{父路径}/{label}`（`expanded` / `selected` 同一规则）。
fn tree_key(path: &str, label: &str) -> String {
    if path.is_empty() {
        label.to_string()
    } else {
        format!("{path}/{label}")
    }
}

/// 展平后的可见条目（键导航用）：路径 key + 有无子 + 首子 key。
struct TreeNavEntry {
    key: String,
    has_children: bool,
    first_child: Option<String>,
}

/// DFS 收集可见条目（只进已展开节点），顺序与渲染一致。
fn collect_visible(nodes: &[TreeNode], path: &str, exp: &[String], out: &mut Vec<TreeNavEntry>) {
    for n in nodes {
        let key = tree_key(path, &n.label);
        let has_children = !n.children.is_empty();
        let first_child = n.children.first().map(|c| tree_key(&key, &c.label));
        out.push(TreeNavEntry {
            key: key.clone(),
            has_children,
            first_child,
        });
        if has_children && exp.contains(&key) {
            collect_visible(&n.children, &key, exp, out);
        }
    }
}

/// `TreeView(nodes, expanded, selected)` — 可展开 + 可选中树（`expanded` 存展开路径集合，
/// `selected` 存选中节点路径 key，选中行 `secondary_container` 高亮）。
///
/// 键盘导航（聚焦容器后）：↑/↓ ±1、PageUp/Down ±8、Home/End 首尾（在可见条目表上，
/// 语义同 `nav_index`）；**Right** 未展开→展开、已展开→进首个子；**Left** 已展开→
/// 折叠停在原节点、否则跳父节点（标准树键盘语义）。无滚动容器，不做 sticky。
pub fn TreeView(
    nodes: &[TreeNode],
    expanded: MutableState<Vec<String>>,
    selected: MutableState<String>,
    modifier: Modifier,
) {
    let ns: Vec<TreeNode> = nodes.to_vec();
    let nav_sel = selected;
    let nav_exp = expanded;
    let nav_ns = ns.clone();
    focusable_full(
        modifier,
        || {},
        move |key| {
            let exp = nav_exp.peek_mut(|v| v.clone());
            let mut table: Vec<TreeNavEntry> = Vec::new();
            collect_visible(&nav_ns, "", &exp, &mut table);
            if table.is_empty() {
                return;
            }
            let cur_key = nav_sel.peek_mut(|s| s.clone());
            let cur = table.iter().position(|e| e.key == cur_key).unwrap_or(0);
            match key {
                NavKey::Left => {
                    let e = &table[cur];
                    if e.has_children && exp.contains(&e.key) {
                        let k = e.key.clone();
                        nav_exp.update(|v| v.retain(|x| x != &k));
                    } else if let Some(pos) = cur_key.rfind('/') {
                        nav_sel.set(cur_key[..pos].to_string());
                    }
                }
                NavKey::Right => {
                    let e = &table[cur];
                    if e.has_children {
                        if exp.contains(&e.key) {
                            if let Some(fc) = &e.first_child {
                                nav_sel.set(fc.clone());
                            }
                        } else {
                            let k = e.key.clone();
                            nav_exp.update(|v| {
                                if !v.contains(&k) {
                                    v.push(k.clone());
                                }
                            });
                        }
                    }
                }
                k => {
                    let idx = nav_index(k, cur, table.len());
                    nav_sel.set(table[idx].key.clone());
                }
            }
        },
        None::<fn(char)>,
        move || {
            let ns = ns.clone();
            // focusable_full 是 Box 语义（子树重叠），根节点必须收进 Column 依次
            // 排布，否则会重叠（见 ListView/List）。
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    for n in ns.clone() {
                        tree_node(n, String::new(), expanded, selected);
                    }
                },
            );
        },
    );
}

fn tree_node(
    node: TreeNode,
    path: String,
    expanded: MutableState<Vec<String>>,
    selected: MutableState<String>,
) {
    let colors = theme_colors();
    let node_label = node.label.clone();
    let node_kids = node.children.clone();
    let key = tree_key(&path, &node.label);
    let open = expanded.read_clone().contains(&key);
    // 选中高亮（组合期读 → 选中变化时重组）；与列表同 M3 语义。
    let is_sel = selected.read_clone() == key;
    let has_children = !node.children.is_empty();
    // M3 `AnimatedVisibility(expandVertically)` + `AnimatedRotation`：箭头随展开进度连续转，
    // 子树按自然高度测量、外层节点高度按进度折算并裁剪。
    let frac = animate_float_as_state(
        if open { 1.0 } else { 0.0 },
        if open {
            tokens::enter_spec()
        } else {
            tokens::exit_spec()
        },
    );
    let st = expanded;
    let st_sel = selected;
    let k = key.clone();
    let k_click = key.clone();
    let m = Modifier::new().fill_max_width().padding_ltrb(
        tokens::SPACE_SM,
        tokens::SPACE_XS,
        tokens::SPACE_MD,
        tokens::SPACE_XS,
    );
    let m = if is_sel {
        m.background(colors.secondary_container, tokens::SHAPE_SMALL)
    } else {
        m
    };
    crate::widgets::basic::InkWell(
        m,
        move || {
            // 点击 = 选中 + 展开态切换（叶子节点 toggle 展开无视觉效果，保留原行为）。
            st_sel.set(k_click.clone());
            st.update(|v| {
                if v.contains(&k) {
                    v.retain(|x| x != &k);
                } else {
                    v.push(k.clone());
                }
            });
        },
        move || {
            let lbl1 = node_label.clone();
            Row(
                Modifier::new(),
                Arrangement::Start,
                CrossAxisAlignment::Center,
                move || {
                    if has_children {
                        crate::widgets::RotatingExpander(
                            frac,
                            tokens::ICON_SMALL,
                            colors.on_surface_variant,
                            Modifier::new(),
                        );
                    } else {
                        Spacer(Modifier::new().width(tokens::ICON_SMALL));
                    }
                    Spacer(Modifier::new().width(tokens::SPACE_XS));
                    icon(
                        if has_children {
                            IconName::Folder
                        } else {
                            IconName::File
                        },
                        tokens::ICON_SMALL,
                        colors.primary,
                        Modifier::new(),
                    );
                    Spacer(Modifier::new().width(tokens::SPACE_SM));
                    Text(
                        lbl1.clone(),
                        tokens::body_medium(colors.on_surface),
                        Modifier::new(),
                    );
                },
            );
        },
    );
    if open {
        let cpath = key.clone();
        Column(
            Modifier::new().padding_ltrb(tokens::SPACE_XXL, 0.0, 0.0, 0.0),
            Arrangement::Start,
            CrossAxisAlignment::Start,
            move || {
                for ch in node_kids.clone() {
                    tree_node(ch, cpath.clone(), expanded, selected);
                }
            },
        );
    }
}

// ══════════════════════════════ 图表 ══════════════════════════════

/// 轴刻度「漂亮上限」：向上取到 1 / 2 / 5 × 10ⁿ。
fn chart_nice_max(v: f32) -> f32 {
    if v <= 0.0 {
        return 1.0;
    }
    let exp = v.log10().floor();
    let base = 10f32.powf(exp);
    let f = v / base;
    let nf = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    nf * base
}

/// 数值文本：整数去小数，否则保留 1 位。
fn chart_fmt_num(v: f32) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i32)
    } else {
        format!("{:.1}", v)
    }
}

/// `BarChart(values, labels)` — 柱状图（含网格线 + y 轴刻度 + 柱顶数值）。
pub fn BarChart(values: &[f32], labels: &[String], modifier: Modifier) {
    let colors = theme_colors();
    let vs: Vec<f32> = values.to_vec();
    let ls: Vec<String> = labels.to_vec();
    Canvas(
        modifier.fill_max_width().height(200.0),
        move |b, x, y, w, h| {
            let pad_l = 30.0;
            let pad_r = 10.0;
            let pad_t = 16.0;
            let pad_b = 22.0;
            let x0 = x + pad_l;
            let y0 = y + pad_t;
            let pw = (w - pad_l - pad_r).max(1.0);
            let ph = (h - pad_t - pad_b).max(1.0);
            let base = y0 + ph;
            let nice = chart_nice_max(vs.iter().cloned().fold(0.0f32, f32::max).max(1.0));
            let ticks = 4;
            for i in 0..=ticks {
                let tv = nice * (i as f32 / ticks as f32);
                let gy = base - (tv / nice) * ph;
                b.line(x0, gy, x0 + pw, gy, colors.outline_variant, 1.0);
                b.draw_text(
                    x + 2.0,
                    gy + 3.0,
                    &chart_fmt_num(tv),
                    &tokens::label_small(colors.on_surface_variant),
                    pad_l - 4.0,
                );
            }
            let n = vs.len().max(1) as f32;
            let slot = pw / n;
            let bw = slot * 0.6;
            for (i, v) in vs.iter().enumerate() {
                let bh = (v / nice) * ph;
                let bx = x0 + i as f32 * slot + (slot - bw) * 0.5;
                b.fill_rrect(bx, base - bh, bw, bh, 3.0, colors.primary);
                b.draw_text(
                    bx,
                    base - bh - 4.0,
                    &chart_fmt_num(*v),
                    &tokens::label_small(colors.on_surface),
                    bw + 4.0,
                );
            }
            for (i, l) in ls.iter().enumerate() {
                let tx = x0 + i as f32 * slot;
                b.draw_text(
                    tx,
                    base + 4.0,
                    l,
                    &tokens::label_small(colors.on_surface_variant),
                    slot,
                );
            }
            b.line(x0, y0, x0, base, colors.outline, 1.0);
            b.line(x0, base, x0 + pw, base, colors.outline, 1.0);
        },
    );
}

/// `LineChart(values)` — 折线图（含网格线 + y 轴刻度 + 面积填充）。
pub fn LineChart(values: &[f32], modifier: Modifier) {
    let colors = theme_colors();
    let vs: Vec<f32> = values.to_vec();
    Canvas(
        modifier.fill_max_width().height(200.0),
        move |b, x, y, w, h| {
            let pad_l = 30.0;
            let pad_r = 10.0;
            let pad_t = 16.0;
            let pad_b = 12.0;
            let x0 = x + pad_l;
            let y0 = y + pad_t;
            let pw = (w - pad_l - pad_r).max(1.0);
            let ph = (h - pad_t - pad_b).max(1.0);
            let base = y0 + ph;
            let max = vs.iter().cloned().fold(0.0f32, f32::max).max(1.0);
            let min = vs.iter().cloned().fold(f32::MAX, f32::min).min(0.0);
            let nice_max = chart_nice_max(max);
            let nice_min = if min < 0.0 {
                -chart_nice_max(min.abs())
            } else {
                0.0
            };
            let span = (nice_max - nice_min).max(1.0);
            let ticks = 4;
            for i in 0..=ticks {
                let tv = nice_min + span * (i as f32 / ticks as f32);
                let gy = base - ((tv - nice_min) / span) * ph;
                b.line(x0, gy, x0 + pw, gy, colors.outline_variant, 1.0);
                b.draw_text(
                    x + 2.0,
                    gy + 3.0,
                    &chart_fmt_num(tv),
                    &tokens::label_small(colors.on_surface_variant),
                    pad_l - 4.0,
                );
            }
            let mut pts: Vec<(f32, f32)> = Vec::new();
            for (i, v) in vs.iter().enumerate() {
                let t = i as f32 / (vs.len() - 1).max(1) as f32;
                let px = x0 + t * pw;
                let py = base - ((v - nice_min) / span) * ph;
                pts.push((px, py));
            }
            let mut area = pts.clone();
            area.push((x0 + pw, base));
            area.push((x0, base));
            b.fill_path(&area, colors.primary.with_alpha(0.15));
            b.stroke_path(&pts, colors.primary, 2.0);
            for (px, py) in pts {
                b.fill_circle(px, py, 3.0, colors.primary);
            }
            b.line(x0, y0, x0, base, colors.outline, 1.0);
            b.line(x0, base, x0 + pw, base, colors.outline, 1.0);
        },
    );
}

/// `PieChart(values, palette, labels, donut)` — 饼图 / 环形图 + 百分比 + 图例。
///
/// - `labels` 为 `Some` 时在下方绘制颜色图例；`donut=true` 渲染环形（中心镂空）。
/// - 切片占比 ≥5% 才标注百分比，避免拥挤。
pub fn PieChart(
    values: &[f32],
    palette: &[Color],
    labels: Option<&[String]>,
    donut: bool,
    modifier: Modifier,
) {
    let colors = theme_colors();
    let vs: Vec<f32> = values.to_vec();
    let pal: Vec<Color> = palette.to_vec();
    let labs: Vec<String> = labels.map(|l| l.to_vec()).unwrap_or_default();
    let legend_h = if !labs.is_empty() {
        labs.len() as f32 * 16.0 + 4.0
    } else {
        0.0
    };
    let pie_size = 160.0;
    Canvas(
        modifier.size(pie_size, pie_size + legend_h),
        move |b, x, y, w, _h| {
            let total: f32 = vs.iter().sum();
            if total <= 0.0 {
                return;
            }
            let cx = x + pie_size * 0.5;
            let cy = y + pie_size * 0.5;
            let r = pie_size * 0.44;
            let inner = if donut { r * 0.55 } else { 0.0 };
            let mut acc = 0.0f32;
            for (i, v) in vs.iter().enumerate() {
                let a0 = -std::f32::consts::FRAC_PI_2 + acc / total * std::f32::consts::TAU;
                acc += v;
                let a1 = -std::f32::consts::FRAC_PI_2 + acc / total * std::f32::consts::TAU;
                let col = pal
                    .get(i % pal.len().max(1))
                    .copied()
                    .unwrap_or(Color::hex(0x6750A4));
                let steps = ((a1 - a0).abs() / 0.15).ceil().max(2.0) as usize;
                if inner > 0.0 {
                    // 环扇：外弧正向 + 内弧反向闭合
                    let mut pts = Vec::new();
                    for s in 0..=steps {
                        let a = a0 + (a1 - a0) * (s as f32 / steps as f32);
                        pts.push((cx + a.cos() * r, cy + a.sin() * r));
                    }
                    for s in (0..=steps).rev() {
                        let a = a0 + (a1 - a0) * (s as f32 / steps as f32);
                        pts.push((cx + a.cos() * inner, cy + a.sin() * inner));
                    }
                    b.fill_path(&pts, col);
                } else {
                    let mut pts = vec![(cx, cy)];
                    for s in 0..=steps {
                        let a = a0 + (a1 - a0) * (s as f32 / steps as f32);
                        pts.push((cx + a.cos() * r, cy + a.sin() * r));
                    }
                    b.fill_path(&pts, col);
                }
                let pct = (v / total * 100.0).round();
                if pct >= 5.0 {
                    let mid = (a0 + a1) * 0.5;
                    let rr = if inner > 0.0 {
                        (r + inner) * 0.5
                    } else {
                        r * 0.62
                    };
                    let lx = cx + mid.cos() * rr;
                    let ly = cy + mid.sin() * rr;
                    b.draw_text(
                        lx - 8.0,
                        ly + 3.0,
                        &format!("{}%", pct as i32),
                        &tokens::label_small(Color::WHITE),
                        40.0,
                    );
                }
            }
            if !labs.is_empty() {
                let ly0 = y + pie_size + 2.0;
                for (i, lab) in labs.iter().enumerate() {
                    let ry = ly0 + i as f32 * 16.0;
                    let col = pal
                        .get(i % pal.len().max(1))
                        .copied()
                        .unwrap_or(Color::hex(0x6750A4));
                    b.fill_rrect(x + 8.0, ry, 10.0, 10.0, 2.0, col);
                    b.draw_text(
                        x + 24.0,
                        ry + 9.0,
                        lab,
                        &tokens::label_small(colors.on_surface),
                        w - 30.0,
                    );
                }
            }
        },
    );
}

/// `Sparkline(values)` — 迷你趋势线。
pub fn Sparkline(values: &[f32], color: Option<Color>, modifier: Modifier) {
    let colors = theme_colors();
    let col = color.unwrap_or(colors.primary);
    let vs: Vec<f32> = values.to_vec();
    Canvas(modifier.size(96.0, 28.0), move |b, x, y, w, h| {
        if vs.len() < 2 {
            return;
        }
        let max = vs.iter().cloned().fold(0.0f32, f32::max).max(1.0);
        let mut pts = Vec::new();
        for (i, v) in vs.iter().enumerate() {
            let t = i as f32 / (vs.len() - 1) as f32;
            pts.push((x + t * w, y + h - (v / max) * h));
        }
        b.stroke_path(&pts, col, 1.5);
    });
}

// ══════════════════════════════ 进度 / 步进 ══════════════════════════════

/// `LinearProgressIndicator(progress)` — 线性进度（`None` 表示不确定态）。
pub fn LinearProgress(progress: Option<f32>, modifier: Modifier) {
    let colors = theme_colors();
    let id = add_node(
        Policy::Leaf,
        modifier.fill_max_width().height(tokens::PROGRESS_TRACK),
    );
    // 相位走 draw 阶段：不确定态每帧只重绘本节点，不再让调用方作用域整页重组
    let p = crate::widgets::animation::draw_phase(
        id,
        progress,
        1.4,
        compose::animation::LoopMode::Reverse,
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            let full = tokens::full_radius(h);
            b.fill_rrect(x, y, w, h, full, colors.surface_variant);
            let fw = (w * p.get().clamp(0.0, 1.0)).max(h);
            b.fill_rrect(x, y, fw, h, full, colors.primary);
        }));
    });
}

/// `CircularProgressIndicator(progress)` — 环形进度。
pub fn CircularProgress(progress: Option<f32>, size: f32, modifier: Modifier) {
    let colors = theme_colors();
    let id = add_node(Policy::Leaf, modifier.size(size, size));
    let p = crate::widgets::animation::draw_phase(
        id,
        progress,
        1.6,
        compose::animation::LoopMode::Reverse,
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let r = w.min(h) * 0.40;
            let stroke = (r * 0.25).clamp(1.0, tokens::PROGRESS_TRACK);
            let a0 = -std::f32::consts::FRAC_PI_2;
            const STEPS: usize = 32;
            // 定长数组：不确定态每帧重采样，避免热路径上的堆分配。
            // 轨道 = 整圈，进度 = 同半径的部分弧（旧实现两者共用一条弧，灰环从不出现）。
            let mut track = [(0.0f32, 0.0f32); STEPS + 1];
            for (s, q) in track.iter_mut().enumerate() {
                let a = a0 + std::f32::consts::TAU * (s as f32 / STEPS as f32);
                *q = (cx + a.cos() * r, cy + a.sin() * r);
            }
            b.stroke_path(&track, colors.surface_variant, stroke);
            let prog = p.get().clamp(0.0, 1.0);
            if prog > 0.0 {
                let mut arc = [(0.0f32, 0.0f32); STEPS + 1];
                for (s, q) in arc.iter_mut().enumerate() {
                    let a = a0 + std::f32::consts::TAU * prog * (s as f32 / STEPS as f32);
                    *q = (cx + a.cos() * r, cy + a.sin() * r);
                }
                b.stroke_path(&arc, colors.primary, stroke);
            }
        }));
    });
}

/// `Stepper(steps, current)` — 横向步骤条。
pub fn Stepper(steps: &[String], current: MutableState<usize>, modifier: Modifier) {
    let colors = theme_colors();
    let ss: Vec<String> = steps.to_vec();
    let cur = current.get();
    Row(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            for (i, s) in ss.clone().into_iter().enumerate() {
                let done = i <= cur;
                let st = current;
                let id = add_node(
                    Policy::Row {
                        arrangement: Arrangement::Center,
                        cross: CrossAxisAlignment::Center,
                    },
                    Modifier::new(),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].on_click = Some(Box::new(move || st.set(i)));
                });
                with_content_scope(id, move || {
                    // 24dp 圆点：完成 = primary + onPrimary 数字，未到 = 描边环
                    let cid = add_node(Policy::Leaf, Modifier::new().size(24.0, 24.0));
                    with_rt_pub(|rt| {
                        rt.nodes[cid.index()].draw =
                            Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
                                if done {
                                    b.fill_circle(x + 12.0, y + 12.0, 12.0, colors.primary);
                                } else {
                                    b.stroke_circle(x + 12.0, y + 12.0, 11.0, colors.outline, 2.0);
                                }
                                crate::widgets::basic::CenteredText(
                                    b,
                                    x,
                                    y,
                                    24.0,
                                    24.0,
                                    &format!("{}", i + 1),
                                    &tokens::label_medium(if done {
                                        colors.on_primary
                                    } else {
                                        colors.on_surface_variant
                                    }),
                                );
                            }));
                    });
                    Spacer(Modifier::new().width(tokens::SPACE_SM));
                    Text(
                        s.clone(),
                        tokens::label_large(if done {
                            colors.on_surface
                        } else {
                            colors.on_surface_variant
                        }),
                        Modifier::new(),
                    );
                });
                if i + 1 < ss.len() {
                    add_node(
                        Policy::Leaf,
                        Modifier::new()
                            .width(tokens::SPACE_XXXL)
                            .height(1.0)
                            .background(colors.outline_variant, 0.0),
                    );
                    Spacer(Modifier::new().width(tokens::SPACE_SM));
                }
            }
        },
    );
}

// ══════════════════════════════ 会话 / 差异 / Markdown ══════════════════════════════

/// 聊天气泡。
pub fn Bubble(message: &str, from_me: bool, modifier: Modifier) {
    let colors = theme_colors();
    let msg = message.to_string();
    let bg = if from_me {
        colors.primary_container
    } else {
        colors.surface_variant
    };
    let fg = if from_me {
        colors.on_primary_container
    } else {
        colors.on_surface_variant
    };
    // 气泡宽度由文本自然宽度决定（上限 `BUBBLE_MAX_W` 后折行），
    // 故必须是「容器 + Text 叶子」而不是自绘节点 —— 自绘节点量不出固有尺寸。
    Box(
        modifier
            .max_width(tokens::BUBBLE_MAX_W)
            .background(bg, tokens::SHAPE_LARGE)
            .padding_ltrb(
                tokens::SPACE_MD,
                tokens::SPACE_SM,
                tokens::SPACE_MD,
                tokens::SPACE_SM,
            ),
        Alignment::CenterStart,
        move || {
            Text(msg.clone(), tokens::body_medium(fg), Modifier::new());
        },
    );
}

/// 差异视图（行首 `+` / `-` / 空格）。
pub fn DiffView(lines: &[(char, String)], modifier: Modifier) {
    let ls: Vec<(char, String)> = lines.to_vec();
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            for (sig, content) in ls.clone() {
                let colors = theme_colors();
                let (bg, fg) = match sig {
                    '+' => (
                        colors.primary_container.with_alpha(0.5),
                        colors.on_primary_container,
                    ),
                    '-' => (
                        colors.error_container.with_alpha(0.5),
                        colors.on_error_container,
                    ),
                    _ => (Color::TRANSPARENT, colors.on_surface_variant),
                };
                let text_str = format!("{sig} {content}");
                let id = add_node(
                    Policy::Leaf,
                    Modifier::new()
                        .fill_max_width()
                        .background(bg, 0.0)
                        .padding_ltrb(tokens::SPACE_MD, tokens::SPACE_XXS, tokens::SPACE_MD, 0.0),
                );
                with_rt_pub(|rt| {
                    rt.nodes[id.index()].draw =
                        Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
                            b.draw_text(x, y, &text_str, &tokens::body_small(fg), f32::INFINITY);
                        }));
                });
            }
        },
    );
}

/// `Markdown(src)` — 轻量 Markdown 渲染（标题 / 列表 / 引用 / 代码块 / 分割线 / 段落）。
///
/// 排版全部走 M3 角色（`headline_small` / `title_large` / `title_medium` / `body_medium` /
/// `body_small`），代码块用 `surface_variant` + `SHAPE_SMALL`。
pub fn Markdown(src: &str, modifier: Modifier) {
    let colors = theme_colors();
    let lines: Vec<String> = src.lines().map(|l| l.to_string()).collect();
    Column(
        modifier,
        Arrangement::Start,
        CrossAxisAlignment::Start,
        move || {
            let mut in_code = false;
            let mut buf: Vec<String> = Vec::new();
            for line in lines.clone() {
                if line.trim_start().starts_with("```") {
                    if in_code {
                        let code = buf.join("\n");
                        buf.clear();
                        in_code = false;
                        md_code_block(code);
                    } else {
                        in_code = true;
                    }
                    continue;
                }
                if in_code {
                    buf.push(line.clone());
                    continue;
                }
                let t = line.trim_end();
                if let Some(h) = t.strip_prefix("### ") {
                    Text(
                        strip_inline(h),
                        tokens::title_medium(colors.on_surface),
                        Modifier::new().padding_ltrb(0.0, tokens::SPACE_MD, 0.0, tokens::SPACE_XXS),
                    );
                } else if let Some(h) = t.strip_prefix("## ") {
                    Text(
                        strip_inline(h),
                        tokens::title_large(colors.on_surface),
                        Modifier::new().padding_ltrb(0.0, tokens::SPACE_LG, 0.0, tokens::SPACE_XS),
                    );
                } else if let Some(h) = t.strip_prefix("# ") {
                    Text(
                        strip_inline(h),
                        tokens::headline_small(colors.on_surface),
                        Modifier::new().padding_ltrb(0.0, tokens::SPACE_XL, 0.0, tokens::SPACE_SM),
                    );
                } else if t == "---" || t == "***" {
                    crate::widgets::basic::Divider(Modifier::new().padding_ltrb(
                        0.0,
                        tokens::SPACE_SM,
                        0.0,
                        tokens::SPACE_SM,
                    ));
                } else if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                    Text(
                        format!("•  {}", strip_inline(b)),
                        tokens::body_medium(colors.on_surface),
                        Modifier::new().padding_ltrb(
                            tokens::SPACE_MD,
                            tokens::SPACE_XXS,
                            0.0,
                            tokens::SPACE_XXS,
                        ),
                    );
                } else if let Some(q) = t.strip_prefix("> ") {
                    Text(
                        strip_inline(q),
                        tokens::body_medium(colors.on_surface_variant),
                        Modifier::new().padding_ltrb(
                            tokens::SPACE_MD,
                            tokens::SPACE_XXS,
                            0.0,
                            tokens::SPACE_XXS,
                        ),
                    );
                } else if t.trim().is_empty() {
                    Spacer(Modifier::new().height(tokens::SPACE_SM));
                } else {
                    Text(
                        strip_inline(t),
                        tokens::body_medium(colors.on_surface),
                        Modifier::new().padding_ltrb(
                            0.0,
                            tokens::SPACE_XXS,
                            0.0,
                            tokens::SPACE_XXS,
                        ),
                    );
                }
            }
            if in_code && !buf.is_empty() {
                let code = buf.join("\n");
                md_code_block(code);
            }
        },
    );
}

/// Markdown 代码块：`surface_variant` 底 + `SHAPE_SMALL` 圆角 + `body_small` 文字。
fn md_code_block(code: String) {
    let colors = theme_colors();
    let id = add_node(
        Policy::Leaf,
        Modifier::new()
            .fill_max_width()
            .background(colors.surface_variant, tokens::SHAPE_SMALL)
            .padding_ltrb(
                tokens::SPACE_MD,
                tokens::SPACE_SM,
                tokens::SPACE_MD,
                tokens::SPACE_SM,
            ),
    );
    with_rt_pub(|rt| {
        rt.nodes[id.index()].draw = Some(Box::new(move |b: &mut DrawBuilder, x, y, _w, _h| {
            b.draw_text(
                x,
                y,
                &code,
                &tokens::body_small(colors.on_surface_variant),
                f32::INFINITY,
            );
        }));
    });
}

/// 去掉 `**粗体**` / `` `代码` `` 标记（单行文本渲染下的退化处理）。
fn strip_inline(s: &str) -> String {
    s.replace("**", "").replace('`', "")
}

// ════════════════════ 列表 / 网格 / 可展开（data 补齐） ════════════════════

/// `ListView(items, selected)` — 纵向可滚动列表（点击选中，主色容器高亮）。
///
/// 键盘导航：点过列表即持焦（焦点在容器，点条目不抢），↑/↓ ±1、PageUp/Down ±8、
/// Home/End 首尾；选中**变化**时把该项标为滚动 sticky，布局期最小幅度滚入视口，
/// 不干扰用户手动滚动。
pub fn ListView(items: &[String], selected: MutableState<usize>, modifier: Modifier) {
    let items: Vec<String> = items.to_vec();
    let cur = selected.get();
    let sel = selected;
    let n = items.len();
    // 各行程节点句柄（每次组合清空重登记，供 sticky 在组合外读父链）+ 上次生效选中。
    let ids: Rc<RefCell<Vec<NodeId>>> = remember(|| Rc::new(RefCell::new(Vec::new())));
    let last_sel: Rc<Cell<usize>> = remember(|| Rc::new(Cell::new(usize::MAX)));
    let nav_sel = sel;
    focusable_full(
        modifier,
        || {},
        move |key| {
            let c = nav_sel.read_unregistered();
            nav_sel.set(nav_index(key, c, n));
        },
        None::<fn(char)>,
        move || {
            let ids1 = ids.clone();
            ids1.borrow_mut().clear();
            let list = items.clone();
            let s = sel;
            let cur_v = cur;
            VerticalScroll(
                Modifier::new().fill_max_width().fill_max_height(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let colors = theme_colors();
                    let ids2 = ids1.clone();
                    for (i, it) in list.clone().into_iter().enumerate() {
                        let on = i == cur_v;
                        let st = s;
                        let idx = i;
                        let id = ListItemR(
                            &it,
                            None,
                            None,
                            None,
                            Modifier::new().background(
                                // M3 选中列表项 = `secondary_container`
                                if on {
                                    colors.secondary_container
                                } else {
                                    Color::TRANSPARENT
                                },
                                tokens::SHAPE_SMALL,
                            ),
                            Some(Box::new(move || st.set(idx))),
                        );
                        ids2.borrow_mut().push(id);
                        if i + 1 < list.len() {
                            Spacer(Modifier::new().height(tokens::SPACE_XXS));
                        }
                    }
                },
            );
            // 选中变化才标 sticky（每帧重组都校正会把手动滚动位顶回）：取行程节点
            // 的父（=滚动容器）写 sticky，`update_scroll_content` 一次性校正后自清。
            if last_sel.replace(cur) != cur {
                let target = ids.borrow().get(cur).copied();
                if let Some(i) = target {
                    with_rt_pub(|rt| {
                        if let Some(sc) = rt.nodes[i.index()].parent {
                            if let Some(s) = rt.nodes[sc.index()].scroll.as_mut() {
                                s.sticky = Some(i);
                            }
                        }
                    });
                }
            }
        },
    );
}

/// `ListViewCustom(count, selected, modifier, item_content)` — 带滚动 + 自定义 item 的列表。
///
/// 与 [`ListView`] 共用选中语义、键盘步长（↑/↓ ±1、PageUp/Down ±8、Home/End）与
/// 滚动 sticky；每行内容由 `item_content(index)` 绘制，调用方可放任意 widget。行外壳
/// （整行点击选中 + `secondary_container` 高亮）由 [`list_row`] 统一提供。
pub fn ListViewCustom(
    count: usize,
    selected: MutableState<usize>,
    modifier: Modifier,
    item_content: impl Fn(usize) + 'static,
) {
    let n = count;
    let cur = selected.get();
    let sel = selected;
    let nav_sel = sel;
    // 各行程节点句柄（每次组合清空重登记，供 sticky 在组合外读父链）+ 上次生效选中。
    let ids: Rc<RefCell<Vec<NodeId>>> = remember(|| Rc::new(RefCell::new(Vec::new())));
    let last_sel: Rc<Cell<usize>> = remember(|| Rc::new(Cell::new(usize::MAX)));
    // `item_content` 是 `Fn`（可多次调用），但 `list_row` 的 content 参数要求 `'static`；
    // 在函数体（而非 Fn 闭包）里用 `Rc` 包裹，闭包内只 clone 共享引用即可。
    let rc = Rc::new(item_content);
    focusable_full(
        modifier,
        || {},
        move |key| {
            let c = nav_sel.read_unregistered();
            nav_sel.set(nav_index(key, c, n));
        },
        None::<fn(char)>,
        move || {
            let ids1 = ids.clone();
            ids1.borrow_mut().clear();
            let s = sel;
            let cur_v = cur;
            let rc_local = rc.clone();
            VerticalScroll(
                Modifier::new().fill_max_width().fill_max_height(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let ids2 = ids1.clone();
                    for i in 0..n {
                        let rc2 = rc_local.clone();
                        let id = list_row(i, s, move || rc2(i));
                        ids2.borrow_mut().push(id);
                        if i + 1 < n {
                            Spacer(Modifier::new().height(tokens::SPACE_XXS));
                        }
                    }
                },
            );
            // 选中变化才标 sticky（每帧重组都校正会把手动滚动位顶回）：取行程节点
            // 的父（=滚动容器）写 sticky，`update_scroll_content` 一次性校正后自清。
            if last_sel.replace(cur_v) != cur_v {
                let target = ids.borrow().get(cur_v).copied();
                if let Some(i) = target {
                    with_rt_pub(|rt| {
                        if let Some(sc) = rt.nodes[i.index()].parent {
                            if let Some(s) = rt.nodes[sc.index()].scroll.as_mut() {
                                s.sticky = Some(i);
                            }
                        }
                    });
                }
            }
        },
    );
}

/// `GridView(items, cols, selected)` — 二维网格（点击选中，主色容器高亮）。
///
/// 键盘导航（聚焦容器后，二维步长）：↑/↓ ±cols、←/→ ±1、PageUp/Down ±(2·cols)、
/// Home/End 首尾，越界夹取；无滚动容器故不做 sticky。
pub fn GridView(items: &[String], cols: usize, selected: MutableState<usize>, modifier: Modifier) {
    let items: Vec<String> = items.to_vec();
    let cols = cols.max(1);
    let cur = selected.get();
    let sel = selected;
    let n = items.len();
    let nav_sel = sel;
    focusable_full(
        modifier,
        || {},
        move |key| {
            if n == 0 {
                return;
            }
            let last = n - 1;
            let page = 2 * cols;
            let cur = nav_sel.read_unregistered();
            let next = match key {
                NavKey::Up => cur.saturating_sub(cols),
                NavKey::Down => (cur + cols).min(last),
                NavKey::Left => cur.saturating_sub(1),
                NavKey::Right => (cur + 1).min(last),
                NavKey::PageUp => cur.saturating_sub(page),
                NavKey::PageDown => (cur + page).min(last),
                NavKey::Home => 0,
                NavKey::End => last,
                NavKey::Delete => cur,
            };
            nav_sel.set(next);
        },
        None::<fn(char)>,
        move || {
            let items = items.clone();
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let items1 = items.clone();
                    let sel1 = sel;
                    let cur1 = cur;
                    let colors = theme_colors();
                    let n = items1.len();
                    let nrows = (n + cols - 1) / cols;
                    for r in 0..nrows {
                        let row_items = items1.clone();
                        Row(
                            Modifier::new().fill_max_width(),
                            Arrangement::Start,
                            CrossAxisAlignment::Stretch,
                            move || {
                                for col in 0..cols {
                                    let idx = r * cols + col;
                                    if idx >= n {
                                        Spacer(Modifier::new().weight(1.0, false));
                                        continue;
                                    }
                                    let on = idx == cur1;
                                    let s = sel1;
                                    let label = row_items[idx].clone();
                                    let id = add_node(
                                        Policy::Box(Alignment::Center),
                                        Modifier::new()
                                            .weight(1.0, false)
                                            .height(tokens::GRID_CELL_HEIGHT)
                                            .padding_ltrb(
                                                tokens::SPACE_XS,
                                                tokens::SPACE_XS,
                                                tokens::SPACE_XS,
                                                tokens::SPACE_XS,
                                            )
                                            .background(
                                                if on {
                                                    colors.secondary_container
                                                } else {
                                                    colors.surface_variant
                                                },
                                                tokens::SHAPE_SMALL,
                                            ),
                                    );
                                    with_rt_pub(|rt| {
                                        rt.nodes[id.index()].on_click =
                                            Some(Box::new(move || s.set(idx)));
                                        rt.nodes[id.index()].draw =
                                            Some(Box::new(move |b: &mut DrawBuilder, x, y, w, h| {
                                                crate::widgets::basic::CenteredText(
                                                    b,
                                                    x,
                                                    y,
                                                    w,
                                                    h,
                                                    &label,
                                                    &tokens::body_medium(if on {
                                                        colors.on_secondary_container
                                                    } else {
                                                        colors.on_surface_variant
                                                    }),
                                                );
                                            }));
                                    });
                                }
                            },
                        );
                    }
                },
            );
        },
    );
}

/// `ExpansionTile(title, expanded, content)` — 可展开/折叠分组。
///
/// 对齐 Compose `ExpansionTile` + `AnimatedVisibility(expandVertically)`：内容按自然高度
/// 测量、外层高度逐帧折算并裁剪，箭头连续旋转而非硬切换图标。
pub fn ExpansionTile(
    title: &str,
    expanded: MutableState<bool>,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let colors = theme_colors();
    let t = title.to_string();
    let open = expanded.get();
    let st = expanded;
    let spec = if open {
        tokens::enter_spec()
    } else {
        tokens::exit_spec()
    };
    let frac = animate_float_as_state(if open { 1.0 } else { 0.0 }, spec);
    // 展开内容是「条件组合」的闭包，`Fn` 外层闭包不能把它 move 进去 → `Rc` 共享。
    let content = std::rc::Rc::new(content);
    Column(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            let tt = t.clone();
            let s = st;
            let s_click = s;
            InkWell(
                Modifier::new()
                    .fill_max_width()
                    .min_height(tokens::LIST_ONE_LINE)
                    .padding_ltrb(
                        tokens::LIST_PAD_H,
                        tokens::SPACE_MD,
                        tokens::LIST_PAD_H,
                        tokens::SPACE_MD,
                    ),
                move || s_click.set(!s_click.get()),
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
                                tokens::LIST_ICON_SIZE,
                                colors.on_surface_variant,
                                Modifier::new(),
                            );
                        },
                    );
                },
            );
            if open || frac > 0.001 {
                let body = content.clone();
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
                with_content_scope(wid, move || body());
            }
        },
    );
}

/// `SearchableList(items, query, selected)` — 顶部搜索框 + 按子串过滤的列表。
pub fn SearchableList(
    items: &[String],
    query: MutableState<String>,
    selected: MutableState<usize>,
    modifier: Modifier,
) {
    let items: Vec<String> = items.to_vec();
    let q = query.read_clone();
    let sel = selected;
    let qst = query;
    Column(
        modifier.fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            TextField(
                qst,
                "Search",
                Modifier::new().fill_max_width().padding_hv(0.0, 0.0),
            );
            Spacer(Modifier::new().height(6.0));
            let items1 = items.clone();
            let sel1 = sel;
            let cur_q = q.clone();
            let filtered: Vec<(usize, String)> = items1
                .iter()
                .enumerate()
                .filter(|(_, s)| s.to_lowercase().contains(&cur_q.to_lowercase()))
                .map(|(i, s)| (i, s.clone()))
                .collect();
            VerticalScroll(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let colors2 = theme_colors();
                    let filtered1 = filtered.clone();
                    let sel2 = sel1;
                    for (orig, label) in filtered1 {
                        let on = orig == sel2.get();
                        let s = sel2;
                        let idx = orig;
                        let lab = label.clone();
                        ListItem(
                            &lab,
                            None,
                            None,
                            None,
                            Modifier::new().background(
                                if on {
                                    colors2.secondary_container
                                } else {
                                    Color::TRANSPARENT
                                },
                                0.0,
                            ),
                            Some(Box::new(move || s.set(idx))),
                        );
                    }
                },
            );
        },
    );
}
