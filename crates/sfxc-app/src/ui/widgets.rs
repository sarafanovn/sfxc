//! Shared components drawn with the theme tokens.

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontId, Frame, Layout, Margin, Pos2, Rect, Response, RichText,
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
/// Vertical space after a card, on top of the item spacing.
const CARD_GAP: f32 = 14.0;

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

/// Digits after the decimal point, by the size of the range, so a column of values lines up.
fn decimals_for(lo: f64, hi: f64, integral: bool) -> usize {
    let span = (hi - lo).abs();
    if integral || span >= 1000.0 {
        0
    } else if span >= 10.0 {
        1
    } else if span >= 1.0 {
        2
    } else {
        3
    }
}

/// Label, track and value sit in fixed columns of one allocated row, so nothing can widen the row.
#[allow(clippy::too_many_arguments)]
fn param_row<N: Numeric>(ui: &mut Ui, label: &str, v: &mut N, lo: N, hi: N, default: N, suffix: &str, log: bool) -> Response {
    let p = palette(ui);
    let total = ui.available_width();
    let h = ui.spacing().interact_size.y.max(26.0);
    let (row, _) = ui.allocate_exact_size(Vec2::new(total, h), Sense::hover());
    let label_rect = Rect::from_min_size(row.min, Vec2::new(LABEL_W, h));
    let value_rect = Rect::from_min_max(Pos2::new((row.right() - VALUE_W).max(label_rect.right()), row.top()), row.right_bottom());
    let track_rect = Rect::from_min_max(
        Pos2::new(label_rect.right() + ROW_GAP, row.top()),
        Pos2::new((value_rect.left() - ROW_GAP).max(label_rect.right() + ROW_GAP + 60.0), row.bottom()),
    );

    let galley = ui.painter().layout(label.to_string(), FontId::proportional(12.5), p.muted, LABEL_W);
    ui.painter().galley(Pos2::new(label_rect.left(), label_rect.center().y - galley.size().y / 2.0), galley, p.muted);

    let mut track_ui = ui.new_child(egui::UiBuilder::new().max_rect(track_rect).layout(Layout::left_to_right(Align::Center)));
    let resp = controls::track(&mut track_ui, v, lo, hi, default, log, track_rect.width());

    let mut drag = egui::DragValue::new(v)
        .range(lo..=hi)
        .suffix(suffix)
        .fixed_decimals(decimals_for(lo.to_f64(), hi.to_f64(), N::INTEGRAL));
    if !log {
        drag = drag.speed(((hi.to_f64() - lo.to_f64()) / 400.0).max(if N::INTEGRAL { 0.05 } else { 0.0 }));
    }
    let mut value_ui = ui.new_child(egui::UiBuilder::new().max_rect(value_rect).layout(Layout::right_to_left(Align::Center)));
    let dv = value_ui
        .scope(|ui| {
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.weak_bg_fill = Color32::TRANSPARENT;
            w.hovered.weak_bg_fill = p.well;
            w.active.weak_bg_fill = p.well;
            ui.add(drag)
        })
        .inner;
    resp.union(dv)
}

/// Dropdown in the same material as the other fields: recessed well, phosphor caret.
pub fn select<R>(ui: &mut Ui, id_salt: &str, selected: impl Into<egui::WidgetText>, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<Option<R>> {
    let p = palette(ui);
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = p.well;
        w.hovered.weak_bg_fill = p.hover;
        w.active.weak_bg_fill = p.hover;
        w.open.weak_bg_fill = p.hover;
        ui.spacing_mut().button_padding = Vec2::new(12.0, 7.0);
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected)
            .width(ui.available_width())
            .icon(move |ui, rect, _visuals, open| {
                let glyph = if open { icon::CARET_UP } else { icon::CARET_DOWN };
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(14.0), p.muted);
            })
            .show_ui(ui, add)
    })
    .inner
}

/// Fixed-width dropdown over a small set of values. Returns true when the choice changed.
pub fn dropdown<T: PartialEq + Copy>(ui: &mut Ui, id_salt: &str, width: f32, value: &mut T, options: &[(T, &str)]) -> bool {
    let before = *value;
    let current = options.iter().find(|(v, _)| *v == *value).map_or("—", |(_, l)| *l);
    ui.allocate_ui(Vec2::new(width, 30.0), |ui| {
        select(ui, id_salt, current, |ui| {
            for (v, l) in options {
                ui.selectable_value(value, *v, *l);
            }
        });
    });
    *value != before
}

/// Short muted label followed by its control, kept together when a row wraps.
pub fn field<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(label).size(12.5).color(p.muted));
        add(ui)
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

/// Button with optional leading icon. Flat label that lifts on hover and sinks on press; `kind` only picks the label color.
pub fn button(ui: &mut Ui, kind: Kind, icon: Option<&str>, text: &str) -> Response {
    button_min_width(ui, kind, icon, text, 0.0)
}

/// Natural width of a [`button`] with this label, for sizing a group of buttons alike.
pub fn button_width(ui: &Ui, icon: Option<&str>, text: &str) -> f32 {
    let label = match icon {
        Some(i) if text.is_empty() => i.to_string(),
        Some(i) => format!("{i}  {text}"),
        None => text.to_string(),
    };
    let galley = ui.painter().layout_no_wrap(label, FontId::proportional(13.0), Color32::PLACEHOLDER);
    let pad = if text.is_empty() { 8.0 } else { 18.0 };
    (galley.size().x + pad * 2.0).max(36.0)
}

/// Like [`button`], at least `min_width` wide with the label centered.
pub fn button_min_width(ui: &mut Ui, kind: Kind, icon: Option<&str>, text: &str, min_width: f32) -> Response {
    let p = palette(ui);
    let fg = |hovered: bool| match kind {
        Kind::Primary => p.accent_text,
        Kind::Danger => p.danger,
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
    let size = (galley.size() + pad * 2.0).max(Vec2::new(36.0f32.max(min_width), 36.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let m = Motion::of(ui, &resp);
        let painter = ui.painter();
        // Flat text at rest; the button lifts off the surface on hover and sinks on press.
        if m.hover > 0.0 || m.press > 0.0 {
            material::raised(painter, rect, R_CONTROL as f32, &p, &m);
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

/// Square power button: sunken and dim when off, raised with an accent icon when on.
pub fn power_toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let p = palette(ui);
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let m = Motion::of(ui, &resp);
    let painter = ui.painter();
    if *on {
        material::raised(painter, rect, R_CONTROL as f32, &p, &m);
    } else {
        material::recessed(painter, rect, R_CONTROL as f32, &p, 0.6);
    }
    let color = p.faint.lerp_to_gamma(p.accent_text, t);
    painter.text(rect.center(), Align2::CENTER_CENTER, egui_phosphor::regular::POWER, FontId::proportional(14.0), color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn card_frame(_ui: &Ui) -> Frame {
    Frame::new().corner_radius(R_CARD).inner_margin(Margin::same(14))
}

/// Frame content on a raised card. Shadows are painted under the content after layout.
pub fn material_card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    material_card_response(ui, add).inner
}

/// Like [`material_card`], but also returns the card's response (its rect, for drop targets).
pub fn material_card_response<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let p = palette(ui);
    let under = ui.painter().add(Shape::Noop);
    let inner = card_frame(ui).show(ui, add);
    let rect = inner.response.rect;
    let m = Motion::of(ui, &ui.interact(rect, inner.response.id.with("card"), Sense::hover()));
    let painter = ui.painter().clone();
    let mut shapes = Vec::new();
    let e = 0.6 + 0.2 * m.hover;
    let d = (5.0 * e) as i8;
    shapes.push(Shape::from(Shadow { offset: [-d, -d], blur: 16, spread: 0, color: p.shadow_light }.as_shape(rect, R_CARD)));
    shapes.push(Shape::from(Shadow { offset: [d, d], blur: 16, spread: 0, color: p.shadow_dark }.as_shape(rect, R_CARD)));
    shapes.push(Shape::rect_filled(rect, R_CARD, p.surface));
    painter.set(under, Shape::Vec(shapes));
    inner
}

/// Lets tests find the drag handles that were drawn in the last frame.
#[cfg(test)]
pub mod test_support {
    use std::cell::RefCell;

    use eframe::egui::Rect;

    thread_local! {
        pub static GRIPS: RefCell<Vec<Rect>> = const { RefCell::new(Vec::new()) };
    }

    pub fn take_grips() -> Vec<Rect> {
        GRIPS.with(|g| std::mem::take(&mut *g.borrow_mut()))
    }
}

/// Where a card sits in a reorderable group: `group` names the list, `index` is its position.
#[derive(Clone, Copy)]
pub struct Grip<'a> {
    pub group: &'a str,
    pub index: usize,
}

#[derive(Clone, Copy)]
struct DragPayload {
    group: egui::Id,
    index: usize,
}

/// Collapsible card; open state is remembered per `key`. `header` adds widgets at the right of the title.
/// With a `grip` the card has a drag handle and can be dropped onto other cards of the same group;
/// the result is `(from, to)` when something was dropped on this card.
pub fn section(
    ui: &mut Ui,
    key: &str,
    title: &str,
    grip: Option<Grip>,
    header: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) -> Option<(usize, usize)> {
    let p = palette(ui);
    let id = ui.make_persistent_id(("section", key));
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    let card = material_card_response(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if let Some(g) = grip {
                grip_handle(ui, g, key);
            }
            state.show_toggle_button(ui, |ui, openness, resp| {
                let p = palette(ui);
                let color = if resp.hovered() { p.text } else { p.muted };
                let glyph = if openness > 0.5 { icon::CARET_DOWN } else { icon::CARET_RIGHT };
                ui.painter().text(resp.rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(14.0), color);
            });
            let label = egui::Label::new(RichText::new(title).font(FontId::new(13.5, theme::semibold())).color(p.text));
            if ui.add(label.sense(Sense::click())).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                state.toggle(ui);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), header);
        });
        state.show_body_unindented(ui, |ui| {
            ui.add_space(4.0);
            body(ui);
        });
    });
    ui.add_space(CARD_GAP);

    let g = grip?;
    let rect = card.response.rect;
    let half_gap = (CARD_GAP + ui.spacing().item_spacing.y) / 2.0;
    drop_zone(ui, rect, g, Axis::Vertical, half_gap)
}

/// Drag handle of a reorderable card. Dragging it carries the card's place in its group.
pub fn grip_handle(ui: &mut Ui, grip: Grip, key: &str) -> Response {
    let p = palette(ui);
    let payload = DragPayload { group: egui::Id::new(grip.group), index: grip.index };
    let handle = ui
        .dnd_drag_source(egui::Id::new(("grip", grip.group, key)), payload, |ui| {
            ui.label(RichText::new(icon::DOTS_SIX_VERTICAL).color(p.faint).size(16.0));
        })
        .response
        .on_hover_cursor(CursorIcon::Grab);
    #[cfg(test)]
    test_support::GRIPS.with(|g| g.borrow_mut().push(handle.rect));
    handle
}

#[derive(Clone, Copy)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// Drop target for the card in `rect`, while a card of the same group is dragged. The zone includes
/// `half_gap` on each side along `axis`, so a drop between two cards still lands. Returns
/// `(from, to)` on release; otherwise draws a line on the side where the dragged card will land.
pub fn drop_zone(ui: &mut Ui, rect: Rect, grip: Grip, axis: Axis, half_gap: f32) -> Option<(usize, usize)> {
    let p = palette(ui);
    let group = egui::Id::new(grip.group);
    let payload = egui::DragAndDrop::payload::<DragPayload>(ui.ctx()).filter(|pl| pl.group == group && pl.index != grip.index)?;
    let zone = match axis {
        Axis::Vertical => rect.expand2(Vec2::new(0.0, half_gap)),
        Axis::Horizontal => rect.expand2(Vec2::new(half_gap, 0.0)),
    };
    if !ui.ctx().pointer_interact_pos().is_some_and(|pt| zone.contains(pt)) {
        return None;
    }
    if ui.input(|i| i.pointer.any_released()) {
        return Some((payload.index, grip.index));
    }
    // Dropping moves the dragged card to this slot, so the line goes on the side it will land.
    let after = payload.index < grip.index;
    let stroke = Stroke::new(2.0, p.accent);
    match axis {
        Axis::Vertical => {
            let y = if after { rect.bottom() + half_gap } else { rect.top() - half_gap };
            ui.painter().hline(rect.x_range(), y, stroke);
        }
        Axis::Horizontal => {
            let x = if after { rect.right() + half_gap } else { rect.left() - half_gap };
            ui.painter().vline(x, rect.y_range(), stroke);
        }
    }
    None
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
    let glyph = if playing { egui_phosphor::fill::STOP } else { egui_phosphor::fill::PLAY };
    let offset = if playing { Vec2::ZERO } else { Vec2::new(1.5, 0.0) };
    ui.painter().text(c + offset, Align2::CENTER_CENTER, glyph, FontId::new(20.0, theme::icon_fill()), p.on_accent);
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(if playing { "Stop (Space)" } else { "Play (Space)" })
}

/// Filled waveform with a playhead. `progress` is the played fraction while audio runs.
/// `axis_secs` fixes the length of the time axis (a fixed export length): the sound is cut or padded with
/// silence to it, and the time label shows that length. `None` shows the sound's own length.
pub fn waveform(ui: &mut Ui, rendered: Option<(&[f32], u32)>, generation: u64, progress: Option<f32>, height: f32, axis_secs: Option<f32>) {
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
    let own_secs = samples.len() as f32 / sample_rate as f32;
    let axis_secs = axis_secs.filter(|s| *s > 0.0).unwrap_or(own_secs);
    let total = (axis_secs * sample_rate as f32).max(1.0);
    let per = (total / cols as f32).max(1.0);
    let half = inner.height() / 2.0;
    let head_x = progress.map(|t| inner.left() + (t * own_secs / axis_secs).min(1.0) * inner.width());
    let played = p.accent;
    let unplayed = if progress.is_some() { p.accent.gamma_multiply(0.45) } else { p.accent.gamma_multiply(0.85) };
    let peaks = column_peaks(ui, samples, generation, cols, per);
    let mut shapes = Vec::with_capacity(peaks.len());
    for (c, &(lo, hi)) in peaks.iter().enumerate() {
        let x = inner.left() + c as f32 + 0.5;
        let color = if head_x.is_some_and(|h| x <= h) { played } else { unplayed };
        let (top, bottom) = (mid - hi.clamp(-1.0, 1.0) * half, mid - lo.clamp(-1.0, 1.0) * half);
        shapes.push(Shape::line_segment([Pos2::new(x, top.min(mid - 0.5)), Pos2::new(x, bottom.max(mid + 0.5))], Stroke::new(1.0, color)));
    }
    painter.extend(shapes);
    if let Some(x) = head_x {
        painter.vline(x, rect.y_range().shrink(4.0), Stroke::new(1.5, p.text));
    }
    let label = format!("{axis_secs:.2} s");
    let galley = painter.layout_no_wrap(label, FontId::monospace(11.0), p.muted);
    let at = rect.right_bottom() - galley.size() - Vec2::new(8.0, 6.0);
    painter.rect_filled(Rect::from_min_size(at, galley.size()).expand2(Vec2::new(5.0, 2.0)), 4.0, p.well);
    painter.galley(at, galley, p.muted);
}

type PeakKey = (u64, usize, usize, u32);

/// Min/max of each column, cached until the render or the layout changes.
fn column_peaks(ui: &Ui, samples: &[f32], generation: u64, cols: usize, per: f32) -> std::sync::Arc<[(f32, f32)]> {
    let key: PeakKey = (generation, samples.len(), cols, per.to_bits());
    let id = ui.id().with("waveform_peaks");
    if let Some((k, peaks)) = ui.data(|d| d.get_temp::<(PeakKey, std::sync::Arc<[(f32, f32)]>)>(id))
        && k == key
    {
        return peaks;
    }
    let peaks: std::sync::Arc<[(f32, f32)]> = (0..cols)
        .map_while(|c| {
            let a = (c as f32 * per) as usize;
            let b = (((c + 1) as f32 * per) as usize).min(samples.len());
            (a < b).then(|| samples[a..b].iter().fold((0.0f32, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v))))
        })
        .collect();
    ui.data_mut(|d| d.insert_temp(id, (key, peaks.clone())));
    peaks
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, vec2, RawInput};

    /// A long value such as "-3.545 oct/s" must not widen the row, and with it the whole card.
    #[test]
    fn param_rows_stay_inside_their_column() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut t = 0.0;
        for width in [360.0f32, 520.0, 700.0] {
            t += 0.1;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width + 80.0, 500.0))),
                time: Some(t),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                ui.allocate_ui(vec2(width, 400.0), |ui| {
                    ui.set_width(width);
                    let (mut slide, mut freq, mut accel) = (-3.545f32, 138.6f32, -0.001f32);
                    param(ui, "Slide", &mut slide, (-8.0, 8.0), 0.0, " oct/s", false);
                    param(ui, "Frequency", &mut freq, (20.0, 20_000.0), 440.0, " Hz", true);
                    param(ui, "Slide accel", &mut accel, (-8.0, 8.0), 0.0, " oct/s²", false);
                    assert!(ui.min_rect().width() <= width + 0.5, "rows grew to {} in a {width} column", ui.min_rect().width());
                });
            });
            out.textures_delta.clear();
        }
    }
}
