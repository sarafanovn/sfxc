//! Path edits on a patch's JSON, for tools that change a few values without writing a whole patch.

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::patch::SoundPatch;

/// Sets `path` (dot-separated; numeric segments index lists) to `raw`. `raw` is parsed as JSON and falls back
/// to a plain string, so `mode=Bit8` needs no quotes. Only existing fields can be set.
pub fn set_path(json: &mut Value, path: &str, raw: &str) -> Result<()> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.iter().any(|s| s.is_empty()) {
        bail!("bad path `{path}`");
    }
    let (last, parents) = segments.split_last().expect("split yields at least one segment");
    if *last == "type" {
        let parent = parents.join(".");
        bail!("`{path}` picks a variant; replace the whole object instead: {parent}='{{\"type\":\"{raw}\", …}}' (defaults for every variant: `sfxc-cli schema`)");
    }
    let mut node = json;
    let mut here = String::new();
    for seg in &segments {
        node = step(node, seg, &here)?;
        if !here.is_empty() {
            here.push('.');
        }
        here.push_str(seg);
    }
    *node = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
    Ok(())
}

fn step<'a>(node: &'a mut Value, seg: &str, at: &str) -> Result<&'a mut Value> {
    let at = if at.is_empty() { "the patch".to_string() } else { format!("`{at}`") };
    match node {
        Value::Object(map) => {
            if !map.contains_key(seg) {
                let keys: Vec<&str> = map.keys().map(String::as_str).collect();
                bail!("no field `{seg}` in {at}; fields: {}", keys.join(", "));
            }
            Ok(map.get_mut(seg).expect("checked above"))
        }
        Value::Array(items) => {
            let len = items.len();
            let i: usize = seg.parse().with_context(|| format!("{at} is a list; use an index below {len}"))?;
            items.get_mut(i).with_context(|| format!("index {i} is out of range in {at} (length {len})"))
        }
        _ => bail!("{at} is a value, not an object or a list"),
    }
}

pub fn get_path<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(json, |node, seg| match node {
        Value::Object(map) => map.get(seg),
        Value::Array(items) => items.get(seg.parse::<usize>().ok()?),
        _ => None,
    })
}

/// Applies `PATH=VALUE` assignments in order. Returns the clamped patch and the final value at each path.
pub fn apply_assignments(patch: &SoundPatch, assignments: &[String]) -> Result<(SoundPatch, Vec<(String, Value)>)> {
    let mut json = serde_json::to_value(patch)?;
    let mut paths = Vec::new();
    for a in assignments {
        let (path, raw) = a.split_once('=').with_context(|| format!("`{a}` is not PATH=VALUE"))?;
        let path = path.trim();
        set_path(&mut json, path, raw.trim())?;
        paths.push(path.to_string());
    }
    let edited = SoundPatch::from_json(&json.to_string()).context("the edited patch is invalid")?;
    let back = serde_json::to_value(&edited)?;
    let finals = paths
        .into_iter()
        .map(|p| {
            let v = get_path(&back, &p).cloned().unwrap_or(Value::Null);
            (p, v)
        })
        .collect();
    Ok((edited, finals))
}

/// Dotted paths present in `input` that parsing into `parsed` dropped: typos and fields of another variant.
pub fn unknown_keys(input: &Value, parsed: &SoundPatch) -> Vec<String> {
    let back = serde_json::to_value(parsed).expect("SoundPatch always serializes");
    let mut out = Vec::new();
    walk(input, &back, "", &mut out);
    out
}

fn walk(input: &Value, back: &Value, at: &str, out: &mut Vec<String>) {
    let join = |k: &str| if at.is_empty() { k.to_string() } else { format!("{at}.{k}") };
    match (input, back) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                match b.get(k) {
                    Some(w) => walk(v, w, &join(k), out),
                    None => out.push(join(k)),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for (i, (v, w)) in a.iter().zip(b).enumerate() {
                walk(v, w, &join(&i.to_string()), out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{Mode, NoiseKind, Source};
    use serde_json::json;

    fn set(a: &[&str]) -> Result<(SoundPatch, Vec<(String, Value)>)> {
        apply_assignments(&SoundPatch::default(), &a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn sets_a_nested_field() {
        let (p, finals) = set(&["layers.0.pitch.base_freq=880"]).unwrap();
        assert_eq!(p.layers[0].pitch.base_freq, 880.0);
        assert_eq!(finals, vec![("layers.0.pitch.base_freq".to_string(), json!(880.0))]);
    }

    #[test]
    fn a_bare_word_is_a_string() {
        assert_eq!(set(&["mode=Bit8"]).unwrap().0.mode, Mode::Bit8);
    }

    #[test]
    fn out_of_range_values_are_clamped_and_reported() {
        let (p, finals) = set(&["layers.0.pitch.base_freq=99999"]).unwrap();
        assert_eq!(p.layers[0].pitch.base_freq, 5000.0);
        assert_eq!(finals[0].1, json!(5000.0));
    }

    #[test]
    fn an_unknown_field_lists_the_real_ones() {
        let e = set(&["layers.0.pitch.freq=1"]).unwrap_err().to_string();
        assert!(e.contains("no field `freq`") && e.contains("base_freq"), "{e}");
    }

    #[test]
    fn an_index_out_of_range_says_the_length() {
        let e = set(&["layers.3.gain=1"]).unwrap_err().to_string();
        assert!(e.contains("out of range") && e.contains("length 1"), "{e}");
    }

    #[test]
    fn type_alone_is_refused_with_the_whole_object_form() {
        let e = set(&["layers.0.source.type=Noise"]).unwrap_err().to_string();
        assert!(e.contains("layers.0.source='{"), "{e}");
    }

    #[test]
    fn a_whole_source_object_switches_the_variant() {
        let (p, _) = set(&[r#"layers.0.source={"type":"Noise","kind":"White"}"#]).unwrap();
        assert_eq!(p.layers[0].source, Source::Noise { kind: NoiseKind::White });
    }

    #[test]
    fn missing_equals_is_an_error() {
        assert!(set(&["master_volume"]).unwrap_err().to_string().contains("PATH=VALUE"));
    }

    #[test]
    fn unknown_keys_finds_typos_and_fields_of_another_variant() {
        let input = json!({"layers": [{"source": {"type": "Pulse", "duty": 0.5, "kind": "White"}, "pitch": {"basefreq": 880}}]});
        let parsed = SoundPatch::from_json(&input.to_string()).unwrap();
        let mut keys = unknown_keys(&input, &parsed);
        keys.sort();
        assert_eq!(keys, vec!["layers.0.pitch.basefreq", "layers.0.source.kind"]);
    }

    #[test]
    fn a_full_patch_has_no_unknown_keys() {
        let p = SoundPatch::default();
        assert!(unknown_keys(&serde_json::to_value(&p).unwrap(), &p).is_empty());
    }
}
