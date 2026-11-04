# Contributing to Hushcap

Thank you for helping make private captions better. Contributions of every size are welcome:
bug reports, testing on your hardware, accessibility feedback, translations, documentation and code.

## Ground rules

1. **Privacy is non-negotiable.** No change may send audio, transcripts or usage data off the
   device. New network access needs a clear, user-initiated reason and must be documented in
   [PRIVACY.md](PRIVACY.md).
2. **Accessibility first.** UI changes should work with large text, high contrast, keyboard only and
   screen readers.
3. Be kind. See the [Code of Conduct](CODE_OF_CONDUCT.md).

## Development setup

See *Building from source* in the [README](README.md#building-from-source). In short:

```bash
cargo test --workspace                    # Rust crates
cd desktop && npm ci && npm run tauri dev # desktop app with hot reload
cd android && ./gradlew assembleDebug     # Android app
```

Working on the desktop UI without Rust: `cd desktop && npm run dev` and open
<http://localhost:1420> in a browser. A development-only mock backend plays a demo conversation
(add `?autostart`, `?theme=dark|light|contrast`, `?fresh` for the first-run screen, `?compact`).
It is stripped from production builds.

Useful environment variables:

- `HUSHCAP_MODELS_DIR`: use a different model folder (shared by the CLI and desktop app).
- `HUSHCAP_TEST_SPEAKER_MODEL`: path to a speaker `.onnx` model to enable the speaker integration test.
- `RUST_LOG=debug`: verbose desktop logs; `hushcap -v` for the CLI.

## Before opening a pull request

Run the same checks as CI:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd desktop && npm run lint && npm run check && npm test)
(cd android && ./gradlew lintDebug testDebugUnitTest)
```

- Keep pull requests focused, and describe what changed and how you tested it.
- Add or update tests for behaviour changes. The DSP and clustering code in `hushcap-core` is pure
  and easy to test with synthetic audio.
- Add a line under **Unreleased** in [CHANGELOG.md](CHANGELOG.md) for user-visible changes.
- Use clear commit messages in the imperative mood ("Add VTT export", not "Added").

## Where things live

| Area | Location |
| --- | --- |
| Voice detection, segmentation, clustering, export | `crates/hushcap-core/src/` |
| Audio capture, Whisper, speaker model, pipeline, downloads | `crates/hushcap-engine/src/` |
| CLI | `crates/hushcap-cli/src/main.rs` |
| Desktop UI | `desktop/src/` |
| Desktop commands and settings | `desktop/src-tauri/src/` |
| Android | `android/app/src/main/java/io/github/lobojoel2003/hushcap/` |

Architecture notes are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Reporting bugs

Use the issue templates. Include your platform, Hushcap version and build number (About screen or
`hushcap --version`), the model you use, and whether the problem is with the microphone, system
audio or both. Please do **not** attach recordings of real conversations.

## License

By contributing, you agree that your contributions are licensed under the GNU General Public
License v3.0 or later, the same as the project.
