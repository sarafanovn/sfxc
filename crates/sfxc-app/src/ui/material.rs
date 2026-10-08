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
    /// 1 when the pointer is on the widget, fading to 0 at `PROXIMITY`.
    pub near: f32,
    /// Pointer position while it is within `PROXIMITY`, for the moving highlight.
    pub pointer: Option<Pos2>,
}

impl Motion {
    // Used by the effects chain (Task 9).
    #[allow(dead_code)]
    pub const REST: Motion = Motion { hover: 0.0, press: 0.0, focus: 0.0, near: 0.0, pointer: None };

    pub fn of(ui: &Ui, resp: &Response) -> Self {
        let ctx = ui.ctx();
        let id = resp.id;
        let enabled = ui.is_enabled();
        let down = enabled && resp.is_pointer_button_down_on();
        let pointer = ctx.pointer_hover_pos().filter(|_| enabled);
        let dist = pointer.map_or(f32::INFINITY, |p| resp.rect.distance_to_pos(p));
        let near_target = (1.0 - dist / theme::PROXIMITY).clamp(0.0, 1.0);
        let press_time = if down { theme::T_PRESS } else { theme::T_RELEASE };
        Self {
            hover: ctx.animate_bool_with_time(id.with("m_hover"), enabled && resp.hovered(), theme::T_HOVER),
            press: ctx.animate_bool_with_time_and_easing(id.with("m_press"), down, press_time, easing::back_out),
            focus: ctx.animate_bool_with_time(id.with("m_focus"), resp.has_focus(), theme::T_HOVER),
            near: ctx.animate_value_with_time(id.with("m_near"), near_target, theme::T_HOVER),
            pointer: pointer.filter(|_| near_target > 0.0),
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

/// Fill with a 4% top-to-bottom gradient and an optional soft highlight at `highlight.0`.
pub fn surface(painter: &Painter, rect: Rect, radius: f32, base: Color32, highlight: Option<(Pos2, f32)>) {
    painter.rect_filled(rect, radius, base);
    let top = base.lerp_to_gamma(Color32::WHITE, 0.04);
    let bottom = base.lerp_to_gamma(Color32::BLACK, 0.04);
    let shade = |y: f32| top.lerp_to_gamma(bottom, ((y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0));
    let outline = rounded_outline(rect.shrink(0.5), radius - 0.5, 6);
    let center = highlight.map_or(rect.center(), |(p, _)| p.clamp(rect.min, rect.max));
    let center_color = match highlight {
        Some((_, s)) => shade(center.y).lerp_to_gamma(Color32::WHITE, s.clamp(0.0, 1.0)),
        None => shade(center.y),
    };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, center_color);
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

/// Thin accent glow along the edge; `t` is its strength (hover or focus).
pub fn accent_edge(painter: &Painter, rect: Rect, radius: f32, p: &Palette, t: f32) {
    if t <= 0.01 {
        return;
    }
    painter.rect_stroke(rect, radius, Stroke::new(1.5, p.accent.gamma_multiply(0.75 * t)), StrokeKind::Outside);
    painter.rect_stroke(rect.expand(1.5), radius + 1.5, Stroke::new(3.0, p.accent.gamma_multiply(0.15 * t)), StrokeKind::Outside);
}

/// Raised surface that bulges on hover, sinks on press and springs back on release.
pub fn raised(painter: &Painter, rect: Rect, radius: f32, p: &Palette, m: &Motion) {
    let e = (0.6 + 0.25 * m.near + 0.4 * m.hover) * (1.0 - m.press);
    if e > 0.0 {
        let d = (4.0 * e).round() as i8;
        let blur = (8.0 + 10.0 * e) as u8;
        painter.add(Shadow { offset: [-d, -d], blur, spread: 0, color: p.shadow_light }.as_shape(rect, radius));
        painter.add(Shadow { offset: [d, d], blur, spread: 0, color: p.shadow_dark }.as_shape(rect, radius));
    }
    let glow = 0.05 * m.near + 0.05 * m.hover;
    surface(painter, rect, radius, p.raised, m.pointer.map(|pt| (pt, glow)));
    inset(painter, rect, radius, p, m.press.clamp(0.0, 1.0));
    accent_edge(painter, rect, radius, p, m.hover.max(m.focus));
}

/// Sunken well (tracks, slots, bypassed cards). `depth` 0..1.
pub fn recessed(painter: &Painter, rect: Rect, radius: f32, p: &Palette, depth: f32) {
    painter.rect_filled(rect, radius, p.well);
    inset(painter, rect, radius, p, depth);
}
