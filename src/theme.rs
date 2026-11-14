//! Fonts, colors and widget styling.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x17, 0x17, 0x1a);
pub const SURFACE: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x23);
pub const HOVER: Color32 = Color32::from_rgb(0x27, 0x27, 0x2c);
pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x30);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe4, 0xde);
pub const MUTED: Color32 = Color32::from_rgb(0x8c, 0x88, 0x80);
pub const FAINT: Color32 = Color32::from_rgb(0x55, 0x53, 0x4f);
pub const ACCENT: Color32 = Color32::from_rgb(0xd4, 0xa3, 0x73);
pub const DANGER: Color32 = Color32::from_rgb(0xe5, 0x6b, 0x6b);

pub const BODY_SIZE: f32 = 17.0;
pub const TITLE_SIZE: f32 = 32.0;

/// Text color swatches (`None` = default).
pub const TEXT_COLORS: [(&str, Option<Color32>); 10] = [
    ("Default", None),
    ("Gray", Some(Color32::from_rgb(0x8c, 0x88, 0x80))),
    ("Red", Some(Color32::from_rgb(0xef, 0x6f, 0x6c))),
    ("Orange", Some(Color32::from_rgb(0xf0, 0xa3, 0x5e))),
    ("Yellow", Some(Color32::from_rgb(0xe9, 0xcf, 0x6a))),
    ("Green", Some(Color32::from_rgb(0x8f, 0xcf, 0x7a))),
    ("Teal", Some(Color32::from_rgb(0x5c, 0xcf, 0xc0))),
    ("Blue", Some(Color32::from_rgb(0x6a, 0xa8, 0xf0))),
    ("Purple", Some(Color32::from_rgb(0xb4, 0x8e, 0xf0))),
    ("Pink", Some(Color32::from_rgb(0xf0, 0x8f, 0xc0))),
];

/// Highlight swatches (`None` = no highlight).
pub const HIGHLIGHTS: [(&str, Option<Color32>); 10] = [
    ("None", None),
    ("Gray", Some(Color32::from_rgb(0x3a, 0x3a, 0x40))),
    ("Red", Some(Color32::from_rgb(0x5c, 0x2b, 0x2d))),
    ("Orange", Some(Color32::from_rgb(0x5e, 0x41, 0x22))),
    ("Yellow", Some(Color32::from_rgb(0x57, 0x4c, 0x1e))),
    ("Green", Some(Color32::from_rgb(0x2d, 0x4b, 0x2a))),
    ("Teal", Some(Color32::from_rgb(0x1e, 0x47, 0x44))),
    ("Blue", Some(Color32::from_rgb(0x22, 0x37, 0x5a))),
    ("Purple", Some(Color32::from_rgb(0x3d, 0x2f, 0x5c))),
    ("Pink", Some(Color32::from_rgb(0x58, 0x2a, 0x47))),
];

/// The color a highlight is *shown as* in pickers and icons: the matching
/// (brighter) text color, so both palettes read the same. Highlights not
/// from the palette are shown as-is.
pub fn highlight_display(bg: Color32) -> Color32 {
    HIGHLIGHTS
        .iter()
        .position(|(_, c)| *c == Some(bg))
        .and_then(|i| TEXT_COLORS[i].1)
        .unwrap_or(bg)
}

pub fn family(bold: bool, italic: bool) -> FontFamily {
    match (bold, italic) {
        (false, false) => FontFamily::Proportional,
        (true, false) => FontFamily::Name("bold".into()),
        (false, true) => FontFamily::Name("italic".into()),
        (true, true) => FontFamily::Name("bold-italic".into()),
    }
}

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    install_style(ctx);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 5] = [
        ("noto-regular", include_bytes!("../assets/fonts/NotoSans-Regular.ttf")),
        ("noto-medium", include_bytes!("../assets/fonts/NotoSans-Medium.ttf")),
        ("noto-bold", include_bytes!("../assets/fonts/NotoSans-Bold.ttf")),
        ("noto-italic", include_bytes!("../assets/fonts/NotoSans-Italic.ttf")),
        ("noto-bold-italic", include_bytes!("../assets/fonts/NotoSans-BoldItalic.ttf")),
    ];
    for (name, bytes) in faces {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    // Keep egui's bundled fonts as fallbacks (emoji, symbols).
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let with_fallbacks = |primary: &str| {
        let mut v = vec![primary.to_owned()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, with_fallbacks("noto-regular"));
    for (fam, face) in [
        ("medium", "noto-medium"),
        ("bold", "noto-bold"),
        ("italic", "noto-italic"),
        ("bold-italic", "noto-bold-italic"),
    ] {
        fonts.families.insert(FontFamily::Name(fam.into()), with_fallbacks(face));
    }
    ctx.set_fonts(fonts);
}

fn install_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Heading, FontId::new(20.0, medium())),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();

        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 6.0);
        s.button_padding = egui::vec2(10.0, 5.0);
        s.menu_margin = Margin::same(6);
        s.window_margin = Margin::same(18);
        s.interact_size.y = 26.0;
        s.scroll = egui::style::ScrollStyle::floating();
        s.scroll.bar_width = 6.0;

        style.interaction.selectable_labels = false;

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = BG;
        v.faint_bg_color = SURFACE;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(10);
        v.window_shadow = egui::Shadow {
            offset: [0, 8],
            blur: 28,
            spread: 0,
            color: Color32::from_black_alpha(110),
        };
        v.popup_shadow = egui::Shadow {
            offset: [0, 6],
            blur: 20,
            spread: 0,
            color: Color32::from_black_alpha(100),
        };
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xd4, 0xa3, 0x73, 70);
        // Keep selected text its own color (egui recolors it with this).
        v.selection.stroke = Stroke::new(1.0_f32, TEXT);
        v.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT);
        v.hyperlink_color = ACCENT;

        let radius = CornerRadius::same(7);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE;
        w.noninteractive.weak_bg_fill = SURFACE;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.noninteractive.corner_radius = radius;

        w.inactive.bg_fill = SURFACE;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0_f32, MUTED);
        w.inactive.corner_radius = radius;

        w.hovered.bg_fill = HOVER;
        w.hovered.weak_bg_fill = HOVER;
        w.hovered.bg_stroke = Stroke::NONE;
        w.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.hovered.corner_radius = radius;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = HOVER;
        w.active.weak_bg_fill = HOVER;
        w.active.bg_stroke = Stroke::NONE;
        w.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.active.corner_radius = radius;
        w.active.expansion = 0.0;

        w.open = w.hovered;
    });
}
