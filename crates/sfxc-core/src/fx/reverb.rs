use super::{mix, wrap_next};

// Freeverb tunings at 44.1 kHz.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

/// Freeverb-style reverb: pre-delay → 8 parallel damped combs → 4 series all-passes.
pub struct Reverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    pre: Vec<f32>,
    pre_i: usize,
    feedback: f32,
    damp: f32,
    mix: f32,
}

impl Reverb {
    pub fn new(size: f32, decay: f32, damping: f32, predelay: f32, mix: f32, sample_rate: f32) -> Self {
        let rate = sample_rate / 44_100.0;
        let comb_scale = rate * (0.4 + 0.6 * size);
        Self {
            combs: COMBS
                .iter()
                .map(|&n| Comb { buf: vec![0.0; ((n as f32 * comb_scale) as usize).max(1)], i: 0, store: 0.0 })
                .collect(),
            allpasses: ALLPASSES
                .iter()
                .map(|&n| Allpass { buf: vec![0.0; ((n as f32 * rate) as usize).max(1)], i: 0 })
                .collect(),
            pre: vec![0.0; ((predelay * sample_rate) as usize).max(1)],
            pre_i: 0,
            feedback: 0.7 + 0.28 * decay,
            damp: damping * 0.4,
            mix,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let input = self.pre[self.pre_i];
        self.pre[self.pre_i] = x;
        self.pre_i = wrap_next(self.pre_i, self.pre.len());
        let inp = input * 0.015;
        let mut out = 0.0;
        for c in &mut self.combs {
            let y = c.buf[c.i];
            c.store = y * (1.0 - self.damp) + c.store * self.damp;
            c.buf[c.i] = inp + c.store * self.feedback;
            c.i = wrap_next(c.i, c.buf.len());
            out += y;
        }
        for a in &mut self.allpasses {
            let b = a.buf[a.i];
            let y = b - out;
            a.buf[a.i] = out + b * 0.5;
            a.i = wrap_next(a.i, a.buf.len());
            out = y;
        }
        mix(x, out * 3.0, self.mix)
    }
}
