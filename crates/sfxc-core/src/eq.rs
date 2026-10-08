//! Six-band graphic equalizer: a low shelf, four peaking bands and a high shelf (RBJ biquads).

use std::f32::consts::{PI, SQRT_2};

use serde::{Deserialize, Serialize};

pub const BANDS: usize = 6;
/// Centre (or corner) frequency of each band in Hz. The first band is a low shelf, the last a high shelf.
pub const FREQS: [f32; BANDS] = [60.0, 150.0, 400.0, 1000.0, 2400.0, 15_000.0];
/// Largest boost or cut of a band, in dB.
pub const RANGE_DB: f32 = 12.0;
/// Width of the peaking bands.
const Q: f32 = 1.0;

/// Per-layer equalizer settings. Flat gains leave the sound untouched.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Equalizer {
    pub enabled: bool,
    /// dB per band, `-RANGE_DB..=RANGE_DB`.
    pub gains: [f32; BANDS],
}

impl Default for Equalizer {
    fn default() -> Self {
        Self { enabled: true, gains: [0.0; BANDS] }
    }
}

/// Gain in dB for a band at `x` octaves "inside" the cut, easing in from 0 to the full range.
fn ramp(x: f32, width_octaves: f32) -> f32 {
    -RANGE_DB * (x / width_octaves + 0.2).clamp(0.0, 1.0)
}

impl Equalizer {
    /// A curve that roughly imitates a low-pass at `cutoff` Hz.
    pub fn lowpass(cutoff: f32) -> Self {
        let cutoff = cutoff.max(20.0);
        Self { enabled: true, gains: FREQS.map(|f| ramp((f / cutoff).log2(), 1.5)) }
    }

    /// A curve that roughly imitates a high-pass at `cutoff` Hz.
    pub fn highpass(cutoff: f32) -> Self {
        let cutoff = cutoff.max(20.0);
        Self { enabled: true, gains: FREQS.map(|f| ramp((cutoff / f).log2(), 1.5)) }
    }

    /// A curve that roughly imitates a band-pass around `centre` Hz.
    pub fn bandpass(centre: f32) -> Self {
        let centre = centre.max(20.0);
        Self { enabled: true, gains: FREQS.map(|f| -RANGE_DB * ((f / centre).log2().abs() / 2.0).clamp(0.0, 1.0)) }
    }

    pub fn clamp(&mut self) {
        for g in &mut self.gains {
            *g = if g.is_finite() { g.clamp(-RANGE_DB, RANGE_DB) } else { 0.0 };
        }
    }
}

/// Ready-made curves, in dB per band. The first one is always flat.
pub const PRESETS: [(&str, [f32; BANDS]); 9] = [
    ("Flat", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Classic", [2.0, 1.5, -1.0, -0.5, -0.5, 2.0]),
    ("Bass boost", [6.0, 4.0, 1.0, 0.0, 0.0, 0.0]),
    ("Treble boost", [0.0, 0.0, 0.0, 1.0, 4.0, 6.0]),
    ("Warm", [3.0, 2.0, 0.0, -1.0, -3.0, -6.0]),
    ("Bright", [-2.0, -1.0, 0.0, 2.0, 4.0, 5.0]),
    ("Telephone", [-12.0, -8.0, 3.0, 5.0, 0.0, -12.0]),
    ("Muffled", [0.0, 0.0, -3.0, -6.0, -9.0, -12.0]),
    ("Thin", [-12.0, -8.0, -3.0, 0.0, 1.0, 2.0]),
];

/// Name of the preset whose curve equals `gains`, if any.
pub fn preset_name(gains: &[f32; BANDS]) -> Option<&'static str> {
    PRESETS.iter().find(|(_, g)| g.iter().zip(gains).all(|(a, b)| (a - b).abs() < 0.05)).map(|(n, _)| *n)
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

#[derive(Clone, Copy)]
enum Shape {
    LowShelf,
    Peak,
    HighShelf,
}

impl Biquad {
    /// RBJ audio-EQ-cookbook coefficients, normalised by `a0`.
    fn new(shape: Shape, freq: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq.min(sample_rate * 0.45) / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::Peak => {
                let alpha = sin / (2.0 * Q);
                (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a)
            }
            Shape::LowShelf | Shape::HighShelf => {
                // Shelf slope 1: alpha = sin(w0)/2 * sqrt(2).
                let beta = 2.0 * a.sqrt() * (sin / 2.0 * SQRT_2);
                if matches!(shape, Shape::LowShelf) {
                    (
                        a * ((a + 1.0) - (a - 1.0) * cos + beta),
                        2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                        a * ((a + 1.0) - (a - 1.0) * cos - beta),
                        (a + 1.0) + (a - 1.0) * cos + beta,
                        -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                        (a + 1.0) + (a - 1.0) * cos - beta,
                    )
                } else {
                    (
                        a * ((a + 1.0) + (a - 1.0) * cos + beta),
                        -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                        a * ((a + 1.0) + (a - 1.0) * cos - beta),
                        (a + 1.0) - (a - 1.0) * cos + beta,
                        2.0 * ((a - 1.0) - (a + 1.0) * cos),
                        (a + 1.0) - (a - 1.0) * cos - beta,
                    )
                }
            }
        };
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, z1: 0.0, z2: 0.0 }
    }

    /// Transposed direct form II.
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// The running filter for one layer. Bands set to (almost) 0 dB are skipped, so a flat curve is free.
pub struct EqFilter {
    stages: Vec<Biquad>,
}

impl EqFilter {
    pub fn new(eq: &Equalizer, sample_rate: f32) -> Self {
        let mut stages = Vec::new();
        if eq.enabled {
            for (i, (&freq, &gain)) in FREQS.iter().zip(&eq.gains).enumerate() {
                if gain.abs() < 0.01 {
                    continue;
                }
                let shape = match i {
                    0 => Shape::LowShelf,
                    i if i == BANDS - 1 => Shape::HighShelf,
                    _ => Shape::Peak,
                };
                stages.push(Biquad::new(shape, freq, gain.clamp(-RANGE_DB, RANGE_DB), sample_rate));
            }
        }
        Self { stages }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        self.stages.iter_mut().fold(x, |s, b| b.process(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn sine(freq: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (TAU * freq * i as f32 / SR).sin()).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// RMS gain of the equalizer for a sine, measured after the filter has settled.
    fn gain_at(eq: &Equalizer, freq: f32) -> f32 {
        let input = sine(freq, 48_000);
        let mut f = EqFilter::new(eq, SR);
        let out: Vec<f32> = input.iter().map(|&x| f.process(x)).collect();
        rms(&out[24_000..]) / rms(&input[24_000..])
    }

    fn with(gains: [f32; BANDS]) -> Equalizer {
        Equalizer { enabled: true, gains }
    }

    #[test]
    fn flat_is_a_bit_exact_bypass() {
        let x = sine(1234.0, 2000);
        let mut f = EqFilter::new(&Equalizer::default(), SR);
        assert!(x.iter().all(|&s| f.process(s) == s));
    }

    #[test]
    fn disabled_ignores_the_gains() {
        let x = sine(1000.0, 2000);
        let mut f = EqFilter::new(&Equalizer { enabled: false, gains: [12.0; BANDS] }, SR);
        assert!(x.iter().all(|&s| f.process(s) == s));
    }

    #[test]
    fn a_peaking_band_boosts_its_centre_by_the_set_amount() {
        let g = gain_at(&with([0.0, 0.0, 0.0, 12.0, 0.0, 0.0]), 1000.0);
        assert!((g - 3.98).abs() < 0.2, "+12 dB should be about x3.98, got {g}");
        let cut = gain_at(&with([0.0, 0.0, 0.0, -12.0, 0.0, 0.0]), 1000.0);
        assert!((cut - 0.251).abs() < 0.02, "-12 dB should be about x0.25, got {cut}");
    }

    #[test]
    fn a_band_leaves_far_away_frequencies_alone() {
        let g = gain_at(&with([0.0, 0.0, 0.0, 12.0, 0.0, 0.0]), 80.0);
        assert!((g - 1.0).abs() < 0.05, "80 Hz should be untouched, got {g}");
    }

    #[test]
    fn outer_bands_are_shelves() {
        let low = gain_at(&with([-12.0, 0.0, 0.0, 0.0, 0.0, 0.0]), 20.0);
        assert!((low - 0.251).abs() < 0.04, "everything below the low shelf is cut, got {low}");
        let mid = gain_at(&with([-12.0, 0.0, 0.0, 0.0, 0.0, 0.0]), 5000.0);
        assert!((mid - 1.0).abs() < 0.05, "the shelf leaves the mids alone, got {mid}");
        let high = gain_at(&with([0.0, 0.0, 0.0, 0.0, 0.0, 12.0]), 20_000.0);
        assert!(high > 3.0, "everything above the high shelf is boosted, got {high}");
    }

    #[test]
    fn noise_through_a_maxed_curve_stays_finite_and_bounded() {
        let mut rng = crate::rng::Rng::new(7);
        let mut f = EqFilter::new(&with([12.0; BANDS]), SR);
        for _ in 0..200_000 {
            let y = f.process(rng.bipolar());
            assert!(y.is_finite() && y.abs() < 200.0);
        }
    }

    #[test]
    fn low_sample_rates_keep_bands_below_nyquist() {
        let mut f = EqFilter::new(&with([12.0; BANDS]), 22_050.0);
        let y: Vec<f32> = (0..5000).map(|i| f.process((i as f32 * 0.3).sin())).collect();
        assert!(y.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn lowpass_curve_cuts_the_top_and_keeps_the_bottom() {
        let g = Equalizer::lowpass(2000.0).gains;
        assert!(g[0].abs() < 0.5 && g[1].abs() < 0.5, "{g:?}");
        assert!(g[5] < -10.0, "{g:?}");
        assert!(g.windows(2).all(|w| w[1] <= w[0] + 1e-6), "monotone: {g:?}");
    }

    #[test]
    fn highpass_curve_mirrors_it() {
        let g = Equalizer::highpass(500.0).gains;
        assert!(g[0] < -10.0, "{g:?}");
        assert!(g[5].abs() < 0.5, "{g:?}");
        assert!(g.windows(2).all(|w| w[1] >= w[0] - 1e-6), "monotone: {g:?}");
    }

    #[test]
    fn bandpass_curve_cuts_both_ends() {
        let g = Equalizer::bandpass(1000.0).gains;
        assert!(g[0] < -6.0 && g[5] < -6.0 && g[3] > -1.0, "{g:?}");
    }

    #[test]
    fn presets_are_in_range_start_flat_and_have_unique_names() {
        assert_eq!(PRESETS[0].0, "Flat");
        assert_eq!(PRESETS[0].1, [0.0; BANDS]);
        for (name, gains) in PRESETS {
            assert!(gains.iter().all(|g| g.abs() <= RANGE_DB), "{name}");
        }
        let mut names: Vec<_> = PRESETS.iter().map(|p| p.0).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PRESETS.len());
    }

    #[test]
    fn matching_preset_is_found_by_gains() {
        assert_eq!(preset_name(&PRESETS[2].1), Some(PRESETS[2].0));
        assert_eq!(preset_name(&[0.3; BANDS]), None);
    }
}
