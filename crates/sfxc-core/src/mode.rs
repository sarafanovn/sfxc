use std::f32::consts::TAU;

use crate::patch::{Mode, NoiseKind, Source};

pub const NES_DUTIES: [f32; 4] = [0.125, 0.25, 0.5, 0.75];

pub fn is_allowed(source: &Source, mode: Mode) -> bool {
    match mode {
        Mode::Bit8 => match source {
            Source::Pulse { duty } => NES_DUTIES.contains(duty),
            Source::Triangle => true,
            Source::Noise { kind } => *kind != NoiseKind::White,
            _ => false,
        },
        Mode::Modern | Mode::Bit16 => true,
    }
}

pub fn map_source_to_mode(source: &Source, mode: Mode) -> Source {
    if mode != Mode::Bit8 {
        return *source;
    }
    match *source {
        Source::Pulse { duty } => {
            let nearest = NES_DUTIES
                .into_iter()
                .min_by(|a, b| (a - duty).abs().total_cmp(&(b - duty).abs()))
                .expect("non-empty");
            Source::Pulse { duty: nearest }
        }
        Source::Saw => Source::Pulse { duty: 0.25 },
        Source::Sine | Source::Triangle => Source::Triangle,
        Source::Noise { kind: NoiseKind::White } => Source::Noise { kind: NoiseKind::LfsrLong },
        noise @ Source::Noise { .. } => noise,
        Source::Fm { .. } => Source::Pulse { duty: 0.5 },
    }
}

pub fn describe_mapping(from: &Source, to: &Source) -> Option<String> {
    (from != to).then(|| format!("8-bit mode changed {} to {}", from.label(), to.label()))
}

pub struct OutputStage {
    mode: Mode,
    hold_step: f32,
    acc: f32,
    held: f32,
    lp: f32,
    lp_coef: f32,
}

impl OutputStage {
    pub fn new(mode: Mode, sample_rate: f32) -> Self {
        let target = match mode {
            Mode::Bit8 => 22_050.0,
            Mode::Bit16 => 32_000.0,
            Mode::Modern => sample_rate,
        };
        Self {
            mode,
            hold_step: (target / sample_rate).min(1.0),
            acc: 1.0,
            held: 0.0,
            lp: 0.0,
            lp_coef: 1.0 - (-TAU * 12_000f32.min(sample_rate * 0.45) / sample_rate).exp(),
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        match self.mode {
            Mode::Modern => x,
            Mode::Bit8 => {
                self.hold(x);
                (self.held.clamp(-1.0, 1.0) * 127.0).round() / 127.0
            }
            Mode::Bit16 => {
                self.hold(x);
                self.lp += self.lp_coef * (self.held - self.lp);
                (self.lp.clamp(-1.0, 1.0) * 32767.0).round() / 32767.0
            }
        }
    }

    fn hold(&mut self, x: f32) {
        if self.acc >= 1.0 {
            self.acc -= 1.0;
            self.held = x;
        }
        self.acc += self.hold_step;
    }
}

pub const LIMIT: f32 = 0.98;

pub struct Limiter {
    env: f32,
    release: f32,
}

impl Limiter {
    pub fn new(sample_rate: f32) -> Self {
        Self { env: 0.0, release: (-1.0 / (0.05 * sample_rate)).exp() }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let a = x.abs();
        self.env = if a > self.env { a } else { a + self.release * (self.env - a) };
        if self.env > LIMIT { x * LIMIT / self.env } else { x }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::NoiseKind;

    #[test]
    fn mapping_to_8bit_yields_allowed_sources() {
        let all = [
            Source::Pulse { duty: 0.3 },
            Source::Saw,
            Source::Triangle,
            Source::Sine,
            Source::Noise { kind: NoiseKind::White },
            Source::Noise { kind: NoiseKind::LfsrShort },
            Source::default_fm(),
        ];
        for s in all {
            let m = map_source_to_mode(&s, Mode::Bit8);
            assert!(is_allowed(&m, Mode::Bit8), "{s:?} -> {m:?}");
            assert_eq!(map_source_to_mode(&s, Mode::Modern), s);
            assert_eq!(map_source_to_mode(&s, Mode::Bit16), s);
        }
        assert_eq!(map_source_to_mode(&Source::Pulse { duty: 0.3 }, Mode::Bit8), Source::Pulse { duty: 0.25 });
        assert_eq!(map_source_to_mode(&Source::Saw, Mode::Bit8), Source::Pulse { duty: 0.25 });
    }

    #[test]
    fn describe_mapping_only_when_changed() {
        assert_eq!(describe_mapping(&Source::Triangle, &Source::Triangle), None);
        let text = describe_mapping(&Source::Saw, &Source::Pulse { duty: 0.25 }).unwrap();
        assert!(text.contains("Saw") && text.contains("25"));
    }

    #[test]
    fn bit8_output_is_8bit_quantized() {
        let mut s = OutputStage::new(Mode::Bit8, 44_100.0);
        for i in 0..1000 {
            let y = s.process((i as f32 * 0.013).sin() * 0.9);
            assert!(((y * 127.0) - (y * 127.0).round()).abs() < 1e-4);
        }
    }

    #[test]
    fn modern_output_is_identity() {
        let mut s = OutputStage::new(Mode::Modern, 48_000.0);
        assert_eq!(s.process(0.123), 0.123);
    }

    #[test]
    fn limiter_never_exceeds_limit() {
        let mut l = Limiter::new(48_000.0);
        for i in 0..10_000 {
            let x = (i as f32 * 0.01).sin() * 5.0;
            assert!(l.process(x).abs() <= LIMIT + 1e-6);
        }
    }
}
