//! Master effects chain: a horizontal strip of tall cards in signal order. Every card has a fixed rail on its
//! left (handle, fold caret, the name reading upward, bypass, remove); open, the knobs sit to the right of it.
//! Folding only hides the knobs: the rail never moves, so the card just narrows to it.

use eframe::egui::{self, vec2, Align, Align2, CursorIcon, FontId, Id, Layout, Pos2, Rect, Sense};
use egui_phosphor::regular as icon;
use sfxc_core::patch::{ranges::*, DistortionKind, Effect, EffectKind};

use super::material;
use super::order;
use super::theme::{self, palette, R_CARD};
use super::widgets::{drop_zone, grip_handle, icon_button, knob, knob_num, power_toggle, section, segmented, Axis, Grip};

const CARD_W: f32 = 224.0;
const CARD_H: f32 = 300.0;
const GAP: f32 = 12.0;
/// Width of the rail, and of a folded card.
const RAIL_W: f32 = 44.0;
const PICK_ROW: f32 = 30.0;
/// Seconds the card takes to widen or narrow.
const FOLD_TIME: f32 = 0.12;

pub enum Op {
    Move(usize, usize),
    Remove(usize),
    Add(EffectKind),
}

/// Applies one edit to the chain. Stale indices are ignored, so a late click cannot panic.
pub fn apply(effects: &mut Vec<Effect>, op: Op, next_id: u64) {
    match op {
        Op::Move(from, to) => order::move_item(effects, from, to),
        Op::Remove(i) if i < effects.len() => {
            effects.remove(i);
        }
        Op::Add(kind) => effects.push(Effect { id: next_id, enabled: true, kind }),
        Op::Remove(_) => {}
    }
}

/// The "Effects" section: a card like the sound-settings ones, holding the strip of effect cards.
pub fn show(ui: &mut egui::Ui, effects: &mut Vec<Effect>, next_id: u64, grip: Grip) -> Option<(usize, usize)> {
    let mut op = None;
    let moved = section(ui, "effects", "Effects", Some(grip), |_| {}, |ui| {
        egui::ScrollArea::horizontal().id_salt("fx_chain").auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for (i, effect) in effects.iter_mut().enumerate() {
                    if let Some((from, to)) = card(ui, effect, i, &mut op) {
                        op = Some(Op::Move(from, to));
                    }
                }
                let (rect, _) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::hover());
                add_card(ui, rect, &mut op);
            });
            // Room for the floating scroll bar.
            ui.add_space(12.0);
        });
    });
    if let Some(op) = op {
        apply(effects, op, next_id);
    }
    moved
}

/// One effect. Returns `(from, to)` when another card was dropped on it.
fn card(ui: &mut egui::Ui, effect: &mut Effect, i: usize, op: &mut Option<Op>) -> Option<(usize, usize)> {
    let p = palette(ui);
    let open_id = Id::new(("fx_open", effect.id));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
    let t = ui.ctx().animate_bool_with_time_and_easing(open_id.with("t"), open, FOLD_TIME, egui::emath::easing::cubic_out);
    let (rect, _) = ui.allocate_exact_size(vec2(egui::lerp(RAIL_W..=CARD_W, t), CARD_H), Sense::hover());
    let grip = Grip { group: "effects", index: i };
    let title_color = if effect.enabled { p.text } else { p.faint };

    // Sunken cell; a bypassed one is nearly flat.
    material::recessed(ui.painter(), rect, R_CARD as f32, &p, if effect.enabled { 0.5 } else { 0.12 });

    // Rail: the same in both states.
    let rail = Rect::from_min_size(rect.min, vec2(RAIL_W, CARD_H));
    let mut top = ui.new_child(egui::UiBuilder::new().max_rect(rail.shrink2(vec2(4.0, 6.0))).layout(Layout::top_down(Align::Center)));
    top.spacing_mut().item_spacing.y = 4.0;
    grip_handle(&mut top, grip, &effect.id.to_string());
    let (caret, caret_tip) = if open { (icon::CARET_LEFT, "Fold") } else { (icon::CARET_RIGHT, "Unfold") };
    if icon_button(&mut top, caret, caret_tip).clicked() {
        open = !open;
    }
    let mut foot = ui.new_child(egui::UiBuilder::new().max_rect(rail.shrink2(vec2(4.0, 6.0))).layout(Layout::bottom_up(Align::Center)));
    foot.spacing_mut().item_spacing.y = 6.0;
    if icon_button(&mut foot, icon::TRASH, "Remove").clicked() {
        *op = Some(Op::Remove(i));
    }
    power_toggle(&mut foot, &mut effect.enabled).on_hover_text(if effect.enabled { "Bypass" } else { "Enable" });

    // Name between the buttons, hanging from the top and reading upward; clicking it folds or unfolds.
    let galley = ui.painter().layout_no_wrap(effect.kind.name().to_owned(), FontId::new(13.5, theme::semibold()), title_color);
    let zone = Rect::from_min_max(Pos2::new(rail.left(), rail.top() + 76.0), Pos2::new(rail.right(), rail.bottom() - 80.0));
    if ui.interact(zone, open_id.with("title"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand).on_hover_text(caret_tip).clicked() {
        open = !open;
    }
    let pos = Pos2::new(rail.center().x - galley.size().y / 2.0, zone.top() + galley.size().x);
    ui.painter().add(egui::epaint::TextShape::new(pos, galley, title_color).with_angle(-std::f32::consts::FRAC_PI_2));

    // Knobs right of the rail, laid out at full width and uncovered as the card widens.
    if t > 0.0 {
        let body = Rect::from_min_size(Pos2::new(rail.right() + 2.0, rect.top() + 12.0), vec2(CARD_W - RAIL_W - 12.0, CARD_H - 24.0));
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::left_to_right(Align::TOP).with_main_wrap(true)));
        body_ui.set_clip_rect(rect.shrink(2.0).intersect(body_ui.clip_rect()));
        body_ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
        body_ui.add_enabled_ui(effect.enabled && open, |ui| params(ui, &mut effect.kind));
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));

    drop_zone(ui, rect, grip, Axis::Horizontal, GAP / 2.0 + ui.spacing().item_spacing.x / 2.0)
}

/// The trailing slot: a sunken well like the effect cards, with a quiet "+". Clicking turns it into
/// the list of effects; picking one, Esc or a click elsewhere turns it back.
fn add_card(ui: &mut egui::Ui, rect: Rect, op: &mut Option<Op>) {
    let p = palette(ui);
    let open_id = Id::new("fx_add_open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let radius = R_CARD as f32;

    material::recessed(ui.painter(), rect, radius, &p, 0.5);
    if !open {
        let resp = ui.interact(rect, Id::new("fx_add"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Add effect");
        if resp.clicked() {
            open = true;
        }
        let c = if resp.hovered() { p.accent_text } else { p.faint };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(26.0), c);
    } else {
        for (i, kind) in EffectKind::all_defaults().into_iter().enumerate() {
            let row = Rect::from_min_size(Pos2::new(rect.left() + 8.0, rect.top() + 8.0 + i as f32 * (PICK_ROW + 2.0)), vec2(CARD_W - 16.0, PICK_ROW));
            let resp = ui.interact(row, Id::new(("fx_pick", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            if resp.hovered() {
                ui.painter().rect_filled(row, theme::R_CONTROL, p.hover);
            }
            let c = if resp.hovered() { p.text } else { p.muted };
            ui.painter().text(row.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, kind.name(), FontId::proportional(13.0), c);
            if resp.clicked() {
                *op = Some(Op::Add(kind));
                open = false;
            }
        }
        let outside = ui.input(|i| i.pointer.any_pressed()) && !ui.ctx().pointer_interact_pos().is_some_and(|pt| rect.contains(pt));
        if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
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
    fn move_reorders_and_ignores_no_ops() {
        let mut e = chain(4);
        apply(&mut e, Op::Move(0, 2), 99);
        assert_eq!(ids(&e), vec![2, 3, 1, 4]);
        apply(&mut e, Op::Move(1, 1), 99);
        apply(&mut e, Op::Move(9, 0), 99);
        assert_eq!(ids(&e), vec![2, 3, 1, 4]);
    }

    #[test]
    fn remove_and_stale_index_are_safe() {
        let mut e = chain(3);
        apply(&mut e, Op::Remove(1), 99);
        assert_eq!(ids(&e), vec![1, 3]);
        apply(&mut e, Op::Remove(7), 99);
        assert_eq!(ids(&e), vec![1, 3]);
    }

    #[test]
    fn add_appends_enabled_effect_with_the_given_id() {
        let mut e = chain(1);
        apply(&mut e, Op::Add(EffectKind::all_defaults()[1]), 5);
        assert_eq!(ids(&e), vec![1, 5]);
        assert!(e[1].enabled);
    }
}
