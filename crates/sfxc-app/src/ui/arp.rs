use sfxc_core::patch::MAX_ARP_STEPS;

use eframe::egui::{self, vec2, Align2, CursorIcon, FontId, Pos2, Rect, Response, Sense, Stroke, Ui};

use super::material;
use super::theme::palette;

pub const PRESETS: [(&str, &[i8]); 4] =
    [("Major", &[0, 4, 7, 12]), ("Minor", &[0, 3, 7, 12]), ("Octaves", &[0, 12]), ("Fifths", &[0, 7])];

const RANGE: i8 = 24;

pub fn add_step(steps: &mut Vec<i8>) {
    if steps.is_empty() {
        steps.extend([0, 12]);
        return;
    }
    if steps.len() >= MAX_ARP_STEPS {
        return;
    }
    let last = *steps.last().expect("not empty");
    let up = (last + 4).min(RANGE);
    steps.push(if up != last { up } else { last - 4 });
}

pub fn steps_that_fit(sound_secs: f32, step_secs: f32) -> usize {
    if step_secs <= 0.0 || sound_secs <= 0.0 { 0 } else { (sound_secs / step_secs).floor() as usize }
}

pub fn playing_step(progress: f32, sound_secs: f32, step_secs: f32, n: usize) -> Option<usize> {
    (n > 0 && step_secs > 0.0).then(|| (progress * sound_secs / step_secs) as usize % n)
}

const BAR_W: f32 = 30.0;
const BAR_H: f32 = 120.0;
const BAR_GAP: f32 = 6.0;

pub fn bars(ui: &mut Ui, steps: &mut [i8], playing: Option<usize>) -> Response {
    let p = palette(ui);
    let n = steps.len().max(1) as f32;
    let (rect, mut all) = ui.allocate_exact_size(vec2(n * (BAR_W + BAR_GAP) - BAR_GAP, BAR_H + 16.0), Sense::hover());
    let y_of = |v: f32| egui::remap(v, -(RANGE as f32)..=RANGE as f32, rect.top() + BAR_H..=rect.top());
    for (i, s) in steps.iter_mut().enumerate() {
        let x = rect.left() + i as f32 * (BAR_W + BAR_GAP);
        let col = Rect::from_min_size(Pos2::new(x, rect.top()), vec2(BAR_W, BAR_H));
        let r = ui.interact(col, all.id.with(i), Sense::click_and_drag()).on_hover_cursor(CursorIcon::ResizeVertical);
        if r.double_clicked() {
            *s = 0;
            all.mark_changed();
        } else if r.dragged()
            && let Some(pt) = r.interact_pointer_pos()
        {
            let v = egui::remap_clamp(pt.y, rect.top() + BAR_H..=rect.top(), -(RANGE as f32)..=RANGE as f32).round() as i8;
            if v != *s {
                *s = v;
                all.mark_changed();
            }
        }
        let painter = ui.painter();
        material::recessed(painter, col, 6.0, &p, 0.6);
        if playing == Some(i) {
            painter.rect_filled(col, 6.0, p.accent_soft);
        }
        let zero = y_of(0.0);
        painter.hline(col.x_range().shrink(4.0), zero, Stroke::new(1.0, p.faint.gamma_multiply(0.5)));
        let top = y_of(*s as f32);
        let fill = Rect::from_x_y_ranges(col.x_range().shrink(5.0), top.min(zero)..=top.max(zero).max(top.min(zero) + 2.0));
        let alpha = if ui.is_enabled() { 1.0 } else { 0.35 };
        painter.rect_filled(fill, 3.0, p.accent.gamma_multiply(alpha));
        let label = format!("{:+}", s);
        painter.text(Pos2::new(col.center().x, rect.bottom() - 6.0), Align2::CENTER_CENTER, label, FontId::monospace(11.0), p.muted);
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_step_first_click_is_audible() {
        let mut s = Vec::new();
        add_step(&mut s);
        assert_eq!(s, vec![0, 12]);
    }

    #[test]
    fn add_step_climbs_by_four_and_never_repeats_at_the_top() {
        let mut s = vec![0, 12];
        add_step(&mut s);
        assert_eq!(s, vec![0, 12, 16]);
        let mut top = vec![24];
        add_step(&mut top);
        assert_eq!(top, vec![24, 20]);
        let mut near = vec![22];
        add_step(&mut near);
        assert_eq!(near, vec![22, 24]);
    }

    #[test]
    fn add_step_respects_max() {
        let mut s = vec![0; MAX_ARP_STEPS];
        add_step(&mut s);
        assert_eq!(s.len(), MAX_ARP_STEPS);
        let mut one_left = vec![0; MAX_ARP_STEPS - 1];
        add_step(&mut one_left);
        assert_eq!(one_left.len(), MAX_ARP_STEPS);
    }

    #[test]
    fn steps_that_fit_edges() {
        assert_eq!(steps_that_fit(0.35, 0.1), 3);
        assert_eq!(steps_that_fit(0.0, 0.1), 0);
        assert_eq!(steps_that_fit(0.35, 0.0), 0);
        assert_eq!(steps_that_fit(0.05, 0.1), 0);
    }

    #[test]
    fn playing_step_wraps() {
        assert_eq!(playing_step(0.0, 1.0, 0.1, 2), Some(0));
        assert_eq!(playing_step(0.15, 1.0, 0.1, 2), Some(1));
        assert_eq!(playing_step(0.25, 1.0, 0.1, 2), Some(0));
        assert_eq!(playing_step(0.5, 1.0, 0.1, 0), None);
    }
}
