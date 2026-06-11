//! Advanced 分组的页面实现。

use compose_material::widgets::{dock_inner_guides, dock_outer_guides};
use crate::common::*;
use crate::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

pub(crate) fn page_animated_gif() {
    let colors = theme_colors();
    let t = Pulse(2.0);
    section("Animated GIF（模拟帧动画播放器）");
    Box(
        Modifier::new().fill_max_width().height(200.0),
        Alignment::Center,
        move || {
            let colors = theme_colors();
            let t = t;
            Canvas(
                Modifier::new().fill_max_size().clip(12.0),
                move |b, x, y, w, h| {
                    b.fill_rrect(x, y, w, h, 12.0, colors.surface_variant);
                    let bounce = 1.0 - (2.0 * t - 1.0).abs();
                    let by = y + h - 30.0 - bounce * (h - 60.0);
                    b.fill_circle(x + w * 0.5, by, 14.0, colors.primary);
                    let ang = t * std::f32::consts::TAU;
                    let cx = x + 56.0;
                    let cy = y + 56.0;
                    let mut pts: Vec<(f32, f32)> = Vec::new();
                    for i in 0..6 {
                        let a = ang + i as f32 * std::f32::consts::TAU / 6.0;
                        pts.push((cx + a.cos() * 24.0, cy + a.sin() * 24.0));
                    }
                    b.stroke_path(&pts, colors.secondary, 4.0);
                },
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    Text(
        "模拟 GIF：循环帧动画（弹跳小球 + 旋转六边形）",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(4.0));
    Text(
        "（接入 GIF 解码器即可播放真实动图；此处以矢量动画演示播放效果）",
        TextStyle::new().size(11.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
}

// ════════════════════════════ Demos ════════════════════════════

pub(crate) fn page_ink_well() {
    let colors = theme_colors();
    section("InkWell（ripple 静态近似）");
    InkWell(
        Modifier::new()
            .fill_max_width()
            .height(56.0)
            .background(colors.primary_container, 12.0),
        || {},
        move || {
            Text(
                "Tappable Surface",
                TextStyle::new().color(colors.on_primary_container),
                Modifier::new().align(Alignment::Center),
            );
        },
    );
}

pub(crate) fn page_popup_menu() {
    section("PopupMenuButton");
    PopupMenuButton(
        &[
            "Edit".to_string(),
            "Duplicate".to_string(),
            "Delete".to_string(),
        ],
        Modifier::new(),
        move |_i| {},
    );
}

pub(crate) fn page_animated() {
    let colors = theme_colors();
    let on = remember(|| mutableStateOf(true));
    section("AnimatedWidget（Crossfade）");
    Crossfade(if on.get() { 1 } else { 0 }, Modifier::new(), move || {
        Text(
            if on.get() { "Visible" } else { "Hidden" },
            TextStyle::new().color(colors.primary),
            Modifier::new(),
        );
    });
    Spacer(Modifier::new().height(8.0));
    Spinner(24.0, Modifier::new());
}

/// 程序生成一张测试位图（PNG 字节），喂给 `Image` 控件走完整「解码 → 上传 → 绘制」管线。
pub(crate) fn demo_png_bytes() -> Vec<u8> {
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    let w = 160u32;
    let h = 120u32;
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let r = (x * 255 / w) as u8;
            let g = (y * 255 / h) as u8;
            let b = 128u8;
            img.put_pixel(x, y, Rgba([r, g, b, 255u8]));
        }
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    let _ = DynamicImage::ImageRgba8(img).write_to(&mut cursor, ImageFormat::Png);
    cursor.into_inner()
}

pub(crate) fn page_images() {
    section("Images（真实位图：解码 → 上传 → 绘制）");
    // 仅在首次组合生成一次 PNG 字节（之后每帧复用同一份），交给 `Image` 异步解码显示。
    let bytes = remember(|| demo_png_bytes());
    // 用户通过系统文件对话框选择的图片路径（None = 尚未选择）。
    let picked = remember(|| mutableStateOf(None::<std::path::PathBuf>));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Button(
                Modifier::new(),
                move || {
                    open_image_dialog(move |path| {
                        picked.set(path);
                    });
                },
                || Text("打开图片…", TextStyle::new(), Modifier::new()),
            );
            Spacer(Modifier::new().width(8.0));
            if let Some(path) = picked.read_clone() {
                // 所选文件：每次改选都会触发 Image 控件重新解码（按 source 变化刷新）。
                Image(
                    Modifier::new().size(200.0, 150.0),
                    ImageSource::File(path),
                );
            } else {
                Image(
                    Modifier::new().size(160.0, 120.0),
                    ImageSource::Bytes(bytes.clone()),
                );
            }
        },
    );
}

pub(crate) fn page_native_dialogs() {
    let open = remember(|| mutableStateOf(false));
    section("Native Dialogs（Dialog）");
    Button(
        Modifier::new(),
        {
            let o = open.clone();
            move || o.set(true)
        },
        move || {
            let colors = theme_colors();
            Text(
                "Open Dialog",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    Dialog(open.clone(), 300.0, 160.0, move || {
        let colors = theme_colors();
        Text(
            "Native-style Dialog",
            TextStyle::new().color(colors.on_surface),
            Modifier::new().align(Alignment::Center),
        );
    });
}

pub(crate) fn page_scroll_area() {
    let colors = theme_colors();
    section("ScrollArea（VerticalScroll）");
    VerticalScroll(
        Modifier::new()
            .fill_max_width()
            .height(160.0)
            .border(1.0, colors.outline_variant, 12.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            for i in 0..20 {
                Text(
                    format!("Line {}", i),
                    TextStyle::new(),
                    Modifier::new().padding(4.0),
                );
            }
        },
    );
}

pub(crate) fn page_builders() {
    let colors = theme_colors();
    section("Builder Helpers（Box 自绘占位）");
    Box(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(colors.primary_container, 12.0),
        Alignment::Center,
        move || {
            icon(IconName::Brush, 32.0, colors.primary, Modifier::new());
        },
    );
}

pub(crate) fn page_calendar() {
    let colors = theme_colors();
    let sel = remember(|| mutableStateOf(31u32));
    section("Calendar");
    Calendar(2026, 10, sel.clone(), Modifier::new().fill_max_width());
    let _ = colors;
}

pub(crate) fn page_code_editor() {
    let colors = theme_colors();
    section("CodeEditor（CodeBlock）");
    CodeBlock(
        "fn main() {\n    println!(\"Hello, compose-rs!\");\n}",
        Modifier::new()
            .fill_max_width()
            .border(1.0, colors.outline_variant, 8.0),
    );
}

pub(crate) fn page_docking() {
    let colors = theme_colors();
    section("Docking（VS2022 IDE 工作区）");
    Text(
        "整个 Storybook 外壳就是一个 IDE：左侧「解决方案资源管理器」本身是 DockPanel，\
         与工具箱 / 文档 / 输出 / 错误列表 / 属性 同处一棵可拖拽停靠树。\
         拖标签头时出现 VS 式停靠指南：目标组中心的 5 向罗盘 + 主窗口四边的外圈菱形，\
         只有命中指南才投放（中心并组、边区分割）；拖到别的标签条上按插入光标并入该组；\
         拖出布局外浮起为窗口（八向缩放、双击标题回靠、拖回主窗口指南即重新停靠）。\
         图钉 = 自动隐藏到边条（竖排标题、悬停滑出预览）；分割条悬停染主色并切换 ↔/↕ 光标、双击均分；\
         双击标签头最大化；中键点击标签关闭；右键标签打开 VS 菜单（关闭 / 关闭其他 / 全部关闭 / 浮动 / 自动隐藏 / 最大化）；\
         拖拽中按 Esc 取消并恢复原状（分割比例 / 浮动窗位置），浮动窗拖动时整窗半透明 ghosting。",
        TextStyle::new().size(12.5).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(10.0));

    // 复用外壳唯一的 DockState（不再注册第二个 DockLayout：命中表是全局单例）。
    let Some(state) = shell_dock() else { return };
    let Some(ids) = shell_dock_ids() else { return };
    let saved = remember(|| mutableStateOf(String::new()));

    section("布局持久化");
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            TextButton("保存 IDE 布局", Modifier::new(), move || {
                saved.set(state.save_full_state());
            });
            TextButton("恢复 IDE 布局", Modifier::new(), move || {
                let s = saved.with_value(|v| v.clone());
                if !s.is_empty() {
                    state.restore_full_state(&s);
                }
            });
            TextButton("重置为默认工作区", Modifier::new(), move || {
                state.reset(ide_default_tree(ids));
            });
        },
    );
    Spacer(Modifier::new().height(10.0));

    section("面板操作（作用于真实 IDE 布局）");
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            TextButton("自动隐藏侧栏", Modifier::new(), move || {
                state.toggle_auto_hide(ids.sidebar);
            });
            TextButton("自动隐藏属性", Modifier::new(), move || {
                state.toggle_auto_hide(ids.properties);
            });
            TextButton("浮动工具箱", Modifier::new(), move || {
                state.toggle_float(ids.toolbox);
            });
            TextButton("最大化文档", Modifier::new(), move || {
                state.toggle_maximize(ids.documents);
            });
        },
    );
    Spacer(Modifier::new().height(6.0));
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            TextButton("输出 → 独立浮动窗口", Modifier::new(), move || {
                state.move_to_new_window(ids.output, None);
            });
            TextButton("错误列表 → 停靠到工具箱窗口", Modifier::new(), move || {
                // 跨窗口停靠：目标在浮动窗口里时，自动并入该窗口的标签组
                if state.window_of(ids.toolbox).is_some() {
                    state.dock_to(
                        ids.errors,
                        DockTarget::OnDockable(ids.toolbox),
                        DockRegion::Center,
                        None,
                    );
                }
            });
        },
    );
    Spacer(Modifier::new().height(12.0));

    // 实时布局摘要（组合期订阅 DockModel，任何停靠变化自动刷新）
    let m = state.model();
    let tree_ids = m.root.as_ref().map(|r| r.collect_ids()).unwrap_or_default();
    section("当前工作区");
    Text(
        format!(
            "主窗口面板 {} 个 · 自动隐藏 {} 组 · 浮动窗口 {} 个 · 最大化: {}",
            tree_ids.len(),
            m.hidden.len(),
            m.floating.len(),
            if m.maximized.is_some() { "是" } else { "否" },
        ),
        TextStyle::new().size(12.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    for f in &m.floating {
        Text(
            format!("浮动窗口 #{}: {:?}", f.id, f.items()),
            TextStyle::new().size(11.5).color(colors.on_surface_variant),
            Modifier::new().padding_hv(0.0, 2.0),
        );
    }
    Spacer(Modifier::new().height(10.0));

    section("停靠指南图例");
    Canvas(
        Modifier::new().fill_max_width().height(150.0).no_hit(),
        move |b, x, y, w, _h| {
            let r = (x + 8.0, y + 8.0, (w - 16.0).min(420.0), 134.0);
            b.fill_rrect(r.0, r.1, r.2, r.3, 6.0, colors.surface_container_low);
            b.stroke_rrect(r.0, r.1, r.2, r.3, 6.0, colors.outline_variant, 1.0);
            for (dir, gr) in dock_outer_guides(r) {
                let hot = dir == DockDirection::West;
                b.fill_rrect(gr.0, gr.1, gr.2, gr.3, 5.0, if hot { colors.primary } else { colors.Surface });
                b.stroke_rrect(gr.0, gr.1, gr.2, gr.3, 5.0, colors.outline, 1.0);
            }
            for (dir, gr) in dock_inner_guides(r) {
                let hot = dir == DockDirection::Center;
                b.fill_rrect(gr.0, gr.1, gr.2, gr.3, 5.0, if hot { colors.primary } else { colors.Surface });
                b.stroke_rrect(gr.0, gr.1, gr.2, gr.3, 5.0, colors.outline, 1.0);
            }
            let style = TextStyle::new().size(11.0).color(colors.on_surface_variant);
            b.draw_text(r.0 + r.2 + 16.0, y + 16.0, "外圈 4 指南 = 整窗四边分割", &style, 260.0);
            b.draw_text(r.0 + r.2 + 16.0, y + 40.0, "中心罗盘 5 指南 = 相对目标组分割 / 并组", &style, 260.0);
            b.draw_text(r.0 + r.2 + 16.0, y + 64.0, "标签条 = 插入光标处并入该标签组", &style, 260.0);
            b.draw_text(r.0 + r.2 + 16.0, y + 88.0, "布局外抬起 = 浮起为独立窗口", &style, 260.0);
            b.draw_text(r.0 + r.2 + 16.0, y + 112.0, "无指南处抬起 = 取消（VS 手感）", &style, 260.0);
        },
    );
    Spacer(Modifier::new().height(10.0));

    section("布局序列化（mdl2 · 实时）");
    let dump = state.save_full_state();
    CodeBlock(
        &dump,
        Modifier::new()
            .fill_max_width()
            .border(1.0, colors.outline_variant, 8.0),
    );
}

pub(crate) fn page_rich_text() {
    section("RichTextEditor（Quill 风格富文本编辑器）");
    Text(
        "工具栏：¶/H1/H2/H3 块格式，B/I/U/S/</> 行内格式，❝引用 UL/OL/☑ 列表，⌫F 清除，↶/↷ 撤销重做。\n\
         快捷键：Ctrl+Z 撤销、Ctrl+Y/Ctrl+Shift+Z 重做、Ctrl+A/C/X/V 全选/剪切/粘贴、F3 查找下一个。",
        TextStyle::new()
            .size(13.0)
            .color(theme_colors().on_surface_variant),
        Modifier::new().fill_max_width(),
    );
    Spacer(Modifier::new().height(8.0));

    // 编辑器须每帧组合；内容仅在首次注入一次。
    let h = RichTextEditor(Modifier::new().fill_max_width().height(360.0));
    let seeded = remember(|| mutableStateOf(false));
    if !seeded.get() {
        seeded.set(true);
        h.set_html(
            "<h2>Quill-like Editor</h2>\
             <p>这是一段 <b>加粗</b>、<i>斜体</i> 与 <span style=\"color:#3577ff\">彩色</span> 混排文本，\
             支持 <u>下划线</u>、<s>删除线</s> 与 <code>行内代码</code>。</p>\
             <blockquote>引用块用于强调一段话，常用于提示或引言。</blockquote>\
             <ul><li><input type=\"checkbox\" checked> 撤销 / 重做</li>\
             <li><input type=\"checkbox\" checked> 块与行内格式</li>\
             <li><input type=\"checkbox\"> 导出 HTML / Markdown</li></ul>\
             <p>试用工具栏或快捷键编辑本段，然后 Ctrl+Z 撤销。</p>",
        );
    }

    Spacer(Modifier::new().height(8.0));
    // 查找框：输入即驱动编辑器高亮匹配，F3 跳到下一个。
    let find_q = remember(|| mutableStateOf(String::new()));
    let applied = remember(|| mutableStateOf(String::new()));
    let q = find_q.with_value(|v| v.clone());
    if q != applied.with_value(|v| v.clone()) {
        applied.set(q.clone());
        h.set_find(&q);
    }
    let hf = h.clone();
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text("查找:", TextStyle::new().size(13.0), Modifier::new());
            Spacer(Modifier::new().width(6.0));
            TextField(
                find_q.clone(),
                "输入关键字，F3 下一个",
                Modifier::new().width(260.0),
            );
            Spacer(Modifier::new().width(10.0));
            Text(
                format!("命中 {} 处", hf.find_match_count()),
                TextStyle::new()
                    .size(13.0)
                    .color(theme_colors().on_surface_variant),
                Modifier::new(),
            );
        },
    );

    Spacer(Modifier::new().height(8.0));
    // 导出：复制到剪贴板 + 字数统计。
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let hh = h.clone();
            let hm = h.clone();
            let hc = h.clone();
            TextButton(
                "复制 HTML",
                Modifier::new(),
                move || copy_text(&hh.to_html()),
            );
            Spacer(Modifier::new().width(8.0));
            TextButton(
                "复制 Markdown",
                Modifier::new(),
                move || copy_text(&hm.to_markdown()),
            );
            Spacer(Modifier::new().width(10.0));
            Text(
                format!("字数 {}", hc.char_count()),
                TextStyle::new()
                    .size(13.0)
                    .color(theme_colors().on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

pub(crate) fn page_settings() {
    let a = remember(|| mutableStateOf(true));
    let b = remember(|| mutableStateOf(false));
    section("Settings（SettingsSection / SettingsItem）");
    SettingsSection("General", Modifier::new().fill_max_width(), move || {
        SettingsItem(
            "Wi-Fi",
            Some("Toggle wireless"),
            Modifier::new().fill_max_width(),
            move || Switch(a.clone(), Modifier::new()),
        );
        SettingsItem(
            "Bluetooth",
            Some("Toggle bluetooth"),
            Modifier::new().fill_max_width(),
            move || Switch(b.clone(), Modifier::new()),
        );
    });
}

pub(crate) fn page_auto_hide() {
    use compose::layout::{Alignment, Box, NodeId as LayoutNodeId, scroll_metrics};
    use compose::runtime::{add_frame_driver, disposable_effect, remove_frame_driver};

    let colors = theme_colors();
    section("AutoHideToolbar — 下滚隐藏 / 上滚显示（真实可交互）");

    let hidden = remember(|| mutableStateOf(false));
    let auto = remember(|| mutableStateOf(true));
    let hint = remember(|| mutableStateOf("—"));
    // 滚动列表节点每帧重建：Rc<Cell> 跨重组持有最新 id 与上一帧偏移。
    let list_id = remember(|| Rc::new(Cell::new(None::<LayoutNodeId>)));
    let last_off = remember(|| Rc::new(Cell::new(0.0f32)));

    // 帧驱动比较相邻帧的滚动偏移判断方向：下滚（offset 增大）隐藏、上滚（减小）显示，
    // 回到顶部强制显示；无位移（动画补帧）跳过判定，避免把方向抹平。
    // 只在方向结论变化时写状态（MutableState::set 同值不失效），不自激循环。
    let drv = remember(|| {
        let (auto, hidden, hint, list_id, last_off) = (
            auto,
            hidden,
            hint,
            list_id.clone(),
            last_off.clone(),
        );
        add_frame_driver(move || {
            if !auto.get() {
                return;
            }
            let Some(id) = list_id.get() else { return };
            let Some((off, content, viewport)) = scroll_metrics(id) else { return };
            let prev = last_off.get();
            last_off.set(off);
            let dy = off - prev;
            if dy.abs() < 0.5 {
                return;
            }
            let at_top = viewport <= 0.0 || off <= 0.5;
            let at_bottom = content > viewport && off + viewport >= content - 0.5;
            if dy > 0.0 && !at_bottom {
                hidden.set(true);
                hint.set("↓ 下滚：工具条隐藏");
            } else if dy < 0.0 || at_top {
                hidden.set(false);
                hint.set(if at_top { "— 已到顶部：工具条显示" } else { "↑ 上滚：工具条显示" });
            }
        })
    });
    // 页面切走、作用域销毁时注销帧驱动，避免空转。
    disposable_effect(0).on_dispose(move || remove_frame_driver(drv));

    // 覆盖式「文章流」：body（提示行 + 滚动列表）铺满整个容器，工具条作为后置
    // 子项悬浮在内容上方展开（VMware 连接栏同款），不把内容往下推。
    let list_cell = list_id.clone();
    let hint_fg = colors.on_surface_variant;
    let content_fg = colors.on_surface;
    // Scaffold 托管：body 是常规内容槽，AutoHideToolbar 由 overlay 覆盖层槽位承载
    // （后组合 ⇒ 悬浮在 body 上一层，BottomSheet 同款层级，页面不自管叠加）。
    let list_cell2 = list_cell.clone();
    let hidden_ov = hidden;
    Scaffold(
        Modifier::new()
            .fill_max_width()
            .height(360.0)
            .background(colors.surface_container_lowest, 12.0)
            .border(1.0, colors.outline_variant, 12.0)
            .clip(12.0),
        None,
        None,
        Some(Box::new(move || {
            let h = hidden_ov;
            AutoHideToolbar(h, 48.0, Modifier::new(), move || {
                Toolbar(
                    vec![
                        AppBarAction::new(IconName::Home, || {}),
                        AppBarAction::new(IconName::Search, || {}),
                        AppBarAction::new(IconName::Settings, || {}),
                    ],
                    Modifier::new().fill_max_width(),
                );
            });
        })),
        move || {
            // 闭包坑：内层 Fn 闭包里不能 clone 捕获的非 Copy 值（E0507），先在外层克隆。
            let list_cell = list_cell2.clone();
            Column(
                Modifier::new().fill_max_size(),
                Arrangement::Start,
                CrossAxisAlignment::Stretch,
                move || {
                    // 实时反馈：当前滚动方向 + 工具条状态。
                    let h2 = hidden;
                    Row(
                        Modifier::new().fill_max_width().height(28.0).padding_hv(12.0, 0.0),
                        Arrangement::SpaceBetween,
                        CrossAxisAlignment::Center,
                        move || {
                            Text(
                                hint.get(),
                                TextStyle::new().size(11.5).color(hint_fg),
                                Modifier::new(),
                            );
                            Text(
                                if h2.get() { "工具条：已隐藏" } else { "工具条：显示中" },
                                TextStyle::new().size(11.5).color(hint_fg),
                                Modifier::new(),
                            );
                        },
                    );

                    let cell = list_cell.clone();
                    let id = VerticalScroll(
                        Modifier::new().fill_max_width().weight(1.0, true),
                        Arrangement::Start,
                        CrossAxisAlignment::Stretch,
                        move || {
                            for i in 0..60 {
                                let n = i + 1;
                                Column(
                                    Modifier::new().fill_max_width().padding_hv(16.0, 8.0),
                                    Arrangement::Start,
                                    CrossAxisAlignment::Start,
                                    move || {
                                        Text(
                                            format!("消息 {n:02}"),
                                            TextStyle::new().size(14.0).weight(600).color(content_fg),
                                            Modifier::new(),
                                        );
                                        Spacer(Modifier::new().height(2.0));
                                        Text(
                                            "向下滚动隐藏工具条，向上滚动显示；点顶部细条也可展开。",
                                            TextStyle::new().size(11.0).color(hint_fg),
                                            Modifier::new(),
                                        );
                                    },
                                );
                            }
                        },
                    );
                    cell.set(Some(id));
                },
            );
        },
    );

    Spacer(Modifier::new().height(10.0));
    Row(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Text("自动检测", TextStyle::new().size(12.0), Modifier::new());
            compose_material::Switch(auto, Modifier::new());
            let h = hidden;
            TextButton(
                if h.get() { "展开工具条" } else { "隐藏工具条" },
                Modifier::new(),
                move || h.update(|v| *v = !*v),
            );
        },
    );
}

// ════════════════════════════ Multiple Windows ════════════════════════════

/// 本示例打开的子窗口句柄（「关闭全部」用）。
#[cfg(all(not(target_os = "android"), not(target_env = "ohos"), not(target_os = "ios")))]
std::thread_local! {
    static CHILD_WINDOWS: std::cell::RefCell<Vec<compose::WindowHandle>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// 子窗口应用：独立组合根 / 独立视口，可与主窗口同时交互（计数互不影响）。
#[cfg(all(not(target_os = "android"), not(target_env = "ohos"), not(target_os = "ios")))]
struct DemoChildWindow {
    n: usize,
    dark: bool,
}

#[cfg(all(not(target_os = "android"), not(target_env = "ohos"), not(target_os = "ios")))]
impl App for DemoChildWindow {
    fn theme(&self) -> ColorScheme {
        if self.dark { ColorScheme::dark() } else { ColorScheme::light() }
    }

    fn content(&self) {
        let n = self.n;
        // 组合期读取本窗口句柄：「关闭本窗口」按钮要用它。
        let me = compose::current_window();
        let count = remember(|| mutableStateOf(0i32));
        MaterialTheme(self.theme(), move || {
            let colors = theme_colors();
            Column(
                Modifier::new()
                    .fill_max_size()
                    .padding_ltrb(22.0, 20.0, 22.0, 20.0),
                Arrangement::Start,
                CrossAxisAlignment::Start,
                move || {
                    Text(
                        format!("Window #{n}"),
                        TextStyle::new().size(22.0).weight(720).color(colors.on_surface),
                        Modifier::new(),
                    );
                    Spacer(Modifier::new().height(6.0));
                    Text(
                        "独立组合根 · 独立视口 · 共享同一运行时 arena",
                        TextStyle::new().size(12.0).color(colors.on_surface_variant),
                        Modifier::new(),
                    );
                    Spacer(Modifier::new().height(18.0));
                    Text(
                        format!("本窗口计数：{}", count.get()),
                        TextStyle::new().size(15.0).weight(600).color(colors.on_surface),
                        Modifier::new(),
                    );
                    Spacer(Modifier::new().height(12.0));
                    Row(
                        Modifier::new(),
                        Arrangement::Start,
                        CrossAxisAlignment::Center,
                        move || {
                            Button(
                                Modifier::new(),
                                move || count.update(|v| *v += 1),
                                move || {
                                    Text(
                                        "+1",
                                        TextStyle::new().color(colors.on_primary),
                                        Modifier::new(),
                                    )
                                },
                            );
                            Spacer(Modifier::new().width(10.0));
                            Button(
                                Modifier::new(),
                                move || {
                                    if let Some(h) = me {
                                        compose::close_window(h);
                                    }
                                },
                                move || {
                                    Text(
                                        "关闭本窗口",
                                        TextStyle::new().color(colors.on_primary),
                                        Modifier::new(),
                                    )
                                },
                            );
                        },
                    );
                    Spacer(Modifier::new().height(16.0));
                    Text(
                        "两个窗口可同时跑动画：续帧按「根」判定，静止窗口不耗 CPU。",
                        TextStyle::new().size(11.5).color(colors.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
        });
    }
}

#[cfg(all(not(target_os = "android"), not(target_env = "ohos"), not(target_os = "ios")))]
pub(crate) fn page_multi_window() {
    let colors = theme_colors();
    let opened = remember(|| mutableStateOf(0usize));
    section("Multiple Windows — 每窗口一个组合根（共享同一运行时 arena）");
    Text(
        "点「打开新窗口」创建独立窗口：各自拥有组合树、视口与 GPU 表面；节点/作用域仍全局唯一，\
         状态更新只重绘受影响的窗口（其余窗口保持休眠）。",
        TextStyle::new().size(13.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
    Spacer(Modifier::new().height(14.0));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Button(
                Modifier::new(),
                move || {
                    let n = opened.get() + 1;
                    opened.set(n);
                    let h = open_window(
                        WindowConfig::sized(format!("compose-rs — Window #{n}"), 480.0, 360.0),
                        DemoChildWindow { n, dark: theme_colors().is_dark() },
                    );
                    CHILD_WINDOWS.with(|w| w.borrow_mut().push(h));
                },
                move || {
                    Text(
                        "打开新窗口",
                        TextStyle::new().color(theme_colors().on_primary),
                        Modifier::new(),
                    )
                },
            );
            Spacer(Modifier::new().width(10.0));
            TextButton("关闭全部子窗口", Modifier::new(), || {
                CHILD_WINDOWS.with(|w| {
                    for h in w.borrow_mut().drain(..) {
                        close_window(h);
                    }
                });
            });
            Spacer(Modifier::new().width(12.0));
            Text(
                format!("已打开 {} 个", opened.get()),
                TextStyle::new().size(12.0).color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(16.0));
    Text(
        "关闭的子窗口会回收其组合根 / 节点 / 视口；最后一个窗口关闭时应用退出。",
        TextStyle::new().size(11.5).color(colors.on_surface_variant),
        Modifier::new(),
    );
}

/// 宿主驱动单窗口的平台（Android / iOS / OHOS）：多窗口由各自平台后端提供。
#[cfg(any(target_os = "android", target_env = "ohos", target_os = "ios"))]
pub(crate) fn page_multi_window() {
    let colors = theme_colors();
    section("Multiple Windows");
    Text(
        "当前平台由宿主驱动单窗口（多窗口需要平台后端提供多个 Surface / 窗口句柄）。",
        TextStyle::new().size(13.0).color(colors.on_surface_variant),
        Modifier::new(),
    );
}

// ════════════════════════════ 入口 ════════════════════════════
