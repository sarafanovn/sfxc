use eframe::egui::{self, vec2, Align, Align2, CursorIcon, FontId, Id, Layout, Pos2, Rect, RichText, Sense, Vec2};
use egui_phosphor::regular as icon;
use sfxc_core::patch::{ranges::*, DistortionKind, Effect, EffectKind};

use super::material::{self, Motion};
use super::theme::{self, palette, Palette, R_CARD};
use super::widgets::{hint, icon_button, knob, knob_num, segmented, toggle};

const CARD_W: f32 = 168.0;
const CARD_H: f32 = 300.0;
const GAP: f32 = 12.0;
/// Horizontal distance between card origins.
const SLOT: f32 = CARD_W + GAP;
const HEAD_H: f32 = 36.0;

/// Slot index under `pointer_x` for a drop among `n` cards starting at `left`.
fn drop_index(pointer_x: f32, left: f32, n: usize) -> usize {
    (((pointer_x - left) / SLOT).floor().max(0.0) as usize).min(n.saturating_sub(1))
}

/// Moves `from` to position `to` (clamped). Same position or bad `from` changes nothing.
fn reorder(effects: &mut Vec<Effect>, from: usize, to: usize) {
    if from >= effects.len() {
        return;
    }
    let to = to.min(effects.len() - 1);
    if from != to {
        let item = effects.remove(from);
        effects.insert(to, item);
    }
}

#[derive(Clone, Copy)]
struct Drag {
    from: usize,
    /// Pointer offset from the card's top-left when the drag started.
    grab: Vec2,
    /// Chain origin and card count, refreshed every frame; the lifted card lives in its own
    /// `Area`, so it cannot read them from its `Ui`.
    left: f32,
    n: usize,
}

enum Op {
    Move(usize, usize),
    Remove(usize),
    Duplicate(usize),
    Add(EffectKind),
}

pub fn show(ui: &mut egui::Ui, effects: &mut Vec<Effect>, next_id: u64) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Effects").text_style(egui::TextStyle::Heading).color(p.text));
        if effects.len() > 1 {
            ui.label(hint(ui, "Drag a card by its title to reorder"));
        }
    });
    ui.add_space(6.0);

    let drag_key = Id::new("fx_drag");
    let mut drag: Option<Drag> = ui.data(|d| d.get_temp(drag_key));
    let mut op = None;
    let n = effects.len();
    egui::ScrollArea::horizontal().id_salt("fx_chain").auto_shrink([false, true]).show(ui, |ui| {
        let (area, _) = ui.allocate_exact_size(vec2((n + 1) as f32 * SLOT + 24.0, CARD_H + 40.0), Sense::hover());
        let left = area.left() + 12.0;
        let slot = |i: usize| Rect::from_min_size(Pos2::new(left + i as f32 * SLOT, area.top() + 16.0), vec2(CARD_W, CARD_H));
        let pointer = ui.ctx().pointer_interact_pos();
        if let Some(d) = drag.as_mut() {
            d.left = left;
            d.n = n;
        }
        let target = drag.and(pointer).map(|pt| drop_index(pt.x, left, n));

        // Display order: every card except the dragged one, with a placeholder (None) at the drop slot.
        let mut display: Vec<Option<usize>> = (0..n).filter(|i| drag.is_none_or(|d| d.from != *i)).map(Some).collect();
        if let (Some(_), Some(t)) = (drag, target) {
            display.insert(t.min(display.len()), None);
        }
        for (pos, item) in display.iter().enumerate() {
            let target_rect = slot(pos);
            match item {
                None => material::recessed(ui.painter(), target_rect, R_CARD as f32, &p, 0.9),
                Some(i) => {
                    let e = &mut effects[*i];
                    let x = ui.ctx().animate_value_with_time(Id::new(("fx_x", e.id)), target_rect.left(), 0.15);
                    let rect = Rect::from_min_size(Pos2::new(x, target_rect.top()), target_rect.size());
                    card(ui, rect, e, *i, false, &mut drag, &mut op);
                }
            }
        }
        if let (Some(d), Some(pt)) = (drag, pointer) {
            let e = &mut effects[d.from];
            egui::Area::new(Id::new("fx_lifted")).order(egui::Order::Foreground).fixed_pos(pt - d.grab).show(ui.ctx(), |ui| {
                let rect = Rect::from_min_size(ui.cursor().min, vec2(CARD_W, CARD_H));
                ui.allocate_rect(rect, Sense::hover());
                card(ui, rect, e, d.from, true, &mut drag, &mut op);
            });
        }
        add_card(ui, slot(n), &mut op);
        if n == 0 {
            ui.painter().text(
                slot(1).left_center(),
                Align2::LEFT_CENTER,
                "Add reverb, delay or a bitcrusher to shape the sound.",
                FontId::proportional(12.5),
                p.faint,
            );
        }
        // Released outside any header (e.g. window lost the pointer): cancel.
        if drag.is_some() && !ui.input(|i| i.pointer.any_down()) && op.is_none() {
            drag = None;
        }
        if let (Some(Op::Move(..)), Some(pt)) = (&op, pointer)
            && !area.expand(24.0).contains(pt)
        {
            op = None;
        }
    });
    ui.data_mut(|d| match drag {
        Some(v) => {
            d.insert_temp(drag_key, v);
        }
        None => d.remove::<Drag>(drag_key),
    });

    match op {
        Some(Op::Move(from, to)) => reorder(effects, from, to),
        Some(Op::Remove(i)) => {
            effects.remove(i);
        }
        Some(Op::Duplicate(i)) => {
            let mut copy = effects[i].clone();
            copy.id = next_id;
            effects.insert(i + 1, copy);
        }
        Some(Op::Add(kind)) => effects.push(Effect { id: next_id, enabled: true, kind }),
        None => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn card(ui: &mut egui::Ui, rect: Rect, effect: &mut Effect, i: usize, lifted: bool, drag: &mut Option<Drag>, op: &mut Option<Op>) {
    let p = palette(ui);
    let head_rect = Rect::from_min_size(rect.min, vec2(CARD_W, HEAD_H));
    let head = ui.interact(head_rect, Id::new(("fx_head", effect.id)), Sense::drag()).on_hover_cursor(CursorIcon::Grab);
    if head.drag_started()
        && let Some(pt) = head.interact_pointer_pos()
    {
        // `left` and `n` are filled in by `show` on the next frame, before anything uses them.
        *drag = Some(Drag { from: i, grab: pt - rect.min, left: 0.0, n: 0 });
    }
    if head.drag_stopped()
        && let Some(d) = drag.take()
        && let Some(pt) = ui.ctx().pointer_interact_pos()
    {
        *op = Some(Op::Move(d.from, drop_index(pt.x, d.left, d.n)));
    }
    let painter = ui.painter();
    if effect.enabled || lifted {
        let m = if lifted { Motion { hover: 1.0, near: 1.0, ..Motion::REST } } else { Motion::of(ui, &head) };
        material::raised(painter, rect, R_CARD as f32, &Palette { raised: p.surface, ..p }, &Motion { pointer: None, ..m });
    } else {
        material::recessed(painter, rect, R_CARD as f32, &p, 0.5);
    }
    let title_color = if effect.enabled { p.text } else { p.faint };
    painter.text(head_rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, effect.kind.name(), FontId::new(13.5, theme::semibold()), title_color);

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(head_rect.shrink2(vec2(8.0, 4.0))).layout(Layout::right_to_left(Align::Center)));
    let more = icon_button(&mut child, icon::DOTS_THREE, "More");
    egui::Popup::menu(&more).show(|ui| {
        if ui.button(format!("{}  Duplicate", icon::COPY)).clicked() {
            *op = Some(Op::Duplicate(i));
        }
        if ui.button(RichText::new(format!("{}  Remove", icon::TRASH)).color(p.danger)).clicked() {
            *op = Some(Op::Remove(i));
        }
    });
    toggle(&mut child, &mut effect.enabled, "").on_hover_text(if effect.enabled { "Bypass" } else { "Enable" });

    let body = Rect::from_min_max(rect.min + vec2(10.0, HEAD_H), rect.max - vec2(10.0, 10.0));
    let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::left_to_right(Align::TOP).with_main_wrap(true)));
    body_ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
    body_ui.add_enabled_ui(effect.enabled, |ui| params(ui, &mut effect.kind));
}

fn add_card(ui: &mut egui::Ui, rect: Rect, op: &mut Option<Op>) {
    let p = palette(ui);
    let resp = ui.interact(rect, Id::new("fx_add"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Add effect");
    let m = Motion::of(ui, &resp);
    material::recessed(ui.painter(), rect, R_CARD as f32, &p, 0.5 + 0.3 * m.hover);
    material::accent_edge(ui.painter(), rect, R_CARD as f32, &p, m.hover);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(28.0), if m.hover > 0.5 { p.accent_text } else { p.muted });
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(160.0);
        for kind in EffectKind::all_defaults() {
            if ui.button(kind.name()).clicked() {
                *op = Some(Op::Add(kind));
            }
        }
    });
}

fn params(ui: &mut egui::Ui, kind: &mut EffectKind) {
    let d = kind.defaults();
    match (kind, d) {
        (EffectKind::Bitcrusher { bits, downsample, mix }, EffectKind::Bitcrusher { bits: b0, downsample: s0, mix: m0 }) => {
            knob(ui, "Bits", bits, BITS, b0, "", false);
            knob(ui, "Downsample", downsample, DOWNSAMPLE, s0, "×", true);
            knob(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (EffectKind::Distortion { kind, drive, tone, mix }, EffectKind::Distortion { drive: d0, tone: t0, mix: m0, .. }) => {
            let options: Vec<_> = DistortionKind::ALL.iter().map(|k| (*k, k.label())).collect();
            ui.allocate_ui(vec2(ui.available_width(), 28.0), |ui| segmented(ui, kind, &options));
            knob(ui, "Drive", drive, DRIVE, d0, "×", true);
            knob(ui, "Tone", tone, TONE, t0, " Hz", true);
            knob(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Phaser { rate, depth, stages, feedback, mix },
            EffectKind::Phaser { rate: r0, depth: d0, feedback: f0, mix: m0, .. },
        ) => {
            knob(ui, "Rate", rate, LFO_RATE, r0, " Hz", true);
            knob(ui, "Depth", depth, UNIT, d0, "", false);
            knob_num(ui, "Stages", stages, 2, 8, 4, "");
            knob(ui, "Feedback", feedback, FEEDBACK, f0, "", false);
            knob(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Flanger { rate, depth_ms, delay_ms, feedback, mix },
            EffectKind::Flanger { rate: r0, depth_ms: dp0, delay_ms: dl0, feedback: f0, mix: m0 },
        ) => {
            knob(ui, "Rate", rate, LFO_RATE, r0, " Hz", true);
            knob(ui, "Depth", depth_ms, FLANGER_DEPTH_MS, dp0, " ms", false);
            knob(ui, "Delay", delay_ms, FLANGER_DELAY_MS, dl0, " ms", false);
            knob(ui, "Feedback", feedback, FLANGER_FEEDBACK, f0, "", false);
            knob(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Delay { time, feedback, damping, mix },
            EffectKind::Delay { time: t0, feedback: f0, damping: d0, mix: m0 },
        ) => {
            knob(ui, "Time", time, DELAY_TIME, t0, " s", true);
            knob(ui, "Feedback", feedback, FEEDBACK, f0, "", false);
            knob(ui, "Damping", damping, UNIT, d0, "", false);
            knob(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Reverb { size, decay, damping, predelay, mix },
            EffectKind::Reverb { size: s0, decay: d0, damping: dm0, predelay: p0, mix: m0 },
        ) => {
            knob(ui, "Size", size, UNIT, s0, "", false);
            knob(ui, "Decay", decay, UNIT, d0, "", false);
            knob(ui, "Damping", damping, UNIT, dm0, "", false);
            knob(ui, "Pre-delay", predelay, PREDELAY, p0, " s", false);
            knob(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Compressor { threshold_db, ratio, attack, release, makeup_db },
            EffectKind::Compressor { threshold_db: t0, ratio: r0, attack: a0, release: rl0, makeup_db: m0 },
        ) => {
            knob(ui, "Threshold", threshold_db, THRESHOLD_DB, t0, " dB", false);
            knob(ui, "Ratio", ratio, RATIO, r0, ":1", true);
            knob(ui, "Attack", attack, COMP_ATTACK, a0, " s", true);
            knob(ui, "Release", release, COMP_RELEASE, rl0, " s", true);
            knob(ui, "Makeup", makeup_db, MAKEUP_DB, m0, " dB", false);
        }
        _ => unreachable!("EffectKind::defaults returns the same variant"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(n: u64) -> Vec<Effect> {
        (1..=n).map(|id| Effect { id, enabled: true, kind: EffectKind::all_defaults()[0] }).collect()
    }

    fn ids(e: &[Effect]) -> Vec<u64> {
        e.iter().map(|e| e.id).collect()
    }

    #[test]
    fn reorder_moves_forward_and_back() {
        let mut e = chain(4);
        reorder(&mut e, 0, 2);
        assert_eq!(ids(&e), vec![2, 3, 1, 4]);
        reorder(&mut e, 3, 0);
        assert_eq!(ids(&e), vec![4, 2, 3, 1]);
    }

    #[test]
    fn reorder_in_place_or_out_of_range_is_noop() {
        let mut e = chain(3);
        reorder(&mut e, 1, 1);
        reorder(&mut e, 5, 0);
        assert_eq!(ids(&e), vec![1, 2, 3]);
        reorder(&mut e, 0, 9);
        assert_eq!(ids(&e), vec![2, 3, 1]);
    }

    #[test]
    fn drop_index_clamps() {
        assert_eq!(drop_index(-50.0, 0.0, 3), 0);
        assert_eq!(drop_index(SLOT * 1.5, 0.0, 3), 1);
        assert_eq!(drop_index(SLOT * 10.0, 0.0, 3), 2);
        assert_eq!(drop_index(10.0, 0.0, 0), 0);
    }
}
