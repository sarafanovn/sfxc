//! Projects: a folder in a game and the sounds written into it, each with its own path and export settings.

use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sfxc_core::export::{wav_bits, ExportFormat, ExportOptions};
use sfxc_core::patch::Mode;
use unicode_normalization::UnicodeNormalization;

use crate::{SoundSummary, Store};

/// Rates the renderer and encoders handle without panicking.
pub const RATE_RANGE: std::ops::RangeInclusive<u32> = 8_000..=192_000;
pub const DEFAULT_OGG_QUALITY: f32 = 6.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub id: i64,
    pub name: String,
    /// Absolute, without a trailing `/`. May not exist.
    pub root: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectSummary {
    pub project: Project,
    pub sounds: usize,
    pub changed: usize,
}

/// Like `ExportFormat`, but WAV bits may follow the sound's mode. Reads old `ExportFormat` JSON as is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum MemberFormat {
    Wav { bits: Option<u16> },
    Ogg { quality: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemberOptions {
    pub format: MemberFormat,
    pub sample_rate: u32,
    pub normalize: bool,
    pub trim: bool,
    pub duration: Option<f32>,
}

impl Default for MemberOptions {
    fn default() -> Self {
        Self { format: MemberFormat::Wav { bits: None }, sample_rate: 44_100, normalize: true, trim: true, duration: None }
    }
}

impl MemberOptions {
    pub fn extension(&self) -> &'static str {
        match self.format {
            MemberFormat::Wav { .. } => "wav",
            MemberFormat::Ogg { .. } => "ogg",
        }
    }

    pub fn resolve(&self, mode: Mode) -> ExportOptions {
        ExportOptions {
            format: match self.format {
                MemberFormat::Wav { bits } => ExportFormat::Wav { bits: bits.unwrap_or(wav_bits(mode)) },
                MemberFormat::Ogg { quality } => ExportFormat::Ogg { quality },
            },
            sample_rate: self.sample_rate,
            normalize: self.normalize,
            trim: self.trim,
            duration: self.duration,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if !RATE_RANGE.contains(&self.sample_rate) {
            bail!("sample rate {} is out of range; use {} to {} Hz", self.sample_rate, RATE_RANGE.start(), RATE_RANGE.end());
        }
        match self.format {
            MemberFormat::Wav { bits: Some(b) } if ![8, 16, 24].contains(&b) => bail!("WAV bit depth {b} is not 8, 16 or 24"),
            MemberFormat::Ogg { quality } if !(0.0..=10.0).contains(&quality) => bail!("OGG quality {quality} is not between 0 and 10"),
            _ => {}
        }
        if let Some(d) = self.duration
            && !(d.is_finite() && d > 0.0)
        {
            bail!("length {d} must be above 0 seconds");
        }
        Ok(())
    }

    fn to_json(self) -> String {
        serde_json::to_string(&self).expect("options always serialize")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Membership {
    pub project_id: i64,
    pub sound_id: i64,
    pub sound_name: String,
    pub tags: String,
    pub updated_at: i64,
    pub rel_path: String,
    pub options: MemberOptions,
    pub exported_version_id: Option<i64>,
    pub current_version_id: Option<i64>,
    /// The draft differs from the current version's patch.
    pub draft_dirty: bool,
}

impl Membership {
    pub fn changed(&self) -> bool {
        self.draft_dirty || self.exported_version_id.is_none() || self.exported_version_id != self.current_version_id
    }

    pub fn abs_path(&self, root: &str) -> PathBuf {
        Path::new(root).join(&self.rel_path)
    }
}

/// Folder rules only (no extension check); returns the path in NFC.
pub(crate) fn check_rel_structure(raw: &str) -> Result<String> {
    let path: String = raw.trim().nfc().collect();
    if path.is_empty() {
        bail!("the path is empty");
    }
    if path.contains('\\') {
        bail!("use `/` between folders in `{path}`");
    }
    if path.starts_with('/') {
        bail!("`{path}` must be relative to the project folder");
    }
    if path.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        bail!("`{path}` may not contain empty, `.` or `..` parts");
    }
    Ok(path)
}

/// A path inside a project, checked and in NFC. Its extension must match the format.
pub fn normalize_rel_path(raw: &str, options: &MemberOptions) -> Result<String> {
    let path = check_rel_structure(raw)?;
    let ext = Path::new(&path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if ext.as_deref() != Some(options.extension()) {
        bail!("`{path}` must end in .{} to match the format", options.extension());
    }
    Ok(path)
}

/// Two paths with the same key are one file on APFS (case-insensitive, normalization-insensitive).
pub(crate) fn path_key(rel: &str) -> String {
    rel.nfc().collect::<String>().to_lowercase()
}

/// `sfx/click.wav` → `sfx/click.ogg`; a file without an extension gets one.
pub fn replace_extension(rel: &str, ext: &str) -> String {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    if dir.is_empty() { format!("{stem}.{ext}") } else { format!("{dir}/{stem}.{ext}") }
}

/// `path` inside `root` as a project path. Both are compared after resolving symlinks (`/tmp` is `/private/tmp`).
pub fn relative_to_root(root: &Path, path: &Path) -> Result<String> {
    let (r, p) = (resolve(root), resolve(path));
    let rel = p
        .strip_prefix(&r)
        .map_err(|_| anyhow!("{} is not inside the project folder {}", path.display(), root.display()))?;
    let parts = rel
        .components()
        .map(|c| match c {
            Component::Normal(s) => Ok(s.to_string_lossy().into_owned()),
            _ => Err(anyhow!("bad path {}", path.display())),
        })
        .collect::<Result<Vec<_>>>()?;
    if parts.is_empty() {
        bail!("{} is the project folder itself; name a file inside it", path.display());
    }
    Ok(parts.join("/"))
}

/// The canonical form of the longest existing ancestor, with the rest appended.
fn resolve(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut tail = Vec::new();
    let mut cur = path.as_path();
    loop {
        if let Ok(c) = cur.canonicalize() {
            return tail.iter().rev().fold(c, |acc, part| acc.join(part));
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                cur = parent;
            }
            _ => return path,
        }
    }
}

fn clean_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        bail!("the project name is empty");
    }
    Ok(name)
}

fn normalize_root(raw: &str) -> Result<String> {
    let root = raw.trim();
    if !Path::new(root).is_absolute() {
        bail!("the project folder `{root}` must be an absolute path");
    }
    let root = root.trim_end_matches('/');
    Ok(if root.is_empty() { "/".to_string() } else { root.to_string() })
}

const MEMBER_SELECT: &str = "SELECT ps.project_id, ps.sound_id, s.name, s.tags, s.updated_at, ps.rel_path, ps.options_json,
        ps.exported_version_id, s.current_version_id,
        s.draft_json IS NOT (SELECT patch_json FROM versions WHERE id = s.current_version_id)
     FROM project_sounds ps JOIN sounds s ON s.id = ps.sound_id";

fn membership_row(r: &Row) -> rusqlite::Result<Membership> {
    let json: String = r.get(6)?;
    let options = serde_json::from_str(&json)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(Membership {
        project_id: r.get(0)?,
        sound_id: r.get(1)?,
        sound_name: r.get(2)?,
        tags: r.get(3)?,
        updated_at: r.get(4)?,
        rel_path: r.get(5)?,
        options,
        exported_version_id: r.get(7)?,
        current_version_id: r.get(8)?,
        draft_dirty: r.get(9)?,
    })
}

fn project_row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project { id: r.get(0)?, name: r.get(1)?, root: r.get(2)?, created_at: r.get(3)? })
}

impl Store {
    pub fn create_project(&self, name: &str, root: &str, now: i64) -> Result<i64> {
        let name = clean_name(name)?;
        let root = normalize_root(root)?;
        self.ensure_project_name_free(name, None)?;
        self.conn.execute("INSERT INTO projects (name, root, created_at) VALUES (?1, ?2, ?3)", params![name, root, now])?;
        Ok(self.conn.last_insert_rowid())
    }

    fn ensure_project_name_free(&self, name: &str, except: Option<i64>) -> Result<()> {
        let other: Option<i64> = self
            .conn
            .query_row("SELECT id FROM projects WHERE name = ?1 AND id IS NOT ?2", params![name, except], |r| r.get(0))
            .optional()?;
        if let Some(id) = other {
            bail!("a project named `{name}` already exists (id {id})");
        }
        Ok(())
    }

    pub fn rename_project(&self, id: i64, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        self.project(id)?;
        self.ensure_project_name_free(name, Some(id))?;
        self.conn.execute("UPDATE projects SET name = ?1 WHERE id = ?2", params![name, id])?;
        Ok(())
    }

    /// Files are not moved, so every sound in the project needs exporting again.
    pub fn set_project_root(&self, id: i64, root: &str) -> Result<()> {
        let root = normalize_root(root)?;
        self.project(id)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE projects SET root = ?1 WHERE id = ?2", params![root, id])?;
        tx.execute("UPDATE project_sounds SET exported_version_id = NULL WHERE project_id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// Returns how many memberships went with it. Sounds and files stay.
    pub fn delete_project(&self, id: i64) -> Result<usize> {
        self.project(id)?;
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM project_sounds WHERE project_id = ?1", [id], |r| r.get(0))?;
        self.conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(n as usize)
    }

    pub fn project(&self, id: i64) -> Result<Project> {
        self.conn
            .query_row("SELECT id, name, root, created_at FROM projects WHERE id = ?1", [id], project_row)
            .optional()?
            .with_context(|| format!("no project with id {id}"))
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let mut stmt = self.conn.prepare("SELECT id, name, root, created_at FROM projects ORDER BY name COLLATE NOCASE, id")?;
        let projects: Vec<Project> = stmt.query_map([], project_row)?.collect::<rusqlite::Result<_>>()?;
        projects
            .into_iter()
            .map(|project| {
                let members = self.project_sounds(project.id, "")?;
                Ok(ProjectSummary { sounds: members.len(), changed: members.iter().filter(|m| m.changed()).count(), project })
            })
            .collect()
    }

    /// An existing id, else a name (ignoring case).
    pub fn find_project(&self, reference: &str) -> Result<i64> {
        let reference = reference.trim();
        if let Ok(id) = reference.parse::<i64>()
            && self.project(id).is_ok()
        {
            return Ok(id);
        }
        self.conn
            .query_row("SELECT id FROM projects WHERE name = ?1", [reference], |r| r.get(0))
            .optional()?
            .with_context(|| format!("no project with id or name `{reference}`; `sfxc-cli project list` shows them"))
    }

    /// `rel_path` checked and normalized for this project; fails when another sound already writes there.
    pub fn check_member_path(&self, project_id: i64, sound_id: Option<i64>, rel_path: &str, options: &MemberOptions) -> Result<String> {
        let rel = normalize_rel_path(rel_path, options)?;
        let other: Option<String> = self
            .conn
            .query_row(
                "SELECT s.name FROM project_sounds ps JOIN sounds s ON s.id = ps.sound_id
                 WHERE ps.project_id = ?1 AND ps.path_key = ?2 AND ps.sound_id IS NOT ?3",
                params![project_id, path_key(&rel), sound_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(name) = other {
            bail!("“{name}” already writes to {rel} in this project");
        }
        Ok(rel)
    }

    pub fn add_to_project(&self, project_id: i64, sound_id: i64, rel_path: &str, options: &MemberOptions) -> Result<()> {
        options.validate()?;
        self.project(project_id)?;
        self.sound_meta(sound_id).with_context(|| format!("no sound with id {sound_id}"))?;
        if self.membership(project_id, sound_id).is_ok() {
            bail!("sound {sound_id} is already in this project; use `project set` to change its path or settings");
        }
        let rel = self.check_member_path(project_id, Some(sound_id), rel_path, options)?;
        self.conn.execute(
            "INSERT INTO project_sounds (project_id, sound_id, rel_path, path_key, options_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id, sound_id, rel, path_key(&rel), options.to_json()],
        )?;
        Ok(())
    }

    /// A new path or new settings mean the file must be written again; the old file stays on disk.
    pub fn set_membership(&self, project_id: i64, sound_id: i64, rel_path: &str, options: &MemberOptions) -> Result<()> {
        options.validate()?;
        let old = self.membership(project_id, sound_id)?;
        let rel = self.check_member_path(project_id, Some(sound_id), rel_path, options)?;
        if old.rel_path == rel && old.options == *options {
            return Ok(());
        }
        self.conn.execute(
            "UPDATE project_sounds SET rel_path = ?3, path_key = ?4, options_json = ?5, exported_version_id = NULL
             WHERE project_id = ?1 AND sound_id = ?2",
            params![project_id, sound_id, rel, path_key(&rel), options.to_json()],
        )?;
        Ok(())
    }

    pub fn remove_from_project(&self, project_id: i64, sound_id: i64) -> Result<()> {
        let n = self.conn.execute("DELETE FROM project_sounds WHERE project_id = ?1 AND sound_id = ?2", params![project_id, sound_id])?;
        if n == 0 {
            bail!("sound {sound_id} is not in project {project_id}");
        }
        Ok(())
    }

    pub fn membership(&self, project_id: i64, sound_id: i64) -> Result<Membership> {
        self.conn
            .query_row(&format!("{MEMBER_SELECT} WHERE ps.project_id = ?1 AND ps.sound_id = ?2"), params![project_id, sound_id], membership_row)
            .optional()?
            .with_context(|| format!("sound {sound_id} is not in project {project_id}"))
    }

    /// Sorted by path; `search` matches the name, tags or path.
    pub fn project_sounds(&self, project_id: i64, search: &str) -> Result<Vec<Membership>> {
        let pattern = format!("%{}%", search.trim());
        let mut stmt = self.conn.prepare(&format!(
            "{MEMBER_SELECT} WHERE ps.project_id = ?1 AND (s.name LIKE ?2 OR s.tags LIKE ?2 OR ps.rel_path LIKE ?2)
             ORDER BY ps.path_key"
        ))?;
        let rows = stmt.query_map(params![project_id, pattern], membership_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn sound_projects(&self, sound_id: i64) -> Result<Vec<(Project, Membership)>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id FROM project_sounds ps JOIN projects p ON p.id = ps.project_id
             WHERE ps.sound_id = ?1 ORDER BY p.name COLLATE NOCASE, p.id",
        )?;
        let ids: Vec<i64> = stmt.query_map([sound_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        ids.into_iter().map(|id| Ok((self.project(id)?, self.membership(id, sound_id)?))).collect()
    }

    /// Sounds in no project, in the same order as `list_sounds`.
    pub fn unassigned_sounds(&self, search: &str) -> Result<Vec<SoundSummary>> {
        let pattern = format!("%{}%", search.trim());
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, updated_at FROM sounds
             WHERE (name LIKE ?1 OR tags LIKE ?1) AND NOT EXISTS (SELECT 1 FROM project_sounds ps WHERE ps.sound_id = sounds.id)
             ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([pattern], |r| {
            Ok(SoundSummary { id: r.get(0)?, name: r.get(1)?, tags: r.get(2)?, updated_at: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The folder of the sound added last (or the root), the sound's name made safe for a file, and a free number
    /// if that file is taken.
    pub fn default_member_path(&self, project_id: i64, sound_name: &str, options: &MemberOptions) -> Result<String> {
        let last: Option<String> = self
            .conn
            .query_row("SELECT rel_path FROM project_sounds WHERE project_id = ?1 ORDER BY rowid DESC LIMIT 1", [project_id], |r| r.get(0))
            .optional()?;
        let dir = last.and_then(|p| p.rsplit_once('/').map(|(d, _)| d.to_string()));
        let stem: String = sound_name.trim().chars().map(|c| if matches!(c, '/' | ':' | '\\') { '_' } else { c }).collect();
        let stem = if stem.is_empty() { "sound".to_string() } else { stem };
        let ext = options.extension();
        for n in 1.. {
            let file = if n == 1 { format!("{stem}.{ext}") } else { format!("{stem} {n}.{ext}") };
            let path = dir.as_ref().map_or(file.clone(), |d| format!("{d}/{file}"));
            let taken: bool = self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM project_sounds WHERE project_id = ?1 AND path_key = ?2)",
                params![project_id, path_key(&path)],
                |r| r.get(0),
            )?;
            if !taken {
                return Ok(path);
            }
        }
        unreachable!("1.. never ends")
    }

    /// Records that `version_id` was written to `rel_path` with `options`. Does nothing to the membership (returns
    /// false) when its path or settings changed since: the file now on disk is not what it asks for.
    pub fn mark_project_exported(
        &self,
        project_id: i64,
        sound_id: i64,
        version_id: i64,
        rel_path: &str,
        options: &MemberOptions,
        now: i64,
    ) -> Result<bool> {
        let tx = self.conn.unchecked_transaction()?;
        let n = tx.execute(
            "UPDATE project_sounds SET exported_version_id = ?3
             WHERE project_id = ?1 AND sound_id = ?2 AND rel_path = ?4 AND options_json = ?5",
            params![project_id, sound_id, version_id, rel_path, options.to_json()],
        )?;
        tx.execute("UPDATE versions SET exported_at = ?1 WHERE id = ?2", params![now, version_id])?;
        tx.commit()?;
        Ok(n == 1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sfxc_core::patch::SoundPatch;

    fn patch(freq: f32) -> SoundPatch {
        let mut p = SoundPatch::default();
        p.layers[0].pitch.base_freq = freq;
        p
    }

    fn setup() -> (Store, i64, i64) {
        let s = Store::open_in_memory().unwrap();
        let pid = s.create_project("Game A", "/games/a", 1).unwrap();
        let sid = s.create_sound("click", &patch(300.0), 1).unwrap();
        (s, pid, sid)
    }

    fn wav() -> MemberOptions {
        MemberOptions::default()
    }

    fn ogg() -> MemberOptions {
        MemberOptions { format: MemberFormat::Ogg { quality: 6.0 }, ..Default::default() }
    }

    #[test]
    fn rel_paths_are_checked() {
        assert_eq!(normalize_rel_path(" sfx/click.WAV ", &wav()).unwrap(), "sfx/click.WAV");
        for bad in ["", "/abs.wav", "a//b.wav", "./a.wav", "../a.wav", "a/../b.wav", "a\\b.wav", "a.ogg", "noext"] {
            assert!(normalize_rel_path(bad, &wav()).is_err(), "{bad}");
        }
        assert!(normalize_rel_path("a.ogg", &ogg()).is_ok());
    }

    #[test]
    fn rel_paths_are_stored_in_nfc() {
        let nfd: String = "й.wav".nfd().collect();
        assert_eq!(normalize_rel_path(&nfd, &wav()).unwrap(), "й.wav".nfc().collect::<String>());
    }

    #[test]
    fn replace_extension_keeps_the_folder() {
        assert_eq!(replace_extension("sfx/click.wav", "ogg"), "sfx/click.ogg");
        assert_eq!(replace_extension("click", "wav"), "click.wav");
        assert_eq!(replace_extension("a.b/c", "ogg"), "a.b/c.ogg");
    }

    #[test]
    fn two_sounds_cannot_share_a_file_even_by_case_or_normalization() {
        let (s, pid, a) = setup();
        let b = s.create_sound("b", &patch(300.0), 1).unwrap();
        let c = s.create_sound("c", &patch(300.0), 1).unwrap();
        s.add_to_project(pid, a, "sfx/Click.wav", &wav()).unwrap();
        let e = s.add_to_project(pid, b, "SFX/click.wav", &wav()).unwrap_err().to_string();
        assert!(e.contains("already writes"), "{e}");
        s.add_to_project(pid, b, "й.wav", &wav()).unwrap();
        assert!(s.add_to_project(pid, c, &"Й.wav".nfd().collect::<String>(), &wav()).is_err());
    }

    #[test]
    fn project_names_are_unique_ignoring_case_and_found_by_id_or_name() {
        let (s, pid, _) = setup();
        assert!(s.create_project("game a", "/x", 1).is_err());
        assert!(s.create_project("  ", "/x", 1).is_err());
        assert!(s.create_project("B", "relative", 1).is_err());
        assert_eq!(s.find_project("GAME A").unwrap(), pid);
        assert_eq!(s.find_project(&pid.to_string()).unwrap(), pid);
        assert!(s.find_project("nope").unwrap_err().to_string().contains("nope"));
        let b = s.create_project("B", "/b/", 1).unwrap();
        assert_eq!(s.project(b).unwrap().root, "/b");
        assert!(s.rename_project(b, "game A").is_err());
        s.rename_project(b, "Game B").unwrap();
        let names: Vec<String> = s.list_projects().unwrap().into_iter().map(|p| p.project.name).collect();
        assert_eq!(names, ["Game A", "Game B"]);
    }

    #[test]
    fn changed_follows_the_draft_versions_and_exports() {
        let (s, pid, sid) = setup();
        s.add_to_project(pid, sid, "click.wav", &wav()).unwrap();
        let m = s.membership(pid, sid).unwrap();
        assert!(m.changed(), "never exported");
        let v1 = m.current_version_id.unwrap();
        assert!(s.mark_project_exported(pid, sid, v1, "click.wav", &wav(), 5).unwrap());
        assert!(!s.membership(pid, sid).unwrap().changed());
        assert_eq!(s.list_versions(sid).unwrap()[0].exported_at, Some(5));

        s.save_draft(sid, &patch(400.0), 6).unwrap();
        let m = s.membership(pid, sid).unwrap();
        assert!(m.draft_dirty && m.changed(), "an unsaved slider move counts");

        let v2 = s.commit_version(sid, "", 7).unwrap();
        assert!(s.membership(pid, sid).unwrap().changed());
        s.mark_project_exported(pid, sid, v2, "click.wav", &wav(), 8).unwrap();
        assert!(!s.membership(pid, sid).unwrap().changed());

        s.restore_version(sid, v1, 9).unwrap();
        assert!(s.membership(pid, sid).unwrap().changed(), "restoring an older version is a change");
    }

    #[test]
    fn changing_path_options_or_root_needs_a_new_export() {
        let (s, pid, sid) = setup();
        s.add_to_project(pid, sid, "click.wav", &wav()).unwrap();
        let v = s.current_version_id(sid).unwrap().unwrap();
        let mark = || {
            let m = s.membership(pid, sid).unwrap();
            s.mark_project_exported(pid, sid, v, &m.rel_path, &m.options, 5).unwrap();
        };
        let changed = || s.membership(pid, sid).unwrap().changed();
        mark();
        s.set_membership(pid, sid, "sfx/click.wav", &wav()).unwrap();
        assert!(changed());
        mark();
        s.set_membership(pid, sid, "sfx/click.ogg", &ogg()).unwrap();
        assert!(changed());
        mark();
        s.set_membership(pid, sid, "sfx/click.ogg", &ogg()).unwrap();
        assert!(!changed(), "a no-op keeps the export");
        s.set_project_root(pid, "/games/b").unwrap();
        assert!(changed());
    }

    #[test]
    fn mark_is_skipped_when_the_membership_changed_meanwhile() {
        let (s, pid, sid) = setup();
        s.add_to_project(pid, sid, "click.wav", &wav()).unwrap();
        let v = s.current_version_id(sid).unwrap().unwrap();
        s.set_membership(pid, sid, "other.wav", &wav()).unwrap();
        assert!(!s.mark_project_exported(pid, sid, v, "click.wav", &wav(), 5).unwrap());
        assert!(s.membership(pid, sid).unwrap().changed());
    }

    #[test]
    fn add_set_remove_and_cascades() {
        let (s, pid, sid) = setup();
        s.add_to_project(pid, sid, "click.wav", &wav()).unwrap();
        assert!(s.add_to_project(pid, sid, "again.wav", &wav()).unwrap_err().to_string().contains("project set"));
        assert!(s.unassigned_sounds("").unwrap().is_empty());
        assert_eq!(s.sound_projects(sid).unwrap()[0].0.name, "Game A");
        s.remove_from_project(pid, sid).unwrap();
        assert_eq!(s.unassigned_sounds("").unwrap()[0].id, sid);
        assert!(s.remove_from_project(pid, sid).is_err());

        s.add_to_project(pid, sid, "click.wav", &wav()).unwrap();
        s.delete_sound(sid).unwrap();
        assert!(s.project_sounds(pid, "").unwrap().is_empty());

        let other = s.create_sound("x", &patch(300.0), 1).unwrap();
        s.add_to_project(pid, other, "x.wav", &wav()).unwrap();
        assert_eq!(s.delete_project(pid).unwrap(), 1);
        assert!(s.sound_projects(other).unwrap().is_empty());
        assert!(s.load_draft(other).is_ok(), "deleting a project keeps its sounds");
    }

    #[test]
    fn project_sounds_search_name_tags_and_path_sorted_by_path() {
        let (s, pid, a) = setup();
        let b = s.create_sound("boom", &patch(300.0), 1).unwrap();
        s.set_tags(b, "loud", 2).unwrap();
        s.add_to_project(pid, a, "z/click.wav", &wav()).unwrap();
        s.add_to_project(pid, b, "a/boom.wav", &wav()).unwrap();
        let names = |q: &str| s.project_sounds(pid, q).unwrap().into_iter().map(|m| m.sound_name).collect::<Vec<_>>();
        assert_eq!(names(""), ["boom", "click"]);
        assert_eq!(names("loud"), ["boom"]);
        assert_eq!(names("z/"), ["click"]);
    }

    #[test]
    fn default_path_uses_the_last_folder_and_avoids_taken_names() {
        let (s, pid, sid) = setup();
        assert_eq!(s.default_member_path(pid, "a/b: c", &wav()).unwrap(), "a_b_ c.wav");
        s.add_to_project(pid, sid, "sfx/click.wav", &wav()).unwrap();
        assert_eq!(s.default_member_path(pid, "Click", &ogg()).unwrap(), "sfx/Click.ogg");
        assert_eq!(s.default_member_path(pid, "Click", &wav()).unwrap(), "sfx/Click 2.wav");
    }

    #[test]
    fn relative_to_root_resolves_symlinks() {
        let dir = std::env::temp_dir().join(format!("sfxc-rel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game/sfx")).unwrap();
        std::os::unix::fs::symlink(dir.join("game"), dir.join("link")).unwrap();
        assert_eq!(relative_to_root(&dir.join("game"), &dir.join("link/sfx/new.wav")).unwrap(), "sfx/new.wav");
        assert_eq!(relative_to_root(&dir.join("link"), &dir.join("game/a/b.wav")).unwrap(), "a/b.wav");
        assert!(relative_to_root(&dir.join("game"), &dir.join("elsewhere.wav")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_bits_follow_the_mode() {
        assert_eq!(wav().resolve(Mode::Bit8).format, ExportFormat::Wav { bits: 8 });
        assert_eq!(wav().resolve(Mode::Modern).format, ExportFormat::Wav { bits: 24 });
        let fixed = MemberOptions { format: MemberFormat::Wav { bits: Some(16) }, ..wav() };
        assert_eq!(fixed.resolve(Mode::Bit8).format, ExportFormat::Wav { bits: 16 });
    }

    #[test]
    fn old_export_options_json_reads_as_member_options() {
        let old = serde_json::to_string(&ExportOptions::default()).unwrap();
        let m: MemberOptions = serde_json::from_str(&old).unwrap();
        assert_eq!(m.format, MemberFormat::Wav { bits: Some(16) });
    }

    #[test]
    fn options_are_validated() {
        assert!(MemberOptions { sample_rate: 0, ..wav() }.validate().is_err());
        assert!(MemberOptions { format: MemberFormat::Wav { bits: Some(12) }, ..wav() }.validate().is_err());
        assert!(MemberOptions { format: MemberFormat::Ogg { quality: 11.0 }, ..wav() }.validate().is_err());
        assert!(MemberOptions { duration: Some(0.0), ..wav() }.validate().is_err());
        assert!(wav().validate().is_ok() && ogg().validate().is_ok());
    }
}
