# Security policy

## Supported versions

Security fixes are made for the latest release. Please update before reporting.

| Version | Supported |
| ------- | --------- |
| 0.1.x   | ✅        |

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Report privately through GitHub: go to the repository's **Security** tab and choose
**Report a vulnerability** (private vulnerability reporting). Include:

- what the issue is and its impact (for example, audio or transcripts leaving the device, a
  malicious model file executing code, a path traversal on import);
- steps to reproduce, affected platform and version (`hushcap --version` or the About screen);
- any proof-of-concept files.

You can expect an acknowledgement within a week. Once a fix is released, the report will be
credited in the changelog unless you prefer otherwise.

## Scope notes

- Model files are untrusted input. Hushcap validates format and checksums, restricts download hosts,
  and rejects archive entries that escape the model folder; bypasses of these checks are in scope.
- Any way for audio, transcripts or settings to leave the device without the user's explicit action
  is treated as a high-severity issue.
