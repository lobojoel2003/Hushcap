# Releasing Hushcap

Releases are automated. A maintainer only bumps the version, updates the changelog and pushes a tag.

## 1. Bump the version

```bash
./scripts/bump-version.sh 0.2.0
```

This updates `VERSION`, the Cargo workspace (and internal crate dependency versions),
`desktop/package.json`, `desktop/package-lock.json`, `desktop/src-tauri/tauri.conf.json` and
`Cargo.lock`, then runs `scripts/check-version.sh`. Android reads `VERSION` at build time; its
`versionCode` is `MAJOR×10000 + MINOR×100 + PATCH`.

## 2. Update the changelog

Move the items under `## [Unreleased]` in `CHANGELOG.md` into a new `## [0.2.0] - YYYY-MM-DD`
section, and update the comparison links at the bottom. The release workflow publishes this section
as the release notes and refuses to release a version without one.

## 3. Commit, tag and push

```bash
git commit -am "Release 0.2.0"
git tag -a v0.2.0 -m "Hushcap 0.2.0"
git push origin main v0.2.0
```

The **Release** workflow then:

1. checks that the tag matches `VERSION` and that the changelog has the section;
2. builds the desktop installers on Linux (deb, rpm, AppImage) and Windows (MSI, NSIS) with
   `GGML_NATIVE=OFF` for portable binaries;
3. builds the CLI archives for Linux and Windows;
4. builds and signs the Android APK;
5. writes `SHA256SUMS.txt` and publishes the GitHub Release.

Every binary embeds the CI run number as its build number, plus the commit and build date.

## Dry run

Run the **Release** workflow manually (*Actions → Release → Run workflow*) with `publish` unchecked
to build every artifact without creating a release. The artifacts are attached to the workflow run.

## Android signing

For upgradeable releases, add these repository secrets once:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | `base64 -w0 release.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | keystore password |
| `ANDROID_KEY_ALIAS` | key alias |
| `ANDROID_KEY_PASSWORD` | key password |

Create a keystore with:

```bash
keytool -genkeypair -v -keystore release.jks -alias hushcap -keyalg RSA -keysize 4096 -validity 10000
```

Keep it safe: Android only installs updates signed with the same key. Without these secrets the
workflow signs the APK with a temporary key and says so in the release notes; users of such builds
must uninstall before installing a build signed with the permanent key.

## Model checksums

The **Model catalogue** workflow downloads every catalogue model and prints its size and SHA-256.
Run it after changing `crates/hushcap-core/src/models.rs` and copy any missing checksums into the
catalogue so downloads are verified.
