# Feature parity: Pulse 1.3.10 (phone build) → Rust

The bar: the Rust app looks and behaves like the original **phone** build (Capacitor, `html.is-mobile`).
Desktop-only rows (hidden by mobile.css / mobile-ui.js on phones) are listed at the end, not ported.
Inventory method: every `.setting-item` in index.html, filtered to what the original shows at 412×915
(Pixel 7) in Chromium; keys and defaults read from app.js (`getAdv(name, default)` keys are
`pulse_adv_<name>` = `'true'|'false'`).

Status: ✅ ported · 🟡 partial (see note) · ❌ missing · ⛔ can't be 1:1 (closest equivalent in note)
"Verified" column: **R** = seen working in the desktop build (screenshot/log), **C** = only compiles,
**—** = n/a. Nothing has been verified on a real phone yet.

Batch 3 (theme + settings): every phone-visible row of the original's Settings is now in the app with its
label, hint, control, key and default (app/settings.rs `SIMPLE` + `settings_groups`). A row this version can't
act on still saves its value under the original key (so backups carry it) and says so in a note under it
("🟡 saved" below). Search box: the placeholder now goes as soon as the field has focus (Android IME
composition text isn't in `text` until committed, so it stayed drawn over the letters).

## Screens & layout

| Item | Status | Verified | Note |
|---|---|---|---|
| Fits every phone width (320–480dp) and landscape | ✅ | R | nav shrinks to 44dp, rows compact <360dp, preset chips wrap, Audio sheet scrolls, player compacts <560dp tall |
| 32-bit ARM phones (armeabi-v7a) | ⛔ | — | Slint's Android backend needs Skia; rust-skia publishes no armv7 Android build. Closest: build Skia from source for armv7 (heavy toolchain). All 64-bit phones (Play-required since 2019) are covered |

## Settings

### Interface
| Setting | Key (default) | Status | Verified | Note |
|---|---|---|---|---|
| Language | `pulse_ui_lang` (en) | 🟡 | R | English only. The original machine-translates via Google Translate at runtime; ⛔ not ported (network service + DOM text walking). Closest: ship translated string tables |
| UI scale | `devsize_ui_scale` (100, 80–130, step 5) | ✅ | R | window scale factor × value/100 (90% and 120% seen); Android may reset it on rotation until the app restarts |
| Font scale | `pulse_font_scale` (100, 80–130) | ✅ | R | every font size × value/100 (`Theme.fk`) |
| Interface font | `pulse_font` (google) | 🟡 | C | asks for Google Sans / SF Pro Display / system; used when the phone has the font (not bundled: licence) |
| Icon style | `pulse_icon_pack` (material) | 🟡 | R | saved; one icon set drawn (note under the row) |
| Corner radius | `pulse_corner_radius` (16) | ✅ | R | slider 0–32 (Sharp/Round), rows/tiles use r−4, covers r (as --md-shape-corner-m/l) |
| No-Hand cursor | `pulse_no_hand_cursor` (on) | 🟡 | R | saved; no cursor on touch (same as the original on phones) |
| Position of searchbar | `pulse_search_pos` (middle) | ✅ | R | stored; no visible effect on phones, same as the original (mobile.css overrides it) |
| Search engine | `pulse_platform` (youtube) | ✅ | R | dropdown + the search bar switch; now stored as `youtube`/`ytmusic`/`soundcloud` like app.js (old `yt`/`ytm`/`sc` still read) |
| Compact engine buttons | `pulse_compact_engine_btns` ('1') | ✅ | R | narrower source pills |
| Hide engine buttons | `pulse_hide_engine_btns` ('1') | ✅ | R | |
| Show artists in search | `pulse_search_artists` (on) | 🟡 | R | saved; search returns songs only |
| Albums (in search) | `pulse_adv_albums` (true) | 🟡 | R | saved; search returns songs only |
| Show My files tab | `pulse_show_local` (!== 'false') | ✅ | R | |
| Show Albums tab / Show Artists tab | `pulse_show_albums`, `pulse_show_artists` (off) | 🟡 | R | saved; those pages aren't ported |
| Home layout | `pulse_home_layout` (spotify) | 🟡 | R | Pulse/Spotify chips, saved; Spotify layout only |
| Jump back in / Your artists / My Wave / Ambient light | `pulse_adv_jump_back`, `_fav_artists`, `_my_wave`, `_my_wave_ambient` (true) | 🟡 | C | Jump back in works (recent songs on home); the other three saved, sections not ported |
| Wavy progress bar | `pulse_wavy_progress` (on) | ✅ | R | travelling wave, flattens over 300ms when paused (player progress) |
| Progress bar knob | `pulse_progress_knob` (on) | ✅ | R | |
| Progress bar background | `pulse_progress_bg` (on) | ✅ | R | |
| Show wave visualizer in bottom bar / position / opacity / connected | `pulse_bar_wave`, `pulse_bar_viz_pos`, `pulse_bar_viz_opacity` (22), `pulse_viz_connected` | 🟡 | R | saved; no audio visualizer (needs FFT of the playing audio) |
| Visualizer in karaoke / with lyrics / opacity / smoothness / sensitivity | `pulse_tvk_visualizer*` (true/false/85), `pulse_viz_smooth` (70), `pulse_viz_sens` (100) | 🟡 | R | saved; no visualizer |
| Performance mode | `pulse_perf_mode` | ✅ | C | animations instant, wave still (same switch as Reduce motion) |
| Reduce motion | `pulse_adv_reduce_motion` (system → off) | ✅ | C | every `animate` duration × `Prefs.motion` (0), the wave stops scrolling |
| Blur effects | `pulse_adv_blur_fx` (true) | ⛔ | | Slint has no backdrop blur; closest: translucent tint (planned) |

### Theme
| Setting | Key (default) | Status | Verified | Note |
|---|---|---|---|---|
| Theme mode (Light/Dark/Auto album) | `pulse_theme_mode` (dark) | ✅ | R | light + dark seen; Auto: app.js analyzeCover on the 48×48 cover + derived mode/intensity/brightness (C) |
| Palette style | `pulse_palette_style` (classic) | ✅ | C | generateM3Palette ported; 4448 recorded JS cases match (app-cases/theme.jsonl) |
| Accent color / Hue | `pulse_theme_accent` (#6750A4) | ✅ | R | 7 swatches; hue sets hslToHex(h,55,50); custom colour picker ⛔ (no colour-picker widget; the hue slider is the closest) |
| Intensity | `pulse_theme_intensity` (0) | ✅ | R | disabled in Auto, like the original |
| Apply intensity to background | `pulse_adv_intensity_bg` (false) | ✅ | C | parity-tested |
| Brightness | `pulse_theme_brightness` (50) | ✅ | C | parity-tested |
| Highlight buttons | `pulse_highlight_buttons` (on) | ✅ | C | off: primary pills grey (body:not(.highlight-buttons)) |
| Interface opacity / Text opacity | `pulse_ui_alpha` (20) / `pulse_text_alpha` (100) | ✅ | R | page, bars and cards at the interface opacity over a background picture; text opacity always |
| Glass blur / strength | `pulse_adv_glass_blur` (false), `pulse_glass_blur` | ⛔ | | no backdrop blur in Slint; closest: frosted tint |
| Background (image/GIF/video) + opacity + blur | `pulse_bg_url`, `pulse_bg_opacity` (100), `pulse_bg_blur` (0) | ✅ | R | Choose file (Android picker, image/*) → kept as `<data>/background.img`, `pulse_bg_url` = its file:// URL; object-fit cover, opacity, blur (app.js: ¼ size, ¼ blur, scaled up). Remove deletes it. GIF = first frame; video ⛔ (no video decoder; closest: pick a still) |
| Cover background + brightness | `pulse_adv_cover_bg` (true), `pulse_cover_bg_brightness` (45) | ✅ | C | switches the drawer's cover gradient; brightness scales it (45 = as measured). No blurred cover |
| Mesh follows intensity / Mesh colour from cover | `pulse_adv_mesh_follow_intensity` (true), `pulse_adv_mesh_cover_color` (false) | 🟡 | R | saved; no mesh gradient |
| Lower interface opacity for backgrounds | `pulse_adv_ui_alpha_auto` (true) | ✅ | C | app.js onBackgroundChanged: a picture caps it at 50 (`pulse_ui_alpha_prev/_auto/_auto_kind`), removing restores it |
| App icon on Home button | `pulse_app_icon` (classic) | 🟡 | R | shown, disabled |

### Tracks
| Setting | Key (default) | Status | Verified | Note |
|---|---|---|---|---|
| Crossfade | `pulse_crossfade` (0 = off) | 🟡 | R | saved; player plays one song at a time (needs two decoders) |
| Output device | — | ✅ | R | system default (Android routes audio itself) |
| Smooth preset transitions | `pulse_smooth_presets` (!== 'false') | ✅ | C | 400ms smoothstep tween (app.js tweenSettings) |
| Save sound settings | `pulse_persist_sound_settings` (!== '0') | ✅ | R | |
| Show explicit tag [E] | `pulse_show_explicit` (!== '0') | ✅ | R | .explicit-badge 14px, library-pages isTrackExplicit |
| Blocked artists and songs | `pulse_blocked_artists`, `pulse_blocked_tracks` | ✅ | C | Block/Unblock song & artist in the song menu, list in Tracks (tap to unblock), skipped when the queue moves on. Not kept out of My Wave/mixes (not ported) |
| Lyrics source | `pulse_lyrics_source` (lrclib) | 🟡 | C | dropdown saved; LRCLIB used even for Genius (note shown) |
| Word-by-word source / translation / language / auto | `pulse_word_source` (auto), `pulse_tr_enabled`, `pulse_tr_lang` (en), `pulse_tr_auto` | 🟡 | R | saved; no word timing, no translation |
| Online lyrics lookup | `pulse_adv_online_lyrics` (true) | ✅ | C | off: only .lrc files next to songs |
| Lyrics translation / language / auto | `pulse_tr_enabled`, `pulse_tr_lang`, `pulse_tr_auto` | ⛔ | | Google Translate web endpoint; possible but network-dependent |
| Lyrics animation / speed / word highlight / word style / tightness | `pulse_lyrics_ease` (inout), `pulse_lyrics_speed` (1.0), `pulse_adv_karaoke_words`, `pulse_lyrics_word_style`, `pulse_lyrics_gradient_tightness` (2) | 🟡 | C | speed works (0.45 s / speed). Easing ⛔: Slint fixes `easing:` at compile time; closest: one curve. Word options saved |
| Auto-scroll synced lyrics | `pulse_adv_lyrics_autoscroll` (true) | ✅ | R | |

### Integrations, Profiles, Sync, Storage, Advanced, About
| Setting | Status | Verified | Note |
|---|---|---|---|
| Last.fm scrobbling | 🟡 | R | row shown, Connect disabled with a note (needs Pulse's API key + sign-in) |
| Import: YouTube / YouTube Music link | 🟡 | C | paste link in search; original opens an import dialog |
| Import: JSON file | ✅ | C | file picker → parsePlaylistFile; title-only songs looked up on YouTube (8 failures stop it, like backup.js) |
| Import: Spotify / SoundCloud / Apple Music / Yandex | 🟡 | R | rows shown, disabled with a note |
| Profiles: list / add / switch / delete | ✅ | R | delete removes the profile's own keys (never device keys); rename and colour missing |
| Sync (link devices) | ⛔ | R | section shown with its rows; Show a code disabled. PeerJS/WebRTC; no Rust WebRTC data-channel stack fits Android here. Closest: backup export/import |
| Offline library + Clear | ✅ | R | |
| Clear cache (temporary data) | 🟡 | C | clears cached cover images (the original clears WebView temp data, which the port doesn't have) |
| Song storage limit (1/2/5/10 GB) | ✅ | R | `pulse_cache_limit_mb` |
| Music folder | 🟡 | R | fixed folders, scan button; no folder picker |
| Download folder | ❌ | | no "Download" action for songs yet |
| Back up this profile / Restore a backup | ✅ | R | |
| Export all playlists / Import playlists (JSON) | ✅ | C | backup-core playlistFile / parsePlaylistFile, saved to Downloads |
| Lightweight thumbnails | ✅ | R | key fixed to `pulse_adv_light_thumbs` (240px vs 600px covers) |
| Free memory in background | 🟡 | R | saved; needs Android onStop via JNI |
| Visualizer frame rate | 🟡 | R | Native/60/30 saved; no visualizer |
| Required tools | — | | nothing to install in the port |
| Stream timeout | ✅ | C | `pulse_stream_timeout` 5–30 s (default 12) bounds finding + fetching the stream |
| Clip effects to panels | 🟡 | R | saved |
| About: version, updates, shortcuts, diagnostics, logs, legal | ✅ | R | diagnostics saved to a file; update check disabled (note) |

## Audio panel (customization)
| Item | Status | Verified | Note |
|---|---|---|---|
| Speed & pitch presets + sliders + volume gain | ✅ | R | speed-only presets shift pitch (no time-stretch: ADR-011) |
| Save/rename custom speed presets | ❌ | | |
| "For this track only" | ❌ | | track-audio.js not ported |
| Sleep timer | ✅ | R | |

## Animations (phone-visible)
Method: every @keyframes in styles.css + mobile.css (54) and the transitions on phone-visible elements;
each mapped to its trigger, timing and easing. Implemented with Slint's own `animate`, `Timer` and
`animation-tick()` (no extra animation framework).

| Animation | Original | Status | Verified | Note |
|---|---|---|---|---|
| Screen change | m3ContainerOpen 400ms cubic-bezier(.2,0,0,1): fade + 16px up + scale .96 | 🟡 | R | fade + 16px up; scale omitted (scaling the whole page container blurs text in Slint's software path) |
| Leaving screen | m3ContainerClose 280ms | ❌ | | Slint `if` swaps pages instantly; would need both pages alive at once |
| Settings section in | settingsIn .2s ease (6px) | 🟡 | R | fade (no 6px shift) |
| Drawer open/close | transform translateY(100%) 320ms | ✅ | R | |
| Drawer backdrop | gradient --np-c1 38% → --np-c2 16% → transparent 82% over #1A1B1E | ✅ | R | c1 = cover average, c2 = c1 lifted 33% to white (the original picks two cover colours) |
| Ambient drift blobs | np-drift-a/b/c | — | | hidden on phones in the original (`display:none !important`) |
| Toast in/out | toastIn 240ms / toastOut 180ms; phone position above play bar | ✅ | C | |
| Like pop | np-like-pop .35s cubic-bezier(.2,.8,.2,1.4) | ✅ | R | two-step 140ms up / 210ms back |
| Cover change | artIn 320ms (fade, 1.02→1) / artGhostOut 240ms | 🟡 | C | new cover fades/settles in; old one doesn't ghost out separately |
| Logo sparkle while playing | sparkleMain 2.6s, sparkleBlink 1.3s | ✅ | C | keyframes approximated piecewise |
| Tap ripple | rippleOut .55s cubic-bezier(.2,0,0,1) | ✅ | C | buttons, pills, chips, rows, nav |
| Press scale | :active scale(.95) 250ms +80ms delay; nav scale(.92) 200ms overshoot | ✅ | C | |
| Audio sheet in | audioPanelIn .22s 6px | ✅ | R | |
| Loading spinner | m3 spinner (rotate + dash 1.4s) | 🟡 | C | rotating arc; no dash growth |
| Sleep timer dot | sleepDot 2s | ✅ | C | |
| Wavy progress | rAF wave | ✅ | R | |
| Lyrics (phone) | 25px/800 left, opacity max(.22,.42−dist·.05), active 1 + scale 1.025, .45s | ✅ | R | |
| Lyrics compact header when lyrics show | 56px cover + left title/artist | ✅ | R | |
| Karaoke word fill | active-line words light up as sung (.22s) | ❌ | | Slint Text can't colour per word; needs a word-flow layout |
| Lyrics animation style/speed settings | pulse_lyrics_ease / pulse_lyrics_speed | 🟡 | C | speed drives the lyric scroll/fade/scale duration; easing fixed (see Tracks) |
| Skeleton shimmer | skeleton-shimmer 1.8s | — | | only on My Wave (not ported) |
| Queue badge restore | queueBadgeRestore 700ms | — | | desktop playlists-drawer badge |
| Home sections in / mix cards | homeInAnim 260ms, m3FadeUp 400ms | 🟡 | R | covered by the screen-change fade |
| Nav pill | transition 150ms | ✅ | R | |

## Not on phones (desktop-only in the original, not ported)
Hide top titlebar, Shaped cover in bottom bar, Download folder (phone variant differs), Hardware
acceleration, Minimize to tray, Legacy media keys, Playbar visualizer position/opacity, Connected liquid
visualizer, Battery saver, Show on Discord, Plugins.
