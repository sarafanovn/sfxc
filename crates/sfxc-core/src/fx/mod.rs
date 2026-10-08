mod bitcrusher;
mod compressor;
mod delay;
mod distortion;
mod flanger;
mod phaser;
mod reverb;

pub use bitcrusher::Bitcrusher;
pub use compressor::Compressor;
pub use delay::Delay;
pub use distortion::Distortion;
pub use flanger::Flanger;
pub use phaser::Phaser;
pub use reverb::Reverb;

use crate::patch::{Effect, EffectKind};

pub(crate) fn mix(dry: f32, wet: f32, amount: f32) -> f32 {
    dry * (1.0 - amount) + wet * amount
}

#[inline]
pub(crate) fn wrap_next(i: usize, len: usize) -> usize {
    let n = i + 1;
    if n == len { 0 } else { n }
}

pub enum Processor {
    Bitcrusher(Bitcrusher),
    Distortion(Distortion),
    Phaser(Phaser),
    Flanger(Flanger),
    Delay(Delay),
    Reverb(Reverb),
    Compressor(Compressor),
}

impl Processor {
    pub fn new(kind: &EffectKind, sr: f32) -> Self {
        match *kind {
            EffectKind::Bitcrusher { bits, downsample, mix } => Processor::Bitcrusher(Bitcrusher::new(bits, downsample, mix)),
            EffectKind::Distortion { kind, drive, tone, mix } => Processor::Distortion(Distortion::new(kind, drive, tone, mix, sr)),
            EffectKind::Phaser { rate, depth, stages, feedback, mix } => Processor::Phaser(Phaser::new(rate, depth, stages, feedback, mix, sr)),
            EffectKind::Flanger { rate, depth_ms, delay_ms, feedback, mix } => {
                Processor::Flanger(Flanger::new(rate, depth_ms, delay_ms, feedback, mix, sr))
            }
            EffectKind::Delay { time, feedback, damping, mix } => Processor::Delay(Delay::new(time, feedback, damping, mix, sr)),
            EffectKind::Reverb { size, decay, damping, predelay, mix } => {
                Processor::Reverb(Reverb::new(size, decay, damping, predelay, mix, sr))
            }
            EffectKind::Compressor { threshold_db, ratio, attack, release, makeup_db } => {
                Processor::Compressor(Compressor::new(threshold_db, ratio, attack, release, makeup_db, sr))
            }
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        match self {
            Processor::Bitcrusher(p) => p.process(x),
            Processor::Distortion(p) => p.process(x),
            Processor::Phaser(p) => p.process(x),
            Processor::Flanger(p) => p.process(x),
            Processor::Delay(p) => p.process(x),
            Processor::Reverb(p) => p.process(x),
            Processor::Compressor(p) => p.process(x),
        }
    }
}

pub struct FxChain {
    items: Vec<Processor>,
}

impl FxChain {
    pub fn new(effects: &[Effect], sample_rate: f32) -> Self {
        Self {
            items: effects
                .iter()
                .filter(|e| e.enabled)
                .map(|e| Processor::new(&e.kind, sample_rate))
                .collect(),
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        self.items.iter_mut().fold(x, |acc, p| p.process(acc))
    }

    pub fn has_tail(effects: &[Effect]) -> bool {
        effects.iter().any(|e| e.enabled && e.kind.has_tail())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::DistortionKind;
    use crate::rng::Rng;

    const SR: f32 = 48_000.0;

    fn noise(n: usize, amp: f32) -> Vec<f32> {
        let mut r = Rng::new(7);
        (0..n).map(|_| r.bipolar() * amp).collect()
    }

    fn run(kind: EffectKind, input: &[f32]) -> Vec<f32> {
        let mut p = Processor::new(&kind, SR);
        input.iter().map(|&x| p.process(x)).collect()
    }

    fn with_mix(mut kind: EffectKind, value: f32) -> EffectKind {
        match &mut kind {
            EffectKind::Bitcrusher { mix, .. }
            | EffectKind::Distortion { mix, .. }
            | EffectKind::Phaser { mix, .. }
            | EffectKind::Flanger { mix, .. }
            | EffectKind::Delay { mix, .. }
            | EffectKind::Reverb { mix, .. } => *mix = value,
            EffectKind::Compressor { .. } => {}
        }
        kind
    }

    #[test]
    fn mix_zero_is_identity() {
        let x = noise(4800, 0.5);
        for kind in EffectKind::all_defaults() {
            if matches!(kind, EffectKind::Compressor { .. }) {
                continue;
            }
            let y = run(with_mix(kind, 0.0), &x);
            for (a, b) in x.iter().zip(&y) {
                assert!((a - b).abs() < 1e-6, "{}", kind.name());
            }
        }
    }

    #[test]
    fn compressor_below_threshold_is_identity() {
        let x: Vec<f32> = (0..4800).map(|i| 0.5 * (i as f32 * 0.05).sin()).collect();
        let k = EffectKind::Compressor { threshold_db: 0.0, ratio: 4.0, attack: 0.005, release: 0.1, makeup_db: 0.0 };
        assert_eq!(run(k, &x), x);
    }

    #[test]
    fn compressor_reduces_loud_signal() {
        let k = EffectKind::Compressor { threshold_db: -20.0, ratio: 10.0, attack: 0.001, release: 0.1, makeup_db: 0.0 };
        let y = run(k, &vec![1.0; 4800]);
        assert!(*y.last().unwrap() < 0.3);
    }

    #[test]
    fn stable_at_extreme_settings() {
        let x = noise(SR as usize * 10, 0.25);
        let extreme = [
            EffectKind::Bitcrusher { bits: 1.0, downsample: 64.0, mix: 1.0 },
            EffectKind::Distortion { kind: DistortionKind::Foldback, drive: 50.0, tone: 20_000.0, mix: 1.0 },
            EffectKind::Phaser { rate: 10.0, depth: 1.0, stages: 8, feedback: 0.95, mix: 1.0 },
            EffectKind::Flanger { rate: 10.0, depth_ms: 10.0, delay_ms: 15.0, feedback: 0.95, mix: 1.0 },
            EffectKind::Flanger { rate: 10.0, depth_ms: 10.0, delay_ms: 0.5, feedback: -0.95, mix: 1.0 },
            EffectKind::Delay { time: 0.01, feedback: 0.95, damping: 0.0, mix: 1.0 },
            EffectKind::Reverb { size: 1.0, decay: 1.0, damping: 0.0, predelay: 0.2, mix: 1.0 },
            EffectKind::Compressor { threshold_db: -60.0, ratio: 20.0, attack: 0.0005, release: 0.01, makeup_db: 24.0 },
        ];
        for k in extreme {
            let y = run(k, &x);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 50.0), "{}", k.name());
        }
    }

    #[test]
    fn bitcrusher_quantizes() {
        let k = EffectKind::Bitcrusher { bits: 2.0, downsample: 1.0, mix: 1.0 };
        for v in run(k, &noise(1000, 1.0)) {
            assert!([-1.0, -0.5, 0.0, 0.5, 1.0].contains(&v), "{v}");
        }
    }

    #[test]
    fn delay_echo_lands_on_time() {
        let mut x = vec![0.0; 2000];
        x[0] = 1.0;
        let y = run(EffectKind::Delay { time: 0.01, feedback: 0.0, damping: 0.0, mix: 0.35 }, &x);
        assert_eq!(y[0], 1.0);
        assert!((y[480] - 0.35).abs() < 1e-6);
        assert_eq!(y[479], 0.0);
    }

    #[test]
    fn chain_skips_disabled() {
        let fx = vec![Effect { id: 1, enabled: false, kind: EffectKind::Bitcrusher { bits: 1.0, downsample: 8.0, mix: 1.0 } }];
        let mut chain = FxChain::new(&fx, SR);
        assert_eq!(chain.process(0.3), 0.3);
        assert!(!FxChain::has_tail(&[Effect { id: 2, enabled: false, kind: EffectKind::all_defaults()[5] }]));
        assert!(FxChain::has_tail(&[Effect { id: 2, enabled: true, kind: EffectKind::all_defaults()[5] }]));
    }
}
