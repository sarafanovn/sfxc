//! The complete, serializable description of one sound. Audio is never stored;
//! it is rendered from this on demand.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SECONDS: f32 = 10.0;
pub const MAX_LAYERS: usize = 4;
pub const MAX_ARP_STEPS: usize = 8;

pub type Range = (f32, f32);

/// Valid ranges for every continuous parameter. Shared by clamping, generators and UI sliders.
pub mod ranges {
    use super::Range;
    pub const UNIT: Range = (0.0, 1.0);
    pub const BASE_FREQ: Range = (20.0, 5000.0);
    pub const SLIDE: Range = (-8.0, 8.0);
    pub const DELTA_SLIDE: Range = (-16.0, 16.0);
    pub const VIBRATO_DEPTH: Range = (0.0, 2.0);
    pub const VIBRATO_RATE: Range = (0.0, 30.0);
    pub const ARP_SPEED: Range = (0.01, 1.0);
    pub const ENV_TIME: Range = (0.0, 5.0);
    pub const DUTY: Range = (0.01, 0.99);
    pub const CUTOFF: Range = (20.0, 20000.0);
    pub const SWEEP: Range = (-8.0, 8.0);
    pub const GAIN: Range = (0.0, 2.0);
    pub const PAN: Range = (-1.0, 1.0);
    pub const FM_RATIO: Range = (0.25, 16.0);
    pub const FM_DETUNE: Range = (-100.0, 100.0);
    pub const FM_TIME: Range = (0.0, 2.0);
    pub const BITS: Range = (1.0, 16.0);
    pub const DOWNSAMPLE: Range = (1.0, 64.0);
    pub const DRIVE: Range = (1.0, 50.0);
    pub const TONE: Range = (500.0, 20000.0);
    pub const LFO_RATE: Range = (0.05, 10.0);
    pub const FEEDBACK: Range = (0.0, 0.95);
    pub const FLANGER_FEEDBACK: Range = (-0.95, 0.95);
    pub const FLANGER_DEPTH_MS: Range = (0.0, 10.0);
    pub const FLANGER_DELAY_MS: Range = (0.5, 15.0);
    pub const DELAY_TIME: Range = (0.01, 1.0);
    pub const PREDELAY: Range = (0.0, 0.2);
    pub const THRESHOLD_DB: Range = (-60.0, 0.0);
    pub const RATIO: Range = (1.0, 20.0);
    pub const COMP_ATTACK: Range = (0.0005, 0.1);
    pub const COMP_RELEASE: Range = (0.01, 1.0);
    pub const MAKEUP_DB: Range = (0.0, 24.0);
}

/// Clamps into `r`; NaN/inf become the lower bound.
pub fn clamp_f(v: &mut f32, r: Range) {
    *v = if v.is_finite() { v.clamp(r.0, r.1) } else { r.0 };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Modern,
    Bit8,
    Bit16,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Modern, Mode::Bit8, Mode::Bit16];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Modern => "Modern",
            Mode::Bit8 => "8-bit",
            Mode::Bit16 => "16-bit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseKind {
    White,
    LfsrLong,
    LfsrShort,
}

impl NoiseKind {
    pub const ALL: [NoiseKind; 3] = [NoiseKind::White, NoiseKind::LfsrLong, NoiseKind::LfsrShort];

    pub fn label(self) -> &'static str {
        match self {
            NoiseKind::White => "White",
            NoiseKind::LfsrLong => "LFSR long",
            NoiseKind::LfsrShort => "LFSR short",
        }
    }
}

/// Operator routing. Operator 4 (index 3) is always a carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FmAlgorithm {
    Serial,
    TwoStacks,
    ThreeToOne,
    Parallel,
}

impl FmAlgorithm {
    pub const ALL: [FmAlgorithm; 4] =
        [FmAlgorithm::Serial, FmAlgorithm::TwoStacks, FmAlgorithm::ThreeToOne, FmAlgorithm::Parallel];

    pub fn label(self) -> &'static str {
        match self {
            FmAlgorithm::Serial => "1→2→3→4",
            FmAlgorithm::TwoStacks => "1→2 + 3→4",
            FmAlgorithm::ThreeToOne => "1+2+3→4",
            FmAlgorithm::Parallel => "1+2+3+4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FmOperator {
    pub ratio: f32,
    /// Cents.
    pub detune: f32,
    pub level: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
}

impl Default for FmOperator {
    fn default() -> Self {
        Self { ratio: 1.0, detune: 0.0, level: 0.5, attack: 0.0, decay: 0.3, sustain: 0.5 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Source {
    Pulse { duty: f32 },
    Saw,
    Triangle,
    Sine,
    Noise { kind: NoiseKind },
    Fm { algorithm: FmAlgorithm, feedback: f32, ops: [FmOperator; 4] },
}

impl Default for Source {
    fn default() -> Self {
        Source::Pulse { duty: 0.5 }
    }
}

impl Source {
    pub const KIND_NAMES: [&'static str; 6] = ["Pulse", "Saw", "Triangle", "Sine", "Noise", "FM"];

    pub fn kind_name(&self) -> &'static str {
        match self {
            Source::Pulse { .. } => "Pulse",
            Source::Saw => "Saw",
            Source::Triangle => "Triangle",
            Source::Sine => "Sine",
            Source::Noise { .. } => "Noise",
            Source::Fm { .. } => "FM",
        }
    }

    pub fn default_of_kind(name: &str) -> Source {
        match name {
            "Saw" => Source::Saw,
            "Triangle" => Source::Triangle,
            "Sine" => Source::Sine,
            "Noise" => Source::Noise { kind: NoiseKind::White },
            "FM" => Source::default_fm(),
            _ => Source::Pulse { duty: 0.5 },
        }
    }

    pub fn default_fm() -> Source {
        let off = FmOperator { level: 0.0, ..Default::default() };
        Source::Fm {
            algorithm: FmAlgorithm::Serial,
            feedback: 0.0,
            ops: [
                FmOperator { ratio: 2.0, ..off },
                off,
                FmOperator { ratio: 1.0, level: 0.6, ..Default::default() },
                FmOperator { ratio: 1.0, level: 1.0, sustain: 0.8, ..Default::default() },
            ],
        }
    }

    pub fn label(&self) -> String {
        match self {
            Source::Pulse { duty } => format!("Pulse {}%", duty * 100.0),
            Source::Noise { kind } => format!("Noise ({})", kind.label()),
            other => other.kind_name().to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pitch {
    /// Hz.
    pub base_freq: f32,
    /// Octaves per second.
    pub slide: f32,
    /// Octaves per second².
    pub delta_slide: f32,
    /// Semitones.
    pub vibrato_depth: f32,
    /// Hz.
    pub vibrato_rate: f32,
    /// Semitone offsets cycled during the sound.
    pub arp_steps: Vec<i8>,
    /// Seconds per arpeggio step.
    pub arp_speed: f32,
}

impl Default for Pitch {
    fn default() -> Self {
        Self {
            base_freq: 440.0,
            slide: 0.0,
            delta_slide: 0.0,
            vibrato_depth: 0.0,
            vibrato_rate: 6.0,
            arp_steps: Vec::new(),
            arp_speed: 0.1,
        }
    }
}

/// One-shot envelope (no note-off). All times in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Envelope {
    pub attack: f32,
    pub decay: f32,
    pub sustain_level: f32,
    pub sustain_time: f32,
    pub release: f32,
    /// Extra level at the start of sustain that fades out over the sustain time.
    pub punch: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self { attack: 0.0, decay: 0.05, sustain_level: 0.6, sustain_time: 0.15, release: 0.2, punch: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterKind {
    #[default]
    Off,
    LowPass,
    HighPass,
    BandPass,
}

impl FilterKind {
    pub const ALL: [FilterKind; 4] =
        [FilterKind::Off, FilterKind::LowPass, FilterKind::HighPass, FilterKind::BandPass];

    pub fn label(self) -> &'static str {
        match self {
            FilterKind::Off => "Off",
            FilterKind::LowPass => "Low-pass",
            FilterKind::HighPass => "High-pass",
            FilterKind::BandPass => "Band-pass",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub kind: FilterKind,
    /// Hz.
    pub cutoff: f32,
    pub resonance: f32,
    /// Cutoff change in octaves per second.
    pub sweep: f32,
}

impl Default for Filter {
    fn default() -> Self {
        Self { kind: FilterKind::Off, cutoff: 8000.0, resonance: 0.0, sweep: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub source: Source,
    pub pitch: Pitch,
    pub env: Envelope,
    pub filter: Filter,
    pub gain: f32,
    /// Stored for future stereo output; v1 renders mono.
    pub pan: f32,
    /// Per-layer chain; not exposed in the v1 UI.
    pub effects: Vec<Effect>,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            source: Source::default(),
            pitch: Pitch::default(),
            env: Envelope::default(),
            filter: Filter::default(),
            gain: 1.0,
            pan: 0.0,
            effects: Vec::new(),
        }
    }
}

impl Layer {
    pub fn clamp(&mut self) {
        use ranges::*;
        match &mut self.source {
            Source::Pulse { duty } => clamp_f(duty, DUTY),
            Source::Fm { feedback, ops, .. } => {
                clamp_f(feedback, UNIT);
                for op in ops.iter_mut() {
                    clamp_f(&mut op.ratio, FM_RATIO);
                    clamp_f(&mut op.detune, FM_DETUNE);
                    clamp_f(&mut op.level, UNIT);
                    clamp_f(&mut op.attack, FM_TIME);
                    clamp_f(&mut op.decay, FM_TIME);
                    clamp_f(&mut op.sustain, UNIT);
                }
            }
            _ => {}
        }
        let p = &mut self.pitch;
        clamp_f(&mut p.base_freq, BASE_FREQ);
        clamp_f(&mut p.slide, SLIDE);
        clamp_f(&mut p.delta_slide, DELTA_SLIDE);
        clamp_f(&mut p.vibrato_depth, VIBRATO_DEPTH);
        clamp_f(&mut p.vibrato_rate, VIBRATO_RATE);
        clamp_f(&mut p.arp_speed, ARP_SPEED);
        p.arp_steps.truncate(MAX_ARP_STEPS);
        for s in &mut p.arp_steps {
            *s = (*s).clamp(-24, 24);
        }
        let e = &mut self.env;
        for t in [&mut e.attack, &mut e.decay, &mut e.sustain_time, &mut e.release] {
            clamp_f(t, ENV_TIME);
        }
        clamp_f(&mut e.sustain_level, UNIT);
        clamp_f(&mut e.punch, UNIT);
        clamp_f(&mut self.filter.cutoff, CUTOFF);
        clamp_f(&mut self.filter.resonance, UNIT);
        clamp_f(&mut self.filter.sweep, SWEEP);
        clamp_f(&mut self.gain, GAIN);
        clamp_f(&mut self.pan, PAN);
        for fx in &mut self.effects {
            fx.kind.clamp();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistortionKind {
    SoftClip,
    HardClip,
    Foldback,
}

impl DistortionKind {
    pub const ALL: [DistortionKind; 3] =
        [DistortionKind::SoftClip, DistortionKind::HardClip, DistortionKind::Foldback];

    pub fn label(self) -> &'static str {
        match self {
            DistortionKind::SoftClip => "Soft",
            DistortionKind::HardClip => "Hard",
            DistortionKind::Foldback => "Fold",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EffectKind {
    Bitcrusher { bits: f32, downsample: f32, mix: f32 },
    Distortion { kind: DistortionKind, drive: f32, tone: f32, mix: f32 },
    Phaser { rate: f32, depth: f32, stages: u8, feedback: f32, mix: f32 },
    Flanger { rate: f32, depth_ms: f32, delay_ms: f32, feedback: f32, mix: f32 },
    Delay { time: f32, feedback: f32, damping: f32, mix: f32 },
    Reverb { size: f32, decay: f32, damping: f32, predelay: f32, mix: f32 },
    Compressor { threshold_db: f32, ratio: f32, attack: f32, release: f32, makeup_db: f32 },
}

impl EffectKind {
    /// One default instance of every effect, in menu order.
    pub fn all_defaults() -> Vec<EffectKind> {
        vec![
            EffectKind::Bitcrusher { bits: 6.0, downsample: 4.0, mix: 1.0 },
            EffectKind::Distortion { kind: DistortionKind::SoftClip, drive: 4.0, tone: 8000.0, mix: 1.0 },
            EffectKind::Phaser { rate: 0.5, depth: 0.7, stages: 4, feedback: 0.5, mix: 0.5 },
            EffectKind::Flanger { rate: 0.3, depth_ms: 3.0, delay_ms: 2.0, feedback: 0.5, mix: 0.5 },
            EffectKind::Delay { time: 0.15, feedback: 0.4, damping: 0.3, mix: 0.35 },
            EffectKind::Reverb { size: 0.5, decay: 0.5, damping: 0.5, predelay: 0.01, mix: 0.25 },
            EffectKind::Compressor { threshold_db: -18.0, ratio: 4.0, attack: 0.005, release: 0.1, makeup_db: 6.0 },
        ]
    }

    /// The default instance of the same variant (used for double-click reset).
    pub fn defaults(&self) -> EffectKind {
        let name = self.name();
        EffectKind::all_defaults()
            .into_iter()
            .find(|k| k.name() == name)
            .expect("all_defaults covers every variant")
    }

    pub fn name(&self) -> &'static str {
        match self {
            EffectKind::Bitcrusher { .. } => "Bitcrusher",
            EffectKind::Distortion { .. } => "Distortion",
            EffectKind::Phaser { .. } => "Phaser",
            EffectKind::Flanger { .. } => "Flanger",
            EffectKind::Delay { .. } => "Delay",
            EffectKind::Reverb { .. } => "Reverb",
            EffectKind::Compressor { .. } => "Compressor",
        }
    }

    /// Effects that keep sounding after the input stops.
    pub fn has_tail(&self) -> bool {
        matches!(self, EffectKind::Delay { .. } | EffectKind::Reverb { .. } | EffectKind::Flanger { .. })
    }

    pub fn clamp(&mut self) {
        use ranges::*;
        match self {
            EffectKind::Bitcrusher { bits, downsample, mix } => {
                clamp_f(bits, BITS);
                clamp_f(downsample, DOWNSAMPLE);
                clamp_f(mix, UNIT);
            }
            EffectKind::Distortion { drive, tone, mix, .. } => {
                clamp_f(drive, DRIVE);
                clamp_f(tone, TONE);
                clamp_f(mix, UNIT);
            }
            EffectKind::Phaser { rate, depth, stages, feedback, mix } => {
                clamp_f(rate, LFO_RATE);
                clamp_f(depth, UNIT);
                *stages = (*stages).clamp(2, 8);
                clamp_f(feedback, FEEDBACK);
                clamp_f(mix, UNIT);
            }
            EffectKind::Flanger { rate, depth_ms, delay_ms, feedback, mix } => {
                clamp_f(rate, LFO_RATE);
                clamp_f(depth_ms, FLANGER_DEPTH_MS);
                clamp_f(delay_ms, FLANGER_DELAY_MS);
                clamp_f(feedback, FLANGER_FEEDBACK);
                clamp_f(mix, UNIT);
            }
            EffectKind::Delay { time, feedback, damping, mix } => {
                clamp_f(time, DELAY_TIME);
                clamp_f(feedback, FEEDBACK);
                clamp_f(damping, UNIT);
                clamp_f(mix, UNIT);
            }
            EffectKind::Reverb { size, decay, damping, predelay, mix } => {
                clamp_f(size, UNIT);
                clamp_f(decay, UNIT);
                clamp_f(damping, UNIT);
                clamp_f(predelay, PREDELAY);
                clamp_f(mix, UNIT);
            }
            EffectKind::Compressor { threshold_db, ratio, attack, release, makeup_db } => {
                clamp_f(threshold_db, THRESHOLD_DB);
                clamp_f(ratio, RATIO);
                clamp_f(attack, COMP_ATTACK);
                clamp_f(release, COMP_RELEASE);
                clamp_f(makeup_db, MAKEUP_DB);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub id: u64,
    pub enabled: bool,
    pub kind: EffectKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundPatch {
    pub schema_version: u32,
    pub mode: Mode,
    pub seed: u64,
    pub master_volume: f32,
    pub layers: Vec<Layer>,
    pub master_effects: Vec<Effect>,
}

impl Default for SoundPatch {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            mode: Mode::Modern,
            seed: 1,
            master_volume: 0.8,
            layers: vec![Layer::default()],
            master_effects: Vec::new(),
        }
    }
}

impl SoundPatch {
    pub fn clamp(&mut self) {
        if self.layers.is_empty() {
            self.layers.push(Layer::default());
        }
        self.layers.truncate(MAX_LAYERS);
        clamp_f(&mut self.master_volume, ranges::UNIT);
        for layer in &mut self.layers {
            layer.clamp();
        }
        for fx in &mut self.master_effects {
            fx.kind.clamp();
        }
    }

    pub fn next_effect_id(&self) -> u64 {
        self.master_effects
            .iter()
            .chain(self.layers.iter().flat_map(|l| l.effects.iter()))
            .map(|e| e.id)
            .max()
            .map_or(1, |m| m + 1)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("SoundPatch always serializes")
    }

    /// Parses, migrates older schemas step by step, and clamps every value.
    pub fn from_json(s: &str) -> Result<Self> {
        let mut value: serde_json::Value = serde_json::from_str(s)?;
        let version = value.get("schema_version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        if version > SCHEMA_VERSION {
            bail!("patch schema {version} is newer than this app supports ({SCHEMA_VERSION})");
        }
        migrate(&mut value, version);
        let mut patch: SoundPatch = serde_json::from_value(value)?;
        patch.schema_version = SCHEMA_VERSION;
        patch.clamp();
        Ok(patch)
    }
}

/// Upgrades raw JSON from `from` to `SCHEMA_VERSION`. When the schema changes, bump
/// `SCHEMA_VERSION` and add a step here: `if from < 2 { v1_to_v2(value) }`.
fn migrate(_value: &mut serde_json::Value, _from: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips() {
        let p = SoundPatch::default();
        let back = SoundPatch::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn every_effect_and_source_round_trips() {
        let mut p = SoundPatch::default();
        p.layers[0].source = Source::default_fm();
        p.master_effects = EffectKind::all_defaults()
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Effect { id: i as u64, enabled: i % 2 == 0, kind })
            .collect();
        assert_eq!(SoundPatch::from_json(&p.to_json()).unwrap(), p);
    }

    #[test]
    fn minimal_old_json_loads_with_defaults() {
        let p = SoundPatch::from_json(
            r#"{"schema_version":1,"mode":"Bit8","layers":[{"source":{"type":"Saw"}}]}"#,
        )
        .unwrap();
        assert_eq!(p.mode, Mode::Bit8);
        assert_eq!(p.layers[0].source, Source::Saw);
        assert_eq!(p.layers[0].env, Envelope::default());
        assert_eq!(p.master_volume, SoundPatch::default().master_volume);
    }

    #[test]
    fn missing_schema_version_is_treated_as_v1() {
        let p = SoundPatch::from_json(r#"{"seed":5}"#).unwrap();
        assert_eq!(p.seed, 5);
        assert_eq!(p.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let err = SoundPatch::from_json(r#"{"schema_version":999}"#).unwrap_err();
        assert!(err.to_string().contains("newer"));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(SoundPatch::from_json("{").is_err());
        assert!(SoundPatch::from_json(r#"{"layers":"nope"}"#).is_err());
    }

    #[test]
    fn clamp_fixes_out_of_range_and_nan() {
        let mut p = SoundPatch { master_volume: f32::NAN, ..Default::default() };
        p.layers[0].pitch.base_freq = 1e9;
        p.layers[0].env.release = -3.0;
        p.layers[0].pitch.arp_steps = vec![100; 20];
        p.master_effects.push(Effect {
            id: 1,
            enabled: true,
            kind: EffectKind::Delay { time: 50.0, feedback: 2.0, damping: -1.0, mix: 9.0 },
        });
        p.clamp();
        assert_eq!(p.master_volume, ranges::UNIT.0);
        assert_eq!(p.layers[0].pitch.base_freq, ranges::BASE_FREQ.1);
        assert_eq!(p.layers[0].env.release, 0.0);
        assert_eq!(p.layers[0].pitch.arp_steps, vec![24; MAX_ARP_STEPS]);
        assert_eq!(
            p.master_effects[0].kind,
            EffectKind::Delay { time: 1.0, feedback: 0.95, damping: 0.0, mix: 1.0 }
        );
    }

    #[test]
    fn clamp_guarantees_one_to_four_layers() {
        let mut p = SoundPatch { layers: vec![], ..Default::default() };
        p.clamp();
        assert_eq!(p.layers.len(), 1);
        p.layers = vec![Layer::default(); 9];
        p.clamp();
        assert_eq!(p.layers.len(), MAX_LAYERS);
    }

    #[test]
    fn next_effect_id_is_unique() {
        let mut p = SoundPatch::default();
        assert_eq!(p.next_effect_id(), 1);
        p.master_effects.push(Effect { id: 7, enabled: true, kind: EffectKind::all_defaults()[0] });
        p.layers[0].effects.push(Effect { id: 9, enabled: true, kind: EffectKind::all_defaults()[1] });
        assert_eq!(p.next_effect_id(), 10);
    }

    #[test]
    fn defaults_returns_same_variant() {
        for k in EffectKind::all_defaults() {
            assert_eq!(k.defaults().name(), k.name());
        }
    }
}
