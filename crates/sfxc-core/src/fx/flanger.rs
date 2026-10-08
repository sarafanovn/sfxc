use std::f32::consts::TAU;

use super::{mix, wrap_next};

pub struct Flanger {
    buf: Vec<f32>,
    w: usize,
    depth: f32,
    delay: f32,
    feedback: f32,
    mix: f32,
    sample_rate: f32,
    phase_step: f32,
    phase: f32,
}

impl Flanger {
    pub fn new(rate: f32, depth_ms: f32, delay_ms: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        let len = ((0.025 * sample_rate) as usize) + 4; // max delay 15 ms + max depth 10 ms
        Self {
            buf: vec![0.0; len],
            w: 0,
            depth: depth_ms / 1000.0,
            delay: delay_ms / 1000.0,
            feedback,
            mix,
            sample_rate,
            phase_step: rate / sample_rate,
            phase: 0.0,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let len = self.buf.len();
        let lfo = 0.5 + 0.5 * (TAU * self.phase).sin();
        self.phase = (self.phase + self.phase_step).fract();
        let d = ((self.delay + self.depth * lfo) * self.sample_rate).clamp(1.0, (len - 2) as f32);
        let mut rp = self.w as f32 - d;
        if rp < 0.0 {
            rp += len as f32;
        }
        let i0 = rp as usize % len;
        let frac = rp.fract();
        let y = self.buf[i0] * (1.0 - frac) + self.buf[(i0 + 1) % len] * frac;
        self.buf[self.w] = x + self.feedback * y;
        self.w = wrap_next(self.w, len);
        mix(x, 0.5 * (x + y), self.mix)
    }
}
