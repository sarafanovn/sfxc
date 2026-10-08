use std::f32::consts::{PI, TAU};

use super::mix;

/// Cascade of first-order all-pass stages swept by a sine LFO (200–3200 Hz).
pub struct Phaser {
    rate: f32,
    depth: f32,
    stages: usize,
    feedback: f32,
    mix: f32,
    sample_rate: f32,
    phase: f32,
    z: [f32; 8],
    last: f32,
}

impl Phaser {
    pub fn new(rate: f32, depth: f32, stages: u8, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        Self {
            rate,
            depth,
            stages: (stages as usize).clamp(2, 8),
            feedback,
            mix,
            sample_rate,
            phase: 0.0,
            z: [0.0; 8],
            last: 0.0,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let lfo = 0.5 + 0.5 * (TAU * self.phase).sin();
        self.phase = (self.phase + self.rate / self.sample_rate).fract();
        let f = (200.0 * 2f32.powf(self.depth * 4.0 * lfo)).min(self.sample_rate * 0.45);
        let tn = (PI * f / self.sample_rate).tan();
        let a = (tn - 1.0) / (tn + 1.0);
        let mut y = x + self.feedback * self.last;
        for z in self.z.iter_mut().take(self.stages) {
            let out = a * y + *z;
            *z = y - a * out;
            y = out;
        }
        self.last = y;
        mix(x, 0.5 * (x + y), self.mix)
    }
}
