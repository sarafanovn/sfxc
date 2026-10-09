use clap::ValueEnum;
use serde_json::{json, Value};
use sfxc_core::eq::{FREQS, RANGE_DB};
use sfxc_core::generators::{generate, Category};
use sfxc_core::patch::ranges::*;
use sfxc_core::patch::{EffectKind, FmAlgorithm, Mode, NoiseKind, Source, MAX_ARP_STEPS, MAX_LAYERS, MAX_SECONDS};

use crate::cli::CategoryArg;

pub fn schema() -> Value {
    let categories: Vec<String> = CategoryArg::value_variants()
        .iter()
        .filter_map(|c| c.to_possible_value().map(|v| v.get_name().to_string()))
        .collect();
    let sources: Vec<Source> = Source::KIND_NAMES.iter().map(|k| Source::default_of_kind(k)).collect();
    json!({
        "notes": [
            "Paths for `set` use dots and list indices: layers.0.pitch.base_freq.",
            "Replace `layers.N.source` and an effect's `kind` as whole JSON objects; take defaults from `sources` and `effects`.",
            "Add effects with `fx add`, then tune them with `set master_effects.I.kind.FIELD=VALUE`.",
            "Out-of-range values are clamped; `set` prints the value that was kept.",
            "Before creating, run `project list` and `list --project P --search WORD`; reuse and edit existing sounds instead of making near-duplicates.",
            "Create with `new --project P --path sfx/folder/name.ogg` (path is relative to the project folder; the output's `abs_path` shows where the file goes), so no sound stays in no project (`list --unassigned`).",
            "Tag every sound: `tag S \"player movement short\"` replaces all tags, so read the current ones with `show S` first. Reuse words already in `list`; add `wip` while iterating and replace it with `final` when done.",
            "Combine all edits in one `set` (one version per call). Run `project export P` once at the end; it writes only what changed (`--dry-run` previews). A person may have tuned a sound in the app: prefer `set` over `put`, and `restore` undoes anything.",
        ],
        "categories": categories,
        "modes": { "cli": ["modern", "8bit", "16bit"], "patch": Mode::ALL },
        "sources": sources,
        "noise_kinds": NoiseKind::ALL,
        "fm_algorithms": FmAlgorithm::ALL,
        "effects": EffectKind::all_defaults(),
        "limits": {
            "max_layers": MAX_LAYERS,
            "max_arp_steps": MAX_ARP_STEPS,
            "arp_step_semitones": [-24, 24],
            "max_seconds": MAX_SECONDS,
            "eq_bands_hz": FREQS,
            "eq_gain_db": [-RANGE_DB, RANGE_DB],
            "phaser_stages": [2, 8],
        },
        "ranges": {
            "master_volume": UNIT,
            "layers.N.gain": GAIN,
            "layers.N.pan": PAN,
            "layers.N.source.duty (Pulse)": DUTY,
            "layers.N.source.feedback (FM)": UNIT,
            "layers.N.source.ops.N.ratio": FM_RATIO,
            "layers.N.source.ops.N.detune (cents)": FM_DETUNE,
            "layers.N.source.ops.N.level / sustain": UNIT,
            "layers.N.source.ops.N.attack / decay (s)": FM_TIME,
            "layers.N.pitch.base_freq (Hz)": BASE_FREQ,
            "layers.N.pitch.slide (octaves/s, negative falls)": SLIDE,
            "layers.N.pitch.delta_slide (octaves/s²)": DELTA_SLIDE,
            "layers.N.pitch.vibrato_depth (semitones)": VIBRATO_DEPTH,
            "layers.N.pitch.vibrato_rate (Hz)": VIBRATO_RATE,
            "layers.N.pitch.arp_speed (s per step)": ARP_SPEED,
            "layers.N.env.attack / decay / sustain_time / release (s)": ENV_TIME,
            "layers.N.env.sustain_level / punch": UNIT,
            "Bitcrusher.bits": BITS,
            "Bitcrusher.downsample": DOWNSAMPLE,
            "Distortion.drive": DRIVE,
            "Distortion.tone (Hz)": TONE,
            "Phaser.rate / Flanger.rate (Hz)": LFO_RATE,
            "Phaser.feedback / Delay.feedback": FEEDBACK,
            "Flanger.depth_ms": FLANGER_DEPTH_MS,
            "Flanger.delay_ms": FLANGER_DELAY_MS,
            "Flanger.feedback": FLANGER_FEEDBACK,
            "Delay.time (s)": DELAY_TIME,
            "Reverb.predelay (s)": PREDELAY,
            "Compressor.threshold_db": THRESHOLD_DB,
            "Compressor.ratio": RATIO,
            "Compressor.attack (s)": COMP_ATTACK,
            "Compressor.release (s)": COMP_RELEASE,
            "Compressor.makeup_db": MAKEUP_DB,
            "every mix, depth, damping, size, decay": UNIT,
        },
        "example": generate(Category::Jump, Mode::Modern, 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sfxc_core::patch::SoundPatch;

    #[test]
    fn schema_lists_every_category_and_a_valid_example() {
        let s = schema();
        assert_eq!(s["categories"].as_array().unwrap().len(), Category::ALL.len());
        assert_eq!(s["effects"].as_array().unwrap().len(), EffectKind::all_defaults().len());
        SoundPatch::from_json(&s["example"].to_string()).unwrap();
    }
}
