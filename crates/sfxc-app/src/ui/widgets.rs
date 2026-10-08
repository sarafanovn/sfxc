use eframe::egui::{self, Align2, FontId, Pos2, Sense, Stroke, Vec2};
use sfxc_core::patch::Range;

/// Slider for one parameter. Double-click resets to `default`.
pub fn param(ui: &mut egui::Ui, label: &str, v: &mut f32, range: Range, default: f32, suffix: &str, log: bool) -> egui::Response {
    let mut resp = ui.add(egui::Slider::new(v, range.0..=range.1).text(label).suffix(suffix).logarithmic(log));
    if resp.double_clicked() {
        *v = default;
        resp.mark_changed();
    }
    resp
}

/// Min/max-per-column waveform with duration label.
pub fn waveform(ui: &mut egui::Ui, rendered: Option<(&[f32], u32)>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 110.0), Sense::hover());
    let visuals = ui.visuals().clone();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
    let mid = rect.center().y;
    painter.line_segment(
        [Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.4)),
    );
    let Some((samples, sample_rate)) = rendered.filter(|(s, _)| !s.is_empty()) else {
        painter.text(rect.center(), Align2::CENTER_CENTER, "silence", FontId::proportional(13.0), visuals.weak_text_color());
        return;
    };
    let cols = rect.width().max(1.0) as usize;
    let per = (samples.len() as f32 / cols as f32).max(1.0);
    let half = rect.height() * 0.45;
    let stroke = Stroke::new(1.0, visuals.selection.stroke.color);
    for c in 0..cols {
        let a = (c as f32 * per) as usize;
        let b = (((c + 1) as f32 * per) as usize).min(samples.len());
        if a >= b {
            break;
        }
        let (lo, hi) = samples[a..b].iter().fold((0.0f32, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        let x = rect.left() + c as f32 + 0.5;
        painter.line_segment([Pos2::new(x, mid - hi * half), Pos2::new(x, mid - lo * half)], stroke);
    }
    painter.text(
        rect.right_bottom() + Vec2::new(-6.0, -4.0),
        Align2::RIGHT_BOTTOM,
        format!("{:.2} s", samples.len() as f32 / sample_rate as f32),
        FontId::monospace(11.0),
        visuals.weak_text_color(),
    );
}
