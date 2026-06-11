//! Basics 分组的页面实现。

use crate::common::*;
use crate::prelude::*;

pub(crate) fn page_buttons() {
    let colors = theme_colors();
    let clicks = remember(|| mutableStateOf(0));
    section("Buttons");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Button(
                Modifier::new(),
                move || clicks.set(clicks.get() + 1),
                move || {
                    Text(
                        "Button()",
                        TextStyle::new().color(colors.on_primary),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().width(8.0));
            OutlinedButton(
                Modifier::new(),
                || {},
                move || {
                    Text(
                        "Outlined",
                        TextStyle::new().color(colors.primary),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().width(8.0));
            TextButton("Text", Modifier::new(), || {});
            Spacer(Modifier::new().width(8.0));
            IconButton(IconName::Add, Modifier::new(), || {});
            Spacer(Modifier::new().width(8.0));
            ExtendedFab(IconName::Add, "Extended", Modifier::new(), || {});
        },
    );
    Spacer(Modifier::new().height(16.0));
    section("Floating Action Button（FAB 尺寸：Small / Regular / Large / Extended）");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            FabEx(IconName::Add, FabSize::Small, Modifier::new(), || {});
            Spacer(Modifier::new().width(12.0));
            Fab(IconName::Add, Modifier::new(), || {});
            Spacer(Modifier::new().width(12.0));
            FabEx(IconName::Add, FabSize::Large, Modifier::new(), || {});
            Spacer(Modifier::new().width(12.0));
            ExtendedFab(IconName::Add, "Extended", Modifier::new(), || {});
        },
    );

    // 对齐 rui `advanced_widgets.rs`：ExpandType(Fan/Up/Side) × FabPosition(Right/Left/Center)
    // × ChildrenAnimation(None/Scale/Stagger) × distance/fan_angle/duration/disabled。
    section("ExpandableFab（Fan/Up/Side × Right/Left/Center × 子项动画）");
    let fan = remember(|| mutableStateOf(false));
    let up = remember(|| mutableStateOf(false));
    let side = remember(|| mutableStateOf(false));
    let cfg_fan = remember(|| mutableStateOf(false));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            // 1) Fan / Right / Stagger / 90° / 110px（rui 默认组合）
            ExpandableFabEx(
                FabConfig {
                    icon: IconName::Add,
                    size: FabSize::Regular,
                    expand_type: FabExpandType::Fan,
                    position: FabPosition::Right,
                    children_anim: FabChildrenAnim::Stagger,
                    distance: 110.0,
                    fan_angle: 90.0,
                    duration_ms: 250.0,
                    disabled: false,
                    actions: vec![
                        FabAction::icon(IconName::Edit, "编辑"),
                        FabAction::icon(IconName::Copy, "复制"),
                        FabAction::icon(IconName::Share, "分享"),
                        FabAction::icon(IconName::Delete, "删除"),
                    ],
                    ..FabConfig::legacy(IconName::Add, FabExpandType::Fan, vec![])
                },
                fan,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(44.0));
            // 2) Up / Left / Scale / 64px 间距（紧凑纵向）
            ExpandableFabEx(
                FabConfig {
                    icon: IconName::MoreVert,
                    expand_type: FabExpandType::Up,
                    position: FabPosition::Left,
                    children_anim: FabChildrenAnim::Scale,
                    distance: 64.0,
                    duration_ms: 300.0,
                    actions: vec![
                        FabAction::icon(IconName::Edit, "编辑"),
                        FabAction::icon(IconName::Copy, "复制"),
                        FabAction::icon(IconName::Share, "分享"),
                    ],
                    ..FabConfig::legacy(IconName::MoreVert, FabExpandType::Up, vec![])
                },
                up,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(44.0));
            // 3) Side / Center / None / 90px 间距
            ExpandableFabEx(
                FabConfig {
                    icon: IconName::Add,
                    expand_type: FabExpandType::Side,
                    position: FabPosition::Center,
                    children_anim: FabChildrenAnim::None,
                    distance: 90.0,
                    actions: vec![
                        FabAction::icon(IconName::Star, "收藏"),
                        FabAction::icon(IconName::Mail, "邮件"),
                    ],
                    ..FabConfig::legacy(IconName::Add, FabExpandType::Side, vec![])
                },
                side,
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    section("FabAction 独立回调 / Extended 尺寸 / 颜色覆盖");
    let ext = remember(|| mutableStateOf(false));
    let feedback = remember(|| mutableStateOf("未点击".to_string()));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            let fb = feedback.clone();
            ExpandableFabEx(
                FabConfig {
                    icon: IconName::Add,
                    size: FabSize::Extended,
                    label: "新建".into(),
                    bg_color: Some(colors.primary),
                    fg_color: Some(colors.on_primary),
                    expand_type: FabExpandType::Fan,
                    children_anim: FabChildrenAnim::Scale,
                    actions: vec![
                        FabAction::new(IconName::Edit, "编辑", move || {
                            feedback.set("编辑 tapped".into());
                        }),
                        FabAction::icon(IconName::Copy, "复制"),
                    ],
                    ..FabConfig::legacy(IconName::Add, FabExpandType::Fan, vec![])
                },
                ext,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(16.0));
            Text(fb.read_clone(), TextStyle::new(), Modifier::new());
        },
    );
    Spacer(Modifier::new().height(8.0));
    section("FabConfig（窄扇角 / 禁用态 / Large 尺寸）");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            // 4) Fan / Left / 50° 窄扇面 / Large 主按钮
            ExpandableFabEx(
                FabConfig {
                    icon: IconName::Star,
                    size: FabSize::Large,
                    expand_type: FabExpandType::Fan,
                    position: FabPosition::Left,
                    children_anim: FabChildrenAnim::Scale,
                    distance: 120.0,
                    fan_angle: 50.0,
                    duration_ms: 180.0,
                    actions: vec![
                        FabAction::icon(IconName::Star, "喜欢"),
                        FabAction::icon(IconName::Share, "分享"),
                    ],
                    ..FabConfig::legacy(IconName::Star, FabExpandType::Fan, vec![])
                },
                cfg_fan,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(44.0));
            // 5) disabled：点击不展开（对齐 rui `FabConfig::disabled`）
            ExpandableFabEx(
                FabConfig {
                    icon: IconName::Add,
                    expand_type: FabExpandType::Up,
                    disabled: true,
                    actions: vec![FabAction::icon(IconName::Edit, "编辑")],
                    ..FabConfig::legacy(IconName::Add, FabExpandType::Up, vec![])
                },
                remember(|| mutableStateOf(false)),
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    Text(
        format!("clicks = {}", clicks.get()),
        TextStyle::new(),
        Modifier::new(),
    );
}

pub(crate) fn page_toggles() {
    let a = remember(|| mutableStateOf(true));
    let b = remember(|| mutableStateOf(false));
    let sel = remember(|| mutableStateOf(1usize));
    section("Toggles（Checkbox / Switch / radio）");
    CheckboxRow(a.clone(), "Enable feature", Modifier::new());
    SwitchRow(b.clone(), "Dark mode", Modifier::new());
    RadioGroup(
        &[
            "Small".to_string(),
            "Medium".to_string(),
            "Large".to_string(),
        ],
        sel.clone(),
        Modifier::new(),
    );
}

pub(crate) fn page_sliders() {
    let colors = theme_colors();
    let v = remember(|| mutableStateOf(0.4f32));
    let n = remember(|| mutableStateOf(3));
    section("Value Input（Slider / progress / spinbox）");
    Slider(v.clone(), Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    LinearProgress(Some(v.get()), Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    CircularProgress(Some(v.get()), 48.0, Modifier::new());
    Spacer(Modifier::new().height(8.0));
    SpinBox(n.clone(), 0, 10, Modifier::new());
    let _ = colors;
}

pub(crate) fn page_text_input() {
    let colors = theme_colors();
    let t = remember(|| mutableStateOf(String::from("Hello")));
    let p = remember(|| mutableStateOf(String::new()));
    let o = remember(|| mutableStateOf(String::new()));
    let q = remember(|| mutableStateOf(String::new()));
    let code = remember(|| mutableStateOf(String::new()));
    let tokens = remember(|| mutableStateOf(vec!["rust".to_string(), "gui".to_string()]));
    let draft = remember(|| mutableStateOf(String::new()));
    section("Text Input（filled / outlined / password / search / otp / token）");
    TextField(t.clone(), "Filled", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    OutlinedTextField(p.clone(), "Outlined", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    PasswordField(o.clone(), "Password", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    SearchBar(q.clone(), "Search…", Modifier::new().fill_max_width());
    Spacer(Modifier::new().height(8.0));
    OtpInput(code.clone(), 4, Modifier::new());
    Spacer(Modifier::new().height(8.0));
    TokenInput(
        tokens.clone(),
        draft.clone(),
        Modifier::new().fill_max_width(),
    );
    let _ = colors;
}

pub(crate) fn page_icons() {
    let colors = theme_colors();
    section("Icon（全套矢量图标）");
    let names = [
        IconName::Home,
        IconName::Star,
        IconName::Heart,
        IconName::Settings,
        IconName::User,
        IconName::Search,
        IconName::Bell,
        IconName::Camera,
    ];
    FlowRow(Modifier::new().fill_max_width(), 8.0, move || {
        for n in names {
            icon(n, 28.0, colors.primary, Modifier::new().padding(6.0));
        }
    });
}

pub(crate) fn page_avatars() {
    let colors = theme_colors();
    section("AvatarGroup & Badges");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Avatar("AB", 40.0, Modifier::new());
            Spacer(Modifier::new().width(8.0));
            Avatar("CD", 40.0, Modifier::new());
            Spacer(Modifier::new().width(8.0));
            AvatarGroup(
                &["AB".to_string(), "CD".to_string(), "EF".to_string()],
                40.0,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(8.0));
            BadgedBox("3", Modifier::new(), move || {
                Avatar("!", 40.0, Modifier::new())
            });
        },
    );
    let _ = colors;
}

pub(crate) fn page_group_box() {
    let colors = theme_colors();
    section("GroupBox");
    GroupBox("Options", Modifier::new().fill_max_width(), move || {
        Text(
            "Grouped controls go here",
            TextStyle::new().color(colors.on_surface_variant),
            Modifier::new(),
        );
    });
}

pub(crate) fn page_kbd() {
    section("Kbd（快捷键提示）");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            Kbd(&["Ctrl".to_string(), "K".to_string()], Modifier::new());
        },
    );
}

pub(crate) fn page_link() {
    let colors = theme_colors();
    section("Link");
    Link("https://example.com", Modifier::new(), || {});
    Spacer(Modifier::new().height(8.0));
    Text(
        "(Link 自带主题色下划线，点击触发回调)",
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

pub(crate) fn page_context_menu() {
    let colors = theme_colors();
    section("ContextMenu（右键）");
    ContextMenu(
        &["Cut".to_string(), "Copy".to_string(), "Paste".to_string()],
        Modifier::new()
            .fill_max_width()
            .height(60.0)
            .border(1.0, colors.outline_variant, 8.0),
        move |_i| {},
        move || {
            Text(
                "Right-click here",
                TextStyle::new(),
                Modifier::new().align(Alignment::Center),
            )
        },
    );
}

pub(crate) fn page_menu_demo() {
    let sel = remember(|| mutableStateOf(0usize));
    section("MenuBar / Dropdown");
    Dropdown(
        &[
            "Option A".to_string(),
            "Option B".to_string(),
            "Option C".to_string(),
        ],
        sel.clone(),
        Modifier::new(),
    );
}

// ════════════════════════════ Data Display ════════════════════════════
