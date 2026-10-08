use eframe::egui::{self, Align, CursorIcon, FontId, Id, Layout, RichText, Stroke};
use egui_phosphor::regular as icon;
use sfxc_core::patch::{ranges::*, DistortionKind, Effect, EffectKind};

use super::theme::{self, palette};
use super::widgets::{button, card_frame, empty_state, icon_button, param, param_num, segmented, toggle, Kind};

pub fn show(ui: &mut egui::Ui, effects: &mut Vec<Effect>, next_id: u64) {
    let p = palette(ui);
    let mut next_id = next_id;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Effects").text_style(egui::TextStyle::Heading).color(p.text));
        if effects.len() > 1 {
            ui.label(super::widgets::hint(ui, "Drag to reorder"));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let add = button(ui, Kind::Secondary, Some(icon::PLUS), "Add effect");
            add_menu(&add, effects, &mut next_id);
        });
    });
    ui.add_space(6.0);
    if effects.is_empty() {
        card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            empty_state(ui, icon::FADERS, "No effects", "Add reverb, delay or a bitcrusher to shape the sound.", |_| {});
        });
        return;
    }

    let mut remove = None;
    let mut duplicate = None;
    let mut moved: Option<(usize, usize)> = None;
    for (i, effect) in effects.iter_mut().enumerate() {
        let id = effect.id;
        let open_id = Id::new(("fx_open", id));
        let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
        let row = card_frame(ui).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.dnd_drag_source(Id::new(("fx_drag", id)), i, |ui| {
                    ui.label(RichText::new(icon::DOTS_SIX_VERTICAL).color(p.faint).size(16.0));
                })
                .response
                .on_hover_cursor(CursorIcon::Grab);
                toggle(ui, &mut effect.enabled, "").on_hover_text(if effect.enabled { "Bypass" } else { "Enable" });
                let color = if effect.enabled { p.text } else { p.faint };
                let title = ui.add(
                    egui::Label::new(RichText::new(effect.kind.name()).font(FontId::new(13.5, theme::semibold())).color(color))
                        .sense(egui::Sense::click()),
                );
                if title.clicked() {
                    open = !open;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, icon::TRASH, "Remove").clicked() {
                        remove = Some(i);
                    }
                    if icon_button(ui, icon::COPY, "Duplicate").clicked() {
                        duplicate = Some(i);
                    }
                    let caret = if open { icon::CARET_DOWN } else { icon::CARET_RIGHT };
                    if icon_button(ui, caret, if open { "Collapse" } else { "Expand" }).clicked() {
                        open = !open;
                    }
                });
            });
            if open {
                ui.add_space(6.0);
                ui.add_enabled_ui(effect.enabled, |ui| params(ui, &mut effect.kind));
            }
        });
        ui.data_mut(|d| d.insert_temp(open_id, open));
        let resp = &row.response;
        if resp.dnd_hover_payload::<usize>().is_some() {
            let stroke = Stroke::new(2.0, p.accent);
            ui.painter().hline(resp.rect.x_range(), resp.rect.top() - 4.0, stroke);
        }
        if let Some(from) = resp.dnd_release_payload::<usize>() {
            moved = Some((*from, i));
        }
        ui.add_space(8.0);
    }

    if let Some(i) = remove {
        effects.remove(i);
    } else if let Some(i) = duplicate {
        let mut copy = effects[i].clone();
        copy.id = next_id;
        effects.insert(i + 1, copy);
    } else if let Some((from, to)) = moved
        && from != to
        && from < effects.len()
    {
        let item = effects.remove(from);
        effects.insert(to.min(effects.len()), item);
    }
}

fn add_menu(button: &egui::Response, effects: &mut Vec<Effect>, next_id: &mut u64) {
    egui::Popup::menu(button).align(egui::RectAlign::BOTTOM_END).show(|ui| {
        ui.set_min_width(160.0);
        for kind in EffectKind::all_defaults() {
            if ui.button(kind.name()).clicked() {
                effects.push(Effect { id: *next_id, enabled: true, kind });
                *next_id += 1;
            }
        }
    });
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
