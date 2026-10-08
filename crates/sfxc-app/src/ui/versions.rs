use eframe::egui;

use super::Action;
use crate::store::VersionInfo;

pub fn show(ui: &mut egui::Ui, versions: &[VersionInfo], current: Option<i64>, now: i64, actions: &mut Vec<Action>) {
    let _ = (versions, current, now, actions);
    ui.heading("Versions");
}
