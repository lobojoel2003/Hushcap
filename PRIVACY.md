# Privacy

Hushcap exists so that people can have captions without giving their conversations to anyone.
This document describes exactly what Hushcap does with data. It applies to the desktop app, the
Android app and the command-line tool.

## Audio

- Audio from the microphone and from system/device audio is processed **inside the Hushcap process
  on your device** and then discarded.
- Audio is **never uploaded** and **never saved to disk**.

## Transcripts

- Captions are kept **in memory** while the app is open.
- They are written to a file **only when you choose** *Save*, *Export* or *Share*, and only to the
  place you pick.
- Clearing the transcript or closing the app removes it from memory.
- On Android, app data is excluded from cloud backup and device-to-device transfer.

## Network

Hushcap makes **one kind of network request**: downloading a model **that you asked for**.

| App | Hosts contacted | When |
| --- | --- | --- |
| Desktop and CLI | `huggingface.co` and its download CDN (speech models); `github.com` and its release CDN (speaker models) | When you click *Download* or run `hushcap models download` |
| Android | `alphacephei.com` | When you tap *Download* |

- Requests go over HTTPS only, and redirects are allowed only to the hosts above.
- Downloaded files are verified (SHA-256 where published, file format always) before use.
- No other requests are made: **no telemetry, analytics, crash reporting, ads, accounts, license
  checks or update checks**.
- The desktop app's user interface runs under a Content Security Policy that blocks all network
  access from the web view.
- You can avoid the network entirely by importing model files copied from another device.

## Settings

Settings (theme, text size, chosen devices and models) are stored locally in the app's
configuration folder. Speaker names you type are kept in memory with the transcript and are not
saved separately.

## Permissions

- **Microphone**: to caption speech around you.
- **Desktop system audio**: read through the operating system's loopback/monitor interface, only
  while captions are running.
- **Android device audio**: Android's screen-capture consent is required to hear other apps
  (Android 10+). Hushcap uses only the audio and never captures the screen image.
- **Android notifications**: to show that captioning is running, as Android requires for
  foreground services.
- **Internet**: model downloads only, as described above.

## Questions

Open an issue at <https://github.com/lobojoel2003/Hushcap/issues>. For security concerns, follow
[SECURITY.md](SECURITY.md).
