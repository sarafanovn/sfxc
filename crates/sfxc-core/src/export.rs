//! Rendering for export and encoding to WAV / OGG Vorbis.

use std::fs::File;
use std::io::{BufWriter, Seek, Write};
use std::num::{NonZeroU32, NonZeroU8};
use std::path::Path;

use anyhow::{bail, Context, Result};
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

use crate::patch::SoundPatch;
use crate::render::{render, trim_tail};

/// −1 dBFS.
pub const NORMALIZE_PEAK: f32 = 0.891_251;
/// −60 dB.
pub const TRIM_THRESHOLD: f32 = 0.001;
/// Fixed Ogg stream serial so exports are reproducible.
const OGG_SERIAL: i32 = 0x5F78_6373;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportFormat {
    Wav { bits: u16 },
    /// Quality 0–10.
    Ogg { quality: f32 },
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Wav { .. } => "wav",
            ExportFormat::Ogg { .. } => "ogg",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub sample_rate: u32,
    pub normalize: bool,
    pub trim: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { format: ExportFormat::Wav { bits: 16 }, sample_rate: 44_100, normalize: true, trim: true }
    }
}

pub fn prepare(patch: &SoundPatch, opts: &ExportOptions) -> Vec<f32> {
    let mut s = render(patch, opts.sample_rate);
    if opts.trim {
        trim_tail(&mut s, TRIM_THRESHOLD);
    }
    if opts.normalize {
        let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        if peak > 0.0 {
            let g = NORMALIZE_PEAK / peak;
            s.iter_mut().for_each(|v| *v *= g);
        }
    }
    s
}

pub fn write_wav<W: Write + Seek>(samples: &[f32], sample_rate: u32, bits: u16, w: W) -> Result<()> {
    if ![8, 16, 24].contains(&bits) {
        bail!("unsupported WAV bit depth {bits}");
    }
    let spec = hound::WavSpec { channels: 1, sample_rate, bits_per_sample: bits, sample_format: hound::SampleFormat::Int };
    let mut wr = hound::WavWriter::new(w, spec)?;
    for &s in samples {
        let s = s.clamp(-1.0, 1.0);
        match bits {
            // hound stores 8-bit as unsigned PCM, as the WAV format requires.
            8 => wr.write_sample((s * 127.0).round() as i8)?,
            16 => wr.write_sample((s * 32_767.0).round() as i16)?,
            _ => wr.write_sample((s * 8_388_607.0).round() as i32)?,
        }
    }
    wr.finalize()?;
    Ok(())
}

pub fn write_ogg<W: Write>(samples: &[f32], sample_rate: u32, quality: f32, w: W) -> Result<()> {
    let rate = NonZeroU32::new(sample_rate).context("sample rate is zero")?;
    let mut builder = VorbisEncoderBuilder::new_with_serial(rate, NonZeroU8::MIN, w, OGG_SERIAL);
    builder.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
        target_quality: (quality / 10.0).clamp(0.0, 1.0),
    });
    let mut enc = builder.build()?;
    for chunk in samples.chunks(4096) {
        enc.encode_audio_block([chunk])?;
    }
    enc.finish()?;
    Ok(())
}

/// Renders and writes atomically: encodes to `<path>.part`, then renames.
pub fn export_to_path(patch: &SoundPatch, opts: &ExportOptions, path: &Path) -> Result<()> {
    let samples = prepare(patch, opts);
    let tmp = path.with_extension("part");
    let result = (|| -> Result<()> {
        let file = BufWriter::new(File::create(&tmp).with_context(|| format!("cannot create {}", tmp.display()))?);
        match opts.format {
            ExportFormat::Wav { bits } => write_wav(&samples, opts.sample_rate, bits, file)?,
            ExportFormat::Ogg { quality } => write_ogg(&samples, opts.sample_rate, quality, file)?,
        }
        std::fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn tone() -> Vec<f32> {
        (0..22_050).map(|i| 0.5 * (i as f32 * 0.05).sin()).collect()
    }

    #[test]
    fn wav_round_trip_all_depths() {
        for bits in [8u16, 16, 24] {
            let mut buf = Cursor::new(Vec::new());
            write_wav(&tone(), 22_050, bits, &mut buf).unwrap();
            buf.set_position(0);
            let r = hound::WavReader::new(buf).unwrap();
            let spec = r.spec();
            assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 22_050, bits));
            assert_eq!(r.len(), 22_050);
        }
    }

    #[test]
    fn wav_rejects_unsupported_depth() {
        assert!(write_wav(&tone(), 22_050, 12, Cursor::new(Vec::new())).is_err());
    }

    #[test]
    fn ogg_decodes_back() {
        let mut out = Vec::new();
        write_ogg(&tone(), 44_100, 5.0, &mut out).unwrap();
        let mut r = lewton::inside_ogg::OggStreamReader::new(Cursor::new(out)).unwrap();
        assert_eq!(r.ident_hdr.audio_sample_rate, 44_100);
        assert_eq!(r.ident_hdr.audio_channels, 1);
        let mut n = 0;
        while let Some(pkt) = r.read_dec_packet_itl().unwrap() {
            n += pkt.len();
        }
        assert!((n as i64 - 22_050).abs() < 2048, "decoded {n}");
    }

    #[test]
    fn prepare_normalizes_to_minus_one_db() {
        let s = prepare(&SoundPatch::default(), &ExportOptions::default());
        let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - NORMALIZE_PEAK).abs() < 1e-4);
    }

    #[test]
    fn export_to_path_writes_file_and_no_partial() {
        let dir = std::env::temp_dir().join(format!("sfxc-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for opts in [
            ExportOptions::default(),
            ExportOptions { format: ExportFormat::Ogg { quality: 6.0 }, ..Default::default() },
        ] {
            let path = dir.join(format!("a.{}", opts.format.extension()));
            export_to_path(&SoundPatch::default(), &opts, &path).unwrap();
            assert!(path.metadata().unwrap().len() > 100);
            assert!(!path.with_extension("part").exists());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn export_to_missing_dir_fails_cleanly() {
        let path = std::env::temp_dir().join("sfxc-no-such-dir-xyz").join("a.wav");
        assert!(export_to_path(&SoundPatch::default(), &ExportOptions::default(), &path).is_err());
    }
}
