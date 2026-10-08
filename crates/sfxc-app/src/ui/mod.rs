//! Main window: wires store, renderer, player and the panels together.

mod accent;
mod arp;
mod controls;
mod editor;
mod effects;
mod export_dialog;
mod library;
mod material;
mod theme;
mod versions;
mod widgets;

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use eframe::egui;
use sfxc_core::export::export_to_path;
use sfxc_core::generators::{self, Category};
use sfxc_core::patch::{Mode, SoundPatch};

use crate::audio::{self, Player};
use crate::history::History;
use crate::render_worker::{RenderJob, RenderResult, RenderWorker};
use crate::store::{SoundSummary, Store, VersionInfo};
use export_dialog::{ExportDialog, Outcome};
use library::Prefs;
use theme::ThemeChoice;
use widgets::{button, dialog, Kind};

/// Commit an automatic version after this long with unversioned changes.
const AUTO_VERSION_SECS: i64 = 300;
const AUTO_VERSION_CHECK: Duration = Duration::from_secs(10);
/// Debounce for writing the draft to disk.
const DRAFT_SAVE_DELAY: Duration = Duration::from_millis(500);
const TOAST_TIME: Duration = Duration::from_secs(4);
/// Height of the custom title bar drawn under the macOS traffic lights.
const TITLEBAR_H: f32 = 30.0;

pub fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Renames sound `id`. Blank names are ignored; if `id` is open, its header is
/// synced (restored to the stored name on blank input). Returns true if the DB changed.
fn apply_rename(store: &Store, current: Option<&mut Current>, id: i64, name: &str, now: i64) -> Result<bool> {
    let name = name.trim();
    let cur = current.filter(|c| c.id == id);
    if name.is_empty() {
        if let Some(cur) = cur {
            cur.name = store.sound_meta(id)?.0;
        }
        return Ok(false);
    }
    store.rename_sound(id, name, now)?;
    if let Some(cur) = cur {
        cur.name = name.to_string();
    }
    Ok(true)
}

fn fresh_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

/// Things panels ask for; applied after the frame's UI is drawn.
pub enum Action {
    NewSound,
    RefreshList,
    Open(i64),
    DuplicateSound(i64),
    AskDelete(i64),
    RenameSound(i64, String),
    SetTags(String),
    Play,
    Generate(Category),
    Mutate,
    Undo,
    Redo,
    AskVersionNote,
    Restore(i64),
    DuplicateVersion(i64),
    DuplicateCurrent,
    Export,
    SetTheme(ThemeChoice),
    SetScale(f32),
    SaveVolume,
    SetAccent { hue: f32, persist: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Success,
    Error,
}

struct Toast {
    msg: String,
    kind: ToastKind,
    at: Instant,
}

pub struct Current {
    pub id: i64,
    pub name: String,
    pub tags: String,
    pub patch: SoundPatch,
    saved_json: String,
    changed_at: Option<Instant>,
}

pub struct SfxcApp {
    ctx: egui::Context,
    prefs: Prefs,
    db_path: PathBuf,
    store: Option<Store>,
    store_error: Option<String>,
    player: Player,
    worker: RenderWorker,
    sounds: Vec<SoundSummary>,
    search: String,
    current: Option<Current>,
    current_version: Option<i64>,
    versions: Vec<VersionInfo>,
    history: History,
    /// Patch as it was before the in-progress edit (slider drag); pushed to history on release.
    edit_base: Option<SoundPatch>,
    generation: u64,
    rendered: Option<RenderResult>,
    play_when_ready: Option<u64>,
    autoplay: bool,
    mode_note: Option<String>,
    export_settings: ExportDialog,
    export_open: bool,
    note_prompt: Option<String>,
    confirm_delete: Option<i64>,
    toasts: Vec<Toast>,
    last_auto_check: Instant,
}

impl SfxcApp {
    pub fn new(cc: &eframe::CreationContext<'_>, db_path: PathBuf) -> Self {
        theme::install(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let worker = RenderWorker::spawn(move || ctx.request_repaint());
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            prefs: Prefs { theme: ThemeChoice::Auto, scale: 1.0, volume: audio::DEFAULT_VOLUME, accent_hue: accent::DEFAULT_HUE },
            db_path,
            store: None,
            store_error: None,
            player: Player::new(),
            worker,
            sounds: Vec::new(),
            search: String::new(),
            current: None,
            current_version: None,
            versions: Vec::new(),
            history: History::new(200),
            edit_base: None,
            generation: 0,
            rendered: None,
            play_when_ready: None,
            autoplay: true,
            mode_note: None,
            export_settings: ExportDialog::default(),
            export_open: false,
            note_prompt: None,
            confirm_delete: None,
            toasts: Vec::new(),
            last_auto_check: Instant::now(),
        };
        app.open_store();
        app.apply_prefs();
        app
    }

    // ---- preferences -------------------------------------------------------

    fn apply_prefs(&self) {
        self.ctx.set_theme(self.prefs.theme.preference());
        self.ctx.set_zoom_factor(self.prefs.scale);
        theme::set_accent(&self.ctx, self.prefs.accent_hue);
    }

    fn load_prefs(&mut self) {
        let Some(store) = &self.store else { return };
        if let Some(t) = store.setting("theme").ok().flatten().and_then(|k| ThemeChoice::from_key(&k)) {
            self.prefs.theme = t;
        }
        if let Some(s) = store.setting("ui_scale").ok().flatten().and_then(|v| v.parse::<f32>().ok()) {
            self.prefs.scale = theme::UI_SCALES.iter().map(|(v, _)| *v).find(|v| (v - s).abs() < 0.01).unwrap_or(1.0);
        }
        if let Some(v) = store.setting("playback_volume").ok().flatten().and_then(|s| audio::parse_volume(&s)) {
            self.prefs.volume = v;
        }
        if let Some(h) = store.setting("accent_hue").ok().flatten().and_then(|s| accent::parse_hue(&s)) {
            self.prefs.accent_hue = h;
        }
        self.player.set_volume(self.prefs.volume);
    }

    fn save_pref(&mut self, key: &str, value: &str) {
        let Some(store) = &self.store else { return };
        let r = store.set_setting(key, value);
        self.check(r);
    }

    // ---- library / store -------------------------------------------------

    fn open_store(&mut self) {
        match Store::open(&self.db_path) {
            Ok(store) => {
                self.store = Some(store);
                self.store_error = None;
                self.load_prefs();
                self.refresh_list();
                if let Some(first) = self.sounds.first().map(|s| s.id) {
                    self.open_sound(first);
                }
            }
            Err(e) => {
                self.store = None;
                self.store_error = Some(format!("{e:#}"));
            }
        }
    }

    /// Moves a damaged library aside (never deletes it) and starts a fresh one.
    fn start_new_library(&mut self) {
        let stamp = now_secs();
        let base = self.db_path.display().to_string();
        for suffix in ["", "-wal", "-shm"] {
            let from = PathBuf::from(format!("{base}{suffix}"));
            if from.exists() {
                let to = PathBuf::from(format!("{base}.broken-{stamp}{suffix}"));
                if let Err(e) = std::fs::rename(&from, &to) {
                    self.store_error = Some(format!("Could not move {} aside: {e}", from.display()));
                    return;
                }
            }
        }
        self.open_store();
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push(Toast { msg: msg.into(), kind: ToastKind::Error, at: Instant::now() });
    }

    fn toast_ok(&mut self, msg: impl Into<String>) {
        self.toasts.push(Toast { msg: msg.into(), kind: ToastKind::Success, at: Instant::now() });
    }

    fn check<T>(&mut self, r: Result<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.toast(format!("{e:#}"));
                None
            }
        }
    }

    fn refresh_list(&mut self) {
        let Some(store) = &self.store else { return };
        let r = store.list_sounds(&self.search);
        if let Some(list) = self.check(r) {
            self.sounds = list;
        }
    }

    fn refresh_versions(&mut self) {
        let (Some(store), Some(id)) = (&self.store, self.current.as_ref().map(|c| c.id)) else {
            self.versions.clear();
            return;
        };
        let r = store.list_versions(id).and_then(|v| Ok((v, store.current_version_id(id)?)));
        if let Some((versions, current)) = self.check(r) {
            self.versions = versions;
            self.current_version = current;
        }
    }

    fn open_sound(&mut self, id: i64) {
        self.flush_draft();
        let Some(store) = &self.store else { return };
        let r = store.sound_meta(id).and_then(|(name, tags)| {
            let patch = store.load_draft(id)?;
            Ok(Current { id, name, tags, saved_json: patch.to_json(), patch, changed_at: None })
        });
        if let Some(cur) = self.check(r) {
            self.current = Some(cur);
            self.history.clear();
            self.edit_base = None;
            self.mode_note = None;
            self.refresh_versions();
            self.request_render();
        }
    }

    fn new_sound(&mut self) {
        let Some(store) = &self.store else { return };
        let name = format!("sound {}", self.sounds.len() + 1);
        let patch = generators::generate(Category::BlipSelect, Mode::Modern, fresh_seed());
        let r = store.create_sound(&name, &patch, now_secs());
        if let Some(id) = self.check(r) {
            self.search.clear();
            self.refresh_list();
            self.open_sound(id);
        }
    }

    fn flush_draft(&mut self) {
        let result = match (&self.store, self.current.as_mut()) {
            (Some(store), Some(cur)) => {
                let json = cur.patch.to_json();
                if json == cur.saved_json {
                    cur.changed_at = None;
                    Ok(false)
                } else {
                    store.save_draft(cur.id, &cur.patch, now_secs()).map(|_| {
                        cur.saved_json = json;
                        cur.changed_at = None;
                        true
                    })
                }
            }
            _ => Ok(false),
        };
        match result {
            Ok(true) => self.refresh_list(),
            Ok(false) => {}
            Err(e) => self.toast(format!("Could not save draft: {e:#}")),
        }
    }

    fn commit_version(&mut self, note: &str, exported: bool) {
        self.flush_draft();
        let (Some(store), Some(cur)) = (&self.store, &self.current) else { return };
        let r = store.commit_version(cur.id, note, exported, now_secs());
        if self.check(r).is_some() {
            self.refresh_versions();
        }
    }

    fn maybe_auto_version(&mut self) {
        let (Some(store), Some(cur)) = (&self.store, &self.current) else { return };
        let due = matches!(store.last_version_time(cur.id), Ok(Some(t)) if now_secs() - t >= AUTO_VERSION_SECS);
        if due && store.draft_differs_from_latest(cur.id).unwrap_or(false) {
            self.commit_version("auto", false);
        }
    }

    fn restore(&mut self, version_id: i64) {
        self.flush_draft();
        let (Some(store), Some(cur)) = (&self.store, &self.current) else { return };
        let r = store.restore_version(cur.id, version_id, now_secs());
        if let Some(patch) = self.check(r) {
            if let Some(cur) = self.current.as_mut() {
                cur.saved_json = patch.to_json();
            }
            self.set_patch(patch, true);
            self.refresh_versions();
        }
    }

    fn duplicate_version(&mut self, version_id: i64) {
        let Some(store) = &self.store else { return };
        let name = self.current.as_ref().map_or_else(|| "copy".to_string(), |c| format!("{} copy", c.name));
        let r = store.duplicate_from_version(version_id, &name, now_secs());
        if let Some(id) = self.check(r) {
            self.refresh_list();
            self.open_sound(id);
        }
    }

    fn duplicate_current(&mut self) {
        self.commit_version("", false);
        if let Some(v) = self.current_version {
            self.duplicate_version(v);
        }
    }

    fn duplicate_sound(&mut self, id: i64) {
        if self.current.as_ref().map(|c| c.id) == Some(id) {
            return self.duplicate_current();
        }
        let Some(store) = &self.store else { return };
        let r = store.current_version_id(id).and_then(|v| {
            let v = v.ok_or_else(|| anyhow::anyhow!("sound has no versions"))?;
            let (name, _) = store.sound_meta(id)?;
            store.duplicate_from_version(v, &format!("{name} copy"), now_secs())
        });
        if let Some(new_id) = self.check(r) {
            self.refresh_list();
            self.open_sound(new_id);
        }
    }

    fn delete_sound(&mut self, id: i64) {
        let Some(store) = &self.store else { return };
        let r = store.delete_sound(id);
        if self.check(r).is_some() {
            if self.current.as_ref().map(|c| c.id) == Some(id) {
                self.current = None;
                self.versions.clear();
                self.rendered = None;
            }
            self.refresh_list();
        }
    }

    fn rename_sound(&mut self, id: i64, name: String) {
        let Some(store) = &self.store else { return };
        let r = apply_rename(store, self.current.as_mut(), id, &name, now_secs());
        if self.check(r) == Some(true) {
            self.refresh_list();
        }
    }

    fn set_tags(&mut self, tags: String) {
        let (Some(store), Some(cur)) = (&self.store, &self.current) else { return };
        let r = store.set_tags(cur.id, tags.trim(), now_secs());
        if self.check(r).is_some() {
            self.refresh_list();
        }
    }

    fn run_export(&mut self) {
        let opts = self.export_settings.options();
        let (Some(store), Some(cur)) = (&self.store, &self.current) else { return };
        let ext = opts.format.extension();
        let file_name: String = cur.name.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '_' } else { c }).collect();
        let mut dialog = rfd::FileDialog::new()
            .set_file_name(format!("{file_name}.{ext}"))
            .add_filter(ext.to_uppercase(), &[ext]);
        if let Some(dir) = store.export_dir(cur.id).ok().flatten().map(PathBuf::from).filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else { return };
        match export_to_path(&cur.patch, &opts, &path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    let _ = store.set_export_dir(cur.id, &parent.to_string_lossy());
                }
                self.commit_version("", true);
                self.toast_ok(format!("Exported {}", path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())));
            }
            Err(e) => self.toast(format!("Export failed: {e:#}")),
        }
    }

    // ---- editing / playback ------------------------------------------------

    fn request_render(&mut self) {
        let Some(cur) = &self.current else { return };
        self.generation += 1;
        self.worker.request(RenderJob {
            generation: self.generation,
            patch: cur.patch.clone(),
            sample_rate: self.player.sample_rate(),
        });
    }

    fn play(&mut self) {
        match &self.rendered {
            Some(r) if r.generation == self.generation => self.player.play(r.samples.clone()),
            _ => self.play_when_ready = Some(self.generation),
        }
    }

    /// Replaces the whole patch (generators, mutate, undo/redo, restore).
    fn set_patch(&mut self, patch: SoundPatch, record_undo: bool) {
        let Some(cur) = self.current.as_mut() else { return };
        if record_undo {
            self.history.push(cur.patch.clone());
        }
        cur.patch = patch;
        cur.changed_at = Some(Instant::now());
        self.mode_note = None;
        self.request_render();
        if self.autoplay {
            self.play();
        }
    }

    fn generate(&mut self, category: Category) {
        let Some(cur) = &self.current else { return };
        let mut p = generators::generate(category, cur.patch.mode, fresh_seed());
        p.master_effects = cur.patch.master_effects.clone();
        self.set_patch(p, true);
    }

    fn mutate(&mut self) {
        let Some(cur) = &self.current else { return };
        let p = generators::mutate(&cur.patch, fresh_seed());
        self.set_patch(p, true);
    }

    fn undo(&mut self) {
        let Some(cur) = &self.current else { return };
        let now = cur.patch.clone();
        if let Some(p) = self.history.undo(&now) {
            self.set_patch(p, false);
        }
    }

    fn redo(&mut self) {
        let Some(cur) = &self.current else { return };
        let now = cur.patch.clone();
        if let Some(p) = self.history.redo(&now) {
            self.set_patch(p, false);
        }
    }

    /// Called every frame after the editor. `before` is the patch before this frame's
    /// edits (None if nothing changed). An edit gesture ends when the pointer is released.
    fn after_edit(&mut self, before: Option<SoundPatch>, ctx: &egui::Context) {
        if let Some(before) = before {
            self.edit_base.get_or_insert(before);
            if let Some(cur) = self.current.as_mut() {
                cur.changed_at = Some(Instant::now());
            }
            self.request_render();
        }
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        if let Some(base) = self.edit_base.take()
            && self.current.as_ref().is_some_and(|c| c.patch != base)
        {
            self.history.push(base);
            if self.autoplay {
                self.play();
            }
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::NewSound => self.new_sound(),
            Action::RefreshList => self.refresh_list(),
            Action::Open(id) => self.open_sound(id),
            Action::DuplicateSound(id) => self.duplicate_sound(id),
            Action::AskDelete(id) => self.confirm_delete = Some(id),
            Action::RenameSound(id, name) => self.rename_sound(id, name),
            Action::SetTags(tags) => self.set_tags(tags),
            Action::Play => self.play(),
            Action::Generate(c) => self.generate(c),
            Action::Mutate => self.mutate(),
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::AskVersionNote => {
                if self.current.is_some() {
                    self.note_prompt = Some(String::new());
                }
            }
            Action::Restore(v) => self.restore(v),
            Action::DuplicateVersion(v) => self.duplicate_version(v),
            Action::DuplicateCurrent => self.duplicate_current(),
            Action::Export => self.export_open = self.current.is_some(),
            Action::SetTheme(t) => {
                self.prefs.theme = t;
                self.apply_prefs();
                self.save_pref("theme", t.key());
            }
            Action::SetScale(v) => {
                self.prefs.scale = v;
                self.apply_prefs();
                self.save_pref("ui_scale", &v.to_string());
            }
            Action::SetAccent { hue, persist } => {
                self.prefs.accent_hue = hue;
                theme::set_accent(&self.ctx, hue);
                if persist {
                    self.save_pref("accent_hue", &hue.to_string());
                }
            }
            Action::SaveVolume => {
                let v = self.prefs.volume.to_string();
                self.save_pref("playback_volume", &v);
            }
        }
    }

    // ---- per-frame plumbing --------------------------------------------------

    fn poll(&mut self, ctx: &egui::Context) {
        self.player.maintain();
        if !self.player.is_available() {
            ctx.request_repaint_after(Duration::from_secs(3));
        }
        if let Some(r) = self.worker.latest() {
            if self.play_when_ready.is_some_and(|g| r.generation >= g) {
                self.player.play(r.samples.clone());
                self.play_when_ready = None;
            }
            self.rendered = Some(r);
        }
        // Device sample rate changed (e.g. after reconnect): re-render for playback.
        let stale_rate = self
            .rendered
            .as_ref()
            .is_some_and(|r| r.generation == self.generation && r.sample_rate != self.player.sample_rate());
        if stale_rate {
            self.request_render();
        }
    }

    fn hotkeys(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        use egui::{Key, Modifiers};
        let typing = ctx.egui_wants_keyboard_input();
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND, Key::N) {
                actions.push(Action::NewSound);
            }
            if i.consume_key(Modifiers::COMMAND, Key::S) {
                actions.push(Action::AskVersionNote);
            }
            if i.consume_key(Modifiers::COMMAND, Key::E) {
                actions.push(Action::Export);
            }
            if i.consume_key(Modifiers::COMMAND, Key::D) {
                actions.push(Action::DuplicateCurrent);
            }
            if typing {
                return; // text fields keep their own undo, space and letters
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) {
                actions.push(Action::Redo);
            }
            if i.consume_key(Modifiers::COMMAND, Key::Z) {
                actions.push(Action::Undo);
            }
            if i.consume_key(Modifiers::NONE, Key::Space) {
                actions.push(Action::Play);
            }
            if i.consume_key(Modifiers::NONE, Key::M) {
                actions.push(Action::Mutate);
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.export_open {
            match export_dialog::show(ctx, &mut self.export_settings) {
                Outcome::Open => {}
                Outcome::Cancel => self.export_open = false,
                Outcome::Export => {
                    self.export_open = false;
                    self.run_export();
                }
            }
        }
        if let Some(note) = self.note_prompt.as_mut() {
            let mut done = None;
            let modal = egui::Modal::new(egui::Id::new("version_note")).show(ctx, |ui| {
                let mut enter = false;
                let (save, cancel) = dialog(
                    ui,
                    "Save version",
                    |ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(note)
                                .hint_text("What changed? (optional)")
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::symmetric(8, 6)),
                        );
                        r.request_focus();
                        enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    },
                    |ui| {
                        let save = button(ui, Kind::Primary, None, "Save").clicked();
                        (save, button(ui, Kind::Secondary, None, "Cancel").clicked())
                    },
                );
                if save || enter {
                    done = Some(true);
                } else if cancel {
                    done = Some(false);
                }
            });
            if done.is_none() && modal.should_close() {
                done = Some(false);
            }
            if let Some(save) = done {
                let note = self.note_prompt.take().unwrap_or_default();
                if save {
                    self.commit_version(note.trim(), false);
                }
            }
        }
        if let Some(id) = self.confirm_delete {
            let name = self.sounds.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default();
            let mut done = None;
            let modal = egui::Modal::new(egui::Id::new("confirm_delete")).show(ctx, |ui| {
                let (yes, no) = dialog(
                    ui,
                    &format!("Delete “{name}”?"),
                    |ui| {
                        let muted = theme::palette(ui).muted;
                        ui.add(egui::Label::new(egui::RichText::new("The sound and all its versions will be removed. This cannot be undone.").color(muted)).wrap());
                    },
                    |ui| {
                        let yes = button(ui, Kind::Danger, None, "Delete").clicked();
                        (yes, button(ui, Kind::Secondary, None, "Cancel").clicked())
                    },
                );
                if yes {
                    done = Some(true);
                } else if no {
                    done = Some(false);
                }
            });
            if done.is_none() && modal.should_close() {
                done = Some(false);
            }
            if let Some(yes) = done {
                self.confirm_delete = None;
                if yes {
                    self.delete_sound(id);
                }
            }
        }
    }

    fn housekeeping(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.current.as_ref().and_then(|c| c.changed_at) {
            if t.elapsed() >= DRAFT_SAVE_DELAY {
                self.flush_draft();
            } else {
                ctx.request_repaint_after(DRAFT_SAVE_DELAY);
            }
        }
        if self.last_auto_check.elapsed() >= AUTO_VERSION_CHECK {
            self.last_auto_check = Instant::now();
            self.maybe_auto_version();
        }
        ctx.request_repaint_after(AUTO_VERSION_CHECK);
        if ctx.input(|i| i.viewport().close_requested()) {
            self.flush_draft();
        }
    }

    fn show_toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.at.elapsed() < TOAST_TIME);
        if self.toasts.is_empty() {
            return;
        }
        let p = theme::palette_of(ctx);
        egui::Area::new(egui::Id::new("toasts")).anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0]).show(ctx, |ui| {
            for t in &self.toasts {
                // Fade in quickly, fade out over the last 400 ms.
                let age = t.at.elapsed().as_secs_f32();
                let left = TOAST_TIME.as_secs_f32() - age;
                ui.set_opacity((age / 0.15).min(1.0).min(left / 0.4).clamp(0.0, 1.0));
                let (glyph, color) = match t.kind {
                    ToastKind::Success => (egui_phosphor::regular::CHECK_CIRCLE, p.accent),
                    ToastKind::Error => (egui_phosphor::regular::WARNING_CIRCLE, p.danger),
                };
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(theme::R_CARD)
                    .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: p.shadow })
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(glyph).color(color).size(17.0));
                            ui.add(egui::Label::new(egui::RichText::new(&t.msg).color(p.text)).wrap());
                        });
                    });
                ui.add_space(8.0);
            }
        });
        ctx.request_repaint_after(Duration::from_millis(30));
    }

    fn title_bar(&self, ui: &mut egui::Ui) {
        let p = theme::palette_of(ui.ctx());
        egui::Panel::top("titlebar")
            .exact_size(TITLEBAR_H)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.chrome))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let resp = ui.interact(rect, egui::Id::new("titlebar_drag"), egui::Sense::click_and_drag());
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if resp.double_clicked() {
                    let max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                let title = self.current.as_ref().map_or("sfxc", |c| c.name.as_str());
                ui.painter().text(
                    egui::pos2(rect.center().x, rect.top() + 14.0),
                    egui::Align2::CENTER_CENTER,
                    title,
                    egui::FontId::new(12.5, theme::semibold()),
                    p.muted,
                );
            });
    }

    fn library_error_screen(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette_of(ui.ctx());
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.canvas)).show(ui, |ui| {
            ui.add_space((ui.available_height() / 2.0 - 160.0).max(24.0));
            ui.vertical_centered(|ui| {
                ui.set_max_width(460.0);
                widgets::material_card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    widgets::banner(ui, widgets::Tone::Danger, egui_phosphor::regular::DATABASE, "Could not open the sound library", false);
                    ui.add_space(12.0);
                    ui.add(egui::Label::new(egui::RichText::new(self.store_error.as_deref().unwrap_or("unknown error")).color(p.text)).wrap());
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(egui::RichText::new(self.db_path.display().to_string()).monospace().color(p.muted)).wrap());
                    ui.add_space(12.0);
                    ui.add(egui::Label::new(egui::RichText::new("The existing file has not been modified. Retry, or move it aside and start a new library.").color(p.muted)).wrap());
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if button(ui, Kind::Primary, None, "Retry").clicked() {
                            self.open_store();
                        }
                        if button(ui, Kind::Secondary, None, "Start a new library").clicked() {
                            self.start_new_library();
                        }
                    });
                });
            });
        });
    }
}

impl eframe::App for SfxcApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.title_bar(ui);
        if self.store.is_none() {
            self.library_error_screen(ui);
            return;
        }
        self.poll(&ctx);
        let mut actions = Vec::new();
        self.hotkeys(&ctx, &mut actions);
        let now = now_secs();
        let p = theme::palette_of(&ctx);
        let side = |left: i8| egui::Frame::new().fill(p.chrome).inner_margin(egui::Margin { left, right: 12, top: 6, bottom: 8 });

        egui::Panel::left("library").resizable(true).default_size(230.0).size_range(190.0..=360.0).frame(side(10)).show(ui, |ui| {
            let current = self.current.as_ref().map(|c| c.id);
            library::show(ui, &self.sounds, &mut self.search, current, now, &self.prefs, &mut actions);
        });
        egui::Panel::right("versions").resizable(true).default_size(250.0).size_range(210.0..=380.0).frame(side(14)).show(ui, |ui| {
            versions::show(ui, &self.versions, self.current_version, self.current.is_some(), now, &mut actions);
        });
        let progress = self.player.progress();
        if progress.is_some() {
            ctx.request_repaint();
        }
        let mut before = None;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.canvas)).show(ui, |ui| match self.current.as_mut() {
            Some(cur) => {
                let snapshot = cur.patch.clone();
                let view = editor::View {
                    rendered: self.rendered.as_ref().map(|r| (r.samples.as_slice(), r.sample_rate)),
                    progress,
                    can_undo: self.history.can_undo(),
                    can_redo: self.history.can_redo(),
                    audio_error: self.player.error(),
                };
                editor::show(ui, cur, &view, &mut self.autoplay, &mut self.prefs.volume, &mut self.mode_note, &mut actions);
                self.player.set_volume(self.prefs.volume);
                if cur.patch != snapshot {
                    before = Some(snapshot);
                }
            }
            None => {
                ui.add_space((ui.available_height() / 2.0 - 120.0).max(24.0));
                widgets::empty_state(
                    ui,
                    egui_phosphor::regular::WAVEFORM,
                    "No sound open",
                    "Create a sound or pick one from the library. Press ⌘N to start.",
                    |ui| {
                        if button(ui, Kind::Primary, Some(egui_phosphor::regular::PLUS), "New sound").clicked() {
                            actions.push(Action::NewSound);
                        }
                    },
                );
            }
        });

        self.after_edit(before, &ctx);
        self.dialogs(&ctx);
        for a in actions {
            self.apply(a);
        }
        self.housekeeping(&ctx);
        self.show_toasts(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current(id: i64, name: &str) -> Current {
        Current {
            id,
            name: name.into(),
            tags: String::new(),
            patch: SoundPatch::default(),
            saved_json: String::new(),
            changed_at: None,
        }
    }

    #[test]
    fn rename_other_sound_leaves_editor_alone() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_sound("a", &SoundPatch::default(), 1).unwrap();
        let b = s.create_sound("b", &SoundPatch::default(), 1).unwrap();
        let mut cur = current(a, "a");
        assert!(apply_rename(&s, Some(&mut cur), b, "  boom ", 2).unwrap());
        assert_eq!(s.sound_meta(b).unwrap().0, "boom");
        assert_eq!(cur.name, "a");
    }

    #[test]
    fn rename_current_updates_header() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_sound("a", &SoundPatch::default(), 1).unwrap();
        let mut cur = current(a, "a");
        assert!(apply_rename(&s, Some(&mut cur), a, "laser", 2).unwrap());
        assert_eq!(cur.name, "laser");
        assert_eq!(s.sound_meta(a).unwrap().0, "laser");
    }

    #[test]
    fn rename_ignores_blank_and_restores_header() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_sound("a", &SoundPatch::default(), 1).unwrap();
        let mut cur = current(a, "   ");
        assert!(!apply_rename(&s, Some(&mut cur), a, "   ", 2).unwrap());
        assert_eq!(cur.name, "a");
        assert_eq!(s.sound_meta(a).unwrap().0, "a");
    }
}
