//! Background rendering. Requests are coalesced: only the newest pending job is rendered.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use sfxc_core::patch::SoundPatch;
use sfxc_core::render::render;

pub struct RenderJob {
    pub generation: u64,
    pub patch: SoundPatch,
    /// Rate the sound is rendered at: the one chosen for the sound, so the preview sounds like the export.
    pub sample_rate: u32,
    /// Rate of the output device; the render is resampled to it.
    pub play_rate: u32,
}

pub struct RenderResult {
    pub generation: u64,
    /// Rate of `samples` (the device rate).
    pub sample_rate: u32,
    /// Rate the sound was rendered at before resampling.
    pub render_rate: u32,
    pub samples: Arc<Vec<f32>>,
}

/// Linear-interpolation resampling. Good enough for previewing: the band limit of the rendered rate survives.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let step = from as f64 / to as f64;
    let n = ((samples.len() as f64) / step).round() as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * step;
            let j = x as usize;
            let t = (x - j as f64) as f32;
            let a = samples[j.min(samples.len() - 1)];
            let b = samples[(j + 1).min(samples.len() - 1)];
            a + (b - a) * t
        })
        .collect()
}

pub struct RenderWorker {
    jobs: Sender<RenderJob>,
    results: Receiver<RenderResult>,
}

impl RenderWorker {
    pub fn spawn(notify: impl Fn() + Send + 'static) -> Self {
        let (jobs, job_rx) = mpsc::channel::<RenderJob>();
        let (result_tx, results) = mpsc::channel();
        thread::Builder::new()
            .name("sfxc-render".into())
            .spawn(move || {
                while let Ok(mut job) = job_rx.recv() {
                    while let Ok(newer) = job_rx.try_recv() {
                        job = newer;
                    }
                    let samples = Arc::new(resample(&render(&job.patch, job.sample_rate), job.sample_rate, job.play_rate));
                    let result =
                        RenderResult { generation: job.generation, sample_rate: job.play_rate, render_rate: job.sample_rate, samples };
                    if result_tx.send(result).is_err() {
                        break;
                    }
                    notify();
                }
            })
            .expect("spawn render thread");
        Self { jobs, results }
    }

    pub fn request(&self, job: RenderJob) {
        let _ = self.jobs.send(job);
    }

    pub fn latest(&self) -> Option<RenderResult> {
        self.results.try_iter().last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn newest_request_wins() {
        let w = RenderWorker::spawn(|| {});
        for g in 1..=5 {
            let mut patch = SoundPatch::default();
            patch.layers[0].pitch.base_freq = 100.0 * g as f32;
            w.request(RenderJob { generation: g, patch, sample_rate: 22_050, play_rate: 22_050 });
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut last = None;
        while Instant::now() < deadline {
            if let Some(r) = w.latest() {
                let done = r.generation == 5;
                last = Some(r);
                if done {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let r = last.expect("worker produced a result");
        assert_eq!(r.generation, 5);
        assert_eq!(r.sample_rate, 22_050);
        assert!(!r.samples.is_empty());
    }

    #[test]
    fn resample_keeps_duration_and_shape() {
        let src: Vec<f32> = (0..22_050).map(|i| (i as f32 / 22_050.0 * std::f32::consts::TAU * 10.0).sin()).collect();
        let out = resample(&src, 22_050, 48_000);
        assert_eq!(out.len(), 48_000);
        // A quarter of the way through each 1/10 s cycle the sine peaks.
        assert!((out[1_200] - 1.0).abs() < 0.01, "got {}", out[1_200]);
        assert_eq!(resample(&src, 44_100, 44_100).len(), src.len());
        assert!(resample(&[], 22_050, 48_000).is_empty());
    }
}
