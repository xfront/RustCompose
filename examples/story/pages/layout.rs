//! Layout 分组的页面实现。

use crate::common::*;
use crate::prelude::*;

pub(crate) fn page_row_col() {
    let colors = theme_colors();
    section("Row & Column");
    Column(
        Modifier::new().fill_max_width(),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Row(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                CrossAxisAlignment::Center,
                move || {
                    Text("Left", TextStyle::new(), Modifier::new());
                    Text(
                        "Right",
                        TextStyle::new().color(colors.on_surface_variant),
                        Modifier::new(),
                    );
                },
            );
            Spacer(Modifier::new().height(8.0));
            Row(
                Modifier::new(),
                Arrangement::Center,
                CrossAxisAlignment::Center,
                move || {
                    for _ in 0..3 {
                        icon(IconName::Star, 20.0, colors.primary, Modifier::new());
                    }
                },
            );
        },
    );
}

pub(crate) fn page_flex() {
    let colors = theme_colors();
    section("Expanded / Flexible（weight）");
    Row(
        Modifier::new().fill_max_width().height(60.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Box(
                Modifier::new()
                    .weight(1.0, true)
                    .background(colors.primary_container, 8.0),
                Alignment::Center,
                move || {
                    Text("weight 1", TextStyle::new(), Modifier::new());
                },
            );
            Spacer(Modifier::new().width(8.0));
            Box(
                Modifier::new()
                    .weight(2.0, true)
                    .background(colors.secondary_container, 8.0),
                Alignment::Center,
                move || {
                    Text("weight 2", TextStyle::new(), Modifier::new());
                },
            );
        },
    );
}

pub(crate) fn page_wrap() {
    section("Wrap（FlowRow）");
    FlowRow(Modifier::new().fill_max_width(), 8.0, move || {
        for w in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"] {
            Chip(w, None, false, Modifier::new(), || {});
        }
    });
}

pub(crate) fn page_stack() {
    let colors = theme_colors();
    section("Stack（Box + Alignment）");
    Box(
        Modifier::new()
            .size(160.0, 120.0)
            .background(colors.surface_variant, 12.0),
        Alignment::Center,
        move || {
            Text("Centered", TextStyle::new(), Modifier::new());
        },
    );
}

pub(crate) fn page_align() {
    let colors = theme_colors();
    section("Align / Spacer");
    Box(
        Modifier::new()
            .fill_max_width()
            .height(80.0)
            .background(colors.surface_variant, 8.0),
        Alignment::BottomEnd,
        move || {
            Text("BottomEnd", TextStyle::new(), Modifier::new().padding(8.0));
        },
    );
}

pub(crate) fn page_modifier() {
    let colors = theme_colors();
    section("Modifier（padding / background / border / radius）");
    Card(
        Modifier::new()
            .padding(16.0)
            .background(colors.primary_container, 16.0)
            .border(1.0, colors.outline, 16.0),
        move || {
            Text("Styled box", TextStyle::new(), Modifier::new());
        },
    );
}

pub(crate) fn page_splitter() {
    let colors = theme_colors();
    section("Splitter（SplitRow）");
    SplitRow(
        Modifier::new().fill_max_width().height(160.0),
        0.5,
        move || {
            Box(
                Modifier::new().background(colors.primary_container, 0.0),
                Alignment::Center,
                move || Text("Pane A", TextStyle::new(), Modifier::new()),
            )
        },
        move || {
            Box(
                Modifier::new().background(colors.secondary_container, 0.0),
                Alignment::Center,
                move || Text("Pane B", TextStyle::new(), Modifier::new()),
            )
        },
    );
}

pub(crate) fn page_zoomable() {
    let colors = theme_colors();
    section("Zoomable（MediaSurface 占位）");
    Box(
        Modifier::new()
            .fill_max_width()
            .height(160.0)
            .background(colors.surface_variant, 12.0),
        Alignment::Center,
        move || {
            Text(
                "Pinch / double-tap to zoom",
                TextStyle::new().color(colors.on_surface_variant),
                Modifier::new(),
            );
        },
    );
}

pub(crate) fn page_divider() {
    let colors = theme_colors();
    section("Divider（horizontal / vertical）");
    Divider(Modifier::new());
    Spacer(Modifier::new().height(8.0));
    Row(
        Modifier::new().height(40.0),
        Arrangement::Start,
        CrossAxisAlignment::Stretch,
        move || {
            Text("A", TextStyle::new(), Modifier::new());
            VerticalDivider(Modifier::new());
            Text("B", TextStyle::new(), Modifier::new());
            let _ = colors;
        },
    );
}

pub(crate) fn page_sized_box() {
    let colors = theme_colors();
    section("SizedBox");
    Row(
        Modifier::new(),
        Arrangement::Start,
        CrossAxisAlignment::Center,
        move || {
            SizedBox(
                60.0,
                60.0,
                Modifier::new().background(colors.primary_container, 8.0),
                || {},
            );
            Spacer(Modifier::new().width(8.0));
            SizedBox(
                100.0,
                40.0,
                Modifier::new().background(colors.secondary_container, 8.0),
                || {},
            );
        },
    );
}

// ════════════════════════════ Navigation ════════════════════════════
