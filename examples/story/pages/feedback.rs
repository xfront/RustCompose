//! Feedback 分组的页面实现。

use crate::common::*;
use crate::prelude::*;

pub(crate) fn page_progress_circle() {
    let colors = theme_colors();
    section("ProgressCircle（CircularProgress / Spinner）");
    let v = remember(|| mutableStateOf(0.65_f32));
    let vc = v.clone();
    Row(
        Modifier::new().fill_max_width().height(80.0),
        Arrangement::SpaceEvenly,
        CrossAxisAlignment::Center,
        move || {
            let p = vc.get();
            CircularProgress(Some(p), 56.0, Modifier::new());
            CircularProgress(None, 56.0, Modifier::new());
            Spinner(40.0, Modifier::new());
        },
    );
    Spacer(Modifier::new().height(12.0));
    Text(
        format!("Progress: {:.0}%", v.get() * 100.0),
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
    Slider(v.clone(), Modifier::new().fill_max_width());
}

pub(crate) fn page_color_picker() {
    let colors = theme_colors();
    section("ColorPicker（HSV 取色器）");
    let picked = remember(|| mutableStateOf(colors.primary));
    ColorPicker(picked, Modifier::new());
    Spacer(Modifier::new().height(12.0));
    let c = picked.get();
    Text(
        format!(
            "当前颜色  R{} G{} B{}",
            (c.r * 255.0).round() as i32,
            (c.g * 255.0).round() as i32,
            (c.b * 255.0).round() as i32
        ),
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

pub(crate) fn page_rating() {
    let colors = theme_colors();
    let r = remember(|| mutableStateOf(3i32));
    section("Rating（RatingBar）");
    RatingBar(r.clone(), 5, Modifier::new());
    Spacer(Modifier::new().height(8.0));
    Text(
        format!("{}/5", r.get()),
        TextStyle::new().color(colors.on_surface_variant),
        Modifier::new(),
    );
}

pub(crate) fn page_time_picker() {
    let colors = theme_colors();
    let h = remember(|| mutableStateOf(9));
    let m = remember(|| mutableStateOf(30));
    section("TimePicker");
    TimePicker(h.clone(), m.clone(), Modifier::new().fill_max_width());
    let _ = colors;
}

pub(crate) fn page_date_picker() {
    let colors = theme_colors();
    let sel = remember(|| mutableStateOf(31u32));
    section("DatePicker");
    DatePicker(2026, 10, sel.clone(), Modifier::new().fill_max_width());
    let _ = colors;
}

pub(crate) fn page_snackbar() {
    // 与壳层 `SnackbarHost(sn)` 共用同一宿主句柄（`story_shell` 写入 `SNACK_HOST`）。
    let snack = SNACK_HOST.with(|c| c.get()).expect("snackbar host");
    let colors = theme_colors();
    section("SnackBar & Banner");
    Banner(
        "This is an informational Banner.",
        IconName::Info,
        Some(Box::new(|| {})),
    );
    Spacer(Modifier::new().height(8.0));
    Button(
        Modifier::new(),
        move || {
            snack.show("Saved successfully");
        },
        move || {
            Text(
                "Show Snackbar",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
}

pub(crate) fn page_alert() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    section("Alert（AlertDialog / InlineAlert）");
    InlineAlert(
        AlertKind::Info,
        "Informational alert",
        "This is an informational alert.",
    );
    Spacer(Modifier::new().height(8.0));
    Button(
        Modifier::new(),
        move || open.set(true),
        move || {
            Text(
                "Open AlertDialog",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    if open.get() {
        AlertDialog(
            open.clone(),
            "Confirm",
            "Are you sure?",
            "OK",
            "Cancel",
            move || open.set(false),
        );
    }
}

pub(crate) fn page_command_palette() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    let q = remember(|| mutableStateOf(String::new()));
    section("CommandPalette（Ctrl+K 占位）");
    Button(
        Modifier::new(),
        move || open.set(true),
        move || {
            Text(
                "Open Palette",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    CommandPalette(
        open.clone(),
        q.clone(),
        &[
            "New File".to_string(),
            "Open".to_string(),
            "Save".to_string(),
            "Quit".to_string(),
        ],
        move |_i| {},
    );
}

pub(crate) fn page_empty_state() {
    section("EmptyState");
    EmptyState(
        IconName::Mail,
        "No items yet",
        "Add your first item to get started.",
        None,
        None,
    );
}

pub(crate) fn page_hover_card() {
    let colors = theme_colors();
    section("HoverCard");
    HoverCard(
        220.0,
        move || Text("Hover me", TextStyle::new(), Modifier::new().padding(8.0)),
        move || {
            Text(
                "Tooltip-like hover Card",
                TextStyle::new().color(colors.on_surface_variant),
                Modifier::new().padding(8.0),
            )
        },
    );
}

pub(crate) fn page_notification() {
    let show = remember(|| mutableStateOf(false));
    let colors = theme_colors();
    section("Notification");
    Button(
        Modifier::new(),
        {
            let s = show.clone();
            move || s.set(!s.get())
        },
        move || {
            Text(
                "Toggle Notification",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    Spacer(Modifier::new().height(8.0));
    if show.get() {
        Notification(
            "New message",
            "You have 1 new message",
            AlertKind::Info,
            None,
        );
    }
}

pub(crate) fn page_skeleton() {
    let colors = theme_colors();
    section("Skeleton（loading placeholder）");
    Skeleton(
        200.0,
        80.0,
        Modifier::new().border(1.0, colors.outline_variant, 8.0),
    );
    Spacer(Modifier::new().height(8.0));
    ShimmerPlaceholder(
        200.0,
        40.0,
        Modifier::new().border(1.0, colors.outline_variant, 8.0),
    );
    let _ = colors;
}

pub(crate) fn page_tooltip() {
    let colors = theme_colors();
    section("Tooltip");
    Tooltip("Tooltip Text", move || {
        Text(
            "Hover",
            TextStyle::new(),
            Modifier::new()
                .padding(8.0)
                .border(1.0, colors.outline_variant, 8.0),
        )
    });
}

pub(crate) fn page_dialog() {
    let colors = theme_colors();
    let open = remember(|| mutableStateOf(false));
    section("Dialog（Dialog）");
    Button(
        Modifier::new(),
        move || open.set(true),
        move || {
            Text(
                "Open Dialog",
                TextStyle::new().color(colors.on_primary),
                Modifier::new(),
            );
        },
    );
    Dialog(open.clone(), 320.0, 200.0, move || {
        Text(
            "Dialog content",
            TextStyle::new().color(colors.on_surface),
            Modifier::new().align(Alignment::Center),
        );
    });
}

// ════════════════════════════ Advanced ════════════════════════════
