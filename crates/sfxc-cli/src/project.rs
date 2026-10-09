//! `sfxc-cli project …`.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sfxc_store::{project_export, relative_to_root, replace_extension, MemberFormat, MemberOptions, Membership, Project, Store, DEFAULT_OGG_QUALITY};

use crate::cli::{Bits, FormatArg, MemberArgs, ProjectCommand, ProjectExportArgs};
use crate::commands::Ctx;

pub fn run(store: &Store, command: ProjectCommand, ctx: &Ctx) -> Result<Value> {
    match command {
        ProjectCommand::New { name, root } => {
            let root = absolute_dir(ctx, root.as_deref().unwrap_or(Path::new(".")))?;
            let id = store.create_project(&name, &root, ctx.now)?;
            project_json(store, id)
        }
        ProjectCommand::List => list(store),
        ProjectCommand::Show { project } => project_json(store, store.find_project(&project)?),
        ProjectCommand::Rename { project, name } => {
            let id = store.find_project(&project)?;
            store.rename_project(id, &name)?;
            Ok(json!({ "id": id, "name": store.project(id)?.name }))
        }
        ProjectCommand::SetRoot { project, dir } => {
            let id = store.find_project(&project)?;
            store.set_project_root(id, &absolute_dir(ctx, &dir)?)?;
            project_json(store, id)
        }
        ProjectCommand::Delete { project } => {
            let id = store.find_project(&project)?;
            let removed = store.delete_project(id)?;
            Ok(json!({ "id": id, "removed_memberships": removed }))
        }
        ProjectCommand::Add { project, sound, path, options } => {
            let pid = store.find_project(&project)?;
            let sid = store.find_sound(&sound)?;
            if store.membership(pid, sid).is_ok() {
                bail!("`{sound}` is already in this project; use `sfxc-cli project set` to change its path or settings");
            }
            let name = store.sound_meta(sid)?.0;
            let (rel, options) = plan_member(store, pid, Some(sid), &name, path.as_deref(), &options)?;
            store.add_to_project(pid, sid, &rel, &options)?;
            member_json(store, pid, sid)
        }
        ProjectCommand::Set { project, sound, path, options } => {
            if path.is_none() && options.is_empty() {
                bail!("nothing to change; pass --path or an export option");
            }
            let pid = store.find_project(&project)?;
            let sid = store.find_sound(&sound)?;
            let old = store.membership(pid, sid)?;
            let root = store.project(pid)?.root;
            let rel = path.as_deref().map(|p| member_path(&root, p)).transpose()?;
            let new_options = apply_member_args(old.options, &options, rel.as_deref())?;
            let rel = rel.unwrap_or_else(|| replace_extension(&old.rel_path, new_options.extension()));
            store.set_membership(pid, sid, &rel, &new_options)?;
            let mut out = member_json(store, pid, sid)?;
            if out["path"] != json!(old.rel_path) {
                out["previous_path"] = json!(old.rel_path);
            }
            Ok(out)
        }
        ProjectCommand::Remove { project, sound } => {
            let pid = store.find_project(&project)?;
            let sid = store.find_sound(&sound)?;
            store.remove_from_project(pid, sid)?;
            Ok(json!({ "project_id": pid, "id": sid, "removed": true }))
        }
        ProjectCommand::Export(args) => export(store, ctx, &args),
    }
}

fn absolute_dir(ctx: &Ctx, dir: &Path) -> Result<String> {
    let path = std::path::absolute(ctx.cwd.join(dir)).with_context(|| format!("bad path {}", dir.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// A relative PATH is inside the project folder; an absolute one must point into it.
fn member_path(root: &str, raw: &str) -> Result<String> {
    let raw = raw.trim();
    if Path::new(raw).is_absolute() { relative_to_root(Path::new(root), Path::new(raw)) } else { Ok(raw.to_string()) }
}

fn format_of(path: &str) -> Option<FormatArg> {
    match Path::new(path).extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "wav" => Some(FormatArg::Wav),
        "ogg" => Some(FormatArg::Ogg),
        _ => None,
    }
}

/// `base` with the flags applied. `path` (when given) decides the format unless --format does.
fn apply_member_args(base: MemberOptions, args: &MemberArgs, path: Option<&str>) -> Result<MemberOptions> {
    let from_path = path.and_then(format_of);
    let format = match (args.format, from_path) {
        (Some(f), Some(p)) if f != p => bail!("--format {f:?} does not match the extension of `{}`", path.unwrap_or_default()),
        (Some(f), _) | (None, Some(f)) => Some(f),
        (None, None) => None,
    };
    let mut o = base;
    if let Some(f) = format {
        o.format = match (f, o.format) {
            (FormatArg::Wav, MemberFormat::Wav { .. }) | (FormatArg::Ogg, MemberFormat::Ogg { .. }) => o.format,
            (FormatArg::Wav, _) => MemberFormat::Wav { bits: None },
            (FormatArg::Ogg, _) => MemberFormat::Ogg { quality: DEFAULT_OGG_QUALITY },
        };
    }
    if let Some(Bits(b)) = args.bits {
        match &mut o.format {
            MemberFormat::Wav { bits } => *bits = b,
            MemberFormat::Ogg { .. } => bail!("--bits applies to WAV only; this sound is exported as OGG"),
        }
    }
    if let Some(q) = args.quality {
        match &mut o.format {
            MemberFormat::Ogg { quality } => *quality = q,
            MemberFormat::Wav { .. } => bail!("--quality applies to OGG only; this sound is exported as WAV"),
        }
    }
    if let Some(r) = args.rate {
        o.sample_rate = r;
    }
    if let Some(l) = args.length {
        o.duration = Some(l);
    }
    if args.auto_length {
        o.duration = None;
    }
    if args.normalize || args.no_normalize {
        o.normalize = args.normalize;
    }
    if args.trim || args.no_trim {
        o.trim = args.trim;
    }
    o.validate()?;
    Ok(o)
}

/// The path and settings a sound would get in a project, checked against the other sounds there. Writes nothing.
pub(crate) fn plan_member(
    store: &Store,
    project_id: i64,
    sound_id: Option<i64>,
    sound_name: &str,
    path: Option<&str>,
    args: &MemberArgs,
) -> Result<(String, MemberOptions)> {
    let root = store.project(project_id)?.root;
    let rel = path.map(|p| member_path(&root, p)).transpose()?;
    let options = apply_member_args(MemberOptions::default(), args, rel.as_deref())?;
    let rel = match rel {
        Some(r) => r,
        None => store.default_member_path(project_id, sound_name, &options)?,
    };
    Ok((store.check_member_path(project_id, sound_id, &rel, &options)?, options))
}

fn member_value(p: &Project, m: &Membership) -> Value {
    let abs = m.abs_path(&p.root);
    json!({
        "id": m.sound_id,
        "name": m.sound_name,
        "path": m.rel_path,
        "abs_path": abs,
        "options": m.options,
        "changed": m.changed(),
        "missing": !abs.is_file(),
        "exported_version_id": m.exported_version_id,
        "current_version_id": m.current_version_id,
    })
}

fn member_json(store: &Store, project_id: i64, sound_id: i64) -> Result<Value> {
    let p = store.project(project_id)?;
    let mut v = member_value(&p, &store.membership(project_id, sound_id)?);
    v["project_id"] = json!(p.id);
    v["project"] = json!(p.name);
    Ok(v)
}

pub(crate) fn project_json(store: &Store, project_id: i64) -> Result<Value> {
    let plan = project_export::plan(store, project_id)?;
    let p = &plan.project;
    let sounds: Vec<Value> = store.project_sounds(project_id, "")?.iter().map(|m| member_value(p, m)).collect();
    Ok(json!({
        "id": p.id,
        "name": p.name,
        "root": p.root,
        "root_exists": plan.root_exists,
        "changed": plan.changed.len(),
        "missing": plan.missing.len(),
        "sounds": sounds,
    }))
}

fn export(store: &Store, ctx: &Ctx, args: &ProjectExportArgs) -> Result<Value> {
    if !args.all {
        let id = store.find_project(args.project.as_deref().context("name a project or pass --all")?)?;
        return export_project(store, ctx, id, args);
    }
    let (mut projects, mut exported, mut failed) = (Vec::new(), 0, 0);
    for s in store.list_projects()? {
        match export_project(store, ctx, s.project.id, args) {
            Ok(v) => {
                exported += v["exported"].as_u64().unwrap_or(0);
                failed += v["failed"].as_u64().unwrap_or(0);
                projects.push(v);
            }
            Err(e) => {
                failed += 1;
                projects.push(json!({ "project_id": s.project.id, "project": s.project.name, "error": format!("{e:#}") }));
            }
        }
    }
    Ok(json!({ "projects": projects, "exported": exported, "failed": failed }))
}

fn export_project(store: &Store, ctx: &Ctx, project_id: i64, args: &ProjectExportArgs) -> Result<Value> {
    let plan = project_export::plan(store, project_id)?;
    let p = &plan.project;
    if !plan.root_exists && !args.create_root && !args.dry_run {
        bail!("project folder {} does not exist (a disk that is not connected?); pass --create-root to create it", p.root);
    }
    let brief = |m: &Membership| json!({ "id": m.sound_id, "name": m.sound_name, "path": m.rel_path });
    let skipped: Vec<Value> = if args.include_missing { Vec::new() } else { plan.missing.iter().map(brief).collect() };
    let items = plan.items(args.include_missing);
    if args.dry_run {
        let would: Vec<Value> = items
            .iter()
            .map(|m| {
                let mut v = brief(m);
                v["reason"] = json!(if m.changed() { "changed" } else { "missing" });
                v
            })
            .collect();
        return Ok(json!({ "project_id": p.id, "project": p.name, "root": p.root, "root_exists": plan.root_exists, "dry_run": true, "would_export": would, "skipped_missing": skipped, "exported": 0, "failed": 0 }));
    }
    let results = project_export::run(store, p, &items, args.create_root, ctx.now);
    let failed = results.iter().filter(|r| r.error.is_some()).count();
    let results: Vec<Value> = results
        .iter()
        .map(|r| {
            let mut v = json!({ "id": r.sound_id, "name": r.name, "path": r.rel_path, "abs_path": r.path, "ok": r.error.is_none() });
            if let Some(e) = &r.error {
                v["error"] = json!(e);
            }
            v
        })
        .collect();
    Ok(json!({
        "project_id": p.id,
        "project": p.name,
        "root": p.root,
        "exported": results.len() - failed,
        "failed": failed,
        "skipped_missing": skipped,
        "results": results,
    }))
}

fn list(store: &Store) -> Result<Value> {
    let mut out = Vec::new();
    for s in store.list_projects()? {
        let plan = project_export::plan(store, s.project.id)?;
        out.push(json!({
            "id": s.project.id,
            "name": s.project.name,
            "root": s.project.root,
            "sounds": s.sounds,
            "changed": plan.changed.len(),
            "missing": plan.missing.len(),
        }));
    }
    Ok(Value::Array(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Bits, CategoryArg, Command, FormatArg, ModeArg, ProjectCommand as P, ProjectExportArgs};
    use crate::commands::{self, tests::{ctx, new_jump, temp_dir}};

    fn run(s: &Store, c: P, cwd: &Path) -> Result<Value> {
        commands::run(s, Command::Project(c), &ctx(cwd))
    }

    fn add(project: &str, sound: &str, path: Option<&str>, options: MemberArgs) -> P {
        P::Add { project: project.into(), sound: sound.into(), path: path.map(Into::into), options }
    }

    fn game(s: &Store, dir: &Path, root: Option<&str>) {
        run(s, P::New { name: "Game".into(), root: root.map(Into::into) }, dir).unwrap();
    }

    #[test]
    fn new_defaults_the_root_to_the_current_folder() {
        let dir = temp_dir("pnew");
        let s = Store::open_in_memory().unwrap();
        let out = run(&s, P::New { name: "Game".into(), root: None }, &dir).unwrap();
        assert_eq!(out["root"], json!(dir.to_string_lossy()));
        let out = run(&s, P::New { name: "Other".into(), root: Some("assets".into()) }, &dir).unwrap();
        assert_eq!(out["root"], json!(dir.join("assets").to_string_lossy()));
        assert_eq!(out["root_exists"], json!(false));
    }

    #[test]
    fn add_uses_a_default_path_and_infers_the_format() {
        let dir = temp_dir("padd");
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&dir);
        new_jump(&s, &c, "Player jump");
        new_jump(&s, &c, "coin");
        game(&s, &dir, None);
        let out = run(&s, add("game", "Player jump", None, MemberArgs::default()), &dir).unwrap();
        assert_eq!(out["path"], json!("Player jump.wav"));
        assert_eq!(out["options"]["format"], json!({ "Wav": { "bits": null } }));
        let out = run(&s, add("Game", "coin", Some("sfx/coin.ogg"), MemberArgs::default()), &dir).unwrap();
        assert_eq!(out["options"]["format"]["Ogg"]["quality"], json!(6.0));
        assert_eq!(out["abs_path"], json!(dir.join("sfx/coin.ogg").to_string_lossy()));
        let e = run(&s, add("Game", "coin", Some("x.wav"), MemberArgs::default()), &dir).unwrap_err().to_string();
        assert!(e.contains("project set"), "{e}");
    }

    /// Review focus 3: a path is relative to the project folder, and an absolute one must point into it.
    #[test]
    fn add_accepts_an_absolute_path_inside_the_root_only() {
        let dir = temp_dir("pabs");
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&dir);
        new_jump(&s, &c, "jump");
        new_jump(&s, &c, "other");
        game(&s, &dir, Some("game"));
        let inside = dir.join("game/sfx/jump.wav");
        let out = run(&s, add("Game", "jump", Some(&inside.to_string_lossy()), MemberArgs::default()), &dir).unwrap();
        assert_eq!(out["path"], json!("sfx/jump.wav"));
        let out = run(&s, add("Game", "other", Some("game/sfx/other.wav"), MemberArgs::default()), &dir).unwrap();
        assert_eq!(out["abs_path"], json!(dir.join("game/game/sfx/other.wav").to_string_lossy()), "relative to the root, not the cwd");
        new_jump(&s, &c, "third");
        let outside = dir.join("elsewhere/o.wav");
        let e = run(&s, add("Game", "third", Some(&outside.to_string_lossy()), MemberArgs::default()), &dir).unwrap_err().to_string();
        assert!(e.contains("not inside"), "{e}");
    }

    #[test]
    fn format_flags_must_agree_with_the_path() {
        let dir = temp_dir("pfmt");
        let s = Store::open_in_memory().unwrap();
        new_jump(&s, &ctx(&dir), "jump");
        game(&s, &dir, None);
        let e = run(&s, add("Game", "jump", Some("a.wav"), MemberArgs { format: Some(FormatArg::Ogg), ..Default::default() }), &dir)
            .unwrap_err()
            .to_string();
        assert!(e.contains("--format"), "{e}");
        let e = run(&s, add("Game", "jump", Some("a.ogg"), MemberArgs { bits: Some(Bits(Some(16))), ..Default::default() }), &dir)
            .unwrap_err()
            .to_string();
        assert!(e.contains("--bits"), "{e}");
    }

    #[test]
    fn set_changes_settings_renames_the_extension_and_reports_the_old_path() {
        let dir = temp_dir("pset");
        let s = Store::open_in_memory().unwrap();
        new_jump(&s, &ctx(&dir), "jump");
        game(&s, &dir, None);
        run(&s, add("Game", "jump", Some("sfx/jump.wav"), MemberArgs::default()), &dir).unwrap();
        let set = |path: Option<&str>, options: MemberArgs| P::Set { project: "Game".into(), sound: "jump".into(), path: path.map(Into::into), options };
        let e = run(&s, set(None, MemberArgs::default()), &dir).unwrap_err().to_string();
        assert!(e.contains("nothing to change"), "{e}");
        let out = run(&s, set(None, MemberArgs { format: Some(FormatArg::Ogg), quality: Some(3.0), ..Default::default() }), &dir).unwrap();
        assert_eq!(out["path"], json!("sfx/jump.ogg"));
        assert_eq!(out["previous_path"], json!("sfx/jump.wav"));
        assert_eq!(out["options"]["format"]["Ogg"]["quality"], json!(3.0));
        let out = run(&s, set(None, MemberArgs { no_normalize: true, length: Some(0.5), ..Default::default() }), &dir).unwrap();
        assert_eq!(out["options"]["normalize"], json!(false));
        assert_eq!(out["options"]["duration"], json!(0.5));
        assert!(out.get("previous_path").is_none());
    }

    #[test]
    fn show_list_rename_set_root_remove_delete() {
        let dir = temp_dir("pmisc");
        let s = Store::open_in_memory().unwrap();
        new_jump(&s, &ctx(&dir), "jump");
        game(&s, &dir, None);
        run(&s, add("Game", "jump", None, MemberArgs::default()), &dir).unwrap();
        let list = run(&s, P::List, &dir).unwrap();
        assert_eq!((list[0]["sounds"].clone(), list[0]["changed"].clone()), (json!(1), json!(1)));
        let show = run(&s, P::Show { project: "game".into() }, &dir).unwrap();
        assert_eq!(show["sounds"][0]["name"], json!("jump"));
        run(&s, P::Rename { project: "Game".into(), name: "Game 2".into() }, &dir).unwrap();
        let out = run(&s, P::SetRoot { project: "Game 2".into(), dir: "moved".into() }, &dir).unwrap();
        assert_eq!(out["root"], json!(dir.join("moved").to_string_lossy()));
        run(&s, P::Remove { project: "Game 2".into(), sound: "jump".into() }, &dir).unwrap();
        let out = run(&s, P::Delete { project: "Game 2".into() }, &dir).unwrap();
        assert_eq!(out["removed_memberships"], json!(0));
        assert!(s.find_sound("jump").is_ok());
    }

    fn export_args(project: Option<&str>) -> ProjectExportArgs {
        ProjectExportArgs { project: project.map(Into::into), all: project.is_none(), include_missing: false, create_root: false, dry_run: false }
    }

    fn new_in_game(s: &Store, dir: &Path, name: &str, path: &str) -> i64 {
        let cmd = Command::New {
            name: name.into(),
            category: Some(CategoryArg::Jump),
            mode: ModeArg::Modern,
            seed: Some(1),
            patch: None,
            project: Some("Game".into()),
            path: Some(path.into()),
        };
        commands::run(s, cmd, &ctx(dir)).unwrap()["id"].as_i64().unwrap()
    }

    #[test]
    fn agent_cycle_new_add_export_set_export() {
        let dir = temp_dir("pcycle");
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&dir);
        game(&s, &dir, None);
        new_in_game(&s, &dir, "jump", "sfx/jump.wav");
        new_jump(&s, &c, "coin");
        run(&s, add("Game", "coin", Some("sfx/coin.wav"), MemberArgs::default()), &dir).unwrap();
        let out = run(&s, P::Export(export_args(Some("Game"))), &dir).unwrap();
        assert_eq!((out["exported"].clone(), out["failed"].clone()), (json!(2), json!(0)));
        assert!(dir.join("sfx/jump.wav").is_file());

        commands::run(&s, Command::Set { sound: "jump".into(), assignments: vec!["master_volume=0.5".into()] }, &c).unwrap();
        let out = run(&s, P::Export(export_args(Some("Game"))), &dir).unwrap();
        assert_eq!(out["exported"], json!(1));
        assert_eq!(out["results"][0]["name"], json!("jump"));

        let set = P::Set { project: "Game".into(), sound: "coin".into(), path: Some("ui/coin.wav".into()), options: MemberArgs::default() };
        assert_eq!(run(&s, set, &dir).unwrap()["previous_path"], json!("sfx/coin.wav"));
        let out = run(&s, P::Export(export_args(Some("Game"))), &dir).unwrap();
        assert_eq!(out["results"][0]["path"], json!("ui/coin.wav"));
        assert!(dir.join("sfx/coin.wav").is_file(), "the old file stays");
    }

    #[test]
    fn missing_files_need_include_missing_and_dry_run_changes_nothing() {
        let dir = temp_dir("pmissing");
        let s = Store::open_in_memory().unwrap();
        game(&s, &dir, None);
        let jump = new_in_game(&s, &dir, "jump", "sfx/jump.wav");
        new_in_game(&s, &dir, "coin", "sfx/coin.wav");
        run(&s, P::Export(export_args(Some("Game"))), &dir).unwrap();
        std::fs::remove_file(dir.join("sfx/coin.wav")).unwrap();

        let out = run(&s, P::Export(export_args(Some("Game"))), &dir).unwrap();
        assert_eq!(out["exported"], json!(0));
        assert_eq!(out["skipped_missing"][0]["name"], json!("coin"));

        let mut patch = s.load_draft(jump).unwrap();
        patch.master_volume = 0.4;
        s.save_draft(jump, &patch, 200).unwrap();
        let versions = s.list_versions(jump).unwrap().len();
        let dry = ProjectExportArgs { dry_run: true, include_missing: true, ..export_args(Some("Game")) };
        let out = run(&s, P::Export(dry), &dir).unwrap();
        assert_eq!(out["would_export"].as_array().unwrap().len(), 2);
        assert_eq!(out["root_exists"], json!(true));
        assert!(!dir.join("sfx/coin.wav").exists());
        assert_eq!(s.list_versions(jump).unwrap().len(), versions, "a dry run commits nothing");

        let out = run(&s, P::Export(ProjectExportArgs { include_missing: true, ..export_args(Some("Game")) }), &dir).unwrap();
        assert_eq!(out["exported"], json!(2));
    }

    /// Review focus 1: a folder that is not there is never created silently.
    #[test]
    fn a_missing_root_needs_create_root_and_all_reports_per_project() {
        let dir = temp_dir("proot");
        let s = Store::open_in_memory().unwrap();
        game(&s, &dir, Some("game"));
        new_in_game(&s, &dir, "jump", "jump.wav");
        let e = run(&s, P::Export(export_args(Some("Game"))), &dir).unwrap_err().to_string();
        assert!(e.contains("--create-root"), "{e}");
        assert!(!dir.join("game").exists());
        let dry = ProjectExportArgs { dry_run: true, ..export_args(Some("Game")) };
        assert_eq!(run(&s, P::Export(dry), &dir).unwrap()["root_exists"], json!(false));

        run(&s, P::New { name: "Other".into(), root: Some("other".into()) }, &dir).unwrap();
        new_jump(&s, &ctx(&dir), "coin");
        run(&s, add("Other", "coin", None, MemberArgs::default()), &dir).unwrap();
        let out = run(&s, P::Export(export_args(None)), &dir).unwrap();
        assert_eq!(out["failed"], json!(2));
        let out = run(&s, P::Export(ProjectExportArgs { create_root: true, ..export_args(None) }), &dir).unwrap();
        assert_eq!((out["exported"].clone(), out["failed"].clone()), (json!(2), json!(0)));
    }

    #[test]
    fn list_filters_by_project_and_shows_memberships() {
        let dir = temp_dir("plist");
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&dir);
        game(&s, &dir, None);
        new_in_game(&s, &dir, "jump", "sfx/jump.wav");
        new_jump(&s, &c, "coin");
        let list = |project: Option<&str>, unassigned: bool| {
            let out = commands::run(&s, Command::List { search: String::new(), project: project.map(Into::into), unassigned }, &c).unwrap();
            out.as_array().unwrap().clone()
        };
        let all = list(None, false);
        let jump = all.iter().find(|v| v["name"] == json!("jump")).unwrap();
        assert_eq!(jump["projects"][0]["path"], json!("sfx/jump.wav"));
        let names = |v: Vec<Value>| v.into_iter().map(|x| x["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(names(list(Some("Game"), false)), ["jump"]);
        assert_eq!(names(list(None, true)), ["coin"]);
    }
}
