//! The app's side of projects: keeping project data fresh and applying the dialogs.

use std::collections::HashMap;

use anyhow::{Context, Result};
use eframe::egui;
use sfxc_store::{MemberOptions, Store};

use super::library::ProjectView;
use super::projects::{self, MemberDialog, ProjectDialog};
use super::{now_secs, SfxcApp};

#[derive(Default)]
pub struct ProjectStatus {
    /// Whether the file is gone from disk, per (project, sound).
    pub missing: HashMap<(i64, i64), bool>,
    /// Unchanged sounds whose file is gone, per project: what "Export all" would add.
    pub missing_count: HashMap<i64, usize>,
    /// The projects each sound is in.
    pub member_of: HashMap<i64, Vec<i64>>,
}

/// Every project with its sounds matching `search`.
pub fn load_projects(store: &Store, search: &str) -> Result<Vec<ProjectView>> {
    store
        .list_projects()?
        .into_iter()
        .map(|summary| Ok(ProjectView { members: store.project_sounds(summary.project.id, search)?, summary }))
        .collect()
}

impl SfxcApp {
    /// Checks the disk, so it runs on events (open, outside change, export, edits, focus), not per keystroke.
    pub(super) fn refresh_project_status(&mut self) {
        let Some(store) = &self.store else { return };
        let r = (|| -> Result<ProjectStatus> {
            let mut status = ProjectStatus::default();
            for s in store.list_projects()? {
                let pid = s.project.id;
                for m in store.project_sounds(pid, "")? {
                    let gone = !m.abs_path(&s.project.root).is_file();
                    if gone && !m.changed() {
                        *status.missing_count.entry(pid).or_default() += 1;
                    }
                    status.missing.insert((pid, m.sound_id), gone);
                    status.member_of.entry(m.sound_id).or_default().push(pid);
                }
            }
            Ok(status)
        })();
        if let Some(status) = self.check(r) {
            self.project_status = status;
        }
    }

    pub(super) fn refresh_current_projects(&mut self) {
        let (Some(store), Some(id)) = (&self.store, self.current.as_ref().map(|c| c.id)) else {
            self.current_projects.clear();
            return;
        };
        let r = store.sound_projects(id);
        if let Some(list) = self.check(r) {
            self.current_projects = list;
        }
    }

    pub(super) fn after_project_change(&mut self) {
        self.refresh_list();
        self.refresh_project_status();
    }

    pub(super) fn ask_new_project(&mut self, add_sound: Option<i64>) {
        self.project_dialog = Some(ProjectDialog::New { name: String::new(), root: String::new(), add_sound, error: None });
    }

    fn project_named(&mut self, id: i64) -> Option<sfxc_store::Project> {
        let store = self.store.as_ref()?;
        let r = store.project(id);
        self.check(r)
    }

    pub(super) fn ask_rename_project(&mut self, id: i64) {
        if let Some(p) = self.project_named(id) {
            self.project_dialog = Some(ProjectDialog::Rename { id, name: p.name, error: None });
        }
    }

    pub(super) fn ask_project_folder(&mut self, id: i64) {
        if let Some(p) = self.project_named(id) {
            self.project_dialog = Some(ProjectDialog::Folder { id, name: p.name, root: p.root, error: None });
        }
    }

    pub(super) fn ask_delete_project(&mut self, id: i64) {
        if let Some(p) = self.project_named(id) {
            self.project_dialog = Some(ProjectDialog::Delete { id, name: p.name, error: None });
        }
    }

    pub(super) fn ask_add_to_project(&mut self, project_id: i64, sound_id: i64) {
        let Some(store) = &self.store else { return };
        let r = (|| -> Result<MemberDialog> {
            let project = store.project(project_id)?;
            let sound = store.sound_meta(sound_id)?.0;
            let options = MemberOptions::default();
            let path = store.default_member_path(project_id, &sound, &options)?;
            Ok(MemberDialog { project_id, project: project.name, root: project.root, sound_id, sound, previous: None, path, options, error: None })
        })();
        if let Some(d) = self.check(r) {
            self.member_dialog = Some(d);
        }
    }

    pub(super) fn ask_edit_membership(&mut self, project_id: i64, sound_id: i64) {
        let Some(store) = &self.store else { return };
        let r = (|| -> Result<MemberDialog> {
            let project = store.project(project_id)?;
            let m = store.membership(project_id, sound_id)?;
            Ok(MemberDialog {
                project_id,
                project: project.name,
                root: project.root,
                sound_id,
                sound: m.sound_name,
                previous: Some(m.rel_path.clone()),
                path: m.rel_path,
                options: m.options,
                error: None,
            })
        })();
        if let Some(d) = self.check(r) {
            self.member_dialog = Some(d);
        }
    }

    pub(super) fn remove_from_project(&mut self, project_id: i64, sound_id: i64) {
        let Some(store) = &self.store else { return };
        let r = store.remove_from_project(project_id, sound_id);
        if self.check(r).is_some() {
            self.after_project_change();
        }
    }

    /// A new sound, straight into the project with the default path.
    pub(super) fn new_sound_in(&mut self, project_id: i64) {
        self.new_sound();
        let (Some(store), Some(id)) = (&self.store, self.current.as_ref().map(|c| c.id)) else { return };
        let r = (|| -> Result<()> {
            let name = store.sound_meta(id)?.0;
            let options = MemberOptions::default();
            let path = store.default_member_path(project_id, &name, &options)?;
            store.add_to_project(project_id, id, &path, &options)
        })();
        self.check(r);
        self.after_project_change();
    }

    fn apply_project_dialog(&mut self, d: &ProjectDialog) -> Result<()> {
        let store = self.store.as_ref().context("the library is not open")?;
        match d {
            ProjectDialog::New { name, root, add_sound, .. } => {
                if root.trim().is_empty() {
                    anyhow::bail!("choose the folder in your game where sounds go");
                }
                let id = store.create_project(name, root, now_secs())?;
                self.after_project_change();
                if let Some(sound) = *add_sound {
                    self.ask_add_to_project(id, sound);
                }
            }
            ProjectDialog::Rename { id, name, .. } => {
                store.rename_project(*id, name)?;
                self.after_project_change();
            }
            ProjectDialog::Folder { id, root, .. } => {
                store.set_project_root(*id, root)?;
                self.after_project_change();
            }
            ProjectDialog::Delete { id, name, .. } => {
                store.delete_project(*id)?;
                self.after_project_change();
                self.toast_ok(format!("Deleted project “{name}”. Its sounds and files stay."));
            }
        }
        Ok(())
    }

    fn apply_member_dialog(&mut self, d: &MemberDialog) -> Result<()> {
        let store = self.store.as_ref().context("the library is not open")?;
        if d.previous.is_some() {
            store.set_membership(d.project_id, d.sound_id, &d.path, &d.options)?;
        } else {
            store.add_to_project(d.project_id, d.sound_id, &d.path, &d.options)?;
        }
        self.after_project_change();
        Ok(())
    }

    pub(super) fn project_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut d) = self.project_dialog.take() {
            match projects::project_dialog(ctx, &mut d) {
                None => self.project_dialog = Some(d),
                Some(false) => {}
                Some(true) => {
                    if let Err(e) = self.apply_project_dialog(&d) {
                        d.set_error(format!("{e:#}"));
                        self.project_dialog = Some(d);
                    }
                }
            }
        }
        if let Some(mut d) = self.member_dialog.take() {
            let (pid, sid) = (d.project_id, d.sound_id);
            let store = self.store.as_ref();
            let outcome = projects::member_dialog(ctx, &mut d, |path, options| match store {
                Some(s) => s.check_member_path(pid, Some(sid), path, options).map_err(|e| format!("{e:#}")),
                None => Err("the library is not open".into()),
            });
            match outcome {
                None => self.member_dialog = Some(d),
                Some(false) => {}
                Some(true) => {
                    if let Err(e) = self.apply_member_dialog(&d) {
                        d.error = Some(format!("{e:#}"));
                        self.member_dialog = Some(d);
                    }
                }
            }
        }
    }
}
