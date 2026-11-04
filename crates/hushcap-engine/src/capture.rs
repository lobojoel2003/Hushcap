//! Audio capture from microphones and from system audio.
//!
//! * **Microphones**: any input device.
//! * **System audio** (what other apps are playing):
//!   * Windows: WASAPI loopback on an output device;
//!   * Linux: the "monitor" source of a PulseAudio/PipeWire sink;
//!   * elsewhere: pick a virtual loopback input device (e.g. BlackHole).
//!
//! Each capture runs on its own thread, which owns the device stream (some
//! platforms require that), converts samples to 16 kHz mono and hands them to
//! a callback. If a device stops delivering audio (Windows loopback goes
//! quiet when nothing is playing), silence is inserted so every source stays
//! aligned with the wall clock and open utterances still end.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Data, DeviceId, SampleFormat};
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use hushcap_core::audio::downmix_to_mono;
use hushcap_core::caption::AudioSource;
use hushcap_core::resample::Resampler;
use hushcap_core::SAMPLE_RATE;
use serde::Serialize;

/// Kind of capture device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    /// A microphone or other input.
    Input,
    /// A loopback of an output device (system audio).
    Loopback,
}

/// A device people can choose in settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    /// Stable identifier (`host:device`), safe to store in settings.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Device kind.
    pub kind: DeviceKind,
    /// Whether this is the system default for its role.
    pub is_default: bool,
}

/// Devices available for each source.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    /// Audio backend in use (e.g. "WASAPI", "PulseAudio", "ALSA").
    pub host: String,
    /// Devices for the microphone source.
    pub microphones: Vec<AudioDevice>,
    /// Devices for the system-audio source.
    pub system: Vec<AudioDevice>,
    /// Whether real system-audio capture (loopback/monitor) is available. When
    /// false, `system` lists input devices so a virtual loopback can be chosen.
    pub system_capture_native: bool,
}

fn host() -> cpal::Host {
    cpal::default_host()
}

fn host_name(host: &cpal::Host) -> String {
    host.id().name().to_string()
}

fn device_id(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.to_string())
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown device".to_string())
}

/// Whether a device is a PulseAudio/PipeWire monitor source.
fn is_monitor(id: &str) -> bool {
    id.ends_with(".monitor")
}

/// Whether `monitor_id` ("pulseaudio:<sink>.monitor") monitors the default
/// output device `default_out` ("pulseaudio:<sink>").
fn monitor_is_default(monitor_id: &str, default_out: Option<&str>) -> bool {
    is_monitor(monitor_id) && default_out == Some(monitor_id.trim_end_matches(".monitor"))
}

fn is_wasapi(host: &cpal::Host) -> bool {
    host_name(host).eq_ignore_ascii_case("wasapi")
}

fn is_pulse(host: &cpal::Host) -> bool {
    host_name(host).to_ascii_lowercase().contains("pulse")
}

/// List capture devices.
pub fn list_devices() -> Result<DeviceList> {
    let host = host();
    let mut list = DeviceList {
        host: host_name(&host),
        ..Default::default()
    };
    let default_in = host.default_input_device().and_then(|d| device_id(&d));
    let default_out = host.default_output_device().and_then(|d| device_id(&d));

    let inputs: Vec<cpal::Device> = host
        .input_devices()
        .context("could not list audio input devices")?
        .collect();
    for device in &inputs {
        let Some(id) = device_id(device) else {
            continue;
        };
        let name = device_name(device);
        if is_monitor(&id) {
            // Monitor of the default sink is the default system source.
            let is_default = monitor_is_default(&id, default_out.as_deref());
            list.system.push(AudioDevice {
                id,
                name,
                kind: DeviceKind::Loopback,
                is_default,
            });
        } else {
            list.microphones.push(AudioDevice {
                is_default: default_in.as_deref() == Some(id.as_str()),
                id,
                name,
                kind: DeviceKind::Input,
            });
        }
    }

    if is_wasapi(&host) {
        for device in host
            .output_devices()
            .context("could not list audio output devices")?
        {
            let Some(id) = device_id(&device) else {
                continue;
            };
            list.system.push(AudioDevice {
                is_default: default_out.as_deref() == Some(id.as_str()),
                name: device_name(&device),
                id,
                kind: DeviceKind::Loopback,
            });
        }
    }

    list.system_capture_native = !list.system.is_empty();
    if !list.system_capture_native {
        // No loopback support: let people pick a virtual loopback input instead.
        list.system = list
            .microphones
            .iter()
            .map(|d| AudioDevice {
                is_default: false,
                ..d.clone()
            })
            .collect();
    }
    Ok(list)
}

/// Whether this platform can capture system audio without extra software.
pub fn system_audio_supported() -> bool {
    let host = host();
    is_wasapi(&host) || is_pulse(&host)
}

/// Find the device to open for a source.
fn find_device(source: AudioSource, device: Option<&str>) -> Result<(cpal::Device, bool)> {
    let host = host();
    if let Some(id) = device.filter(|d| !d.is_empty()) {
        let parsed =
            DeviceId::from_str(id).map_err(|e| anyhow!("invalid device id '{id}': {e}"))?;
        let found = host
            .device_by_id(&parsed)
            .with_context(|| format!("audio device '{id}' is not connected"))?;
        let loopback =
            matches!(source, AudioSource::System) && is_wasapi(&host) && !found.supports_input();
        return Ok((found, loopback));
    }
    match source {
        AudioSource::Microphone => Ok((
            host.default_input_device()
                .context("no microphone found; check that one is connected and allowed")?,
            false,
        )),
        AudioSource::System => {
            if is_wasapi(&host) {
                let out = host
                    .default_output_device()
                    .context("no audio output device to capture from")?;
                return Ok((out, true));
            }
            if is_pulse(&host) {
                let default_sink = host
                    .default_output_device()
                    .and_then(|d| device_id(&d))
                    .and_then(|id| id.split_once(':').map(|(_, s)| s.to_string()));
                let monitors: Vec<cpal::Device> = host
                    .input_devices()?
                    .filter(|d| device_id(d).is_some_and(|id| is_monitor(&id)))
                    .collect();
                let wanted = default_sink.map(|s| format!("{s}.monitor"));
                let pick = monitors
                    .iter()
                    .find(|d| {
                        device_id(d).zip(wanted.as_ref()).is_some_and(|(id, w)| {
                            id.split_once(':').map(|(_, s)| s) == Some(w.as_str())
                        })
                    })
                    .or(monitors.first())
                    .cloned()
                    .context(
                        "no system audio monitor found (is PulseAudio or PipeWire running?)",
                    )?;
                return Ok((pick, false));
            }
            bail!(
                "this system cannot capture system audio directly; choose a loopback \
                 device (for example BlackHole) as the system audio device"
            )
        }
    }
}

/// A running capture. Stops when dropped.
pub struct Capture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    device_name: String,
}

impl Capture {
    /// Name of the device being captured.
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Stop capturing and wait for the capture thread to finish.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Callbacks a capture delivers to.
pub trait CaptureSink: Send + 'static {
    /// 16 kHz mono audio (including inserted silence for gaps).
    fn audio(&mut self, samples: &[f32]);
    /// The device failed (unplugged, permission revoked…).
    fn error(&mut self, message: String);
    /// The capture is ending; flush any state.
    fn finished(&mut self) {}
}

/// Start capturing `source` from `device` (or the default device) and deliver
/// audio to `sink`. `session_start` anchors the timeline shared by all
/// sources.
pub fn start_capture(
    source: AudioSource,
    device: Option<&str>,
    session_start: Instant,
    sink: impl CaptureSink,
) -> Result<Capture> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = bounded::<Result<String>>(1);
    let device = device.map(str::to_owned);
    let stop_thread = stop.clone();
    let thread = std::thread::Builder::new()
        .name(format!("hushcap-capture-{source:?}").to_lowercase())
        .spawn(move || {
            capture_thread(source, device, session_start, sink, stop_thread, ready_tx);
        })
        .context("could not start capture thread")?;
    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(device_name)) => Ok(Capture {
            stop,
            thread: Some(thread),
            device_name,
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err(anyhow!(
                "timed out opening the {} device",
                source.label().to_lowercase()
            ))
        }
    }
}

enum Chunk {
    Audio(Vec<f32>),
    Error(String),
}

fn convert<T>(data: &Data, channels: usize, tx: &Sender<Chunk>)
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    if let Some(samples) = data.as_slice::<T>() {
        let floats: Vec<f32> = samples
            .iter()
            .map(|&s| cpal::Sample::to_sample::<f32>(s))
            .collect();
        let mut mono = Vec::with_capacity(floats.len() / channels.max(1));
        downmix_to_mono(&floats, channels, &mut mono);
        let _ = tx.try_send(Chunk::Audio(mono));
    }
}

fn open_stream(
    device: &cpal::Device,
    loopback: bool,
    tx: Sender<Chunk>,
) -> Result<(cpal::Stream, u32)> {
    let config = if loopback {
        device.default_output_config()
    } else {
        device.default_input_config()
    }
    .context("could not read the device's audio format")?;
    let format = config.sample_format();
    let stream_config = config.config();
    let channels = stream_config.channels as usize;
    let rate = stream_config.sample_rate;
    let err_tx = tx.clone();
    let on_error = move |e: cpal::Error| {
        let _ = err_tx.try_send(Chunk::Error(e.to_string()));
    };

    macro_rules! build {
        ($t:ty) => {{
            let tx = tx.clone();
            device.build_input_stream_raw(
                stream_config,
                format,
                move |data: &Data, _: &cpal::InputCallbackInfo| convert::<$t>(data, channels, &tx),
                on_error,
                Some(Duration::from_secs(5)),
            )
        }};
    }
    let stream = match format {
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I24 => build!(cpal::I24),
        SampleFormat::I8 => build!(i8),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        other => bail!("unsupported audio sample format {other}"),
    }
    .map_err(|e| anyhow!("could not open audio device: {e}"))?;
    stream
        .play()
        .map_err(|e| anyhow!("could not start audio device: {e}"))?;
    Ok((stream, rate))
}

fn capture_thread(
    source: AudioSource,
    device: Option<String>,
    session_start: Instant,
    mut sink: impl CaptureSink,
    stop: Arc<AtomicBool>,
    ready: Sender<Result<String>>,
) {
    let (tx, rx): (Sender<Chunk>, Receiver<Chunk>) = bounded(256);
    let opened = find_device(source, device.as_deref()).and_then(|(device, loopback)| {
        let name = device_name(&device);
        open_stream(&device, loopback, tx).map(|(stream, rate)| (stream, rate, name))
    });
    let (stream, rate, name) = match opened {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    log::info!("capturing {source:?} from '{name}' at {rate} Hz");
    let _ = ready.send(Ok(name));

    let mut resampler = Resampler::new(rate, SAMPLE_RATE);
    let mut out = Vec::with_capacity(4096);
    // Samples delivered so far, on the session timeline.
    let mut emitted: u64 = 0;
    let expected = |now: Instant| -> u64 {
        (now.duration_since(session_start).as_secs_f64() * SAMPLE_RATE as f64) as u64
    };
    // Align the start of this source with the session timeline.
    let lead = expected(Instant::now());
    if lead > 0 {
        sink.audio(&vec![0.0; lead as usize]);
        emitted = lead;
    }
    let mut last_chunk = Instant::now();
    // Only pad after a real pause in delivery, never because of clock drift.
    const PAUSE: Duration = Duration::from_millis(300);

    while !stop.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Chunk::Audio(mono)) => {
                let now = Instant::now();
                out.clear();
                resampler.process(&mono, &mut out);
                if now.duration_since(last_chunk) > PAUSE {
                    let behind = expected(now).saturating_sub(emitted + out.len() as u64);
                    if behind > 0 {
                        sink.audio(&vec![0.0; behind as usize]);
                        emitted += behind;
                    }
                }
                last_chunk = now;
                sink.audio(&out);
                emitted += out.len() as u64;
            }
            Ok(Chunk::Error(message)) => {
                log::warn!("{source:?} capture error: {message}");
                sink.error(message);
            }
            Err(RecvTimeoutError::Timeout) => {
                // Nothing arrived: the device is silent or paused (Windows
                // loopback stops delivering when nothing plays). Keep time moving.
                let now = Instant::now();
                if now.duration_since(last_chunk) > PAUSE {
                    let behind = expected(now).saturating_sub(emitted);
                    if behind > 0 {
                        sink.audio(&vec![0.0; behind as usize]);
                        emitted += behind;
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let t = Instant::now();
    drop(stream);
    log::debug!("stream closed in {:?}", t.elapsed());
    sink.finished();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_detection() {
        assert!(is_monitor(
            "pulseaudio:alsa_output.pci.analog-stereo.monitor"
        ));
        assert!(!is_monitor("pulseaudio:alsa_input.usb-mic"));
        assert!(monitor_is_default(
            "pulseaudio:speakers.monitor",
            Some("pulseaudio:speakers")
        ));
        assert!(!monitor_is_default(
            "pulseaudio:hdmi.monitor",
            Some("pulseaudio:speakers")
        ));
        assert!(!monitor_is_default("pulseaudio:speakers.monitor", None));
        assert!(!monitor_is_default(
            "pulseaudio:speakers",
            Some("pulseaudio:speakers")
        ));
    }
}
