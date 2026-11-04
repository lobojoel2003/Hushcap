# Changelog

All notable changes to Hushcap are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-04

First public release: private, offline live captions for Windows, Linux and Android.

### Added

- **Desktop app (Windows, Linux)** built with Tauri 2 and Svelte 5.
  - Live captions from the microphone, system audio (WASAPI loopback on Windows,
    PulseAudio/PipeWire monitor sources on Linux), or both at once.
  - On-device speech recognition with whisper.cpp 1.8, with interim text while people talk and a
    shortened encoder window for low latency on short phrases.
  - Speaker labels from an ONNX speaker-embedding model (CAM++ or WeSpeaker ResNet34) run by
    tract, with online clustering that has no small speaker limit, merges split speakers, and lets
    you rename people.
  - Microphone echo suppression while system audio is speaking.
  - Model manager: verified downloads (SHA-256, format checks, resume), offline import, and a
    catalogue of English and multilingual Whisper models.
  - Readability: Atkinson Hyperlegible and OpenDyslexic fonts, text size and line spacing, light,
    dark and yellow-on-black high-contrast themes, colour-blind-safe speaker colours,
    captions-only view and always-on-top window.
  - Transcript export to TXT, SRT, WebVTT, Markdown and JSON; copy to clipboard.
  - Keyboard shortcuts for every main action; screen-reader friendly live region.
- **Android app** (Android 8.0+) in Kotlin and Jetpack Compose.
  - Foreground captioning service for the microphone and, on Android 10+, other apps' audio.
  - On-device Vosk recognition in 15 languages with speaker labels from Vosk speaker vectors.
  - Model download and offline zip import, transcript sharing and saving, high-contrast theme,
    adjustable text size, latest caption in the notification (hidden on the lock screen).
- **Command-line tool** `hushcap`: live captions in the terminal, WAV transcription to any export
  format, device listing, and model download, verification, import and deletion.
- **Core library** with a streaming resampler, adaptive voice activity detection, utterance
  segmentation, Kaldi-compatible filterbank features, speaker clustering, Whisper hallucination
  filtering and transcript formats.
- Versioning from a single `VERSION` file, build numbers and git commit embedded in every binary.
- CI for Rust (Linux, Windows), frontend, Android and an end-to-end transcription test; release
  automation producing installers, APK, CLI archives and checksums.

[Unreleased]: https://github.com/lobojoel2003/Hushcap/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/lobojoel2003/Hushcap/releases/tag/v0.1.0
