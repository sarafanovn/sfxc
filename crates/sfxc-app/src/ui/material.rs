//! Soft-relief drawing and the animated interaction state shared by custom widgets.

use eframe::egui::{
    emath::easing, vec2, Color32, Mesh, Painter, Pos2, Rect, Response, Shadow, Stroke, StrokeKind, Ui,
};

use super::theme::{self, Palette};

/// Animated 0..1 interaction values. `press` can briefly go below 0 on release (spring).
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub hover: f32,
    pub press: f32,
    pub focus: f32,
}

impl Motion {
    pub const REST: Motion = Motion { hover: 0.0, press: 0.0, focus: 0.0 };

    pub fn of(ui: &Ui, resp: &Response) -> Self {
        let ctx = ui.ctx();
        let id = resp.id;
        let enabled = ui.is_enabled();
        let down = enabled && resp.is_pointer_button_down_on();
        let press_time = if down { theme::T_PRESS } else { theme::T_RELEASE };
        Self {
            hover: ctx.animate_bool_with_time(id.with("m_hover"), enabled && resp.hovered(), theme::T_HOVER),
            press: ctx.animate_bool_with_time_and_easing(id.with("m_press"), down, press_time, easing::back_out),
            focus: ctx.animate_bool_with_time(id.with("m_focus"), resp.has_focus(), theme::T_HOVER),
        }
    }
}

/// Points of a rounded rectangle outline, clockwise from the bottom-right corner.
fn rounded_outline(rect: Rect, r: f32, seg: usize) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let corners = [
        (rect.right_bottom() + vec2(-r, -r), 0.0f32),
        (rect.left_bottom() + vec2(r, -r), 90.0),
        (rect.left_top() + vec2(r, r), 180.0),
        (rect.right_top() + vec2(-r, r), 270.0),
    ];
    let mut pts = Vec::with_capacity(4 * (seg + 1));
    for (c, start) in corners {
        for i in 0..=seg {
            let a = (start + 90.0 * i as f32 / seg as f32).to_radians();
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts
}

/// Fill with a 4% top-to-bottom gradient.
pub fn surface(painter: &Painter, rect: Rect, radius: f32, base: Color32) {
    painter.rect_filled(rect, radius, base);
    let top = base.lerp_to_gamma(Color32::WHITE, 0.04);
    let bottom = base.lerp_to_gamma(Color32::BLACK, 0.04);
    let shade = |y: f32| top.lerp_to_gamma(bottom, ((y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0));
    let outline = rounded_outline(rect.shrink(0.5), radius - 0.5, 6);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), shade(rect.center().y));
    for pt in &outline {
        mesh.colored_vertex(*pt, shade(pt.y));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(mesh);
}

/// Inner shadow: dark along the top-left inside edge, light along the bottom-right.
fn inset(painter: &Painter, rect: Rect, radius: f32, p: &Palette, depth: f32) {
    if depth <= 0.0 {
        return;
    }
    let clip = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    for k in 1..=3 {
        let k = k as f32;
        let a = depth * 0.45 / k;
        let o = 1.5 * k;
        clip.rect_stroke(rect.translate(vec2(o, o)), radius, Stroke::new(o + 1.0, p.shadow_dark.gamma_multiply(a)), StrokeKind::Outside);
        clip.rect_stroke(rect.translate(vec2(-o, -o)), radius, Stroke::new(o + 1.0, p.shadow_light.gamma_multiply(a)), StrokeKind::Outside);
    }
}

/// Thin accent line just inside the edge; `t` is its strength (hover or focus). Nothing is drawn outside the element.
pub fn accent_edge(painter: &Painter, rect: Rect, radius: f32, p: &Palette, t: f32) {
    if t <= 0.01 {
        return;
    }
    painter.rect_stroke(rect, radius, Stroke::new(1.5, p.accent.gamma_multiply(0.75 * t)), StrokeKind::Inside);
}

/// Raised surface that bulges on hover, sinks on press and springs back on release.
pub fn raised(painter: &Painter, rect: Rect, radius: f32, p: &Palette, m: &Motion) {
    let e = (0.6 + 0.4 * m.hover) * (1.0 - m.press);
    if e > 0.0 {
        // Small controls get small shadows, so they stay inside the gap around them.
        let k = (rect.size().min_elem() / 44.0).clamp(0.45, 1.0);
        let d = ((4.0 * e * k).round() as i8).max(1);
        let blur = ((8.0 + 10.0 * e) * k) as u8;
        painter.add(Shadow { offset: [-d, -d], blur, spread: 0, color: p.shadow_light }.as_shape(rect, radius));
        painter.add(Shadow { offset: [d, d], blur, spread: 0, color: p.shadow_dark }.as_shape(rect, radius));
    }
    surface(painter, rect, radius, p.raised);
    inset(painter, rect, radius, p, m.press.clamp(0.0, 1.0));
    accent_edge(painter, rect, radius, p, m.hover.max(m.focus));
}

/// Sunken well (tracks, slots, bypassed cards). `depth` 0..1.
pub fn recessed(painter: &Painter, rect: Rect, radius: f32, p: &Palette, depth: f32) {
    painter.rect_filled(rect, radius, p.well);
    inset(painter, rect, radius, p, depth);
}
