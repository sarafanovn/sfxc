//! Design tokens, fonts and egui styles for the light and dark themes.
//!
//! Zinc neutrals plus one accent. The accent hue is a user setting; accent colors
//! are derived from it in OKLCH. Corner radii follow one rule: controls 6, cards 10, dialogs 14.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin, Shadow, Stroke, TextStyle, Theme,
    ThemePreference, Visuals,
};

use super::accent;

pub const R_CONTROL: u8 = 6;
pub const R_CARD: u8 = 10;
pub const R_DIALOG: u8 = 14;

/// Semantic colors. Code outside this module never uses raw hex values.
#[derive(Clone, Copy)]
pub struct Palette {
    /// Sidebars and title bar.
    pub chrome: Color32,
    /// Editor background behind the cards.
    pub canvas: Color32,
    /// Cards, dialogs, popups.
    pub surface: Color32,
    /// Buttons and inputs at rest.
    pub raised: Color32,
    pub hover: Color32,
    /// Sunken track behind segmented controls and the search field.
    pub well: Color32,
    /// Selected segment.
    pub knob: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Text and icons on top of `accent`.
    pub on_accent: Color32,
    /// Tinted background for selected rows and accent badges.
    pub accent_soft: Color32,
    /// Accent-colored text on `accent_soft` or neutral surfaces.
    pub accent_text: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub warn: Color32,
    pub warn_soft: Color32,
    pub shadow: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const DARK: Palette = Palette {
    chrome: rgb(0x0E0E10),
    canvas: rgb(0x141416),
    surface: rgb(0x1A1A1D),
    raised: rgb(0x232327),
    hover: rgb(0x2C2C31),
    well: rgb(0x111113),
    knob: rgb(0x2E2E34),
    border: rgb(0x26262B),
    border_strong: rgb(0x3A3A41),
    text: rgb(0xEDEDF0),
    muted: rgb(0xA1A1AA),
    faint: rgb(0x8A8A94),
    accent: rgb(0x34D399),
    accent_hover: rgb(0x5EDDB0),
    on_accent: rgb(0x04291C),
    accent_soft: rgb(0x1E3831),
    accent_text: rgb(0x6EE7B7),
    danger: rgb(0xF87171),
    danger_soft: rgb(0x3A1D1F),
    warn: rgb(0xFBBF24),
    warn_soft: rgb(0x3A2E14),
    shadow: Color32::from_black_alpha(110),
};

pub const LIGHT: Palette = Palette {
    chrome: rgb(0xF4F4F5),
    canvas: rgb(0xFAFAFA),
    surface: rgb(0xFFFFFF),
    raised: rgb(0xF3F3F5),
    hover: rgb(0xE8E8EB),
    well: rgb(0xEBEBEE),
    knob: rgb(0xFFFFFF),
    border: rgb(0xE4E4E7),
    border_strong: rgb(0xD4D4D8),
    text: rgb(0x18181B),
    muted: rgb(0x52525B),
    faint: rgb(0x71717A),
    accent: rgb(0x047857),
    accent_hover: rgb(0x065F46),
    on_accent: rgb(0xF7FFFB),
    accent_soft: rgb(0xE3F4EC),
    accent_text: rgb(0x065F46),
    danger: rgb(0xDC2626),
    danger_soft: rgb(0xFDECEC),
    warn: rgb(0xB45309),
    warn_soft: rgb(0xFDF3E1),
    shadow: Color32::from_rgba_unmultiplied_const(24, 24, 27, 28),
};

/// OKLCH lightness and chroma of each accent role; the hue comes from the user.
struct AccentLevels {
    accent: (f32, f32),
    hover: (f32, f32),
    on_accent: (f32, f32),
    soft: (f32, f32),
    text: (f32, f32),
}

const LIGHT_ACCENT: AccentLevels =
    AccentLevels { accent: (0.50, 0.13), hover: (0.45, 0.13), on_accent: (0.99, 0.01), soft: (0.92, 0.03), text: (0.42, 0.11) };
const DARK_ACCENT: AccentLevels =
    AccentLevels { accent: (0.80, 0.13), hover: (0.86, 0.12), on_accent: (0.24, 0.05), soft: (0.33, 0.04), text: (0.84, 0.10) };

pub fn with_accent(base: Palette, hue: f32, dark: bool) -> Palette {
    let a = if dark { &DARK_ACCENT } else { &LIGHT_ACCENT };
    let c = |(l, ch): (f32, f32)| accent::oklch(l, ch, hue);
    Palette {
        accent: c(a.accent),
        accent_hover: c(a.hover),
        on_accent: c(a.on_accent),
        accent_soft: c(a.soft),
        accent_text: c(a.text),
        ..base
    }
}

fn palette_id(dark: bool) -> Id {
    Id::new(("sfxc_palette", dark))
}

/// Rebuilds both palettes for `hue` and restyles egui with them.
pub fn set_accent(ctx: &egui::Context, hue: f32) {
    let light = with_accent(LIGHT, hue, false);
    let dark = with_accent(DARK, hue, true);
    ctx.data_mut(|d| {
        d.insert_temp(palette_id(false), light);
        d.insert_temp(palette_id(true), dark);
    });
    ctx.set_style_of(Theme::Light, style(&light, false));
    ctx.set_style_of(Theme::Dark, style(&dark, true));
}

fn palette_for(ctx: &egui::Context, dark: bool) -> Palette {
    ctx.data(|d| d.get_temp::<Palette>(palette_id(dark))).unwrap_or(if dark { DARK } else { LIGHT })
}

pub fn palette(ui: &egui::Ui) -> Palette {
    palette_for(ui.ctx(), ui.visuals().dark_mode)
}

pub fn palette_of(ctx: &egui::Context) -> Palette {
    palette_for(ctx, ctx.theme() == Theme::Dark)
}

/// User-facing theme setting, stored in the library as a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    Auto,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Auto, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn key(self) -> &'static str {
        match self {
            ThemeChoice::Auto => "auto",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::Auto => "Auto",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
        }
    }

    pub fn icon(self) -> &'static str {
        use egui_phosphor::regular as i;
        match self {
            ThemeChoice::Auto => i::MONITOR,
            ThemeChoice::Light => i::SUN,
            ThemeChoice::Dark => i::MOON,
        }
    }

    pub fn preference(self) -> ThemePreference {
        match self {
            ThemeChoice::Auto => ThemePreference::System,
            ThemeChoice::Light => ThemePreference::Light,
            ThemeChoice::Dark => ThemePreference::Dark,
        }
    }
}

/// Interface size presets, applied as egui zoom.
pub const UI_SCALES: [(f32, &str); 3] = [(0.9, "Compact"), (1.0, "Default"), (1.15, "Large")];

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn icon_fill() -> FontFamily {
    FontFamily::Name("phosphor-fill".into())
}

pub fn title_style() -> TextStyle {
    TextStyle::Name("title".into())
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    set_accent(ctx, accent::DEFAULT_HUE);
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let data = |bytes: &'static [u8]| std::sync::Arc::new(FontData::from_static(bytes));
    fonts.font_data.insert("Geist".into(), data(include_bytes!("../../assets/fonts/Geist-Regular.ttf")));
    fonts.font_data.insert("Geist-SemiBold".into(), data(include_bytes!("../../assets/fonts/Geist-SemiBold.ttf")));
    fonts.font_data.insert("GeistMono".into(), data(include_bytes!("../../assets/fonts/GeistMono-Regular.ttf")));
    // egui's bundled fonts stay as fallbacks for symbols Geist lacks (⌘, arrows).
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Geist".into());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "GeistMono".into());
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    egui_phosphor::add_font_bytes_as_family(&mut fonts, "phosphor-fill", egui_phosphor::Variant::Fill.font_bytes());

    let mut bold = fonts.families[&FontFamily::Proportional].clone();
    bold[0] = "Geist-SemiBold".into();
    fonts.families.insert(semibold(), bold);
    fonts
}

fn style(p: &Palette, dark: bool) -> egui::Style {
    let mut s = egui::Style { visuals: visuals(p, dark), ..Default::default() };
    s.text_styles = [
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        (TextStyle::Heading, FontId::new(15.0, semibold())),
        (title_style(), FontId::new(22.0, semibold())),
    ]
    .into();
    let sp = &mut s.spacing;
    sp.item_spacing = egui::vec2(8.0, 6.0);
    sp.button_padding = egui::vec2(10.0, 5.0);
    sp.interact_size.y = 26.0;
    sp.window_margin = Margin::same(20);
    sp.menu_margin = Margin::same(6);
    sp.slider_rail_height = 4.0;
    sp.combo_height = 260.0;
    sp.icon_width = 16.0;
    sp.icon_width_inner = 10.0;
    sp.scroll = egui::style::ScrollStyle::floating();
    s.animation_time = 0.12;
    s
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    let control = CornerRadius::same(R_CONTROL);
    v.panel_fill = p.chrome;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(R_DIALOG);
    v.window_shadow = Shadow { offset: [0, 12], blur: 36, spread: 0, color: p.shadow };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: p.shadow };
    v.menu_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = p.raised;
    v.text_edit_bg_color = Some(p.raised);
    v.faint_bg_color = p.raised;
    v.code_bg_color = p.raised;
    v.hyperlink_color = p.accent_text;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger;
    v.weak_text_color = Some(p.faint);
    // Soft tint keeps selected text readable; focus rings use the stroke.
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent_text);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.bg_fill = p.hover;
    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.bg_fill = p.border_strong;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.active.bg_fill = p.border_strong;
    w.active.weak_bg_fill = p.border_strong;
    w.active.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.open = w.hovered;
    for wv in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        wv.corner_radius = control;
        wv.expansion = 0.0;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::accent::contrast;

    #[test]
    fn accent_contrast_holds_for_every_hue() {
        for (base, dark) in [(LIGHT, false), (DARK, true)] {
            for step in 0..72 {
                let p = with_accent(base, step as f32 * 5.0, dark);
                let h = step * 5;
                assert!(contrast(p.accent_text, p.surface) >= 4.5, "accent_text hue {h} dark {dark}");
                assert!(contrast(p.on_accent, p.accent) >= 4.5, "on_accent hue {h} dark {dark}");
            }
        }
    }

    #[test]
    fn neutral_text_contrast() {
        for p in [LIGHT, DARK] {
            for c in [p.text, p.muted, p.faint] {
                assert!(contrast(c, p.surface) >= 4.5);
                assert!(contrast(c, p.canvas) >= 4.5);
            }
        }
    }
}
