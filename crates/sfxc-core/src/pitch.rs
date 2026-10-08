//! Pitch over time: base frequency, slide, vibrato, arpeggio, NES quantization.

use std::f32::consts::TAU;

use crate::patch::{Mode, Pitch};

pub const NES_CPU_HZ: f32 = 1_789_773.0;

/// Snaps to the nearest frequency the NES pulse channel can play (11-bit period register).
pub fn nes_quantize(freq: f32) -> f32 {
    let period = (NES_CPU_HZ / (16.0 * freq) - 1.0).round().clamp(0.0, 2047.0);
    NES_CPU_HZ / (16.0 * (period + 1.0))
}

pub fn freq_at(p: &Pitch, mode: Mode, t: f32, sample_rate: f32) -> f32 {
    let mut semis = 12.0 * (p.slide * t + 0.5 * p.delta_slide * t * t);
    if p.vibrato_depth > 0.0 {
        semis += p.vibrato_depth * (TAU * p.vibrato_rate * t).sin();
    }
    if p.arp_enabled && !p.arp_steps.is_empty() {
        let idx = (t / p.arp_speed.max(0.001)) as usize % p.arp_steps.len();
        semis += p.arp_steps[idx] as f32;
    }
    let nyquist_guard = sample_rate * 0.45;
    let f = (p.base_freq * 2f32.powf(semis / 12.0)).clamp(10.0, nyquist_guard);
    if mode == Mode::Bit8 { nes_quantize(f).min(nyquist_guard) } else { f }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SR: f32 = 48_000.0;

    #[test]
    fn constant_without_modulation() {
        let p = Pitch::default();
        assert!((freq_at(&p, Mode::Modern, 0.7, SR) - 440.0).abs() < 1e-3);
    }

    #[test]
    fn slide_one_octave_per_second() {
        let p = Pitch { slide: 1.0, ..Default::default() };
        assert!((freq_at(&p, Mode::Modern, 1.0, SR) - 880.0).abs() < 0.01);
    }

    #[test]
    fn arpeggio_steps() {
        let p = Pitch { arp_steps: vec![0, 12], arp_speed: 0.1, ..Default::default() };
        assert!((freq_at(&p, Mode::Modern, 0.05, SR) - 440.0).abs() < 0.01);
        assert!((freq_at(&p, Mode::Modern, 0.15, SR) - 880.0).abs() < 0.01);
        assert!((freq_at(&p, Mode::Modern, 0.25, SR) - 440.0).abs() < 0.01);
    }

    #[test]
    fn disabled_arpeggio_is_ignored() {
        let p = Pitch { arp_steps: vec![0, 12], arp_speed: 0.1, arp_enabled: false, ..Default::default() };
        assert!((freq_at(&p, Mode::Modern, 0.15, SR) - 440.0).abs() < 0.01);
    }

    #[test]
    fn nes_quantization_matches_period_register() {
        // 1 789 773 / (16 * 254) for period 253.
        assert!((nes_quantize(440.0) - 440.397).abs() < 0.01);
        let p = Pitch::default();
        assert_eq!(freq_at(&p, Mode::Bit8, 0.0, SR), nes_quantize(440.0));
    }

    #[test]
    fn extreme_slide_stays_in_range() {
        let up = Pitch { slide: 8.0, delta_slide: 16.0, ..Default::default() };
        let down = Pitch { slide: -8.0, delta_slide: -16.0, ..Default::default() };
        for mode in Mode::ALL {
            for t in [0.0, 1.0, 10.0] {
                for p in [&up, &down] {
                    let f = freq_at(p, mode, t, SR);
                    assert!(f.is_finite() && (10.0..=SR * 0.45).contains(&f), "{f}");
                }
            }
        }
    }
}
