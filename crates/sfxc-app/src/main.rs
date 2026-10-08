mod audio;
mod history;
mod render_worker;
mod store;
mod ui;

use std::path::PathBuf;

use directories::ProjectDirs;
use eframe::egui;

fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SFXC_LIBRARY") {
        return PathBuf::from(path);
    }
    ProjectDirs::from("", "", "sfxc")
        .map(|d| d.data_dir().join("library.db"))
        .unwrap_or_else(|| PathBuf::from("library.db"))
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("sfxc")
            .with_inner_size([1440.0, 880.0])
            .with_min_inner_size([1320.0, 780.0])
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).expect("bundled icon is a valid PNG")),
        ..Default::default()
    };
    eframe::run_native("sfxc", options, Box::new(|cc| Ok(Box::new(ui::SfxcApp::new(cc, library_path())))))
}
