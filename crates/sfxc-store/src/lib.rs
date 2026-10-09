//! SQLite library shared by the app and the CLI: sounds with an autosaved draft and an append-only version history.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sfxc_core::export::ExportOptions;
use sfxc_core::patch::SoundPatch;

mod link_migration;
mod projects;
pub mod project_export;

pub use projects::{
    normalize_rel_path, relative_to_root, replace_extension, MemberFormat, MemberOptions, Membership, Project, ProjectSummary,
    DEFAULT_OGG_QUALITY, RATE_RANGE,
};

/// One schema step. `post` runs after `sql` in the same transaction.
struct Migration {
    sql: &'static str,
    post: Option<fn(&Connection) -> Result<()>>,
    /// Code that knows fewer migrations must refuse a library after this step.
    breaking: bool,
}

const fn step(sql: &'static str) -> Migration {
    Migration { sql, post: None, breaking: false }
}

/// Each entry upgrades the schema by one version. Never edit an entry after release.
const MIGRATIONS: &[Migration] = &[
    step(r#"
    CREATE TABLE sounds (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        tags TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        current_version_id INTEGER,
        draft_json TEXT NOT NULL,
        last_export_dir TEXT
    );
    CREATE TABLE versions (
        id INTEGER PRIMARY KEY,
        sound_id INTEGER NOT NULL REFERENCES sounds(id) ON DELETE CASCADE,
        parent_version_id INTEGER,
        created_at INTEGER NOT NULL,
        note TEXT NOT NULL DEFAULT '',
        exported_at INTEGER,
        patch_json TEXT NOT NULL
    );
    CREATE INDEX versions_by_sound ON versions(sound_id, id);
"#),
    step(r#"
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
"#),
    step(r#"
    ALTER TABLE sounds ADD COLUMN export_link TEXT;
"#),
    Migration {
        sql: r#"
    CREATE TABLE projects (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE,
        root TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE project_sounds (
        project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        sound_id INTEGER NOT NULL REFERENCES sounds(id) ON DELETE CASCADE,
        rel_path TEXT NOT NULL,
        path_key TEXT NOT NULL,
        options_json TEXT NOT NULL,
        exported_version_id INTEGER,
        PRIMARY KEY (project_id, sound_id),
        UNIQUE (project_id, path_key)
    );
    CREATE INDEX project_sounds_by_sound ON project_sounds(sound_id);
"#,
        post: Some(link_migration::export_links_to_project),
        breaking: false,
    },
];

const NEWER_LIBRARY: &str = "this library was upgraded by a newer sfxc; update the app and sfxc-cli";

/// Where `sfxc-cli export` writes a sound and with which settings. `path` is absolute.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportLink {
    pub path: String,
    pub options: ExportOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundSummary {
    pub id: i64,
    pub name: String,
    pub tags: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VersionInfo {
    pub id: i64,
    pub parent_version_id: Option<i64>,
    pub created_at: i64,
    pub note: String,
    pub exported_at: Option<i64>,
    pub broken: bool,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        Self::init(conn, Some(path))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    pub fn export_link(&self, id: i64) -> Result<Option<ExportLink>> {
        let json: Option<String> = self.conn.query_row("SELECT export_link FROM sounds WHERE id = ?1", [id], |r| r.get(0))?;
        json.map(|j| serde_json::from_str(&j).context("export link is unreadable")).transpose()
    }

    pub fn set_export_link(&self, id: i64, link: &ExportLink) -> Result<()> {
        self.conn.execute("UPDATE sounds SET export_link = ?1 WHERE id = ?2", params![serde_json::to_string(link)?, id])?;
        Ok(())
    }

    pub fn linked_sounds(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT id FROM sounds WHERE export_link IS NOT NULL ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Changes whenever another connection commits; this connection's own writes leave it as is.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self.conn.query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    pub fn sounds_named(&self, name: &str) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT id FROM sounds WHERE name = ?1 ORDER BY id")?;
        let rows = stmt.query_map([name], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// An existing id, else an exact (trimmed) name that only one sound has.
    pub fn find_sound(&self, reference: &str) -> Result<i64> {
        let reference = reference.trim();
        if let Ok(id) = reference.parse::<i64>() {
            let exists: Option<i64> = self.conn.query_row("SELECT id FROM sounds WHERE id = ?1", [id], |r| r.get(0)).optional()?;
            if exists.is_some() {
                return Ok(id);
            }
        }
        match self.sounds_named(reference)?.as_slice() {
            [] => bail!("no sound with id or name `{reference}`; `sfxc-cli list` shows them"),
            [id] => Ok(*id),
            ids => {
                let ids: Vec<String> = ids.iter().map(i64::to_string).collect();
                bail!("{} sounds are named `{reference}` (ids {}); use an id", ids.len(), ids.join(", "))
            }
        }
    }

    pub fn version_owner(&self, version_id: i64) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT sound_id FROM versions WHERE id = ?1", [version_id], |r| r.get(0)).optional()?)
    }

    pub fn draft_json_if_exists(&self, id: i64) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT draft_json FROM sounds WHERE id = ?1", [id], |r| r.get(0)).optional()?)
    }

    fn init(mut conn: Connection, path: Option<&Path>) -> Result<Self> {
        // Read-only probe first: a non-database file fails here before anything is written.
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .context("library file is not a readable database")?;
        if check != "ok" {
            bail!("library database is damaged: {check}");
        }
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        // journal_mode returns a row ("wal", or "memory" for in-memory DBs), so read it.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)")?;
        let required: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if required > MIGRATIONS.len() as i64 {
            bail!(NEWER_LIBRARY);
        }
        let applied = applied_version(&conn)?;
        if applied < MIGRATIONS.len() as i64 {
            if let Some(path) = path.filter(|_| applied > 0) {
                backup(&conn, path, applied)?;
            }
            for (i, m) in MIGRATIONS.iter().enumerate() {
                let version = i as i64 + 1;
                // IMMEDIATE takes the write lock up front: a second process waits here, then sees the step done.
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if applied_version(&tx)? >= version {
                    continue;
                }
                tx.execute_batch(m.sql)?;
                if let Some(post) = m.post {
                    post(&tx)?;
                }
                if m.breaking {
                    tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
                }
                tx.execute("INSERT INTO schema_migrations (version) VALUES (?1)", [version])?;
                tx.commit()?;
            }
        }
        Ok(Self { conn })
    }

    pub fn create_sound(&self, name: &str, patch: &SoundPatch, now: i64) -> Result<i64> {
        let json = patch.to_json();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO sounds (name, created_at, updated_at, draft_json) VALUES (?1, ?2, ?2, ?3)",
            params![name, now, json],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO versions (sound_id, created_at, patch_json) VALUES (?1, ?2, ?3)", params![id, now, json])?;
        let vid = tx.last_insert_rowid();
        tx.execute("UPDATE sounds SET current_version_id = ?1 WHERE id = ?2", params![vid, id])?;
        tx.commit()?;
        Ok(id)
    }

    pub fn list_sounds(&self, search: &str) -> Result<Vec<SoundSummary>> {
        let pattern = format!("%{}%", search.trim());
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, updated_at FROM sounds
             WHERE name LIKE ?1 OR tags LIKE ?1 ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([pattern], |r| {
            Ok(SoundSummary { id: r.get(0)?, name: r.get(1)?, tags: r.get(2)?, updated_at: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn sound_meta(&self, id: i64) -> Result<(String, String)> {
        Ok(self.conn.query_row("SELECT name, tags FROM sounds WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?)
    }

    pub fn rename_sound(&self, id: i64, name: &str, now: i64) -> Result<()> {
        self.conn.execute("UPDATE sounds SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id])?;
        Ok(())
    }

    pub fn set_tags(&self, id: i64, tags: &str, now: i64) -> Result<()> {
        self.conn.execute("UPDATE sounds SET tags = ?1, updated_at = ?2 WHERE id = ?3", params![tags, now, id])?;
        Ok(())
    }

    pub fn delete_sound(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM sounds WHERE id = ?1", [id])?;
        Ok(())
    }

    fn draft_json(&self, id: i64) -> Result<String> {
        Ok(self.conn.query_row("SELECT draft_json FROM sounds WHERE id = ?1", [id], |r| r.get(0))?)
    }

    pub fn load_draft(&self, id: i64) -> Result<SoundPatch> {
        SoundPatch::from_json(&self.draft_json(id)?).context("draft is unreadable")
    }

    pub fn save_draft(&self, id: i64, patch: &SoundPatch, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sounds SET draft_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![patch.to_json(), now, id],
        )?;
        Ok(())
    }

    fn latest_version(&self, sound_id: i64) -> Result<Option<(i64, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, patch_json FROM versions WHERE sound_id = ?1 ORDER BY id DESC LIMIT 1",
                [sound_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn draft_differs_from_latest(&self, sound_id: i64) -> Result<bool> {
        let draft = self.draft_json(sound_id)?;
        Ok(self.latest_version(sound_id)?.is_none_or(|(_, json)| json != draft))
    }

    pub fn commit_version(&self, sound_id: i64, note: &str, now: i64) -> Result<i64> {
        let draft = self.draft_json(sound_id)?;
        let tx = self.conn.unchecked_transaction()?;
        let vid = match self.latest_version(sound_id)? {
            Some((id, json)) if json == draft => {
                if !note.is_empty() {
                    tx.execute("UPDATE versions SET note = ?1 WHERE id = ?2", params![note, id])?;
                }
                id
            }
            _ => {
                let parent: Option<i64> =
                    tx.query_row("SELECT current_version_id FROM sounds WHERE id = ?1", [sound_id], |r| r.get(0))?;
                tx.execute(
                    "INSERT INTO versions (sound_id, parent_version_id, created_at, note, patch_json)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![sound_id, parent, now, note, draft],
                )?;
                tx.last_insert_rowid()
            }
        };
        tx.execute("UPDATE sounds SET current_version_id = ?1, updated_at = ?2 WHERE id = ?3", params![vid, now, sound_id])?;
        tx.commit()?;
        Ok(vid)
    }

    pub fn mark_exported(&self, version_id: i64, now: i64) -> Result<()> {
        self.conn.execute("UPDATE versions SET exported_at = ?1 WHERE id = ?2", params![now, version_id])?;
        Ok(())
    }

    pub fn list_versions(&self, sound_id: i64) -> Result<Vec<VersionInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_version_id, created_at, note, exported_at, patch_json
             FROM versions WHERE sound_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([sound_id], |r| {
            let json: String = r.get(5)?;
            Ok(VersionInfo {
                id: r.get(0)?,
                parent_version_id: r.get(1)?,
                created_at: r.get(2)?,
                note: r.get(3)?,
                exported_at: r.get(4)?,
                broken: SoundPatch::from_json(&json).is_err(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn current_version_id(&self, sound_id: i64) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT current_version_id FROM sounds WHERE id = ?1", [sound_id], |r| r.get(0))?)
    }

    pub fn load_version(&self, version_id: i64) -> Result<SoundPatch> {
        let json: String =
            self.conn.query_row("SELECT patch_json FROM versions WHERE id = ?1", [version_id], |r| r.get(0))?;
        SoundPatch::from_json(&json).context("version is unreadable")
    }

    pub fn restore_version(&self, sound_id: i64, version_id: i64, now: i64) -> Result<SoundPatch> {
        let patch = self.load_version(version_id)?;
        self.commit_version(sound_id, "", now)?;
        self.conn.execute(
            "UPDATE sounds SET draft_json = ?1, current_version_id = ?2, updated_at = ?3 WHERE id = ?4",
            params![patch.to_json(), version_id, now, sound_id],
        )?;
        Ok(patch)
    }

    pub fn duplicate_from_version(&self, version_id: i64, name: &str, now: i64) -> Result<i64> {
        let patch = self.load_version(version_id)?;
        let json = patch.to_json();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO sounds (name, created_at, updated_at, draft_json) VALUES (?1, ?2, ?2, ?3)",
            params![name, now, json],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO versions (sound_id, parent_version_id, created_at, patch_json) VALUES (?1, ?2, ?3, ?4)",
            params![id, version_id, now, json],
        )?;
        let vid = tx.last_insert_rowid();
        tx.execute("UPDATE sounds SET current_version_id = ?1 WHERE id = ?2", params![vid, id])?;
        tx.commit()?;
        Ok(id)
    }

    pub fn last_version_time(&self, sound_id: i64) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT MAX(created_at) FROM versions WHERE sound_id = ?1", [sound_id], |r| r.get(0))?)
    }

    pub fn export_dir(&self, sound_id: i64) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT last_export_dir FROM sounds WHERE id = ?1", [sound_id], |r| r.get(0))?)
    }

    pub fn set_export_dir(&self, sound_id: i64, dir: &str) -> Result<()> {
        self.conn.execute("UPDATE sounds SET last_export_dir = ?1 WHERE id = ?2", params![dir, sound_id])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

/// `~/Library/Application Support/sfxc/library.db` on macOS; `SFXC_LIBRARY` overrides it.
pub fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SFXC_LIBRARY") {
        return PathBuf::from(path);
    }
    ProjectDirs::from("", "", "sfxc")
        .map(|d| d.data_dir().join("library.db"))
        .unwrap_or_else(|| PathBuf::from("library.db"))
}

/// Next to the library; remembers the last update check so the app and the CLI ask GitHub at most once a day.
pub fn update_cache_path() -> PathBuf {
    library_path().with_file_name("update-check")
}

fn applied_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))?)
}

/// `<library>.v{applied}.bak` before an upgrade. An existing backup is kept; another process making it at the
/// same moment is fine.
fn backup(conn: &Connection, library: &Path, applied: i64) -> Result<()> {
    let target = PathBuf::from(format!("{}.v{applied}.bak", library.display()));
    if target.exists() {
        return Ok(());
    }
    let name = target.to_string_lossy().into_owned();
    match conn.execute("VACUUM INTO ?1", [&name]) {
        Ok(_) => Ok(()),
        Err(_) if target.exists() => Ok(()),
        Err(e) => Err(e).with_context(|| format!("cannot back up the library to {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sfxc_core::patch::{Mode, SoundPatch};

    fn patch(freq: f32) -> SoundPatch {
        let mut p = SoundPatch::default();
        p.layers[0].pitch.base_freq = freq;
        p
    }

    fn temp_db(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("sfxc-store-{}-{name}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn create_sound_has_one_version_and_draft() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("jump", &patch(300.0), 10).unwrap();
        assert_eq!(s.load_draft(id).unwrap(), patch(300.0));
        let v = s.list_versions(id).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(s.current_version_id(id).unwrap(), Some(v[0].id));
        assert_eq!(s.sound_meta(id).unwrap(), ("jump".to_string(), String::new()));
    }

    #[test]
    fn commit_creates_child_version_only_when_changed() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        let v1 = s.current_version_id(id).unwrap().unwrap();
        assert!(!s.draft_differs_from_latest(id).unwrap());
        assert_eq!(s.commit_version(id, "", 11).unwrap(), v1);

        s.save_draft(id, &patch(400.0), 12).unwrap();
        assert!(s.draft_differs_from_latest(id).unwrap());
        let v2 = s.commit_version(id, "higher", 13).unwrap();
        assert_ne!(v2, v1);
        let versions = s.list_versions(id).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].id, v2);
        assert_eq!(versions[0].parent_version_id, Some(v1));
        assert_eq!(versions[0].note, "higher");
        assert_eq!(s.load_version(v2).unwrap(), patch(400.0));
    }

    #[test]
    fn note_on_unchanged_draft_annotates_latest() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        let v1 = s.commit_version(id, "keeper", 11).unwrap();
        let v = s.list_versions(id).unwrap();
        assert_eq!((v.len(), v[0].id, v[0].note.as_str()), (1, v1, "keeper"));
    }

    #[test]
    fn export_marks_version() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        let v = s.commit_version(id, "", 15).unwrap();
        s.mark_exported(v, 20).unwrap();
        assert_eq!(s.list_versions(id).unwrap()[0].exported_at, Some(20));
    }

    #[test]
    fn restore_commits_draft_first() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        let v1 = s.current_version_id(id).unwrap().unwrap();
        s.save_draft(id, &patch(500.0), 11).unwrap();
        let restored = s.restore_version(id, v1, 12).unwrap();
        assert_eq!(restored, patch(300.0));
        assert_eq!(s.load_draft(id).unwrap(), patch(300.0));
        let versions = s.list_versions(id).unwrap();
        assert_eq!(versions.len(), 2, "unsaved 500 Hz draft must be kept as a version");
        assert_eq!(s.load_version(versions[0].id).unwrap(), patch(500.0));
        assert_eq!(s.current_version_id(id).unwrap(), Some(v1));
    }

    #[test]
    fn duplicate_from_version_links_parent() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        let v1 = s.current_version_id(id).unwrap().unwrap();
        let dup = s.duplicate_from_version(v1, "a copy", 11).unwrap();
        assert_ne!(dup, id);
        let v = s.list_versions(dup).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].parent_version_id, Some(v1));
        assert_eq!(s.load_draft(dup).unwrap(), patch(300.0));
    }

    #[test]
    fn delete_cascades() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        s.delete_sound(id).unwrap();
        assert!(s.list_sounds("").unwrap().is_empty());
        assert!(s.list_versions(id).unwrap().is_empty());
    }

    #[test]
    fn list_search_and_order() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_sound("laser", &patch(300.0), 10).unwrap();
        let b = s.create_sound("coin", &patch(300.0), 20).unwrap();
        s.set_tags(a, "ui retro", 30).unwrap();
        let all = s.list_sounds("").unwrap();
        assert_eq!(all.iter().map(|x| x.id).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(s.list_sounds("coi").unwrap()[0].id, b);
        assert_eq!(s.list_sounds("retro").unwrap()[0].id, a);
        s.rename_sound(b, "big coin", 40).unwrap();
        assert_eq!(s.list_sounds("").unwrap()[0].name, "big coin");
    }

    #[test]
    fn broken_version_is_flagged_not_fatal() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        s.conn.execute("UPDATE versions SET patch_json = '{' WHERE sound_id = ?1", [id]).unwrap();
        let v = s.list_versions(id).unwrap();
        assert!(v[0].broken);
        assert!(s.load_version(v[0].id).is_err());
    }

    #[test]
    fn export_dir_round_trip() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        assert_eq!(s.export_dir(id).unwrap(), None);
        s.set_export_dir(id, "/tmp/sfx").unwrap();
        assert_eq!(s.export_dir(id).unwrap().as_deref(), Some("/tmp/sfx"));
    }

    #[test]
    fn reopen_file_keeps_data() {
        let path = temp_db("reopen");
        let id = Store::open(&path).unwrap().create_sound("a", &SoundPatch { mode: Mode::Bit8, ..Default::default() }, 1).unwrap();
        let s = Store::open(&path).unwrap();
        assert_eq!(s.load_draft(id).unwrap().mode, Mode::Bit8);
        drop(s);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn garbage_file_is_rejected_and_untouched() {
        let path = temp_db("garbage");
        let junk = b"this is definitely not sqlite, but it is long enough to look like a header....".repeat(20);
        std::fs::write(&path, &junk).unwrap();
        assert!(Store::open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), junk);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn settings_round_trip() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.setting("theme").unwrap(), None);
        s.set_setting("theme", "dark").unwrap();
        s.set_setting("theme", "light").unwrap();
        assert_eq!(s.setting("theme").unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn write_waits_for_another_connections_lock() {
        let path = temp_db("busy");
        let gui = Store::open(&path).unwrap();
        let cli = Store::open(&path).unwrap();
        gui.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            gui.conn.execute_batch("COMMIT").unwrap();
            gui
        });
        cli.create_sound("from cli", &patch(300.0), 1).expect("busy_timeout should wait for the lock");
        drop(release.join().unwrap());
        drop(cli);
        let _ = std::fs::remove_file(&path);
    }

    use sfxc_core::export::{ExportFormat, ExportOptions};
    use serde_json::json;

    /// A library file at schema `version`, in WAL like every real library.
    fn library_at_version(path: &Path, version: usize) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(())).unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY)").unwrap();
        for (i, m) in MIGRATIONS[..version].iter().enumerate() {
            conn.execute_batch(m.sql).unwrap();
            conn.execute("INSERT INTO schema_migrations (version) VALUES (?1)", [i as i64 + 1]).unwrap();
        }
        conn
    }

    fn cleanup(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        for v in 0..=MIGRATIONS.len() {
            let _ = std::fs::remove_file(format!("{}.v{v}.bak", path.display()));
        }
    }

    /// Adds a v3 sound with one version; `exported` sets `exported_at` on it.
    fn v3_sound(conn: &Connection, name: &str, link: Option<serde_json::Value>, exported: bool) -> i64 {
        let json = SoundPatch::default().to_json();
        conn.execute(
            "INSERT INTO sounds (name, created_at, updated_at, draft_json, export_link) VALUES (?1, 1, 1, ?2, ?3)",
            params![name, json, link.map(|l| l.to_string())],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO versions (sound_id, created_at, exported_at, patch_json) VALUES (?1, 1, ?2, ?3)",
            params![id, exported.then_some(5), json],
        )
        .unwrap();
        conn.execute("UPDATE sounds SET current_version_id = ?1 WHERE id = ?2", params![conn.last_insert_rowid(), id]).unwrap();
        id
    }

    #[test]
    fn migration_4_turns_export_links_into_the_linked_project() {
        let path = temp_db("links");
        {
            let conn = library_at_version(&path, 3);
            let wav = serde_json::to_value(ExportOptions::default()).unwrap();
            let ogg = serde_json::to_value(ExportOptions { format: ExportFormat::Ogg { quality: 4.0 }, ..Default::default() }).unwrap();
            v3_sound(&conn, "jump", Some(json!({ "path": "/games/a/sfx/jump.wav", "options": wav })), true);
            v3_sound(&conn, "music", Some(json!({ "path": "/games/a/music/../music/theme.ogg", "options": ogg })), false);
            v3_sound(&conn, "twin", Some(json!({ "path": "/games/a/SFX/jump.wav", "options": wav })), true);
            v3_sound(&conn, "broken", Some(json!({ "nope": 1 })), false);
            v3_sound(&conn, "plain", None, false);
        }
        let s = Store::open(&path).unwrap();
        let projects = s.list_projects().unwrap();
        assert_eq!(projects.len(), 1);
        let p = &projects[0].project;
        assert_eq!((p.name.as_str(), p.root.as_str()), ("Linked", "/games/a"));
        let members = s.project_sounds(p.id, "").unwrap();
        let got: Vec<(&str, &str, bool)> = members.iter().map(|m| (m.sound_name.as_str(), m.rel_path.as_str(), m.changed())).collect();
        assert_eq!(got, [("music", "music/theme.ogg", true), ("jump", "sfx/jump.wav", false)]);
        assert_eq!(members[1].options.format, MemberFormat::Wav { bits: Some(16) });
        let twin = s.find_sound("twin").unwrap();
        assert!(s.sound_projects(twin).unwrap().is_empty(), "the same file (ignoring case) keeps the lower id");
        drop(s);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.list_projects().unwrap().len(), 1, "reopening does not migrate twice");
        drop(s);
        cleanup(&path);
    }

    #[test]
    fn migration_4_without_links_creates_no_project() {
        let path = temp_db("nolinks");
        drop({
            let conn = library_at_version(&path, 3);
            v3_sound(&conn, "plain", None, false);
            conn
        });
        let s = Store::open(&path).unwrap();
        assert!(s.list_projects().unwrap().is_empty());
        drop(s);
        cleanup(&path);
    }

    #[test]
    fn a_library_needing_newer_code_is_refused() {
        let path = temp_db("newer");
        drop(Store::open(&path).unwrap());
        Connection::open(&path).unwrap().execute_batch("PRAGMA user_version = 999").unwrap();
        let err = Store::open(&path).err().expect("must refuse").to_string();
        assert!(err.contains("newer sfxc"), "{err}");
        cleanup(&path);
    }

    #[test]
    fn additive_steps_from_newer_code_do_not_block_opening() {
        let path = temp_db("additive");
        drop(Store::open(&path).unwrap());
        Connection::open(&path).unwrap().execute("INSERT INTO schema_migrations (version) VALUES (999)", []).unwrap();
        Store::open(&path).expect("unknown additive steps are fine");
        cleanup(&path);
    }

    #[test]
    fn upgrade_backs_up_first_and_keeps_an_existing_backup() {
        let path = temp_db("backup");
        let backup = PathBuf::from(format!("{}.v2.bak", path.display()));
        drop(library_at_version(&path, 2));
        drop(Store::open(&path).unwrap());
        let old = Connection::open(&backup).unwrap();
        let v: i64 = old.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 2, "the backup is the library before the upgrade");
        drop(old);

        cleanup(&path);
        drop(library_at_version(&path, 2));
        std::fs::write(&backup, b"keep").unwrap();
        drop(Store::open(&path).unwrap());
        assert_eq!(std::fs::read(&backup).unwrap(), b"keep");
        cleanup(&path);
    }

    #[test]
    fn a_new_library_is_not_backed_up() {
        let path = temp_db("fresh");
        drop(Store::open(&path).unwrap());
        for v in 0..=MIGRATIONS.len() {
            assert!(!PathBuf::from(format!("{}.v{v}.bak", path.display())).exists());
        }
        cleanup(&path);
    }

    #[test]
    fn two_processes_upgrading_at_once_both_succeed() {
        let path = temp_db("race");
        drop(library_at_version(&path, 1));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let p = path.clone();
                std::thread::spawn(move || Store::open(&p).map(|_| ()))
            })
            .collect();
        for h in handles {
            h.join().unwrap().expect("every opener succeeds");
        }
        let n: i64 = Connection::open(&path).unwrap().query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0)).unwrap();
        assert_eq!(n, MIGRATIONS.len() as i64);
        cleanup(&path);
    }

    #[test]
    fn migration_3_upgrades_a_v2_library() {
        let path = temp_db("v2");
        {
            let conn = library_at_version(&path, 2);
            conn.execute(
                "INSERT INTO sounds (name, created_at, updated_at, draft_json) VALUES ('old', 1, 1, ?1)",
                [SoundPatch::default().to_json()],
            )
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        let id = s.find_sound("old").unwrap();
        assert_eq!(s.export_link(id).unwrap(), None);
        drop(s);
        cleanup(&path);
    }

    #[test]
    fn export_link_round_trip_and_listing() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_sound("a", &patch(300.0), 10).unwrap();
        let _b = s.create_sound("b", &patch(300.0), 10).unwrap();
        let link = ExportLink {
            path: "/tmp/sfx/a.ogg".into(),
            options: ExportOptions { format: ExportFormat::Ogg { quality: 5.0 }, ..Default::default() },
        };
        s.set_export_link(a, &link).unwrap();
        assert_eq!(s.export_link(a).unwrap(), Some(link));
        assert_eq!(s.linked_sounds().unwrap(), vec![a]);
    }

    #[test]
    fn find_sound_by_id_then_exact_name() {
        let s = Store::open_in_memory().unwrap();
        let jump = s.create_sound("Player jump", &patch(300.0), 10).unwrap();
        let numeric = s.create_sound("4242", &patch(300.0), 10).unwrap();
        let named_like_jump_id = s.create_sound(&jump.to_string(), &patch(300.0), 10).unwrap();
        assert_eq!(s.find_sound("Player jump").unwrap(), jump);
        assert_eq!(s.find_sound(" Player jump ").unwrap(), jump);
        assert_eq!(s.find_sound(&jump.to_string()).unwrap(), jump, "an existing id wins over a name");
        assert_eq!(s.find_sound("4242").unwrap(), numeric, "no sound has id 4242, so the name matches");
        assert_ne!(named_like_jump_id, jump);
        let missing = s.find_sound("nope").unwrap_err().to_string();
        assert!(missing.contains("nope"), "{missing}");
    }

    #[test]
    fn find_sound_refuses_an_ambiguous_name() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_sound("twin", &patch(300.0), 10).unwrap();
        let b = s.create_sound("twin", &patch(300.0), 10).unwrap();
        assert_eq!(s.sounds_named("twin").unwrap(), vec![a, b]);
        let err = s.find_sound("twin").unwrap_err().to_string();
        assert!(err.contains(&a.to_string()) && err.contains(&b.to_string()), "{err}");
    }

    #[test]
    fn data_version_moves_only_for_other_connections() {
        let path = temp_db("dv");
        let gui = Store::open(&path).unwrap();
        let before = gui.data_version().unwrap();
        gui.create_sound("own", &patch(300.0), 1).unwrap();
        assert_eq!(gui.data_version().unwrap(), before, "own writes do not count");
        let cli = Store::open(&path).unwrap();
        cli.create_sound("other", &patch(300.0), 2).unwrap();
        assert_ne!(gui.data_version().unwrap(), before);
        drop((gui, cli));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn version_owner_and_optional_draft() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        let v = s.current_version_id(id).unwrap().unwrap();
        assert_eq!(s.version_owner(v).unwrap(), Some(id));
        assert_eq!(s.version_owner(v + 100).unwrap(), None);
        assert_eq!(s.draft_json_if_exists(id).unwrap(), Some(patch(300.0).to_json()));
        s.delete_sound(id).unwrap();
        assert_eq!(s.draft_json_if_exists(id).unwrap(), None);
    }
}
