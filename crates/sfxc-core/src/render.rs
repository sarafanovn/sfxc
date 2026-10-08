//! Offline renderer: patch → mono f32 samples at the requested sample rate.

use crate::env;
use crate::eq::EqFilter;
use crate::fx::FxChain;
use crate::mode::{map_source_to_mode, Limiter, OutputStage};
use crate::osc::Oscillator;
use crate::patch::{Layer, Mode, SoundPatch, MAX_SECONDS};
use crate::pitch;

/// Extra time rendered after the envelopes end when a tail effect is enabled.
pub const TAIL_SECONDS: f32 = 4.0;
/// −80 dB: anything quieter at the end is trimmed.
pub const SILENCE: f32 = 1e-4;

const CANCEL_CHECK: usize = 4096;

pub fn render(patch: &SoundPatch, sample_rate: u32) -> Vec<f32> {
    render_cancellable(patch, sample_rate, &|| false).expect("render without cancellation always finishes")
}

/// Returns `None` as soon as `cancelled` reports true.
pub fn render_cancellable(patch: &SoundPatch, sample_rate: u32, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<Vec<f32>> {
    let mut patch = patch.clone();
    patch.clamp();
    let sr = sample_rate as f32;

    let body = patch.layers.iter().map(|l| env::length(&l.env)).fold(0.0, f32::max);
    let tail = FxChain::has_tail(&patch.master_effects) || patch.layers.iter().any(|l| FxChain::has_tail(&l.effects));
    let secs = (body + if tail { TAIL_SECONDS } else { 0.0 }).min(MAX_SECONDS);
    let len = (secs * sr).ceil() as usize;
    let mut out = vec![0.0f32; len];

    let (first, rest) = patch.layers.split_first().expect("clamp keeps at least one layer");
    let rest_done = std::thread::scope(|scope| {
        let workers: Vec<_> = rest
            .iter()
            .enumerate()
            .map(|(i, layer)| {
                let patch = &patch;
                scope.spawn(move || {
                    let mut buf = vec![0.0f32; len];
                    render_layer(patch, layer, i + 1, sr, &mut buf, cancelled).then_some(buf)
                })
            })
            .collect();
        if !render_layer(&patch, first, 0, sr, &mut out, cancelled) {
            return None;
        }
        let mut bufs = Vec::with_capacity(workers.len());
        for w in workers {
            bufs.push(w.join().expect("layer render panicked")?);
        }
        Some(bufs)
    })?;
    // Summed in layer order so the result does not depend on thread timing.
    for buf in rest_done {
        out.iter_mut().zip(&buf).for_each(|(o, v)| *o += v);
    }

    let mut master = FxChain::new(&patch.master_effects, sr);
    let mut stage = OutputStage::new(patch.mode, sr);
    let mut limiter = Limiter::new(sr);
    for chunk in out.chunks_mut(CANCEL_CHECK) {
        if cancelled() {
            return None;
        }
        for s in chunk {
            let mut y = stage.process(master.process(*s) * patch.master_volume);
            if !y.is_finite() {
                y = 0.0;
            }
            *s = limiter.process(y);
        }
    }
    trim_tail(&mut out, SILENCE);
    Some(out)
}

fn render_layer(patch: &SoundPatch, layer: &Layer, index: usize, sr: f32, out: &mut [f32], cancelled: &(dyn Fn() -> bool + Sync)) -> bool {
    let source = map_source_to_mode(&layer.source, patch.mode);
    let seed = patch.seed.wrapping_add((index as u64).wrapping_mul(0x9E37_79B9));
    let mut osc = Oscillator::new(source, patch.mode, sr, seed);
    let mut eq = EqFilter::new(&layer.eq, sr);
    let mut fx = FxChain::new(&layer.effects, sr);
    let fixed_freq = pitch::constant_freq(&layer.pitch, patch.mode, sr);
    let bit8 = patch.mode == Mode::Bit8;
    for (c, chunk) in out.chunks_mut(CANCEL_CHECK).enumerate() {
        if cancelled() {
            return false;
        }
        for (j, sample) in chunk.iter_mut().enumerate() {
            let t = (c * CANCEL_CHECK + j) as f32 / sr;
            let freq = fixed_freq.unwrap_or_else(|| pitch::freq_at(&layer.pitch, patch.mode, t, sr));
            let s = eq.process(osc.next(freq, t));
            let mut a = env::amp(&layer.env, t);
            if bit8 {
                a = (a * 15.0).round() / 15.0;
            }
            *sample += fx.process(s * a) * layer.gain;
        }
    }
    true
}

/// Drops trailing samples quieter than `threshold`.
pub fn trim_tail(samples: &mut Vec<f32>, threshold: f32) {
    let end = samples.iter().rposition(|s| s.abs() >= threshold).map_or(0, |i| i + 1);
    samples.truncate(end);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{Effect, EffectKind, Equalizer, Mode, NoiseKind, Source, MAX_SECONDS};
    use proptest::prelude::*;

    #[test]
    fn default_patch_renders_expected_length() {
        let s = render(&SoundPatch::default(), 48_000);
        let secs = s.len() as f32 / 48_000.0;
        assert!((0.3..=0.41).contains(&secs), "{secs}");
    }

    #[test]
    fn deterministic_in_every_mode() {
        for mode in Mode::ALL {
            let mut p = SoundPatch { mode, ..Default::default() };
            p.layers[0].source = Source::Noise { kind: NoiseKind::LfsrLong };
            p.master_effects.push(Effect { id: 1, enabled: true, kind: EffectKind::all_defaults()[5] });
            assert_eq!(render(&p, 44_100), render(&p, 44_100));
        }
    }

    #[test]
    fn seed_changes_white_noise() {
        let mut p = SoundPatch::default();
        p.layers[0].source = Source::Noise { kind: NoiseKind::White };
        let a = render(&p, 44_100);
        p.seed = 2;
        assert_ne!(a, render(&p, 44_100));
    }

    #[test]
    fn disallowed_source_is_mapped_at_render_time() {
        let mut saw = SoundPatch { mode: Mode::Bit8, ..Default::default() };
        saw.layers[0].source = Source::Saw;
        let mut pulse = saw.clone();
        pulse.layers[0].source = Source::Pulse { duty: 0.25 };
        assert_eq!(render(&saw, 44_100), render(&pulse, 44_100));
    }

    #[test]
    fn length_is_capped() {
        let mut p = SoundPatch::default();
        let e = &mut p.layers[0].env;
        e.attack = 5.0;
        e.decay = 5.0;
        e.sustain_time = 5.0;
        e.release = 5.0;
        p.master_effects.push(Effect { id: 1, enabled: true, kind: EffectKind::all_defaults()[5] });
        let s = render(&p, 22_050);
        assert!(s.len() <= (MAX_SECONDS * 22_050.0).ceil() as usize);
    }

    #[test]
    fn bit8_render_is_on_8bit_grid() {
        let mut p = SoundPatch { mode: Mode::Bit8, master_volume: 0.5, ..Default::default() };
        p.layers[0].source = Source::Pulse { duty: 0.5 };
        for v in render(&p, 44_100) {
            assert!(((v * 127.0) - (v * 127.0).round()).abs() < 1e-3, "{v}");
        }
    }

    #[test]
    fn cancelled_render_returns_none() {
        assert_eq!(render_cancellable(&SoundPatch::default(), 48_000, &|| true), None);
    }

    #[test]
    fn two_layers_equal_one_layer_at_double_gain() {
        let mut one = SoundPatch::default();
        one.layers[0].gain = 1.0;
        let mut two = one.clone();
        two.layers[0].gain = 0.5;
        two.layers.push(two.layers[0].clone());
        assert_eq!(render(&one, 22_050), render(&two, 22_050));
    }

    #[test]
    fn trim_tail_removes_quiet_end() {
        let mut v = vec![0.5, 0.2, 1e-6, 0.0];
        trim_tail(&mut v, 1e-4);
        assert_eq!(v, vec![0.5, 0.2]);
        let mut silent = vec![0.0; 10];
        trim_tail(&mut silent, 1e-4);
        assert!(silent.is_empty());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]
        #[test]
        fn any_patch_is_finite_and_bounded(
            base in 20.0f32..5000.0,
            slide in -8.0f32..8.0,
            gain_db in -12.0f32..12.0,
            mode_i in 0usize..3,
            src_i in 0usize..6,
            fx_on: bool,
            seed: u64,
        ) {
            let mut p = SoundPatch { seed, mode: Mode::ALL[mode_i], master_volume: 1.0, ..Default::default() };
            let l = &mut p.layers[0];
            l.source = Source::default_of_kind(Source::KIND_NAMES[src_i]);
            l.pitch.base_freq = base;
            l.pitch.slide = slide;
            l.eq = Equalizer { enabled: true, gains: [gain_db; 6] };
            l.gain = 2.0;
            if fx_on {
                p.master_effects = EffectKind::all_defaults()
                    .into_iter()
                    .enumerate()
                    .map(|(i, kind)| Effect { id: i as u64, enabled: true, kind })
                    .collect();
            }
            let s = render(&p, 22_050);
            prop_assert!(s.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        }
    }
}
