//! Dialogs for projects and for a sound's place in one.

use std::path::Path;

use eframe::egui::{self, Id, RichText, Ui};
use egui_phosphor::regular as icon;
use sfxc_store::project_export::ExportPlan;
use sfxc_store::{relative_to_root, replace_extension, MemberFormat, MemberOptions, DEFAULT_OGG_QUALITY};

use super::theme::palette;
use super::widgets::{self, button, dialog, dropdown, field, toggle, Kind};

pub enum ProjectDialog {
    New { name: String, root: String, add_sound: Option<i64>, error: Option<String> },
    Rename { id: i64, name: String, error: Option<String> },
    Folder { id: i64, name: String, root: String, error: Option<String> },
    Delete { id: i64, name: String, error: Option<String> },
}

impl ProjectDialog {
    pub fn set_error(&mut self, e: String) {
        match self {
            Self::New { error, .. } | Self::Rename { error, .. } | Self::Folder { error, .. } | Self::Delete { error, .. } => *error = Some(e),
        }
    }
}

/// `Some(true)` to apply, `Some(false)` when closed, `None` while open.
pub fn project_dialog(ctx: &egui::Context, d: &mut ProjectDialog) -> Option<bool> {
    let title = match d {
        ProjectDialog::New { .. } => "New project".to_string(),
        ProjectDialog::Rename { .. } => "Rename project".to_string(),
        ProjectDialog::Folder { name, .. } => format!("Folder of “{name}”"),
        ProjectDialog::Delete { name, .. } => format!("Delete “{name}”?"),
    };
    let (label, kind) = match d {
        ProjectDialog::New { .. } => ("Create", Kind::Primary),
        ProjectDialog::Delete { .. } => ("Delete", Kind::Danger),
        _ => ("Save", Kind::Primary),
    };
    // Enter must not delete.
    let enter_applies = !matches!(d, ProjectDialog::Delete { .. });
    let mut done = None;
    let modal = egui::Modal::new(Id::new("project_dialog")).show(ctx, |ui| {
        let (ok, cancel) = dialog(ui, &title, |ui| project_body(ui, d), |ui| {
            let ok = button(ui, kind, None, label).clicked();
            (ok, button(ui, Kind::Secondary, None, "Cancel").clicked())
        });
        if ok || (enter_applies && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            done = Some(true);
        } else if cancel {
            done = Some(false);
        }
    });
    if done.is_none() && modal.should_close() {
        done = Some(false);
    }
    done
}

fn project_body(ui: &mut Ui, d: &mut ProjectDialog) {
    let muted = palette(ui).muted;
    let note = |ui: &mut Ui, text: &str| {
        ui.add(egui::Label::new(RichText::new(text).color(muted)).wrap());
    };
    match d {
        ProjectDialog::New { name, root, error, .. } => {
            name_field(ui, name);
            folder_field(ui, root);
            error_line(ui, error.as_deref());
        }
        ProjectDialog::Rename { name, error, .. } => {
            name_field(ui, name);
            error_line(ui, error.as_deref());
        }
        ProjectDialog::Folder { root, error, .. } => {
            folder_field(ui, root);
            note(ui, "Files are not moved. Every sound is exported again on the next project export.");
            error_line(ui, error.as_deref());
        }
        ProjectDialog::Delete { error, .. } => {
            note(ui, "Sounds and files stay: the sounds remain in the library and the files remain on disk.");
            error_line(ui, error.as_deref());
        }
    }
}

fn name_field(ui: &mut Ui, name: &mut String) {
    ui.label(widgets::hint(ui, "Name"));
    let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY).hint_text("My game"));
    if ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
}

fn folder_field(ui: &mut Ui, root: &mut String) {
    ui.label(widgets::hint(ui, "Folder in your game where its sounds are written"));
    ui.horizontal(|ui| {
        let w = ui.available_width() - 110.0;
        ui.add_sized([w, 30.0], egui::TextEdit::singleline(root).hint_text("/path/to/game/assets"));
        if button(ui, Kind::Secondary, Some(icon::FOLDER_OPEN), "Choose…").clicked() {
            let mut picker = rfd::FileDialog::new();
            if Path::new(root.as_str()).is_dir() {
                picker = picker.set_directory(root.as_str());
            }
            if let Some(dir) = picker.pick_folder() {
                *root = dir.to_string_lossy().into_owned();
            }
        }
    });
}

fn error_line(ui: &mut Ui, error: Option<&str>) {
    if let Some(e) = error {
        let color = palette(ui).danger;
        ui.add(egui::Label::new(RichText::new(e).color(color)).wrap());
    }
}

pub struct MemberDialog {
    pub project_id: i64,
    pub project: String,
    pub root: String,
    pub sound_id: i64,
    pub sound: String,
    /// The path before editing; `None` when adding.
    pub previous: Option<String>,
    pub path: String,
    pub options: MemberOptions,
    /// From "Choose…" or from saving; cleared when the path is edited.
    pub error: Option<String>,
}

/// `check` validates the path against the store and returns it normalized. `Some(true)` to save.
pub fn member_dialog(ctx: &egui::Context, d: &mut MemberDialog, check: impl Fn(&str, &MemberOptions) -> Result<String, String>) -> Option<bool> {
    let verdict = check(&d.path, &d.options);
    let can_save = verdict.is_ok() && d.error.is_none();
    let title = if d.previous.is_some() { format!("“{}” in {}", d.sound, d.project) } else { format!("Add “{}” to {}", d.sound, d.project) };
    let label = if d.previous.is_some() { "Save" } else { "Add" };
    let mut done = None;
    let modal = egui::Modal::new(Id::new("member_dialog")).show(ctx, |ui| {
        let (ok, cancel) = dialog(ui, &title, |ui| member_body(ui, d, &verdict), |ui| {
            let ok = ui.add_enabled_ui(can_save, |ui| button(ui, Kind::Primary, None, label).clicked()).inner;
            (ok, button(ui, Kind::Secondary, None, "Cancel").clicked())
        });
        if ok || (can_save && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            done = Some(true);
        } else if cancel {
            done = Some(false);
        }
    });
    if done.is_none() && modal.should_close() {
        done = Some(false);
    }
    done
}

fn member_body(ui: &mut Ui, d: &mut MemberDialog, verdict: &Result<String, String>) {
    ui.label(widgets::hint(ui, format!("Path inside {}", d.root)));
    ui.horizontal(|ui| {
        let w = ui.available_width() - 110.0;
        if ui.add_sized([w, 30.0], egui::TextEdit::singleline(&mut d.path)).changed() {
            d.error = None;
        }
        if button(ui, Kind::Secondary, Some(icon::FOLDER_OPEN), "Choose…").clicked() {
            choose_file(d);
        }
    });
    match (&d.error, verdict) {
        (Some(e), _) | (None, Err(e)) => error_line(ui, Some(e.as_str())),
        (None, Ok(rel)) if d.previous.as_deref().is_some_and(|old| old != rel) => {
            ui.label(widgets::hint(ui, "The old file stays on disk."));
        }
        _ => {}
    }
    ui.add_space(6.0);
    options_editor(ui, &mut d.options, &mut d.path);
}

fn choose_file(d: &mut MemberDialog) {
    let root = Path::new(&d.root);
    let current = root.join(&d.path);
    let ext = d.options.extension();
    let mut picker = rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]);
    if let Some(dir) = current.parent().filter(|p| p.is_dir()).or(Some(root).filter(|r| r.is_dir())) {
        picker = picker.set_directory(dir);
    }
    if let Some(name) = current.file_name() {
        picker = picker.set_file_name(name.to_string_lossy().into_owned());
    }
    let Some(picked) = picker.save_file() else { return };
    match relative_to_root(root, &picked) {
        Ok(rel) => {
            d.path = rel;
            d.error = None;
        }
        Err(e) => d.error = Some(format!("{e:#}")),
    }
}

const RATES: [(u32, &str); 3] = [(22_050, "22.05 kHz"), (44_100, "44.1 kHz"), (48_000, "48 kHz")];
const BITS: [(Option<u16>, &str); 4] = [(None, "Auto"), (Some(8), "8-bit"), (Some(16), "16-bit"), (Some(24), "24-bit")];

fn options_editor(ui: &mut Ui, o: &mut MemberOptions, path: &mut String) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(20.0, 8.0);
        let mut ogg = matches!(o.format, MemberFormat::Ogg { .. });
        field(ui, "Format", |ui| {
            if dropdown(ui, "member_format", 80.0, &mut ogg, &[(false, "WAV"), (true, "OGG")]) {
                o.format = if ogg { MemberFormat::Ogg { quality: DEFAULT_OGG_QUALITY } } else { MemberFormat::Wav { bits: None } };
                *path = replace_extension(path, o.extension());
            }
        });
        match &mut o.format {
            MemberFormat::Wav { bits } => {
                field(ui, "Bits", |ui| {
                    dropdown(ui, "member_bits", 90.0, bits, &BITS);
                });
            }
            MemberFormat::Ogg { quality } => {
                field(ui, "Quality", |ui| {
                    super::controls::track(ui, quality, 0.0, 10.0, DEFAULT_OGG_QUALITY, false, 100.0);
                    ui.label(widgets::hint(ui, format!("{quality:.0}")));
                });
            }
        }
        field(ui, "Rate", |ui| {
            dropdown(ui, "member_rate", 100.0, &mut o.sample_rate, &RATES);
        });
        let mut fixed = o.duration.is_some();
        field(ui, "Length", |ui| {
            if dropdown(ui, "member_length", 80.0, &mut fixed, &[(false, "Auto"), (true, "Fixed")]) {
                o.duration = fixed.then_some(1.0);
            }
            if let Some(secs) = &mut o.duration {
                super::controls::track(ui, secs, 0.05, 10.0, 1.0, true, 120.0);
                ui.label(widgets::hint(ui, format!("{secs:.2} s")));
            }
        });
        toggle(ui, &mut o.normalize, "Normalize −1 dBFS");
        toggle(ui, &mut o.trim, "Trim silence");
    });
    ui.label(widgets::hint(ui, "Auto bits follow the sound's mode: 8, 16 or 24-bit."));
}

pub enum ExportPrompt {
    CreateRoot { plan: ExportPlan },
    Missing { plan: ExportPlan, create_root: bool },
}

pub enum PromptChoice {
    CreateRoot,
    All,
    ChangedOnly,
    Cancel,
}

pub fn export_prompt(ctx: &egui::Context, prompt: &ExportPrompt) -> Option<PromptChoice> {
    let mut choice = None;
    let modal = egui::Modal::new(Id::new("project_export_prompt")).show(ctx, |ui| {
        let muted = palette(ui).muted;
        match prompt {
            ExportPrompt::CreateRoot { plan } => {
                let (yes, no) = dialog(
                    ui,
                    "Project folder not found",
                    |ui| {
                        let text = format!("{} does not exist. It may be on a disk that is not connected.", plan.project.root);
                        ui.add(egui::Label::new(RichText::new(text).color(muted)).wrap());
                    },
                    |ui| {
                        let yes = button(ui, Kind::Primary, None, "Create and export").clicked();
                        (yes, button(ui, Kind::Secondary, None, "Cancel").clicked())
                    },
                );
                if yes {
                    choice = Some(PromptChoice::CreateRoot);
                } else if no {
                    choice = Some(PromptChoice::Cancel);
                }
            }
            ExportPrompt::Missing { plan, .. } => {
                let n = plan.missing.len();
                let title = if n == 1 { "1 file is missing on disk".to_string() } else { format!("{n} files are missing on disk") };
                let (all, changed, cancel) = dialog(
                    ui,
                    &title,
                    |ui| {
                        for m in plan.missing.iter().take(5) {
                            ui.label(RichText::new(&m.rel_path).monospace().color(muted));
                        }
                        if n > 5 {
                            ui.label(widgets::hint(ui, format!("and {} more", n - 5)));
                        }
                        ui.label(RichText::new("Export them again?").color(muted));
                    },
                    |ui| {
                        let all = button(ui, Kind::Primary, None, "Export all").clicked();
                        let changed = !plan.changed.is_empty() && button(ui, Kind::Secondary, None, "Export changed only").clicked();
                        (all, changed, button(ui, Kind::Secondary, None, "Cancel").clicked())
                    },
                );
                if all {
                    choice = Some(PromptChoice::All);
                } else if changed {
                    choice = Some(PromptChoice::ChangedOnly);
                } else if cancel {
                    choice = Some(PromptChoice::Cancel);
                }
            }
        }
    });
    if choice.is_none() && modal.should_close() {
        choice = Some(PromptChoice::Cancel);
    }
    choice
}
