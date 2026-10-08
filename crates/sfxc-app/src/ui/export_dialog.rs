use eframe::egui::{self, Align, Align2, CursorIcon, FontId, Layout, RichText, Sense, Ui};
use egui_phosphor::regular as icon;
use sfxc_core::export::{ExportFormat, ExportOptions};
use sfxc_core::patch::Mode;

use super::theme::{self, palette};
use super::widgets::{self, dropdown, field, toggle};

/// Export settings; kept between exports for the session.
pub struct ExportDialog {
    pub ogg: bool,
    pub ogg_quality: f32,
    pub sample_rate: u32,
    pub normalize: bool,
    pub trim: bool,
    /// When true the export is exactly `seconds` long instead of the sound's natural length.
    pub fixed_length: bool,
    pub seconds: f32,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            ogg: false,
            ogg_quality: 6.0,
            sample_rate: 44_100,
            normalize: true,
            trim: true,
            fixed_length: false,
            seconds: 1.0,
        }
    }
}

/// Range of the fixed export length, in seconds.
const SECONDS: (f32, f32) = (0.05, 10.0);

/// WAV bit depth that goes with a sound's mode: 8-bit exports 8-bit, 16-bit exports 16-bit, Modern exports 24-bit.
pub fn wav_bits(mode: Mode) -> u16 {
    match mode {
        Mode::Bit8 => 8,
        Mode::Bit16 => 16,
        Mode::Modern => 24,
    }
}

impl ExportDialog {
    pub fn options(&self, mode: Mode) -> ExportOptions {
        ExportOptions {
            format: if self.ogg { ExportFormat::Ogg { quality: self.ogg_quality } } else { ExportFormat::Wav { bits: wav_bits(mode) } },
            sample_rate: self.sample_rate,
            normalize: self.normalize,
            trim: self.trim,
            duration: self.fixed_length.then_some(self.seconds),
        }
    }

    /// One line describing the settings the folded panel hides (the sample rate is always in view).
    pub fn summary(&self, mode: Mode) -> String {
        let depth = if self.ogg { format!("quality {:.0}", self.ogg_quality) } else { format!("{}-bit", wav_bits(mode)) };
        let length = if self.fixed_length { format!("{:.2} s", self.seconds) } else { "auto length".to_string() };
        format!("{} · {depth} · {length}", if self.ogg { "OGG" } else { "WAV" })
    }
}

/// Foldable "Export settings" at the bottom of the main card: the header shows the format, the body the options
/// in one wrapping row. Folded by default; the state is remembered.
pub fn panel(ui: &mut Ui, d: &mut ExportDialog, natural_secs: Option<f32>) {
    let p = palette(ui);
    let id = ui.make_persistent_id("export_panel");
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    ui.horizontal(|ui| {
        state.show_toggle_button(ui, |ui, openness, resp| {
            let p = palette(ui);
            let color = if resp.hovered() { p.text } else { p.muted };
            let glyph = if openness > 0.5 { icon::CARET_DOWN } else { icon::CARET_RIGHT };
            ui.painter().text(resp.rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(14.0), color);
        });
        let title = egui::Label::new(RichText::new("Export settings").font(FontId::new(13.0, theme::semibold())).color(p.text));
        if ui.add(title.sense(Sense::click())).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            state.toggle(ui);
        }
        // Once open, the Format field below says the same.
        if !state.is_open() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(widgets::hint(ui, if d.ogg { "OGG" } else { "WAV" }));
            });
        }
    });
    state.show_body_unindented(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(20.0, 8.0);
            field(ui, "Format", |ui| {
                dropdown(ui, "export_format", 80.0, &mut d.ogg, &[(false, "WAV"), (true, "OGG")]);
            });
            if d.ogg {
                field(ui, "Quality", |ui| {
                    super::controls::track(ui, &mut d.ogg_quality, 0.0, 10.0, 6.0, false, 100.0);
                    ui.label(widgets::hint(ui, format!("{:.0}", d.ogg_quality)));
                });
            }
            field(ui, "Length", |ui| {
                if dropdown(ui, "export_length", 80.0, &mut d.fixed_length, &[(false, "Auto"), (true, "Fixed")]) && d.fixed_length {
                    // Start from the length the waveform shows, so the two agree until the slider moves.
                    if let Some(n) = natural_secs {
                        d.seconds = n.clamp(SECONDS.0, SECONDS.1);
                    }
                }
                if d.fixed_length {
                    super::controls::track(ui, &mut d.seconds, SECONDS.0, SECONDS.1, 1.0, true, 120.0);
                    ui.label(widgets::hint(ui, format!("{:.2} s", d.seconds)));
                }
            });
            toggle(ui, &mut d.normalize, "Normalize −1 dBFS");
            toggle(ui, &mut d.trim, "Trim silence");
        });
        ui.add_space(2.0);
    });
}
