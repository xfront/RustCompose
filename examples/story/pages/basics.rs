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
    section("Floating Action Button（FAB 尺寸）");
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
    Spacer(Modifier::new().height(16.0));
    section("ExpandableFab（点击展开动作：Up / Side / Fan）");
    let up = remember(|| mutableStateOf(false));
    let side = remember(|| mutableStateOf(false));
    let fan = remember(|| mutableStateOf(false));
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            ExpandableFab(
                IconName::Add,
                &[
                    (IconName::Edit, "编辑".into()),
                    (IconName::Copy, "复制".into()),
                    (IconName::Share, "分享".into()),
                    (IconName::Delete, "删除".into()),
                ],
                FabExpandType::Up,
                up,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(40.0));
            ExpandableFab(
                IconName::MoreVert,
                &[
                    (IconName::Edit, "编辑".into()),
                    (IconName::Copy, "复制".into()),
                    (IconName::Share, "分享".into()),
                ],
                FabExpandType::Side,
                side,
                Modifier::new(),
            );
            Spacer(Modifier::new().width(40.0));
            ExpandableFab(
                IconName::Add,
                &[
                    (IconName::Edit, "编辑".into()),
                    (IconName::Copy, "复制".into()),
                    (IconName::Share, "分享".into()),
                    (IconName::Delete, "删除".into()),
                ],
                FabExpandType::Fan,
                fan,
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
