mod audio;
mod history;
mod render_worker;
mod store;
mod ui;

use std::path::PathBuf;

use directories::ProjectDirs;
use eframe::egui;

/// `~/Library/Application Support/sfxc/library.db` on macOS.
fn library_path() -> PathBuf {
    ProjectDirs::from("", "", "sfxc")
        .map(|d| d.data_dir().join("library.db"))
        .unwrap_or_else(|| PathBuf::from("library.db"))
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("sfxc")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("sfxc", options, Box::new(|cc| Ok(Box::new(ui::SfxcApp::new(cc, library_path())))))
}
