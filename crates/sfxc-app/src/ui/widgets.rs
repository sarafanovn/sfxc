//! Shared components drawn with the theme tokens.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Layout, Margin, Pos2, Rect, Response, RichText,
    Sense, Shadow, Shape, Stroke, TextStyle, Ui, Vec2, WidgetText,
};
use egui_phosphor::regular as icon;
use eframe::egui::emath::Numeric;
use sfxc_core::patch::Range;

use super::controls;
use super::material::{self, Motion};
use super::theme::{self, palette, Palette, R_CARD, R_CONTROL};

const LABEL_W: f32 = 104.0;
const VALUE_W: f32 = 92.0;
const ROW_GAP: f32 = 10.0;

/// Muted, left-aligned label in the fixed first column of parameter rows.
pub fn row_label(ui: &mut Ui, label: &str) {
    let p = palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(LABEL_W, ui.spacing().interact_size.y), Sense::hover());
    let galley = ui.painter().layout(label.to_string(), FontId::proportional(12.5), p.muted, LABEL_W);
    ui.painter().galley(Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0), galley, p.muted);
}

/// Parameter row: label, slider, editable value. Double-click the slider to reset.
pub fn param(ui: &mut Ui, label: &str, v: &mut f32, range: Range, default: f32, suffix: &str, log: bool) -> Response {
    param_row(ui, label, v, range.0, range.1, default, suffix, log)
}

/// Rotary variant of [`param`] for compact layouts (effect cards).
pub fn knob(ui: &mut Ui, label: &str, v: &mut f32, range: Range, default: f32, suffix: &str, log: bool) -> Response {
    controls::knob(ui, label, v, range.0, range.1, default, suffix, log)
}

pub fn knob_num<N: Numeric>(ui: &mut Ui, label: &str, v: &mut N, lo: N, hi: N, default: N, suffix: &str) -> Response {
    controls::knob(ui, label, v, lo, hi, default, suffix, false)
}

#[allow(clippy::too_many_arguments)]
fn param_row<N: Numeric>(ui: &mut Ui, label: &str, v: &mut N, lo: N, hi: N, default: N, suffix: &str, log: bool) -> Response {
    let total = ui.available_width();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_GAP;
        row_label(ui, label);
        let width = (total - LABEL_W - VALUE_W - ROW_GAP * 2.0).max(60.0);
        let resp = controls::track(ui, v, lo, hi, default, log, width);
        let mut drag = egui::DragValue::new(v).range(lo..=hi).suffix(suffix).max_decimals(3);
        if !log {
            drag = drag.speed(((hi.to_f64() - lo.to_f64()) / 400.0).max(if N::INTEGRAL { 0.05 } else { 0.0 }));
        }
        let dv = ui.allocate_ui_with_layout(
            Vec2::new(VALUE_W, ui.spacing().interact_size.y),
            Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.scope(|ui| {
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                    ui.add(drag)
                })
                .inner
            },
        );
        let dv = dv.inner;
        resp.union(dv)
    })
    .inner
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

/// Button with optional leading icon. Bulges on hover and sinks on press.
pub fn button(ui: &mut Ui, kind: Kind, icon: Option<&str>, text: &str) -> Response {
    let p = palette(ui);
    let fg = |hovered: bool| match kind {
        Kind::Primary => p.on_accent,
        Kind::Danger => Color32::WHITE,
        Kind::Secondary => p.text,
        Kind::Ghost if hovered => p.text,
        Kind::Ghost => p.muted,
    };
    let font = FontId::proportional(13.0);
    let label = match icon {
        Some(i) if text.is_empty() => i.to_string(),
        Some(i) => format!("{i}  {text}"),
        None => text.to_string(),
    };
    let galley = ui.painter().layout_no_wrap(label, font, Color32::PLACEHOLDER);
    let pad = if text.is_empty() { Vec2::splat(8.0) } else { Vec2::new(18.0, 8.0) };
    let size = (galley.size() + pad * 2.0).max(Vec2::new(36.0, 36.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let m = Motion::of(ui, &resp);
        let painter = ui.painter();
        match kind {
            Kind::Secondary => material::raised(painter, rect, R_CONTROL as f32, &p, &m),
            Kind::Ghost => {
                if m.hover > 0.0 {
                    material::raised(painter, rect, R_CONTROL as f32, &p, &Motion { near: 0.0, ..m });
                }
            }
            Kind::Primary | Kind::Danger => {
                let base = if kind == Kind::Primary { p.accent } else { p.danger };
                let fill = base.lerp_to_gamma(if kind == Kind::Primary { p.accent_hover } else { Color32::BLACK }, 0.15 * m.hover);
                material::raised(painter, rect, R_CONTROL as f32, &Palette { raised: fill, ..p }, &m);
            }
        }
        let hovered = m.hover > 0.5;
        let offset = Vec2::splat(m.press.clamp(0.0, 1.0));
        painter.galley(rect.center() - galley.size() / 2.0 + offset, galley, fg(hovered));
    }
    if ui.is_enabled() { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
}

/// Square icon-only ghost button with a tooltip.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    button(ui, Kind::Ghost, Some(icon), "").on_hover_text(tooltip)
}

/// Segmented control. Returns true when the selection changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = palette(ui);
    let font = FontId::proportional(12.5);
    let galleys: Vec<_> =
        options.iter().map(|(_, l)| ui.painter().layout_no_wrap(l.to_string(), font.clone(), Color32::PLACEHOLDER)).collect();
    let inset = 2.0;
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 20.0).collect();
    let size = Vec2::new(widths.iter().sum::<f32>() + inset * 2.0, 28.0);
    let (rect, base) = ui.allocate_exact_size(size, Sense::hover());
    material::recessed(ui.painter(), rect, R_CONTROL as f32, &p, 0.6);
    let mut x = rect.left() + inset;
    let mut changed = false;
    for (i, (((opt, _), galley), w)) in options.iter().zip(galleys).zip(widths).enumerate() {
        let seg = Rect::from_min_size(Pos2::new(x, rect.top() + inset), Vec2::new(w, rect.height() - inset * 2.0));
        x += w;
        let resp = ui.interact(seg, base.id.with(i), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let selected = *value == *opt;
        if resp.clicked() && !selected {
            *value = *opt;
            changed = true;
        }
        if selected {
            let m = Motion::of(ui, &resp);
            material::raised(ui.painter(), seg, (R_CONTROL - 1) as f32, &Palette { raised: p.knob, ..p }, &m);
        } else if resp.hovered() {
            ui.painter().rect_filled(seg, R_CONTROL - 1, p.hover.gamma_multiply(0.6));
        }
        let color = if selected || resp.hovered() { p.text } else { p.muted };
        ui.painter().galley(seg.center() - galley.size() / 2.0, galley, color);
    }
    changed
}

/// iOS-style switch with an optional label to its right.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = palette(ui);
    let track = Vec2::new(30.0, 18.0);
    let galley = (!label.is_empty())
        .then(|| ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), Color32::PLACEHOLDER));
    let text_w = galley.as_ref().map_or(0.0, |g| g.size().x + 8.0);
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(track.x + text_w, 26.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let track_rect = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - track.y / 2.0), track);
    let m = Motion::of(ui, &resp);
    let painter = ui.painter();
    material::recessed(painter, track_rect, track.y / 2.0, &p, 0.7);
    painter.rect_filled(track_rect, track.y / 2.0, p.accent.gamma_multiply(t));
    let r = track.y / 2.0 - 2.0;
    let cx = egui::lerp(track_rect.left() + r + 2.0..=track_rect.right() - r - 2.0, t);
    let knob = Rect::from_center_size(Pos2::new(cx, track_rect.center().y), Vec2::splat(r * 2.0));
    material::raised(painter, knob, r, &Palette { raised: p.knob, ..p }, &m);
    if let Some(g) = galley {
        let pos = Pos2::new(track_rect.right() + 8.0, rect.center().y - g.size().y / 2.0);
        painter.galley(pos, g, p.text);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn card_frame(_ui: &Ui) -> Frame {
    Frame::new().corner_radius(R_CARD).inner_margin(Margin::same(14))
}

/// Frame content on a raised card. Shadows are painted under the content after layout.
pub fn material_card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = palette(ui);
    let under = ui.painter().add(Shape::Noop);
    let inner = card_frame(ui).show(ui, add);
    let rect = inner.response.rect;
    let m = Motion { pointer: None, ..Motion::of(ui, &ui.interact(rect, inner.response.id.with("card"), Sense::hover())) };
    let painter = ui.painter().clone();
    let mut shapes = Vec::new();
    let e = 0.6 + 0.2 * m.near;
    let d = (5.0 * e) as i8;
    shapes.push(Shape::from(Shadow { offset: [-d, -d], blur: 16, spread: 0, color: p.shadow_light }.as_shape(rect, R_CARD)));
    shapes.push(Shape::from(Shadow { offset: [d, d], blur: 16, spread: 0, color: p.shadow_dark }.as_shape(rect, R_CARD)));
    shapes.push(Shape::rect_filled(rect, R_CARD, p.surface));
    painter.set(under, Shape::Vec(shapes));
    inner.inner
}

/// Collapsible card; open state is remembered per `key`. `header` adds widgets at the right of the title.
pub fn section(ui: &mut Ui, key: &str, title: &str, header: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(("section", key));
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    material_card(ui, |ui| {
        ui.set_width(ui.available_width());
        state
            .show_header(ui, |ui| {
                ui.label(RichText::new(title).font(FontId::new(13.5, theme::semibold())).color(palette(ui).text));
                ui.with_layout(Layout::right_to_left(Align::Center), header);
            })
            .body(|ui| {
                ui.add_space(4.0);
                body(ui);
            });
    });
    ui.add_space(14.0);
}

pub fn panel_header(ui: &mut Ui, title: &str, trailing: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(36.0);
        ui.label(RichText::new(title).text_style(TextStyle::Heading).color(palette(ui).text));
        ui.with_layout(Layout::right_to_left(Align::Center), trailing);
    });
}

/// Small rounded label. `tone` picks the colors.
pub fn badge(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let p = palette(ui);
    let (fg, bg) = tone.colors(&p);
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(11.0), fg);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(12.0, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, R_CONTROL, bg);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    resp
}

#[derive(Clone, Copy)]
pub enum Tone {
    Neutral,
    Accent,
    Warn,
    Danger,
}

impl Tone {
    fn colors(self, p: &Palette) -> (Color32, Color32) {
        match self {
            Tone::Neutral => (p.muted, p.raised),
            Tone::Accent => (p.accent_text, p.accent_soft),
            Tone::Warn => (p.warn, p.warn_soft),
            Tone::Danger => (p.danger, p.danger_soft),
        }
    }
}

/// Inline notice with an icon. Returns true when its close button was clicked.
pub fn banner(ui: &mut Ui, tone: Tone, icon_glyph: &str, text: &str, closable: bool) -> bool {
    let p = palette(ui);
    let (fg, bg) = tone.colors(&p);
    let mut closed = false;
    Frame::new().fill(bg).corner_radius(R_CONTROL).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon_glyph).color(fg).size(15.0));
            ui.add(egui::Label::new(RichText::new(text).color(p.text)).wrap());
            if closable {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    closed = icon_button(ui, icon::X, "Dismiss").clicked();
                });
            }
        });
    });
    closed
}

/// Centered icon, title and hint for panels with nothing to show.
pub fn empty_state(ui: &mut Ui, glyph: &str, title: &str, body: &str, add: impl FnOnce(&mut Ui)) {
    let p = palette(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
        ui.painter().rect_filled(rect, R_CARD, p.raised);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(22.0), p.muted);
        ui.add_space(10.0);
        ui.label(RichText::new(title).font(FontId::new(14.0, theme::semibold())).color(p.text));
        ui.add(egui::Label::new(RichText::new(body).color(p.muted).size(12.5)).wrap());
        ui.add_space(8.0);
        add(ui);
    });
}

/// Search input with a magnifier, drawn as one control.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    let p = palette(ui);
    Frame::new()
        .fill(p.well)
        .stroke(Stroke::NONE)
        .corner_radius(R_CONTROL)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(p.faint));
                let r = ui.add(
                    egui::TextEdit::singleline(text)
                        .hint_text(RichText::new(hint).color(p.faint))
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY),
                );
                if !text.is_empty() && icon_button(ui, icon::X, "Clear").clicked() {
                    text.clear();
                    return r.clone().with_changed();
                }
                r
            })
            .inner
        })
        .inner
}

trait WithChanged {
    fn with_changed(self) -> Self;
}

impl WithChanged for Response {
    fn with_changed(mut self) -> Self {
        self.mark_changed();
        self
    }
}

/// Text in the theme's muted color at a small size.
pub fn hint(ui: &Ui, text: impl Into<String>) -> WidgetText {
    RichText::new(text).color(palette(ui).faint).size(11.5).into()
}

/// Large round play button.
pub fn play_button(ui: &mut Ui, playing: bool) -> Response {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::click());
    let m = Motion::of(ui, &resp);
    let c = rect.center() + Vec2::splat(m.press.clamp(0.0, 1.0));
    let fill = p.accent.lerp_to_gamma(p.accent_hover, m.hover);
    material::raised(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(44.0)), 22.0, &Palette { raised: fill, ..p }, &m);
    let glyph = if playing { egui_phosphor::fill::WAVEFORM } else { egui_phosphor::fill::PLAY };
    let offset = if playing { Vec2::ZERO } else { Vec2::new(1.5, 0.0) };
    ui.painter().text(c + offset, Align2::CENTER_CENTER, glyph, FontId::new(20.0, theme::icon_fill()), p.on_accent);
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Play (Space)")
}

/// Filled waveform with a playhead. `progress` is the played fraction while audio runs.
pub fn waveform(ui: &mut Ui, rendered: Option<(&[f32], u32)>, progress: Option<f32>, height: f32) {
    let p = palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    material::recessed(&painter, rect, R_CONTROL as f32, &p, 0.8);
    let mid = rect.center().y;
    painter.hline(rect.x_range().shrink(8.0), mid, Stroke::new(1.0, p.border));
    let Some((samples, sample_rate)) = rendered.filter(|(s, _)| !s.is_empty()) else {
        painter.text(rect.center(), Align2::CENTER_CENTER, "Silence", FontId::proportional(12.5), p.faint);
        return;
    };
    let inner = rect.shrink2(Vec2::new(8.0, 10.0));
    let cols = inner.width().max(1.0) as usize;
    let per = (samples.len() as f32 / cols as f32).max(1.0);
    let half = inner.height() / 2.0;
    let head_x = progress.map(|t| inner.left() + t * inner.width());
    let played = p.accent;
    let unplayed = if progress.is_some() { p.accent.gamma_multiply(0.45) } else { p.accent.gamma_multiply(0.85) };
    let mut shapes = Vec::with_capacity(cols);
    for c in 0..cols {
        let a = (c as f32 * per) as usize;
        let b = (((c + 1) as f32 * per) as usize).min(samples.len());
        if a >= b {
            break;
        }
        let (lo, hi) = samples[a..b].iter().fold((0.0f32, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        let x = inner.left() + c as f32 + 0.5;
        let color = if head_x.is_some_and(|h| x <= h) { played } else { unplayed };
        let (top, bottom) = (mid - hi.clamp(-1.0, 1.0) * half, mid - lo.clamp(-1.0, 1.0) * half);
        shapes.push(Shape::line_segment([Pos2::new(x, top.min(mid - 0.5)), Pos2::new(x, bottom.max(mid + 0.5))], Stroke::new(1.0, color)));
    }
    painter.extend(shapes);
    if let Some(x) = head_x {
        painter.vline(x, rect.y_range().shrink(4.0), Stroke::new(1.5, p.text));
    }
    let label = format!("{:.2} s", samples.len() as f32 / sample_rate as f32);
    let galley = painter.layout_no_wrap(label, FontId::monospace(11.0), p.muted);
    let at = rect.right_bottom() - galley.size() - Vec2::new(8.0, 6.0);
    painter.rect_filled(Rect::from_min_size(at, galley.size()).expand2(Vec2::new(5.0, 2.0)), 4.0, p.well);
    painter.galley(at, galley, p.muted);
}

/// Human-readable age for timestamps in lists.
pub fn age(secs: i64) -> String {
    match secs {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} h ago", s / 3600),
        s => format!("{} d ago", s / 86_400),
    }
}

/// Dialog body shared by the modals: title, content, right-aligned actions.
pub fn dialog<R>(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui) -> R) -> R {
    ui.set_width(440.0);
    ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
    ui.label(RichText::new(title).font(FontId::new(17.0, theme::semibold())).color(palette(ui).text));
    ui.add_space(10.0);
    body(ui);
    ui.add_space(20.0);
    ui.with_layout(Layout::right_to_left(Align::Center), actions).inner
}

/// Paints a rounded rect behind a list row and returns its interaction.
pub fn row_background(ui: &Ui, rect: Rect, resp: &Response, selected: bool) {
    let p = palette(ui);
    let radius = R_CONTROL as f32;
    if selected {
        material::recessed(ui.painter(), rect, radius, &p, 0.5);
        ui.painter().rect_filled(rect, CornerRadius::same(R_CONTROL), p.accent_soft.gamma_multiply(0.55));
    } else {
        let m = Motion::of(ui, resp);
        if m.hover > 0.01 {
            material::raised(ui.painter(), rect, radius, &p, &Motion { near: 0.0, pointer: None, ..m });
        }
    }
}

/// Sunken area that holds a list; content is inset so row shadows stay inside.
pub fn list_well(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let p = palette(ui);
    let rect = ui.available_rect_before_wrap();
    material::recessed(ui.painter(), rect, R_CARD as f32, &p, 0.55);
    let inner = rect.shrink2(Vec2::new(10.0, 10.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(*ui.layout()));
    add(&mut child);
    ui.advance_cursor_after_rect(rect);
}
