use eframe::egui;

use super::Action;
use crate::store::VersionInfo;

pub fn show(ui: &mut egui::Ui, versions: &[VersionInfo], current: Option<i64>, now: i64, actions: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Versions");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Save").on_hover_text("⌘S").clicked() {
                actions.push(Action::AskVersionNote);
            }
        });
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.label(egui::RichText::new("● Draft (autosaved)").strong());
        ui.add_space(4.0);
        let total = versions.len();
        for (i, v) in versions.iter().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.strong(format!("v{}", total - i));
                    if current == Some(v.id) {
                        ui.weak("current");
                    }
                    if v.exported_at.is_some() {
                        ui.label("exported");
                    }
                    if v.broken {
                        ui.colored_label(ui.visuals().error_fg_color, "unreadable");
                    }
                });
                ui.weak(age(now - v.created_at));
                if !v.note.is_empty() {
                    ui.label(&v.note);
                }
                ui.add_enabled_ui(!v.broken, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("Restore").clicked() {
                            actions.push(Action::Restore(v.id));
                        }
                        if ui.small_button("Duplicate as new").clicked() {
                            actions.push(Action::DuplicateVersion(v.id));
                        }
                    });
                });
            });
        }
    });
}

fn age(secs: i64) -> String {
    match secs {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} h ago", s / 3600),
        s => format!("{} d ago", s / 86_400),
    }
}
