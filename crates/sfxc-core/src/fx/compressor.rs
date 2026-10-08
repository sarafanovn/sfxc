/// Feed-forward peak compressor with dB-domain attack/release smoothing.
pub struct Compressor {
    threshold_db: f32,
    ratio: f32,
    att: f32,
    rel: f32,
    makeup_db: f32,
    env_db: f32,
}

impl Compressor {
    pub fn new(threshold_db: f32, ratio: f32, attack: f32, release: f32, makeup_db: f32, sample_rate: f32) -> Self {
        Self {
            threshold_db,
            ratio,
            att: (-1.0 / (attack * sample_rate)).exp(),
            rel: (-1.0 / (release * sample_rate)).exp(),
            makeup_db,
            env_db: -120.0,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let level_db = 20.0 * x.abs().max(1e-6).log10();
        let coef = if level_db > self.env_db { self.att } else { self.rel };
        self.env_db = level_db + coef * (self.env_db - level_db);
        let over = self.env_db - self.threshold_db;
        let reduction = if over > 0.0 { over * (1.0 - 1.0 / self.ratio) } else { 0.0 };
        x * 10f32.powf((self.makeup_db - reduction) / 20.0)
    }
}
