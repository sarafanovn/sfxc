//! SQLite library: sounds with an autosaved draft and an append-only version history.

use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use sfxc_core::patch::SoundPatch;

/// Each entry upgrades the schema by one version. Never edit an entry after release.
const MIGRATIONS: &[&str] = &[r#"
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
"#];

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
    /// Opens or creates the library. Fails (without modifying the file) if it is damaged.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
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
        let applied: i64 =
            conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute("INSERT INTO schema_migrations (version) VALUES (?1)", [i as i64 + 1])?;
            tx.commit()?;
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

    /// Snapshots the draft as a version. If the draft equals the latest version, no new
    /// version is created; a non-empty note is attached to the latest one instead.
    pub fn commit_version(&self, sound_id: i64, note: &str, exported: bool, now: i64) -> Result<i64> {
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
        if exported {
            tx.execute("UPDATE versions SET exported_at = ?1 WHERE id = ?2", params![now, vid])?;
        }
        tx.execute("UPDATE sounds SET current_version_id = ?1, updated_at = ?2 WHERE id = ?3", params![vid, now, sound_id])?;
        tx.commit()?;
        Ok(vid)
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

    /// Keeps the current draft as a version, then makes `version_id` the draft.
    pub fn restore_version(&self, sound_id: i64, version_id: i64, now: i64) -> Result<SoundPatch> {
        let patch = self.load_version(version_id)?;
        self.commit_version(sound_id, "", false, now)?;
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
        assert_eq!(s.commit_version(id, "", false, 11).unwrap(), v1);

        s.save_draft(id, &patch(400.0), 12).unwrap();
        assert!(s.draft_differs_from_latest(id).unwrap());
        let v2 = s.commit_version(id, "higher", false, 13).unwrap();
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
        let v1 = s.commit_version(id, "keeper", false, 11).unwrap();
        let v = s.list_versions(id).unwrap();
        assert_eq!((v.len(), v[0].id, v[0].note.as_str()), (1, v1, "keeper"));
    }

    #[test]
    fn export_marks_version() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_sound("a", &patch(300.0), 10).unwrap();
        s.commit_version(id, "", true, 20).unwrap();
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
}
