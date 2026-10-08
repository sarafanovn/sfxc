use eframe::egui::{self, Align, CursorIcon, FontId, Id, Key, Layout, Pos2, Sense, Vec2};
use egui_phosphor::regular as icon;

use super::theme::{palette, ThemeChoice, UI_SCALES};
use super::widgets::{self, age, button, empty_state, icon_button, row_background, search_field, segmented, Kind};
use super::Action;
use crate::store::SoundSummary;

const ROW_H: f32 = 46.0;

/// egui temp key holding `(sound id, edited text, just started)` while a row is renamed.
fn rename_key() -> Id {
    Id::new("library_rename")
}

pub struct Prefs {
    pub theme: ThemeChoice,
    pub scale: f32,
    /// Playback amplitude, 0..=1. Not part of any sound.
    pub volume: f32,
}

pub fn show(
    ui: &mut egui::Ui,
    sounds: &[SoundSummary],
    search: &mut String,
    current: Option<i64>,
    now: i64,
    prefs: &Prefs,
    actions: &mut Vec<Action>,
) {
    widgets::panel_header(ui, "Library", |ui| {
        if icon_button(ui, icon::PLUS, "New sound (⌘N)").clicked() {
            actions.push(Action::NewSound);
        }
    });
    ui.add_space(4.0);
    if search_field(ui, search, "Search name or tag").changed() {
        actions.push(Action::RefreshList);
    }
    ui.add_space(8.0);

    egui::Panel::bottom("library_footer").frame(egui::Frame::NONE).show_separator_line(false).show(ui, |ui| {
        ui.add_space(6.0);
        settings_button(ui, prefs, actions);
        ui.add_space(4.0);
    });

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if sounds.is_empty() {
            ui.add_space(24.0);
            if search.trim().is_empty() {
                empty_state(ui, icon::WAVEFORM, "No sounds yet", "Generate your first effect and it appears here.", |ui| {
                    if button(ui, Kind::Primary, Some(icon::PLUS), "New sound").clicked() {
                        actions.push(Action::NewSound);
                    }
                });
            } else {
                empty_state(ui, icon::MAGNIFYING_GLASS, "No matches", "Nothing matches this name or tag.", |ui| {
                    if button(ui, Kind::Secondary, None, "Clear search").clicked() {
                        search.clear();
                        actions.push(Action::RefreshList);
                    }
                });
            }
            return;
        }
        for s in sounds {
            row(ui, s, current == Some(s.id), now, actions);
        }
    });
}

fn row(ui: &mut egui::Ui, s: &SoundSummary, selected: bool, now: i64, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    row_background(ui, rect, &resp, selected);
    let inner = rect.shrink2(Vec2::new(10.0, 7.0));
    let editing: Option<(i64, String, bool)> = ui.data(|d| d.get_temp(rename_key()));
    if let Some((id, mut text, started)) = editing.filter(|(id, ..)| *id == s.id) {
        let name_rect = egui::Rect::from_min_size(inner.left_top(), Vec2::new(inner.width(), 20.0));
        let edit_id = Id::new(("library_rename_field", id));
        let r = ui.put(name_rect, egui::TextEdit::singleline(&mut text).id(edit_id).font(FontId::proportional(13.5)));
        if started {
            r.request_focus();
            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), edit_id) {
                let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()));
                st.cursor.set_char_range(Some(all));
                st.store(ui.ctx(), edit_id);
            }
        }
        if r.lost_focus() {
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                actions.push(Action::RenameSound(id, text));
            }
            ui.data_mut(|d| d.remove::<(i64, String, bool)>(rename_key()));
        } else {
            ui.data_mut(|d| d.insert_temp(rename_key(), (id, text, false)));
        }
        return;
    }
    let name_color = if selected { p.accent_text } else { p.text };
    let painter = ui.painter_at(rect);
    let when = painter.layout_no_wrap(age(now - s.updated_at), FontId::proportional(11.0), p.faint);
    let name = one_line(ui, &s.name, 13.5, name_color, inner.width() - when.size().x - 8.0);
    painter.galley(inner.left_top(), name, name_color);
    painter.galley(Pos2::new(inner.right() - when.size().x, inner.top() + 2.0), when, p.faint);
    let tags = if s.tags.trim().is_empty() { "No tags" } else { s.tags.as_str() };
    let sub = one_line(ui, tags, 11.5, p.faint, inner.width());
    painter.galley(Pos2::new(inner.left(), inner.bottom() - sub.size().y), sub, p.faint);

    if resp.clicked() && !selected {
        actions.push(Action::Open(s.id));
    }
    if resp.double_clicked() {
        ui.data_mut(|d| d.insert_temp(rename_key(), (s.id, s.name.clone(), true)));
    }
    let menu = |ui: &mut egui::Ui| {
        if ui.button(format!("{}  Rename", icon::PENCIL_SIMPLE)).clicked() {
            ui.data_mut(|d| d.insert_temp(rename_key(), (s.id, s.name.clone(), true)));
        }
        if ui.button(format!("{}  Duplicate", icon::COPY)).clicked() {
            actions.push(Action::DuplicateSound(s.id));
        }
        if ui.button(egui::RichText::new(format!("{}  Delete…", icon::TRASH)).color(p.danger)).clicked() {
            actions.push(Action::AskDelete(s.id));
        }
    };
    resp.context_menu(menu);
}

/// Lays out one line cut to `width` with an ellipsis.
fn one_line(ui: &egui::Ui, text: &str, size: f32, color: egui::Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), FontId::proportional(size), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    ui.painter().layout_job(job)
}

fn settings_button(ui: &mut egui::Ui, prefs: &Prefs, actions: &mut Vec<Action>) {
    let resp = button(ui, Kind::Ghost, Some(icon::GEAR_SIX), "Settings");
    egui::Popup::from_toggle_button_response(&resp)
        .align(egui::RectAlign::TOP_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(260.0)
        .show(|ui| {
            let p = palette(ui);
            ui.set_width(260.0);
            ui.add_space(2.0);
            ui.label(egui::RichText::new("Appearance").font(FontId::new(13.5, super::theme::semibold())).color(p.text));
            ui.add_space(8.0);
            ui.label(widgets::hint(ui, "Theme"));
            let mut theme = prefs.theme;
            let options: Vec<(ThemeChoice, String)> =
                ThemeChoice::ALL.iter().map(|t| (*t, format!("{}  {}", t.icon(), t.label()))).collect();
            let opts: Vec<(ThemeChoice, &str)> = options.iter().map(|(t, l)| (*t, l.as_str())).collect();
            if segmented(ui, &mut theme, &opts) {
                actions.push(Action::SetTheme(theme));
            }
            ui.add_space(10.0);
            ui.label(widgets::hint(ui, "Interface size"));
            let mut scale = prefs.scale;
            if segmented(ui, &mut scale, &UI_SCALES) {
                actions.push(Action::SetScale(scale));
            }
            ui.add_space(10.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.label(widgets::hint(ui, "Auto follows the macOS appearance."));
            });
        });
}
