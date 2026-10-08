use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sfxc_core::generators::generate;
use sfxc_core::patch::SoundPatch;
use sfxc_core::patch_edit::unknown_keys;
use sfxc_store::Store;

use crate::cli::Command;

pub struct Ctx {
    pub now: i64,
    /// Seed used when a command needs one and none was given.
    pub seed: u64,
    /// Relative paths resolve against this.
    pub cwd: PathBuf,
}

pub fn run(store: &Store, command: Command, ctx: &Ctx) -> Result<Value> {
    match command {
        Command::Schema => Ok(crate::schema::schema()),
        Command::List { search } => list(store, &search),
        Command::Show { sound } => show(store, store.find_sound(&sound)?),
        Command::New { name, category, mode, seed, patch } => {
            let name = name.trim();
            ensure_name_free(store, name, None)?;
            let (patch, warnings, seed) = match (category, patch) {
                (Some(c), None) => {
                    let seed = seed.unwrap_or(ctx.seed);
                    (generate(c.category(), mode.mode(), seed), Vec::new(), Some(seed))
                }
                (None, Some(src)) => {
                    let (p, w) = parse_patch(&read_source(&src, &ctx.cwd)?)?;
                    (p, w, None)
                }
                _ => bail!("give either --category or --patch"),
            };
            let id = store.create_sound(name, &patch, ctx.now)?;
            store.commit_version(id, "cli: new", ctx.now)?;
            let mut out = show(store, id)?;
            if let Some(seed) = seed {
                out["seed"] = json!(seed);
            }
            attach_warnings(&mut out, warnings);
            Ok(out)
        }
    }
}

fn list(store: &Store, search: &str) -> Result<Value> {
    let mut out = Vec::new();
    for s in store.list_sounds(search)? {
        out.push(json!({ "id": s.id, "name": s.name, "tags": s.tags, "updated_at": s.updated_at, "export_link": store.export_link(s.id)? }));
    }
    Ok(Value::Array(out))
}

pub(crate) fn show(store: &Store, id: i64) -> Result<Value> {
    let (name, tags) = store.sound_meta(id)?;
    Ok(json!({
        "id": id,
        "name": name,
        "tags": tags,
        "patch": store.load_draft(id)?,
        "current_version_id": store.current_version_id(id)?,
        "versions": versions_json(store, id)?,
        "export_link": store.export_link(id)?,
    }))
}

pub(crate) fn versions_json(store: &Store, id: i64) -> Result<Value> {
    let versions = store.list_versions(id)?;
    Ok(versions
        .into_iter()
        .map(|v| {
            json!({ "id": v.id, "parent_version_id": v.parent_version_id, "created_at": v.created_at, "note": v.note, "exported_at": v.exported_at, "broken": v.broken })
        })
        .collect())
}

pub(crate) fn ensure_name_free(store: &Store, name: &str, except: Option<i64>) -> Result<()> {
    if name.is_empty() {
        bail!("the name is empty");
    }
    if let Some(other) = store.sounds_named(name)?.into_iter().find(|&id| Some(id) != except) {
        bail!("a sound named `{name}` already exists (id {other}); pick another name or edit that sound");
    }
    Ok(())
}

pub(crate) fn read_source(src: &str, cwd: &Path) -> Result<String> {
    if src == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).context("cannot read the patch from stdin")?;
        return Ok(s);
    }
    let path = cwd.join(src);
    std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))
}

pub(crate) fn parse_patch(text: &str) -> Result<(SoundPatch, Vec<String>)> {
    let input: Value = serde_json::from_str(text).context("the patch is not valid JSON")?;
    let patch = SoundPatch::from_json(text).context("the patch does not match the schema (see `sfxc-cli schema`)")?;
    let warnings = unknown_keys(&input, &patch).into_iter().map(|p| format!("unknown field `{p}` was ignored")).collect();
    Ok((patch, warnings))
}

pub(crate) fn attach_warnings(out: &mut Value, warnings: Vec<String>) {
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{CategoryArg, ModeArg};

    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sfxc-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    pub(crate) fn ctx(cwd: &Path) -> Ctx {
        Ctx { now: 100, seed: 7, cwd: cwd.to_path_buf() }
    }

    pub(crate) fn new_jump(store: &Store, ctx: &Ctx, name: &str) -> i64 {
        let cmd = Command::New { name: name.into(), category: Some(CategoryArg::Jump), mode: ModeArg::Modern, seed: Some(1), patch: None };
        run(store, cmd, ctx).unwrap()["id"].as_i64().unwrap()
    }

    pub(crate) fn notes(store: &Store, id: i64) -> Vec<String> {
        store.list_versions(id).unwrap().into_iter().map(|v| v.note).collect()
    }

    fn new_from_file(store: &Store, ctx: &Ctx, name: &str, file: &str) -> Result<Value> {
        run(store, Command::New { name: name.into(), category: None, mode: ModeArg::Modern, seed: None, patch: Some(file.into()) }, ctx)
    }

    #[test]
    fn new_from_a_category_is_one_version_noted_cli_new() {
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&std::env::temp_dir());
        let id = new_jump(&s, &c, "Player jump");
        assert_eq!(notes(&s, id), vec!["cli: new"]);
        assert_eq!(s.find_sound("Player jump").unwrap(), id);
    }

    #[test]
    fn new_trims_and_keeps_non_ascii_names() {
        let s = Store::open_in_memory().unwrap();
        let id = new_jump(&s, &ctx(&std::env::temp_dir()), "  Прыжок  ");
        assert_eq!(s.sound_meta(id).unwrap().0, "Прыжок");
        assert_eq!(s.find_sound("Прыжок").unwrap(), id);
    }

    #[test]
    fn new_refuses_a_taken_name() {
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&std::env::temp_dir());
        let id = new_jump(&s, &c, "jump");
        let cmd = Command::New { name: "jump".into(), category: Some(CategoryArg::Coin), mode: ModeArg::Modern, seed: None, patch: None };
        let e = run(&s, cmd, &c).unwrap_err().to_string();
        assert!(e.contains("already exists") && e.contains(&format!("id {id}")), "{e}");
    }

    #[test]
    fn new_from_a_patch_file_warns_about_unknown_keys() {
        let dir = temp_dir("newpatch");
        std::fs::write(dir.join("p.json"), r#"{"layers":[{"pitch":{"basefreq":880}}]}"#).unwrap();
        let s = Store::open_in_memory().unwrap();
        let out = new_from_file(&s, &ctx(&dir), "typo", "p.json").unwrap();
        assert_eq!(out["warnings"], json!(["unknown field `layers.0.pitch.basefreq` was ignored"]));
    }

    #[test]
    fn json_that_is_not_a_patch_is_a_readable_error() {
        let dir = temp_dir("notpatch");
        let s = Store::open_in_memory().unwrap();
        for (i, text) in ["[1, 2]", "\"x\"", "{"].iter().enumerate() {
            let file = format!("p{i}.json");
            std::fs::write(dir.join(&file), text).unwrap();
            let e = new_from_file(&s, &ctx(&dir), &format!("bad {i}"), &file).unwrap_err();
            assert!(format!("{e:#}").contains("patch"), "{e:#}");
        }
        assert!(s.list_sounds("").unwrap().is_empty());
    }

    #[test]
    fn list_and_show_carry_the_export_link() {
        let s = Store::open_in_memory().unwrap();
        let id = new_jump(&s, &ctx(&std::env::temp_dir()), "jump");
        let list = run(&s, Command::List { search: String::new() }, &ctx(&std::env::temp_dir())).unwrap();
        assert_eq!(list[0]["id"], json!(id));
        assert_eq!(list[0]["export_link"], Value::Null);
        let show = run(&s, Command::Show { sound: "jump".into() }, &ctx(&std::env::temp_dir())).unwrap();
        assert_eq!(show["versions"].as_array().unwrap().len(), 1);
        SoundPatch::from_json(&show["patch"].to_string()).unwrap();
    }
}
