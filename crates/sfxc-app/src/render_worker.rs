//! Background rendering. Requests are coalesced: only the newest pending job is rendered.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use sfxc_core::patch::SoundPatch;
use sfxc_core::render::render;

pub struct RenderJob {
    pub generation: u64,
    pub patch: SoundPatch,
    pub sample_rate: u32,
}

pub struct RenderResult {
    pub generation: u64,
    pub sample_rate: u32,
    pub samples: Arc<Vec<f32>>,
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
                    let samples = Arc::new(render(&job.patch, job.sample_rate));
                    let result = RenderResult { generation: job.generation, sample_rate: job.sample_rate, samples };
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
            w.request(RenderJob { generation: g, patch, sample_rate: 22_050 });
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
}
