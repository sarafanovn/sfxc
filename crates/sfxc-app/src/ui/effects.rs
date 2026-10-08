//! Master effects chain: a horizontal strip of cards. Each card has a drag handle, folds up by its
//! title like the sound-settings cards, and shows its parameters as knobs.

use eframe::egui::{self, vec2, Align, Align2, CursorIcon, FontId, Id, Layout, Pos2, Rect, RichText, Sense};
use egui_phosphor::regular as icon;
use sfxc_core::patch::{ranges::*, DistortionKind, Effect, EffectKind};

use super::material::{self, Motion};
use super::order;
use super::theme::{self, palette, Palette, R_CARD};
use super::widgets::{drop_zone, grip_handle, icon_button, knob, knob_num, segmented, toggle, Axis, Grip};

const CARD_W: f32 = 212.0;
const CARD_H: f32 = 300.0;
const GAP: f32 = 12.0;
const HEAD_H: f32 = 36.0;
const PICK_ROW: f32 = 30.0;

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
    ui.add_space(6.0);

    let mut op = None;
    egui::ScrollArea::horizontal().id_salt("fx_chain").auto_shrink([false, true]).show(ui, |ui| {
        // Room around the cards for their soft shadows, which the scroll area would otherwise clip.
        ui.add_space(16.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.add_space(GAP);
            for (i, effect) in effects.iter_mut().enumerate() {
                if let Some((from, to)) = card(ui, effect, i, &mut op) {
                    op = Some(Op::Move(from, to));
                }
            }
            let (rect, _) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::hover());
            add_card(ui, rect, &mut op);
            ui.add_space(GAP);
        });
        ui.add_space(28.0);
    });

    if let Some(op) = op {
        apply(effects, op, next_id);
    }
}

/// One effect. Returns `(from, to)` when another card was dropped on it.
fn card(ui: &mut egui::Ui, effect: &mut Effect, i: usize, op: &mut Option<Op>) -> Option<(usize, usize)> {
    let p = palette(ui);
    let open_id = Id::new(("fx_open", effect.id));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
    let t = ui.ctx().animate_bool_with_time(open_id.with("t"), open, 0.15);
    let (rect, _) = ui.allocate_exact_size(vec2(CARD_W, egui::lerp(HEAD_H..=CARD_H, t)), Sense::hover());
    let head_rect = Rect::from_min_size(rect.min, vec2(CARD_W, HEAD_H));
    let radius = R_CARD as f32;

    if effect.enabled {
        material::raised(ui.painter(), rect, radius, &Palette { raised: p.surface, ..p }, &Motion::REST);
    } else {
        material::recessed(ui.painter(), rect, radius, &p, 0.5);
    }

    // Left of the header: drag handle, caret and title (clicking either folds the card).
    let mut left = ui.new_child(egui::UiBuilder::new().max_rect(head_rect.shrink2(vec2(10.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    left.spacing_mut().item_spacing.x = 4.0;
    grip_handle(&mut left, Grip { group: "effects", index: i }, &effect.id.to_string());
    let caret = if open { icon::CARET_DOWN } else { icon::CARET_RIGHT };
    let title_color = if effect.enabled { p.text } else { p.faint };
    let title = egui::Label::new(RichText::new(format!("{caret} {}", effect.kind.name())).font(FontId::new(13.5, theme::semibold())).color(title_color));
    if left.add(title.sense(Sense::click())).on_hover_cursor(CursorIcon::PointingHand).clicked() {
        open = !open;
    }

    // Right of the header: bypass switch and the overflow menu.
    let mut right = ui.new_child(egui::UiBuilder::new().max_rect(head_rect.shrink2(vec2(8.0, 0.0))).layout(Layout::right_to_left(Align::Center)));
    right.spacing_mut().item_spacing.x = 2.0;
    let more = icon_button(&mut right, icon::DOTS_THREE, "More");
    egui::Popup::menu(&more).show(|ui| {
        if ui.button(format!("{}  Duplicate", icon::COPY)).clicked() {
            *op = Some(Op::Duplicate(i));
        }
        if ui.button(RichText::new(format!("{}  Remove", icon::TRASH)).color(p.danger)).clicked() {
            *op = Some(Op::Remove(i));
        }
    });
    toggle(&mut right, &mut effect.enabled, "").on_hover_text(if effect.enabled { "Bypass" } else { "Enable" });

    if t > 0.02 {
        let body = Rect::from_min_max(rect.min + vec2(10.0, HEAD_H), Pos2::new(rect.right() - 10.0, rect.min.y + CARD_H - 10.0));
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::left_to_right(Align::TOP).with_main_wrap(true)));
        body_ui.set_clip_rect(rect.intersect(body_ui.clip_rect()));
        body_ui.set_opacity(t);
        body_ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
        body_ui.add_enabled_ui(effect.enabled, |ui| params(ui, &mut effect.kind));
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));

    let zone = Rect::from_min_size(rect.min, vec2(CARD_W, CARD_H));
    drop_zone(ui, zone, Grip { group: "effects", index: i }, Axis::Horizontal, GAP / 2.0 + ui.spacing().item_spacing.x / 2.0)
}

/// The trailing "+" card. Clicking it turns the card into the list of effects; picking one, Esc,
/// the close button or a click elsewhere turns it back.
fn add_card(ui: &mut egui::Ui, rect: Rect, op: &mut Option<Op>) {
    let p = palette(ui);
    let open_id = Id::new("fx_add_open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let t = ui.ctx().animate_bool_with_time(open_id.with("t"), open, 0.18);
    let radius = R_CARD as f32;

    // Closed look fades out as the list fades in.
    material::recessed(ui.painter(), rect, radius, &p, 0.5 * (1.0 - t));
    if t < 0.99 {
        let closed_alpha = 1.0 - t;
        if !open {
            let resp = ui.interact(rect, Id::new("fx_add"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Add effect");
            let m = Motion::of(ui, &resp);
            material::accent_edge(ui.painter(), rect, radius, &p, m.hover * closed_alpha);
            if resp.clicked() {
                open = true;
            }
            let c = if m.hover > 0.5 { p.accent_text } else { p.muted };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(28.0), c.gamma_multiply(closed_alpha));
        } else {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(28.0), p.muted.gamma_multiply(closed_alpha));
        }
    }
    if t > 0.01 {
        let mut painter = ui.painter().clone();
        painter.set_opacity(t);
        material::raised(&painter, rect, radius, &Palette { raised: p.surface, ..p }, &Motion::REST);
        let head_rect = Rect::from_min_size(rect.min, vec2(CARD_W, HEAD_H));
        painter.text(head_rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, "Add effect", FontId::new(13.5, theme::semibold()), p.text);
        let mut head = ui.new_child(egui::UiBuilder::new().max_rect(head_rect.shrink2(vec2(8.0, 4.0))).layout(Layout::right_to_left(Align::Center)));
        head.set_opacity(t);
        if open && icon_button(&mut head, icon::X, "Close").clicked() {
            open = false;
        }
        for (i, kind) in EffectKind::all_defaults().into_iter().enumerate() {
            let row = Rect::from_min_size(
                Pos2::new(rect.left() + 10.0, rect.top() + HEAD_H + i as f32 * (PICK_ROW + 2.0)),
                vec2(CARD_W - 20.0, PICK_ROW),
            );
            let resp = ui.interact(row, Id::new(("fx_pick", i)), if open { Sense::click() } else { Sense::hover() });
            if open {
                resp.clone().on_hover_cursor(CursorIcon::PointingHand);
            }
            let mut row_ui = ui.new_child(egui::UiBuilder::new().max_rect(row));
            row_ui.set_opacity(t);
            super::widgets::row_background(&row_ui, row, &resp, false);
            row_ui.painter().text(row.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, kind.name(), FontId::proportional(13.0), p.text);
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
