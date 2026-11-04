# Architecture

Hushcap is split into small layers so that the hard parts (audio, recognition, speaker labels) are
shared by the desktop app and the CLI, and the pure logic is easy to test.

```text
┌──────────────────────┐   ┌───────────────────┐   ┌──────────────────────────────┐
│ desktop (Tauri)      │   │ hushcap-cli       │   │ android (Kotlin, Compose)    │
│ Svelte UI ⇄ commands │   │ live / transcribe │   │ CaptionService · Vosk        │
└─────────┬────────────┘   └────────┬──────────┘   │ SpeakerClusterer (Kotlin)    │
          │                         │              └──────────────────────────────┘
          └──────────┬──────────────┘
                     ▼
        ┌──────────────────────────────┐
        │ hushcap-engine               │  capture (cpal) · whisper.cpp · tract ONNX
        │ Engine / Session / pipeline  │  model store · verified downloads
        └──────────────┬───────────────┘
                       ▼
        ┌──────────────────────────────┐
        │ hushcap-core (pure Rust)     │  resampler · VAD · segmenter · fbank ·
        │                              │  speaker clustering · text cleanup · export
        └──────────────────────────────┘
```

## Desktop and CLI pipeline

```text
 capture thread (one per source)               recogniser thread         speaker thread
 ─────────────────────────────────            ─────────────────         ──────────────
 cpal stream → downmix → resample 16 kHz       finals (FIFO, never        fbank → ONNX
   → gap filling (silence when loopback          dropped) first,          embedding →
     pauses) → Segmenter (VAD)                   then latest partial      online clustering
       ├─ SpeechStarted ───────────────────► UI   per source
       ├─ Partial snapshot ─► queue (latest) ─►  whisper (greedy, 1 seg)
       └─ Final utterance ─► queue (FIFO) ───►  whisper (best-of / beam) ◄── label (parallel)
                                                 → clean + filter hallucinations
                                                 → Final caption ─────────────────────────► UI
```

Key decisions:

- **One recogniser for all sources.** Whisper is CPU-heavy; running a single worker keeps CPU use
  bounded. Final utterances are processed in order and never dropped; interim snapshots are
  replaced by newer ones, so a slow CPU makes interim text sparser instead of making captions lag.
- **Speaker embeddings run in parallel** with recognition of the same utterance, so labels add no
  latency.
- **Shared timeline.** Every source's timestamps are milliseconds since the session started.
  Capture threads insert silence when a device stops delivering audio (Windows loopback is silent
  when nothing plays), which also lets open utterances end on time.
- **Echo suppression.** System-audio utterances are tracked as time intervals. Microphone utterances
  that overlap system speech by more than half are dropped (configurable), which removes the copy of
  a call that speakers play into the microphone.
- **Short encoder window.** Whisper always encodes 30 s windows; for short utterances the encoder
  context is reduced to the audio length plus headroom (`asr::audio_ctx_for`), which cuts latency
  several-fold.

## Voice activity detection

`hushcap-core::vad` band-limits audio to roughly 150 Hz–4 kHz (two biquads), measures frame energy
every 20 ms, and estimates the noise floor as the 10th percentile of the last four seconds. A frame
is speech when it is louder than the floor by a threshold and above an absolute minimum. The
user-facing *voice sensitivity* moves both. The `Segmenter` adds pre-roll, a minimum speech length,
an end-of-utterance pause, interim snapshots and splitting of long monologues at the quietest frame.

## Speaker labels

1. Kaldi-compatible 80-bin log-mel filterbank (`hushcap-core::fbank`). For each model family the
   window, scaling and mean normalisation are read from ONNX metadata (WeSpeaker: Hamming window,
   16-bit scale; 3D-Speaker: Povey window). Output was verified against `kaldi-native-fbank` plus
   ONNX Runtime (cosine similarity 1.00000).
2. The ONNX model runs in [tract](https://github.com/sonos/tract) with a symbolic frame count, so
   any utterance length works without a native inference runtime.
3. `SpeakerClusterer` assigns each embedding to the closest speaker profile by cosine similarity,
   creates a new speaker above a threshold, never creates speakers from very short utterances, and
   merges profiles that converge. Merges are emitted as events so the UI relabels earlier captions.

On Android, Vosk's speaker model produces x-vectors that feed a Kotlin port of the same clusterer.

## Models and downloads

`hushcap-core::models` is the catalogue (URL, size, SHA-256). `hushcap-engine::download` enforces
HTTPS and a host allow-list (including on redirects), resumes partial downloads, verifies size,
checksum and file magic, and only then moves the file into place. A `.sha256` sidecar records the
checksum for later `hushcap models verify`.

## Desktop app

- `desktop/src-tauri` exposes commands (`start_captions`, `list_models`, `export_transcript`, …) and
  forwards engine events to the web view as `engine` events. Settings are JSON in the app config
  folder, written atomically and repaired if hand-edited out of range.
- `desktop/src` is Svelte 5. `lib/transcript.ts` is a pure reducer from engine events to transcript
  state (unit-tested); `lib/state.svelte.ts` holds app state with runes.
- Capabilities grant the web view only what it needs (dialogs, always-on-top), and the CSP blocks
  network access.

## Android app

- `CaptionService` is a foreground service (`microphone|mediaProjection`). One Vosk `Recognizer`
  per source runs on that source's capture thread.
- Device audio uses `AudioPlaybackCaptureConfiguration` (Android 10+), which requires the user's
  screen-capture consent each session.
- `CaptionRepository` is in-memory `StateFlow` state observed by Compose; nothing is persisted
  except settings.
- Pure logic (clustering, export, Vosk JSON parsing, zip extraction) lives in `core/` and is covered
  by JVM unit tests.
