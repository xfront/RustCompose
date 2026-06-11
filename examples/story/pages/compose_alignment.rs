//! Compose 对齐演示：把已实现的核心架构特性（SlotTable / Applier / Snapshot / Effects /
//! Modifier.Node / GraphicsLayer / SubcomposeLayout / Semantics）以交互方式呈现在 story 中。
//! 每个页面既展示视觉效果，又用文本探针显示内部状态，便于对照 Jetpack Compose。

use std::cell::Cell;
use std::rc::Rc;

use crate::common::*;
use crate::prelude::*;

use compose::applier;
use compose::gesture::{
    PointerEvent, PointerEventType, PointerInput, detect_drag_gestures, detect_tap_gestures,
    drag_config, tap_config,
};
use compose::foundation::Constraints;
use compose::layout::{
    Alignment, Arrangement, CrossAxisAlignment, DrawCmd, MeasurePolicy, MeasureResult, Modifier,
    Policy, SubcomposeLayout, SubcomposeStackPolicy, add_node, create_root_node, draw_tree,
    perform_layout, with_content_scope,
};
use compose::modifier_node;
use compose::runtime::{self, SideEffect, producedState, rememberUpdatedState, snapshotFlow, with_rt_pub};
use compose::semantics::{self, Role, build_tree};
use compose::slot_table::{GroupKey, SlotTable};
use compose::snapshot::{self, SnapshotId};
use compose_material::{Button, Card, Chip, TextButton};

// ══════════════════════ 1. SlotTable gap-buffer ══════════════════════

pub(crate) fn page_slot_table() {
    section("SlotTable gap-buffer（对齐 Compose SlotTable）");
    Text(
        "线性槽数组 + 可搬移 gap：删除把槽圈回 gap，重新插入复用同一物理槽，不扩容。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    // 演示表在 remember 中持有，按钮增删 slot；初始带一个 group 头（Compose 的 group 概念）。
    let table = remember(|| {
        let mut t = SlotTable::new();
        t.start_group(0, GroupKey::Explicit(42));
        t.end_group(0, 0);
        Rc::new(std::cell::RefCell::new(t))
    });
    let rev = remember(|| mutableStateOf(0u32));
    let t = table.clone();
    let r = rev;
    Card(
        Modifier::new().padding(12.0).border(1.0, theme_colors().outline_variant, 12.0),
        move || {
            let t = t.clone();
            let r = r;
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let t = t.clone();
                    let r = r;
                    // 必须读一次 rev，让本作用域注册依赖；否则按钮里 update 的
                    // 状态没有读者，只有鼠标离开触发 hover 重绘时文本才更新。
                    let _ = r.get();
                    let (data_len, physical, gap) = {
                        let tb = t.borrow();
                        (tb.data_len(), tb.physical_len(), tb.gap())
                    };
                    Text(
                        format!("逻辑槽 = {data_len}   物理槽 = {physical}   gap = [{}, {})", gap.0, gap.1),
                        TextStyle::new().size(12.0).weight(600),
                        Modifier::new(),
                    );
                    Text(
                        "删掉末尾再插入 → physical 不变（gap 复用）；这正是 Compose 条件组合的槽回收。",
                        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
                        Modifier::new(),
                    );
                    Spacer(Modifier::new().height(8.0));
                    let add_t = t.clone();
                    let add_r = r;
                    Button(
                        Modifier::new(),
                        move || {
                            let n = add_t.borrow().data_len();
                            add_t.borrow_mut().insert_data(n, Box::new(format!("slot-{n}")));
                            add_r.update(|v| *v += 1);
                        },
                        || Text("追加 slot", TextStyle::new(), Modifier::new()),
                    );
                    let del_t = t.clone();
                    let del_r = r;
                    TextButton(
                        "删除末尾（圈入 gap）",
                        Modifier::new(),
                        move || {
                            let n = del_t.borrow().data_len();
                            if n > 0 {
                                del_t.borrow_mut().truncate_data(n - 1);
                                del_r.update(|v| *v += 1);
                            }
                        },
                    );
                    // group 头探测只在首次建组（避免每次重组重复插入 group）。
                    Text(
                        format!("group key 探测: {:?}", t.borrow().group_key_at(0)),
                        TextStyle::new().size(10.0).color(theme_colors().on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        },
    );
}

// ══════════════════════ 2. Applier ══════════════════════

pub(crate) fn page_applier() {
    section("Applier（对齐 Compose Applier）");
    Text(
        "所有节点插入/删除/移动/清空都经 NodeApplier。可注入自定义 applier 拦截这些操作。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    let hits = remember(|| Rc::new(Cell::new(0usize)));
    let rev = remember(|| mutableStateOf(0u32));
    let h = hits.clone();
    let r = rev;

    // 首次进入时用自定义 applier 建几层节点，统计插入次数。
    if h.get() == 0 {
        struct Counting(Rc<Cell<usize>>);
        impl applier::NodeApplier for Counting {
            fn insert_bottom_up(&self, rt: &mut compose::runtime::RuntimeData, p: compose::layout::NodeId, i: usize, c: compose::layout::NodeId) {
                self.0.set(self.0.get() + 1);
                applier::ArenaApplier.insert_bottom_up(rt, p, i, c);
            }
            fn remove(&self, rt: &mut compose::runtime::RuntimeData, p: compose::layout::NodeId, i: usize, n: usize) {
                applier::ArenaApplier.remove(rt, p, i, n);
            }
            fn move_nodes(&self, rt: &mut compose::runtime::RuntimeData, p: compose::layout::NodeId, f: usize, t: usize, n: usize) {
                applier::ArenaApplier.move_nodes(rt, p, f, t, n);
            }
            fn clear(&self, rt: &mut compose::runtime::RuntimeData, p: compose::layout::NodeId) {
                applier::ArenaApplier.clear(rt, p);
            }
        }
        let h = h.clone();
        applier::with_applier(Counting(h), || {
            let id = add_node(Policy::Box(Alignment::TopStart), Modifier::new());
            with_content_scope(id, || {
                for i in 0..4 {
                    let c = add_node(Policy::Leaf, Modifier::new());
                    with_content_scope(c, || {});
                    let _ = i;
                }
            });
        });
    }

    Card(
        Modifier::new().padding(12.0).border(1.0, theme_colors().outline_variant, 12.0),
        move || {
            let h = h.clone();
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let h = h.clone();
                    Text(
                        format!("自定义 applier 拦截到 {} 次 insert_bottom_up", h.get()),
                        TextStyle::new().size(12.0).weight(600),
                        Modifier::new(),
                    );
                    Text(
                        format!("当前 applier 是否被替换：{}（退出 with_applier 后恢复默认）", applier::is_custom()),
                        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
                        Modifier::new(),
                    );
                    let _ = r;
                },
            );
        },
    );
}

// ══════════════════════ 3. Snapshot ══════════════════════

pub(crate) fn page_snapshot() {
    section("Snapshot（对齐 Compose Snapshot）");
    Text(
        "可变快照内的写隔离到写集合，apply 后才提交全局；快照读冻结首次读到的值。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    let global = remember(|| mutableStateOf(0i32));
    let view = remember(|| mutableStateOf(String::from("尚未创建快照")));
    let g = global;
    let v = view;

    Card(
        Modifier::new().padding(12.0).border(1.0, theme_colors().outline_variant, 12.0),
        move || {
            let g = g;
            let v = v;
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let g = g;
                    let v = v;
                    Text(
                        format!("全局值 = {}", g.get()),
                        TextStyle::new().size(12.0).weight(600),
                        Modifier::new(),
                    );
                    Text(
                        v.read_clone(),
                        TextStyle::new().size(11.0).color(theme_colors().primary),
                        Modifier::new(),
                    );
                    Spacer(Modifier::new().height(8.0));
                    Button(
                        Modifier::new(),
                        move || {
                            let s = g.get() + 10;
                            g.set(s);
                            v.set(format!("全局写为 {s}（快照读不受影响）"));
                        },
                        || Text("全局 +10", TextStyle::new(), Modifier::new()),
                    );
                    let g2 = g;
                    let v2 = v;
                    TextButton(
                        "快照内 +1（discard）",
                        Modifier::new(),
                        move || {
                            let (id, _): (SnapshotId, _) = snapshot::withMutableSnapshot(|| {
                                g2.set(g2.get() + 1);
                            });
                            v2.set(format!("快照内写 +1，未 apply；全局仍 {}", g2.get()));
                            snapshot::discard(id);
                        },
                    );
                },
            );
        },
    );
}

// ══════════════════════ 4. Effects ══════════════════════

pub(crate) fn page_effects() {
    section("Effects（对齐 Compose SideEffect / producedState / snapshotFlow）");
    Text(
        "SideEffect 在每次成功重组后执行；producedState 由协程发布值；snapshotFlow 仅在值变化时发射。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    let trigger = remember(|| mutableStateOf(0i32));
    // SideEffect 计数用 Cell，避免写 state 触发自身重组的无限循环。
    let side_count = remember(|| Rc::new(Cell::new(0i32)));
    let sc = side_count.clone();
    let up = remember(|| mutableStateOf(String::from("initial")));
    let u = up;
    let produced = producedState(0i32, 0, |out| async move {
        out.set(2024);
    });
    let flow_src = trigger;
    let flow = snapshotFlow(move || flow_src.get() / 2);
    let latest = rememberUpdatedState(trigger.get());

    Card(
        Modifier::new().padding(12.0).border(1.0, theme_colors().outline_variant, 12.0),
        move || {
            let sc_effect = sc.clone();
            let sc = sc.clone();
            let u = u;
            let trigger = trigger;
            SideEffect(move || sc_effect.set(sc_effect.get() + 1));
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let sc = sc.clone();
                    let u = u;
                    let trigger = trigger;
                    Text(
                        format!("trigger={}  SideEffect 次数={}  producedState={}  snapshotFlow={}  updatedState={}",
                            trigger.get(), sc.get(), produced.get(), flow.get(), latest.get()),
                        TextStyle::new().size(12.0).weight(600),
                        Modifier::new(),
                    );
                    let t = trigger;
                    let up = u;
                    Button(
                        Modifier::new(),
                        move || {
                            t.update(|v| *v += 1);
                            let _ = up;
                        },
                        || Text("触发重组", TextStyle::new(), Modifier::new()),
                    );
                },
            );
        },
    );
}

// ══════════════════════ 5. Modifier.Node + GraphicsLayer ══════════════════════

pub(crate) fn page_modifier_node_graphics_layer() {
    section("Modifier.Node + GraphicsLayer（对齐 Compose）");
    Text(
        "上：自定义 Modifier.Node 参与测量（收紧约束）；下：graphicsLayer 施加整体 alpha / 旋转 / 缩放。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    let angle = remember(|| mutableStateOf(0.0f32));
    let a = angle;
    // 自定义测量节点：每层扣 8px 水平 padding，视觉上 progressively 收窄。
    let m1 = modifier_node::delta_modifier(Modifier::new(), 8.0, 0.0, 8.0, 0.0);
    Box(m1, Alignment::TopStart, || {
        Card(
            Modifier::new().padding(8.0).border(1.0, theme_colors().outline_variant, 8.0),
            || {
                Text("自定义 Measure Node：宽度被收紧 16px", TextStyle::new(), Modifier::new());
            },
        );
    });
    Spacer(Modifier::new().height(12.0));

    let colors = theme_colors();
    Box(
        Modifier::new()
            .size(160.0, 60.0)
            .background(colors.primary_container, 12.0)
            .graphics_layer(0.7, 1.2, 1.2, a.get(), 0.0, 0.0, 0.5, 0.5),
        Alignment::Center,
        move || {
            Text("alpha 0.7 · scale 1.2", TextStyle::new(), Modifier::new());
        },
    );
    let a2 = a;
    Button(Modifier::new(), move || a2.update(|v| *v = (*v + 15.0) % 360.0), || {
        Text("旋转 +15°", TextStyle::new(), Modifier::new())
    });
}

// ══════════════════════ 6. SubcomposeLayout + Semantics ══════════════════════

/// 演示用 measurePolicy：按 slot 组合 3 个文本，竖向堆叠测量。
struct DemoSubcomposePolicy(core::cell::Cell<bool>);

impl MeasurePolicy for DemoSubcomposePolicy {
    fn measure(
        &mut self,
        ctx: &mut compose::layout::MeasureCtx,
        measurables: &[compose::layout::NodeId],
        constraints: Constraints,
    ) -> MeasureResult {
        if !self.0.get() {
            self.0.set(true);
            for slot in 0..3u64 {
                ctx.subcompose(slot, move || {
                    Text(format!("Subcompose slot {slot}"), TextStyle::new(), Modifier::new());
                });
            }
        }
        SubcomposeStackPolicy.measure(ctx, measurables, constraints)
    }
}

pub(crate) fn page_subcompose_semantics() {
    section("SubcomposeLayout + Semantics（对齐 Compose）");
    Text(
        "SubcomposeLayout 在 measure 期按 slot 请求子组合；Semantics 生成可查询的语义树。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    // 语义树探针：构造一个带标签的按钮，build_tree 后查询。
    let label_state = remember(|| mutableStateOf(String::from("尚未构建语义树")));
    let ls = label_state;
    let probe = remember(|| mutableStateOf(0));
    let p = probe;
    if p.get() == 0 {
        let root = create_root_node(Alignment::TopStart);
        let comp = runtime::Composition::new(root);
        comp.set_content(|| {
            let id = add_node(
                Policy::Leaf,
                Modifier::new()
                    .size(40.0, 40.0)
                    .test_tag("demo-node")
                    .content_description("Demo semantic node")
                    .role(Role::Button),
            );
            with_rt_pub(|rt| rt.nodes[id.index()].on_click = Some(Box::new(|| {})));
            with_content_scope(id, || {});
        });
        perform_layout(root, Constraints::tight(200.0, 200.0));
        let tree = build_tree(root);
        let summary = tree
            .find_by_test_tag("demo-node")
            .and_then(|id| tree.get(id))
            .map(|n| format!("语义节点: label={:?} role={:?} actions={:?}", n.label(), n.config.get(semantics::ROLE), n.config.actions()))
            .unwrap_or_else(|| "未找到语义节点".to_string());
        ls.set(summary);
        p.set(1);
    }

    Card(
        Modifier::new().padding(12.0).border(1.0, theme_colors().outline_variant, 12.0),
        move || {
            let ls = ls;
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    Text(ls.read_clone(), TextStyle::new().size(11.0), Modifier::new());
                    Text(
                        format!("语义树 JSON 长度 = {}", semantics::with_tree(|t| t.to_json().len())),
                        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    // 真正的 SubcomposeLayout：3 个按需组合的子项竖向堆叠。
    SubcomposeLayout(
        Modifier::new().fill_max_width(),
        Box::new(DemoSubcomposePolicy(core::cell::Cell::new(false))),
        || {},
    );
}

// ══════════════════════ 7. pointerInput 手势 ══════════════════════

pub(crate) fn page_pointer_input() {
    section("pointerInput 手势（对齐 Compose detectTapGestures / detectDragGestures）");
    Text(
        "点击/长按/拖动由统一 PointerEvent 驱动，检测器带 tap slop 与 drag slop。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    let taps = remember(|| mutableStateOf(0i32));
    let drags = remember(|| mutableStateOf(0i32));
    let t = taps;
    let d = drags;

    Card(
        Modifier::new().padding(12.0).border(1.0, theme_colors().outline_variant, 12.0),
        move || {
            let t = t;
            let d = d;
            Column(
                Modifier::new().fill_max_width(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    let t = t;
                    let d = d;
                    Text(
                        format!("tap={}  drag 事件={}（点击/拖动下方手势区）", t.get(), d.get()),
                        TextStyle::new().size(12.0).weight(600),
                        Modifier::new(),
                    );
                },
            );
        },
    );
    Spacer(Modifier::new().height(8.0));

    let tap_hits = t;
    let drag_hits = d;
    PointerInput(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(theme_colors().primary_container, 12.0),
        move || {
            Text(
                "手势区：tap / 长按 / 拖动",
                TextStyle::new().color(theme_colors().on_primary_container),
                Modifier::new(),
            );
        },
        detect_tap_gestures(tap_config(
            move |_, _| tap_hits.update(|v| *v += 1),
            |_, _| {},
            |_, _| {},
        )),
    );
    // 第二个手势区演示 drag（同一区域只注册一个 handler，故分开）。
    Spacer(Modifier::new().height(8.0));
    PointerInput(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(theme_colors().secondary_container, 12.0),
        || {
            Text("拖动区：超过 8px 触发 onDrag", TextStyle::new(), Modifier::new());
        },
        detect_drag_gestures(drag_config(
            || {},
            move |_, _| drag_hits.update(|v| *v += 1),
            || {},
        )),
    );
    let _ = PointerEvent::press;
    let _ = PointerEventType::Press;
}

// ══════════════════════ 8. AnimatedVisibility / AnimatedContent ══════════════════════

pub(crate) fn page_animated_transition() {
    section("AnimatedVisibility / AnimatedContent（对齐 Compose）");
    Text(
        "可见性带 Enter/Exit 生命周期（exit 完成才 dispose）；内容切换按 key 播入场转场。",
        TextStyle::new().size(11.0).color(theme_colors().on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(8.0));

    let visible = remember(|| mutableStateOf(true));
    let page = remember(|| mutableStateOf(0u64));
    let v = visible;
    let p = page;

    Button(
        Modifier::new(),
        move || v.update(|b| *b = !*b),
        || Text("切换可见性", TextStyle::new(), Modifier::new()),
    );
    let v2 = v;
    AnimatedVisibilityEx(
        v2.get(),
        compose_material::VisibilityTransition::SlideDown(24.0),
        compose_material::VisibilityTransition::Fade,
        Modifier::new(),
        || {
            Card(
                Modifier::new().padding(10.0).border(1.0, theme_colors().outline_variant, 8.0),
                || Text("可见性内容（Enter/Exit）", TextStyle::new(), Modifier::new()),
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    TextButton(
        "切换内容 state",
        Modifier::new(),
        move || p.update(|v| *v = (*v + 1) % 3),
    );
    let p2 = p;
    AnimatedContent(
        p2.get(),
        compose_material::ContentTransform::SlideInVertically(20.0),
        Modifier::new(),
        move || {
            Text(
                format!("Content state {}", p2.get()),
                TextStyle::new().size(14.0).weight(600),
                Modifier::new(),
            );
        },
    );
}

// 消除未用导入告警（保留供后续扩展）
#[allow(dead_code)]
fn _unused() {
    let _ = DrawCmd::Restore;
}
