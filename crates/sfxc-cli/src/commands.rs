use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sfxc_core::analyze::analyze;
use sfxc_core::export::{export_to_path, wav_bits, ExportFormat, ExportOptions, TRIM_THRESHOLD};
use sfxc_core::generators::{generate, mutate};
use sfxc_core::patch::{Effect, EffectKind, Mode, SoundPatch};
use sfxc_core::patch_edit::{apply_assignments, unknown_keys};
use sfxc_core::render::{render, trim_tail};
use sfxc_store::{ExportLink, Store};

use crate::cli::{Command, ExportArgs, FormatArg, FxCommand};

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
        Command::Mutate { sound, seed } => {
            let id = store.find_sound(&sound)?;
            let patch = mutate(&store.load_draft(id)?, seed.unwrap_or(ctx.seed));
            save(store, id, &patch, "mutate", ctx.now)?;
            show(store, id)
        }
        Command::Set { sound, assignments } => {
            let id = store.find_sound(&sound)?;
            let (patch, finals) = apply_assignments(&store.load_draft(id)?, &assignments)?;
            let version_id = save(store, id, &patch, "set", ctx.now)?;
            let set: serde_json::Map<String, Value> = finals.into_iter().collect();
            Ok(json!({ "id": id, "version_id": version_id, "set": set }))
        }
        Command::Put { sound, patch } => {
            let id = store.find_sound(&sound)?;
            let (patch, warnings) = parse_patch(&read_source(&patch, &ctx.cwd)?)?;
            save(store, id, &patch, "put", ctx.now)?;
            let mut out = show(store, id)?;
            attach_warnings(&mut out, warnings);
            Ok(out)
        }
        Command::Fx(FxCommand::Add { sound, kind, layer, at }) => fx_add(store, ctx, &sound, &kind, layer, at),
        Command::Fx(FxCommand::Remove { sound, id: fx_id }) => {
            let id = store.find_sound(&sound)?;
            let mut patch = store.load_draft(id)?;
            let ids = effect_ids(&patch);
            if !ids.contains(&fx_id) {
                let ids: Vec<String> = ids.iter().map(u64::to_string).collect();
                bail!("no effect with id {fx_id}; ids: {}", if ids.is_empty() { "none".to_string() } else { ids.join(", ") });
            }
            patch.master_effects.retain(|e| e.id != fx_id);
            for l in &mut patch.layers {
                l.effects.retain(|e| e.id != fx_id);
            }
            save(store, id, &patch, "fx remove", ctx.now)?;
            Ok(json!({ "id": id, "removed": fx_id }))
        }
        Command::Rename { sound, name } => {
            let id = store.find_sound(&sound)?;
            let name = name.trim();
            ensure_name_free(store, name, Some(id))?;
            store.rename_sound(id, name, ctx.now)?;
            Ok(json!({ "id": id, "name": name }))
        }
        Command::Tag { sound, tags } => {
            let id = store.find_sound(&sound)?;
            store.set_tags(id, tags.trim(), ctx.now)?;
            Ok(json!({ "id": id, "tags": tags.trim() }))
        }
        Command::Versions { sound } => versions_json(store, store.find_sound(&sound)?),
        Command::Restore { sound, version } => {
            let id = store.find_sound(&sound)?;
            if store.version_owner(version)? != Some(id) {
                bail!("version {version} is not a version of sound {id}; `sfxc-cli versions {id}` lists them");
            }
            store.restore_version(id, version, ctx.now)?;
            show(store, id)
        }
        Command::Export(args) => export(store, ctx, &args),
        Command::Analyze { sound } => {
            let id = store.find_sound(&sound)?;
            let mut samples = render(&store.load_draft(id)?, ANALYZE_RATE);
            trim_tail(&mut samples, TRIM_THRESHOLD);
            Ok(json!({ "id": id, "analysis": analyze(&samples, ANALYZE_RATE) }))
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

/// Saves `patch` as the draft and snapshots it. The note is set only when something changed, so a note a
/// person wrote on the latest version is never replaced.
pub(crate) fn save(store: &Store, id: i64, patch: &SoundPatch, command: &str, now: i64) -> Result<i64> {
    store.save_draft(id, patch, now)?;
    let note = if store.draft_differs_from_latest(id)? { format!("cli: {command}") } else { String::new() };
    store.commit_version(id, &note, now)
}

fn effect_ids(patch: &SoundPatch) -> Vec<u64> {
    patch.master_effects.iter().chain(patch.layers.iter().flat_map(|l| l.effects.iter())).map(|e| e.id).collect()
}

fn fx_add(store: &Store, ctx: &Ctx, sound: &str, name: &str, layer: Option<usize>, at: Option<usize>) -> Result<Value> {
    let id = store.find_sound(sound)?;
    let mut patch = store.load_draft(id)?;
    let defaults = EffectKind::all_defaults();
    let kind = defaults.iter().find(|k| k.name().eq_ignore_ascii_case(name.trim())).copied().with_context(|| {
        let names: Vec<&str> = defaults.iter().map(|k| k.name()).collect();
        format!("unknown effect `{name}`; one of: {}", names.join(", "))
    })?;
    let fx_id = patch.next_effect_id();
    let layers = patch.layers.len();
    let (chain, prefix) = match layer {
        None => (&mut patch.master_effects, "master_effects".to_string()),
        Some(l) => {
            let layer = patch.layers.get_mut(l).with_context(|| format!("layer {l} does not exist (the sound has {layers})"))?;
            (&mut layer.effects, format!("layers.{l}.effects"))
        }
    };
    let at = at.unwrap_or(chain.len()).min(chain.len());
    chain.insert(at, Effect { id: fx_id, enabled: true, kind });
    let version_id = save(store, id, &patch, "fx add", ctx.now)?;
    Ok(json!({ "id": id, "version_id": version_id, "effect_id": fx_id, "path": format!("{prefix}.{at}") }))
}

const ANALYZE_RATE: u32 = 44_100;
/// Same default as the app's export dialog.
const DEFAULT_OGG_QUALITY: f32 = 6.0;
/// Rates the renderer and encoders handle without panicking; the app only offers 44.1 kHz.
const RATE_RANGE: std::ops::RangeInclusive<u32> = 8_000..=192_000;

fn export(store: &Store, ctx: &Ctx, args: &ExportArgs) -> Result<Value> {
    if args.linked {
        return export_linked(store, ctx);
    }
    let id = store.find_sound(args.sound.as_deref().context("name a sound or pass --linked")?)?;
    let link = match &args.to {
        Some(to) => {
            let path = std::path::absolute(ctx.cwd.join(to)).with_context(|| format!("bad path {}", to.display()))?;
            ExportLink { path: path.to_string_lossy().into_owned(), options: options_from(args, &path, store.load_draft(id)?.mode)? }
        }
        None => store
            .export_link(id)?
            .with_context(|| format!("sound {id} has no export link yet; run `sfxc-cli export {id} --to FILE` once"))?,
    };
    let version_id = export_one(store, id, &link, ctx.now)?;
    store.set_export_link(id, &link)?;
    Ok(json!({ "id": id, "version_id": version_id, "path": link.path, "export_link": link }))
}

fn options_from(args: &ExportArgs, path: &Path, mode: Mode) -> Result<ExportOptions> {
    let format = match args.format {
        Some(f) => f,
        None => match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
            Some("wav") => FormatArg::Wav,
            Some("ogg") => FormatArg::Ogg,
            _ => bail!("cannot tell the format from `{}`; add --format wav or --format ogg", path.display()),
        },
    };
    if let Some(rate) = args.rate.filter(|r| !RATE_RANGE.contains(r)) {
        bail!("--rate {rate} is out of range; use {} to {} Hz", RATE_RANGE.start(), RATE_RANGE.end());
    }
    if let Some(length) = args.length.filter(|l| l.is_nan() || *l <= 0.0) {
        bail!("--length {length} must be above 0 seconds");
    }
    let defaults = ExportOptions::default();
    Ok(ExportOptions {
        format: match format {
            FormatArg::Wav => ExportFormat::Wav { bits: args.bits.unwrap_or(wav_bits(mode)) },
            FormatArg::Ogg => ExportFormat::Ogg { quality: args.quality.unwrap_or(DEFAULT_OGG_QUALITY) },
        },
        sample_rate: args.rate.unwrap_or(defaults.sample_rate),
        normalize: !args.no_normalize,
        trim: !args.no_trim,
        duration: args.length,
    })
}

/// Writes the draft to the link's file, then records the export on a version.
fn export_one(store: &Store, id: i64, link: &ExportLink, now: i64) -> Result<i64> {
    let patch = store.load_draft(id)?;
    let path = Path::new(&link.path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    export_to_path(&patch, &link.options, path)?;
    let version_id = save(store, id, &patch, "export", now)?;
    store.mark_exported(version_id, now)?;
    Ok(version_id)
}

fn export_linked(store: &Store, ctx: &Ctx) -> Result<Value> {
    let mut results = Vec::new();
    let mut failed = 0;
    for id in store.linked_sounds()? {
        let name = store.sound_meta(id).map(|m| m.0).unwrap_or_default();
        let r = store.export_link(id).and_then(|link| {
            let link = link.context("the export link disappeared")?;
            export_one(store, id, &link, ctx.now)?;
            Ok(link.path)
        });
        match r {
            Ok(path) => results.push(json!({ "id": id, "name": name, "ok": true, "path": path })),
            Err(e) => {
                failed += 1;
                results.push(json!({ "id": id, "name": name, "ok": false, "error": format!("{e:#}") }));
            }
        }
    }
    Ok(json!({ "exported": results.len() - failed, "failed": failed, "results": results }))
}

pub(crate) fn attach_warnings(out: &mut Value, warnings: Vec<String>) {
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{CategoryArg, ExportArgs, FxCommand, ModeArg};

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

    fn set_cmd(sound: &str, a: &[&str]) -> Command {
        Command::Set { sound: sound.into(), assignments: a.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn every_edit_is_a_version_noted_with_its_command() {
        let dir = temp_dir("edits");
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&dir);
        let id = new_jump(&s, &c, "jump");
        run(&s, Command::Mutate { sound: "jump".into(), seed: Some(3) }, &c).unwrap();
        run(&s, set_cmd("jump", &["master_volume=0.5"]), &c).unwrap();
        std::fs::write(dir.join("p.json"), SoundPatch::default().to_json()).unwrap();
        run(&s, Command::Put { sound: "jump".into(), patch: "p.json".into() }, &c).unwrap();
        run(&s, Command::Fx(FxCommand::Add { sound: "jump".into(), kind: "reverb".into(), layer: None, at: None }), &c).unwrap();
        assert_eq!(notes(&s, id), vec!["cli: fx add", "cli: put", "cli: set", "cli: mutate", "cli: new"]);
    }

    #[test]
    fn a_no_op_set_keeps_a_persons_note() {
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&std::env::temp_dir());
        let id = new_jump(&s, &c, "jump");
        run(&s, set_cmd("jump", &["master_volume=0.5"]), &c).unwrap();
        s.commit_version(id, "final", 200).unwrap();
        run(&s, set_cmd("jump", &["master_volume=0.5"]), &c).unwrap();
        assert_eq!(notes(&s, id), vec!["final", "cli: new"]);
    }

    #[test]
    fn set_reports_the_value_that_was_kept() {
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&std::env::temp_dir());
        new_jump(&s, &c, "jump");
        let out = run(&s, set_cmd("jump", &["layers.0.pitch.base_freq=99999"]), &c).unwrap();
        assert_eq!(out["set"]["layers.0.pitch.base_freq"], json!(5000.0));
    }

    #[test]
    fn restore_adds_exactly_one_version_and_refuses_foreign_ones() {
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&std::env::temp_dir());
        let id = new_jump(&s, &c, "jump");
        let other = new_jump(&s, &c, "other");
        let first = s.current_version_id(id).unwrap().unwrap();
        run(&s, set_cmd("jump", &["master_volume=0.3"]), &c).unwrap();
        let before = s.list_versions(id).unwrap().len();
        run(&s, Command::Restore { sound: "jump".into(), version: first }, &c).unwrap();
        assert_eq!(s.list_versions(id).unwrap().len(), before, "the edited draft was already a version");
        assert_eq!(s.current_version_id(id).unwrap(), Some(first));
        let foreign = s.current_version_id(other).unwrap().unwrap();
        let e = run(&s, Command::Restore { sound: "jump".into(), version: foreign }, &c).unwrap_err().to_string();
        assert!(e.contains("not a version of"), "{e}");
    }

    #[test]
    fn fx_add_gives_fresh_ids_and_remove_drops_one() {
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&std::env::temp_dir());
        let id = new_jump(&s, &c, "jump");
        let add = |kind: &str, layer| Command::Fx(FxCommand::Add { sound: "jump".into(), kind: kind.into(), layer, at: None });
        let a = run(&s, add("Delay", None), &c).unwrap();
        let b = run(&s, add("bitcrusher", Some(0)), &c).unwrap();
        assert_ne!(a["effect_id"], b["effect_id"]);
        assert_eq!(a["path"], json!("master_effects.0"));
        assert_eq!(b["path"], json!("layers.0.effects.0"));
        let fx_id = a["effect_id"].as_u64().unwrap();
        run(&s, Command::Fx(FxCommand::Remove { sound: "jump".into(), id: fx_id }), &c).unwrap();
        let p = s.load_draft(id).unwrap();
        assert!(p.master_effects.is_empty() && p.layers[0].effects.len() == 1);
        assert!(run(&s, Command::Fx(FxCommand::Remove { sound: "jump".into(), id: fx_id }), &c).is_err());
        let e = run(&s, add("Chorus", None), &c).unwrap_err().to_string();
        assert!(e.contains("Reverb"), "{e}");
        assert!(run(&s, add("Delay", Some(3)), &c).is_err());
    }

    #[test]
    fn rename_keeps_names_unique() {
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&std::env::temp_dir());
        new_jump(&s, &c, "a");
        new_jump(&s, &c, "b");
        assert!(run(&s, Command::Rename { sound: "b".into(), name: "a".into() }, &c).is_err());
        run(&s, Command::Rename { sound: "b".into(), name: "b".into() }, &c).unwrap();
        run(&s, Command::Rename { sound: "b".into(), name: "c".into() }, &c).unwrap();
        assert!(s.find_sound("c").is_ok());
    }

    fn export_args(sound: Option<&str>, to: Option<&str>) -> ExportArgs {
        ExportArgs {
            sound: sound.map(Into::into),
            linked: sound.is_none(),
            to: to.map(PathBuf::from),
            format: None,
            bits: None,
            quality: None,
            rate: None,
            length: None,
            no_normalize: false,
            no_trim: false,
        }
    }

    #[test]
    fn export_links_an_absolute_path_and_reexports_from_anywhere() {
        let project = temp_dir("project");
        let elsewhere = temp_dir("elsewhere");
        let s = Store::open_in_memory().unwrap();
        let id = new_jump(&s, &ctx(&project), "jump");
        let out = run(&s, Command::Export(export_args(Some("jump"), Some("sfx/jump.wav"))), &ctx(&project)).unwrap();
        let file = project.join("sfx/jump.wav");
        assert!(file.metadata().unwrap().len() > 100);
        let link = s.export_link(id).unwrap().unwrap();
        assert!(Path::new(&link.path).is_absolute());
        assert_eq!(out["path"], json!(link.path));
        assert_eq!(link.options.format, ExportFormat::Wav { bits: 24 });

        std::fs::remove_file(&file).unwrap();
        run(&s, Command::Export(export_args(Some("jump"), None)), &ctx(&elsewhere)).unwrap();
        assert!(file.exists(), "re-export writes the linked file, not one under the current directory");
        assert!(s.list_versions(id).unwrap()[0].exported_at.is_some());
    }

    #[test]
    fn export_keeps_a_persons_note() {
        let dir = temp_dir("note");
        let s = Store::open_in_memory().unwrap();
        let id = new_jump(&s, &ctx(&dir), "jump");
        s.commit_version(id, "final", 150).unwrap();
        run(&s, Command::Export(export_args(Some("jump"), Some("a.ogg"))), &ctx(&dir)).unwrap();
        assert_eq!(notes(&s, id), vec!["final"]);
        assert!(dir.join("a.ogg").exists());
    }

    #[test]
    fn export_without_a_link_or_with_an_unknown_extension_fails() {
        let dir = temp_dir("nolink");
        let s = Store::open_in_memory().unwrap();
        new_jump(&s, &ctx(&dir), "jump");
        let e = run(&s, Command::Export(export_args(Some("jump"), None)), &ctx(&dir)).unwrap_err().to_string();
        assert!(e.contains("--to"), "{e}");
        let e = run(&s, Command::Export(export_args(Some("jump"), Some("a.mp3"))), &ctx(&dir)).unwrap_err().to_string();
        assert!(e.contains("--format"), "{e}");
    }

    #[test]
    fn export_refuses_a_sample_rate_or_length_that_would_panic() {
        let dir = temp_dir("badrate");
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&dir);
        let id = new_jump(&s, &c, "jump");
        for rate in [0, 1, 7_999, 192_001] {
            let mut args = export_args(Some("jump"), Some("a.wav"));
            args.rate = Some(rate);
            let e = run(&s, Command::Export(args), &c).unwrap_err().to_string();
            assert!(e.contains("--rate"), "{rate}: {e}");
        }
        let mut args = export_args(Some("jump"), Some("a.wav"));
        args.length = Some(0.0);
        let e = run(&s, Command::Export(args), &c).unwrap_err().to_string();
        assert!(e.contains("--length"), "{e}");
        assert_eq!(s.export_link(id).unwrap(), None, "a refused export leaves no link");
    }

    #[test]
    fn export_linked_reports_each_sound_and_counts_failures() {
        let dir = temp_dir("linked");
        let s = Store::open_in_memory().unwrap();
        let c = ctx(&dir);
        new_jump(&s, &c, "good");
        let bad = new_jump(&s, &c, "bad");
        run(&s, Command::Export(export_args(Some("good"), Some("good.wav"))), &c).unwrap();
        std::fs::write(dir.join("not-a-dir"), b"x").unwrap();
        let broken = ExportLink { path: dir.join("not-a-dir/bad.wav").to_string_lossy().into_owned(), options: ExportOptions::default() };
        s.set_export_link(bad, &broken).unwrap();
        let out = run(&s, Command::Export(export_args(None, None)), &c).unwrap();
        assert_eq!((out["exported"].clone(), out["failed"].clone()), (json!(1), json!(1)));
        let failed: Vec<&Value> = out["results"].as_array().unwrap().iter().filter(|r| r["ok"] == json!(false)).collect();
        assert_eq!(failed[0]["name"], json!("bad"));
    }

    #[test]
    fn analyze_describes_the_rendered_sound() {
        let s = Store::open_in_memory().unwrap();
        new_jump(&s, &ctx(&std::env::temp_dir()), "jump");
        let out = run(&s, Command::Analyze { sound: "jump".into() }, &ctx(&std::env::temp_dir())).unwrap();
        assert!(out["analysis"]["duration_s"].as_f64().unwrap() > 0.0);
        assert_eq!(out["analysis"]["envelope"].as_array().unwrap().len(), 10);
    }
}
