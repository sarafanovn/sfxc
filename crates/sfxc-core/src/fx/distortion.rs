use std::f32::consts::TAU;

use super::mix;
use crate::patch::DistortionKind;

pub struct Distortion {
    kind: DistortionKind,
    drive: f32,
    mix: f32,
    lp: f32,
    lp_coef: f32,
}

impl Distortion {
    pub fn new(kind: DistortionKind, drive: f32, tone: f32, mix: f32, sample_rate: f32) -> Self {
        let lp_coef = 1.0 - (-TAU * tone.min(sample_rate * 0.45) / sample_rate).exp();
        Self { kind, drive, mix, lp: 0.0, lp_coef }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let d = x * self.drive;
        let shaped = match self.kind {
            DistortionKind::SoftClip => d.tanh(),
            DistortionKind::HardClip => d.clamp(-1.0, 1.0),
            DistortionKind::Foldback => 1.0 - ((d + 1.0).rem_euclid(4.0) - 2.0).abs(),
        };
        self.lp += self.lp_coef * (shaped - self.lp);
        mix(x, self.lp, self.mix)
    }
}
