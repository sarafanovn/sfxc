//! Plays rendered buffers on the default output device via CoreAudio (cpal).
//! A missing or lost device never blocks editing; we retry every few seconds.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const RETRY_EVERY: Duration = Duration::from_secs(3);

/// Playback volume (amplitude) when nothing is stored yet.
pub const DEFAULT_VOLUME: f32 = 0.8;

/// Slider position (0..1) → amplitude. Squared so the lower half of the slider stays usable.
pub fn volume_from_slider(pos: f32) -> f32 {
    pos.clamp(0.0, 1.0).powi(2)
}

pub fn slider_from_volume(amp: f32) -> f32 {
    amp.clamp(0.0, 1.0).sqrt()
}

/// Parses a stored volume; garbage and NaN are rejected, out-of-range values clamped.
pub fn parse_volume(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0))
}

enum Command {
    Play(Arc<Vec<f32>>),
    Stop,
}

/// Playhead shared with the audio thread. `len` is 0 when nothing is queued.
#[derive(Default)]
struct Position {
    pos: AtomicUsize,
    len: AtomicUsize,
}

pub struct Player {
    stream: Option<cpal::Stream>,
    commands: Option<Sender<Command>>,
    position: Arc<Position>,
    /// Buffers handed to the audio thread, kept here so it never frees memory itself.
    sent: Vec<Arc<Vec<f32>>>,
    failed: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    sample_rate: u32,
    error: Option<String>,
    last_attempt: Instant,
}

impl Player {
    pub fn new() -> Self {
        let mut p = Self {
            stream: None,
            commands: None,
            position: Arc::default(),
            sent: Vec::new(),
            failed: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(AtomicU32::new(DEFAULT_VOLUME.to_bits())),
            sample_rate: 48_000,
            error: None,
            last_attempt: Instant::now(),
        };
        p.connect();
        p
    }

    fn connect(&mut self) {
        self.last_attempt = Instant::now();
        self.position.len.store(0, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        match self.build(rx) {
            Ok((stream, sr)) => {
                self.stream = Some(stream);
                self.commands = Some(tx);
                self.sample_rate = sr;
                self.error = None;
            }
            Err(e) => {
                self.stream = None;
                self.commands = None;
                self.error = Some(format!("Audio output unavailable: {e:#}"));
            }
        }
    }

    fn build(&self, commands: Receiver<Command>) -> Result<(cpal::Stream, u32)> {
        let device = cpal::default_host().default_output_device().context("no output device")?;
        let supported = device.default_output_config()?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            bail!("unsupported device sample format {:?}", supported.sample_format());
        }
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate;
        let position = self.position.clone();
        let mut current: Option<Arc<Vec<f32>>> = None;
        let mut pos = 0;
        let failed = self.failed.clone();
        let volume = self.volume.clone();
        failed.store(false, Ordering::SeqCst);
        let stream = device.build_output_stream::<f32, _, _>(
            config,
            move |out: &mut [f32], _| {
                for cmd in commands.try_iter() {
                    current = match cmd {
                        Command::Play(samples) => Some(samples),
                        Command::Stop => None,
                    };
                    pos = 0;
                }
                let gain = f32::from_bits(volume.load(Ordering::Relaxed));
                let samples = current.as_deref().map_or(&[][..], Vec::as_slice);
                for frame in out.chunks_mut(channels) {
                    let v = samples.get(pos).copied().unwrap_or(0.0);
                    pos = (pos + 1).min(samples.len());
                    frame.fill(v * gain);
                }
                position.pos.store(pos, Ordering::Relaxed);
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

    /// Output gain applied while playing; takes effect immediately, also mid-sound.
    pub fn set_volume(&self, amp: f32) {
        self.volume.store(amp.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn is_available(&self) -> bool {
        self.stream.is_some()
    }

    pub fn play(&mut self, samples: Arc<Vec<f32>>) {
        self.position.pos.store(0, Ordering::Relaxed);
        self.position.len.store(samples.len(), Ordering::Relaxed);
        self.sent.push(samples.clone());
        self.send(Command::Play(samples));
    }

    /// Stops playback immediately; the next `play` starts from the beginning.
    pub fn stop(&mut self) {
        self.position.pos.store(0, Ordering::Relaxed);
        self.position.len.store(0, Ordering::Relaxed);
        self.send(Command::Stop);
    }

    fn send(&mut self, cmd: Command) {
        if let Some(tx) = &self.commands
            && tx.send(cmd).is_err()
        {
            self.commands = None;
        }
    }

    fn playhead(&self) -> (usize, usize) {
        (self.position.pos.load(Ordering::Relaxed), self.position.len.load(Ordering::Relaxed))
    }

    /// True while a buffer is queued and not yet played to the end.
    pub fn is_playing(&self) -> bool {
        let (pos, len) = self.playhead();
        pos < len
    }

    /// Fraction of the current buffer already played, while something is playing.
    pub fn progress(&self) -> Option<f32> {
        let (pos, len) = self.playhead();
        (self.stream.is_some() && pos < len).then(|| pos as f32 / len as f32)
    }

    /// Call once per UI frame.
    pub fn maintain(&mut self) {
        self.sent.retain(|b| Arc::strong_count(b) > 1);
        if self.failed.swap(false, Ordering::SeqCst) {
            self.stream = None;
            self.error = Some("Audio device lost, reconnecting…".into());
        }
        if self.stream.is_none() && self.last_attempt.elapsed() >= RETRY_EVERY {
            self.connect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_curve_round_trips() {
        for pos in [0.0, 0.25, 0.5, 0.9, 1.0] {
            assert!((slider_from_volume(volume_from_slider(pos)) - pos).abs() < 1e-5);
        }
        assert_eq!(volume_from_slider(0.5), 0.25);
        assert_eq!(volume_from_slider(2.0), 1.0);
    }

    #[test]
    fn stop_ends_playback() {
        let mut p = Player::new();
        assert!(!p.is_playing());
        // Long enough that a real device cannot finish it during the test.
        p.play(Arc::new(vec![0.1; 4_000_000]));
        assert!(p.is_playing());
        p.stop();
        assert!(!p.is_playing());
    }

    #[test]
    fn parse_volume_rejects_garbage_and_clamps() {
        assert_eq!(parse_volume("0.5"), Some(0.5));
        assert_eq!(parse_volume("5"), Some(1.0));
        assert_eq!(parse_volume("-1"), Some(0.0));
        assert_eq!(parse_volume("abc"), None);
        assert_eq!(parse_volume("NaN"), None);
    }
}
