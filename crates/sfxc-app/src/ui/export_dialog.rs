use eframe::egui;
use sfxc_core::export::ExportOptions;

#[derive(Default)]
pub struct ExportDialog;

impl ExportDialog {
    pub fn options(&self) -> ExportOptions {
        ExportOptions::default()
    }
}

pub enum Outcome {
    Open,
    Cancel,
    Export,
}

pub fn show(_ctx: &egui::Context, _d: &mut ExportDialog) -> Outcome {
    Outcome::Export
}
