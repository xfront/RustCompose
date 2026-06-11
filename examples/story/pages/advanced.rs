//! Advanced 分组的页面实现。

use crate::common::*;
use crate::prelude::*;

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
        "fn main() {\n    println!(\"Hello, RustCompose!\");\n}",
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
         拖动标签头到上/下/左/右/中心 5 区重新停靠（中心并组）；拖出布局外浮起为窗口；\
         拖动分割条调整比例；双击标签头最大化；右键标签打开菜单（自动隐藏 / 浮动 / 关闭）。",
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
    let colors = theme_colors();
    section("AutoHideToolbar（peek 2px）");
    Toolbar(
        vec![
            AppBarAction::new(IconName::Home, || {}),
            AppBarAction::new(IconName::Search, || {}),
        ],
        Modifier::new().fill_max_width(),
    );
    Spacer(Modifier::new().height(8.0));
    Text(
        "Toolbar auto-hides on scroll (demo).",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

// ════════════════════════════ 入口 ════════════════════════════
