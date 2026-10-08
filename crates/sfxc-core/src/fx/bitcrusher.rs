use super::mix;

pub struct Bitcrusher {
    levels: f32,
    step: f32,
    mix: f32,
    acc: f32,
    held: f32,
}

impl Bitcrusher {
    pub fn new(bits: f32, downsample: f32, mix: f32) -> Self {
        Self { levels: 2f32.powf(bits - 1.0), step: 1.0 / downsample.max(1.0), mix, acc: 1.0, held: 0.0 }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        if self.acc >= 1.0 {
            self.acc -= 1.0;
            self.held = (x * self.levels).round() / self.levels;
        }
        self.acc += self.step;
        mix(x, self.held, self.mix)
    }
}
