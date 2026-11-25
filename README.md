# Pulse

A music player for Android: YouTube, YouTube Music and SoundCloud search, your own music files,
playlists, favorites, synced lyrics, an audio sheet with speed, pitch, EQ presets and a sleep timer,
themes, background pictures and profiles. It is a native rewrite in Rust of
[Pulse 1.3.10](LEGAL.md) (originally a JavaScript/Capacitor app), made to look and behave like the
original's phone version while using far less battery.

[![Made with Slint](https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-light.svg)](https://slint.dev)

## Install

Download `pulse-<version>.apk` from the [latest release](../../releases/latest) and open it on your
phone (allow installing apps from your browser or file manager when Android asks). After that, Pulse
updates itself: **Settings → About → Updates** checks for a new release (also quietly once a day),
downloads it and opens Android's installer.

Coming from the original app (or from a build signed with another key)? Android only updates an app
signed with the same key, so: in the old app, **Settings → Storage → Back up this profile**, uninstall
it, install this one, then **Settings → Storage → Restore a backup**.

Phones: 64-bit ARM (arm64-v8a), Android 8.0 or newer. An x86_64 build is included for emulators.

## What's in it

[PARITY.md](PARITY.md) lists every setting and screen of the original and where each stands: ported,
partly ported (with what's missing), or not possible as it was (with the closest equivalent).

```
crates/pulse-core    pure logic, ported 1:1 and checked against the original's recorded results
crates/pulse-net     YouTube / YouTube Music / SoundCloud search and audio, HLS, LRCLIB lyrics
crates/pulse-audio   decoding (symphonia), the EQ chain, speed and pitch, output (cpal / AAudio)
crates/pulse-store   the original's storage keys in one JSON file; profiles; backups
crates/pulse-app     the Slint UI and controller; the Android entry point and JNI bridge
android/             the Java layer: media session, notification, audio focus, screen-off player,
                     file picker, update installer; Gradle files
app-cases/           recorded results of the original's JavaScript, for the parity tests
```

The original JavaScript app is in this repository's history (the first commit), as the reference the
Rust code was checked against.

## Build

Needs Rust, the Android SDK (platform 35, build-tools 35), **NDK r30** (Skia needs its libc++), Java 17
and Gradle 8.9+.

```bash
export ANDROID_HOME=~/Android/Sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/30.0.16248370
./build-android.sh release     # -> dist/pulse-<version>.apk
```

Release builds are signed with the Pulse release key when `PULSE_SIGNING_ENV` points at its
`signing.env` (kept outside the repository, never committed); otherwise with the debug key, which can't
update an installed release.

Versions: `./bump-version.sh` moves the version on for each release (2.0.1 … 2.0.10, then 2.1.0). It
lives in `Cargo.toml` only; the app's About page, Android's version name and code and the APK's name
all come from there.

Releasing: bump the version and commit, then on GitHub: Actions → Release → Run workflow. Bug fixes go
out as pre-releases (betas, the default); phones with Settings → About → "Get beta versions" on get
them. To make one the version everyone gets, edit the release and untick "Set as a pre-release". The Release workflow
(`.github/workflows/release.yml`) builds the APK, signs it and publishes it as that GitHub release. It
needs two repository secrets (Settings → Secrets and variables → Actions): `PULSE_KEYSTORE_BASE64`
(the release keystore, base64-encoded) and `PULSE_KEYSTORE_PASSWORD`.

On a computer (for development): `cargo run -p pulse-app --release` (Linux needs
`libasound2-dev libfontconfig1-dev libxkbcommon-dev`).

## Tests

```bash
cargo test --workspace
```

The parity tests replay thousands of results recorded from the original's JavaScript
(`app-cases/*.jsonl`) against the Rust ports: search text, sorting, backups, playlists, profiles, the
theme engine and more.

## License

MIT, see [LICENSE](LICENSE). Pulse is a player for private listening; see [LEGAL.md](LEGAL.md).
The UI is built with [Slint](https://slint.dev) under its royalty-free license.
