use eframe::egui;

use super::Action;
use crate::store::SoundSummary;

pub fn show(ui: &mut egui::Ui, sounds: &[SoundSummary], search: &mut String, current: Option<i64>, actions: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Library");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("+ New").on_hover_text("⌘N").clicked() {
                actions.push(Action::NewSound);
            }
        });
    });
    if ui.add(egui::TextEdit::singleline(search).hint_text("Search name or tag")).changed() {
        actions.push(Action::RefreshList);
    }
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for s in sounds {
            let selected = current == Some(s.id);
            let r = ui.selectable_label(selected, &s.name);
            if r.clicked() && !selected {
                actions.push(Action::Open(s.id));
            }
            r.context_menu(|ui| {
                if ui.button("Duplicate").clicked() {
                    actions.push(Action::DuplicateSound(s.id));
                    ui.close();
                }
                if ui.button("Delete…").clicked() {
                    actions.push(Action::AskDelete(s.id));
                    ui.close();
                }
            });
        }
        if sounds.is_empty() {
            ui.weak("No sounds yet. Press + New.");
        }
    });
}
