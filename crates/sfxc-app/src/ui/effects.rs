//! Master effects chain: one collapsible card per effect, same look as the sound settings.

use sfxc_core::patch::{ranges::*, DistortionKind, Effect, EffectKind};

use eframe::egui::{self, vec2, Align, Align2, CursorIcon, FontId, Id, Layout, Pos2, Rect, RichText, Sense};
use egui_phosphor::regular as icon;

use super::material::{self, Motion};
use super::order;
use super::theme::{self, palette, Palette, R_CARD};
use super::widgets::{icon_button, param, param_num, row_background, section, segmented, toggle, Grip};

const HEAD_H: f32 = 36.0;
const PICK_ROW: f32 = 30.0;
const ADD_CLOSED_H: f32 = 56.0;

pub enum Op {
    Move(usize, usize),
    Remove(usize),
    Duplicate(usize),
    Add(EffectKind),
}

/// Applies one edit to the chain. Stale indices are ignored, so a late click cannot panic.
pub fn apply(effects: &mut Vec<Effect>, op: Op, next_id: u64) {
    match op {
        Op::Move(from, to) => order::move_item(effects, from, to),
        Op::Remove(i) if i < effects.len() => {
            effects.remove(i);
        }
        Op::Duplicate(i) if i < effects.len() => {
            let mut copy = effects[i].clone();
            copy.id = next_id;
            effects.insert(i + 1, copy);
        }
        Op::Add(kind) => effects.push(Effect { id: next_id, enabled: true, kind }),
        Op::Remove(_) | Op::Duplicate(_) => {}
    }
}

pub fn show(ui: &mut egui::Ui, effects: &mut Vec<Effect>, next_id: u64) {
    let p = palette(ui);
    ui.label(RichText::new("Effects").text_style(egui::TextStyle::Heading).color(p.text));
    ui.add_space(8.0);

    let mut op = None;
    for (i, e) in effects.iter_mut().enumerate() {
        let enabled = e.enabled;
        let key = format!("fx{}", e.id);
        let moved = section(
            ui,
            &key,
            e.kind.name(),
            Some(Grip { group: "effects", index: i }),
            |ui| {
                let more = icon_button(ui, icon::DOTS_THREE, "More");
                egui::Popup::menu(&more).show(|ui| {
                    if ui.button(format!("{}  Duplicate", icon::COPY)).clicked() {
                        op = Some(Op::Duplicate(i));
                    }
                    if ui.button(RichText::new(format!("{}  Remove", icon::TRASH)).color(p.danger)).clicked() {
                        op = Some(Op::Remove(i));
                    }
                });
                toggle(ui, &mut e.enabled, "").on_hover_text(if enabled { "Bypass" } else { "Enable" });
            },
            |ui| {
                ui.add_enabled_ui(enabled, |ui| params(ui, &mut e.kind));
            },
        );
        if let Some((from, to)) = moved {
            op = Some(Op::Move(from, to));
        }
    }
    add_row(ui, &mut op);

    if let Some(op) = op {
        apply(effects, op, next_id);
    }
}

/// The trailing "+" bar. Clicking it grows the bar into the list of effects; picking one, Esc,
/// the close button or a click elsewhere folds it back.
fn add_row(ui: &mut egui::Ui, op: &mut Option<Op>) {
    let p = palette(ui);
    let kinds = EffectKind::all_defaults();
    let open_id = Id::new("fx_add_open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let t = ui.ctx().animate_bool_with_time(open_id.with("t"), open, 0.2);
    let open_h = HEAD_H + kinds.len() as f32 * (PICK_ROW + 2.0) + 10.0;
    let h = egui::lerp(ADD_CLOSED_H..=open_h, t);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let radius = R_CARD as f32;

    material::recessed(ui.painter(), rect, radius, &p, 0.5 * (1.0 - t));
    if t < 0.99 {
        let closed = Rect::from_min_size(rect.min, vec2(rect.width(), ADD_CLOSED_H));
        let alpha = 1.0 - t;
        let mut color = p.muted;
        if !open {
            let resp = ui.interact(closed, Id::new("fx_add"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            let m = Motion::of(ui, &resp);
            material::accent_edge(ui.painter(), closed, radius, &p, m.hover * alpha);
            if m.hover > 0.5 {
                color = p.accent_text;
            }
            if resp.clicked() {
                open = true;
            }
        }
        ui.painter().text(
            closed.center(),
            Align2::CENTER_CENTER,
            format!("{}  Add effect", icon::PLUS),
            FontId::proportional(13.5),
            color.gamma_multiply(alpha),
        );
    }
    if t > 0.01 {
        let mut painter = ui.painter().clone();
        painter.set_opacity(t);
        material::raised(&painter, rect, radius, &Palette { raised: p.surface, ..p }, &Motion::REST);
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), HEAD_H));
        painter.text(head.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, "Add effect", FontId::new(13.5, theme::semibold()), p.text);
        let mut head_ui = ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(8.0, 4.0))).layout(Layout::right_to_left(Align::Center)));
        head_ui.set_opacity(t);
        if open && icon_button(&mut head_ui, icon::X, "Close").clicked() {
            open = false;
        }
        for (i, kind) in kinds.into_iter().enumerate() {
            let row = Rect::from_min_size(
                Pos2::new(rect.left() + 10.0, rect.top() + HEAD_H + i as f32 * (PICK_ROW + 2.0)),
                vec2(rect.width() - 20.0, PICK_ROW),
            );
            if row.bottom() > rect.bottom() {
                break; // still growing
            }
            let resp = ui.interact(row, Id::new(("fx_pick", i)), if open { Sense::click() } else { Sense::hover() });
            if open {
                resp.clone().on_hover_cursor(CursorIcon::PointingHand);
            }
            row_background(ui, row, &resp, false);
            ui.painter().text(row.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, kind.name(), FontId::proportional(13.0), p.text.gamma_multiply(t));
            if open && resp.clicked() {
                *op = Some(Op::Add(kind));
                open = false;
            }
        }
    }
    if open {
        let outside = ui.input(|i| i.pointer.any_pressed()) && !ui.ctx().pointer_interact_pos().is_some_and(|pt| rect.contains(pt));
        if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    ui.add_space(14.0);
}

fn params(ui: &mut egui::Ui, kind: &mut EffectKind) {
    let d = kind.defaults();
    match (kind, d) {
        (EffectKind::Bitcrusher { bits, downsample, mix }, EffectKind::Bitcrusher { bits: b0, downsample: s0, mix: m0 }) => {
            param(ui, "Bits", bits, BITS, b0, "", false);
            param(ui, "Downsample", downsample, DOWNSAMPLE, s0, "×", true);
            param(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (EffectKind::Distortion { kind, drive, tone, mix }, EffectKind::Distortion { drive: d0, tone: t0, mix: m0, .. }) => {
            let options: Vec<_> = DistortionKind::ALL.iter().map(|k| (*k, k.label())).collect();
            segmented(ui, kind, &options);
            ui.add_space(4.0);
            param(ui, "Drive", drive, DRIVE, d0, "×", true);
            param(ui, "Tone", tone, TONE, t0, " Hz", true);
            param(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Phaser { rate, depth, stages, feedback, mix },
            EffectKind::Phaser { rate: r0, depth: d0, feedback: f0, mix: m0, .. },
        ) => {
            param(ui, "Rate", rate, LFO_RATE, r0, " Hz", true);
            param(ui, "Depth", depth, UNIT, d0, "", false);
            param_num(ui, "Stages", stages, 2, 8, 4, "");
            param(ui, "Feedback", feedback, FEEDBACK, f0, "", false);
            param(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Flanger { rate, depth_ms, delay_ms, feedback, mix },
            EffectKind::Flanger { rate: r0, depth_ms: dp0, delay_ms: dl0, feedback: f0, mix: m0 },
        ) => {
            param(ui, "Rate", rate, LFO_RATE, r0, " Hz", true);
            param(ui, "Depth", depth_ms, FLANGER_DEPTH_MS, dp0, " ms", false);
            param(ui, "Delay", delay_ms, FLANGER_DELAY_MS, dl0, " ms", false);
            param(ui, "Feedback", feedback, FLANGER_FEEDBACK, f0, "", false);
            param(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Delay { time, feedback, damping, mix },
            EffectKind::Delay { time: t0, feedback: f0, damping: d0, mix: m0 },
        ) => {
            param(ui, "Time", time, DELAY_TIME, t0, " s", true);
            param(ui, "Feedback", feedback, FEEDBACK, f0, "", false);
            param(ui, "Damping", damping, UNIT, d0, "", false);
            param(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Reverb { size, decay, damping, predelay, mix },
            EffectKind::Reverb { size: s0, decay: d0, damping: dm0, predelay: p0, mix: m0 },
        ) => {
            param(ui, "Size", size, UNIT, s0, "", false);
            param(ui, "Decay", decay, UNIT, d0, "", false);
            param(ui, "Damping", damping, UNIT, dm0, "", false);
            param(ui, "Pre-delay", predelay, PREDELAY, p0, " s", false);
            param(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (
            EffectKind::Compressor { threshold_db, ratio, attack, release, makeup_db },
            EffectKind::Compressor { threshold_db: t0, ratio: r0, attack: a0, release: rl0, makeup_db: m0 },
        ) => {
            param(ui, "Threshold", threshold_db, THRESHOLD_DB, t0, " dB", false);
            param(ui, "Ratio", ratio, RATIO, r0, ":1", true);
            param(ui, "Attack", attack, COMP_ATTACK, a0, " s", true);
            param(ui, "Release", release, COMP_RELEASE, rl0, " s", true);
            param(ui, "Makeup", makeup_db, MAKEUP_DB, m0, " dB", false);
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
        apply(&mut e, Op::Duplicate(7), 99);
        assert_eq!(ids(&e), vec![1, 3]);
    }

    #[test]
    fn duplicate_goes_right_after_the_original_with_a_fresh_id() {
        let mut e = chain(3);
        apply(&mut e, Op::Duplicate(0), 10);
        assert_eq!(ids(&e), vec![1, 10, 2, 3]);
    }

    #[test]
    fn add_appends_enabled_effect_with_the_given_id() {
        let mut e = chain(1);
        apply(&mut e, Op::Add(EffectKind::all_defaults()[1]), 5);
        assert_eq!(ids(&e), vec![1, 5]);
        assert!(e[1].enabled);
    }
}
