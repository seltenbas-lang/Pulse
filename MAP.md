# Pulse → Rust: map

Goal: native Android music player in Rust (Slint UI), behaviorally equal to Pulse 1.3.10 for the v1
scope. Decisions: DECISIONS.md (ADR-001…012). Terms: GLOSSARY.md. Generated plan: ../../plan.md.

## Crates
| Crate | What | Proof |
| --- | --- | --- |
| pulse-core | pure logic ported 1:1 | 3329/3329 recorded JS cases (tests/parity.rs) |
| pulse-net | YouTube, YouTube Music, SoundCloud, HLS joining, lrclib | unit tests; **not run live** (see README-rust.md) |
| pulse-audio | symphonia decode, EQ chain, rate, cpal output | unit tests (EQ response, decode, resampler) |
| pulse-store | localStorage keys in a JSON file, profiles, backup restore | unit tests |
| pulse-app | Slint UI + controller, Android JNI bridge | desktop run checked on screen; Android **not built** |

## Units
| Unit | State |
| --- | --- |
| search-text.js | done (parity) |
| lyrics-rtl.js | done (parity) |
| now-playing.js | done (parity) |
| library-tools.js | done |
| lyrics-match.js | done (parity) |
| track-match.js | done (parity) |
| track-sort.js | done (parity) |
| library-pages.js | done (parity) |
| recommend-core.js | done (parity) |
| backup-core.js | done (parity) + Riff .rfacc |
| profiles-core.js | key layout done (parity); profile switching UI open |
| mobile/extract.js | done except youtubei.js fallback deciphering (ADR-010) and playlist LockupView parsing |
| mobile/ts2aac.js, HLS download | done |
| server.js fetchLyricsDirect, app.js LRC parsing | done |
| audio-engine.js | EQ (3+10 band), volume, rate done; reverb, echo, distortion, HQ, normalize, crossfade open |
| app.js UI | search, library (favorites/history/device/playlists), player, lyrics, queue, settings done |
| MediaControlsPlugin/PlaybackService | media session, notification, foreground service done (Java, not built) |
| lyrics-sources.js (Genius, NetEase yrc) | open |
| ytm-artist.js (artist pages) | open |
| soundcloud-search.js | covered by pulse-net soundcloud::search (extract.js rules) |

## Out of scope for v1 (ADR-003)
track-trim.js, tools.js/plugins.js/vendor/monaco, sync.js/sync-core.js, google-library.js,
AutoBrowserService, lastfm*.js, discord-presence.js, Electron main process, desktop/iOS packaging.

## Open
- Build the APK (needs the Android SDK/NDK; see README-rust.md) and test on a phone.
- Live-test pulse-net against YouTube/SoundCloud (`cargo run -p pulse-net --example probe`).
- Opus playback (itag 251, `.mka` downloads of the old app): needs an Opus decoder.
