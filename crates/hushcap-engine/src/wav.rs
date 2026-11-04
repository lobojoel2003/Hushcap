//! Reading and writing WAV files (used by the CLI and tests).

use std::path::Path;

use anyhow::{bail, Context, Result};
use hushcap_core::audio::downmix_to_mono;
use hushcap_core::resample::resample_all;
use hushcap_core::SAMPLE_RATE;

/// Read a WAV file as 16 kHz mono `f32` samples in [-1, 1].
pub fn read_wav_16k_mono(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("could not open {}", path.display()))?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader.samples::<f32>().collect::<Result<_, _>>()?,
        (hound::SampleFormat::Int, bits @ 8..=32) => {
            let scale = 1.0 / (1u64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()?
        }
        (format, bits) => bail!("unsupported WAV encoding: {format:?} {bits}-bit"),
    };
    let mut mono = Vec::with_capacity(interleaved.len() / spec.channels.max(1) as usize);
    downmix_to_mono(&interleaved, spec.channels as usize, &mut mono);
    Ok(resample_all(&mono, spec.sample_rate, SAMPLE_RATE))
}

/// Write 16 kHz mono samples as a 16-bit WAV file.
pub fn write_wav_16k_mono(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_resample() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        let samples: Vec<f32> = (0..16_000)
            .map(|i| ((i as f32) * 0.05).sin() * 0.5)
            .collect();
        write_wav_16k_mono(&path, &samples).unwrap();
        let back = read_wav_16k_mono(&path).unwrap();
        assert_eq!(back.len(), samples.len());
        assert!((back[100] - samples[100]).abs() < 1e-3);

        // 48 kHz stereo is downmixed and resampled.
        let path48 = dir.path().join("b.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path48, spec).unwrap();
        for _ in 0..48_000 {
            w.write_sample(0.25f32).unwrap();
            w.write_sample(0.75f32).unwrap();
        }
        w.finalize().unwrap();
        let back = read_wav_16k_mono(&path48).unwrap();
        assert_eq!(back.len(), 16_000);
        assert!((back[8000] - 0.5).abs() < 1e-3);
    }
}
