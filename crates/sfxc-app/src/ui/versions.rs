use eframe::egui::{self, Align, FontId, Id, Layout, Pos2, Rect, RichText, Stroke};
use egui_phosphor::regular as icon;

use super::theme::{self, palette};
use super::widgets::{self, age, badge, empty_state, icon_button, Tone};
use super::Action;
use sfxc_store::VersionInfo;

const GUTTER: f32 = 22.0;

pub fn show(ui: &mut egui::Ui, versions: &[VersionInfo], current: Option<i64>, has_sound: bool, now: i64, note_prompt: &mut Option<String>, actions: &mut Vec<Action>) {
    widgets::panel_header(ui, "History", |_| {});
    ui.add_space(8.0);
    if !has_sound {
        ui.add_space(24.0);
        empty_state(ui, icon::CLOCK_COUNTER_CLOCKWISE, "No history", "Versions of the open sound appear here.", |_| {});
        return;
    }
    widgets::list_well(ui, |ui| {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let total = versions.len();
        entry(ui, true, total == 0, |ui| {
            ui.horizontal(|ui| {
                if let Some(note) = note_prompt.as_mut() {
                    let r = ui.add(
                        egui::TextEdit::singleline(note)
                            .hint_text("Name this version")
                            .font(FontId::new(13.5, theme::semibold()))
                            .desired_width(f32::INFINITY),
                    );
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    if r.lost_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            actions.push(Action::CommitVersion(std::mem::take(note)));
                        } else {
                            *note_prompt = None;
                        }
                    }
                } else {
                    ui.label(RichText::new("Draft").font(FontId::new(13.5, theme::semibold())).color(palette(ui).text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, icon::FLOPPY_DISK, "Save a version (⌘S)").clicked() {
                            actions.push(Action::AskVersionNote);
                        }
                    });
                }
            });
            ui.label(widgets::hint(ui, "Autosaved as you edit"));
        });
        for (i, v) in versions.iter().enumerate() {
            let last = i + 1 == total;
            let hover_id = Id::new(("version_hover", v.id));
            let hovered = ui.data(|d| d.get_temp::<bool>(hover_id)).unwrap_or(false);
            let rect = entry(ui, false, last, |ui| version_body(ui, v, total - i, current == Some(v.id), hovered, now, actions));
            let hovered = ui.rect_contains_pointer(rect);
            ui.data_mut(|d| d.insert_temp(hover_id, hovered));
        }
    });
    });
}

fn entry(ui: &mut egui::Ui, draft: bool, last: bool, body: impl FnOnce(&mut egui::Ui)) -> Rect {
    let p = palette(ui);
    let top = ui.cursor().top();
    let resp = ui
        .horizontal(|ui| {
            ui.add_space(GUTTER);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                body(ui);
                ui.add_space(12.0);
            });
        })
        .response;
    let x = resp.rect.left() + GUTTER / 2.0 - 2.0;
    let node = Pos2::new(x, top + 9.0);
    if !last {
        ui.painter().vline(x, (node.y + 6.0)..=resp.rect.bottom() + 2.0, Stroke::new(1.0, p.border_strong));
    }
    if draft {
        ui.painter().circle(node, 4.5, p.accent_soft, Stroke::new(1.5, p.accent));
    } else {
        ui.painter().circle(node, 4.0, p.chrome, Stroke::new(1.5, p.border_strong));
    }
    resp.rect
}

#[allow(clippy::too_many_arguments)]
fn version_body(
    ui: &mut egui::Ui,
    v: &VersionInfo,
    number: usize,
    is_current: bool,
    hovered: bool,
    now: i64,
    actions: &mut Vec<Action>,
) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("v{number}")).font(FontId::new(13.5, theme::semibold())).color(p.text));
        if is_current {
            badge(ui, "current", Tone::Accent);
        }
        if v.exported_at.is_some() {
            badge(ui, "exported", Tone::Neutral);
        }
        if v.broken {
            badge(ui, "unreadable", Tone::Danger);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(!v.broken, |ui| {
                ui.set_opacity(if hovered { 1.0 } else { 0.0 });
                if icon_button(ui, icon::COPY, "Duplicate as new sound").clicked() {
                    actions.push(Action::DuplicateVersion(v.id));
                }
                if icon_button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Restore this version").clicked() {
                    actions.push(Action::Restore(v.id));
                }
            });
        });
    });
    let note = if v.note == "auto" { "Auto-saved" } else { v.note.as_str() };
    if !note.is_empty() {
        ui.add(egui::Label::new(RichText::new(note).color(p.muted).size(12.5)).wrap());
    }
    ui.label(widgets::hint(ui, age(now - v.created_at)));
}
