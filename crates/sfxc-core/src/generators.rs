//! sfxr-style generators: each category draws parameters from ranges that sound
//! like that kind of effect. `mutate` nudges an existing sound.

use crate::mode::{map_source_to_mode, NES_DUTIES};
use crate::patch::*;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    PickupCoin,
    LaserShoot,
    Explosion,
    PowerUp,
    HitHurt,
    Jump,
    BlipSelect,
    Random,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::PickupCoin,
        Category::LaserShoot,
        Category::Explosion,
        Category::PowerUp,
        Category::HitHurt,
        Category::Jump,
        Category::BlipSelect,
        Category::Random,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::PickupCoin => "Coin",
            Category::LaserShoot => "Shoot",
            Category::Explosion => "Explosion",
            Category::PowerUp => "Power-up",
            Category::HitHurt => "Hit",
            Category::Jump => "Jump",
            Category::BlipSelect => "Blip",
            Category::Random => "Random",
        }
    }
}

const PULSE: u8 = 0;
const SAW: u8 = 1;
const TRIANGLE: u8 = 2;
const SINE: u8 = 3;

fn pulse(rng: &mut Rng, mode: Mode) -> Source {
    let duty = if mode == Mode::Bit8 { *rng.pick(&NES_DUTIES) } else { rng.range(0.1, 0.5) };
    Source::Pulse { duty }
}

fn fm(rng: &mut Rng) -> Source {
    let mut ops = [FmOperator::default(); 4];
    for op in ops.iter_mut() {
        op.ratio = *rng.pick(&[0.5, 1.0, 2.0, 3.0, 4.0]);
        op.level = rng.range(0.0, 0.8);
        op.attack = 0.0;
        op.decay = rng.range(0.05, 0.6);
        op.sustain = rng.range(0.0, 0.6);
    }
    ops[3] = FmOperator { ratio: 1.0, level: 1.0, sustain: 1.0, ..ops[3] };
    Source::Fm { algorithm: *rng.pick(&FmAlgorithm::ALL), feedback: rng.range(0.0, 0.5), ops }
}

/// A pitched source from `choices`; in 16-bit mode sometimes FM instead.
fn tonal(rng: &mut Rng, mode: Mode, choices: &[u8]) -> Source {
    let s = match *rng.pick(choices) {
        PULSE => pulse(rng, mode),
        SAW => Source::Saw,
        TRIANGLE => Source::Triangle,
        _ => Source::Sine,
    };
    let s = if mode == Mode::Bit16 && rng.chance(0.4) { fm(rng) } else { s };
    map_source_to_mode(&s, mode)
}

fn noise(rng: &mut Rng, mode: Mode) -> Source {
    map_source_to_mode(&Source::Noise { kind: *rng.pick(&NoiseKind::ALL) }, mode)
}

pub fn generate(category: Category, mode: Mode, seed: u64) -> SoundPatch {
    let mut rng = Rng::new(seed);
    let r = &mut rng;
    let mut l = Layer {
        env: Envelope { attack: 0.0, decay: 0.0, sustain_level: 1.0, sustain_time: 0.1, release: 0.2, punch: 0.0 },
        ..Default::default()
    };
    match category {
        Category::PickupCoin => {
            l.source = tonal(r, mode, &[PULSE, TRIANGLE, SINE]);
            l.pitch.base_freq = r.range(600.0, 1500.0);
            if r.chance(0.7) {
                l.pitch.arp_steps = vec![0, *r.pick(&[4i8, 5, 7, 12])];
                l.pitch.arp_speed = r.range(0.04, 0.1);
            }
            l.env.sustain_time = r.range(0.02, 0.1);
            l.env.release = r.range(0.1, 0.3);
            l.env.punch = r.range(0.3, 0.6);
        }
        Category::LaserShoot => {
            l.source = tonal(r, mode, &[PULSE, PULSE, SAW, SINE]);
            l.pitch.base_freq = r.range(500.0, 2000.0);
            l.pitch.slide = r.range(-6.0, -2.0);
            if r.chance(0.3) {
                l.pitch.delta_slide = r.range(0.0, 4.0);
            }
            l.env.sustain_time = r.range(0.05, 0.15);
            l.env.release = r.range(0.05, 0.25);
            l.env.punch = r.range(0.0, 0.3);
            if r.chance(0.3) {
                l.filter = Filter { kind: FilterKind::HighPass, cutoff: r.range(200.0, 1000.0), ..Default::default() };
            }
        }
        Category::Explosion => {
            l.source = map_source_to_mode(&Source::Noise { kind: NoiseKind::White }, mode);
            l.pitch.base_freq = r.range(40.0, 200.0);
            l.pitch.slide = r.range(-1.0, 0.0);
            l.env.sustain_time = r.range(0.1, 0.3);
            l.env.release = r.range(0.3, 0.9);
            l.env.punch = r.range(0.2, 0.8);
            if r.chance(0.6) {
                l.filter = Filter {
                    kind: FilterKind::LowPass,
                    cutoff: r.range(1000.0, 5000.0),
                    resonance: 0.0,
                    sweep: r.range(-2.0, 0.0),
                };
            }
        }
        Category::PowerUp => {
            l.source = tonal(r, mode, &[PULSE, SAW, TRIANGLE]);
            l.pitch.base_freq = r.range(200.0, 600.0);
            l.pitch.slide = r.range(1.0, 3.0);
            if r.chance(0.5) {
                l.pitch.vibrato_depth = r.range(0.1, 0.5);
                l.pitch.vibrato_rate = r.range(8.0, 20.0);
            } else if r.chance(0.5) {
                l.pitch.arp_steps = vec![0, 4, 7, 12];
                l.pitch.arp_speed = r.range(0.04, 0.08);
            }
            l.env.sustain_time = r.range(0.1, 0.3);
            l.env.release = r.range(0.1, 0.4);
        }
        Category::HitHurt => {
            l.source = if r.chance(0.4) {
                map_source_to_mode(&Source::Noise { kind: NoiseKind::White }, mode)
            } else {
                tonal(r, mode, &[PULSE, SAW])
            };
            l.pitch.base_freq = r.range(100.0, 500.0);
            l.pitch.slide = r.range(-4.0, -1.0);
            l.env.sustain_time = r.range(0.01, 0.05);
            l.env.release = r.range(0.05, 0.2);
            l.env.punch = r.range(0.0, 0.5);
            if r.chance(0.5) {
                l.filter = Filter { kind: FilterKind::LowPass, cutoff: r.range(2000.0, 8000.0), ..Default::default() };
            }
        }
        Category::Jump => {
            l.source = map_source_to_mode(&pulse(r, mode), mode);
            l.pitch.base_freq = r.range(200.0, 500.0);
            l.pitch.slide = r.range(1.0, 3.0);
            l.env.sustain_time = r.range(0.05, 0.15);
            l.env.release = r.range(0.05, 0.2);
        }
        Category::BlipSelect => {
            l.source = tonal(r, mode, &[PULSE, SINE]);
            l.pitch.base_freq = r.range(400.0, 1200.0);
            l.env.sustain_time = r.range(0.02, 0.06);
            l.env.release = r.range(0.01, 0.06);
        }
        Category::Random => {
            l.source = if r.chance(0.25) { noise(r, mode) } else { tonal(r, mode, &[PULSE, SAW, TRIANGLE, SINE]) };
            l.pitch.base_freq = r.range(100.0, 2000.0);
            l.pitch.slide = r.range(-4.0, 4.0);
            l.pitch.delta_slide = r.range(-4.0, 4.0);
            if r.chance(0.3) {
                l.pitch.vibrato_depth = r.range(0.0, 1.0);
                l.pitch.vibrato_rate = r.range(2.0, 20.0);
            }
            if r.chance(0.2) {
                let n = 2 + (r.next_u64() % 3) as usize;
                l.pitch.arp_steps = (0..n).map(|_| r.range(-12.0, 12.0).round() as i8).collect();
                l.pitch.arp_speed = r.range(0.03, 0.15);
            }
            l.env = Envelope {
                attack: r.range(0.0, 0.2),
                decay: r.range(0.0, 0.2),
                sustain_level: r.range(0.3, 1.0),
                sustain_time: r.range(0.02, 0.3),
                release: r.range(0.05, 0.6),
                punch: r.range(0.0, 0.5),
            };
            if r.chance(0.4) {
                let kind = *r.pick(&[FilterKind::LowPass, FilterKind::HighPass]);
                // A high-pass far above a low tone would make the sound inaudible.
                let cutoff = if kind == FilterKind::HighPass { r.range(100.0, 1500.0) } else { r.range(500.0, 10_000.0) };
                l.filter = Filter { kind, cutoff, resonance: r.range(0.0, 0.7), sweep: r.range(-1.0, 1.0) };
            }
        }
    }
    let mut p = SoundPatch { mode, seed: rng.next_u64(), layers: vec![l], ..Default::default() };
    p.clamp();
    p
}

/// Small random variation of every parameter; keeps mode and effects chain.
pub fn mutate(patch: &SoundPatch, seed: u64) -> SoundPatch {
    let mut rng = Rng::new(seed);
    let r = &mut rng;
    let mut p = patch.clone();
    // Absolute nudge: up to 5% of the parameter's full range.
    let nudge = |r: &mut Rng, v: &mut f32, range: Range| *v += r.bipolar() * 0.05 * (range.1 - range.0);
    // Relative nudge for times: up to ±20%.
    let scale = |r: &mut Rng, v: &mut f32| *v *= 1.0 + r.bipolar() * 0.2;
    for l in &mut p.layers {
        l.pitch.base_freq *= 2f32.powf(r.bipolar() * 0.25);
        nudge(r, &mut l.pitch.slide, ranges::SLIDE);
        nudge(r, &mut l.pitch.delta_slide, ranges::DELTA_SLIDE);
        nudge(r, &mut l.pitch.vibrato_depth, ranges::VIBRATO_DEPTH);
        scale(r, &mut l.pitch.vibrato_rate);
        scale(r, &mut l.pitch.arp_speed);
        for t in [&mut l.env.attack, &mut l.env.decay, &mut l.env.sustain_time, &mut l.env.release] {
            scale(r, t);
        }
        nudge(r, &mut l.env.sustain_level, ranges::UNIT);
        nudge(r, &mut l.env.punch, ranges::UNIT);
        l.filter.cutoff *= 2f32.powf(r.bipolar() * 0.5);
        nudge(r, &mut l.filter.resonance, ranges::UNIT);
        nudge(r, &mut l.filter.sweep, ranges::SWEEP);
        match &mut l.source {
            Source::Pulse { duty } if p.mode == Mode::Bit8 => {
                if r.chance(0.1) {
                    *duty = *r.pick(&NES_DUTIES);
                }
            }
            Source::Pulse { duty } => nudge(r, duty, ranges::DUTY),
            Source::Noise { kind } => {
                if r.chance(0.1) {
                    let choices: &[NoiseKind] =
                        if p.mode == Mode::Bit8 { &[NoiseKind::LfsrLong, NoiseKind::LfsrShort] } else { &NoiseKind::ALL };
                    *kind = *r.pick(choices);
                }
            }
            Source::Fm { ops, feedback, .. } => {
                nudge(r, feedback, ranges::UNIT);
                for op in ops.iter_mut() {
                    nudge(r, &mut op.level, ranges::UNIT);
                    scale(r, &mut op.decay);
                }
            }
            _ => {}
        }
    }
    p.seed = rng.next_u64();
    p.clamp();
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::is_allowed;
    use crate::render::render;

    #[test]
    fn generate_uses_default_gain_and_enabled_arp() {
        for c in Category::ALL {
            for seed in 0..20 {
                let p = generate(c, Mode::Modern, seed);
                assert_eq!(p.master_volume, SoundPatch::default().master_volume);
                assert!(p.layers.iter().all(|l| l.pitch.arp_enabled));
            }
        }
    }

    #[test]
    fn every_category_and_mode_is_valid_and_short() {
        for mode in Mode::ALL {
            for cat in Category::ALL {
                for seed in 0..8 {
                    let p = generate(cat, mode, seed);
                    assert_eq!(p.mode, mode);
                    assert!(is_allowed(&p.layers[0].source, mode), "{cat:?} {mode:?} {:?}", p.layers[0].source);
                    let s = render(&p, 22_050);
                    assert!(!s.is_empty(), "{cat:?} {mode:?} seed {seed} is silent");
                    assert!(s.len() as f32 / 22_050.0 <= 3.0);
                    assert!(s.iter().all(|v| v.is_finite()));
                }
            }
        }
    }

    #[test]
    fn generate_is_deterministic() {
        for cat in Category::ALL {
            assert_eq!(generate(cat, Mode::Modern, 11), generate(cat, Mode::Modern, 11));
        }
        assert_ne!(generate(Category::Random, Mode::Modern, 1), generate(Category::Random, Mode::Modern, 2));
    }

    #[test]
    fn mutate_changes_a_little_and_keeps_chain_and_mode() {
        let mut base = generate(Category::LaserShoot, Mode::Bit8, 5);
        base.master_effects.push(Effect { id: 1, enabled: true, kind: EffectKind::all_defaults()[4] });
        let m = mutate(&base, 99);
        assert_ne!(m, base);
        assert_eq!(m.mode, base.mode);
        assert_eq!(m.master_effects, base.master_effects);
        assert!(is_allowed(&m.layers[0].source, Mode::Bit8));
        let ratio = m.layers[0].pitch.base_freq / base.layers[0].pitch.base_freq;
        assert!((0.8..=1.25).contains(&ratio), "{ratio}");
    }
}
