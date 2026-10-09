//! Writes a project's changed sounds to disk, for the app and the CLI alike. Writing files is separate from the
//! database, so the app can write on a background thread and record the result on its own connection.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use sfxc_core::export::export_to_path;
use sfxc_core::patch::SoundPatch;

use crate::{MemberOptions, Membership, Project, Store};

pub struct ExportPlan {
    pub project: Project,
    pub root_exists: bool,
    /// Changed since the last project export (including drafts nobody committed).
    pub changed: Vec<Membership>,
    /// Unchanged, but the file is gone from disk.
    pub missing: Vec<Membership>,
}

impl ExportPlan {
    pub fn items(&self, include_missing: bool) -> Vec<Membership> {
        let mut items = self.changed.clone();
        if include_missing {
            items.extend(self.missing.iter().cloned());
        }
        items
    }
}

/// Reads only: nothing is committed or written.
pub fn plan(store: &Store, project_id: i64) -> Result<ExportPlan> {
    let project = store.project(project_id)?;
    let root_exists = Path::new(&project.root).is_dir();
    let (changed, rest): (Vec<_>, Vec<_>) = store.project_sounds(project_id, "")?.into_iter().partition(Membership::changed);
    let missing = rest.into_iter().filter(|m| !m.abs_path(&project.root).is_file()).collect();
    Ok(ExportPlan { project, root_exists, changed, missing })
}

pub struct ExportJob {
    pub project_id: i64,
    pub sound_id: i64,
    pub sound_name: String,
    pub version_id: i64,
    pub rel_path: String,
    pub options: MemberOptions,
    pub path: PathBuf,
    pub patch: SoundPatch,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemResult {
    pub sound_id: i64,
    pub name: String,
    pub rel_path: String,
    pub path: PathBuf,
    pub error: Option<String>,
}

/// Commits drafts that differ from their version and loads the patch of each version to write. Items that fail
/// here come back as results right away.
pub fn prepare(store: &Store, project: &Project, items: &[Membership], now: i64) -> (Vec<ExportJob>, Vec<ItemResult>) {
    let mut jobs = Vec::new();
    let mut failed = Vec::new();
    for m in items {
        let job = (|| -> Result<ExportJob> {
            let version_id = match m.current_version_id {
                Some(v) if !m.draft_dirty => v,
                _ => {
                    // Same rule as the CLI: never replace a note a person wrote on the latest version.
                    let note = if store.draft_differs_from_latest(m.sound_id)? { "export" } else { "" };
                    store.commit_version(m.sound_id, note, now)?
                }
            };
            Ok(ExportJob {
                project_id: m.project_id,
                sound_id: m.sound_id,
                sound_name: m.sound_name.clone(),
                version_id,
                rel_path: m.rel_path.clone(),
                options: m.options,
                path: m.abs_path(&project.root),
                patch: store.load_version(version_id)?,
            })
        })();
        match job {
            Ok(j) => jobs.push(j),
            Err(e) => failed.push(ItemResult {
                sound_id: m.sound_id,
                name: m.sound_name.clone(),
                rel_path: m.rel_path.clone(),
                path: m.abs_path(&project.root),
                error: Some(format!("{e:#}")),
            }),
        }
    }
    (jobs, failed)
}

/// Files only. A missing root fails every job unless `create_root`. One failed file does not stop the rest.
pub fn write(root: &Path, create_root: bool, jobs: &[ExportJob]) -> Vec<Result<()>> {
    if !root.is_dir() {
        let made = if create_root {
            std::fs::create_dir_all(root).with_context(|| format!("cannot create {}", root.display()))
        } else {
            Err(anyhow!("project folder {} does not exist", root.display()))
        };
        if let Err(e) = made {
            let msg = format!("{e:#}");
            return jobs.iter().map(|_| Err(anyhow!(msg.clone()))).collect();
        }
    }
    jobs.iter().map(write_one).collect()
}

fn write_one(job: &ExportJob) -> Result<()> {
    if let Some(dir) = job.path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    export_to_path(&job.patch, &job.options.resolve(job.patch.mode), &job.path)
}

/// Marks what was written. A membership whose path or settings changed meanwhile stays changed.
pub fn record(store: &Store, jobs: &[ExportJob], results: Vec<Result<()>>, now: i64) -> Vec<ItemResult> {
    jobs.iter()
        .zip(results)
        .map(|(j, r)| {
            let error = match r {
                Ok(()) => store
                    .mark_project_exported(j.project_id, j.sound_id, j.version_id, &j.rel_path, &j.options, now)
                    .err()
                    .map(|e| format!("{e:#}")),
                Err(e) => Some(format!("{e:#}")),
            };
            ItemResult { sound_id: j.sound_id, name: j.sound_name.clone(), rel_path: j.rel_path.clone(), path: j.path.clone(), error }
        })
        .collect()
}

/// `prepare`, `write` and `record` in one go, for the CLI.
pub fn run(store: &Store, project: &Project, items: &[Membership], create_root: bool, now: i64) -> Vec<ItemResult> {
    let (jobs, mut out) = prepare(store, project, items, now);
    let results = write(Path::new(&project.root), create_root, &jobs);
    out.extend(record(store, &jobs, results, now));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sfxc_core::generators::{generate, Category};
    use sfxc_core::patch::Mode;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sfxc-pexport-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A project in a fresh folder with sounds `a` and `b` at `sfx/a.wav` and `sfx/b.wav`.
    fn setup(name: &str) -> (Store, PathBuf, i64, i64, i64) {
        let dir = temp_dir(name);
        let s = Store::open_in_memory().unwrap();
        let pid = s.create_project("Game", &dir.to_string_lossy(), 1).unwrap();
        let a = s.create_sound("a", &generate(Category::Jump, Mode::Modern, 1), 1).unwrap();
        let b = s.create_sound("b", &generate(Category::PickupCoin, Mode::Modern, 2), 1).unwrap();
        s.add_to_project(pid, a, "sfx/a.wav", &MemberOptions::default()).unwrap();
        s.add_to_project(pid, b, "sfx/b.wav", &MemberOptions::default()).unwrap();
        (s, dir, pid, a, b)
    }

    fn export_all(s: &Store, pid: i64, now: i64) -> Vec<ItemResult> {
        let p = plan(s, pid).unwrap();
        run(s, &p.project, &p.items(true), false, now)
    }

    #[test]
    fn first_export_writes_everything_then_nothing_is_changed() {
        let (s, dir, pid, ..) = setup("first");
        let out = export_all(&s, pid, 10);
        assert!(out.iter().all(|r| r.error.is_none()), "{out:?}");
        assert!(dir.join("sfx/a.wav").is_file() && dir.join("sfx/b.wav").is_file());
        let p = plan(&s, pid).unwrap();
        assert!(p.changed.is_empty() && p.missing.is_empty());
    }

    #[test]
    fn an_unsaved_edit_plans_one_sound_and_planning_writes_nothing() {
        let (s, _dir, pid, a, _) = setup("one");
        export_all(&s, pid, 10);
        s.save_draft(a, &generate(Category::Jump, Mode::Bit8, 3), 11).unwrap();
        let versions = s.list_versions(a).unwrap().len();
        let p = plan(&s, pid).unwrap();
        assert_eq!(p.changed.iter().map(|m| m.sound_id).collect::<Vec<_>>(), [a]);
        assert_eq!(s.list_versions(a).unwrap().len(), versions, "plan must not commit");
        let out = run(&s, &p.project, &p.items(false), false, 12);
        assert_eq!(out.len(), 1);
        let v = s.list_versions(a).unwrap();
        assert_eq!((v.len(), v[0].note.as_str(), v[0].exported_at), (versions + 1, "export", Some(12)));
        assert!(!s.membership(pid, a).unwrap().changed());
    }

    #[test]
    fn a_deleted_file_is_missing_not_changed() {
        let (s, dir, pid, _, b) = setup("missing");
        export_all(&s, pid, 10);
        std::fs::remove_file(dir.join("sfx/b.wav")).unwrap();
        let p = plan(&s, pid).unwrap();
        assert!(p.changed.is_empty());
        assert_eq!(p.missing.iter().map(|m| m.sound_id).collect::<Vec<_>>(), [b]);
        assert!(p.items(false).is_empty());
        assert_eq!(p.items(true).len(), 1);
    }

    #[test]
    fn one_bad_file_does_not_stop_the_rest() {
        let (s, dir, pid, a, b) = setup("bad");
        std::fs::write(dir.join("blocker"), b"x").unwrap();
        s.set_membership(pid, a, "blocker/a.wav", &MemberOptions::default()).unwrap();
        let out = export_all(&s, pid, 10);
        let failed: Vec<i64> = out.iter().filter(|r| r.error.is_some()).map(|r| r.sound_id).collect();
        assert_eq!(failed, [a]);
        assert!(dir.join("sfx/b.wav").is_file());
        assert!(s.membership(pid, a).unwrap().changed() && !s.membership(pid, b).unwrap().changed());
    }

    #[test]
    fn a_missing_root_is_created_only_when_asked() {
        let (s, dir, pid, ..) = setup("root");
        let root = dir.join("not yet");
        s.set_project_root(pid, &root.to_string_lossy()).unwrap();
        let p = plan(&s, pid).unwrap();
        assert!(!p.root_exists);
        let out = run(&s, &p.project, &p.items(true), false, 10);
        assert!(out.iter().all(|r| r.error.as_deref().is_some_and(|e| e.contains("does not exist"))), "{out:?}");
        assert!(!root.exists());
        let out = run(&s, &p.project, &p.items(true), true, 11);
        assert!(out.iter().all(|r| r.error.is_none()), "{out:?}");
        assert!(root.join("sfx/a.wav").is_file());
    }

    #[test]
    fn a_membership_edited_during_the_write_stays_changed() {
        let (s, _dir, pid, a, _) = setup("race");
        let p = plan(&s, pid).unwrap();
        let (jobs, failed) = prepare(&s, &p.project, &p.items(true), 10);
        assert!(failed.is_empty());
        let results = write(Path::new(&p.project.root), false, &jobs);
        s.set_membership(pid, a, "sfx/renamed.wav", &MemberOptions::default()).unwrap();
        let out = record(&s, &jobs, results, 11);
        assert!(out.iter().all(|r| r.error.is_none()));
        assert!(s.membership(pid, a).unwrap().changed(), "the new path was never written");
    }

    #[test]
    fn auto_bits_write_the_mode_depth() {
        let (s, dir, pid, a, _) = setup("bits");
        s.save_draft(a, &generate(Category::Jump, Mode::Bit8, 4), 5).unwrap();
        export_all(&s, pid, 10);
        let bytes = std::fs::read(dir.join("sfx/a.wav")).unwrap();
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 8, "bits per sample in the fmt chunk");
    }
}
