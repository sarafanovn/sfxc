//! Oscillators. One `Oscillator` renders one layer's source sample by sample.

use std::f32::consts::TAU;

use crate::patch::{FmAlgorithm, FmOperator, Mode, NoiseKind, Source};
use crate::rng::Rng;

/// Noise is clocked at `freq * NOISE_CLOCK_MULT`, so pitch controls noise colour.
pub const NOISE_CLOCK_MULT: f32 = 32.0;

/// PolyBLEP residual for a discontinuity at phase 0. `t` is phase in [0,1), `dt` is freq/sr.
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// One step of the NES 15-bit noise LFSR. `short` selects the 93-step mode.
pub fn lfsr_step(state: u16, short: bool) -> u16 {
    let tap = if short { 6 } else { 1 };
    let bit = (state ^ (state >> tap)) & 1;
    (state >> 1) | (bit << 14)
}

/// Envelope of one FM operator: linear attack, linear decay to sustain, then hold.
pub fn fm_op_env(op: &FmOperator, t: f32) -> f32 {
    if t < op.attack {
        return t / op.attack;
    }
    let t = t - op.attack;
    if t < op.decay {
        return 1.0 - (1.0 - op.sustain) * t / op.decay;
    }
    op.sustain
}

pub struct Oscillator {
    source: Source,
    bandlimited: bool,
    stepped_triangle: bool,
    sample_rate: f32,
    phase: f32,
    rng: Rng,
    noise_acc: f32,
    noise_value: f32,
    lfsr: u16,
    fm_phase: [f32; 4],
    fm_prev: [f32; 2],
}

impl Oscillator {
    pub fn new(source: Source, mode: Mode, sample_rate: f32, seed: u64) -> Self {
        Self {
            source,
            bandlimited: mode != Mode::Bit8,
            stepped_triangle: mode == Mode::Bit8,
            sample_rate,
            phase: 0.0,
            rng: Rng::new(seed),
            noise_acc: 1.0,
            noise_value: 0.0,
            lfsr: 1,
            fm_phase: [0.0; 4],
            fm_prev: [0.0; 2],
        }
    }

    /// `freq` in Hz (caller keeps it below Nyquist), `t` seconds since sound start.
    pub fn next(&mut self, freq: f32, t: f32) -> f32 {
        let dt = freq / self.sample_rate;
        let p = self.phase;
        let out = match self.source {
            Source::Pulse { duty } => self.pulse(p, dt, duty),
            Source::Saw => {
                let mut v = 2.0 * p - 1.0;
                if self.bandlimited {
                    v -= poly_blep(p, dt);
                }
                v
            }
            Source::Triangle => self.triangle(p),
            Source::Sine => (TAU * p).sin(),
            Source::Noise { kind } => self.noise(kind, dt),
            Source::Fm { algorithm, feedback, ops } => self.fm(algorithm, feedback, &ops, dt, t),
        };
        self.phase = (p + dt).fract();
        out
    }

    fn pulse(&self, p: f32, dt: f32, duty: f32) -> f32 {
        let mut v = if p < duty { 1.0 } else { -1.0 };
        if self.bandlimited {
            v += poly_blep(p, dt);
            v -= poly_blep((p - duty + 1.0).fract(), dt);
        }
        v
    }

    fn triangle(&self, p: f32) -> f32 {
        if self.stepped_triangle {
            // NES triangle: 32-step sequence 15..0, 0..15.
            let step = ((p * 32.0) as u32).min(31);
            let level = if step < 16 { 15 - step } else { step - 16 };
            level as f32 / 7.5 - 1.0
        } else {
            4.0 * (p - 0.5).abs() - 1.0
        }
    }

    fn noise(&mut self, kind: NoiseKind, dt: f32) -> f32 {
        self.noise_acc += dt * NOISE_CLOCK_MULT;
        let mut clocks = 0;
        while self.noise_acc >= 1.0 && clocks < 64 {
            self.noise_acc -= 1.0;
            clocks += 1;
            self.noise_value = match kind {
                NoiseKind::White => self.rng.bipolar(),
                NoiseKind::LfsrLong | NoiseKind::LfsrShort => {
                    self.lfsr = lfsr_step(self.lfsr, kind == NoiseKind::LfsrShort);
                    if self.lfsr & 1 == 0 { 1.0 } else { -1.0 }
                }
            };
        }
        if clocks == 64 {
            self.noise_acc = self.noise_acc.fract();
        }
        self.noise_value
    }

    fn fm(&mut self, algorithm: FmAlgorithm, feedback: f32, ops: &[FmOperator; 4], dt: f32, t: f32) -> f32 {
        let lvl: [f32; 4] = std::array::from_fn(|i| ops[i].level * fm_op_env(&ops[i], t));
        let ph = self.fm_phase;
        // Modulation is a phase offset in cycles.
        let op = |i: usize, m: f32| (TAU * (ph[i] + m)).sin() * lvl[i];
        let fb = feedback * 0.5 * (self.fm_prev[0] + self.fm_prev[1]);
        let o0 = op(0, fb);
        let out = match algorithm {
            FmAlgorithm::Serial => op(3, op(2, op(1, o0))),
            FmAlgorithm::TwoStacks => 0.5 * (op(1, o0) + op(3, op(2, 0.0))),
            FmAlgorithm::ThreeToOne => op(3, o0 + op(1, 0.0) + op(2, 0.0)),
            FmAlgorithm::Parallel => 0.25 * (o0 + op(1, 0.0) + op(2, 0.0) + op(3, 0.0)),
        };
        self.fm_prev = [o0, self.fm_prev[0]];
        for (i, o) in ops.iter().enumerate() {
            let ratio = o.ratio * 2f32.powf(o.detune / 1200.0);
            self.fm_phase[i] = (self.fm_phase[i] + dt * ratio).fract();
        }
        out.clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::FmOperator;

    const SR: f32 = 48_000.0;

    fn run(source: Source, mode: Mode, freq: f32, n: usize) -> Vec<f32> {
        let mut osc = Oscillator::new(source, mode, SR, 3);
        (0..n).map(|i| osc.next(freq, i as f32 / SR)).collect()
    }

    fn goertzel_mag(x: &[f32], freq: f32) -> f64 {
        let w = std::f64::consts::TAU * freq as f64 / SR as f64;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s = v as f64 + c * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0).sqrt() * 2.0 / x.len() as f64
    }

    #[test]
    fn all_sources_stay_in_range() {
        let sources = [
            Source::Pulse { duty: 0.1 },
            Source::Saw,
            Source::Triangle,
            Source::Sine,
            Source::Noise { kind: NoiseKind::White },
            Source::Noise { kind: NoiseKind::LfsrLong },
            Source::Noise { kind: NoiseKind::LfsrShort },
            Source::default_fm(),
        ];
        for mode in Mode::ALL {
            for s in sources {
                for f in [30.0, 440.0, 5000.0, 20_000.0] {
                    for v in run(s, mode, f, 4800) {
                        assert!(v.is_finite() && v.abs() <= 1.05, "{s:?} {mode:?} {f} -> {v}");
                    }
                }
            }
        }
    }

    #[test]
    fn lfsr_periods_match_nes() {
        for (short, expected) in [(false, 32_767), (true, 93)] {
            let start = lfsr_step(1, short);
            let mut s = start;
            let mut n = 0;
            loop {
                s = lfsr_step(s, short);
                n += 1;
                if s == start {
                    break;
                }
            }
            assert_eq!(n, expected, "short={short}");
        }
    }

    #[test]
    fn polyblep_reduces_aliasing() {
        // Harmonic 14 of 3520 Hz (49 280 Hz) folds back to 1280 Hz at 48 kHz.
        let n = SR as usize;
        let naive = run(Source::Saw, Mode::Bit8, 3520.0, n);
        let blep = run(Source::Saw, Mode::Modern, 3520.0, n);
        let a_naive = goertzel_mag(&naive, 1280.0);
        let a_blep = goertzel_mag(&blep, 1280.0);
        assert!(a_naive > 0.01, "naive alias should be audible: {a_naive}");
        assert!(a_blep < a_naive * 0.3, "blep {a_blep} vs naive {a_naive}");
    }

    #[test]
    fn stepped_triangle_has_16_levels() {
        let v = run(Source::Triangle, Mode::Bit8, 100.0, 4800);
        let mut levels: Vec<i32> = v.iter().map(|x| (x * 1000.0).round() as i32).collect();
        levels.sort();
        levels.dedup();
        assert_eq!(levels.len(), 16);
    }

    #[test]
    fn fm_with_silent_modulators_equals_sine() {
        let silent = FmOperator { level: 0.0, ..Default::default() };
        let carrier = FmOperator { level: 1.0, sustain: 1.0, ..Default::default() };
        let fm = Source::Fm {
            algorithm: FmAlgorithm::Serial,
            feedback: 0.0,
            ops: [silent, silent, silent, carrier],
        };
        let a = run(fm, Mode::Modern, 440.0, 2000);
        let b = run(Source::Sine, Mode::Modern, 440.0, 2000);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-5);
        }
    }

    #[test]
    fn noise_is_seeded() {
        let a = run(Source::Noise { kind: NoiseKind::White }, Mode::Modern, 1000.0, 500);
        let b = run(Source::Noise { kind: NoiseKind::White }, Mode::Modern, 1000.0, 500);
        assert_eq!(a, b);
        let mut other = Oscillator::new(Source::Noise { kind: NoiseKind::White }, Mode::Modern, SR, 4);
        let c: Vec<f32> = (0..500).map(|i| other.next(1000.0, i as f32 / SR)).collect();
        assert_ne!(a, c);
    }
}
