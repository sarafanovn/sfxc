use eframe::egui;
use egui_phosphor::regular as icon;
use sfxc_core::export::{ExportFormat, ExportOptions};

use super::widgets::{button, dialog, param, row_label, segmented, toggle, Kind};

/// Export settings; kept between exports for the session.
pub struct ExportDialog {
    pub ogg: bool,
    pub wav_bits: u16,
    pub ogg_quality: f32,
    pub sample_rate: u32,
    pub normalize: bool,
    pub trim: bool,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self { ogg: false, wav_bits: 16, ogg_quality: 6.0, sample_rate: 44_100, normalize: true, trim: true }
    }
}

impl ExportDialog {
    pub fn options(&self) -> ExportOptions {
        ExportOptions {
            format: if self.ogg { ExportFormat::Ogg { quality: self.ogg_quality } } else { ExportFormat::Wav { bits: self.wav_bits } },
            sample_rate: self.sample_rate,
            normalize: self.normalize,
            trim: self.trim,
        }
    }
}

pub enum Outcome {
    Open,
    Cancel,
    Export,
}

pub fn show(ctx: &egui::Context, d: &mut ExportDialog) -> Outcome {
    let mut outcome = Outcome::Open;
    let modal = egui::Modal::new(egui::Id::new("export")).show(ctx, |ui| {
        let clicked = dialog(
            ui,
            "Export sound",
            |ui| {
                row(ui, "Format", |ui| {
                    segmented(ui, &mut d.ogg, &[(false, "WAV"), (true, "OGG")]);
                });
                row(ui, "Sample rate", |ui| {
                    segmented(ui, &mut d.sample_rate, &[(22_050, "22.05 kHz"), (44_100, "44.1 kHz"), (48_000, "48 kHz")]);
                });
                if d.ogg {
                    ui.add_space(2.0);
                    param(ui, "Quality", &mut d.ogg_quality, (0.0, 10.0), 6.0, "", false);
                } else {
                    row(ui, "Bit depth", |ui| {
                        segmented(ui, &mut d.wav_bits, &[(8, "8-bit"), (16, "16-bit"), (24, "24-bit")]);
                    });
                }
                ui.add_space(6.0);
                toggle(ui, &mut d.normalize, "Normalize to −1 dBFS");
                toggle(ui, &mut d.trim, "Trim trailing silence");
            },
            |ui| {
                let export = button(ui, Kind::Primary, Some(icon::EXPORT), "Export…").clicked();
                let cancel = button(ui, Kind::Secondary, None, "Cancel").clicked();
                (export, cancel)
            },
        );
        match clicked {
            (true, _) => outcome = Outcome::Export,
            (_, true) => outcome = Outcome::Cancel,
            _ => {}
        }
    });
    if matches!(outcome, Outcome::Open) && modal.should_close() {
        outcome = Outcome::Cancel;
    }
    outcome
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        row_label(ui, label);
        add(ui);
    });
}
