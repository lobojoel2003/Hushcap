## What does this change?

<!-- A short description and the reason for it. Link issues with "Fixes #123". -->

## How was it tested?

<!-- Platforms, models and audio sources you tried; new or updated tests. -->

## Checklist

- [ ] No audio, transcript or usage data can leave the device because of this change
- [ ] `cargo fmt`, `cargo clippy -D warnings` and `cargo test` pass
- [ ] Frontend `npm run lint`, `npm run check` and `npm test` pass (if the desktop UI changed)
- [ ] Android `./gradlew lintDebug testDebugUnitTest` passes (if the Android app changed)
- [ ] Works with large text, high contrast and keyboard only (if the UI changed)
- [ ] `CHANGELOG.md` updated under **Unreleased** (if user-visible)
