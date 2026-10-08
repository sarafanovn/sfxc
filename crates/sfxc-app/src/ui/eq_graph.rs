//! Graphic-equalizer widget: one draggable point per band on a smooth curve, over a ±12 dB grid.

use eframe::egui::{self, vec2, Align2, Color32, CursorIcon, FontId, Id, Mesh, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};
use sfxc_core::eq::{Equalizer, BANDS, RANGE_DB};

use super::material;
use super::theme::{palette, R_CONTROL};

pub const FREQ_LABELS: [&str; BANDS] = ["60 Hz", "150 Hz", "400 Hz", "1 kHz", "2.4 kHz", "15 kHz"];

/// Space left of the plot for the "+12 dB / -12 dB" labels, and below it for the frequencies.
const LEFT: f32 = 62.0;
const BOTTOM: f32 = 30.0;
const TOP: f32 = 18.0;
pub const HEIGHT: f32 = 230.0;

/// y position of `gain` dB: +12 at `top`, -12 at `bottom`.
pub fn gain_to_y(gain: f32, top: f32, bottom: f32) -> f32 {
    top + (RANGE_DB - gain.clamp(-RANGE_DB, RANGE_DB)) / (2.0 * RANGE_DB) * (bottom - top)
}

/// Gain under pointer height `y`, clamped to the range and snapped to half a decibel.
pub fn y_to_gain(y: f32, top: f32, bottom: f32) -> f32 {
    let g = RANGE_DB - (y - top) / (bottom - top) * 2.0 * RANGE_DB;
    ((g * 2.0).round() / 2.0).clamp(-RANGE_DB, RANGE_DB)
}

/// x of band `i`: the bands sit in the middle of equal columns of the plot.
pub fn band_x(i: usize, left: f32, right: f32) -> f32 {
    left + (i as f32 + 0.5) / BANDS as f32 * (right - left)
}

/// Smooth curve through the band gains (monotone cubic, so it never overshoots a point).
/// Returns `(band position 0..=5, gain)` with `per_segment` steps between neighbouring bands.
pub fn smooth_curve(gains: &[f32; BANDS], per_segment: usize) -> Vec<(f32, f32)> {
    let n = BANDS;
    // Fritsch–Carlson tangents.
    let delta: Vec<f32> = (0..n - 1).map(|i| gains[i + 1] - gains[i]).collect();
    let mut m = vec![0.0f32; n];
    m[0] = delta[0];
    m[n - 1] = delta[n - 2];
    for i in 1..n - 1 {
        m[i] = if delta[i - 1] * delta[i] <= 0.0 { 0.0 } else { (delta[i - 1] + delta[i]) / 2.0 };
    }
    for i in 0..n - 1 {
        if delta[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / delta[i], m[i + 1] / delta[i]);
        let h = a.hypot(b);
        if h > 3.0 {
            m[i] = 3.0 * a / h * delta[i];
            m[i + 1] = 3.0 * b / h * delta[i];
        }
    }
    let mut out = Vec::with_capacity((n - 1) * per_segment + 1);
    for i in 0..n - 1 {
        for k in 0..per_segment {
            let t = k as f32 / per_segment as f32;
            let (t2, t3) = (t * t, t * t * t);
            let y = (2.0 * t3 - 3.0 * t2 + 1.0) * gains[i]
                + (t3 - 2.0 * t2 + t) * m[i]
                + (-2.0 * t3 + 3.0 * t2) * gains[i + 1]
                + (t3 - t2) * m[i + 1];
            out.push((i as f32 + t, y));
        }
    }
    out.push(((n - 1) as f32, gains[n - 1]));
    out
}

/// Lets tests find the draggable points drawn in the last frame.
#[cfg(test)]
pub mod test_support {
    use std::cell::RefCell;

    use eframe::egui::Pos2;

    thread_local! {
        pub static POINTS: RefCell<Vec<Pos2>> = const { RefCell::new(Vec::new()) };
    }

    pub fn take_points() -> Vec<Pos2> {
        POINTS.with(|p| std::mem::take(&mut *p.borrow_mut()))
    }
}

/// The equalizer graph. Dragging inside a band's column sets its gain, double click resets it to 0.
/// The response is marked changed when a gain moved.
pub fn show(ui: &mut Ui, eq: &mut Equalizer) -> Response {
    let p = palette(ui);
    let (rect, mut all) = ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), egui::Sense::hover());
    material::recessed(ui.painter(), rect, R_CONTROL as f32 + 2.0, &p, 0.6);
    let plot = Rect::from_min_max(Pos2::new(rect.left() + LEFT, rect.top() + TOP), Pos2::new(rect.right() - 16.0, rect.bottom() - BOTTOM));
    let enabled = eq.enabled && ui.is_enabled();
    let accent = if enabled { p.accent } else { p.faint };
    let column_w = plot.width() / BANDS as f32;

    // Interaction first, so the drawing below shows the value after the drag.
    for i in 0..BANDS {
        let x = band_x(i, plot.left(), plot.right());
        let col = Rect::from_min_max(Pos2::new(x - column_w / 2.0, plot.top() - 8.0), Pos2::new(x + column_w / 2.0, plot.bottom() + 8.0));
        let r = ui.interact(col, Id::new(("eq_band", i)), Sense::click_and_drag()).on_hover_cursor(CursorIcon::ResizeVertical);
        let new = if r.double_clicked() {
            Some(0.0)
        } else if r.dragged() {
            r.interact_pointer_pos().map(|pt| y_to_gain(pt.y, plot.top(), plot.bottom()))
        } else {
            None
        };
        if let Some(g) = new.filter(|g| (*g - eq.gains[i]).abs() > 1e-6) {
            eq.gains[i] = g;
            eq.enabled = true;
            all.mark_changed();
        }
    }

    let painter = ui.painter();
    // Grid: a line through every band and the zero line.
    let grid = Stroke::new(1.0, p.faint.gamma_multiply(0.25));
    for i in 0..BANDS {
        let x = band_x(i, plot.left(), plot.right());
        painter.vline(x, plot.y_range(), grid);
    }
    painter.hline(plot.x_range().expand(8.0), gain_to_y(0.0, plot.top(), plot.bottom()), Stroke::new(1.0, p.faint.gamma_multiply(0.45)));
    let axis = FontId::proportional(11.5);
    painter.text(Pos2::new(rect.left() + 14.0, plot.top()), Align2::LEFT_CENTER, "+12 dB", axis.clone(), p.muted);
    painter.text(Pos2::new(rect.left() + 14.0, plot.bottom()), Align2::LEFT_CENTER, "-12 dB", axis.clone(), p.muted);
    for (i, label) in FREQ_LABELS.iter().enumerate() {
        painter.text(Pos2::new(band_x(i, plot.left(), plot.right()), rect.bottom() - 14.0), Align2::CENTER_CENTER, *label, axis.clone(), p.muted);
    }

    // The curve, with a fade under it.
    let to_screen = |(t, g): (f32, f32)| {
        let x = band_x(0, plot.left(), plot.right()) + t * column_w;
        Pos2::new(x, gain_to_y(g, plot.top(), plot.bottom()))
    };
    let pts: Vec<Pos2> = smooth_curve(&eq.gains, 12).into_iter().map(to_screen).collect();
    if enabled {
        let base = plot.bottom();
        let mut mesh = Mesh::default();
        for pt in &pts {
            mesh.colored_vertex(*pt, accent.gamma_multiply(0.5));
            mesh.colored_vertex(Pos2::new(pt.x, base), Color32::TRANSPARENT);
        }
        for k in 0..pts.len() as u32 - 1 {
            let a = 2 * k;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
        painter.add(mesh);
    }
    painter.add(Shape::line(pts, Stroke::new(3.0, accent)));

    // Points and, while a band is hovered or dragged, its value.
    for i in 0..BANDS {
        let c = Pos2::new(band_x(i, plot.left(), plot.right()), gain_to_y(eq.gains[i], plot.top(), plot.bottom()));
        #[cfg(test)]
        test_support::POINTS.with(|v| v.borrow_mut().push(c));
        let col = Rect::from_center_size(c, vec2(column_w, plot.height() + 16.0));
        let active = ui.rect_contains_pointer(col) || ui.ctx().is_being_dragged(Id::new(("eq_band", i)));
        painter.circle_filled(c, if active { 7.5 } else { 6.0 }, if enabled { Color32::WHITE } else { p.muted });
        if active {
            painter.text(
                c - Vec2::new(0.0, 16.0),
                Align2::CENTER_BOTTOM,
                format!("{:+.1} dB", eq.gains[i]),
                FontId::monospace(11.5),
                p.text,
            );
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_passes_through_every_control_point() {
        let gains = [2.0, 1.5, -1.0, -0.5, -0.5, 2.0];
        let curve = smooth_curve(&gains, 8);
        for (i, g) in gains.iter().enumerate() {
            let (x, y) = curve[i * 8];
            assert!((x - i as f32).abs() < 1e-5 && (y - g).abs() < 1e-4, "band {i}: {x}, {y}");
        }
        assert_eq!(curve.len(), (gains.len() - 1) * 8 + 1);
    }

    #[test]
    fn curve_never_overshoots_the_points_it_joins() {
        let gains = [12.0, -12.0, 12.0, 12.0, -12.0, 0.0];
        for (_, y) in smooth_curve(&gains, 16) {
            assert!((-12.0..=12.0).contains(&y), "{y}");
        }
        // Flat stays flat, a monotone ramp stays monotone.
        assert!(smooth_curve(&[0.0; 6], 4).iter().all(|(_, y)| y.abs() < 1e-6));
        let ramp = smooth_curve(&[-6.0, -3.0, 0.0, 3.0, 6.0, 12.0], 8);
        assert!(ramp.windows(2).all(|w| w[1].1 >= w[0].1 - 1e-5));
    }

    #[test]
    fn gain_and_y_are_inverse_and_clamped() {
        let (top, bottom) = (10.0, 210.0);
        assert_eq!(gain_to_y(12.0, top, bottom), top);
        assert_eq!(gain_to_y(-12.0, top, bottom), bottom);
        assert_eq!(gain_to_y(0.0, top, bottom), 110.0);
        for g in [-12.0, -7.5, 0.0, 3.5, 12.0] {
            assert!((y_to_gain(gain_to_y(g, top, bottom), top, bottom) - g).abs() < 0.26, "{g}");
        }
        assert_eq!(y_to_gain(-500.0, top, bottom), 12.0);
        assert_eq!(y_to_gain(900.0, top, bottom), -12.0);
    }

    #[test]
    fn dragged_gains_snap_to_half_a_decibel() {
        let (top, bottom) = (0.0, 240.0);
        let g = y_to_gain(100.3, top, bottom);
        assert!(((g * 2.0).round() - g * 2.0).abs() < 1e-5, "{g}");
    }

    #[test]
    fn band_columns_are_evenly_spaced_inside_the_plot() {
        let xs: Vec<f32> = (0..6).map(|i| band_x(i, 100.0, 700.0)).collect();
        assert!(xs[0] > 100.0 && xs[5] < 700.0);
        let step = xs[1] - xs[0];
        assert!(xs.windows(2).all(|w| (w[1] - w[0] - step).abs() < 1e-4));
    }

    #[test]
    fn labels_name_every_band() {
        assert_eq!(FREQ_LABELS.len(), sfxc_core::eq::BANDS);
        assert_eq!(FREQ_LABELS[0], "60 Hz");
        assert_eq!(FREQ_LABELS[5], "15 kHz");
    }
}
