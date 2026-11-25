# Decisions

## ADR-001: Native Rust app with a thin JNI layer
Status: accepted (2026-11-21)
Context: Pulse is a JS app (Electron + Capacitor WebView); Android-only code is ~1k lines of Java
(MediaControlsPlugin, PlaybackService, AutoBrowserService). MediaSession, notifications, foreground
services and Android Auto are Java system APIs.
Decision: Rewrite logic and UI in Rust. Keep only the minimal Java/Kotlin glue that Android requires,
called through the `jni` crate.
Consequences: No WebView, no Capacitor, no JS runtime except ADR-004. The web UI (app.js, styles.css)
is reference only.

## ADR-002: Slint for the UI
Status: accepted (2026-11-21)
Context: Needs a polished, touch-first media UI on Android. Slint 1.19 targets Android directly.
Decision: Slint, under its Royalty-free 2.0 license (allows desktop/mobile apps; requires attribution).
Consequences: UI is rebuilt from scratch in `.slint`; Monaco plugin editor is not portable.

## ADR-003: First-version scope
Status: accepted (2026-11-21)
Decision: v1 = streaming + search (YouTube, YouTube Music, SoundCloud), local library, queue/playback,
lyrics (synced + LRC, karaoke view), EQ, background playback + media notification.
Deferred: clip trimmer, Monaco plugin editor/plugins, P2P sync (PeerJS), Google library import,
Android Auto, Last.fm, Discord presence, custom video backgrounds.
Consequences: deferred units are listed as out of scope in MAP.md, not deleted.

## ADR-004: YouTube stream resolving with embedded QuickJS
Status: accepted (2026-11-21)
Context: youtubei.js evaluates YouTube's player JS for sig/n deciphering (mobile/extract.js:9).
No maintained pure-Rust equivalent.
Decision: InnerTube requests in Rust (reqwest); `rquickjs` evaluates only the decipher functions.
Consequences: adds QuickJS (C) to the build; the only JS that runs is YouTube's own player code.

## ADR-005: Keep existing data compatible
Status: accepted (2026-11-21)
Decision: Read and write the same files the Capacitor app uses (virtual `/data` under `Pulse/` in app
storage; downloads in `Music/Pulse`, `.webm` stored as `.mka`) and the same backup format (backup-core.js).
Consequences: users upgrade in place; storage layout is a compatibility contract to record.

## ADR-006: Android first
Status: accepted (2026-11-21)
Decision: Target Android only for v1. Desktop/iOS later, reusing the same Rust crates.

## ADR-007: Equivalence bar for pure-logic units
Status: accepted (2026-11-21)
Decision: Each ported logic unit must match the original's recorded results exactly (`diff` N/N,
`gate` pass). Unicode behavior follows JS (NFKD, toLowerCase, `\s`, localeCompare) unless an ADR says otherwise.

## ADR-008: No checkpoints
Status: accepted (2026-11-21)
Decision: The user asked for the port without questions. Port units back to back; report at the end.

## ADR-009: Data carries over through Pulse backup files
Status: accepted (2026-11-21), refines ADR-005
Context: the Capacitor app keeps settings, playlists, favourites and history in WebView localStorage
(`pulse_*`, `devsize_*` keys), inside Chromium's private LevelDB; only song files live under `Pulse/`.
Decision: the Rust app stores the same keys with the same JSON string values in its own key-value file,
and imports the old app's data through its backup files (backup-core.js format, and Riff `.rfacc`).
Songs already downloaded to `Music/Pulse` are picked up as they are.
Consequences: users export a backup in the old app once. Reading the WebView's LevelDB directly is possible
later (same keys), but not in v1.

## ADR-010: No embedded JS engine in v1
Status: accepted (2026-11-21), supersedes ADR-004
Context: the mobile app's main YouTube route (visionOS client HLS, mobile/extract.js youtubeHlsAudio) needs
no signature deciphering; only the youtubei.js fallback does.
Decision: pulse-net talks to InnerTube directly: visionOS HLS first, the iOS client's plain audio formats
second. Ciphered formats are skipped; no QuickJS.
Consequences: a song that only has ciphered formats fails with a message. QuickJS (rquickjs) can be added
behind the fallback later without touching callers.

## ADR-011: Playback rate without time-stretch
Status: accepted (2026-11-21)
Context: audio-engine.js plays speed-only presets ("Sped up", "Practice 0.75x") with preservesPitch.
Decision: v1 changes speed by resampling: "Slowed", "Daycore", "Nightcore" behave as in the original;
speed-only presets also shift the pitch.
Consequences: a WSOLA/phase-vocoder stage in pulse-audio restores the original behavior later.

## ADR-012: Parity harness instead of rust-anything bridges
Status: accepted (2026-11-21)
Context: `rust-anything bridge` finds no exports in the original's `if (typeof module …) module.exports`
files, and its napi bridges only matter while the JS app keeps running (ADR-001 drops it).
Decision: app-cases/record.js (+ capture.js for the original test suite) records the JS functions'
results; crates/pulse-core/tests/parity.rs replays them against the Rust ports.
Consequences: `cargo test -p pulse-core --test parity` is the proof for pulse-core; re-record after
changing the originals (`node app-cases/record.js > app-cases/cases.jsonl`).

## ADR-013: cpal 0.18 (AAudio from Rust, no Oboe)
Status: accepted (2026-11-21)
Context: cpal 0.15 plays through Oboe on Android, a C++ library that needs the NDK's C++ runtime packaged
into the APK. cpal 0.18 calls Android's AAudio through the pure-Rust `ndk` crate.
Decision: pulse-audio uses cpal 0.18.
Consequences: no libc++_shared.so in the APK; two small API changes in player.rs.
