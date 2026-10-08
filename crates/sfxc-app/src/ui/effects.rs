use eframe::egui;
use sfxc_core::patch::{ranges::*, DistortionKind, Effect, EffectKind};

use super::widgets::param;

pub fn show(ui: &mut egui::Ui, effects: &mut Vec<Effect>, next_id: u64) {
    let mut next_id = next_id;
    ui.horizontal(|ui| {
        ui.strong("Effects");
        ui.weak("drag ☰ to reorder");
        ui.menu_button("+ Add", |ui| {
            for kind in EffectKind::all_defaults() {
                if ui.button(kind.name()).clicked() {
                    effects.push(Effect { id: next_id, enabled: true, kind });
                    next_id += 1;
                    ui.close();
                }
            }
        });
    });
    if effects.is_empty() {
        ui.weak("No effects. Add one with + Add.");
    }

    let mut remove = None;
    let mut duplicate = None;
    let mut moved: Option<(usize, usize)> = None;
    for (i, effect) in effects.iter_mut().enumerate() {
        let id = effect.id;
        let row = egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), egui::Id::new(("fx", id)), true)
                .show_header(ui, |ui| {
                    ui.dnd_drag_source(egui::Id::new(("fx_drag", id)), i, |ui| {
                        ui.label("☰");
                    });
                    ui.checkbox(&mut effect.enabled, effect.kind.name());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("Remove").clicked() {
                            remove = Some(i);
                        }
                        if ui.small_button("Duplicate").clicked() {
                            duplicate = Some(i);
                        }
                    });
                })
                .body(|ui| {
                    ui.add_enabled_ui(effect.enabled, |ui| params(ui, &mut effect.kind));
                });
        });
        let resp = &row.response;
        if resp.dnd_hover_payload::<usize>().is_some() {
            let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
            ui.painter().hline(resp.rect.x_range(), resp.rect.top(), stroke);
        }
        if let Some(from) = resp.dnd_release_payload::<usize>() {
            moved = Some((*from, i));
        }
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

fn params(ui: &mut egui::Ui, kind: &mut EffectKind) {
    let d = kind.defaults();
    match (kind, d) {
        (EffectKind::Bitcrusher { bits, downsample, mix }, EffectKind::Bitcrusher { bits: b0, downsample: s0, mix: m0 }) => {
            param(ui, "Bits", bits, BITS, b0, "", false);
            param(ui, "Downsample", downsample, DOWNSAMPLE, s0, "×", true);
            param(ui, "Mix", mix, UNIT, m0, "", false);
        }
        (EffectKind::Distortion { kind, drive, tone, mix }, EffectKind::Distortion { drive: d0, tone: t0, mix: m0, .. }) => {
            ui.horizontal(|ui| {
                for k in DistortionKind::ALL {
                    ui.selectable_value(kind, k, k.label());
                }
            });
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
            ui.add(egui::Slider::new(stages, 2..=8).text("Stages"));
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
