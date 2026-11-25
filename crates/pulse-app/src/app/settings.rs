//! Settings: every row of the original's Settings screen (index.html #view-settings, phone build),
//! with its storage key, values and default as app.js keeps them. Rust describes the rows; the UI
//! draws them (app.slint SetItemView). A row this version can't act on says so in its note; its value
//! is still saved under the original key, so backups move it to and from the original app.

use super::*;
use pulse_core::theme;

/// How a setting is stored (the formats app.js uses).
#[derive(Clone, Copy)]
enum Kind {
    /// `pulse_adv_<name>`: 'true' / 'false' (app.js getAdv / setAdv)
    Adv(bool),
    /// '1' / '0'; `default` when missing
    OneZero(bool),
    /// 'true' / 'false'; `default` when missing
    TrueFalse(bool),
    /// a number, clamped
    Num(f64, f64, f64),
    /// one of these strings (in the original's order); the default's index
    Choice(&'static [&'static str], usize),
}

/// The settings that are a plain stored value (switches, sliders, choices), by storage key.
const SIMPLE: &[(&str, Kind)] = &[
    ("devsize_ui_scale", Kind::Num(100.0, 80.0, 130.0)),
    ("pulse_font_scale", Kind::Num(100.0, 80.0, 130.0)),
    ("pulse_font", Kind::Choice(&["google", "sf", "system"], 0)),
    ("pulse_icon_pack", Kind::Choice(&["material", "sf", "lucide", "solar"], 0)),
    ("pulse_corner_radius", Kind::Num(16.0, 0.0, 32.0)),
    ("pulse_no_hand_cursor", Kind::OneZero(true)),
    ("pulse_search_pos", Kind::Choice(&["left", "middle", "right"], 1)),
    ("pulse_platform", Kind::Choice(&["youtube", "ytmusic", "soundcloud"], 0)),
    ("pulse_compact_engine_btns", Kind::OneZero(false)),
    ("pulse_hide_engine_btns", Kind::OneZero(false)),
    ("pulse_search_artists", Kind::OneZero(true)),
    ("albums", Kind::Adv(true)),
    ("pulse_show_local", Kind::TrueFalse(true)),
    ("pulse_show_albums", Kind::OneZero(false)),
    ("pulse_show_artists", Kind::OneZero(false)),
    ("pulse_home_layout", Kind::Choice(&["pulse", "spotify"], 1)),
    ("jump_back", Kind::Adv(true)),
    ("fav_artists", Kind::Adv(true)),
    ("my_wave", Kind::Adv(true)),
    ("my_wave_ambient", Kind::Adv(true)),
    ("pulse_wavy_progress", Kind::TrueFalse(true)),
    ("pulse_progress_knob", Kind::TrueFalse(true)),
    ("pulse_progress_bg", Kind::TrueFalse(true)),
    ("pulse_bar_wave", Kind::OneZero(true)),
    ("pulse_bar_viz_pos", Kind::Choice(&["mini", "behind"], 1)),
    ("pulse_bar_viz_opacity", Kind::Num(22.0, 5.0, 85.0)),
    ("pulse_viz_connected", Kind::OneZero(false)),
    ("pulse_tvk_visualizer", Kind::TrueFalse(true)),
    ("pulse_tvk_visualizer_lyrics", Kind::TrueFalse(false)),
    ("pulse_tvk_visualizer_opacity", Kind::Num(85.0, 10.0, 100.0)),
    ("pulse_viz_smooth", Kind::Num(70.0, 0.0, 95.0)),
    ("pulse_viz_sens", Kind::Num(100.0, 30.0, 300.0)),
    ("pulse_perf_mode", Kind::OneZero(false)),
    ("reduce_motion", Kind::Adv(false)),
    ("blur_fx", Kind::Adv(true)),
    ("pulse_theme_mode", Kind::Choice(&["light", "dark", "auto"], 1)),
    ("pulse_palette_style", Kind::Choice(&["classic", "tonal-spot", "expressive", "vibrant", "neutral"], 0)),
    ("pulse_theme_intensity", Kind::Num(0.0, 0.0, 100.0)),
    ("intensity_bg", Kind::Adv(false)),
    ("pulse_theme_brightness", Kind::Num(50.0, 0.0, 100.0)),
    ("pulse_highlight_buttons", Kind::OneZero(true)),
    ("pulse_ui_alpha", Kind::Num(20.0, 20.0, 100.0)),
    ("pulse_text_alpha", Kind::Num(100.0, 1.0, 100.0)),
    ("glass_blur", Kind::Adv(false)),
    ("pulse_glass_blur", Kind::Num(16.0, 4.0, 40.0)),
    ("pulse_bg_opacity", Kind::Num(100.0, 5.0, 100.0)),
    ("pulse_bg_blur", Kind::Num(0.0, 0.0, 40.0)),
    ("cover_bg", Kind::Adv(true)),
    ("pulse_cover_bg_brightness", Kind::Num(45.0, 5.0, 100.0)),
    ("mesh_follow_intensity", Kind::Adv(true)),
    ("mesh_cover_color", Kind::Adv(false)),
    ("ui_alpha_auto", Kind::Adv(true)),
    ("pulse_crossfade", Kind::Num(0.0, 0.0, 12.0)),
    ("pulse_smooth_presets", Kind::TrueFalse(true)),
    ("pulse_persist_sound_settings", Kind::OneZero(true)),
    ("pulse_show_explicit", Kind::OneZero(true)),
    ("pulse_lyrics_source", Kind::Choice(&["lrclib", "genius"], 0)),
    ("pulse_word_source", Kind::Choice(&["auto", "netease", "estimate"], 0)),
    ("online_lyrics", Kind::Adv(true)),
    ("pulse_tr_enabled", Kind::TrueFalse(false)),
    ("pulse_tr_lang", Kind::Choice(&["en", "ru", "es", "de", "fr", "pt", "ja", "ko", "zh-CN"], 0)),
    ("pulse_tr_auto", Kind::TrueFalse(false)),
    ("pulse_lyrics_ease", Kind::Choice(&["inout", "quad", "sine", "quart", "soft", "expo", "linear"], 0)),
    ("pulse_lyrics_speed", Kind::Num(1.0, 0.5, 2.5)),
    ("karaoke_words", Kind::Adv(true)),
    ("pulse_lyrics_word_style", Kind::Choice(&["gradient", "words"], 0)),
    ("pulse_lyrics_gradient_tightness", Kind::Num(2.0, 2.0, 24.0)),
    ("lyrics_autoscroll", Kind::Adv(true)),
    ("light_thumbs", Kind::Adv(true)),
    ("trim_bg", Kind::Adv(true)),
    ("pulse_fps_cap", Kind::Choice(&["0", "60", "30"], 0)),
    ("pulse_stream_timeout", Kind::Num(12.0, 5.0, 30.0)),
    ("clip_fx", Kind::Adv(false)),
];

/// The accent swatches (index.html .theme-swatch).
pub const SWATCHES: [&str; 7] = ["#B69DF8", "#7AA7FF", "#56C5C5", "#6DD58C", "#F5B83D", "#F28B82", "#F48FB1"];

fn kind_of(id: &str) -> Option<Kind> {
    SIMPLE.iter().find(|(k, _)| *k == id).map(|(_, k)| *k)
}

fn store_key(id: &str, kind: Kind) -> String {
    match kind {
        Kind::Adv(_) => format!("pulse_adv_{id}"),
        _ => id.to_string(),
    }
}

fn hex_color(hex: &str) -> Color {
    let (r, g, b) = theme::hex_to_rgb(hex);
    Color::from_rgb_u8(r, g, b)
}

/// `rgba(r, g, b, a)` or `#rrggbb`.
fn css_color(s: &str) -> Color {
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|x| x.strip_suffix(')')) {
        let v: Vec<f32> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if v.len() == 4 {
            return Color::from_argb_u8((v[3] * 255.0).round() as u8, v[0] as u8, v[1] as u8, v[2] as u8);
        }
    }
    hex_color(s)
}

/// A row being described.
struct Row(SetItem);

impl Row {
    fn new(id: &str, kind: i32, label: &str, desc: &str) -> Self {
        Self(SetItem { id: id.into(), kind, label: label.into(), desc: desc.into(), ..Default::default() })
    }
    fn note(mut self, n: &str) -> Self {
        self.0.note = n.into();
        self
    }
    fn options(mut self, o: &[&str]) -> Self {
        let v: Vec<SharedString> = o.iter().map(|s| (*s).into()).collect();
        self.0.options = ModelRc::new(VecModel::from(v));
        self
    }
    fn disabled(mut self, d: bool) -> Self {
        self.0.disabled = d;
        self
    }
}

type Group = (String, String, Vec<SetItem>);

fn group(title: &str, hint: &str, rows: Vec<Row>) -> Group {
    (title.into(), hint.into(), rows.into_iter().map(|r| r.0).collect())
}

const NOT_HERE: &str = "Not in this version yet. Your choice is saved, and kept in backups.";

impl App {
    // ---------- values ----------

    fn flag(&self, id: &str) -> bool {
        match kind_of(id) {
            Some(Kind::Adv(d)) => self.adv(id, d),
            Some(Kind::OneZero(d)) => match self.store.get(id) {
                None => d,
                // app.js reads '1' (or 'true') as on, everything else as off; default-on ones: !== '0'
                Some(v) => if d { v != "0" && v != "false" } else { v == "1" || v == "true" },
            },
            Some(Kind::TrueFalse(d)) => match self.store.get(id) {
                None => d,
                Some(v) => if d { v != "false" } else { v == "true" },
            },
            _ => false,
        }
    }

    fn num(&self, id: &str) -> f64 {
        match kind_of(id) {
            Some(Kind::Num(d, lo, hi)) => self.store.number(id, d).clamp(lo, hi),
            _ => 0.0,
        }
    }

    /// The index of a choice setting's stored value (the default when missing or unknown).
    fn choice(&self, id: &str) -> usize {
        match kind_of(id) {
            Some(Kind::Choice(vals, d)) => self
                .store
                .get(id)
                .and_then(|v| vals.iter().position(|x| *x == v))
                .unwrap_or(d),
            _ => 0,
        }
    }

    fn choice_str(&self, id: &str) -> &'static str {
        match kind_of(id) {
            Some(Kind::Choice(vals, _)) => vals[self.choice(id)],
            _ => "",
        }
    }

    // ---------- rows ----------

    fn toggle(&self, id: &str, label: &str, desc: &str) -> Row {
        let mut r = Row::new(id, 1, label, desc);
        r.0.on = self.flag(id);
        r
    }

    fn slider(&self, id: &str, label: &str, desc: &str, step: f64, text: String, ends: &[&str]) -> Row {
        let (lo, hi) = match kind_of(id) {
            Some(Kind::Num(_, lo, hi)) => (lo, hi),
            _ => (0.0, 100.0),
        };
        let mut r = Row::new(id, 2, label, desc).options(ends);
        r.0.value = self.num(id) as f32;
        r.0.min = lo as f32;
        r.0.max = hi as f32;
        r.0.step = step as f32;
        r.0.text = text.into();
        r
    }

    fn pct(&self, id: &str) -> String {
        format!("{}%", self.num(id).round())
    }

    /// A choice shown as chips (3) or a dropdown (4) with these labels, in the order of its values.
    fn pick(&self, id: &str, kind: i32, label: &str, desc: &str, labels: &[&str]) -> Row {
        let mut r = Row::new(id, kind, label, desc).options(labels);
        r.0.value = self.choice(id) as f32;
        r
    }

    fn buttons(id: &str, label: &str, desc: &str, labels: &[&str]) -> Row {
        Row::new(id, 5, label, desc).options(labels)
    }

    fn info(label: &str, desc: &str) -> Row {
        Row::new("", 0, label, desc)
    }

    /// Settings > `section` (1-based, the original's order), group by group.
    pub(super) fn settings_groups(&self, section: i32) -> Vec<Group> {
        match section {
            1 => self.interface_groups(),
            2 => self.theme_groups(),
            3 => self.tracks_groups(),
            4 => vec![
                group("Listening activity", "", vec![
                    Self::buttons("lastfm", "Last.fm scrobbling", "Send the songs you play to your Last.fm profile", &["Connect"])
                        .disabled(true)
                        .note("Not in this version yet: Last.fm needs Pulse's API key and a sign-in page."),
                ]),
                group("Import playlists from", "", vec![
                    Self::buttons("import_spotify", "Spotify", "", &["Import"]).disabled(true).note("Not in this version yet (it needs a Spotify sign-in)."),
                    Self::buttons("import_link", "YouTube", "Paste a playlist link into the search bar", &["Import"]),
                    Self::buttons("import_link", "YouTube Music", "Paste a playlist link into the search bar", &["Import"]),
                    Self::buttons("import_sc", "SoundCloud", "", &["Import"]).disabled(true).note("Not in this version yet. A SoundCloud song link in the search bar plays that song."),
                    Self::buttons("import_apple", "Apple Music", "", &["Import"]).disabled(true).note("Not in this version yet."),
                    Self::buttons("import_yandex", "Yandex Music", "", &["Import"]).disabled(true).note("Not in this version yet."),
                    Self::buttons("import_json", "Playlist file (JSON)", "A Pulse or Riff playlist file, or a list of songs", &["Import"]),
                ]),
            ],
            // Settings > Profiles is drawn by app.slint (rows, the form) from app/profiles.rs
            5 => Vec::new(),
            6 => vec![group("Your music on all your devices", "Link your phone and computer: playlists, favorites and history follow you.", vec![
                Self::buttons("sync_code", "Show a code", "Scan it with Pulse on another device", &["Show a code"])
                    .disabled(true)
                    .note("Not in this version yet: linking needs Pulse's sync server. To move your library, back it up on one device and restore it on the other (Storage)."),
                Self::info("Library", "Playlists and favorites"),
                Self::info("History and stats", "What you played"),
                Self::info("Theme", "Colors and look"),
                Self::info("Continue listening", "Pick up where you left off on the other device"),
            ])],
            7 => self.storage_groups(),
            8 => vec![
                group("Performance tuning", "", vec![
                    self.toggle("light_thumbs", "Lightweight thumbnails", "Load small cover images in lists instead of full HD frames. Biggest RAM saver for long lists"),
                    self.toggle("trim_bg", "Free memory in background", "Drop cached covers while Pulse is in the background").note(NOT_HERE),
                    self.pick("pulse_fps_cap", 3, "Visualizer frame rate", "", &["Native", "60", "30"]).note("This version has no visualizer; the choice is kept for the original app."),
                ]),
                group("System", "", vec![
                    Self::info("Required tools", "Built in: no yt-dlp or ffmpeg needed"),
                    self.slider("pulse_stream_timeout", "Stream timeout", "How long a song may sit without loading anything before Pulse gives up on it", 1.0, format!("{}s", self.num("pulse_stream_timeout")), &["5s", "30s"]),
                    self.toggle("clip_fx", "Clip effects to panels", "Keep glows and shadows inside their cards").note(NOT_HERE),
                ]),
            ],
            9 => vec![group("", "Plugins are JavaScript files with full access to Pulse. Only add plugins you trust.", vec![
                Self::buttons("plugins", "Plugins", "No plugins", &["Open folder", "Reload list", "Add"])
                    .disabled(true)
                    .note("The Rust version of Pulse can't run JavaScript plugins."),
            ])],
            10 => vec![group("", "", vec![
                {
                    let (desc, button) = self.updates_row();
                    Self::buttons("updates", "Updates", &desc, &[button.as_str()]).disabled(self.update.busy)
                },
                {
                    let mut r = Row::new("pulse_update_beta", 1, "Get beta versions", "Bug fixes arrive sooner; they are tested less than full releases");
                    r.0.on = self.store.get("pulse_update_beta") == Some("1");
                    r
                },
                Self::info("Keyboard shortcuts", "Space play/pause, arrows seek and change the volume (with a keyboard attached)"),
                Self::buttons("diagnostics", "Diagnostics", &self.ui.get_diagnostics(), &["Save diagnostics"]),
                Self::info("Logs", "Errors go to the Android log (tag \"pulse\"): adb logcat -s pulse"),
                Self::info("Legal", "Pulse is a player: it doesn't host, upload or sell music. Use it for private listening. Not affiliated with YouTube, Google, SoundCloud or Spotify. MIT license; a Rust version of Pulse by rootscripts. Made with Slint (slint.dev)."),
            ])],
            _ => Vec::new(),
        }
    }

    fn interface_groups(&self) -> Vec<Group> {
        let corner = self.num("pulse_corner_radius");
        vec![
            group("Language", "", vec![
                Row::new("lang", 4, "Language", "").options(&["English (en)"])
                    .note("English only: the original translates its menus with Google Translate on first use."),
            ]),
            group("Layout & size", "", vec![
                self.slider("devsize_ui_scale", "UI scale", "", 5.0, self.pct("devsize_ui_scale"), &[]),
                self.slider("pulse_font_scale", "Font scale", "", 5.0, self.pct("pulse_font_scale"), &[]),
                self.pick("pulse_font", 3, "Interface font", "", &["Google Sans", "SF Pro", "System"])
                    .note("Google Sans and SF Pro are used when the phone has them; otherwise the system font."),
                self.pick("pulse_icon_pack", 4, "Icon style", "Choose the icon pack used across navigation and player controls", &["Google Symbols (Default)", "iOS / SF Symbols", "Lucide Crisp", "Solar Duotone"])
                    .note("This version draws one icon set; the choice is kept for the original app."),
                self.slider("pulse_corner_radius", "Corner radius", "", 4.0, if corner == 16.0 { "Default".into() } else { format!("{corner}px") }, &["Sharp", "Round"]),
                self.toggle("pulse_no_hand_cursor", "No-Hand cursor", "Keep the arrow pointer over buttons").note("Only matters with a mouse."),
            ]),
            group("Search bar", "", vec![
                self.pick("pulse_search_pos", 4, "Position of searchbar", "", &["Left", "Middle", "Right"]),
                self.pick("pulse_platform", 4, "Search engine", "", &["YouTube", "YouTube Music", "SoundCloud"]),
                self.toggle("pulse_compact_engine_btns", "Compact engine buttons", "Show only icons for YouTube, YouTube Music, and SoundCloud in the top bar"),
                self.toggle("pulse_hide_engine_btns", "Hide engine buttons", "Hide search engine buttons from the top bar"),
                self.toggle("pulse_search_artists", "Show artists in search", "Artists matching your search above the songs").note("Search shows songs only in this version."),
                self.toggle("albums", "Albums", "Albums matching your search").note("Search shows songs only in this version."),
            ]),
            group("Tabs & Home", "", vec![
                self.toggle("pulse_show_local", "Show My files tab", ""),
                self.toggle("pulse_show_albums", "Show Albums tab", "").note("There is no Albums tab in this version yet."),
                self.toggle("pulse_show_artists", "Show Artists tab", "").note("There is no Artists tab in this version yet."),
                self.pick("pulse_home_layout", 3, "Home layout", "", &["Pulse", "Spotify"]).note("This version has the Spotify-style home only."),
                self.toggle("jump_back", "Jump back in", "The songs you played last, on the home page"),
                self.toggle("fav_artists", "Your artists", "").note(NOT_HERE),
                self.toggle("my_wave", "My Wave recommendations", "").note(NOT_HERE),
                self.toggle("my_wave_ambient", "Ambient light on home page", "").note(NOT_HERE),
            ]),
            group("Player bar", "", vec![
                self.toggle("pulse_wavy_progress", "Wavy progress bar", ""),
                self.toggle("pulse_progress_knob", "Progress bar knob", ""),
                self.toggle("pulse_progress_bg", "Progress bar background", "Show the uncompleted track rail in the playbar"),
                self.toggle("pulse_bar_wave", "Show wave visualizer in bottom bar", ""),
                self.pick("pulse_bar_viz_pos", 3, "Playbar visualizer position", "Show visualizer as a small icon on the right or wide behind the playbar", &["Compact", "Behind bar"]),
                self.slider("pulse_bar_viz_opacity", "Playbar visualizer opacity", "", 1.0, self.pct("pulse_bar_viz_opacity"), &[]),
                self.toggle("pulse_viz_connected", "Connected liquid visualizer", "Draw a smooth connected liquid wave instead of separate bars")
                    .note("This version has no audio visualizer yet; these four are kept for the original app."),
            ]),
            group("Visualizer", "This version has no audio visualizer yet; these are kept for the original app.", vec![
                self.toggle("pulse_tvk_visualizer", "Visualizer in full karaoke mode", ""),
                self.toggle("pulse_tvk_visualizer_lyrics", "Visualizer with lyrics", ""),
                self.slider("pulse_tvk_visualizer_opacity", "Visualizer opacity", "", 5.0, self.pct("pulse_tvk_visualizer_opacity"), &[]),
                self.slider("pulse_viz_smooth", "Visualizer smoothness", "", 1.0, self.pct("pulse_viz_smooth"), &[]),
                self.slider("pulse_viz_sens", "Visualizer sensitivity", "", 5.0, self.pct("pulse_viz_sens"), &[]),
            ]),
            group("Performance", "", vec![
                self.toggle("pulse_perf_mode", "Performance mode", "Turn off animations and moving effects for slower phones"),
                self.toggle("reduce_motion", "Reduce motion", "Screens, sheets and buttons change without animating"),
                self.toggle("blur_fx", "Blur effects", "").note("This version draws no blur (its renderer has no backdrop blur)."),
            ]),
        ]
    }

    fn theme_groups(&self) -> Vec<Group> {
        let auto = self.choice_str("pulse_theme_mode") == "auto";
        let accent = self.accent();
        let (r, g, b) = theme::hex_to_rgb(&accent);
        let hue = theme::rgb_to_hsl(r, g, b).0;
        let mut swatches = Row::new("accent", 6, "Accent color", "");
        swatches.0.colors = ModelRc::new(VecModel::from(SWATCHES.iter().map(|h| hex_color(h)).collect::<Vec<_>>()));
        swatches.0.value = SWATCHES
            .iter()
            .position(|s| s.eq_ignore_ascii_case(&accent))
            .map_or(-1.0, |i| i as f32);
        let mut hue_row = Row::new("hue", 2, "Hue", "");
        hue_row.0.value = hue as f32;
        hue_row.0.max = 360.0;
        hue_row.0.step = 1.0;
        hue_row.0.text = format!("{hue}°").into();
        vec![
            group("Colors", "", vec![
                self.pick("pulse_theme_mode", 3, "Theme mode", "", &["Light", "Dark", "Auto (album)"]),
                self.pick("pulse_palette_style", 4, "Palette style", "", &["Classic", "Tonal spot", "Expressive", "Vibrant", "Neutral"]),
                swatches,
                hue_row,
                self.slider("pulse_theme_intensity", "Intensity", "", 1.0, self.pct("pulse_theme_intensity"), &[]).disabled(auto),
                self.toggle("intensity_bg", "Apply intensity to background", "The background takes on the accent as strongly as the intensity"),
                self.slider("pulse_theme_brightness", "Brightness", "", 1.0, self.pct("pulse_theme_brightness"), &[]).disabled(auto),
                self.toggle("pulse_highlight_buttons", "Highlight buttons", "Fill the main button of each screen with the accent color"),
            ]),
            group("Transparency", "", vec![
                self.slider("pulse_ui_alpha", "Interface opacity", "", 1.0, self.pct("pulse_ui_alpha"), &[])
                    .note(if self.has_background() { "" } else { "Shows once a background picture is set (Background, below)." }),
                self.slider("pulse_text_alpha", "Text opacity", "", 1.0, self.pct("pulse_text_alpha"), &[]),
                Self::buttons("reset_alpha", "Reset", "Interface and text opacity back to their defaults", &["Reset"]),
                self.toggle("glass_blur", "Glass blur", "").note("This version draws no blur."),
                self.slider("pulse_glass_blur", "Glass blur strength", "", 1.0, format!("{}px", self.num("pulse_glass_blur")), &[]),
            ]),
            group("Background", "", vec![
                Self::buttons("bg_image", "Background", if self.store.get("pulse_bg_url").unwrap_or("").is_empty() { "A picture behind the app" } else { "A picture is set" }, &["Choose file", "Remove"])
                    .note("Pictures (JPEG, PNG, WebP, the first frame of a GIF). Videos aren't supported in this version."),
                self.slider("pulse_bg_opacity", "Opacity", "", 5.0, self.pct("pulse_bg_opacity"), &[]),
                self.slider("pulse_bg_blur", "Blur", "", 1.0, format!("{}px", self.num("pulse_bg_blur")), &[]),
                self.toggle("cover_bg", "Cover background", "The playing song's colors behind the player"),
                self.slider("pulse_cover_bg_brightness", "Cover background brightness", "", 5.0, self.pct("pulse_cover_bg_brightness"), &[]),
                self.toggle("mesh_follow_intensity", "Mesh follows theme intensity", "").note("No mesh gradient in this version yet."),
                self.toggle("mesh_cover_color", "Mesh color from current cover", "").note("No mesh gradient in this version yet."),
                self.toggle("ui_alpha_auto", "Lower interface opacity for backgrounds", "Setting a picture brings the interface opacity down to 50% (and back when it goes)"),
            ]),
            group("Home button icon", "", vec![
                Self::buttons("app_icon", "App icon on Home button", "Choose the icon displayed on the Home button", &["Custom icon..."]).disabled(true).note(NOT_HERE),
            ]),
        ]
    }

    fn tracks_groups(&self) -> Vec<Group> {
        let xf = self.num("pulse_crossfade");
        let blocked: Vec<String> = self.blocked_labels();
        let mut blocked_row = Row::new("unblock", 7, "Blocked artists and songs", if blocked.is_empty() {
            "Nothing blocked. Block an artist or a song from its menu or the artist page."
        } else {
            "Skipped when the queue moves on and kept out of My Wave and the mixes. Tap one to unblock it"
        });
        blocked_row.0.options = ModelRc::new(VecModel::from(blocked.into_iter().map(SharedString::from).collect::<Vec<_>>()));
        let genius = self.choice_str("pulse_lyrics_source") == "genius";
        vec![
            group("Playback", "", vec![
                self.slider("pulse_crossfade", "Crossfade", "Blend the end of a song into the next", 1.0, if xf == 0.0 { "Off".into() } else { format!("{xf}s") }, &[])
                    .note("Not in this version yet: its player plays one song at a time."),
                Self::info("Output device", "System default").note("Android sends sound to the speaker or headset it has chosen."),
                self.toggle("pulse_smooth_presets", "Smooth preset transitions", ""),
                self.toggle("pulse_persist_sound_settings", "Save sound settings", "Keep speed, pitch and volume gain between launches"),
                self.toggle("pulse_show_explicit", "Show explicit tag [E] on songs", "An [E] next to songs with explicit lyrics"),
                blocked_row,
            ]),
            group("Lyrics", "", vec![
                self.pick("pulse_lyrics_source", 4, "Lyrics source", "An .lrc file next to a song on your phone is used first", &["LRCLIB", "Genius"])
                    .note(if genius { "Genius isn't in this version yet; LRCLIB is used." } else { "" }),
                self.pick("pulse_word_source", 4, "Word-by-word source", "", &["Auto", "NetEase", "Estimate only"]).note("Word timing isn't in this version yet."),
                self.toggle("online_lyrics", "Online lyrics lookup", "Fetch synced and plain lyrics from network providers"),
                self.toggle("pulse_tr_enabled", "Lyrics translation", ""),
                self.pick("pulse_tr_lang", 4, "Translation language", "", &["English (en)", "Russian (ru)", "Spanish (es)", "German (de)", "French (fr)", "Portuguese (pt)", "Japanese (ja)", "Korean (ko)", "Chinese (zh-CN)"]),
                self.toggle("pulse_tr_auto", "Auto-translate", "").note("Lyrics translation isn't in this version yet (it uses Google Translate)."),
            ]),
            group("Lyrics display", "", vec![
                self.pick("pulse_lyrics_ease", 4, "Lyrics animation", "", &["Ease in-out (Cubic)", "Smooth (Quad)", "Gentle (Sine)", "Ease out (Quart)", "Soft ease out", "Snappy (Expo)", "Linear"])
                    .note("This version always moves lyrics with one curve (soft ease out); the speed below works."),
                {
                    let mut r = self.slider("pulse_lyrics_speed", "Lyrics animation speed", "", 0.1, format!("{:.1}x", self.num("pulse_lyrics_speed")), &["Slow", "Fast"]);
                    r.0.step = 0.1;
                    r
                },
                self.toggle("karaoke_words", "Karaoke word highlighting", "").note("Word timing isn't in this version yet."),
                self.pick("pulse_lyrics_word_style", 4, "Word mode style", "", &["Gradient", "Words"]),
                self.slider("pulse_lyrics_gradient_tightness", "Gradient tightness", "", 2.0, format!("{}", self.num("pulse_lyrics_gradient_tightness")), &[]),
                self.toggle("lyrics_autoscroll", "Auto-scroll synced lyrics", "Smoothly follow the active lyric line during playback"),
            ]),
        ]
    }

    fn storage_groups(&self) -> Vec<Group> {
        let limit = self.store.number("pulse_cache_limit_mb", 2048.0);
        let mut limit_row = Row::new("cache_limit", 3, "Song storage limit", "Songs you play are kept for offline use. Over this limit, the ones played longest ago are removed.")
            .options(&["1 GB", "2 GB", "5 GB", "10 GB"]);
        limit_row.0.value = [1024.0, 2048.0, 5120.0, 10240.0]
            .iter()
            .position(|m| (*m - limit).abs() < 1.0)
            .map_or(1.0, |i| i as f32);
        vec![
            group("Offline songs", "", vec![
                Self::buttons("clear_offline", "Offline library", &self.ui.get_cache_size(), &["Clear"]),
                Self::buttons("clear_temp", "Clear cache", "Temporary web and partial data", &["Clear"]),
                limit_row,
            ]),
            group("Folders", "", vec![
                Self::buttons("music_folder", "Music folder", &self.ui.get_music_folder(), &["Scan"])
                    .note("Pulse looks in Music and Download on this phone."),
            ]),
            group("Backups", "", vec![
                Self::buttons("export_backup", "Back up this profile", "Playlists, favorites, history and settings in one file (Downloads)", &["Export"]),
                Self::buttons("restore_backup", "Restore a backup", "A Pulse backup, or a Riff account file", &["Import"]),
                Self::buttons("export_playlists", "Export all playlists", "Every playlist in one JSON file (Downloads)", &["Export"]),
                Self::buttons("import_json", "Import playlists", "A Pulse or Riff playlist file, or a list of songs", &["Import"]),
            ]),
        ]
    }

    // ---------- the page ----------

    /// Shows the open section's rows, updating rows in place when only values changed (a slider
    /// being dragged keeps its grip).
    pub(super) fn show_settings_groups(&mut self) {
        let page = self.ui.get_settings_page();
        let groups = self.settings_groups(page);
        let same_shape = self.set_page_shown == page
            && self.set_models.len() == groups.len()
            && self
                .set_models
                .iter()
                .zip(&groups)
                .all(|(m, g)| m.row_count() == g.2.len());
        if same_shape {
            for (gi, (m, g)) in self.set_models.iter().zip(&groups).enumerate() {
                for (i, it) in g.2.iter().enumerate() {
                    if m.row_data(i).as_ref() != Some(it) {
                        m.set_row_data(i, it.clone());
                    }
                }
                let outer = self.ui.get_set_groups();
                if let Some(mut gd) = outer.row_data(gi)
                    && (gd.title.as_str() != g.0 || gd.hint.as_str() != g.1)
                {
                    gd.title = g.0.as_str().into();
                    gd.hint = g.1.as_str().into();
                    outer.set_row_data(gi, gd);
                }
            }
            return;
        }
        self.set_page_shown = page;
        self.set_models = groups
            .iter()
            .map(|g| Rc::new(VecModel::from(g.2.clone())))
            .collect();
        let data: Vec<SetGroupData> = groups
            .iter()
            .zip(&self.set_models)
            .map(|(g, m)| SetGroupData { title: g.0.as_str().into(), hint: g.1.as_str().into(), items: ModelRc::from(m.clone()) })
            .collect();
        self.ui.set_set_groups(ModelRc::new(VecModel::from(data)));
    }

    /// A row's value changed in the UI.
    pub(super) fn settings_change(&mut self, id: &str, v: f32) {
        let v = f64::from(v);
        match id {
            "accent" => {
                if let Some(hex) = SWATCHES.get(v as usize) {
                    self.set_accent(hex);
                }
            }
            "hue" => {
                let hex = theme::hsl_to_hex(v.round().clamp(0.0, 360.0), 55.0, 50.0);
                self.set_accent(&hex);
            }
            "cache_limit" => {
                let mb = [1024, 2048, 5120, 10240][(v as usize).min(3)];
                self.store.set("pulse_cache_limit_mb", mb.to_string());
            }
            "lang" => {}
            "pulse_update_beta" => {
                self.store.set("pulse_update_beta", if v != 0.0 { "1" } else { "0" });
                self.update.found = None;
                self.update.status.clear();
                self.persist();
                self.check_updates(true);
                self.show_settings_groups();
                return;
            }
            _ => {
                let Some(kind) = kind_of(id) else { return };
                let key = store_key(id, kind);
                let value = match kind {
                    Kind::Adv(_) | Kind::TrueFalse(_) => (v != 0.0).to_string(),
                    Kind::OneZero(_) => if v != 0.0 { "1" } else { "0" }.to_string(),
                    Kind::Num(_, lo, hi) => {
                        let x = v.clamp(lo, hi);
                        if id == "pulse_lyrics_speed" { format!("{:.1}", x) } else { js::num_to_string(x.round()) }
                    }
                    Kind::Choice(vals, _) => vals[(v as usize).min(vals.len() - 1)].to_string(),
                };
                self.store.set(&key, value);
                if id == "pulse_platform" {
                    self.ui.set_source(self.choice("pulse_platform") as i32);
                }
                if id == "pulse_theme_mode" && self.choice_str(id) != "auto" {
                    // leaving Auto: nothing else to undo, the stored accent comes back
                }
                if id == "light_thumbs" {
                    self.arts.clear();
                    self.refresh_rows();
                }
                if id == "pulse_show_local" && v == 0.0 && self.ui.get_page() == 2 {
                    self.nav(0);
                }
                if id == "jump_back" {
                    self.refresh_home();
                }
                if id == "pulse_bg_blur" {
                    self.load_background();
                }
            }
        }
        self.persist();
        self.apply_prefs();
        self.apply_theme();
        if id == "devsize_ui_scale" {
            self.apply_ui_scale();
        }
        self.show_settings_groups();
    }

    /// A row's button was pressed.
    pub(super) fn settings_action(&mut self, id: &str, i: usize) {
        match id {
            "unblock" => self.unblock(i),
            "import_link" => {
                self.nav(0);
                self.toast("Paste a YouTube or YouTube Music playlist link into the search bar");
                return;
            }
            "import_json" => self.import_playlist_file(),
            "export_playlists" => self.export_playlists(),
            "export_backup" => self.export_backup(),
            "restore_backup" => self.restore_backup(),
            "clear_offline" => self.clear_cache(),
            "clear_temp" => {
                self.arts.clear();
                self.covers.clear();
                self.pending_art.clear();
                self.refresh_rows();
                self.toast("Cache cleared");
            }
            "music_folder" => {
                self.toast("Looking for music on this device…");
                self.scan_local();
            }
            "reset_alpha" => {
                self.store.remove("pulse_ui_alpha");
                self.store.remove("pulse_text_alpha");
                self.persist();
                self.apply_theme();
            }
            "updates" => {
                if self.update.found.is_some() {
                    self.start_update();
                } else {
                    self.check_updates(true);
                }
            }
            "bg_image" if i == 0 => self.choose_background(),
            "bg_image" => self.remove_background(),
            "diagnostics" => {
                let text = format!("{}\n\n{}", self.ui.get_diagnostics(), journal_tail(60));
                match (self.platform.save_file)("pulse-diagnostics.txt", text.as_bytes()) {
                    Ok(w) => self.toast(&format!("Saved {w}")),
                    Err(e) => self.toast(&format!("Couldn't save: {e}")),
                }
            }
            _ => return,
        }
        self.refresh_settings();
    }

    // ---------- theme ----------

    fn accent(&self) -> String {
        self.store.get("pulse_theme_accent").unwrap_or("#6750A4").to_string()
    }

    /// A swatch or the hue: app.js leaves Auto for the mode it was showing.
    fn set_accent(&mut self, hex: &str) {
        self.store.set("pulse_theme_accent", hex.to_string());
        if self.choice_str("pulse_theme_mode") == "auto" {
            let shown = if self.ui.global::<Theme>().get_light() { "light" } else { "dark" };
            self.store.set("pulse_theme_mode", shown);
        }
    }

    /// app.js updateThemeFromState + applyTheme + applyUiOpacity: the colour tokens for the theme
    /// settings (Auto: from the playing song's cover).
    pub(super) fn apply_theme(&self) {
        let mode = self.choice_str("pulse_theme_mode");
        let style = self.choice_str("pulse_palette_style");
        let bg = self.flag("intensity_bg");
        let cover = self
            .current
            .as_ref()
            .and_then(|t| self.covers.get(&js::str_or_empty(t.get("thumbnail"))));
        let (accent, mode, intensity, brightness) = match (mode, cover) {
            ("auto", Some(a)) => {
                let (m, i, b) = theme::auto_theme(a);
                (a.accent.clone(), m, i, b)
            }
            ("auto", None) => (self.accent(), "dark", self.num("pulse_theme_intensity"), self.num("pulse_theme_brightness")),
            (m, _) => (self.accent(), m, self.num("pulse_theme_intensity"), self.num("pulse_theme_brightness")),
        };
        let p = theme::generate_palette(&accent, mode, intensity, brightness, style, bg);
        let light = mode == "light";
        // the surfaces the original takes from its own CSS between the M3 ones, on the same hue
        let (r, g, b) = theme::hex_to_rgb(&accent);
        let (h, hs, _) = theme::rgb_to_hsl(r, g, b);
        let ifac = intensity / 100.0;
        let mono = ifac < 0.08;
        let sf = ifac * theme::palette_style(style).surf_sat;
        let sat = if mono { 0.0 } else if bg { ((if hs == 0.0 { 60.0 } else { hs }) * sf * 2.4).round().min(85.0) } else { (6.0 * sf).round() };
        let br = brightness / 100.0;
        let tone = |v: f64| {
            let l = if light { (99.0 - (v - 7.0) * 0.7 + (br - 0.5) * 24.0).round() } else { (v * br * 2.0).round() };
            hex_color(&theme::hsl_to_hex(h, sat, l.clamp(0.0, 100.0)))
        };
        let text_a = (self.num("pulse_text_alpha") / 100.0) as f32;
        // app.js setToken: the surface tokens are color-mix(... var(--ui-alpha), transparent). It only
        // shows over a background picture, so only then
        let ui_a = if self.has_background() { (self.num("pulse_ui_alpha") / 100.0) as f32 } else { 1.0 };
        let t = self.ui.global::<Theme>();
        t.set_light(light);
        t.set_bg_solid(css_color(&p.background));
        t.set_bg(css_color(&p.background).with_alpha(ui_a));
        t.set_bar(css_color(&p.surface_container).with_alpha(ui_a));
        t.set_surface(css_color(&p.surface_container_low).with_alpha(ui_a));
        t.set_thumb(css_color(&p.surface_container_high).with_alpha(ui_a));
        t.set_field(tone(11.0).with_alpha(ui_a));
        t.set_tile(tone(12.0).with_alpha(ui_a));
        self.ui.set_bg_opacity((self.num("pulse_bg_opacity") / 100.0) as f32);
        t.set_chip(tone(18.0));
        t.set_menu(tone(16.5));
        t.set_sheet(tone(15.0));
        t.set_pill(tone(20.0));
        t.set_pill_pressed(tone(23.0));
        t.set_toast(tone(20.5));
        t.set_text(css_color(&p.on_surface).with_alpha(text_a));
        t.set_muted(css_color(&p.on_surface_variant).with_alpha(text_a));
        self.ui.global::<IconTint>().set_c(css_color(&p.on_surface_variant));
        t.set_faint(css_color(&p.outline));
        t.set_outline_variant(css_color(&p.outline_variant));
        // Highlight buttons off: the main buttons stay neutral instead of taking the accent
        let (prim, on_prim) = if self.flag("pulse_highlight_buttons") || mono {
            (css_color(&p.primary), css_color(&p.on_primary))
        } else if light {
            (hex_color("#111111"), hex_color("#FFFFFF"))
        } else {
            (hex_color("#FFFFFF"), hex_color("#121212"))
        };
        t.set_primary(prim);
        t.set_on_primary(on_prim);
        t.set_ov(css_color(&p.on_surface));
        t.set_fk((self.num("pulse_font_scale") / 100.0) as f32);
    }

    /// UI scale (devsize_ui_scale): the window's scale factor, relative to what the phone gave it.
    pub(super) fn apply_ui_scale(&mut self) {
        let want = self.num("devsize_ui_scale") as f32;
        if (want - self.ui_scale_applied).abs() < 0.5 {
            return;
        }
        let now = self.ui.window().scale_factor();
        let next = now * want / self.ui_scale_applied;
        self.ui_scale_applied = want;
        self.ui
            .window()
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: next });
    }

    // ---------- background picture ----------

    fn has_background(&self) -> bool {
        !self.store.get("pulse_bg_url").unwrap_or("").is_empty()
    }

    fn background_path(&self) -> PathBuf {
        self.platform.data_dir.join("background.img")
    }

    /// Choose file: the picture is kept in the app's own folder; pulse_bg_url points at it.
    fn choose_background(&mut self) {
        journal("background: picker opened");
        (self.platform.pick_image)(Box::new(|bytes| {
            journal(&format!("background: picked {} bytes", bytes.len()));
            later(move |a| {
                if image::guess_format(&bytes).is_err() {
                    journal("background: not a known picture format");
                    a.toast("That file isn't a picture Pulse can show");
                    return;
                }
                let path = a.background_path();
                if let Err(e) = std::fs::write(&path, &bytes) {
                    a.toast(&format!("Couldn't keep the picture: {e}"));
                    return;
                }
                a.store.set("pulse_bg_url", format!("file://{}", path.display()));
                a.background_changed(true);
                a.persist();
                a.load_background();
                a.apply_theme();
                a.refresh_settings();
            });
        }));
    }

    fn remove_background(&mut self) {
        if !self.has_background() {
            return;
        }
        self.store.remove("pulse_bg_url");
        let _ = std::fs::remove_file(self.background_path());
        self.background_changed(false);
        self.persist();
        self.load_background();
        self.apply_theme();
    }

    /// app.js onBackgroundChanged('custom', …): with "Lower interface opacity for backgrounds" on, a
    /// picture brings the interface opacity down to 50 (if above), and removing it puts it back.
    fn background_changed(&mut self, enabled: bool) {
        if !self.flag("ui_alpha_auto") {
            return;
        }
        let now = self.num("pulse_ui_alpha");
        if enabled {
            if now > 50.0 {
                self.store.set("pulse_ui_alpha_prev", js::num_to_string(now));
                self.store.set("pulse_ui_alpha", "50");
                self.store.set("pulse_ui_alpha_auto", "50");
                self.store.set("pulse_ui_alpha_auto_kind", "custom");
            }
            return;
        }
        match self.store.get("pulse_ui_alpha_auto_kind") {
            Some(k) if k != "custom" => return,
            // no kind: the old check, "no background left at all" (the cover background counts)
            None if self.flag("cover_bg") => return,
            _ => {}
        }
        let auto = self.store.get("pulse_ui_alpha_auto").and_then(|v| v.parse::<f64>().ok());
        let prev = self.store.get("pulse_ui_alpha_prev").map(str::to_string);
        if let (Some(auto), Some(prev)) = (auto, prev)
            && now == auto
        {
            self.store.set("pulse_ui_alpha", prev);
        }
        for k in ["pulse_ui_alpha_auto", "pulse_ui_alpha_prev", "pulse_ui_alpha_auto_kind"] {
            self.store.remove(k);
        }
    }

    /// Decodes the picture in the background (and blurs it: like app.js, at a quarter of the size with
    /// a quarter of the blur, then scaled back up), then shows it. Only the newest request lands.
    pub(super) fn load_background(&mut self) {
        self.bg_gen += 1;
        let ticket = self.bg_gen;
        let url = self.store.get("pulse_bg_url").unwrap_or("").to_string();
        let path = url.strip_prefix("file://").map(PathBuf::from).filter(|_| !url.is_empty());
        let Some(path) = path else {
            self.ui.set_bg_image(Image::default());
            return;
        };
        let blur = self.num("pulse_bg_blur") as f32;
        self.rt.spawn_blocking(move || {
            journal(&format!("background: decoding (blur {blur})"));
            let pixels = std::fs::read(&path).ok().and_then(|b| decode_picture(&b)).map(|img| {
                journal(&format!("background: decoded to {}x{}", img.width(), img.height()));
                if blur > 0.0 {
                    let small = image::imageops::resize(&img, (img.width() / 4).max(1), (img.height() / 4).max(1), image::imageops::FilterType::Triangle);
                    image::imageops::blur(&small, blur / 4.0)
                } else {
                    img
                }
            });
            later(move |a| {
                if a.bg_gen != ticket {
                    return;
                }
                journal("background: showing");
                match pixels {
                    Some(p) => a.ui.set_bg_image(Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(p.as_raw(), p.width(), p.height()))),
                    None => {
                        a.ui.set_bg_image(Image::default());
                        a.toast("Couldn't open the background picture");
                    }
                }
            });
        });
    }

    // ---------- playlists and profiles ----------

    fn export_playlists(&mut self) {
        let pls = self.store.list(PLAYLISTS);
        if pls.is_empty() {
            self.toast("No playlists to export");
            return;
        }
        let file = pulse_core::backup::playlist_file(&pls, APP_VERSION, &iso_now());
        let name = pulse_core::backup::file_name("pulse-playlists", "json");
        match (self.platform.save_file)(&name, serde_json::to_string_pretty(&file).unwrap_or_default().as_bytes()) {
            Ok(w) => self.toast(&if pls.len() == 1 { format!("Exported \"{}\" to {w}", js::str_or_empty(pls[0].get("name"))) } else { format!("Exported {} playlists to {w}", pls.len()) }),
            Err(e) => self.toast(&format!("Couldn't export: {e}")),
        }
    }

    /// backup.js importPlaylistFiles: songs known only by title and artist are looked up on YouTube first.
    fn import_playlist_file(&mut self) {
        self.ui.set_status("Choose a playlist file (.json)".into());
        (self.platform.pick_backup)(Box::new(|bytes| {
            later(move |a| {
                let text = String::from_utf8_lossy(&bytes).to_string();
                let found: Vec<Value> = pulse_core::backup::parse_playlist_file(&text)
                    .into_iter()
                    .filter(|p| !js::list(p.get("tracks")).is_empty())
                    .collect();
                if found.is_empty() {
                    a.toast("No playlists with songs in that file");
                    return;
                }
                let unmatched = found
                    .iter()
                    .flat_map(|p| js::list(p.get("tracks")).to_vec())
                    .filter(|t| t.get("needsMatch").is_some_and(js::truthy))
                    .count();
                if unmatched > 0 {
                    a.toast(&format!("Finding {} on YouTube…", plural(unmatched, "song")));
                }
                let net = a.net.clone();
                a.rt.spawn(async move {
                    let mut out = Vec::new();
                    let (mut missing, mut failures) = (0, 0);
                    for p in found {
                        let mut tracks = Vec::new();
                        for t in js::list(p.get("tracks")) {
                            if !t.get("needsMatch").is_some_and(js::truthy) {
                                tracks.push(t.clone());
                                continue;
                            }
                            if failures >= 8 {
                                missing += 1;
                                continue;
                            }
                            let q = format!("{} {}", js::str_or_empty(t.get("title")), js::str_or_empty(t.get("artist")));
                            match pulse_net::youtube::search(&net, js::trim(&q), 1).await {
                                Ok(r) if !r.is_empty() => {
                                    failures = 0;
                                    tracks.push(entry_track(&r[0], "youtube"));
                                }
                                Ok(_) => missing += 1,
                                Err(_) => {
                                    failures += 1;
                                    missing += 1;
                                }
                            }
                        }
                        if !tracks.is_empty() {
                            out.push((p, tracks));
                        }
                    }
                    later(move |a| {
                        if out.is_empty() {
                            a.toast("None of those songs could be found");
                            return;
                        }
                        let mut pls = a.store.list(PLAYLISTS);
                        let songs: usize = out.iter().map(|(_, t)| t.len()).sum();
                        let made = out.len();
                        for (n, (p, tracks)) in out.into_iter().enumerate() {
                            let id = pulse_core::backup::new_playlist_id(now_ms() as u64 + n as u64, a.seed.wrapping_add(n as u64));
                            let mut pl = json!({ "id": id, "name": js::str_or_empty(p.get("name")), "tracks": tracks, "createdAt": now_ms() });
                            for k in ["cover", "description"] {
                                if let Some(v) = p.get(k) {
                                    pl[k] = v.clone();
                                }
                            }
                            pls.push(pl);
                        }
                        a.store.set_json(PLAYLISTS, &Value::Array(pls));
                        a.persist();
                        a.refresh_all();
                        a.toast(&format!(
                            "Imported {} ({}){}",
                            plural(made, "playlist"),
                            plural(songs, "song"),
                            if missing > 0 { format!(", {missing} not found") } else { String::new() }
                        ));
                    });
                });
            });
        }));
    }

}

/// The longest side a background picture is kept at (object-fit: cover on a phone never needs more).
const BG_MAX: u32 = 1600;

/// A picture file -> pixels at most BG_MAX on a side, turned the way the photo's EXIF says (as a
/// browser shows it). JPEGs are decoded already shrunk (1/2, 1/4, 1/8 in the decoder, like browsers
/// do), so a 50 MP photo never has to fit in memory whole.
fn decode_picture(bytes: &[u8]) -> Option<image::RgbaImage> {
    use image::ImageDecoder;
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let format = reader.format();
    let orientation = reader
        .into_decoder()
        .ok()
        .and_then(|mut d| d.orientation().ok())
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = if format == Some(image::ImageFormat::Jpeg) {
        let mut d = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
        d.read_info().ok()?;
        let _ = d.scale(BG_MAX as u16, BG_MAX as u16);
        let data = match d.decode() {
            Ok(p) => p,
            Err(e) => {
                journal(&format!("background: decode failed: {e}"));
                return None;
            }
        };
        let info = d.info()?;
        let (w, h) = (u32::from(info.width), u32::from(info.height));
        match info.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, data)?),
            jpeg_decoder::PixelFormat::L8 => image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, data)?),
            jpeg_decoder::PixelFormat::L16 => {
                let v: Vec<u8> = data.chunks_exact(2).map(|c| c[0]).collect();
                image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, v)?)
            }
            jpeg_decoder::PixelFormat::CMYK32 => {
                let v: Vec<u8> = data
                    .chunks_exact(4)
                    .flat_map(|c| {
                        let k = 255 - u32::from(c[3]);
                        [c[0], c[1], c[2]].map(|x| ((255 - u32::from(x)) * k / 255) as u8)
                    })
                    .collect();
                image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, v)?)
            }
        }
    } else {
        let mut r = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
        // a broken or enormous file can't make Pulse allocate gigabytes
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(300 * 1024 * 1024);
        r.limits(limits);
        match r.decode() {
            Ok(i) => i,
            Err(e) => {
                journal(&format!("background: decode failed: {e}"));
                return None;
            }
        }
    };
    if img.width() > BG_MAX || img.height() > BG_MAX {
        img = img.resize(BG_MAX, BG_MAX, image::imageops::FilterType::Triangle);
    }
    img.apply_orientation(orientation);
    Some(img.to_rgba8())
}
