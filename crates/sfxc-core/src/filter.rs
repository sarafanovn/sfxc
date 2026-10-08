//! Topology-preserving-transform state-variable filter (Zavalishin). Stable under
//! per-sample cutoff modulation.

use std::f32::consts::PI;

use crate::patch::{Filter, FilterKind};

#[derive(Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn process(&mut self, x: f32, kind: FilterKind, cutoff: f32, resonance: f32, sample_rate: f32) -> f32 {
        if kind == FilterKind::Off {
            return x;
        }
        let fc = cutoff.clamp(20.0, sample_rate * 0.45);
        let g = (PI * fc / sample_rate).tan();
        let k = 2.0 - 1.96 * resonance.clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match kind {
            FilterKind::LowPass => v2,
            FilterKind::BandPass => v1,
            FilterKind::HighPass => x - k * v1 - v2,
            FilterKind::Off => x,
        }
    }
}

/// Cutoff at time `t` with the exponential sweep applied (clamped later in `process`).
pub fn cutoff_at(f: &Filter, t: f32) -> f32 {
    f.cutoff * 2f32.powf(f.sweep * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    const SR: f32 = 48_000.0;

    fn run(kind: FilterKind, cutoff: f32, res: f32, input: &[f32]) -> Vec<f32> {
        let mut f = Svf::default();
        input.iter().map(|&x| f.process(x, kind, cutoff, res, SR)).collect()
    }

    fn sine(freq: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (TAU * freq * i as f32 / SR).sin()).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn off_is_identity() {
        let x = sine(1000.0, 1000);
        assert_eq!(run(FilterKind::Off, 100.0, 0.5, &x), x);
    }

    #[test]
    fn lowpass_passes_dc_and_cuts_highs() {
        let dc = run(FilterKind::LowPass, 200.0, 0.0, &vec![1.0; 9600]);
        assert!((dc.last().unwrap() - 1.0).abs() < 1e-3);
        let hi = run(FilterKind::LowPass, 200.0, 0.0, &sine(10_000.0, 9600));
        assert!(rms(&hi[4800..]) < 0.01);
    }

    #[test]
    fn highpass_removes_dc() {
        let y = run(FilterKind::HighPass, 200.0, 0.0, &vec![1.0; 9600]);
        assert!(y.last().unwrap().abs() < 1e-3);
    }

    #[test]
    fn stable_at_max_resonance() {
        let mut r = crate::rng::Rng::new(1);
        let noise: Vec<f32> = (0..48_000).map(|_| r.bipolar()).collect();
        for kind in [FilterKind::LowPass, FilterKind::HighPass, FilterKind::BandPass] {
            let y = run(kind, 5000.0, 1.0, &noise);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 60.0));
        }
    }

    #[test]
    fn cutoff_sweep() {
        let f = Filter { cutoff: 1000.0, sweep: 1.0, ..Default::default() };
        assert!((cutoff_at(&f, 1.0) - 2000.0).abs() < 0.1);
    }
}
