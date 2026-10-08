use super::wrap_next;

pub struct Delay {
    buf: Vec<f32>,
    w: usize,
    feedback: f32,
    damping: f32,
    mix: f32,
    lp: f32,
}

impl Delay {
    pub fn new(time: f32, feedback: f32, damping: f32, mix: f32, sample_rate: f32) -> Self {
        let len = ((time * sample_rate) as usize).max(1);
        Self { buf: vec![0.0; len], w: 0, feedback, damping, mix, lp: 0.0 }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.buf[self.w];
        self.lp = y * (1.0 - self.damping) + self.lp * self.damping;
        self.buf[self.w] = x + self.feedback * self.lp;
        self.w = wrap_next(self.w, self.buf.len());
        x + self.mix * y
    }
}
