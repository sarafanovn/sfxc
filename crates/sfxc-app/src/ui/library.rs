use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Align, Align2, CursorIcon, FontId, Id, Key, Layout, Pos2, Rect, Sense, UiBuilder, Vec2};
use egui_phosphor::regular as icon;
use sfxc_store::{Membership, ProjectSummary, SoundSummary};

use super::accent;
use super::project_actions::ProjectStatus;
use super::theme::{self, palette, ThemeChoice, R_CONTROL, UI_SCALES};
use super::widgets::{self, age, button, empty_state, icon_button, search_field, segmented, Kind};
use super::Action;

pub struct ProjectView {
    pub summary: ProjectSummary,
    /// Sounds matching the current search, sorted by path.
    pub members: Vec<Membership>,
}

pub struct Prefs {
    pub theme: ThemeChoice,
    pub scale: f32,
    pub volume: f32,
    pub accent_hue: f32,
    pub autoplay: bool,
}

const ROW_H: f32 = 46.0;
const HEADER_H: f32 = 34.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Section {
    NoProject,
    Project(i64),
}

impl Section {
    /// Key in the `library_collapsed` setting.
    pub fn key(self) -> String {
        match self {
            Section::NoProject => "none".into(),
            Section::Project(id) => format!("p{id}"),
        }
    }
}

pub struct LibraryView<'a> {
    /// Every matching sound; shown as one flat list while there are no projects.
    pub sounds: &'a [SoundSummary],
    pub unassigned: &'a [SoundSummary],
    pub projects: &'a [ProjectView],
    pub status: &'a ProjectStatus,
    /// The last export error per (project, sound), until that sound is exported fine.
    pub export_errors: &'a HashMap<(i64, i64), String>,
    /// The project being exported right now.
    pub exporting: Option<i64>,
    pub collapsed: &'a HashSet<String>,
}

/// Inline rename in progress. Keyed by section too: a sound in two projects has two rows.
#[derive(Clone)]
struct Rename {
    section: Section,
    id: i64,
    text: String,
    started: bool,
}

fn rename_key() -> Id {
    Id::new("library_rename")
}

fn rename_field_id(section: Section, id: i64) -> Id {
    Id::new(("library_rename_field", section, id))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Marker {
    Changed,
    Missing,
    Failed,
}

enum Right {
    Age(i64),
    Mark(Marker),
    Nothing,
}

struct RowData<'a> {
    id: i64,
    name: &'a str,
    sub: String,
    right: Right,
    tip: Option<String>,
}

impl<'a> RowData<'a> {
    fn sound(s: &'a SoundSummary, now: i64) -> Self {
        Self { id: s.id, name: &s.name, sub: super::editor::parse_tags(&s.tags).join(" · "), right: Right::Age(now - s.updated_at), tip: None }
    }

    fn member(m: &'a Membership, view: &LibraryView) -> Self {
        let key = (m.project_id, m.sound_id);
        let missing = view.status.missing.get(&key).copied().unwrap_or(false);
        let (right, mut tip) = if let Some(e) = view.export_errors.get(&key) {
            (Right::Mark(Marker::Failed), format!("Export failed: {e}"))
        } else if m.changed() {
            (Right::Mark(Marker::Changed), "Changed since last export".to_string())
        } else if missing {
            (Right::Mark(Marker::Missing), format!("File missing: {}", m.rel_path))
        } else {
            (Right::Nothing, m.rel_path.clone())
        };
        let tags = super::editor::parse_tags(&m.tags).join(" · ");
        if !tags.is_empty() {
            tip.push_str(&format!("\nTags: {tags}"));
        }
        Self { id: m.sound_id, name: &m.sound_name, sub: m.rel_path.clone(), right, tip: Some(tip) }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    view: &LibraryView,
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
        let searching = !search.trim().is_empty();
        let mut shown = 0;
        if view.projects.is_empty() {
            for s in view.sounds {
                row(ui, &RowData::sound(s, now), Section::NoProject, current == Some(s.id), view, actions);
            }
            shown = view.sounds.len();
        } else {
            if !(searching && view.unassigned.is_empty()) {
                shown += 1;
                if section_header(ui, Section::NoProject, "No project", None, view, searching, actions) {
                    for s in view.unassigned {
                        row(ui, &RowData::sound(s, now), Section::NoProject, current == Some(s.id), view, actions);
                    }
                }
            }
            for pv in view.projects {
                if searching && pv.members.is_empty() {
                    continue;
                }
                shown += 1;
                let section = Section::Project(pv.summary.project.id);
                if section_header(ui, section, &pv.summary.project.name, Some(pv), view, searching, actions) {
                    for m in &pv.members {
                        row(ui, &RowData::member(m, view), section, current == Some(m.sound_id), view, actions);
                    }
                }
            }
        }
        if shown == 0 {
            ui.add_space(24.0);
            if !searching {
                empty_state(ui, icon::WAVEFORM, "No sounds yet", "Generate your first effect and it appears here.", |_| {});
            } else {
                empty_state(ui, icon::MAGNIFYING_GLASS, "No matches", "Nothing matches this name, tag or path.", |ui| {
                    if button(ui, Kind::Secondary, None, "Clear search").clicked() {
                        search.clear();
                        actions.push(Action::RefreshList);
                    }
                });
            }
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


/// Draws a section title; returns whether its rows are shown. Collapsed sections open while searching.
fn section_header(
    ui: &mut egui::Ui,
    section: Section,
    title: &str,
    project: Option<&ProjectView>,
    view: &LibraryView,
    searching: bool,
    actions: &mut Vec<Action>,
) -> bool {
    let p = palette(ui);
    let open = searching || !view.collapsed.contains(&section.key());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HEADER_H), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter_at(rect);
    let cy = rect.center().y;
    let caret = if open { icon::CARET_DOWN } else { icon::CARET_RIGHT };
    painter.text(Pos2::new(rect.left() + 6.0, cy), Align2::LEFT_CENTER, caret, FontId::proportional(13.0), p.muted);
    let x = rect.left() + 24.0;
    let title_galley = painter.layout_no_wrap(title.to_string(), FontId::new(12.5, theme::semibold()), p.text);
    let title_w = title_galley.size().x;
    painter.galley(Pos2::new(x, cy - title_galley.size().y / 2.0), title_galley, p.text);
    let (count, color) = match project {
        Some(pv) if pv.summary.changed > 0 => (format!("• {}", pv.summary.changed), p.accent_text),
        Some(pv) => (pv.summary.sounds.to_string(), p.faint),
        None => (view.unassigned.len().to_string(), p.faint),
    };
    painter.text(Pos2::new(x + title_w + 8.0, cy), Align2::LEFT_CENTER, count, FontId::proportional(11.5), color);
    let resp = match project {
        Some(pv) => {
            let area = Rect::from_min_max(Pos2::new(rect.right() - 80.0, rect.top()), rect.max);
            let mut tail = ui.new_child(UiBuilder::new().max_rect(area).layout(Layout::right_to_left(Align::Center)));
            header_buttons(&mut tail, pv, view, actions);
            let id = pv.summary.project.id;
            let resp = resp.on_hover_text(&pv.summary.project.root);
            resp.context_menu(|ui| project_menu(ui, id, actions));
            resp
        }
        None => resp,
    };
    if resp.clicked() {
        actions.push(Action::ToggleSection(section.key()));
    }
    open
}

fn header_buttons(ui: &mut egui::Ui, pv: &ProjectView, view: &LibraryView, actions: &mut Vec<Action>) {
    let id = pv.summary.project.id;
    if view.exporting == Some(id) {
        ui.add(egui::Spinner::new().size(14.0));
    } else {
        let missing = view.status.missing_count.get(&id).copied().unwrap_or(0);
        let changed = pv.summary.changed;
        let tip = match (changed, missing) {
            (0, 0) => "Everything is exported".to_string(),
            (c, 0) => format!("Export {c} changed sound{}", if c == 1 { "" } else { "s" }),
            (c, m) => format!("Export {c} changed; {m} missing on disk"),
        };
        let r = ui.add_enabled_ui(changed + missing > 0, |ui| icon_button(ui, icon::EXPORT, &tip).on_disabled_hover_text(&tip)).inner;
        if r.clicked() {
            actions.push(Action::ExportProject(id));
        }
    }
    if icon_button(ui, icon::PLUS, "New sound in this project").clicked() {
        actions.push(Action::NewSoundIn(id));
    }
}

fn project_menu(ui: &mut egui::Ui, id: i64, actions: &mut Vec<Action>) {
    if ui.button(format!("{}  Export", icon::EXPORT)).clicked() {
        actions.push(Action::ExportProject(id));
    }
    if ui.button(format!("{}  Rename…", icon::PENCIL_SIMPLE)).clicked() {
        actions.push(Action::AskRenameProject(id));
    }
    if ui.button(format!("{}  Change folder…", icon::FOLDER_OPEN)).clicked() {
        actions.push(Action::AskProjectFolder(id));
    }
    let danger = palette(ui).danger;
    if ui.button(egui::RichText::new(format!("{}  Delete project…", icon::TRASH)).color(danger)).clicked() {
        actions.push(Action::AskDeleteProject(id));
    }
}

fn row(ui: &mut egui::Ui, d: &RowData, section: Section, selected: bool, view: &LibraryView, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    flat_row_background(ui, rect, &resp, selected);
    let inner = rect.shrink2(Vec2::new(10.0, 7.0));
    let editing: Option<Rename> = ui.data(|m| m.get_temp(rename_key()));
    if let Some(mut r) = editing.filter(|r| r.section == section && r.id == d.id) {
        let name_rect = egui::Rect::from_min_size(inner.left_top(), Vec2::new(inner.width(), 20.0));
        let edit_id = rename_field_id(section, d.id);
        let field = ui.put(name_rect, egui::TextEdit::singleline(&mut r.text).id(edit_id).font(FontId::proportional(13.5)));
        if r.started {
            field.request_focus();
            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), edit_id) {
                let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(r.text.chars().count()));
                st.cursor.set_char_range(Some(all));
                st.store(ui.ctx(), edit_id);
            }
        }
        if field.lost_focus() {
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                actions.push(Action::RenameSound(d.id, r.text));
            }
            ui.data_mut(|m| m.remove::<Rename>(rename_key()));
        } else {
            r.started = false;
            ui.data_mut(|m| m.insert_temp(rename_key(), r));
        }
        return;
    }
    let name_color = if selected { p.accent_text } else { p.text };
    let painter = ui.painter_at(rect);
    let right_w = match &d.right {
        Right::Age(secs) => {
            let g = painter.layout_no_wrap(age(*secs), FontId::proportional(11.0), p.faint);
            let w = g.size().x;
            painter.galley(Pos2::new(inner.right() - w, inner.top() + 2.0), g, p.faint);
            w
        }
        Right::Mark(m) => {
            let (glyph, color, size) = match m {
                Marker::Changed => ("•", p.accent, 18.0),
                Marker::Missing => (icon::WARNING, p.warn, 13.0),
                Marker::Failed => (icon::X_CIRCLE, p.danger, 13.0),
            };
            painter.text(Pos2::new(inner.right(), inner.top() + 1.0), Align2::RIGHT_TOP, glyph, FontId::proportional(size), color);
            14.0
        }
        Right::Nothing => 0.0,
    };
    let name = one_line(ui, d.name, 13.5, name_color, inner.width() - right_w - 8.0);
    painter.galley(inner.left_top(), name, name_color);
    if !d.sub.is_empty() {
        let sub = one_line(ui, &d.sub, 11.5, p.faint, inner.width());
        painter.galley(Pos2::new(inner.left(), inner.bottom() - sub.size().y), sub, p.faint);
    }
    let resp = match &d.tip {
        Some(t) => resp.on_hover_text(t),
        None => resp,
    };
    if resp.clicked() && !selected {
        actions.push(Action::Open(d.id));
    }
    if resp.double_clicked() {
        ui.data_mut(|m| m.insert_temp(rename_key(), Rename { section, id: d.id, text: d.name.to_string(), started: true }));
    }
    resp.context_menu(|ui| sound_menu(ui, d, section, view, actions));
}

fn sound_menu(ui: &mut egui::Ui, d: &RowData, section: Section, view: &LibraryView, actions: &mut Vec<Action>) {
    let danger = palette(ui).danger;
    if ui.button(format!("{}  Rename", icon::PENCIL_SIMPLE)).clicked() {
        ui.data_mut(|m| m.insert_temp(rename_key(), Rename { section, id: d.id, text: d.name.to_string(), started: true }));
    }
    if ui.button(format!("{}  Duplicate", icon::COPY)).clicked() {
        actions.push(Action::DuplicateSound(d.id));
    }
    let member_of = view.status.member_of.get(&d.id).map_or(&[][..], Vec::as_slice);
    ui.menu_button(format!("{}  Add to project", icon::FOLDER_SIMPLE_PLUS), |ui| {
        for pv in view.projects {
            let project = &pv.summary.project;
            if !member_of.contains(&project.id) && ui.button(&project.name).clicked() {
                actions.push(Action::AskAddToProject { project_id: project.id, sound_id: d.id });
            }
        }
        if !view.projects.is_empty() {
            ui.separator();
        }
        if ui.button("New project…").clicked() {
            actions.push(Action::AskNewProject { add_sound: Some(d.id) });
        }
    });
    if let Section::Project(project_id) = section {
        if ui.button(format!("{}  Edit export path…", icon::PENCIL_LINE)).clicked() {
            actions.push(Action::AskEditMembership { project_id, sound_id: d.id });
        }
        if ui.button(format!("{}  Remove from project", icon::FOLDER_MINUS)).clicked() {
            actions.push(Action::RemoveFromProject { project_id, sound_id: d.id });
        }
    }
    if ui.button(egui::RichText::new(format!("{}  Delete…", icon::TRASH)).color(danger)).clicked() {
        actions.push(Action::AskDelete(d.id));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, vec2, RawInput};
    use sfxc_store::{MemberOptions, Project};

    fn member(project_id: i64) -> Membership {
        Membership {
            project_id,
            sound_id: 7,
            sound_name: "laser".into(),
            tags: String::new(),
            updated_at: 0,
            rel_path: "laser.wav".into(),
            options: MemberOptions::default(),
            exported_version_id: None,
            current_version_id: Some(1),
            draft_dirty: false,
        }
    }

    fn project(id: i64) -> ProjectView {
        let project = Project { id, name: format!("Game {id}"), root: "/tmp".into(), created_at: 0 };
        ProjectView { summary: ProjectSummary { project, sounds: 1, changed: 1 }, members: vec![member(id)] }
    }

    /// The same sound in two sections used to share one rename field id.
    #[test]
    fn a_sound_in_two_projects_is_renamed_in_one_row_only() {
        let ctx = egui::Context::default();
        crate::ui::theme::install(&ctx);
        let projects = [project(1), project(2)];
        let status = ProjectStatus::default();
        let (errors, collapsed) = (HashMap::new(), HashSet::new());
        let view = LibraryView { sounds: &[], unassigned: &[], projects: &projects, status: &status, export_errors: &errors, exporting: None, collapsed: &collapsed };
        let mut prefs = Prefs { theme: ThemeChoice::Auto, scale: 1.0, volume: 0.5, accent_hue: accent::DEFAULT_HUE, autoplay: true };
        let (mut search, mut actions) = (String::new(), Vec::new());
        let mut frame = |t: f64| {
            let input = RawInput { screen_rect: Some(egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 900.0))), time: Some(t), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| show(ui, &view, &mut search, Some(7), 0, &mut prefs, &mut actions));
            out.textures_delta.clear();
        };
        frame(0.0);
        ctx.data_mut(|d| d.insert_temp(rename_key(), Rename { section: Section::Project(2), id: 7, text: "laser".into(), started: true }));
        frame(0.1);
        frame(0.2);
        assert_eq!(ctx.memory(|m| m.focused()), Some(rename_field_id(Section::Project(2), 7)));
        assert_ne!(rename_field_id(Section::Project(1), 7), rename_field_id(Section::Project(2), 7));
        assert!(actions.is_empty());
    }
}
