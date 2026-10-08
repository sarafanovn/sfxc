//! Numbers that describe rendered audio, for tools that cannot listen to it.

use std::f32::consts::PI;

use serde::Serialize;

const FRAME: usize = 1024;
/// Caps the plain DFT work for long sounds; frames are spread evenly over the sound.
const MAX_FRAMES: usize = 200;
const ENVELOPE_SLICES: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Analysis {
    pub duration_s: f32,
    pub peak: f32,
    pub rms: f32,
    /// Energy-weighted mean spectral centroid: higher is brighter. 0 for silence.
    pub spectral_centroid_hz: f32,
    /// RMS of 10 equal slices, scaled so the loudest slice is 1.
    pub envelope: Vec<f32>,
}

pub fn analyze(samples: &[f32], sample_rate: u32) -> Analysis {
    Analysis {
        duration_s: samples.len() as f32 / sample_rate as f32,
        peak: samples.iter().fold(0.0f32, |m, v| m.max(v.abs())),
        rms: rms(samples),
        spectral_centroid_hz: centroid(samples, sample_rate),
        envelope: envelope(samples),
    }
}

fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
}

fn envelope(samples: &[f32]) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.0; ENVELOPE_SLICES];
    }
    let n = samples.len();
    let slices: Vec<f32> = (0..ENVELOPE_SLICES)
        .map(|i| {
            let a = i * n / ENVELOPE_SLICES;
            let b = ((i + 1) * n / ENVELOPE_SLICES).max(a + 1).min(n);
            rms(&samples[a..b])
        })
        .collect();
    let top = slices.iter().copied().fold(0.0, f32::max);
    if top > 0.0 { slices.iter().map(|v| v / top).collect() } else { slices }
}

fn centroid(samples: &[f32], sample_rate: u32) -> f32 {
    let window: Vec<f32> = (0..FRAME).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FRAME as f32).cos()).collect();
    let (cos, sin): (Vec<f32>, Vec<f32>) = (0..FRAME)
        .map(|j| {
            let a = 2.0 * PI * j as f32 / FRAME as f32;
            (a.cos(), a.sin())
        })
        .unzip();
    let whole = samples.len().saturating_sub(FRAME) / FRAME + 1;
    let used = whole.min(MAX_FRAMES);
    let (mut weighted, mut energy) = (0.0f64, 0.0f64);
    let mut frame = vec![0.0f32; FRAME];
    for k in 0..used {
        let start = k * whole / used * FRAME;
        for (i, x) in frame.iter_mut().enumerate() {
            *x = samples.get(start + i).copied().unwrap_or(0.0) * window[i];
        }
        let e: f32 = frame.iter().map(|x| x * x).sum();
        if e > 0.0 {
            weighted += frame_centroid(&frame, sample_rate, &cos, &sin) as f64 * e as f64;
            energy += e as f64;
        }
    }
    if energy > 0.0 { (weighted / energy) as f32 } else { 0.0 }
}

fn frame_centroid(frame: &[f32], sample_rate: u32, cos: &[f32], sin: &[f32]) -> f32 {
    let n = frame.len();
    let (mut num, mut den) = (0.0f32, 0.0f32);
    for k in 1..n / 2 {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (i, x) in frame.iter().enumerate() {
            let j = (k * i) % n;
            re += x * cos[j];
            im -= x * sin[j];
        }
        let mag = (re * re + im * im).sqrt();
        num += mag * k as f32 * sample_rate as f32 / n as f32;
        den += mag;
    }
    if den > 0.0 { num / den } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 44_100;

    fn sine(freq: f32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(secs * SR as f32) as usize).map(|i| amp * (2.0 * PI * freq * i as f32 / SR as f32).sin()).collect()
    }

    #[test]
    fn silence_and_empty_input_are_all_zero() {
        for s in [vec![], vec![0.0; 5000]] {
            let a = analyze(&s, SR);
            assert_eq!((a.peak, a.rms, a.spectral_centroid_hz), (0.0, 0.0, 0.0));
            assert_eq!(a.envelope, vec![0.0; 10]);
        }
    }

    #[test]
    fn a_sine_reports_its_level_and_frequency() {
        let a = analyze(&sine(1000.0, 1.0, 0.5), SR);
        assert!((a.duration_s - 1.0).abs() < 1e-3);
        assert!((a.peak - 0.5).abs() < 1e-3);
        assert!((a.rms - 0.5 / 2f32.sqrt()).abs() < 1e-2);
        assert!((a.spectral_centroid_hz - 1000.0).abs() < 100.0, "{}", a.spectral_centroid_hz);
    }

    #[test]
    fn higher_tones_are_brighter() {
        let low = analyze(&sine(200.0, 0.5, 0.5), SR).spectral_centroid_hz;
        let high = analyze(&sine(4000.0, 0.5, 0.5), SR).spectral_centroid_hz;
        assert!(high > low * 5.0, "{low} {high}");
    }

    #[test]
    fn a_decay_shows_in_the_envelope() {
        let s: Vec<f32> = sine(440.0, 1.0, 1.0).iter().enumerate().map(|(i, v)| v * (-(i as f32) / 6000.0).exp()).collect();
        let e = analyze(&s, SR).envelope;
        assert_eq!(e.len(), 10);
        assert_eq!(e[0], 1.0);
        assert!(e[9] < 0.2, "{e:?}");
    }

    #[test]
    fn a_long_sound_is_handled() {
        let a = analyze(&sine(300.0, 10.0, 0.3), SR);
        assert!((a.duration_s - 10.0).abs() < 1e-2);
    }
}
