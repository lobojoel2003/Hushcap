//! The live captioning pipeline.
//!
//! ```text
//!  capture thread (per source)            worker thread           speaker thread
//!  ───────────────────────────            ─────────────           ──────────────
//!  device → 16 kHz → Segmenter ─┬─ final ─► recognise ◄── label ── embed + cluster
//!                               └ partial ► (latest only)
//! ```
//!
//! * Each source (microphone, system audio) segments its own audio.
//! * One worker runs speech recognition for all sources. Finished utterances
//!   are never dropped; interim snapshots are, if the worker is busy, so the
//!   captions never fall behind.
//! * Speaker embeddings are computed on a separate thread *while* the worker
//!   recognises the same utterance, so labels add no latency.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use hushcap_core::audio::LevelMeter;
use hushcap_core::caption::{AudioSource, Caption, EngineEvent, EngineState};
use hushcap_core::diarization::{ClusterConfig, SpeakerClusterer};
use hushcap_core::segmenter::{SegmentEvent, Segmenter, SegmenterConfig};
use hushcap_core::vad::VadConfig;
use hushcap_core::{samples_to_ms, text, SAMPLE_RATE};
use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};

use crate::asr::{Mode, Recognizer, WhisperOptions};
use crate::capture::{self, Capture, CaptureSink};
use crate::speaker::{SpeakerEmbedder, MIN_SAMPLES};

/// Receives engine events. Implemented for any `Fn(EngineEvent)`.
pub trait EventSink: Send + Sync + 'static {
    /// Handle one event. Called from engine threads; must not block for long.
    fn emit(&self, event: EngineEvent);
}

impl<F: Fn(EngineEvent) + Send + Sync + 'static> EventSink for F {
    fn emit(&self, event: EngineEvent) {
        self(event)
    }
}

/// Per-source capture settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SourceSettings {
    /// Device id from [`capture::list_devices`], or `None` for the default.
    pub device: Option<String>,
}

/// Tuning that applies to every source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProcessingSettings {
    /// Show words while people are still talking.
    pub partial_results: bool,
    /// Voice detection sensitivity, 0 (loud speech only) to 1 (very quiet speech).
    pub vad_sensitivity: f32,
    /// Speaker separation, 0 (merge similar voices) to 1 (split eagerly).
    pub speaker_separation: f32,
    /// Ignore the microphone while system audio is speaking. Prevents double
    /// captions when a call plays through speakers instead of headphones.
    pub suppress_mic_echo: bool,
    /// Pause that ends a caption, in milliseconds.
    pub end_silence_ms: u32,
    /// Longest caption before it is split, in milliseconds.
    pub max_segment_ms: u32,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self {
            partial_results: true,
            vad_sensitivity: 0.5,
            speaker_separation: 0.5,
            suppress_mic_echo: true,
            end_silence_ms: 650,
            max_segment_ms: 12_000,
        }
    }
}

impl ProcessingSettings {
    fn segmenter_config(&self) -> SegmenterConfig {
        SegmenterConfig {
            vad: VadConfig::default().with_sensitivity(self.vad_sensitivity),
            end_silence_ms: self.end_silence_ms.clamp(250, 3000),
            max_segment_ms: self.max_segment_ms.clamp(3000, 25_000),
            partial_interval_ms: if self.partial_results { 900 } else { 0 },
            ..Default::default()
        }
    }

    fn cluster_config(&self) -> ClusterConfig {
        ClusterConfig::default().with_separation(self.speaker_separation)
    }
}

/// Everything needed to start live captions.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Speech model (GGML) path.
    pub speech_model: PathBuf,
    /// Speaker model (ONNX) path; `None` disables speaker labels.
    pub speaker_model: Option<PathBuf>,
    /// Recogniser options.
    pub whisper: WhisperOptions,
    /// Microphone settings; `None` disables the microphone.
    pub microphone: Option<SourceSettings>,
    /// System audio settings; `None` disables system audio.
    pub system: Option<SourceSettings>,
    /// Shared processing settings.
    pub processing: ProcessingSettings,
}

// ---------------------------------------------------------------------------
// Work queue

struct FinalWork {
    id: u64,
    source: AudioSource,
    start_ms: u64,
    end_ms: u64,
    speech_ms: u64,
    audio: Arc<[f32]>,
}

struct PartialWork {
    id: u64,
    source: AudioSource,
    start_ms: u64,
    audio: Vec<f32>,
    queued_at: Instant,
}

enum Work {
    Final(FinalWork),
    Partial(PartialWork),
}

#[derive(Default)]
struct Queue {
    finals: VecDeque<FinalWork>,
    partials: HashMap<AudioSource, PartialWork>,
    closed: bool,
}

/// Recent system-audio speech, for microphone echo suppression.
#[derive(Default)]
struct SpeechTimeline {
    open_since: Option<u64>,
    intervals: VecDeque<(u64, u64)>,
}

impl SpeechTimeline {
    fn overlap_fraction(&self, start: u64, end: u64) -> f32 {
        if end <= start {
            return 0.0;
        }
        let mut covered = 0u64;
        let open = self.open_since.map(|s| (s, u64::MAX));
        for &(s, e) in self.intervals.iter().chain(open.iter()) {
            let lo = s.max(start);
            let hi = e.min(end);
            if hi > lo {
                covered += hi - lo;
            }
        }
        covered as f32 / (end - start) as f32
    }
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    sink: Arc<dyn EventSink>,
    next_id: AtomicU64,
    system_speech: Mutex<SpeechTimeline>,
    settings: ProcessingSettings,
}

impl Shared {
    fn emit(&self, event: EngineEvent) {
        self.sink.emit(event);
    }

    fn push(&self, work: Work) {
        let mut q = self.queue.lock();
        match work {
            Work::Final(f) => {
                // A final supersedes the interim snapshot of the same utterance.
                if q.partials.get(&f.source).is_some_and(|p| p.id == f.id) {
                    q.partials.remove(&f.source);
                }
                q.finals.push_back(f);
            }
            Work::Partial(p) => {
                q.partials.insert(p.source, p);
            }
        }
        drop(q);
        self.wake.notify_one();
    }

    fn cancel_partial(&self, source: AudioSource, id: u64) {
        let mut q = self.queue.lock();
        if q.partials.get(&source).is_some_and(|p| p.id == id) {
            q.partials.remove(&source);
        }
    }

    /// Next job, or `None` once closed and drained.
    fn next(&self) -> Option<Work> {
        let mut q = self.queue.lock();
        loop {
            if let Some(f) = q.finals.pop_front() {
                return Some(Work::Final(f));
            }
            if q.closed {
                return None;
            }
            // Oldest waiting partial first, so sources take turns.
            let oldest = q
                .partials
                .iter()
                .min_by_key(|(_, p)| p.queued_at)
                .map(|(s, _)| *s);
            if let Some(source) = oldest {
                return q.partials.remove(&source).map(Work::Partial);
            }
            self.wake.wait(&mut q);
        }
    }

    fn queued(&self) -> usize {
        self.queue.lock().finals.len()
    }
}

// ---------------------------------------------------------------------------
// Source processing (runs on the capture thread)

/// Turns one source's audio into work for the recogniser. Implements
/// [`CaptureSink`], and can also be fed directly (files, tests).
pub struct SourceProcessor {
    source: AudioSource,
    shared: Arc<Shared>,
    segmenter: Segmenter,
    meter: LevelMeter,
    ids: HashMap<u64, u64>,
}

impl SourceProcessor {
    fn new(source: AudioSource, shared: Arc<Shared>) -> Self {
        Self {
            source,
            segmenter: Segmenter::new(shared.settings.segmenter_config()),
            meter: LevelMeter::new((SAMPLE_RATE / 10) as usize),
            shared,
            ids: HashMap::new(),
        }
    }

    /// Feed 16 kHz mono audio.
    pub fn push(&mut self, samples: &[f32]) {
        for db in self.meter.push(samples) {
            self.shared.emit(EngineEvent::Level {
                source: self.source,
                db,
                speaking: self.segmenter.is_speaking(),
            });
        }
        let events = self.segmenter.push(samples);
        self.handle(events);
    }

    /// End of input: close any open utterance.
    pub fn flush(&mut self) {
        let events = self.segmenter.flush();
        self.handle(events);
    }

    fn echo_suppressed(&self, start_ms: u64, end_ms: u64) -> bool {
        self.source == AudioSource::Microphone
            && self.shared.settings.suppress_mic_echo
            && self
                .shared
                .system_speech
                .lock()
                .overlap_fraction(start_ms, end_ms)
                > 0.5
    }

    fn handle(&mut self, events: Vec<SegmentEvent>) {
        for event in events {
            if self.source == AudioSource::System {
                let mut timeline = self.shared.system_speech.lock();
                match &event {
                    SegmentEvent::Started { start_ms, .. } => timeline.open_since = Some(*start_ms),
                    SegmentEvent::Final {
                        start_ms, end_ms, ..
                    } => {
                        timeline.open_since = None;
                        timeline.intervals.push_back((*start_ms, *end_ms));
                        while timeline.intervals.len() > 64 {
                            timeline.intervals.pop_front();
                        }
                    }
                    SegmentEvent::Discarded { .. } => timeline.open_since = None,
                    SegmentEvent::Partial { .. } => {}
                }
            }
            match event {
                SegmentEvent::Started { id, start_ms } => {
                    let caption_id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
                    self.ids.insert(id, caption_id);
                    if !self.echo_suppressed(start_ms, start_ms + 1) {
                        self.shared.emit(EngineEvent::SpeechStarted {
                            id: caption_id,
                            source: self.source,
                            start_ms,
                        });
                    }
                }
                SegmentEvent::Partial {
                    id,
                    start_ms,
                    audio,
                } => {
                    let Some(&caption_id) = self.ids.get(&id) else {
                        continue;
                    };
                    let now_ms = start_ms + samples_to_ms(audio.len() as u64);
                    if self.echo_suppressed(start_ms, now_ms) {
                        continue;
                    }
                    self.shared.push(Work::Partial(PartialWork {
                        id: caption_id,
                        source: self.source,
                        start_ms,
                        audio,
                        queued_at: Instant::now(),
                    }));
                }
                SegmentEvent::Final {
                    id,
                    start_ms,
                    end_ms,
                    speech_ms,
                    audio,
                } => {
                    let Some(caption_id) = self.ids.remove(&id) else {
                        continue;
                    };
                    if self.echo_suppressed(start_ms, end_ms) {
                        self.shared.cancel_partial(self.source, caption_id);
                        self.shared.emit(EngineEvent::Dropped { id: caption_id });
                        continue;
                    }
                    self.shared.push(Work::Final(FinalWork {
                        id: caption_id,
                        source: self.source,
                        start_ms,
                        end_ms,
                        speech_ms,
                        audio: audio.into(),
                    }));
                }
                SegmentEvent::Discarded { id } => {
                    if let Some(caption_id) = self.ids.remove(&id) {
                        self.shared.cancel_partial(self.source, caption_id);
                        self.shared.emit(EngineEvent::Dropped { id: caption_id });
                    }
                }
            }
        }
    }
}

impl CaptureSink for SourceProcessor {
    fn audio(&mut self, samples: &[f32]) {
        self.push(samples);
    }

    fn error(&mut self, message: String) {
        self.shared.emit(EngineEvent::Warning {
            message: format!("{}: {message}", self.source.label()),
        });
    }

    fn finished(&mut self) {
        self.flush();
    }
}

// ---------------------------------------------------------------------------
// Speaker labelling thread

struct SpeakerRequest {
    audio: Arc<[f32]>,
    duration_ms: u64,
    reply: Sender<Option<u32>>,
}

fn speaker_thread(
    model: PathBuf,
    config: ClusterConfig,
    requests: Receiver<SpeakerRequest>,
    shared: Arc<Shared>,
) {
    let embedder = match SpeakerEmbedder::load(&model) {
        Ok(e) => {
            log::info!("speaker model loaded ({}, {} dims)", e.framework(), e.dim());
            Some(e)
        }
        Err(e) => {
            shared.emit(EngineEvent::Warning {
                message: format!("Speaker labels are off: {e:#}"),
            });
            None
        }
    };
    let mut clusterer = SpeakerClusterer::new(config);
    for req in requests {
        let speaker = embedder.as_ref().and_then(|embedder| {
            if req.audio.len() < MIN_SAMPLES {
                return None;
            }
            match embedder.embed(&req.audio) {
                Ok(embedding) => {
                    let assignment = clusterer.assign(&embedding, req.duration_ms);
                    for (from, into) in assignment.merges {
                        shared.emit(EngineEvent::SpeakersMerged { from, into });
                    }
                    assignment.speaker
                }
                Err(e) => {
                    log::warn!("speaker embedding failed: {e:#}");
                    None
                }
            }
        });
        let _ = req.reply.send(speaker);
    }
}

// ---------------------------------------------------------------------------
// Recognition worker

struct SourceContext {
    text: String,
    last_end_ms: u64,
    last_speaker: Option<u32>,
}

fn worker_thread(
    shared: Arc<Shared>,
    mut recognizer: Box<dyn Recognizer>,
    speakers: Option<Sender<SpeakerRequest>>,
) {
    let mut contexts: HashMap<AudioSource, SourceContext> = HashMap::new();
    let mut done: HashSet<u64> = HashSet::new();
    let mut done_order: VecDeque<u64> = VecDeque::new();
    let mut remember_done = |id: u64, done: &mut HashSet<u64>| {
        done.insert(id);
        done_order.push_back(id);
        if done_order.len() > 256 {
            if let Some(old) = done_order.pop_front() {
                done.remove(&old);
            }
        }
    };

    while let Some(work) = shared.next() {
        match work {
            Work::Partial(p) => {
                if done.contains(&p.id) {
                    continue;
                }
                match recognizer.transcribe(&p.audio, Mode::Partial, "") {
                    Ok(t) => {
                        let Some(clean) = text::clean(&t.text) else {
                            continue;
                        };
                        if t.no_speech_prob > 0.85 || done.contains(&p.id) {
                            continue;
                        }
                        shared.emit(EngineEvent::Partial {
                            id: p.id,
                            source: p.source,
                            text: clean,
                            start_ms: p.start_ms,
                        });
                    }
                    Err(e) => log::warn!("partial recognition failed: {e:#}"),
                }
            }
            Work::Final(f) => {
                let started = Instant::now();
                let duration_ms = f.end_ms.saturating_sub(f.start_ms).max(1);
                // Kick off speaker labelling in parallel with recognition.
                let pending_speaker = speakers.as_ref().map(|tx| {
                    let (reply, rx) = bounded(1);
                    let _ = tx.send(SpeakerRequest {
                        audio: f.audio.clone(),
                        duration_ms,
                        reply,
                    });
                    rx
                });

                let ctx = contexts.entry(f.source).or_insert_with(|| SourceContext {
                    text: String::new(),
                    last_end_ms: 0,
                    last_speaker: None,
                });
                // Only use context from the last 20 s of conversation.
                if f.start_ms.saturating_sub(ctx.last_end_ms) > 20_000 {
                    ctx.text.clear();
                }
                let prompt = text::tail_for_prompt(&ctx.text, 200).to_string();
                let result = recognizer.transcribe(&f.audio, Mode::Final, &prompt);
                let speaker = pending_speaker
                    .and_then(|rx| rx.recv_timeout(Duration::from_secs(5)).ok())
                    .flatten();
                remember_done(f.id, &mut done);

                let transcript = match result {
                    Ok(t) => t,
                    Err(e) => {
                        shared.emit(EngineEvent::Warning {
                            message: format!("Could not transcribe a phrase: {e:#}"),
                        });
                        shared.emit(EngineEvent::Dropped { id: f.id });
                        continue;
                    }
                };
                let speech_ratio = f.speech_ms as f32 / duration_ms as f32;
                let cleaned = text::clean(&transcript.text).filter(|t| {
                    !text::is_probable_hallucination(t, transcript.no_speech_prob, speech_ratio)
                });
                let Some(caption_text) = cleaned else {
                    shared.emit(EngineEvent::Dropped { id: f.id });
                    continue;
                };

                // Very short phrases can't be labelled reliably; assume the
                // previous speaker on this source kept talking.
                let speaker = speaker.or_else(|| {
                    (speakers.is_some() && f.start_ms.saturating_sub(ctx.last_end_ms) < 2_000)
                        .then_some(ctx.last_speaker)
                        .flatten()
                });

                if !ctx.text.is_empty() {
                    ctx.text.push(' ');
                }
                ctx.text.push_str(&caption_text);
                if ctx.text.len() > 2_000 {
                    ctx.text = text::tail_for_prompt(&ctx.text, 600).to_string();
                }
                ctx.last_end_ms = f.end_ms;
                ctx.last_speaker = speaker;

                shared.emit(EngineEvent::Final {
                    caption: Caption {
                        id: f.id,
                        source: f.source,
                        speaker,
                        text: caption_text,
                        start_ms: f.start_ms,
                        end_ms: f.end_ms,
                        language: transcript.language,
                    },
                });
                let elapsed = started.elapsed().as_secs_f32();
                shared.emit(EngineEvent::Stats {
                    realtime_factor: elapsed / (duration_ms as f32 / 1000.0),
                    queued: shared.queued(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Session and engine

/// Recognition machinery without any audio devices: feed it audio through
/// [`Session::processor`]. Used by [`Engine`], file transcription and tests.
pub struct Session {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    speaker: Option<JoinHandle<()>>,
    started: Instant,
}

impl Session {
    /// Start the recognition and speaker threads.
    pub fn start(
        recognizer: Box<dyn Recognizer>,
        speaker_model: Option<PathBuf>,
        settings: ProcessingSettings,
        sink: Arc<dyn EventSink>,
    ) -> Result<Self> {
        let cluster_config = settings.cluster_config();
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            sink,
            next_id: AtomicU64::new(1),
            system_speech: Mutex::new(SpeechTimeline::default()),
            settings,
        });

        let (speaker_tx, speaker) = match speaker_model {
            Some(model) => {
                let (tx, rx) = unbounded();
                let shared = shared.clone();
                let handle = std::thread::Builder::new()
                    .name("hushcap-speakers".into())
                    .spawn(move || speaker_thread(model, cluster_config, rx, shared))
                    .context("could not start speaker thread")?;
                (Some(tx), Some(handle))
            }
            None => (None, None),
        };
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("hushcap-recognizer".into())
            .spawn(move || worker_thread(worker_shared, recognizer, speaker_tx))
            .context("could not start recognition thread")?;
        Ok(Self {
            shared,
            worker: Some(worker),
            speaker,
            started: Instant::now(),
        })
    }

    /// When the session started; all timestamps are relative to this.
    pub fn started(&self) -> Instant {
        self.started
    }

    /// A processor that feeds this session's recogniser.
    pub fn processor(&self, source: AudioSource) -> SourceProcessor {
        SourceProcessor::new(source, self.shared.clone())
    }

    /// Finish all queued work, then stop the threads.
    pub fn finish(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.shared.queue.lock().closed = true;
        self.shared.wake.notify_all();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        // The worker owned the speaker request sender; the speaker thread ends
        // once it is dropped.
        if let Some(s) = self.speaker.take() {
            let _ = s.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Live captions from real audio devices.
pub struct Engine {
    session: Option<Session>,
    captures: Vec<Capture>,
    sink: Arc<dyn EventSink>,
    description: String,
}

impl Engine {
    /// Load models, open devices and start captioning.
    #[cfg(feature = "whisper")]
    pub fn start(config: EngineConfig, sink: Arc<dyn EventSink>) -> Result<Self> {
        sink.emit(EngineEvent::State {
            state: EngineState::Starting,
        });
        let recognizer =
            match crate::asr::WhisperRecognizer::load(&config.speech_model, config.whisper.clone())
            {
                Ok(r) => r,
                Err(e) => {
                    sink.emit(EngineEvent::State {
                        state: EngineState::Stopped,
                    });
                    return Err(e);
                }
            };
        Self::start_with(config, Box::new(recognizer), sink)
    }

    /// Start with a specific recogniser (for alternative backends and tests).
    pub fn start_with(
        config: EngineConfig,
        recognizer: Box<dyn Recognizer>,
        sink: Arc<dyn EventSink>,
    ) -> Result<Self> {
        sink.emit(EngineEvent::State {
            state: EngineState::Starting,
        });
        let description = recognizer.describe();
        let result = (|| {
            if config.microphone.is_none() && config.system.is_none() {
                bail!("turn on the microphone, system audio, or both");
            }
            let session = Session::start(
                recognizer,
                config.speaker_model.clone(),
                config.processing.clone(),
                sink.clone(),
            )?;
            let mut captures = Vec::new();
            let mut errors = Vec::new();
            for (source, settings) in [
                (AudioSource::Microphone, &config.microphone),
                (AudioSource::System, &config.system),
            ] {
                let Some(settings) = settings else { continue };
                match capture::start_capture(
                    source,
                    settings.device.as_deref(),
                    session.started(),
                    session.processor(source),
                ) {
                    Ok(c) => captures.push(c),
                    Err(e) => errors.push(format!("{}: {e:#}", source.label())),
                }
            }
            if captures.is_empty() {
                bail!("{}", errors.join("; "));
            }
            for e in errors {
                sink.emit(EngineEvent::Warning { message: e });
            }
            Ok((session, captures))
        })();

        match result {
            Ok((session, captures)) => {
                sink.emit(EngineEvent::State {
                    state: EngineState::Running,
                });
                Ok(Self {
                    session: Some(session),
                    captures,
                    sink,
                    description,
                })
            }
            Err(e) => {
                sink.emit(EngineEvent::State {
                    state: EngineState::Stopped,
                });
                Err(e)
            }
        }
    }

    /// Description of the speech model in use.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Names of the devices being captured.
    pub fn devices(&self) -> Vec<String> {
        self.captures
            .iter()
            .map(|c| c.device_name().to_string())
            .collect()
    }

    /// Stop capturing, finish the phrases already heard, and shut down.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if self.session.is_none() {
            return;
        }
        self.sink.emit(EngineEvent::State {
            state: EngineState::Stopping,
        });
        for c in self.captures.drain(..) {
            c.stop();
        }
        if let Some(session) = self.session.take() {
            session.finish();
        }
        self.sink.emit(EngineEvent::State {
            state: EngineState::Stopped,
        });
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Transcribe pre-recorded 16 kHz mono audio with the same pipeline used for
/// live captions. Returns the finished captions in order.
pub fn transcribe_audio(
    audio: &[f32],
    recognizer: Box<dyn Recognizer>,
    speaker_model: Option<PathBuf>,
    settings: ProcessingSettings,
    on_event: impl Fn(&EngineEvent) + Send + Sync + 'static,
) -> Result<Vec<Caption>> {
    let captions = Arc::new(Mutex::new(Vec::new()));
    let sink_captions = captions.clone();
    let sink: Arc<dyn EventSink> = Arc::new(move |event: EngineEvent| {
        on_event(&event);
        match event {
            EngineEvent::Final { caption } => sink_captions.lock().push(caption),
            EngineEvent::SpeakersMerged { from, into } => {
                for c in sink_captions.lock().iter_mut() {
                    if c.speaker == Some(from) {
                        c.speaker = Some(into);
                    }
                }
            }
            _ => {}
        }
    });
    let settings = ProcessingSettings {
        partial_results: false,
        suppress_mic_echo: false,
        ..settings
    };
    let session = Session::start(recognizer, speaker_model, settings, sink)?;
    let mut processor = session.processor(AudioSource::Microphone);
    for chunk in audio.chunks(SAMPLE_RATE as usize / 10) {
        processor.push(chunk);
    }
    // Trailing silence lets the last phrase end naturally.
    processor.push(&vec![0.0; SAMPLE_RATE as usize]);
    processor.flush();
    session.finish();
    let mut out = std::mem::take(&mut *captions.lock());
    out.sort_by_key(|c| c.start_ms);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::Transcript;

    /// Recogniser that reports how much audio it saw.
    struct FakeRecognizer {
        calls: Arc<Mutex<Vec<(Mode, usize)>>>,
        delay: Duration,
    }

    impl Recognizer for FakeRecognizer {
        fn transcribe(&mut self, audio: &[f32], mode: Mode, _prompt: &str) -> Result<Transcript> {
            std::thread::sleep(self.delay);
            self.calls.lock().push((mode, audio.len()));
            let secs = audio.len() as f32 / SAMPLE_RATE as f32;
            Ok(Transcript {
                text: format!(" heard {secs:.1} seconds"),
                no_speech_prob: 0.01,
                language: Some("en".into()),
            })
        }
        fn describe(&self) -> String {
            "fake".into()
        }
    }

    fn speech(ms: u32, f0: f32) -> Vec<f32> {
        let n = (SAMPLE_RATE as u64 * ms as u64 / 1000) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let env = 0.55 + 0.45 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                (1..=6)
                    .map(|h| (2.0 * std::f32::consts::PI * f0 * h as f32 * t).sin() / h as f32)
                    .sum::<f32>()
                    * 0.15
                    * env
            })
            .collect()
    }

    fn silence(ms: u32) -> Vec<f32> {
        vec![0.0; (SAMPLE_RATE as u64 * ms as u64 / 1000) as usize]
    }

    fn collect() -> (Arc<Mutex<Vec<EngineEvent>>>, Arc<dyn EventSink>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let e = events.clone();
        (events, Arc::new(move |ev: EngineEvent| e.lock().push(ev)))
    }

    #[test]
    fn file_transcription_produces_ordered_captions() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recognizer = Box::new(FakeRecognizer {
            calls: calls.clone(),
            delay: Duration::ZERO,
        });
        let mut audio = silence(800);
        audio.extend(speech(2000, 140.0));
        audio.extend(silence(1500));
        audio.extend(speech(3000, 220.0));
        audio.extend(silence(500));
        let captions = transcribe_audio(
            &audio,
            recognizer,
            None,
            ProcessingSettings::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(captions.len(), 2, "{captions:?}");
        assert!(captions[0].start_ms < captions[1].start_ms);
        assert!(
            captions[0].text.starts_with("heard 2."),
            "{}",
            captions[0].text
        );
        assert!(captions.iter().all(|c| c.speaker.is_none()));
        // No partials in file mode.
        assert!(calls.lock().iter().all(|(m, _)| *m == Mode::Final));
    }

    #[test]
    fn live_session_emits_partials_then_final() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (events, sink) = collect();
        let session = Session::start(
            Box::new(FakeRecognizer {
                calls,
                delay: Duration::from_millis(5),
            }),
            None,
            ProcessingSettings::default(),
            sink,
        )
        .unwrap();
        let mut mic = session.processor(AudioSource::Microphone);
        let mut audio = silence(500);
        audio.extend(speech(4000, 150.0));
        audio.extend(silence(1200));
        for chunk in audio.chunks(1600) {
            mic.push(chunk);
            std::thread::sleep(Duration::from_millis(2));
        }
        mic.flush();
        session.finish();

        let events = events.lock();
        let started = events
            .iter()
            .position(|e| matches!(e, EngineEvent::SpeechStarted { .. }))
            .expect("speech started");
        let partial = events
            .iter()
            .position(|e| matches!(e, EngineEvent::Partial { .. }))
            .expect("partial");
        let final_ = events
            .iter()
            .position(|e| matches!(e, EngineEvent::Final { .. }))
            .expect("final");
        assert!(started < partial && partial < final_);
        assert!(events
            .iter()
            .any(|e| matches!(e, EngineEvent::Level { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, EngineEvent::Stats { .. })));
        // Every partial refers to the id of the final.
        let EngineEvent::Final { caption } = &events[final_] else {
            unreachable!()
        };
        for e in events.iter() {
            if let EngineEvent::Partial { id, .. } = e {
                assert_eq!(*id, caption.id);
            }
        }
    }

    #[test]
    fn mic_echo_of_system_audio_is_suppressed() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (events, sink) = collect();
        let session = Session::start(
            Box::new(FakeRecognizer {
                calls,
                delay: Duration::ZERO,
            }),
            None,
            ProcessingSettings {
                partial_results: false,
                ..Default::default()
            },
            sink,
        )
        .unwrap();
        let mut system = session.processor(AudioSource::System);
        let mut mic = session.processor(AudioSource::Microphone);
        // The same far-end speech arrives on both (speakers → microphone).
        let mut audio = silence(500);
        audio.extend(speech(2500, 180.0));
        audio.extend(silence(1200));
        for chunk in audio.chunks(320) {
            system.push(chunk);
            mic.push(&chunk.iter().map(|s| s * 0.3).collect::<Vec<_>>());
        }
        system.flush();
        mic.flush();
        session.finish();
        let finals: Vec<_> = events
            .lock()
            .iter()
            .filter_map(|e| match e {
                EngineEvent::Final { caption } => Some(caption.source),
                _ => None,
            })
            .collect();
        assert_eq!(finals, vec![AudioSource::System]);
    }

    #[test]
    fn hallucinations_on_noise_are_dropped() {
        struct Polite;
        impl Recognizer for Polite {
            fn transcribe(&mut self, _: &[f32], _: Mode, _: &str) -> Result<Transcript> {
                Ok(Transcript {
                    text: "Thanks for watching!".into(),
                    no_speech_prob: 0.6,
                    language: None,
                })
            }
            fn describe(&self) -> String {
                "polite".into()
            }
        }
        let mut audio = silence(500);
        audio.extend(speech(1500, 150.0));
        audio.extend(silence(1000));
        let captions = transcribe_audio(
            &audio,
            Box::new(Polite),
            None,
            ProcessingSettings::default(),
            |_| {},
        )
        .unwrap();
        assert!(captions.is_empty());
    }

    #[test]
    fn engine_requires_a_source() {
        let (_, sink) = collect();
        let config = EngineConfig {
            speech_model: PathBuf::from("unused"),
            speaker_model: None,
            whisper: WhisperOptions::default(),
            microphone: None,
            system: None,
            processing: ProcessingSettings::default(),
        };
        let recognizer = Box::new(FakeRecognizer {
            calls: Arc::new(Mutex::new(Vec::new())),
            delay: Duration::ZERO,
        });
        let err = Engine::start_with(config, recognizer, sink).err().unwrap();
        assert!(err.to_string().contains("microphone"));
    }

    #[test]
    fn timeline_overlap() {
        let mut t = SpeechTimeline::default();
        t.intervals.push_back((1000, 2000));
        assert_eq!(t.overlap_fraction(1000, 2000), 1.0);
        assert_eq!(t.overlap_fraction(1500, 2500), 0.5);
        assert_eq!(t.overlap_fraction(3000, 4000), 0.0);
        t.open_since = Some(3500);
        assert_eq!(t.overlap_fraction(3000, 4000), 0.5);
    }
}
