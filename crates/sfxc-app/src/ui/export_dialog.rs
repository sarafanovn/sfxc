use eframe::egui;
use sfxc_core::export::{ExportFormat, ExportOptions};

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
        ui.heading("Export");
        ui.horizontal(|ui| {
            ui.label("Format");
            ui.selectable_value(&mut d.ogg, false, "WAV");
            ui.selectable_value(&mut d.ogg, true, "OGG");
        });
        ui.horizontal(|ui| {
            ui.label("Sample rate");
            for sr in [22_050u32, 44_100, 48_000] {
                ui.selectable_value(&mut d.sample_rate, sr, format!("{sr} Hz"));
            }
        });
        if d.ogg {
            ui.add(egui::Slider::new(&mut d.ogg_quality, 0.0..=10.0).text("Quality"));
        } else {
            ui.horizontal(|ui| {
                ui.label("Bit depth");
                for b in [8u16, 16, 24] {
                    ui.selectable_value(&mut d.wav_bits, b, format!("{b}-bit"));
                }
            });
        }
        ui.checkbox(&mut d.normalize, "Normalize to −1 dBFS");
        ui.checkbox(&mut d.trim, "Trim trailing silence");
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Export…").clicked() {
                outcome = Outcome::Export;
            }
            if ui.button("Cancel").clicked() {
                outcome = Outcome::Cancel;
            }
        });
    });
    if matches!(outcome, Outcome::Open) && modal.should_close() {
        outcome = Outcome::Cancel;
    }
    outcome
}
