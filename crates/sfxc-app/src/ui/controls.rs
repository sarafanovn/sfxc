//! Custom value controls: horizontal track and rotary knob, sharing one input model.

use eframe::egui::{
    self, emath::Numeric, vec2, Align2, CursorIcon, FontId, Id, Key, LayerId, Order, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2,
};

use super::material::{self, Motion};
use super::theme::{palette, Palette};

/// Value → 0..1 position. Log scales need `lo > 0`.
pub fn to_norm(v: f64, lo: f64, hi: f64, log: bool) -> f32 {
    let t = if log { (v.max(lo).ln() - lo.ln()) / (hi.ln() - lo.ln()) } else { (v - lo) / (hi - lo) };
    t.clamp(0.0, 1.0) as f32
}

pub fn from_norm(t: f32, lo: f64, hi: f64, log: bool) -> f64 {
    let t = t.clamp(0.0, 1.0) as f64;
    if log { (lo.ln() + t * (hi.ln() - lo.ln())).exp() } else { lo + t * (hi - lo) }
}

/// The wheel edits a value only while the control is hovered and either focused or Alt is held;
/// otherwise it scrolls the page.
pub fn wheel_allowed(focused: bool, hovered: bool, alt: bool) -> bool {
    hovered && (focused || alt)
}

fn format_value(v: f64, integral: bool, suffix: &str) -> String {
    let a = v.abs();
    let s = if integral || a >= 100.0 {
        format!("{v:.0}")
    } else if a >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    };
    format!("{s}{suffix}")
}

/// Shared input model. `per_point` is the normalized change per point of drag along `axis`
/// (positive x or negative y). Returns true when `t` changed.
fn input(ui: &Ui, resp: &mut Response, t: &mut f32, per_point: f32, vertical: bool) -> bool {
    let before = *t;
    let fine = ui.input(|i| i.modifiers.shift);
    let k = if fine { 0.1 } else { 1.0 };
    if resp.clicked() || resp.drag_started() {
        resp.request_focus();
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        *t += k * per_point * if vertical { -d.y } else { d.x };
    }
    let alt = ui.input(|i| i.modifiers.alt);
    if wheel_allowed(resp.has_focus(), resp.hovered(), alt) {
        let d = ui.input(|i| i.smooth_scroll_delta);
        let delta = d.y + d.x; // Shift+wheel arrives as x on macOS.
        if delta != 0.0 {
            *t += k * delta / 400.0;
            ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
    }
    if resp.has_focus() {
        let step = if fine { 0.001 } else { 0.01 };
        ui.input(|i| {
            if i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::ArrowRight) {
                *t += step;
            }
            if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowLeft) {
                *t -= step;
            }
        });
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            resp.surrender_focus();
        }
    }
    *t = t.clamp(0.0, 1.0);
    *t != before
}

/// Writes `t` back into `v`, handles double-click reset, marks the response changed.
#[allow(clippy::too_many_arguments)]
fn commit<N: Numeric>(resp: &mut Response, v: &mut N, t: f32, lo: N, hi: N, default: N, log: bool, moved: bool) {
    if resp.double_clicked() {
        *v = default;
        resp.mark_changed();
    } else if moved {
        let mut x = from_norm(t, lo.to_f64(), hi.to_f64(), log);
        if N::INTEGRAL {
            x = x.round();
        }
        let new = N::from_f64(x);
        if new.to_f64() != v.to_f64() {
            *v = new;
            resp.mark_changed();
        }
    }
}

/// Value bubble next to the pointer while dragging.
fn bubble(ui: &Ui, id: Id, text: &str, p: &Palette) {
    let Some(pos) = ui.ctx().pointer_interact_pos() else { return };
    let painter = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, id.with("bubble")));
    let galley = painter.layout_no_wrap(text.to_string(), FontId::monospace(11.5), p.text);
    let rect = Rect::from_min_size(pos + vec2(14.0, -26.0), galley.size()).expand2(vec2(6.0, 3.0));
    painter.rect_filled(rect, 5.0, p.surface);
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0, p.accent.gamma_multiply(0.6)), egui::StrokeKind::Outside);
    painter.galley(rect.min + vec2(6.0, 3.0), galley, p.text);
}

/// Horizontal fader: recessed track, accent fill, raised handle.
#[allow(clippy::too_many_arguments)]
pub fn track<N: Numeric>(ui: &mut Ui, v: &mut N, lo: N, hi: N, default: N, log: bool, width: f32) -> Response {
    let p = palette(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click_and_drag());
    let resp_c = resp.clone();
    let mut t = to_norm(v.to_f64(), lo.to_f64(), hi.to_f64(), log);
    let moved = input(ui, &mut resp, &mut t, 1.0 / (width - 16.0).max(1.0), false);
    commit(&mut resp, v, t, lo, hi, default, log, moved);
    let t = to_norm(v.to_f64(), lo.to_f64(), hi.to_f64(), log);
    if ui.is_rect_visible(rect) {
        let m = Motion::of(ui, &resp_c);
        let painter = ui.painter();
        let rail = Rect::from_center_size(rect.center(), vec2(rect.width() - 8.0, 6.0));
        material::recessed(painter, rail, 3.0, &p, 0.6);
        let x = egui::lerp(rail.left()..=rail.right(), t);
        let active = 0.6 + 0.4 * m.hover.max(m.focus);
        painter.rect_filled(Rect::from_min_max(rail.min, Pos2::new(x, rail.max.y)), 3.0, p.accent.gamma_multiply(active));
        let handle = Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::splat(16.0));
        material::raised(painter, handle, 8.0, &Palette { raised: p.knob, ..p }, &m);
    }
    if resp.dragged() {
        bubble(ui, resp.id, &format_value(v.to_f64(), N::INTEGRAL, ""), &p);
    }
    resp.on_hover_cursor(CursorIcon::ResizeHorizontal)
}

/// Rotary knob with label below and value shown on hover or drag.
#[allow(clippy::too_many_arguments, dead_code)] // Used by the effects chain (Task 9).
pub fn knob<N: Numeric>(ui: &mut Ui, label: &str, v: &mut N, lo: N, hi: N, default: N, suffix: &str, log: bool) -> Response {
    let p = palette(ui);
    let size = vec2(64.0, 76.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let resp_c = resp.clone();
    let mut t = to_norm(v.to_f64(), lo.to_f64(), hi.to_f64(), log);
    let moved = input(ui, &mut resp, &mut t, 1.0 / 180.0, true);
    commit(&mut resp, v, t, lo, hi, default, log, moved);
    let t = to_norm(v.to_f64(), lo.to_f64(), hi.to_f64(), log);
    if ui.is_rect_visible(rect) {
        let m = Motion::of(ui, &resp_c);
        let painter = ui.painter();
        let c = Pos2::new(rect.center().x, rect.top() + 24.0);
        let r = 18.0;
        let arc = |from: f32, to: f32| -> Vec<Pos2> {
            (0..=32)
                .map(|i| {
                    let a = egui::lerp(from..=to, i as f32 / 32.0);
                    c + vec2(a.cos(), a.sin()) * (r + 4.0)
                })
                .collect()
        };
        let (start, sweep) = (135f32.to_radians(), 270f32.to_radians());
        painter.add(Shape::line(arc(start, start + sweep), Stroke::new(3.0, p.well)));
        let strength = 0.6 + 0.4 * m.hover.max(m.focus).max(if resp_c.dragged() { 1.0 } else { 0.0 });
        painter.add(Shape::line(arc(start, start + sweep * t), Stroke::new(3.0, p.accent.gamma_multiply(strength))));
        let body = Rect::from_center_size(c, Vec2::splat(r * 2.0));
        material::raised(painter, body, r, &Palette { raised: p.knob, ..p }, &m);
        let a = start + sweep * t;
        painter.circle_filled(c + vec2(a.cos(), a.sin()) * (r - 6.0), 2.5, p.accent);
        let shown = if m.hover > 0.5 || resp_c.dragged() || resp_c.has_focus() {
            format_value(v.to_f64(), N::INTEGRAL, suffix)
        } else {
            label.to_string()
        };
        let color = if ui.is_enabled() { p.muted } else { p.faint };
        painter.text(Pos2::new(rect.center().x, rect.bottom() - 10.0), Align2::CENTER_CENTER, shown, FontId::proportional(11.5), color);
    }
    if resp.dragged() {
        bubble(ui, resp.id, &format_value(v.to_f64(), N::INTEGRAL, suffix), &p);
    }
    resp.on_hover_cursor(CursorIcon::ResizeVertical).on_hover_text(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_round_trip_linear_and_log() {
        for (lo, hi, log) in [(0.0, 1.0, false), (-24.0, 24.0, false), (20.0, 20_000.0, true), (0.01, 1.0, true)] {
            for t in [0.0f32, 0.3, 0.5, 1.0] {
                let v = from_norm(t, lo, hi, log);
                assert!((to_norm(v, lo, hi, log) - t).abs() < 1e-4, "{lo}..{hi} log {log} t {t}");
            }
        }
        assert!((from_norm(0.5, 20.0, 20_000.0, true) - 632.45).abs() < 0.1);
        assert_eq!(to_norm(5.0, 0.0, 1.0, false), 1.0);
    }

    #[test]
    fn wheel_allowed_only_with_focus_or_alt() {
        assert!(!wheel_allowed(false, true, false));
        assert!(wheel_allowed(true, true, false));
        assert!(wheel_allowed(false, true, true));
        assert!(!wheel_allowed(true, false, false));
    }
}
