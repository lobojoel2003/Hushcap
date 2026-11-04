//! Diagnostic: list devices and capture a few seconds from a source, printing
//! levels and detected speech. Useful when system audio "doesn't work".
//!
//! ```sh
//! cargo run -p hushcap-engine --example capture_probe -- system 5
//! ```

use std::time::{Duration, Instant};

use hushcap_core::audio::{rms, to_dbfs};
use hushcap_core::caption::AudioSource;
use hushcap_core::segmenter::{SegmentEvent, Segmenter, SegmenterConfig};
use hushcap_engine::capture::{self, CaptureSink};

struct Probe {
    samples: u64,
    segmenter: Segmenter,
    window: Vec<f32>,
}

impl CaptureSink for Probe {
    fn audio(&mut self, samples: &[f32]) {
        self.samples += samples.len() as u64;
        self.window.extend_from_slice(samples);
        if self.window.len() >= 8000 {
            println!("  level {:6.1} dBFS", to_dbfs(rms(&self.window)));
            self.window.clear();
        }
        for e in self.segmenter.push(samples) {
            match e {
                SegmentEvent::Started { start_ms, .. } => {
                    println!("  speech started at {start_ms} ms")
                }
                SegmentEvent::Final {
                    start_ms, end_ms, ..
                } => {
                    println!("  utterance {start_ms}–{end_ms} ms")
                }
                _ => {}
            }
        }
    }
    fn error(&mut self, message: String) {
        eprintln!("  device error: {message}");
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let source = match args.next().as_deref() {
        Some("system") => AudioSource::System,
        _ => AudioSource::Microphone,
    };
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let device = args.next();

    let list = capture::list_devices()?;
    println!(
        "host: {} (native system capture: {})",
        list.host, list.system_capture_native
    );
    for d in &list.microphones {
        println!(
            "  mic    {}{} [{}]",
            d.name,
            if d.is_default { " (default)" } else { "" },
            d.id
        );
    }
    for d in &list.system {
        println!(
            "  system {}{} [{}]",
            d.name,
            if d.is_default { " (default)" } else { "" },
            d.id
        );
    }

    let probe = Probe {
        samples: 0,
        segmenter: Segmenter::new(SegmenterConfig::default()),
        window: Vec::new(),
    };
    let start = Instant::now();
    let cap = capture::start_capture(source, device.as_deref(), start, probe)?;
    println!(
        "capturing {:?} from '{}' for {secs} s (opened in {:?})",
        source,
        cap.device_name(),
        start.elapsed()
    );
    std::thread::sleep(Duration::from_secs(secs));
    let t = Instant::now();
    cap.stop();
    println!("stop took {:?}", t.elapsed());
    println!("done after {:.1} s", start.elapsed().as_secs_f32());
    Ok(())
}
