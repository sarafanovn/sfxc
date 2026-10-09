use eframe::egui::{self, Align, CursorIcon, FontId, Id, Key, Layout, Pos2, Sense, Vec2};
use egui_phosphor::regular as icon;

use super::accent;
use super::theme::{palette, ThemeChoice, UI_SCALES};
use super::widgets::{self, age, button, empty_state, icon_button, search_field, segmented, Kind};
use super::theme::R_CONTROL;
use super::Action;
use sfxc_store::{Membership, ProjectSummary, SoundSummary};

pub struct ProjectView {
    pub summary: ProjectSummary,
    /// Sounds matching the current search, sorted by path.
    pub members: Vec<Membership>,
}

const ROW_H: f32 = 46.0;

fn rename_key() -> Id {
    Id::new("library_rename")
}

pub struct Prefs {
    pub theme: ThemeChoice,
    pub scale: f32,
    pub volume: f32,
    pub accent_hue: f32,
    pub autoplay: bool,
}

pub fn show(
    ui: &mut egui::Ui,
    sounds: &[SoundSummary],
    search: &mut String,
    current: Option<i64>,
    now: i64,
    prefs: &mut Prefs,
    actions: &mut Vec<Action>,
) {
    widgets::panel_header(ui, "Library", |ui| {
        if icon_button(ui, icon::FOLDER_PLUS, "New project").clicked() {
            actions.push(Action::AskNewProject { add_sound: None });
        }
    });
    ui.add_space(4.0);
    if search_field(ui, search, "Search name or tag").changed() {
        actions.push(Action::RefreshList);
    }
    ui.add_space(8.0);

    egui::Panel::bottom("library_footer").frame(egui::Frame::NONE).show_separator_line(false).show(ui, |ui| {
        ui.set_clip_rect(ui.clip_rect().expand(14.0));
        ui.add_space(6.0);
        settings_button(ui, prefs, actions);
        ui.add_space(4.0);
    });

    widgets::list_well(ui, |ui| {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_clip_rect(ui.clip_rect().expand2(Vec2::new(8.0, 0.0)));
        ui.spacing_mut().item_spacing.y = 8.0;
        new_row(ui, actions);
        if sounds.is_empty() {
            ui.add_space(24.0);
            if search.trim().is_empty() {
                empty_state(ui, icon::WAVEFORM, "No sounds yet", "Generate your first effect and it appears here.", |_| {});
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
    });
}

fn flat_row_background(ui: &egui::Ui, rect: egui::Rect, resp: &egui::Response, selected: bool) {
    let p = palette(ui);
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let fill = if selected { p.accent_soft.gamma_multiply(0.55) } else { p.hover.gamma_multiply(hover) };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(R_CONTROL), fill);
}

fn new_row(ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("New sound (⌘N)");
    flat_row_background(ui, rect, &resp, false);
    let painter = ui.painter_at(rect);
    let color = p.accent_text;
    let inner = rect.shrink2(Vec2::new(10.0, 7.0));
    let plus = painter.layout_no_wrap(icon::PLUS.to_string(), FontId::proportional(15.0), color);
    let label = painter.layout_no_wrap("New sound".to_string(), FontId::proportional(13.5), color);
    let hint = painter.layout_no_wrap("⌘N".to_string(), FontId::proportional(11.0), p.faint);
    let cy = rect.center().y;
    painter.galley(Pos2::new(inner.left(), cy - plus.size().y / 2.0), plus.clone(), color);
    painter.galley(Pos2::new(inner.left() + plus.size().x + 8.0, cy - label.size().y / 2.0), label, color);
    painter.galley(Pos2::new(inner.right() - hint.size().x, cy - hint.size().y / 2.0), hint, p.faint);
    if resp.clicked() {
        actions.push(Action::NewSound);
    }
}

fn row(ui: &mut egui::Ui, s: &SoundSummary, selected: bool, now: i64, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    flat_row_background(ui, rect, &resp, selected);
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
    let tags = super::editor::parse_tags(&s.tags).join(" · ");
    if !tags.is_empty() {
        let sub = one_line(ui, &tags, 11.5, p.faint, inner.width());
        painter.galley(Pos2::new(inner.left(), inner.bottom() - sub.size().y), sub, p.faint);
    }

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

fn one_line(ui: &egui::Ui, text: &str, size: f32, color: egui::Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), FontId::proportional(size), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    ui.painter().layout_job(job)
}

const POPUP_W: f32 = 300.0;

pub fn settings_button(ui: &mut egui::Ui, prefs: &mut Prefs, actions: &mut Vec<Action>) -> egui::Rect {
    let resp = icon_button(ui, icon::GEAR_SIX, "Settings");
    let frame = egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(22));
    egui::Popup::from_toggle_button_response(&resp)
        .align(egui::RectAlign::TOP_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(frame)
        .width(POPUP_W)
        .show(|ui| {
            let p = palette(ui);
            ui.set_width(POPUP_W);
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            let heading = |ui: &mut egui::Ui, text: &str| {
                ui.label(egui::RichText::new(text).font(FontId::new(13.5, super::theme::semibold())).color(p.text));
            };

            heading(ui, "Appearance");
            ui.add_space(6.0);
            ui.label(widgets::hint(ui, "Theme"));
            let mut theme = prefs.theme;
            let options: Vec<(ThemeChoice, String)> =
                ThemeChoice::ALL.iter().map(|t| (*t, format!("{}  {}", t.icon(), t.label()))).collect();
            let opts: Vec<(ThemeChoice, &str)> = options.iter().map(|(t, l)| (*t, l.as_str())).collect();
            if segmented(ui, &mut theme, &opts) {
                actions.push(Action::SetTheme(theme));
            }
            ui.label(widgets::hint(ui, "Auto follows the macOS appearance."));
            ui.add_space(10.0);

            ui.label(widgets::hint(ui, "Accent"));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let dark = ui.visuals().dark_mode;
                for hue in accent::SWATCHES {
                    let color = super::theme::with_accent(p, hue, dark).accent;
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                    let selected = (prefs.accent_hue - hue).abs() < 0.5;
                    ui.painter().circle_filled(rect.center(), if selected { 11.0 } else { 9.0 }, color);
                    if selected {
                        ui.painter().circle_stroke(rect.center(), 12.0, egui::Stroke::new(2.0, p.text));
                    }
                    if r.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        actions.push(Action::SetAccent { hue, persist: true });
                    }
                }
            });
            ui.add_space(10.0);

            ui.label(widgets::hint(ui, "Interface size"));
            let mut scale = prefs.scale;
            if segmented(ui, &mut scale, &UI_SCALES) {
                actions.push(Action::SetScale(scale));
            }
            ui.add_space(14.0);

            heading(ui, "Sound");
            ui.add_space(6.0);
            let mut autoplay = prefs.autoplay;
            if widgets::toggle(ui, &mut autoplay, "Auto-play").on_hover_text("Play after every change").changed() {
                actions.push(Action::SetAutoplay(autoplay));
            }
            let mut pct = crate::audio::slider_from_volume(prefs.volume) * 100.0;
            ui.horizontal(|ui| {
                ui.label(widgets::hint(ui, "Volume"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(widgets::hint(ui, format!("{pct:.0}%")));
                });
            });
            let default = crate::audio::slider_from_volume(crate::audio::DEFAULT_VOLUME) * 100.0;
            let r = super::controls::track_quiet(ui, &mut pct, 0.0, 100.0, default, false, POPUP_W);
            if r.changed() {
                prefs.volume = crate::audio::volume_from_slider(pct / 100.0);
            }
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                actions.push(Action::SaveVolume);
            }
            ui.label(widgets::hint(ui, "Playback only. A sound's own level is Gain."));
            #[cfg(test)]
            ui.data_mut(|d| d.insert_temp(egui::Id::new("volume_track"), r.rect));
        });
    resp.rect
}
