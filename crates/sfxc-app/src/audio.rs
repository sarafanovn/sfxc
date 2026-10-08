//! Plays rendered buffers on the default output device via CoreAudio (cpal).
//! A missing or lost device never blocks editing; we retry every few seconds.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const RETRY_EVERY: Duration = Duration::from_secs(3);

struct Playback {
    samples: Arc<Vec<f32>>,
    pos: usize,
}

pub struct Player {
    stream: Option<cpal::Stream>,
    state: Arc<Mutex<Option<Playback>>>,
    failed: Arc<AtomicBool>,
    sample_rate: u32,
    error: Option<String>,
    last_attempt: Instant,
}

impl Player {
    pub fn new() -> Self {
        let mut p = Self {
            stream: None,
            state: Arc::new(Mutex::new(None)),
            failed: Arc::new(AtomicBool::new(false)),
            sample_rate: 48_000,
            error: None,
            last_attempt: Instant::now(),
        };
        p.connect();
        p
    }

    fn connect(&mut self) {
        self.last_attempt = Instant::now();
        match self.build() {
            Ok((stream, sr)) => {
                self.stream = Some(stream);
                self.sample_rate = sr;
                self.error = None;
            }
            Err(e) => {
                self.stream = None;
                self.error = Some(format!("Audio output unavailable: {e:#}"));
            }
        }
    }

    fn build(&self) -> Result<(cpal::Stream, u32)> {
        let device = cpal::default_host().default_output_device().context("no output device")?;
        let supported = device.default_output_config()?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            bail!("unsupported device sample format {:?}", supported.sample_format());
        }
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate;
        let state = self.state.clone();
        let failed = self.failed.clone();
        failed.store(false, Ordering::SeqCst);
        let stream = device.build_output_stream::<f32, _, _>(
            config,
            move |out: &mut [f32], _| {
                // Never block the audio thread: if the UI holds the lock, output silence.
                let Ok(mut guard) = state.try_lock() else {
                    out.fill(0.0);
                    return;
                };
                for frame in out.chunks_mut(channels) {
                    let v = match guard.as_mut() {
                        Some(pb) if pb.pos < pb.samples.len() => {
                            pb.pos += 1;
                            pb.samples[pb.pos - 1]
                        }
                        _ => 0.0,
                    };
                    frame.fill(v);
                }
            },
            move |err| {
                eprintln!("audio stream error: {err}");
                // Glitches and automatic rerouting leave the stream running; only rebuild when it is dead.
                if !matches!(err.kind(), cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied) {
                    failed.store(true, Ordering::SeqCst);
                }
            },
            None,
        )?;
        stream.play()?;
        Ok((stream, sample_rate))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn is_available(&self) -> bool {
        self.stream.is_some()
    }

    pub fn play(&self, samples: Arc<Vec<f32>>) {
        if let Ok(mut g) = self.state.lock() {
            *g = Some(Playback { samples, pos: 0 });
        }
    }

    /// Fraction of the current buffer already played, while something is playing.
    pub fn progress(&self) -> Option<f32> {
        let g = self.state.try_lock().ok()?;
        let pb = g.as_ref()?;
        (self.stream.is_some() && pb.pos < pb.samples.len()).then(|| pb.pos as f32 / pb.samples.len() as f32)
    }

    /// Call once per UI frame.
    pub fn maintain(&mut self) {
        if self.failed.swap(false, Ordering::SeqCst) {
            self.stream = None;
            self.error = Some("Audio device lost, reconnecting…".into());
        }
        if self.stream.is_none() && self.last_attempt.elapsed() >= RETRY_EVERY {
            self.connect();
        }
    }
}
