use eframe::egui::{self, vec2, Align, Align2, FontId, Frame, Id, Layout, Margin, Rect, RichText, Sense, Ui};
use egui_phosphor::regular as icon;
use sfxc_core::generators::Category;
use sfxc_core::mode::{describe_mapping, map_source_to_mode, NES_DUTIES};
use sfxc_core::patch::*;

use super::theme::{self, palette};
use super::widgets::{
    banner, button, icon_button, material_card, material_card_response, param, play_button, row_label, section, segmented, select, toggle,
    waveform, Grip, Kind, Tone,
};
use super::{arp, controls, effects, order, Action, Current};

/// Sound-settings cards, in their default order. The user can drag them into another order.
pub const SETTINGS: [&str; 5] = ["source", "pitch", "arp", "envelope", "filter"];
pub const SETTINGS_ORDER: &str = "settings_order";

/// Read-only state the editor shows but does not own.
pub struct View<'a> {
    pub rendered: Option<(&'a [f32], u32)>,
    pub progress: Option<f32>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub audio_error: Option<&'a str>,
}

/// Widest the editor content gets; wider windows add side margins.
const MAX_CONTENT: f32 = 720.0;

pub fn show(
    ui: &mut Ui,
    cur: &mut Current,
    view: &View,
    autoplay: &mut bool,
    volume: &mut f32,
    mode_note: &mut Option<String>,
    actions: &mut Vec<Action>,
) {
    let panel = ui.max_rect();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let side = ((ui.available_width() - MAX_CONTENT) / 2.0).clamp(24.0, 127.0) as i8;
        Frame::new().inner_margin(Margin { left: side, right: side, top: 16, bottom: 96 }).show(ui, |ui| {
            ui.set_max_width(MAX_CONTENT);
            header(ui, cur, view, actions);
            ui.add_space(14.0);
            if let Some(err) = view.audio_error {
                banner(ui, Tone::Warn, icon::SPEAKER_SLASH, &format!("{err}. Editing and export still work."), false);
                ui.add_space(10.0);
            }
            transport(ui, cur, view, autoplay, mode_note, actions);
            ui.add_space(12.0);
            generators(ui, actions);
            ui.add_space(16.0);

            let order_id = Id::new(SETTINGS_ORDER);
            let mut sections: Vec<String> = ui.data(|d| d.get_temp(order_id)).unwrap_or_else(|| order::restore("", &SETTINGS));
            let mode = cur.patch.mode;
            let layer = &mut cur.patch.layers[0];
            let mut moved = None;
            for (i, key) in sections.iter().enumerate() {
                let grip = Grip { group: "settings", index: i };
                let drop = match key.as_str() {
                    "source" => source_section(ui, layer, mode, grip),
                    "pitch" => pitch_section(ui, layer, grip),
                    "arp" => arp_section(ui, layer, view, grip),
                    "envelope" => envelope_section(ui, layer, grip),
                    _ => filter_section(ui, layer, grip),
                };
                moved = moved.or(drop);
            }
            if let Some((from, to)) = moved {
                order::move_item(&mut sections, from, to);
                actions.push(Action::SaveSectionOrder(order::save(&sections)));
            }
            ui.data_mut(|d| d.insert_temp(order_id, sections));

            ui.add_space(8.0);
            let next_id = cur.patch.next_effect_id();
            effects::show(ui, &mut cur.patch.master_effects, next_id);
        });
    });
    dock(ui, panel, volume, actions);
}

/// Bottom-right corner: a small speaker that grows into the playback Volume slider while the pointer
/// is over it.
fn dock(ui: &mut Ui, panel: Rect, volume: &mut f32, actions: &mut Vec<Action>) {
    const OPEN_W: f32 = 270.0;
    const ROW_H: f32 = 30.0;
    let p = palette(ui);
    let id = Id::new("dock");
    let last: Option<(Rect, bool)> = ui.data(|d| d.get_temp(id.with("state")));
    let pointer = ui.ctx().pointer_hover_pos();
    let dragging = ui.input(|i| i.pointer.any_down());
    // Stay open while a slider is being dragged even if the pointer leaves the card.
    let open = last.is_some_and(|(rect, was_open)| pointer.is_some_and(|pt| rect.expand(10.0).contains(pt)) || (was_open && dragging));
    let t = ui.ctx().animate_bool_with_time(id.with("t"), open, 0.18);
    let rows_h = ROW_H + 8.0;
    let w = egui::lerp(36.0..=OPEN_W, t);

    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(panel.right_bottom() - vec2(28.0, 24.0))
        .show(ui.ctx(), |ui| {
            material_card_response(ui, |ui| {
                ui.set_width(w);
                if t > 0.02 {
                    let (r, _) = ui.allocate_exact_size(vec2(w, rows_h * t), Sense::hover());
                    let mut rows = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(r.min, vec2(w, rows_h))).layout(Layout::top_down(Align::LEFT)));
                    rows.set_clip_rect(r.intersect(rows.clip_rect()));
                    rows.set_opacity(t);
                    dock_row(&mut rows, "Volume", w, |ui, width| {
                        let mut pos = crate::audio::slider_from_volume(*volume);
                        let default = crate::audio::slider_from_volume(crate::audio::DEFAULT_VOLUME);
                        let r = controls::track(ui, &mut pos, 0.0, 1.0, default, false, width);
                        *volume = crate::audio::volume_from_slider(pos);
                        if r.drag_stopped() || (r.changed() && !r.dragged()) {
                            actions.push(Action::SaveVolume);
                        }
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(icon::SPEAKER_HIGH).color(p.muted).size(18.0));
                });
            })
        });
    ui.data_mut(|d| d.insert_temp(id.with("state"), (area.inner.response.rect, open)));
}

fn dock_row(ui: &mut Ui, label: &str, total: f32, add: impl FnOnce(&mut Ui, f32)) {
    const LABEL_W: f32 = 56.0;
    ui.horizontal(|ui| {
        ui.set_height(30.0);
        ui.label(RichText::new(label).size(12.5).color(palette(ui).muted));
        ui.add_space((LABEL_W - 40.0).max(0.0));
        add(ui, (total - LABEL_W - ui.spacing().item_spacing.x * 2.0).max(60.0));
    });
}

fn header(ui: &mut Ui, cur: &mut Current, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if button(ui, Kind::Secondary, Some(icon::EXPORT), "Export").on_hover_text("⌘E").clicked() {
                actions.push(Action::Export);
            }
            ui.add_space(6.0);
            ui.add_enabled_ui(view.can_redo, |ui| {
                if icon_button(ui, icon::ARROW_U_UP_RIGHT, "Redo (⇧⌘Z)").clicked() {
                    actions.push(Action::Redo);
                }
            });
            ui.add_enabled_ui(view.can_undo, |ui| {
                if icon_button(ui, icon::ARROW_U_UP_LEFT, "Undo (⌘Z)").clicked() {
                    actions.push(Action::Undo);
                }
            });
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let name = ui.add(
                    egui::TextEdit::singleline(&mut cur.name)
                        .font(theme::title_style())
                        .text_color(p.text)
                        .frame(Frame::new().inner_margin(Margin::symmetric(4, 2)))
                        .desired_width(ui.available_width() - 12.0),
                );
                if name.lost_focus() {
                    actions.push(Action::RenameSound(cur.id, cur.name.clone()));
                }
            });
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(icon::TAG).color(p.faint));
        let tags = ui.add(
            egui::TextEdit::singleline(&mut cur.tags)
                .hint_text(RichText::new("Add tags").color(p.faint))
                .text_color(p.muted)
                .frame(Frame::NONE)
                .desired_width(ui.available_width()),
        );
        if tags.lost_focus() {
            actions.push(Action::SetTags(cur.tags.clone()));
        }
    });
}

fn transport(ui: &mut Ui, cur: &mut Current, view: &View, autoplay: &mut bool, mode_note: &mut Option<String>, actions: &mut Vec<Action>) {
    material_card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_space(22.0);
                if play_button(ui, view.progress.is_some()).clicked() {
                    actions.push(Action::Play);
                }
            });
            ui.add_space(6.0);
            waveform(ui, view.rendered, view.progress, 92.0);
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let before = cur.patch.mode;
            let modes: Vec<_> = Mode::ALL.iter().map(|m| (*m, m.label())).collect();
            segmented(ui, &mut cur.patch.mode, &modes);
            if cur.patch.mode != before {
                apply_mode(&mut cur.patch, mode_note);
            }
            ui.add_space(12.0);
            toggle(ui, autoplay, "Auto-play").on_hover_text("Play after every change");
            ui.add_space(4.0);
            if button(ui, Kind::Secondary, Some(icon::MAGIC_WAND), "Mutate").on_hover_text("Small random changes (M)").clicked() {
                actions.push(Action::Mutate);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Gain").size(12.5).color(palette(ui).muted));
            let default = SoundPatch::default().master_volume;
            controls::track(ui, &mut cur.patch.master_volume, ranges::UNIT.0, ranges::UNIT.1, default, false, 110.0)
                .on_hover_text("Level of this sound, saved with it");
        });
    });
    if let Some(note) = mode_note.as_ref() {
        ui.add_space(8.0);
        if banner(ui, Tone::Accent, icon::INFO, note, true) {
            *mode_note = None;
        }
    }
}

fn category_icon(c: Category) -> &'static str {
    match c {
        Category::PickupCoin => icon::COIN,
        Category::LaserShoot => icon::CROSSHAIR,
        Category::Explosion => icon::FIRE,
        Category::PowerUp => icon::ARROW_FAT_UP,
        Category::HitHurt => icon::HEART_BREAK,
        Category::Jump => icon::ARROW_LINE_UP,
        Category::BlipSelect => icon::CURSOR_CLICK,
        Category::Random => icon::DICE_FIVE,
    }
}

fn generators(ui: &mut Ui, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for c in Category::ALL {
            if button(ui, Kind::Secondary, Some(category_icon(c)), c.label())
                .on_hover_text(format!("Generate a new {} sound", c.label().to_lowercase()))
                .clicked()
            {
                actions.push(Action::Generate(c));
            }
        }
    });
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

/// Muted label column matching [`param`] rows, followed by `add`.
fn labeled(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        row_label(ui, label);
        add(ui);
    });
}

fn source_section(ui: &mut Ui, layer: &mut Layer, mode: Mode, grip: Grip) -> Option<(usize, usize)> {
    section(ui, "source", "Source", Some(grip), |_| {}, |ui| {
        let kinds: &[&'static str] = if mode == Mode::Bit8 { &["Pulse", "Triangle", "Noise"] } else { &Source::KIND_NAMES };
        let mut kind = layer.source.kind_name();
        labeled(ui, "Waveform", |ui| {
            select(ui, "source_kind", kind, |ui| {
                for k in kinds {
                    ui.selectable_value(&mut kind, *k, *k);
                }
            });
        });
        if kind != layer.source.kind_name() {
            layer.source = map_source_to_mode(&Source::default_of_kind(kind), mode);
        }
        match &mut layer.source {
            Source::Pulse { duty } if mode == Mode::Bit8 => {
                let labels: Vec<String> = NES_DUTIES.iter().map(|d| format!("{}%", d * 100.0)).collect();
                let options: Vec<_> = NES_DUTIES.iter().zip(&labels).map(|(d, l)| (*d, l.as_str())).collect();
                labeled(ui, "Duty", |ui| {
                    segmented(ui, duty, &options);
                });
            }
            Source::Pulse { duty } => {
                param(ui, "Duty", duty, ranges::DUTY, 0.5, "", false);
            }
            Source::Noise { kind } => {
                let kinds: &[NoiseKind] =
                    if mode == Mode::Bit8 { &[NoiseKind::LfsrLong, NoiseKind::LfsrShort] } else { &NoiseKind::ALL };
                let options: Vec<_> = kinds.iter().map(|k| (*k, k.label())).collect();
                labeled(ui, "Noise", |ui| {
                    segmented(ui, kind, &options);
                });
            }
            Source::Fm { algorithm, feedback, ops } => fm_controls(ui, algorithm, feedback, ops),
            _ => {}
        }
    })
}

fn fm_controls(ui: &mut Ui, algorithm: &mut FmAlgorithm, feedback: &mut f32, ops: &mut [FmOperator; 4]) {
    labeled(ui, "Algorithm", |ui| {
        select(ui, "fm_algorithm", algorithm.label(), |ui| {
            for a in FmAlgorithm::ALL {
                ui.selectable_value(algorithm, a, a.label());
            }
        });
    });
    param(ui, "Feedback", feedback, ranges::UNIT, 0.0, "", false);
    let d = FmOperator::default();
    for (i, op) in ops.iter_mut().enumerate() {
        ui.add_space(2.0);
        egui::CollapsingHeader::new(RichText::new(format!("Operator {}", i + 1)).font(FontId::new(13.0, theme::semibold())))
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

fn pitch_section(ui: &mut Ui, layer: &mut Layer, grip: Grip) -> Option<(usize, usize)> {
    let d = Pitch::default();
    section(ui, "pitch", "Pitch", Some(grip), |_| {}, |ui| {
        let p = &mut layer.pitch;
        param(ui, "Frequency", &mut p.base_freq, ranges::BASE_FREQ, d.base_freq, " Hz", true);
        param(ui, "Slide", &mut p.slide, ranges::SLIDE, 0.0, " oct/s", false);
        param(ui, "Slide accel", &mut p.delta_slide, ranges::DELTA_SLIDE, 0.0, " oct/s²", false);
        param(ui, "Vibrato depth", &mut p.vibrato_depth, ranges::VIBRATO_DEPTH, 0.0, " st", false);
        param(ui, "Vibrato rate", &mut p.vibrato_rate, ranges::VIBRATO_RATE, d.vibrato_rate, " Hz", false);
    })
}

fn arp_section(ui: &mut Ui, layer: &mut Layer, view: &View, grip: Grip) -> Option<(usize, usize)> {
    let d = Pitch::default();
    let sound_secs = sfxc_core::env::length(&layer.env);
    let pitch = &mut layer.pitch;
    let mut enabled = pitch.arp_enabled;
    // The header toggle owns `enabled` while the section draws; the body reads this copy.
    let body_enabled = enabled;
    let moved = section(ui, "arp", "Arpeggio", Some(grip), |ui| {
        toggle(ui, &mut enabled, "").on_hover_text(if body_enabled { "Turn arpeggio off" } else { "Turn arpeggio on" });
    }, |ui| {
        ui.add_enabled_ui(body_enabled, |ui| {
            let rendered_secs = view.rendered.map_or(sound_secs, |(s, sr)| s.len() as f32 / sr as f32);
            let playing = view.progress.and_then(|t| arp::playing_step(t, rendered_secs, pitch.arp_speed, pitch.arp_steps.len()));
            ui.horizontal(|ui| {
                if !pitch.arp_steps.is_empty() {
                    arp::bars(ui, &mut pitch.arp_steps, playing);
                }
                ui.vertical(|ui| {
                    if pitch.arp_steps.len() < MAX_ARP_STEPS && icon_button(ui, icon::PLUS, "Add step").clicked() {
                        arp::add_step(&mut pitch.arp_steps);
                    }
                    if !pitch.arp_steps.is_empty() && icon_button(ui, icon::MINUS, "Remove last step").clicked() {
                        pitch.arp_steps.pop();
                    }
                });
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (name, steps) in arp::PRESETS {
                    if button(ui, Kind::Ghost, None, name).clicked() {
                        pitch.arp_steps = steps.to_vec();
                    }
                }
            });
            if !pitch.arp_steps.is_empty() {
                ui.add_space(4.0);
                param(ui, "Step", &mut pitch.arp_speed, ranges::ARP_SPEED, d.arp_speed, " s", true);
                let fit = arp::steps_that_fit(sound_secs, pitch.arp_speed);
                // One line either way, so the section keeps its height while Step is dragged.
                if fit < 2 {
                    let warn = palette(ui).warn;
                    ui.label(RichText::new(format!("{}  Only the first step plays: shorten Step or lengthen the envelope.", icon::WARNING)).color(warn).size(11.5));
                } else {
                    ui.label(super::widgets::hint(ui, format!("{fit} steps fit into the sound")));
                }
            }
        });
    });
    pitch.arp_enabled = enabled;
    moved
}

fn envelope_section(ui: &mut Ui, layer: &mut Layer, grip: Grip) -> Option<(usize, usize)> {
    let d = Envelope::default();
    section(ui, "envelope", "Envelope", Some(grip), |_| {}, |ui| {
        let e = &mut layer.env;
        param(ui, "Attack", &mut e.attack, ranges::ENV_TIME, d.attack, " s", true);
        param(ui, "Decay", &mut e.decay, ranges::ENV_TIME, d.decay, " s", true);
        param(ui, "Sustain level", &mut e.sustain_level, ranges::UNIT, d.sustain_level, "", false);
        param(ui, "Sustain time", &mut e.sustain_time, ranges::ENV_TIME, d.sustain_time, " s", true);
        param(ui, "Release", &mut e.release, ranges::ENV_TIME, d.release, " s", true);
        param(ui, "Punch", &mut e.punch, ranges::UNIT, d.punch, "", false);
    })
}

fn filter_section(ui: &mut Ui, layer: &mut Layer, grip: Grip) -> Option<(usize, usize)> {
    let d = Filter::default();
    let f = &mut layer.filter;
    section(ui, "filter", "Filter", Some(grip), |_| {}, |ui| {
        let options: Vec<_> = FilterKind::ALL.iter().map(|k| (*k, k.label())).collect();
        segmented(ui, &mut f.kind, &options);
        ui.add_space(4.0);
        ui.add_enabled_ui(f.kind != FilterKind::Off, |ui| {
            param(ui, "Cutoff", &mut f.cutoff, ranges::CUTOFF, d.cutoff, " Hz", true);
            param(ui, "Resonance", &mut f.resonance, ranges::UNIT, d.resonance, "", false);
            param(ui, "Sweep", &mut f.sweep, ranges::SWEEP, d.sweep, " oct/s", false);
        });
    })
}
