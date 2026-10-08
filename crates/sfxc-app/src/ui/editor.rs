use eframe::egui::{self, Align, CursorIcon, FontId, Frame, Id, Layout, Margin, Pos2, Rect, RichText, Sense, Ui, Vec2};
use egui_phosphor::regular as icon;
use sfxc_core::generators::Category;
use sfxc_core::mode::{describe_mapping, map_source_to_mode, NES_DUTIES};
use sfxc_core::patch::*;

use super::theme::{self, palette};
use super::widgets::{
    banner, button, button_min_width, button_width, dropdown, field, hint, icon_button, material_card, param, play_button, row_label, section,
    select, toggle, waveform, Grip, Kind, Tone,
};
use super::export_dialog::{self, ExportDialog};
use super::{arp, controls, effects, eq_graph, order, Action, Current};

/// Sound-settings cards, in their default order. The user can drag them into another order.
pub const SETTINGS: [&str; 5] = ["generate", "pitch", "envelope", "eq", "effects"];
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
    export: &mut ExportDialog,
    mode_note: &mut Option<String>,
    actions: &mut Vec<Action>,
) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let side = ((ui.available_width() - MAX_CONTENT) / 2.0).clamp(24.0, 127.0) as i8;
        Frame::new().inner_margin(Margin { left: side, right: side, top: 16, bottom: 96 }).show(ui, |ui| {
            ui.set_max_width(MAX_CONTENT);
            header(ui, cur, view, export, actions);
            ui.add_space(14.0);
            if let Some(err) = view.audio_error {
                banner(ui, Tone::Warn, icon::SPEAKER_SLASH, &format!("{err}. Editing and export still work."), false);
                ui.add_space(10.0);
            }
            transport(ui, cur, view, export, mode_note, actions);
            ui.add_space(12.0);

            let order_id = Id::new(SETTINGS_ORDER);
            let mut sections: Vec<String> = ui.data(|d| d.get_temp(order_id)).unwrap_or_else(|| order::restore("", &SETTINGS));
            let next_id = cur.patch.next_effect_id();
            let mut moved = None;
            for (i, key) in sections.iter().enumerate() {
                let grip = Grip { group: "settings", index: i };
                let drop = match key.as_str() {
                    "generate" => generators(ui, grip, actions),
                    "pitch" => pitch_section(ui, &mut cur.patch.layers[0], grip),
                    "envelope" => envelope_section(ui, &mut cur.patch.layers[0], grip),
                    "effects" => effects::show(ui, &mut cur.patch.master_effects, next_id, grip),
                    _ => eq_section(ui, &mut cur.patch.layers[0], grip),
                };
                moved = moved.or(drop);
            }
            if let Some((from, to)) = moved {
                order::move_item(&mut sections, from, to);
                actions.push(Action::SaveSectionOrder(order::save(&sections)));
            }
            ui.data_mut(|d| d.insert_temp(order_id, sections));

        });
    });
}

fn header(ui: &mut Ui, cur: &mut Current, view: &View, export: &mut ExportDialog, actions: &mut Vec<Action>) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, icon::EXPORT, &format!("Export (⌘E)\n{}", export.summary(cur.patch.mode))).clicked() {
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
    tag_row(ui, cur, actions);
}

/// Tags are stored as one whitespace-separated string; these helpers treat it as a list.
pub fn parse_tags(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in s.split(|c: char| c.is_whitespace() || c == ',').map(|t| t.trim_start_matches('#')).filter(|t| !t.is_empty()) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(t)) {
            out.push(t.to_string());
        }
    }
    out
}

/// Chips with a remove button each, followed by a field that adds a tag on Enter, space or comma.
fn tag_row(ui: &mut Ui, cur: &mut Current, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let mut tags = parse_tags(&cur.tags);
    let mut changed = false;
    let input_id = Id::new(("tag_input", cur.id));
    let mut text: String = ui.data(|d| d.get_temp(input_id)).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
        ui.add_space(4.0);
        ui.label(RichText::new(icon::TAG).color(p.faint));
        let mut remove = None;
        for (i, tag) in tags.iter().enumerate() {
            if tag_chip(ui, tag) {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            tags.remove(i);
            changed = true;
        }
        let field = ui.add(
            egui::TextEdit::singleline(&mut text)
                .id(input_id.with("field"))
                .hint_text(RichText::new(if tags.is_empty() { "Add tags" } else { "Add tag" }).color(p.faint))
                .text_color(p.muted)
                .frame(Frame::NONE)
                .desired_width(70.0),
        );
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let separated = text.contains(|c: char| c.is_whitespace() || c == ',');
        if separated || field.lost_focus() {
            for t in parse_tags(&text) {
                if !tags.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
                    tags.push(t);
                    changed = true;
                }
            }
            text.clear();
            if enter {
                field.request_focus();
            }
        } else if text.is_empty() && field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
            // The press that emptied the field has already been consumed by it; this one removes the last chip.
            if ui.data(|d| d.get_temp::<bool>(input_id.with("was_empty"))).unwrap_or(false) && tags.pop().is_some() {
                changed = true;
            }
        }
        ui.data_mut(|d| d.insert_temp(input_id.with("was_empty"), text.is_empty()));
    });
    ui.data_mut(|d| d.insert_temp(input_id, text));
    if changed {
        cur.tags = tags.join(" ");
        actions.push(Action::SetTags(cur.tags.clone()));
    }
}

/// One flat tag chip. Returns true when its remove button was clicked.
fn tag_chip(ui: &mut Ui, tag: &str) -> bool {
    let p = palette(ui);
    let galley = ui.painter().layout_no_wrap(tag.to_string(), FontId::proportional(12.0), p.muted);
    let x_w = 14.0;
    let size = Vec2::new(galley.size().x + 10.0 + x_w + 6.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let x_rect = Rect::from_min_size(Pos2::new(rect.right() - x_w - 5.0, rect.top()), Vec2::new(x_w + 5.0, rect.height()));
    let x = ui.interact(x_rect, Id::new(("tag_remove", tag)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Remove tag");
    let painter = ui.painter();
    painter.rect_filled(rect, 11.0, p.well);
    painter.galley(Pos2::new(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0), galley, p.muted);
    let color = if x.hovered() { p.text } else { p.faint };
    painter.text(Pos2::new(x_rect.center().x + 1.0, rect.center().y), egui::Align2::CENTER_CENTER, icon::X, FontId::proportional(11.0), color);
    x.clicked()
}

/// Main card: listening on top, then the sound's format and oscillator in one dense row, then export settings.
#[allow(clippy::too_many_arguments)]
fn transport(
    ui: &mut Ui,
    cur: &mut Current,
    view: &View,
    export: &mut ExportDialog,
    mode_note: &mut Option<String>,
    actions: &mut Vec<Action>,
) {
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
            ui.vertical(|ui| {
                waveform(ui, view.rendered, view.progress, 92.0, export.fixed_length.then_some(export.seconds));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let modes: Vec<_> = Mode::ALL.iter().map(|m| (*m, m.label())).collect();
                    if dropdown(ui, "mode", 100.0, &mut cur.patch.mode, &modes) {
                        apply_mode(&mut cur.patch, mode_note);
                    }
                    dropdown(ui, "sample_rate", 100.0, &mut export.sample_rate, &[(22_050, "22.05 kHz"), (44_100, "44.1 kHz"), (48_000, "48 kHz")]);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let default = SoundPatch::default().master_volume;
                        controls::track(ui, &mut cur.patch.master_volume, ranges::UNIT.0, ranges::UNIT.1, default, false, 110.0)
                            .on_hover_text("Level of this sound, saved with it");
                        ui.label(RichText::new("Gain").size(12.5).color(palette(ui).muted));
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    let mode = cur.patch.mode;
                    let before = cur.patch.layers[0].source.clone();
                    wave_field(ui, &mut cur.patch.layers[0], mode);
                    wave_extras(ui, &mut cur.patch.layers[0], mode);
                    if cur.patch.layers[0].source != before {
                        *mode_note = None;
                    }
                    // What the mode switch changed, next to the wave it changed.
                    if let Some(note) = mode_note.as_deref() {
                        let color = palette(ui).accent_text;
                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon::INFO, FontId::proportional(16.0), color);
                        // Shown at once while hovered, without egui's still-pointer delay.
                        if resp.on_hover_cursor(CursorIcon::Help).hovered() {
                            egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), Id::new("mode_note_tip"), egui::PopupAnchor::Pointer)
                                .show(|ui| {
                                    ui.label(note);
                                });
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, Kind::Secondary, Some(icon::MAGIC_WAND), "Mutate").on_hover_text("Small random changes (M)").clicked() {
                            actions.push(Action::Mutate);
                        }
                    });
                });
            });
        });
        if let Source::Fm { algorithm, feedback, ops } = &mut cur.patch.layers[0].source {
            ui.add_space(6.0);
            fm_controls(ui, algorithm, feedback, ops);
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);
        let natural = view.rendered.filter(|(s, _)| !s.is_empty()).map(|(s, sr)| s.len() as f32 / sr as f32);
        export_dialog::panel(ui, export, natural);
    });
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

/// "Generate sample" card: one button per category, laid out like the other settings cards.
fn generators(ui: &mut Ui, grip: Grip, actions: &mut Vec<Action>) -> Option<(usize, usize)> {
    section(ui, "generate", "Generate sample", Some(grip), |_| {}, |ui| {
        const COLUMNS: usize = 4;
        let gap = 6.0;
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        let natural = Category::ALL.iter().map(|c| button_width(ui, Some(category_icon(*c)), c.label())).fold(0.0, f32::max);
        let fit = (ui.available_width() - gap * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
        let width = natural.max(fit.floor());
        for row in Category::ALL.chunks(COLUMNS) {
            ui.horizontal(|ui| {
                for &c in row {
                    if button_min_width(ui, Kind::Ghost, Some(category_icon(c)), c.label(), width)
                        .on_hover_text(format!("Replace this sound with a random {} sound", c.label().to_lowercase()))
                        .clicked()
                    {
                        actions.push(Action::Generate(c));
                    }
                }
            });
        }
    })
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

/// Waveform choice and its parameter, as fields of the main card's settings row.
fn wave_field(ui: &mut Ui, layer: &mut Layer, mode: Mode) {
    // FM is not offered for now; a sound that already uses it keeps its controls below the row.
    let all: Vec<&'static str> = Source::KIND_NAMES.iter().copied().filter(|k| *k != "FM").collect();
    let kinds: &[&'static str] = if mode == Mode::Bit8 { &["Pulse", "Triangle", "Noise"] } else { &all };
    let mut kind = layer.source.kind_name();
    wave_select(ui, &mut kind, kinds);
    if kind != layer.source.kind_name() {
        layer.source = map_source_to_mode(&Source::default_of_kind(kind), mode);
    }
}

/// Settings that only some waves have: duty for pulse, the noise type.
fn wave_extras(ui: &mut Ui, layer: &mut Layer, mode: Mode) {
    match &mut layer.source {
        Source::Pulse { duty } if mode == Mode::Bit8 => {
            let labels: Vec<String> = NES_DUTIES.iter().map(|d| format!("{}%", d * 100.0)).collect();
            let options: Vec<_> = NES_DUTIES.iter().zip(&labels).map(|(d, l)| (*d, l.as_str())).collect();
            field(ui, "Duty", |ui| dropdown(ui, "duty", 80.0, duty, &options));
        }
        Source::Pulse { duty } => {
            field(ui, "Duty", |ui| {
                controls::track(ui, duty, ranges::DUTY.0, ranges::DUTY.1, 0.5, false, 110.0);
                ui.label(hint(ui, format!("{:.2}", duty)));
            });
        }
        Source::Noise { kind } => {
            let kinds: &[NoiseKind] = if mode == Mode::Bit8 { &[NoiseKind::LfsrLong, NoiseKind::LfsrShort] } else { &NoiseKind::ALL };
            let options: Vec<_> = kinds.iter().map(|k| (*k, k.label())).collect();
            field(ui, "Noise", |ui| dropdown(ui, "noise_kind", 120.0, kind, &options));
        }
        _ => {}
    }
}

/// Waveform dropdown that shows the chosen wave's shape next to its name, closed or open.
fn wave_select(ui: &mut Ui, kind: &mut &'static str, kinds: &[&'static str]) {
    let p = palette(ui);
    let mut job = egui::text::LayoutJob::default();
    job.append(kind, 34.0, egui::TextFormat { font_id: FontId::proportional(13.0), color: p.text, ..Default::default() });
    let r = ui.allocate_ui(Vec2::new(140.0, 30.0), |ui| {
        select(ui, "source_kind", job, |ui| {
            for k in kinds {
                wave_option(ui, kind, k);
            }
        })
        .response
    });
    let rect = r.inner.rect;
    let wave = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.center().y - 6.0), Vec2::new(24.0, 12.0));
    ui.painter().add(egui::Shape::line(wave_points(kind, wave), egui::Stroke::new(1.5, p.accent_text)));
}

/// One cycle-or-two of the named waveform as a polyline inside `rect`.
fn wave_points(name: &str, rect: Rect) -> Vec<Pos2> {
    const N: usize = 48;
    let cycles = 2.0;
    (0..=N)
        .map(|i| {
            let t = i as f32 / N as f32;
            let ph = (t * cycles).fract();
            let y = match name {
                "Pulse" => {
                    if ph < 0.5 { 1.0 } else { -1.0 }
                }
                "Saw" => 1.0 - 2.0 * ph,
                "Triangle" => 1.0 - 4.0 * (ph - 0.5).abs(),
                "Noise" => {
                    // Fixed pseudo-random wiggle, so the icon does not change between frames.
                    let h = (i as u32).wrapping_mul(2_654_435_761) >> 16;
                    (h % 1000) as f32 / 500.0 - 1.0
                }
                _ => (ph * std::f32::consts::TAU).sin(),
            };
            Pos2::new(egui::lerp(rect.left()..=rect.right(), t), rect.center().y - y * rect.height() * 0.5)
        })
        .collect()
}

/// Dropdown entry: a small drawing of the waveform, then its name.
fn wave_option(ui: &mut Ui, value: &mut &'static str, name: &'static str) {
    let p = palette(ui);
    let selected = *value == name;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(150.0), 28.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if selected {
        ui.painter().rect_filled(rect, theme::R_CONTROL, p.accent_soft.gamma_multiply(0.55));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, theme::R_CONTROL, p.hover);
    }
    let color = if selected { p.accent_text } else if resp.hovered() { p.text } else { p.muted };
    let wave = Rect::from_min_size(Pos2::new(rect.left() + 8.0, rect.center().y - 8.0), Vec2::new(40.0, 16.0));
    ui.painter().add(egui::Shape::line(wave_points(name, wave), egui::Stroke::new(1.5, color)));
    ui.painter().text(Pos2::new(wave.right() + 12.0, rect.center().y), egui::Align2::LEFT_CENTER, name, FontId::proportional(13.0), color);
    if resp.clicked() {
        *value = name;
    }
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

/// Not shown for now; see `SETTINGS`.
#[allow(dead_code)]
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

fn eq_section(ui: &mut Ui, layer: &mut Layer, grip: Grip) -> Option<(usize, usize)> {
    let eq = &mut layer.eq;
    section(ui, "eq", "Equalizer", Some(grip), |_| {}, |ui| {
        eq_graph::show(ui, eq);
        ui.add_space(6.0);
        // `with_layout` alone would claim all the remaining height of the scroll area.
        ui.horizontal(|ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                if button(ui, Kind::Secondary, None, "Reset").on_hover_text("Flatten every band").clicked() {
                    eq.gains = [0.0; sfxc_core::eq::BANDS];
                }
            });
        });
    })
}

#[cfg(test)]
mod tag_tests {
    use super::parse_tags;

    #[test]
    fn tags_split_on_whitespace_and_commas_without_duplicates() {
        assert_eq!(parse_tags("ui retro"), vec!["ui", "retro"]);
        assert_eq!(parse_tags(" #ui, Retro ,ui  RETRO "), vec!["ui", "Retro"]);
        assert!(parse_tags("  ,, ").is_empty());
    }
}
