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

/// How this copy can update itself.
enum Updater {
    /// `~/Applications/sfxc.app`, put there by install.sh.
    Script(PathBuf),
    /// `/Applications/sfxc.app` from the Homebrew cask.
    Brew { brew: PathBuf, bundle: PathBuf },
    /// Anything else (a dev build, a copy dragged somewhere): only the release page, so an update never lands
    /// somewhere unexpected.
    Manual,
}

fn updater() -> Updater {
    let bundle = || -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
        Some(exe.parent()?.parent()?.parent()?.to_path_buf())
    };
    let Some(bundle) = bundle() else { return Updater::Manual };
    if let Some(brew) = sfxc_update::brew_for_app(&bundle) {
        return Updater::Brew { brew, bundle };
    }
    let script = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications/sfxc.app"));
    match script.and_then(|p| p.canonicalize().ok()) {
        Some(p) if p == bundle => Updater::Script(bundle),
        _ => Updater::Manual,
    }
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
        let tip = match updater() {
            Updater::Script(_) => format!("Download {version}, install it to ~/Applications and restart sfxc"),
            Updater::Brew { .. } => format!("Run `{}` and restart sfxc", sfxc_update::brew_command(Part::App)),
            Updater::Manual => format!("Open the {version} release page (this copy was not installed by install.sh or Homebrew)"),
        };
        if resp.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
            self.start_install();
        }
    }

    fn start_install(&mut self) {
        let UpdateState::Available(version) = &self.update else { return };
        let version = version.clone();
        let (bundle, brew) = match updater() {
            Updater::Script(bundle) => (bundle, None),
            Updater::Brew { brew, bundle } => (bundle, Some(brew)),
            Updater::Manual => {
                if let Err(e) = Command::new("open").arg(sfxc_update::release_page(&version)).spawn() {
                    self.toast(format!("Could not open the release page: {e}"));
                }
                return;
            }
        };
        self.flush_draft();
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("sfxc-update-install".into())
            .spawn(move || {
                let result = match brew {
                    Some(brew) => sfxc_update::brew_upgrade(&brew, Part::App),
                    None => sfxc_update::install(Part::App),
                };
                let _ = tx.send(result);
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
