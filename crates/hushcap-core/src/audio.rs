//! Small, allocation-friendly helpers for raw audio buffers.

/// Average interleaved multi-channel audio down to mono, appending to `out`.
///
/// A trailing partial frame (fewer samples than `channels`) is ignored.
pub fn downmix_to_mono(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => out.extend_from_slice(interleaved),
        _ => {
            let scale = 1.0 / channels as f32;
            out.extend(
                interleaved
                    .chunks_exact(channels)
                    .map(|frame| frame.iter().sum::<f32>() * scale),
            );
        }
    }
}

/// Root-mean-square amplitude of a buffer (0.0 for an empty buffer).
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// Lowest level reported by [`to_dbfs`], used for digital silence.
pub const DB_FLOOR: f32 = -100.0;

/// Convert a linear amplitude (1.0 = full scale) to dBFS, clamped at [`DB_FLOOR`].
pub fn to_dbfs(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        DB_FLOOR
    } else {
        (20.0 * amplitude.log10()).max(DB_FLOOR)
    }
}

/// Peak absolute amplitude of a buffer.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
}

/// Accumulates audio and reports one RMS level reading per `window` samples.
///
/// Used to drive the input level meters in the UI, so people can see whether a
/// source is actually picking up sound.
#[derive(Debug, Clone)]
pub struct LevelMeter {
    window: usize,
    sum_sq: f64,
    count: usize,
}

impl LevelMeter {
    /// Create a meter that reports every `window` samples.
    pub fn new(window: usize) -> Self {
        Self {
            window: window.max(1),
            sum_sq: 0.0,
            count: 0,
        }
    }

    /// Feed samples; returns the levels (in dBFS) of every window completed.
    pub fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        let mut readings = Vec::new();
        for &s in samples {
            self.sum_sq += (s as f64) * (s as f64);
            self.count += 1;
            if self.count == self.window {
                let rms = (self.sum_sq / self.count as f64).sqrt() as f32;
                readings.push(to_dbfs(rms));
                self.sum_sq = 0.0;
                self.count = 0;
            }
        }
        readings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_stereo_averages_channels() {
        let mut out = Vec::new();
        downmix_to_mono(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0, 0.25], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn downmix_mono_is_passthrough() {
        let mut out = vec![9.0];
        downmix_to_mono(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![9.0, 0.1, 0.2]);
    }

    #[test]
    fn rms_of_full_scale_square_is_one() {
        assert!((rms(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn dbfs_conversion() {
        assert!((to_dbfs(1.0) - 0.0).abs() < 1e-5);
        assert!((to_dbfs(0.1) + 20.0).abs() < 1e-4);
        assert_eq!(to_dbfs(0.0), DB_FLOOR);
    }

    #[test]
    fn level_meter_reports_per_window() {
        let mut meter = LevelMeter::new(4);
        assert!(meter.push(&[0.5, 0.5]).is_empty());
        let levels = meter.push(&[0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(levels.len(), 2);
        assert!((levels[0] - to_dbfs(0.5)).abs() < 1e-4);
        assert_eq!(levels[1], DB_FLOOR);
    }

    #[test]
    fn peak_is_max_abs() {
        assert_eq!(peak(&[0.1, -0.7, 0.3]), 0.7);
    }
}
