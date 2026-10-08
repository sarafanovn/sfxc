use eframe::egui;
use sfxc_core::generators::Category;
use sfxc_core::mode::{describe_mapping, map_source_to_mode, NES_DUTIES};
use sfxc_core::patch::*;

use super::widgets::{param, waveform};
use super::{effects, Action, Current};

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    cur: &mut Current,
    rendered: Option<(&[f32], u32)>,
    autoplay: &mut bool,
    mode_note: &mut Option<String>,
    audio_error: Option<&str>,
    actions: &mut Vec<Action>,
) {
    toolbar(ui, cur, autoplay, mode_note, actions);
    if let Some(err) = audio_error {
        ui.colored_label(ui.visuals().warn_fg_color, format!("{err}. Editing and export still work."));
    }
    ui.horizontal_wrapped(|ui| {
        for c in Category::ALL {
            if ui.button(c.label()).clicked() {
                actions.push(Action::Generate(c));
            }
        }
        ui.separator();
        if ui.button("Mutate").on_hover_text("M").clicked() {
            actions.push(Action::Mutate);
        }
    });
    ui.add_space(4.0);
    waveform(ui, rendered);
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let mode = cur.patch.mode;
        let layer = &mut cur.patch.layers[0];
        ui.columns(2, |cols| {
            source_panel(&mut cols[0], layer, mode);
            pitch_panel(&mut cols[0], layer);
            envelope_panel(&mut cols[1], layer);
            filter_panel(&mut cols[1], layer);
        });
        ui.separator();
        let next_id = cur.patch.next_effect_id();
        effects::show(ui, &mut cur.patch.master_effects, next_id);
    });
}

fn toolbar(ui: &mut egui::Ui, cur: &mut Current, autoplay: &mut bool, mode_note: &mut Option<String>, actions: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let name = ui.add(egui::TextEdit::singleline(&mut cur.name).desired_width(180.0).font(egui::TextStyle::Heading));
        if name.lost_focus() {
            actions.push(Action::Rename(cur.name.clone()));
        }
        let tags = ui.add(egui::TextEdit::singleline(&mut cur.tags).desired_width(140.0).hint_text("tags"));
        if tags.lost_focus() {
            actions.push(Action::SetTags(cur.tags.clone()));
        }
        ui.separator();
        let before = cur.patch.mode;
        for m in Mode::ALL {
            ui.selectable_value(&mut cur.patch.mode, m, m.label());
        }
        if cur.patch.mode != before {
            apply_mode(&mut cur.patch, mode_note);
        }
        ui.separator();
        if ui.button("▶ Play").on_hover_text("Space").clicked() {
            actions.push(Action::Play);
        }
        ui.checkbox(autoplay, "Auto-play");
        param(ui, "Volume", &mut cur.patch.master_volume, ranges::UNIT, 0.8, "", false);
        if ui.button("Export…").on_hover_text("⌘E").clicked() {
            actions.push(Action::Export);
        }
    });
    let mut dismiss = false;
    if let Some(note) = mode_note.as_ref() {
        ui.horizontal(|ui| {
            ui.weak(note.as_str());
            dismiss = ui.small_button("✕").clicked();
        });
    }
    if dismiss {
        *mode_note = None;
    }
}

fn apply_mode(patch: &mut SoundPatch, note: &mut Option<String>) {
    *note = None;
    let mode = patch.mode;
    for l in &mut patch.layers {
        let mapped = map_source_to_mode(&l.source, mode);
        if let Some(text) = describe_mapping(&l.source, &mapped) {
            *note = Some(text);
        }
        l.source = mapped;
    }
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.strong(title);
        add(ui);
    });
    ui.add_space(4.0);
}

fn source_panel(ui: &mut egui::Ui, layer: &mut Layer, mode: Mode) {
    section(ui, "Source", |ui| {
        let kinds: &[&'static str] = if mode == Mode::Bit8 { &["Pulse", "Triangle", "Noise"] } else { &Source::KIND_NAMES };
        let mut kind = layer.source.kind_name();
        egui::ComboBox::from_id_salt("source_kind").selected_text(kind).show_ui(ui, |ui| {
            for k in kinds {
                ui.selectable_value(&mut kind, *k, *k);
            }
        });
        if kind != layer.source.kind_name() {
            layer.source = map_source_to_mode(&Source::default_of_kind(kind), mode);
        }
        match &mut layer.source {
            Source::Pulse { duty } if mode == Mode::Bit8 => {
                ui.horizontal(|ui| {
                    ui.label("Duty");
                    for d in NES_DUTIES {
                        ui.selectable_value(duty, d, format!("{}%", d * 100.0));
                    }
                });
            }
            Source::Pulse { duty } => {
                param(ui, "Duty", duty, ranges::DUTY, 0.5, "", false);
            }
            Source::Noise { kind } => {
                let kinds: &[NoiseKind] =
                    if mode == Mode::Bit8 { &[NoiseKind::LfsrLong, NoiseKind::LfsrShort] } else { &NoiseKind::ALL };
                ui.horizontal(|ui| {
                    for k in kinds {
                        ui.selectable_value(kind, *k, k.label());
                    }
                });
            }
            Source::Fm { algorithm, feedback, ops } => fm_controls(ui, algorithm, feedback, ops),
            _ => {}
        }
    });
}

fn fm_controls(ui: &mut egui::Ui, algorithm: &mut FmAlgorithm, feedback: &mut f32, ops: &mut [FmOperator; 4]) {
    egui::ComboBox::from_id_salt("fm_algorithm").selected_text(algorithm.label()).show_ui(ui, |ui| {
        for a in FmAlgorithm::ALL {
            ui.selectable_value(algorithm, a, a.label());
        }
    });
    param(ui, "Feedback", feedback, ranges::UNIT, 0.0, "", false);
    let d = FmOperator::default();
    for (i, op) in ops.iter_mut().enumerate() {
        egui::CollapsingHeader::new(format!("Operator {}", i + 1))
            .id_salt(("fm_op", i))
            .default_open(i == 3)
            .show(ui, |ui| {
                param(ui, "Ratio", &mut op.ratio, ranges::FM_RATIO, d.ratio, "×", true);
                param(ui, "Detune", &mut op.detune, ranges::FM_DETUNE, d.detune, " ct", false);
                param(ui, "Level", &mut op.level, ranges::UNIT, d.level, "", false);
                param(ui, "Attack", &mut op.attack, ranges::FM_TIME, d.attack, " s", true);
                param(ui, "Decay", &mut op.decay, ranges::FM_TIME, d.decay, " s", true);
                param(ui, "Sustain", &mut op.sustain, ranges::UNIT, d.sustain, "", false);
            });
    }
}

fn pitch_panel(ui: &mut egui::Ui, layer: &mut Layer) {
    let d = Pitch::default();
    section(ui, "Pitch", |ui| {
        let p = &mut layer.pitch;
        param(ui, "Frequency", &mut p.base_freq, ranges::BASE_FREQ, d.base_freq, " Hz", true);
        param(ui, "Slide", &mut p.slide, ranges::SLIDE, 0.0, " oct/s", false);
        param(ui, "Slide accel", &mut p.delta_slide, ranges::DELTA_SLIDE, 0.0, " oct/s²", false);
        param(ui, "Vibrato depth", &mut p.vibrato_depth, ranges::VIBRATO_DEPTH, 0.0, " st", false);
        param(ui, "Vibrato rate", &mut p.vibrato_rate, ranges::VIBRATO_RATE, d.vibrato_rate, " Hz", false);
        ui.horizontal_wrapped(|ui| {
            ui.label("Arpeggio");
            for s in p.arp_steps.iter_mut() {
                ui.add(egui::DragValue::new(s).range(-24..=24).suffix(" st"));
            }
            if p.arp_steps.len() < MAX_ARP_STEPS && ui.small_button("+").clicked() {
                p.arp_steps.push(0);
            }
            if !p.arp_steps.is_empty() && ui.small_button("−").clicked() {
                p.arp_steps.pop();
            }
        });
        if !p.arp_steps.is_empty() {
            param(ui, "Arp step", &mut p.arp_speed, ranges::ARP_SPEED, d.arp_speed, " s", true);
        }
    });
}

fn envelope_panel(ui: &mut egui::Ui, layer: &mut Layer) {
    let d = Envelope::default();
    section(ui, "Envelope", |ui| {
        let e = &mut layer.env;
        param(ui, "Attack", &mut e.attack, ranges::ENV_TIME, d.attack, " s", true);
        param(ui, "Decay", &mut e.decay, ranges::ENV_TIME, d.decay, " s", true);
        param(ui, "Sustain level", &mut e.sustain_level, ranges::UNIT, d.sustain_level, "", false);
        param(ui, "Sustain time", &mut e.sustain_time, ranges::ENV_TIME, d.sustain_time, " s", true);
        param(ui, "Release", &mut e.release, ranges::ENV_TIME, d.release, " s", true);
        param(ui, "Punch", &mut e.punch, ranges::UNIT, d.punch, "", false);
    });
}

fn filter_panel(ui: &mut egui::Ui, layer: &mut Layer) {
    let d = Filter::default();
    section(ui, "Filter", |ui| {
        let f = &mut layer.filter;
        ui.horizontal(|ui| {
            for k in FilterKind::ALL {
                ui.selectable_value(&mut f.kind, k, k.label());
            }
        });
        ui.add_enabled_ui(f.kind != FilterKind::Off, |ui| {
            param(ui, "Cutoff", &mut f.cutoff, ranges::CUTOFF, d.cutoff, " Hz", true);
            param(ui, "Resonance", &mut f.resonance, ranges::UNIT, d.resonance, "", false);
            param(ui, "Sweep", &mut f.sweep, ranges::SWEEP, d.sweep, " oct/s", false);
        });
    });
}
