//! Migration 4: the export links `sfxc-cli export --to` remembered become the project "Linked".

use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Deserialize;

use crate::projects::{check_rel_structure, path_key, MemberOptions};

#[derive(Deserialize)]
struct OldLink {
    path: String,
    options: MemberOptions,
}

struct Link {
    sound_id: i64,
    path: PathBuf,
    options: MemberOptions,
    exported_version_id: Option<i64>,
}

pub(crate) fn export_links_to_project(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.export_link, CASE WHEN v.exported_at IS NOT NULL THEN s.current_version_id END
         FROM sounds s LEFT JOIN versions v ON v.id = s.current_version_id
         WHERE s.export_link IS NOT NULL ORDER BY s.id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)))?;
    let mut links = Vec::new();
    for row in rows {
        let (sound_id, json, exported_version_id) = row?;
        let Ok(old) = serde_json::from_str::<OldLink>(&json) else { continue };
        let path = lexical(Path::new(&old.path));
        if path.is_absolute() && path.file_name().is_some() {
            links.push(Link { sound_id, path, options: old.options, exported_version_id });
        }
    }
    if links.is_empty() {
        return Ok(());
    }
    let root = common_dir(links.iter().map(|l| l.path.as_path()));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    conn.execute("INSERT INTO projects (name, root, created_at) VALUES ('Linked', ?1, ?2)", params![root.to_string_lossy().into_owned(), now])?;
    let project_id = conn.last_insert_rowid();
    for link in links {
        let Ok(rest) = link.path.strip_prefix(&root) else { continue };
        let parts: Vec<String> = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        // The extension is not checked: `--format ogg --to x.wav` was allowed, and the link is kept as it was.
        let Ok(rel) = check_rel_structure(&parts.join("/")) else { continue };
        // Ordered by id, so of two links to one file the lower id wins.
        conn.execute(
            "INSERT OR IGNORE INTO project_sounds (project_id, sound_id, rel_path, path_key, options_json, exported_version_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project_id, link.sound_id, rel, path_key(&rel), serde_json::to_string(&link.options)?, link.exported_version_id],
        )?;
    }
    Ok(())
}

/// Drops `.` and folds `..` without touching the disk.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// The deepest folder that holds every path; `/` at worst.
fn common_dir<'a>(paths: impl Iterator<Item = &'a Path>) -> PathBuf {
    let mut common: Option<Vec<Component<'a>>> = None;
    for p in paths {
        let dir: Vec<Component> = p.parent().map(|d| d.components().collect()).unwrap_or_default();
        common = Some(match common {
            None => dir,
            Some(c) => c.into_iter().zip(dir).take_while(|(a, b)| a == b).map(|(a, _)| a).collect(),
        });
    }
    let dir: PathBuf = common.unwrap_or_default().into_iter().collect();
    if dir.as_os_str().is_empty() { PathBuf::from("/") } else { dir }
}
