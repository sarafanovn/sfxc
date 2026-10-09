//! Once-a-day release check and the "Update to …" badge in the title bar.

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;
use sfxc_update::Part;

use super::{now_secs, widgets, SfxcApp};

pub enum UpdateState {
    Idle,
    Checking(mpsc::Receiver<Option<String>>),
    Available(String),
    Installing { version: String, bundle: PathBuf, done: mpsc::Receiver<Result<(), String>> },
}

pub fn start_check(ctx: &egui::Context) -> UpdateState {
    if sfxc_update::auto_check_disabled() {
        return UpdateState::Idle;
    }
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    let cache = sfxc_store::update_cache_path();
    let spawned = std::thread::Builder::new().name("sfxc-update-check".into()).spawn(move || {
        let _ = tx.send(sfxc_update::check(&cache, now_secs() as u64, Duration::from_secs(10)));
        ctx.request_repaint();
    });
    if spawned.is_ok() { UpdateState::Checking(rx) } else { UpdateState::Idle }
}

/// The bundle install.sh manages. Only that copy is replaced in place; any other copy (a dev build, one
/// dragged to /Applications) gets the release page instead, so the update never lands somewhere unexpected.
fn managed_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let bundle = exe.parent()?.parent()?.parent()?;
    let managed = PathBuf::from(std::env::var_os("HOME")?).join("Applications/sfxc.app").canonicalize().ok()?;
    (bundle == managed).then(|| bundle.to_path_buf())
}

impl SfxcApp {
    pub(super) fn poll_update(&mut self) {
        match &self.update {
            UpdateState::Checking(rx) => match rx.try_recv() {
                Ok(Some(version)) => self.update = UpdateState::Available(version),
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => self.update = UpdateState::Idle,
                Err(mpsc::TryRecvError::Empty) => {}
            },
            UpdateState::Installing { version, bundle, done } => {
                let result = match done.try_recv() {
                    Ok(r) => r,
                    Err(mpsc::TryRecvError::Empty) => return,
                    Err(mpsc::TryRecvError::Disconnected) => Err("installer thread stopped".into()),
                };
                match result {
                    Ok(()) => self.relaunch(bundle.clone()),
                    Err(e) => {
                        self.update = UpdateState::Available(version.clone());
                        self.toast(format!("Update failed: {e}"));
                    }
                }
            }
            UpdateState::Idle | UpdateState::Available(_) => {}
        }
    }

    /// Draws the badge at the right end of the title bar; clicking it installs the update.
    pub(super) fn update_badge(&mut self, ui: &mut egui::Ui, bar: egui::Rect) {
        let text = match &self.update {
            UpdateState::Available(v) => format!("{}  Update to {v}", egui_phosphor::regular::ARROW_CIRCLE_UP),
            UpdateState::Installing { version, .. } => format!("Installing {version}…"),
            UpdateState::Idle | UpdateState::Checking(_) => return,
        };
        let area = egui::Rect::from_min_max(egui::pos2(bar.right() - 260.0, bar.top()), egui::pos2(bar.right() - 12.0, bar.bottom()));
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        let resp = ui.scope_builder(egui::UiBuilder::new().max_rect(area).layout(layout), |ui| widgets::badge(ui, &text, widgets::Tone::Accent)).inner;
        let UpdateState::Available(version) = &self.update else { return };
        let tip = if managed_bundle().is_some() {
            format!("Download {version}, install it to ~/Applications and restart sfxc")
        } else {
            format!("Open the {version} release page (this copy is not the one in ~/Applications)")
        };
        if resp.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
            self.start_install();
        }
    }

    fn start_install(&mut self) {
        let UpdateState::Available(version) = &self.update else { return };
        let version = version.clone();
        let Some(bundle) = managed_bundle() else {
            if let Err(e) = Command::new("open").arg(sfxc_update::release_page(&version)).spawn() {
                self.toast(format!("Could not open the release page: {e}"));
            }
            return;
        };
        self.flush_draft();
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("sfxc-update-install".into())
            .spawn(move || {
                let _ = tx.send(sfxc_update::install(Part::App));
                ctx.request_repaint();
            })
            .expect("spawn update thread");
        self.update = UpdateState::Installing { version, bundle, done: rx };
    }

    /// Opens the new bundle once this process has exited, then closes the window (which saves the draft).
    fn relaunch(&mut self, bundle: PathBuf) {
        self.update = UpdateState::Idle;
        let waiter = Command::new("sh")
            .args(["-c", r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; open "$2""#, "sfxc-relaunch"])
            .arg(std::process::id().to_string())
            .arg(&bundle)
            .spawn();
        match waiter {
            Ok(_) => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(_) => self.toast_ok("Updated. Restart sfxc to use the new version"),
        }
    }
}
