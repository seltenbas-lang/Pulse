//! The controller: owns the store, the player and the network client; turns UI callbacks into
//! actions and results back into UI properties. Lives on the UI thread (thread-local); async work
//! runs on a tokio runtime and comes back through `slint::invoke_from_event_loop`.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pulse_audio::dsp::EqSettings;
use pulse_audio::{Event, Growing, Player};
use pulse_core::{js, search_text, track_sort};
use pulse_net::lyrics::{self, Line};
use pulse_net::{Entry, http::Net};
use pulse_store::{FAVORITES, HISTORY, PLAYLISTS, Store};
use serde_json::{Value, json};
use std::rc::Rc;

use slint::{
    Color, ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString,
    VecModel,
};

use crate::{
    AppWindow, IconTint, LyricLine, Prefs, ProfileRow, SyncLine, SetGroupData, SetItem, SettingsSection, Theme, Tile, TrackRow,
};

mod lyrics_tools;
mod updates;
mod profiles;
mod settings;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const HISTORY_CAP: usize = 100;
const AUDIO_EXTS: &[&str] = &["mp3", "m4a", "aac", "flac", "ogg", "wav", "mp4"];
/// track-audio.js BUILTIN_PRESETS: (speed, speedPitch, pitch).
const SPEED_PRESETS: [(f64, f64, f64); 6] = [
    (1.0, 1.0, 0.0),
    (1.0, 0.8, 0.0),
    (1.0, 0.85, -1.0),
    (1.25, 1.0, 0.0),
    (1.0, 1.25, 0.0),
    (0.75, 1.0, 0.0),
];
/// The drawer's colour without a cover (the original's default banner tint).
/// --np-c1 without a cover (mobile.css fallback #6750A4); --np-c2 is derived from c1.
const DEFAULT_TINT: (u8, u8, u8) = (0x67, 0x50, 0xA4);

/// Lists the UI shows songs in (the `list` argument of its callbacks).
const RESULTS: i32 = 0;
const FAVS: i32 = 1;
const LOCAL: i32 = 2;
const HIST: i32 = 3;
const QUEUE: i32 = 4;
const REPEAT: i32 = 5;
const PLAYLIST: i32 = 6;

/// What the app plays now, for the system's media controls.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub artwork_url: String,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
}

/// Called back with a picked file's bytes, from any thread.
pub type OnPicked = Box<dyn FnOnce(Vec<u8>) + Send>;
/// Saves `bytes` as `name` where the person finds it; returns where.
pub type SaveFile = Box<dyn Fn(&str, &[u8]) -> Result<String, String>>;

/// Android's own player, used while the screen is off (compressed offload: the phone's audio
/// processor decodes, the CPU sleeps). Positions are in seconds.
pub trait Offload {
    /// Starts the file at `start`; false when Android can't play it.
    fn play(&self, path: &str, start: f64, volume: f32) -> bool;
    fn pause(&self);
    fn resume(&self);
    fn seek(&self, secs: f64);
    fn is_playing(&self) -> bool;
    /// Seconds into the song; None when nothing is loaded.
    fn position(&self) -> Option<f64>;
    fn release(&self);
}

/// What differs between Android and desktop.
pub struct Platform {
    /// Android's player for the screen-off time (None on desktop).
    pub offload: Option<Box<dyn Offload>>,
    pub data_dir: PathBuf,
    pub music_dirs: Vec<PathBuf>,
    /// Asks the person for a backup file; calls back with its bytes (from any thread).
    pub pick_backup: Box<dyn Fn(OnPicked)>,
    /// Asks the person for a picture (Settings > Theme > Background); calls back with its bytes.
    pub pick_image: Box<dyn Fn(OnPicked)>,
    /// Saves a file where the person finds it; returns where.
    pub save_file: SaveFile,
    /// Opens the system's installer for a downloaded update: "ok", "permission" (the person has to
    /// allow installing first; the settings page was opened), or what went wrong.
    pub install_apk: Box<dyn Fn(&Path) -> String>,
    /// Tells the system's media controls (notification, lock screen) what is playing.
    pub media: Box<dyn Fn(&MediaInfo)>,
    /// Commands from the system's media controls: play, pause, toggle, next, prev, seek:<s>.
    pub media_commands: Option<Receiver<String>>,
}

impl Platform {
    pub fn desktop() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let data_dir = std::env::var_os("PULSE_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share/pulse-rs"));
        let import = data_dir.join("import");
        Self {
            music_dirs: vec![home.join("Music")],
            pick_backup: Box::new(move |done| {
                // the newest file in <data>/import (or $PULSE_BACKUP)
                let pick = std::env::var_os("PULSE_BACKUP")
                    .map(PathBuf::from)
                    .or_else(|| {
                        let mut files: Vec<_> = std::fs::read_dir(&import)
                            .ok()?
                            .flatten()
                            .filter(|e| e.path().is_file())
                            .collect();
                        files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
                        files.last().map(|e| e.path())
                    });
                if let Some(bytes) = pick.and_then(|p| std::fs::read(p).ok()) {
                    done(bytes);
                }
            }),
            // desktop stand-in for the phone's picker: the file in $PULSE_BG_FILE
            pick_image: Box::new(|done| {
                if let Some(bytes) = std::env::var_os("PULSE_BG_FILE").and_then(|p| std::fs::read(p).ok()) {
                    done(bytes);
                }
            }),
            save_file: Box::new(move |name, bytes| {
                let dir = home.join("Downloads");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let p = dir.join(name);
                std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
                Ok(p.display().to_string())
            }),
            install_apk: Box::new(|p| format!("updates install on Android (downloaded to {})", p.display())),
            media: Box::new(|_| {}),
            media_commands: debug_commands(),
            offload: debug_offload(),
            data_dir,
        }
    }
}

/// Desktop testing of what only phones do: lines written to the file in $PULSE_CMD_FILE arrive as
/// the commands Android sends (background, foreground, pause, play, focus:…, offload-ended).
fn debug_commands() -> Option<Receiver<String>> {
    let path = PathBuf::from(std::env::var_os("PULSE_CMD_FILE")?);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut seen = 0;
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            for l in lines.iter().skip(seen) {
                if tx.send(l.trim().to_string()).is_err() {
                    return;
                }
                later(App::poll_commands);
            }
            seen = lines.len();
        }
    });
    Some(rx)
}

/// Desktop stand-in for Android's player ($PULSE_FAKE_OFFLOAD): keeps time, plays nothing.
fn debug_offload() -> Option<Box<dyn Offload>> {
    std::env::var_os("PULSE_FAKE_OFFLOAD")?;
    #[derive(Default)]
    struct Fake(std::sync::Mutex<Option<(f64, Option<Instant>)>>);
    impl Offload for Fake {
        fn play(&self, path: &str, start: f64, _v: f32) -> bool {
            journal(&format!("fake offload: play {path} from {start:.1}"));
            *self.0.lock().unwrap() = Some((start, Some(Instant::now())));
            true
        }
        fn pause(&self) {
            let mut g = self.0.lock().unwrap();
            if let Some((p, Some(t))) = *g {
                *g = Some((p + t.elapsed().as_secs_f64(), None));
            }
        }
        fn resume(&self) {
            let mut g = self.0.lock().unwrap();
            if let Some((p, None)) = *g {
                *g = Some((p, Some(Instant::now())));
            }
        }
        fn seek(&self, secs: f64) {
            let mut g = self.0.lock().unwrap();
            if let Some((_, t)) = *g {
                *g = Some((secs, t.map(|_| Instant::now())));
            }
        }
        fn is_playing(&self) -> bool {
            matches!(*self.0.lock().unwrap(), Some((_, Some(_))))
        }
        fn position(&self) -> Option<f64> {
            self.0.lock().unwrap().map(|(p, t)| p + t.map_or(0.0, |t| t.elapsed().as_secs_f64()))
        }
        fn release(&self) {
            *self.0.lock().unwrap() = None;
        }
    }
    Some(Box::new(Fake::default()))
}

#[derive(Clone, Copy, PartialEq)]
enum Repeat {
    Off,
    All,
    One,
}

/// What the open song menu is about.
#[derive(Clone)]
enum Menu {
    /// "…": play next, add to queue, (remove).
    Song {
        track: Value,
        list: i32,
        index: usize,
        removable: bool,
    },
    /// "+": add to one of the playlists, or a new one.
    AddTo { track: Value },
}

#[derive(Clone, Copy, PartialEq)]
enum Sleep {
    Off,
    At(Instant),
    EndOfTrack,
}

struct App {
    ui: AppWindow,
    platform: Platform,
    store: Store,
    player: Player,
    events: Receiver<Event>,
    rt: Arc<tokio::runtime::Runtime>,
    net: Arc<Net>,
    results: Vec<Value>,
    fav_view: Vec<Value>,
    fav_filter: String,
    local: Vec<Value>,
    local_view: Vec<Value>,
    local_filter: String,
    on_repeat: Vec<Value>,
    recents: Vec<Value>,
    playlist: Option<usize>,
    queue: Vec<Value>,
    index: Option<usize>,
    current: Option<Value>,
    token: u64,
    lyrics: Vec<Line>,
    lyric_index: i32,
    arts: HashMap<String, (Image, (u8, u8, u8))>,
    pending_art: HashSet<String>,
    repeat: Repeat,
    shuffle: bool,
    seed: u64,
    last_media: MediaInfo,
    loaded: bool,
    menu: Option<Menu>,
    sleep: Sleep,
    volume: f32,
    gain: f32,
    preset_tween: Option<slint::Timer>,
    /// each cover's colours (app.js analyzeCover), for the Auto (album) theme
    covers: HashMap<String, pulse_core::theme::CoverAnalysis>,
    /// the UI scale (devsize_ui_scale, %) the window's scale factor carries now
    ui_scale_applied: f32,
    /// the open settings section's rows, kept to update them in place
    set_models: Vec<Rc<VecModel<SetItem>>>,
    set_page_shown: i32,
    /// the newest background-picture load (older ones are dropped when they finish)
    bg_gen: u64,
    /// Settings > Profiles: the add / edit form being filled in
    prof_draft: Option<profiles::Draft>,
    /// Sync lyrics: the song being synced and each line's tapped time
    sync: Option<lyrics_tools::Sync>,
    /// the app's heartbeat (player events, the notification, the sleep timer, the screen)
    ticker: slint::Timer,
    ticker_ms: u64,
    /// in the background (Android onStop): no screen updates, a slow heartbeat or none
    background: bool,
    /// Android's player has the song (screen off, sound untouched)
    offloaded: bool,
    /// the playing song's file, and the one the app's engine has loaded
    current_file: Option<PathBuf>,
    engine_file: Option<PathBuf>,
    /// the equaliser does nothing (one of the conditions for handing over to Android's player)
    eq_flat: bool,
    /// Android's player turned this song down: don't keep asking (each try is a small hiccup)
    offload_refused: bool,
    /// songs in a row that couldn't be played (after three, playback stops)
    fail_streak: u32,
    /// songs downloading now, by cache name: playing one and fetching the next ahead share them
    downloads: Downloads,
    /// the playing song while it still downloads: its finished file, once it has all come in
    current_growing: Option<(PathBuf, Arc<Growing>)>,
    /// Settings > About > Updates
    update: updates::State,
    /// the question shown (`ask`), waiting for its answer
    asking: Option<updates::Ask>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

/// A command arrived from Android (another thread): handle it now, even with the heartbeat stopped.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn wake() {
    later(App::poll_commands);
}

/// Runs `f` on the UI thread with the app.
fn later(f: impl FnOnce(&mut App) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        with(f);
    });
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

pub fn clock_text(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "0:00".into();
    }
    let s = secs.round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The app's identity for a track (app.js getTrackCanonicalId).
pub fn canonical_id(t: &Value) -> String {
    let platform = js::or(t, "platform").map_or_else(|| "yt".to_string(), js::to_string);
    if let Some(id) = js::or(t, "id") {
        return format!("{platform}:{}", js::to_string(id));
    }
    if let Some(u) = js::or(t, "url").or_else(|| js::or(t, "videoId")) {
        return format!("{platform}:{}", js::to_string(u));
    }
    let t_ = js::trim(&js::str_or_empty(t.get("title"))).to_lowercase();
    let a = js::trim(&js::str_or_empty(t.get("artist"))).to_lowercase();
    format!(
        "{platform}:{t_}|{a}|{}",
        js::num_to_string(js::round(js::num_or_zero(t.get("duration"))))
    )
}

/// A search result as the app stores tracks.
pub fn entry_track(e: &Entry, platform: &str) -> Value {
    let artist = if e.artists.is_empty() {
        e.uploader.clone()
    } else {
        e.artists.join(", ")
    };
    let thumb = e
        .thumbnails
        .last()
        .map(|t| big_thumb(&t.url))
        .unwrap_or_else(|| {
            if platform == "soundcloud" {
                String::new()
            } else {
                format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", e.id)
            }
        });
    let mut t = json!({ "id": e.id, "title": e.title, "artist": artist, "duration": e.duration, "thumbnail": thumb, "url": e.webpage_url, "platform": platform });
    if let Some(c) = &e.channel_id {
        t["artistId"] = json!(c);
    }
    t
}

/// YouTube Music's thumbnails come as =w60-h60: ask for a size the player can show.
fn big_thumb(url: &str) -> String {
    match url.rfind("=w") {
        Some(i) if url.contains("googleusercontent.com") || url.contains("ggpht.com") => {
            format!("{}=w544-h544-l90-rj", &url[..i])
        }
        _ => url.to_string(),
    }
}

fn cache_name(t: &Value, ext: &str) -> String {
    let id: String = canonical_id(t)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    format!("{id}.{ext}")
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// The cover's colour, darkened, for the drawer's backdrop (the original blurs the cover in).
fn tint_of(rgba: &[u8]) -> (u8, u8, u8) {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for px in rgba.chunks_exact(4).step_by(7) {
        r += u64::from(px[0]);
        g += u64::from(px[1]);
        b += u64::from(px[2]);
        n += 1;
    }
    if n == 0 {
        return DEFAULT_TINT;
    }
    ((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

/// app.js analyzeCover: the cover drawn at 48x48, then its colours binned by hue.
fn cover_analysis(w: u32, h: u32, rgba: &[u8]) -> pulse_core::theme::CoverAnalysis {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let mut px = Vec::with_capacity(48 * 48 * 4);
    for y in 0..48 {
        for x in 0..48 {
            let i = ((y * h / 48).min(h - 1) * w + (x * w / 48).min(w - 1)) * 4;
            px.extend_from_slice(rgba.get(i..i + 4).unwrap_or(&[0, 0, 0, 255]));
        }
    }
    pulse_core::theme::analyze_cover(&px)
}

/// --np-c2: the cover colour lifted toward white (#6750A4 gives #9A82DB-like).
fn tint2_of(c: (u8, u8, u8)) -> Color {
    let up = |v: u8| (f32::from(v) + (255.0 - f32::from(v)) * 0.33) as u8;
    Color::from_rgb_u8(up(c.0), up(c.1), up(c.2))
}

/// Where `journal` writes (set once, at start).
static JOURNAL_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// A line in `<data>/pulse-log.txt` (and logcat on Android): what Pulse was doing, for the
/// diagnostics file when something goes wrong. Kept small.
pub fn journal(msg: &str) {
    #[cfg(target_os = "android")]
    crate::android::log(msg);
    let Some(dir) = JOURNAL_DIR.get() else { return };
    let path = dir.join("pulse-log.txt");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
        let _ = std::fs::rename(&path, dir.join("pulse-log.old.txt"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{} {msg}", iso_now());
    }
}

/// A crash: kept in `<data>/crash.txt` so the next start can say so (and diagnostics carry it).
fn record_crash(msg: &str) {
    journal(&format!("CRASH {msg}"));
    if let Some(dir) = JOURNAL_DIR.get() {
        let _ = std::fs::write(dir.join("crash.txt"), format!("{} {msg}\n", iso_now()));
    }
}

/// The last lines of the journal and the last crash, for Settings > About > Save diagnostics.
fn journal_tail(n: usize) -> String {
    let Some(dir) = JOURNAL_DIR.get() else { return String::new() };
    let log = std::fs::read_to_string(dir.join("pulse-log.txt")).unwrap_or_default();
    let lines: Vec<&str> = log.lines().collect();
    let crash = std::fs::read_to_string(dir.join("crash-last.txt")).unwrap_or_default();
    format!(
        "Last crash:\n{}\nRecent log:\n{}\n",
        if crash.is_empty() { "none\n" } else { &crash },
        lines[lines.len().saturating_sub(n)..].join("\n")
    )
}

pub fn run(platform: Platform) -> Result<(), String> {
    let _ = std::fs::create_dir_all(&platform.data_dir);
    let _ = JOURNAL_DIR.set(platform.data_dir.clone());
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_crash(&format!("panic: {info}"));
        default_hook(info);
    }));
    // the crash before this start: shown once, kept for diagnostics
    let crashed = std::fs::read_to_string(platform.data_dir.join("crash.txt")).ok();
    if crashed.is_some() {
        let _ = std::fs::rename(platform.data_dir.join("crash.txt"), platform.data_dir.join("crash-last.txt"));
    }
    journal(&format!("start {APP_VERSION}"));
    let ui = AppWindow::new().map_err(|e| e.to_string())?;
    let store = Store::open(platform.data_dir.join("pulse-store.json"))
        .map_err(|e| format!("store: {e}"))?;
    let (player, events) = Player::start()?;
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?,
    );
    let net = Arc::new(Net::new().map_err(|e| e.to_string())?);
    let app = App {
        ui: ui.clone_strong(),
        platform,
        store,
        player,
        events,
        rt,
        net,
        results: Vec::new(),
        fav_view: Vec::new(),
        fav_filter: String::new(),
        local: Vec::new(),
        local_view: Vec::new(),
        local_filter: String::new(),
        on_repeat: Vec::new(),
        recents: Vec::new(),
        playlist: None,
        queue: Vec::new(),
        index: None,
        current: None,
        token: 0,
        lyrics: Vec::new(),
        lyric_index: -1,
        arts: HashMap::new(),
        pending_art: HashSet::new(),
        repeat: Repeat::Off,
        shuffle: false,
        seed: now_ms() as u64 | 1,
        last_media: MediaInfo::default(),
        loaded: false,
        menu: None,
        sleep: Sleep::Off,
        volume: 0.85,
        gain: 1.0,
        preset_tween: None,
        covers: HashMap::new(),
        ui_scale_applied: 100.0,
        set_models: Vec::new(),
        set_page_shown: -1,
        bg_gen: 0,
        prof_draft: None,
        sync: None,
        ticker: slint::Timer::default(),
        ticker_ms: 0,
        background: false,
        offloaded: false,
        current_file: None,
        engine_file: None,
        eq_flat: true,
        offload_refused: false,
        fail_streak: 0,
        downloads: Arc::default(),
        current_growing: None,
        update: updates::State::default(),
        asking: None,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    ui.set_version(APP_VERSION.into());
    with(App::restore_settings);
    with(|a| {
        a.apply_prefs();
        a.apply_theme();
        a.load_background();
    });
    wire(&ui);
    // the scale factor is the phone's only once the window is shown
    slint::Timer::single_shot(Duration::from_millis(600), || {
        with(App::apply_ui_scale);
    });
    // a quiet look for a new version, at most once a day
    slint::Timer::single_shot(Duration::from_secs(8), || {
        with(|a| a.check_updates(false));
    });
    with(App::schedule_tick);
    with(|a| {
        a.refresh_all();
        a.scan_local();
    });
    if crashed.is_some() {
        with(|a| a.toast("Pulse closed unexpectedly last time. Settings → About → Save diagnostics has the details"));
    }
    let r = run_loop(&ui);
    with(|a| {
        a.save_queue();
        let _ = a.store.save();
    });
    r
}

/// The event loop. On Android a renderer error (the window's surface lost while another app, such as
/// the photo picker, was in front) would end it, and with it the app: it starts again instead and
/// picks the window up when Android hands it back.
fn run_loop(ui: &AppWindow) -> Result<(), String> {
    let mut failures = 0;
    loop {
        match ui.run() {
            Ok(()) => return Ok(()),
            Err(e) => {
                record_crash(&format!("event loop: {e}"));
                failures += 1;
                if !cfg!(target_os = "android") || failures > 20 {
                    return Err(e.to_string());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn wire(ui: &AppWindow) {
    ui.on_search(|q| {
        with(|a| a.search(q.to_string()));
    });
    ui.on_set_source(|i| {
        with(|a| {
            a.ui.set_source(i);
            a.store.set(
                "pulse_platform",
                ["youtube", "ytmusic", "soundcloud"][i.clamp(0, 2) as usize],
            );
            a.persist();
            let q = a.ui.get_query().to_string();
            if a.ui.get_page() == 6 && !q.is_empty() {
                a.search(q);
            }
        });
    });
    ui.on_play(|list, i| {
        with(|a| a.play_from(list, i as usize));
    });
    ui.on_add(|list, i| {
        with(|a| a.open_add_menu(list, i as usize));
    });
    ui.on_more(|list, i| {
        with(|a| a.open_song_menu(list, i as usize));
    });
    ui.on_fav(|list, i| {
        with(|a| {
            if let Some(t) = a.list(list).get(i as usize).cloned() {
                a.toggle_favorite_of(&t);
            }
        });
    });
    ui.on_menu_action(|i| {
        with(|a| a.menu_action(i as usize));
    });
    ui.on_tile_clicked(|i| {
        with(|a| a.tile_clicked(i as usize));
    });
    ui.on_playlist_clicked(|i| {
        with(|a| a.open_playlist(i as usize));
    });
    ui.on_new_playlist(|| {
        with(|a| {
            let i = a.new_playlist(None);
            a.open_playlist(i);
        });
    });
    ui.on_set_repeat_range(|i| {
        with(|a| {
            a.ui.set_repeat_range(i);
            a.refresh_home();
        });
    });
    ui.on_set_fav_sort(|i| {
        with(|a| {
            a.store.set(
                "pulse_favorites_sort",
                track_sort::TRACK_SORT_MODES
                    .get(i as usize)
                    .copied()
                    .unwrap_or("manual"),
            );
            a.persist();
            a.refresh_favorites();
        });
    });
    ui.on_filter(|list, q| {
        with(|a| {
            if list == FAVS {
                a.fav_filter = q.to_string();
                a.refresh_favorites();
            } else {
                a.local_filter = q.to_string();
                a.refresh_local();
            }
        });
    });
    ui.on_add_files(|| {
        with(|a| {
            a.toast("Looking for music on this device…");
            a.scan_local();
        });
    });
    ui.on_clear_history(|| {
        with(|a| {
            a.store.set_json(HISTORY, &json!([]));
            a.persist();
            a.refresh_all();
        });
    });
    ui.on_clear_queue(|| {
        with(|a| {
            // the playing song stays
            a.queue = a.current.clone().into_iter().collect();
            a.index = a.current.as_ref().map(|_| 0);
            a.save_queue();
            a.refresh_lists();
        });
    });
    ui.on_download_all(|| {
        with(App::download_all);
    });
    ui.on_restore_backup(|| {
        with(App::restore_backup);
    });
    ui.on_export_backup(|| {
        with(App::export_backup);
    });
    ui.on_clear_cache(|| {
        with(App::clear_cache);
    });
    ui.on_toggle_play(|| {
        with(App::toggle_play);
    });
    ui.on_next(|| {
        with(|a| a.next(true));
    });
    ui.on_prev(|| {
        with(App::prev);
    });
    ui.on_seek(|s| {
        with(|a| a.player.seek(f64::from(s)));
    });
    ui.on_toggle_favorite(|| {
        with(|a| {
            if let Some(t) = a.current.clone() {
                a.toggle_favorite_of(&t);
            }
        });
    });
    ui.on_cycle_repeat(|| {
        with(App::cycle_repeat);
    });
    ui.on_toggle_shuffle(|| {
        with(|a| {
            a.shuffle = !a.shuffle;
            a.store
                .set("pulse_shuffle", if a.shuffle { "1" } else { "0" });
            a.ui.set_shuffle(a.shuffle);
            a.persist();
        });
    });
    ui.on_volume_changed(|v| {
        with(|a| {
            a.volume = v;
            a.apply_volume();
            a.store.set("pulse_volume", js::num_to_string(f64::from(v)));
            a.persist();
        });
    });
    ui.on_audio_changed(|| {
        with(|a| a.audio_changed(true));
    });
    ui.on_audio_preset(|i| {
        with(|a| a.apply_preset(i as usize));
    });
    ui.on_audio_reset(|| {
        with(|a| {
            a.ui.set_speed(1.0);
            a.ui.set_speed_pitch(1.0);
            a.ui.set_pitch(0.0);
            a.ui.set_gain(1.0);
            a.audio_changed(true);
        });
    });
    ui.on_sleep(|m| {
        with(|a| a.set_sleep(m));
    });
    ui.on_nav(|p| {
        with(|a| a.nav(p));
    });
    ui.global::<Prefs>().on_set(|key, v| {
        with(|a| a.set_pref(&key, v));
    });
    ui.on_unblock(|i| {
        with(|a| a.unblock(i as usize));
    });
    ui.on_clear_temp(|| {
        with(|a| {
            // the original clears the WebView's temporary data; here: covers kept in memory
            a.arts.clear();
            a.pending_art.clear();
            a.refresh_rows();
            a.toast("Cache cleared");
        });
    });
    ui.on_open_settings(|_| {
        with(App::refresh_settings);
    });
    ui.on_settings_search(|q| {
        with(|a| a.ui.set_settings_sections(settings_sections(&q)));
    });
    ui.on_set_change(|id, v| {
        with(|a| a.settings_change(&id, v));
    });
    ui.on_set_action(|id, i| {
        with(|a| a.settings_action(&id, i.max(0) as usize));
    });
    ui.on_set_toggle(|which, on| {
        with(|a| a.set_toggle(which, on));
    });
    ui.on_set_cache_limit(|i| {
        with(|a| {
            let mb = [1024, 2048, 5120, 10240][i.clamp(0, 3) as usize];
            a.store.set("pulse_cache_limit_mb", mb.to_string());
            a.persist();
            a.refresh_settings();
        });
    });
    ui.on_import_link(|| {
        with(|a| {
            a.nav(0);
            a.toast("Paste a YouTube or YouTube Music playlist link into the search bar");
        });
    });
    ui.on_add_profile(|| {
        with(|a| a.start_profile_form(None));
    });
    ui.on_ask_answer(|yes| {
        with(|a| a.answered(yes));
    });
    ui.on_lyric_seek(|i| {
        with(|a| a.lyric_seek(i.max(0) as usize));
    });
    ui.on_lyrics_translate(|| {
        with(|a| a.toast("Lyrics translation isn't in this version yet (it uses Google Translate)"));
    });
    ui.on_lyrics_edit(|| {
        with(App::lyrics_edit);
    });
    ui.on_lyrics_save(|text| {
        with(|a| a.lyrics_save(&text));
    });
    ui.on_sync_start(|| {
        with(App::sync_start);
    });
    ui.on_sync_tap(|i| {
        with(|a| a.sync_tap(i.max(0) as usize));
    });
    ui.on_sync_restart(|| {
        with(App::sync_restart);
    });
    ui.on_sync_save(|| {
        with(App::sync_save);
    });
    ui.on_prof_start_add(|| {
        with(|a| a.start_profile_form(None));
    });
    ui.on_prof_start_edit(|i| {
        with(|a| a.start_profile_form(Some(i.max(0) as usize)));
    });
    ui.on_prof_switch(|i| {
        with(|a| a.open_profile(i.max(0) as usize));
    });
    ui.on_prof_name_edited(|t| {
        with(|a| a.profile_draft_name(&t));
    });
    ui.on_prof_pick_color(|i| {
        with(|a| a.profile_draft_color(i.max(0) as usize));
    });
    ui.on_prof_pick_emoji(|i| {
        with(|a| a.profile_draft_emoji(i.max(0) as usize));
    });
    ui.on_prof_save(|| {
        with(App::save_profile_form);
    });
    ui.on_prof_delete(|i| {
        with(|a| a.delete_profile_row(i.max(0) as usize));
    });
    ui.on_open_profile(|i| {
        with(|a| a.open_profile(i as usize));
    });
}

/// The settings list (index.html's order), each with words it answers to in Settings search.
const SECTIONS: [(&str, &str); 10] = [
    ("Interface", "language layout size scale font"),
    (
        "Theme",
        "colors colours dark light accent palette background icon",
    ),
    (
        "Tracks",
        "playback output device sound speed lyrics source crossfade",
    ),
    (
        "Integrations",
        "import playlists youtube music lastfm spotify link",
    ),
    ("Profiles", "profile people account switch"),
    ("Sync", "devices link phone computer"),
    (
        "Storage",
        "offline songs cache limit folder music backup export import restore",
    ),
    ("Advanced", "performance thumbnails memory tools"),
    ("Plugins", "plugin extensions"),
    ("About", "version diagnostics license legal"),
];

fn settings_sections(query: &str) -> ModelRc<SettingsSection> {
    let q = search_text::normalize_search(query);
    let icons: [(&str, &str); 10] = [
        (
            "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM3 9L21 9M9 21L9 9",
            "",
        ),
        (
            "M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.7-.8 1.7-1.7 0-.4-.2-.8-.4-1.1-.3-.3-.4-.7-.4-1.1 0-.9.8-1.7 1.7-1.7H16c3.3 0 6-2.7 6-6 0-4.9-4.5-8.4-10-8.4z",
            "M14 6.5a.5.5 0 1 1-1 0a.5.5 0 1 1 1 0M18 10.5a.5.5 0 1 1-1 0a.5.5 0 1 1 1 0M9 7.5a.5.5 0 1 1-1 0a.5.5 0 1 1 1 0M7 12.5a.5.5 0 1 1-1 0a.5.5 0 1 1 1 0",
        ),
        (
            "M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0a3 3 0 1 1 6 0M21 16a3 3 0 1 1-6 0a3 3 0 1 1 6 0",
            "",
        ),
        (
            "M12 22v-5M9 8V2M15 8V2M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z",
            "",
        ),
        (
            "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M13 7a4 4 0 1 1-8 0a4 4 0 1 1 8 0M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75",
            "",
        ),
        (
            "M3.5 4h9A1.5 1.5 0 0 1 14 5.5v6A1.5 1.5 0 0 1 12.5 13h-9A1.5 1.5 0 0 1 2 11.5v-6A1.5 1.5 0 0 1 3.5 4zM5 17h6M16.5 8h4A1.5 1.5 0 0 1 22 9.5v10A1.5 1.5 0 0 1 20.5 21h-4A1.5 1.5 0 0 1 15 19.5v-10A1.5 1.5 0 0 1 16.5 8zM18 18h1",
            "",
        ),
        (
            "M22 12H2M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11zM6 16h.01M10 16h.01",
            "",
        ),
        (
            "M21 4L14 4M10 4L3 4M21 12L12 12M8 12L3 12M21 20L16 20M12 20L3 20M14 2L14 6M8 10L8 14M16 18L16 22",
            "",
        ),
        (
            "",
            "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z",
        ),
        (
            "M22 12a10 10 0 1 1-20 0a10 10 0 1 1 20 0M12 16v-4M12 8h.01",
            "",
        ),
    ];
    let rows: Vec<SettingsSection> = SECTIONS
        .iter()
        .zip(icons)
        .map(|((name, words), (icon, fill))| SettingsSection {
            name: (*name).into(),
            icon: icon.into(),
            fill: fill.into(),
            r#match: q.is_empty()
                || search_text::normalize_search(&format!("{name} {words}")).contains(&q),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

impl App {
    fn persist(&self) {
        if let Err(e) = self.store.save() {
            self.toast(&format!("Couldn't save: {e}"));
        }
    }

    fn toast(&self, msg: &str) {
        self.ui.set_status(msg.into());
        if self.ui.get_page() == 6 || msg.is_empty() {
            return;
        }
        // the toast goes after a few seconds, unless something else replaced it
        let msg = msg.to_string();
        let weak = self.ui.as_weak();
        slint::Timer::single_shot(Duration::from_secs(4), move || {
            if let Some(ui) = weak.upgrade()
                && ui.get_status().as_str() == msg
            {
                ui.set_status(SharedString::new());
            }
        });
    }

    fn nav(&mut self, page: i32) {
        self.ui.invoke_drop_focus();
        self.ui.set_status(SharedString::new());
        self.ui.set_page(page);
        if page == 4 {
            self.ui.set_settings_page(0);
            self.ui.set_settings_query(SharedString::new());
            self.ui.set_settings_sections(settings_sections(""));
            self.refresh_settings();
        }
        self.refresh_all();
    }

    // ---------- settings ----------

    /// app.js getAdv: `pulse_adv_<name>` is 'true' / 'false', missing = the default.
    fn adv(&self, name: &str, default: bool) -> bool {
        match self.store.get(&format!("pulse_adv_{name}")) {
            None => default,
            Some(v) => v == "true",
        }
    }

    fn stream_timeout(&self) -> Duration {
        Duration::from_secs(
            self.store
                .number("pulse_stream_timeout", 12.0)
                .clamp(5.0, 30.0) as u64,
        )
    }

    /// Reads the original's settings keys into the UI (same keys, values and defaults as app.js).
    fn apply_prefs(&self) {
        let p = self.ui.global::<Prefs>();
        let s = &self.store;
        p.set_show_local(s.get("pulse_show_local") != Some("false"));
        p.set_hide_engine(s.get("pulse_hide_engine_btns") == Some("1"));
        p.set_compact_engine(s.get("pulse_compact_engine_btns") == Some("1"));
        p.set_corner(s.number("pulse_corner_radius", 16.0).clamp(0.0, 32.0) as f32);
        p.set_show_explicit(s.get("pulse_show_explicit") != Some("0"));
        p.set_highlight(s.get("pulse_highlight_buttons") != Some("0"));
        p.set_wavy(s.get("pulse_wavy_progress") != Some("false"));
        p.set_knob(s.get("pulse_progress_knob") != Some("false"));
        p.set_progress_bg(s.get("pulse_progress_bg") != Some("false"));
        p.set_smooth_presets(s.get("pulse_smooth_presets") != Some("false"));
        p.set_autoscroll(self.adv("lyrics_autoscroll", true));
        p.set_online_lyrics(self.adv("online_lyrics", true));
        p.set_light_thumbs(self.adv("light_thumbs", true));
        p.set_stream_timeout(self.stream_timeout().as_secs() as f32);
        p.set_search_pos(match s.get("pulse_search_pos") {
            Some("left") => 0,
            Some("right") => 2,
            _ => 1,
        });
        // Reduce motion (never chosen: off, as on a phone without the system setting) or
        // Performance mode: every animation becomes instant and the wave stands still
        let perf = matches!(s.get("pulse_perf_mode"), Some("1" | "true"));
        p.set_motion(if self.adv("reduce_motion", false) || perf { 0.0 } else { 1.0 });
        let speed = s.number("pulse_lyrics_speed", 1.0);
        let speed = if (0.5..=2.5).contains(&speed) { speed } else { 1.0 };
        p.set_lyric_ms((450.0 / speed).round() as i64);
        p.set_font_family(
            match s.get("pulse_font").unwrap_or("google") {
                "google" => "Google Sans",
                "sf" => "SF Pro Display",
                _ => "",
            }
            .into(),
        );
        p.set_cover_bg(self.adv("cover_bg", true));
        p.set_cover_k((s.number("pulse_cover_bg_brightness", 45.0).clamp(5.0, 100.0) / 45.0) as f32);
    }

    /// A setting changed in the UI: stored the way app.js stores it.
    fn set_pref(&mut self, key: &str, v: f32) {
        let on = v != 0.0;
        let (k, value): (&str, String) = match key {
            "show_local" => ("pulse_show_local", on.to_string()),
            "hide_engine_btns" => ("pulse_hide_engine_btns", if on { "1" } else { "0" }.into()),
            "compact_engine_btns" => (
                "pulse_compact_engine_btns",
                if on { "1" } else { "0" }.into(),
            ),
            "corner_radius" => ("pulse_corner_radius", (v.round() as i32).to_string()),
            "show_explicit" => ("pulse_show_explicit", if on { "1" } else { "0" }.into()),
            "highlight_buttons" => ("pulse_highlight_buttons", if on { "1" } else { "0" }.into()),
            "wavy_progress" => ("pulse_wavy_progress", on.to_string()),
            "progress_knob" => ("pulse_progress_knob", on.to_string()),
            "progress_bg" => ("pulse_progress_bg", on.to_string()),
            "smooth_presets" => ("pulse_smooth_presets", on.to_string()),
            "stream_timeout" => (
                "pulse_stream_timeout",
                (v.round() as i32).clamp(5, 30).to_string(),
            ),
            "search_pos" => (
                "pulse_search_pos",
                ["left", "middle", "right"][(v as usize).min(2)].into(),
            ),
            "adv_online_lyrics" => ("pulse_adv_online_lyrics", on.to_string()),
            "adv_lyrics_autoscroll" => ("pulse_adv_lyrics_autoscroll", on.to_string()),
            "adv_light_thumbs" => ("pulse_adv_light_thumbs", on.to_string()),
            _ => return,
        };
        self.store.set(k, value);
        self.persist();
        self.apply_prefs();
        if key == "adv_light_thumbs" {
            // covers load again at the new size
            self.arts.clear();
            self.refresh_rows();
        }
        if key == "show_local" && !on && self.ui.get_page() == 2 {
            self.nav(0);
        }
    }

    /// `{ artists, songs }` from pulse_blocked_artists / pulse_blocked_tracks (app.js getBlocks).
    fn blocks(&self) -> Value {
        let artists: Vec<Value> = self
            .store
            .list("pulse_blocked_artists")
            .into_iter()
            .filter(Value::is_string)
            .collect();
        let songs: Vec<Value> = self
            .store
            .list("pulse_blocked_tracks")
            .into_iter()
            .filter(Value::is_object)
            .collect();
        json!({ "artists": artists, "songs": songs })
    }

    fn blocked_labels(&self) -> Vec<String> {
        let b = self.blocks();
        let artists = js::list(b.get("artists"))
            .iter()
            .map(|a| js::str_or_empty(Some(a)));
        let songs = js::list(b.get("songs")).iter().map(|s| {
            let artist = js::str_or_empty(s.get("artist"));
            let title = js::str_or_empty(s.get("title"));
            if artist.is_empty() {
                title
            } else {
                format!("{title} · {artist}")
            }
        });
        artists.chain(songs).collect()
    }

    fn toggle_block_song(&mut self, track: &Value) {
        let b = self.blocks();
        let (list, blocked) =
            pulse_core::library_pages::toggle_blocked_song(js::list(b.get("songs")), track);
        self.store
            .set_json("pulse_blocked_tracks", &Value::Array(list));
        self.persist();
        let title = js::str_or_empty(track.get("title"));
        self.toast(&if blocked {
            format!("Blocked \"{title}\": it is skipped")
        } else {
            format!("Unblocked \"{title}\"")
        });
        self.refresh_settings();
    }

    fn toggle_block_artist(&mut self, name: &str) {
        let b = self.blocks();
        let (list, blocked) = pulse_core::library_pages::toggle_blocked_artist(
            js::list(b.get("artists")),
            Some(&json!(name)),
        );
        self.store
            .set_json("pulse_blocked_artists", &Value::Array(list));
        self.persist();
        self.toast(&if blocked {
            format!("Blocked {name}: their songs are skipped")
        } else {
            format!("Unblocked {name}")
        });
        self.refresh_settings();
    }

    /// Settings > Tracks: a tap on a blocked artist or song unblocks it.
    fn unblock(&mut self, i: usize) {
        let b = self.blocks();
        let artists = js::list(b.get("artists")).to_vec();
        if let Some(a) = artists.get(i) {
            let name = js::str_or_empty(Some(a));
            self.toggle_block_artist(&name);
        } else if let Some(s) = js::list(b.get("songs")).get(i - artists.len()).cloned() {
            self.toggle_block_song(&s);
        }
    }

    /// A speed & pitch preset: with "Smooth preset transitions" the values glide there over 400ms
    /// (app.js tweenSettings, smoothstep easing), else they jump.
    fn apply_preset(&mut self, i: usize) {
        let (s, sp, p) = SPEED_PRESETS.get(i).copied().unwrap_or((1.0, 1.0, 0.0));
        let to = [s as f32, sp as f32, p as f32];
        if self.store.get("pulse_smooth_presets") == Some("false") {
            self.set_speed_values(to);
            self.audio_changed(true);
            return;
        }
        let from = [
            self.ui.get_speed(),
            self.ui.get_speed_pitch(),
            self.ui.get_pitch(),
        ];
        let t0 = Instant::now();
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(16),
            move || {
                let k = (t0.elapsed().as_secs_f32() / 0.4).min(1.0);
                let e = k * k * (3.0 - 2.0 * k);
                let cur = [0, 1, 2].map(|j| from[j] + (to[j] - from[j]) * e);
                with(|a| {
                    a.set_speed_values(cur);
                    a.audio_changed(k >= 1.0);
                    if k >= 1.0 {
                        a.preset_tween = None;
                    }
                });
            },
        );
        self.preset_tween = Some(timer);
    }

    fn set_speed_values(&self, v: [f32; 3]) {
        self.ui.set_speed(v[0]);
        self.ui.set_speed_pitch(v[1]);
        self.ui.set_pitch(v[2]);
    }

    /// The values the settings pages show.
    fn refresh_settings(&mut self) {
        let cache = self.platform.data_dir.join("cache");
        let (n, bytes) = cache_stats(&cache);
        self.ui.set_cache_size(
            format!(
                "{} ({:.1} MB)",
                plural(n, "song"),
                bytes as f64 / 1_048_576.0
            )
            .into(),
        );
        let limit = self.store.number("pulse_cache_limit_mb", 2048.0);
        self.ui.set_cache_limit(
            [1024.0, 2048.0, 5120.0, 10240.0]
                .iter()
                .position(|m| (*m - limit).abs() < 1.0)
                .map_or(1, |i| i as i32),
        );
        let dirs: Vec<String> = self
            .platform
            .music_dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        self.ui.set_music_folder(
            format!("{} · {}", dirs.join(", "), plural(self.local.len(), "song")).into(),
        );
        self.ui
            .set_persist_sound(self.store.get("pulse_persist_sound_settings") != Some("0"));
        let blocked: Vec<SharedString> =
            self.blocked_labels().into_iter().map(Into::into).collect();
        self.ui.set_blocked(ModelRc::new(VecModel::from(blocked)));
        self.ui.set_diagnostics(
            format!(
                "Pulse {APP_VERSION} (Rust) · {} {} · {} favorites, {} playlists",
                std::env::consts::OS,
                std::env::consts::ARCH,
                self.store.list(FAVORITES).len(),
                self.store.list(PLAYLISTS).len()
            )
            .into(),
        );
        self.refresh_profiles();
        self.show_settings_groups();
    }

    fn set_toggle(&mut self, which: i32, on: bool) {
        let v = if on { "1" } else { "0" };
        match which {
            0 => self.store.set("pulse_persist_sound_settings", v),
            _ => return,
        }
        self.persist();
        self.refresh_settings();
    }

    /// Switches profile: the open profile's keys change, so everything is read again (the original reloads).
    fn open_profile(&mut self, i: usize) {
        let Some(p) = self.profile_list().get(i).cloned() else {
            return;
        };
        let id = js::str_or_empty(p.get("id"));
        if id == self.store.profile {
            return;
        }
        self.save_queue();
        self.player.stop();
        self.token += 1;
        self.store.profile = id.clone();
        self.prof_draft = None;
        self.ui.set_prof_edit(-1);
        self.ui.set_prof_confirm(-1);
        self.store.set(pulse_core::profiles::ACTIVE, id);
        self.persist();
        self.current = None;
        self.queue.clear();
        self.index = None;
        self.ui.set_has_track(false);
        self.restore_settings();
        self.apply_prefs();
        self.apply_theme();
        self.load_background();
        self.refresh_all();
        self.refresh_settings();
        self.toast(&format!("Opened {}", js::str_or_empty(p.get("name"))));
    }

    fn restore_settings(&mut self) {
        self.volume = self.store.number("pulse_volume", 0.85).clamp(0.0, 1.0) as f32;
        self.ui.set_volume(self.volume);
        self.repeat = match self.store.get("pulse_repeat") {
            Some("1") => Repeat::One,
            Some("all" | "inf") => Repeat::All,
            _ => Repeat::Off,
        };
        self.shuffle = self.store.get("pulse_shuffle") == Some("1");
        self.ui.set_shuffle(self.shuffle);
        self.ui.set_repeat_mode(self.repeat_index());
        // app.js: 'youtube' / 'ytmusic' / 'soundcloud' (earlier Rust builds wrote yt / ytm / sc)
        self.ui.set_source(match self.store.get("pulse_platform") {
            Some("ytmusic" | "ytm") => 1,
            Some("soundcloud" | "sc") => 2,
            _ => 0,
        });
        let sort = self.store.get("pulse_favorites_sort").unwrap_or("manual");
        self.ui.set_fav_sort(
            track_sort::TRACK_SORT_MODES
                .iter()
                .position(|m| *m == sort)
                .unwrap_or(0) as i32,
        );
        let s = self.audio_settings();
        let n = |v: &Value, d: f64| v.as_f64().unwrap_or(d);
        // the EQ a restored backup carries still applies (the phone layout has no EQ panel)
        let mut eq = EqSettings {
            low: n(&s["eq"]["low"], 0.0),
            mid: n(&s["eq"]["mid"], 0.0),
            high: n(&s["eq"]["high"], 0.0),
            ..Default::default()
        };
        for (i, b) in js::list(s.get("eqBands")).iter().take(10).enumerate() {
            eq.bands[i] = n(b, 0.0);
        }
        self.eq_flat = eq.clamped().is_flat();
        self.player.set_eq(eq.clamped());
        // "Save sound settings" off: speed, pitch and gain start from normal every launch (app.js)
        let keep = self.store.get("pulse_persist_sound_settings") != Some("0");
        let d = |k: &str, default: f64| if keep { n(&s[k], default) } else { default };
        self.ui.set_speed(d("speed", 1.0) as f32);
        self.ui.set_speed_pitch(d("speedPitch", 1.0) as f32);
        self.ui.set_pitch(d("pitch", 0.0) as f32);
        self.ui.set_gain(d("volume", 1.0) as f32);
        self.audio_changed(false);
        self.refresh_profiles();
        // the queue as it was left
        let q = self.store.json("pulse_queue_state", json!({}));
        self.queue = js::list(q.get("queue")).to_vec();
        self.index = q
            .get("index")
            .and_then(Value::as_u64)
            .map(|i| i as usize)
            .filter(|i| *i < self.queue.len());
        if let Some(t) = self.index.and_then(|i| self.queue.get(i)).cloned() {
            self.current = Some(t.clone());
            self.show_now(&t);
        }
    }

    fn repeat_index(&self) -> i32 {
        match self.repeat {
            Repeat::Off => 0,
            Repeat::All => 1,
            Repeat::One => 2,
        }
    }

    fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
        self.store.set(
            "pulse_repeat",
            match self.repeat {
                Repeat::Off => "0",
                Repeat::All => "all",
                Repeat::One => "1",
            },
        );
        self.ui.set_repeat_mode(self.repeat_index());
        self.persist();
    }

    fn audio_settings(&self) -> Value {
        match self.store.json("pulse_audio_settings", json!({})) {
            v @ Value::Object(_) => v,
            _ => json!({}),
        }
    }

    fn apply_volume(&self) {
        self.player.set_volume(self.volume * self.gain);
    }

    /// audio-engine.js playbackFor: with speed+pitch set the rate changes both; speed alone keeps
    /// the pitch in the original (no time-stretch in the port yet: ADR-011).
    fn audio_changed(&mut self, save: bool) {
        let (speed, sp, pitch) = (
            f64::from(self.ui.get_speed()),
            f64::from(self.ui.get_speed_pitch()),
            f64::from(self.ui.get_pitch()),
        );
        self.gain = self.ui.get_gain();
        let rate = if sp != 1.0 || pitch != 0.0 {
            sp * 2f64.powf(pitch / 12.0)
        } else {
            speed
        };
        self.player.set_rate(rate);
        self.apply_volume();
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let preset = SPEED_PRESETS
            .iter()
            .position(|p| close(p.0, speed) && close(p.1, sp) && close(p.2, pitch))
            .map_or(-1, |i| i as i32);
        self.ui.set_speed_preset(preset);
        if save && self.store.get("pulse_persist_sound_settings") != Some("0") {
            let mut s = self.audio_settings();
            s["speed"] = json!(speed);
            s["speedPitch"] = json!(sp);
            s["pitch"] = json!(pitch);
            s["volume"] = json!(f64::from(self.gain));
            self.store.set_json("pulse_audio_settings", &s);
            self.persist();
        }
    }

    fn set_sleep(&mut self, minutes: i32) {
        self.sleep = match minutes {
            0 => Sleep::Off,
            m if m < 0 => Sleep::EndOfTrack,
            m => Sleep::At(Instant::now() + Duration::from_secs(m as u64 * 60)),
        };
        self.ui.set_sleep_label(match minutes {
            0 => "Off".into(),
            m if m < 0 => "End of track".into(),
            m => format!("{m} min").into(),
        });
    }

    // ---------- lists and rows ----------

    fn list(&self, list: i32) -> Vec<Value> {
        match list {
            RESULTS => self.results.clone(),
            FAVS => self.fav_view.clone(),
            LOCAL => self.local_view.clone(),
            HIST => self.store.list(HISTORY),
            QUEUE => self.queue.clone(),
            REPEAT => self.on_repeat.clone(),
            PLAYLIST => self
                .playlist
                .and_then(|i| self.store.list(PLAYLISTS).get(i).cloned())
                .map(|p| js::list(p.get("tracks")).to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn favorite_keys(&self) -> HashSet<String> {
        self.store
            .list(FAVORITES)
            .iter()
            .map(track_sort::favorite_track_key)
            .collect()
    }

    fn rows(&mut self, list: &[Value]) -> ModelRc<TrackRow> {
        let favs = self.favorite_keys();
        let cur = self.current.as_ref().map(canonical_id);
        let rows: Vec<TrackRow> = list
            .iter()
            .map(|t| {
                let art = self.art(&js::str_or_empty(t.get("thumbnail")));
                TrackRow {
                    explicit: pulse_core::library_pages::is_track_explicit(t),
                    title: js::str_or_empty(t.get("title")).into(),
                    artist: js::str_or_empty(t.get("artist")).into(),
                    duration: clock_text(js::num_or_zero(t.get("duration"))).into(),
                    art,
                    favorite: favs.contains(&track_sort::favorite_track_key(t)),
                    playing: cur.as_deref() == Some(canonical_id(t).as_str()),
                }
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    }

    fn refresh_all(&mut self) {
        self.refresh_home();
        self.refresh_favorites();
        self.refresh_local();
        self.refresh_lists();
    }

    /// The discover page: liked songs, recent songs and playlists as tiles, the playlists row, On repeat.
    fn refresh_home(&mut self) {
        let favs = self.store.list(FAVORITES).len();
        let history = self.store.list(HISTORY);
        let playlists = self.store.list(PLAYLISTS);
        // Settings > Interface > Jump back in (pulse_adv_jump_back)
        let n = if self.adv("jump_back", true) { 4 } else { 0 };
        self.recents = history.iter().take(n).cloned().collect();
        let mut tiles = vec![Tile {
            title: "Liked songs".into(),
            subtitle: plural(favs, "song").into(),
            art: Image::default(),
            kind: 0,
        }];
        for t in self.recents.clone() {
            let art = self.art(&js::str_or_empty(t.get("thumbnail")));
            tiles.push(Tile {
                title: js::str_or_empty(t.get("title")).into(),
                subtitle: js::str_or_empty(t.get("artist")).into(),
                art,
                kind: 1,
            });
        }
        for p in playlists.iter().take(6 - tiles.len().min(6)) {
            let art = self.playlist_art(p);
            tiles.push(Tile {
                title: js::str_or_empty(p.get("name")).into(),
                subtitle: plural(js::list(p.get("tracks")).len(), "song").into(),
                art,
                kind: 2,
            });
        }
        self.ui.set_tiles(ModelRc::new(VecModel::from(tiles)));
        let cards: Vec<Tile> = playlists
            .iter()
            .map(|p| {
                let art = self.playlist_art(p);
                Tile {
                    title: js::str_or_empty(p.get("name")).into(),
                    subtitle: plural(js::list(p.get("tracks")).len(), "song").into(),
                    art,
                    kind: 2,
                }
            })
            .collect();
        self.ui.set_playlists(ModelRc::new(VecModel::from(cards)));
        // On repeat: what was played in the chosen period, most recent first
        let window = match self.ui.get_repeat_range() {
            0 => 7.0,
            1 => 30.0,
            _ => f64::INFINITY,
        } * 86_400_000.0;
        let now = now_ms();
        self.on_repeat = history
            .into_iter()
            .filter(|t| now - js::num_or_zero(t.get("playedAt")) <= window)
            .take(5)
            .collect();
        let rows = self.rows(&self.on_repeat.clone());
        self.ui.set_on_repeat(rows);
        self.ui.set_favorites_count(favs as i32);
    }

    fn playlist_art(&mut self, p: &Value) -> Image {
        let url = js::list(p.get("tracks"))
            .iter()
            .map(|t| js::str_or_empty(t.get("thumbnail")))
            .find(|u| !u.is_empty())
            .unwrap_or_default();
        self.art(&url)
    }

    fn refresh_favorites(&mut self) {
        let all = track_sort::dedupe_favorite_tracks(&self.store.list(FAVORITES));
        let mode = track_sort::TRACK_SORT_MODES
            .get(self.ui.get_fav_sort() as usize)
            .copied()
            .unwrap_or("manual");
        let sorted = track_sort::sort_tracks(&all, mode);
        let q = search_text::normalize_search(&self.fav_filter);
        self.fav_view = sorted
            .into_iter()
            .filter(|t| {
                q.is_empty()
                    || search_text::normalize_search(&js::str_or_empty(t.get("title"))).contains(&q)
                    || search_text::normalize_search(&js::str_or_empty(t.get("artist")))
                        .contains(&q)
            })
            .collect();
        let labels: Vec<SharedString> = self.ui.get_fav_sort_labels().iter().collect();
        self.ui.set_fav_sort_label(
            labels
                .get(self.ui.get_fav_sort() as usize)
                .cloned()
                .unwrap_or_else(|| "Manual order".into()),
        );
        let rows = self.rows(&self.fav_view.clone());
        self.ui.set_favorites(rows);
        self.ui.set_favorites_count(all.len() as i32);
    }

    fn refresh_local(&mut self) {
        let q = search_text::normalize_search(&self.local_filter);
        self.local_view = self
            .local
            .iter()
            .filter(|t| {
                q.is_empty()
                    || search_text::normalize_search(&js::str_or_empty(t.get("title"))).contains(&q)
                    || search_text::normalize_search(&js::str_or_empty(t.get("artist")))
                        .contains(&q)
            })
            .cloned()
            .collect();
        let rows = self.rows(&self.local_view.clone());
        self.ui.set_local(rows);
    }

    fn refresh_lists(&mut self) {
        let r = self.rows(&self.results.clone());
        self.ui.set_results(r);
        let h = self.rows(&self.store.list(HISTORY));
        self.ui.set_history(h);
        let q = self.rows(&self.queue.clone());
        self.ui.set_queue(q);
        if let Some(i) = self.playlist {
            let p = self
                .store
                .list(PLAYLISTS)
                .get(i)
                .cloned()
                .unwrap_or(Value::Null);
            self.ui
                .set_playlist_name(js::str_or_empty(p.get("name")).into());
            let rows = self.rows(js::list(p.get("tracks")));
            self.ui.set_playlist(rows);
        }
    }

    /// Rows after a cover arrived or the favourites changed.
    fn refresh_rows(&mut self) {
        self.refresh_home();
        let f = self.rows(&self.fav_view.clone());
        self.ui.set_favorites(f);
        let l = self.rows(&self.local_view.clone());
        self.ui.set_local(l);
        self.refresh_lists();
    }

    /// A thumbnail: cached, or fetched in the background (the rows refresh when it arrives).
    fn art(&mut self, url: &str) -> Image {
        if url.is_empty() {
            return Image::default();
        }
        if let Some((i, _)) = self.arts.get(url) {
            return i.clone();
        }
        if self.pending_art.insert(url.to_string()) {
            let (net, url) = (self.net.clone(), url.to_string());
            // lightweight thumbnails (Advanced): small covers, far less memory for long lists
            let size = if !self.adv("light_thumbs", true) {
                600
            } else {
                240
            };
            self.rt.spawn(async move {
                let pixels = fetch_thumb(&net, &url, size).await;
                later(move |a| {
                    a.pending_art.remove(&url);
                    if let Some((w, h, raw)) = pixels {
                        let tint = tint_of(&raw);
                        a.covers.insert(url.clone(), cover_analysis(w, h, &raw));
                        a.arts.insert(
                            url.clone(),
                            (
                                Image::from_rgba8(
                                    SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&raw, w, h),
                                ),
                                tint,
                            ),
                        );
                        a.refresh_rows();
                        if a.current
                            .as_ref()
                            .is_some_and(|t| js::str_or_empty(t.get("thumbnail")) == url)
                        {
                            a.ui.set_now_art(a.arts[&url].0.clone());
                            a.ui.set_tint(Color::from_rgb_u8(tint.0, tint.1, tint.2));
                            a.ui.set_tint2(tint2_of(tint));
                            a.apply_theme();
                        }
                    }
                });
            });
        }
        Image::default()
    }

    // ---------- search ----------

    fn search(&mut self, q: String) {
        let q = js::trim(&q).to_string();
        if q.is_empty() {
            return;
        }
        let source = self.ui.get_source();
        self.ui.set_page(6);
        self.results.clear();
        self.refresh_lists();
        self.ui.set_status("Searching…".into());
        let net = self.net.clone();
        self.rt.spawn(async move {
            let r: pulse_net::Result<Vec<Value>> = async {
                if q.starts_with("http") {
                    if let Some(list) = regex_list_id(&q) {
                        let (_, entries) = pulse_net::youtube::playlist(&net, &list, 500).await?;
                        return Ok(entries.iter().map(|e| entry_track(e, "youtube")).collect());
                    }
                    if q.contains("soundcloud.com") {
                        let a = pulse_net::soundcloud::audio(&net, &q).await?;
                        return Ok(vec![json!({ "id": q, "title": a.title, "artist": a.artist, "duration": a.duration, "url": q, "platform": "soundcloud" })]);
                    }
                    if let Some(id) = pulse_net::youtube::video_id(&q) {
                        let a = pulse_net::youtube::audio(&net, &id).await?;
                        let e = Entry { id: id.clone(), title: a.title, uploader: a.artist, duration: a.duration, webpage_url: format!("https://www.youtube.com/watch?v={id}"), ..Default::default() };
                        return Ok(vec![entry_track(&e, "youtube")]);
                    }
                }
                Ok(match source {
                    1 => pulse_net::youtube::music_search(&net, &q, 25).await?.iter().map(|e| entry_track(e, "ytmusic")).collect(),
                    2 => pulse_net::soundcloud::search(&net, &q, 25).await?.iter().map(|e| entry_track(e, "soundcloud")).collect(),
                    _ => pulse_net::youtube::search(&net, &q, 25).await?.iter().map(|e| entry_track(e, "youtube")).collect(),
                })
            }
            .await;
            later(move |a| match r {
                Ok(list) => {
                    a.ui.set_status(if list.is_empty() { "Nothing found".into() } else { SharedString::new() });
                    a.results = list;
                    a.refresh_lists();
                }
                Err(e) => a.ui.set_status(format!("Search failed: {e}").into()),
            });
        });
    }

    // ---------- library ----------

    fn tile_clicked(&mut self, i: usize) {
        let recents = self.recents.len();
        if i == 0 {
            self.nav(1);
        } else if i <= recents {
            let list = self.recents.clone();
            self.play_list(list, i - 1);
        } else {
            self.open_playlist(i - 1 - recents);
        }
    }

    fn open_playlist(&mut self, i: usize) {
        if i >= self.store.list(PLAYLISTS).len() {
            return;
        }
        self.playlist = Some(i);
        self.ui.set_page(5);
        self.refresh_lists();
    }

    /// Makes a playlist (with `first` in it); returns its index.
    fn new_playlist(&mut self, first: Option<Value>) -> usize {
        let mut pls = self.store.list(PLAYLISTS);
        let n = (1..)
            .find(|n| {
                !pls.iter()
                    .any(|p| js::str_or_empty(p.get("name")) == format!("My playlist #{n}"))
            })
            .unwrap_or(1);
        let id = pulse_core::backup::new_playlist_id(now_ms() as u64, self.seed);
        pls.push(json!({ "id": id, "name": format!("My playlist #{n}"), "tracks": first.into_iter().collect::<Vec<_>>(), "createdAt": now_ms() }));
        self.store.set_json(PLAYLISTS, &Value::Array(pls.clone()));
        self.persist();
        self.refresh_all();
        pls.len() - 1
    }

    fn toggle_favorite_of(&mut self, t: &Value) {
        let key = track_sort::favorite_track_key(t);
        let mut favs = self.store.list(FAVORITES);
        if favs
            .iter()
            .any(|f| track_sort::favorite_track_key(f) == key || track_sort::is_same_song(f, t))
        {
            favs.retain(|f| {
                track_sort::favorite_track_key(f) != key && !track_sort::is_same_song(f, t)
            });
        } else {
            let mut t = t.clone();
            t["addedAt"] = json!(now_ms());
            favs.insert(0, t);
        }
        self.store.set_json(FAVORITES, &Value::Array(favs));
        self.persist();
        if let Some(cur) = &self.current {
            self.ui.set_now_favorite(
                self.favorite_keys()
                    .contains(&track_sort::favorite_track_key(cur)),
            );
        }
        self.refresh_favorites();
        self.refresh_rows();
    }

    fn open_song_menu(&mut self, list: i32, i: usize) {
        let Some(track) = self.list(list).get(i).cloned() else {
            return;
        };
        let removable = matches!(list, FAVS | HIST | QUEUE | PLAYLIST);
        let mut items: Vec<SharedString> = vec!["Play next".into(), "Add to queue".into()];
        if removable {
            items.push(
                match list {
                    FAVS => "Remove from favorites",
                    HIST => "Remove from history",
                    QUEUE => "Remove from queue",
                    _ => "Remove from this playlist",
                }
                .into(),
            );
        }
        let blocks = self.blocks();
        items.push(
            if pulse_core::library_pages::is_song_blocked(js::list(blocks.get("songs")), &track) {
                "Unblock song"
            } else {
                "Block song"
            }
            .into(),
        );
        let artist = js::str_or_empty(track.get("artist"));
        if !artist.is_empty() {
            items.push(
                if pulse_core::library_pages::is_artist_blocked(
                    js::list(blocks.get("artists")),
                    track.get("artist"),
                ) {
                    "Unblock artist"
                } else {
                    "Block artist"
                }
                .into(),
            );
        }
        self.ui.set_menu_title(
            format!(
                "{} · {}",
                js::str_or_empty(track.get("title")),
                js::str_or_empty(track.get("artist"))
            )
            .into(),
        );
        self.ui.set_menu_items(ModelRc::new(VecModel::from(items)));
        self.menu = Some(Menu::Song {
            track,
            list,
            index: i,
            removable,
        });
        self.ui.set_menu_open(true);
    }

    fn open_add_menu(&mut self, list: i32, i: usize) {
        let Some(track) = self.list(list).get(i).cloned() else {
            return;
        };
        let mut items: Vec<SharedString> = self
            .store
            .list(PLAYLISTS)
            .iter()
            .map(|p| js::str_or_empty(p.get("name")).into())
            .collect();
        items.push("New playlist".into());
        self.ui.set_menu_title("Add to playlist".into());
        self.ui.set_menu_items(ModelRc::new(VecModel::from(items)));
        self.menu = Some(Menu::AddTo { track });
        self.ui.set_menu_open(true);
    }

    fn menu_action(&mut self, i: usize) {
        match self.menu.take() {
            Some(Menu::Song {
                track,
                list,
                index,
                removable,
            }) => match i {
                0 => {
                    let at = self
                        .index
                        .map_or(self.queue.len(), |c| c + 1)
                        .min(self.queue.len());
                    self.queue.insert(at, track);
                    self.after_queue_change("Playing next");
                }
                1 => {
                    self.queue.push(track);
                    self.after_queue_change("Added to queue");
                }
                2 if removable => self.remove_from(list, index, &track),
                i if i == 2 + usize::from(removable) => self.toggle_block_song(&track),
                i if i == 3 + usize::from(removable) => {
                    self.toggle_block_artist(&js::str_or_empty(track.get("artist")))
                }
                _ => {}
            },
            Some(Menu::AddTo { track }) => {
                let mut pls = self.store.list(PLAYLISTS);
                if i >= pls.len() {
                    self.new_playlist(Some(track));
                    self.toast("Added to a new playlist");
                    return;
                }
                let name = js::str_or_empty(pls[i].get("name"));
                let id = canonical_id(&track);
                let tracks = pls[i]["tracks"].as_array_mut();
                match tracks {
                    Some(list) if list.iter().any(|t| canonical_id(t) == id) => {
                        self.toast(&format!("Already in {name}"));
                        return;
                    }
                    Some(list) => list.push(track),
                    None => pls[i]["tracks"] = json!([track]),
                }
                self.store.set_json(PLAYLISTS, &Value::Array(pls));
                self.persist();
                self.refresh_all();
                self.toast(&format!("Added to {name}"));
            }
            None => {}
        }
    }

    fn after_queue_change(&mut self, msg: &str) {
        if self.index.is_none() && !self.queue.is_empty() {
            self.play_index(0);
        } else {
            self.save_queue();
            self.refresh_lists();
        }
        self.toast(msg);
    }

    fn remove_from(&mut self, list: i32, index: usize, track: &Value) {
        let id = canonical_id(track);
        match list {
            FAVS => return self.toggle_favorite_of(track),
            HIST => {
                let mut h = self.store.list(HISTORY);
                h.retain(|t| canonical_id(t) != id);
                self.store.set_json(HISTORY, &Value::Array(h));
            }
            QUEUE => {
                if index < self.queue.len() && Some(index) != self.index {
                    self.queue.remove(index);
                    if let Some(c) = self.index.filter(|c| *c > index) {
                        self.index = Some(c - 1);
                    }
                    self.save_queue();
                }
            }
            PLAYLIST => {
                let mut pls = self.store.list(PLAYLISTS);
                if let Some(t) = self
                    .playlist
                    .and_then(|p| pls.get_mut(p))
                    .and_then(|p| p["tracks"].as_array_mut())
                    && index < t.len()
                {
                    t.remove(index);
                }
                self.store.set_json(PLAYLISTS, &Value::Array(pls));
            }
            _ => return,
        }
        self.persist();
        self.refresh_all();
    }

    /// Music files on the device (the original's local library), read in the background.
    fn scan_local(&mut self) {
        let dirs = self.platform.music_dirs.clone();
        self.rt.spawn_blocking(move || {
            let mut out = Vec::new();
            for dir in dirs {
                for e in walkdir::WalkDir::new(&dir)
                    .max_depth(6)
                    .into_iter()
                    .flatten()
                    .take(20000)
                {
                    let p = e.path();
                    let ext = p
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(str::to_lowercase)
                        .unwrap_or_default();
                    if !e.file_type().is_file() || !AUDIO_EXTS.contains(&ext.as_str()) {
                        continue;
                    }
                    out.push(local_track(p));
                    if out.len() >= 5000 {
                        break;
                    }
                }
            }
            out.sort_by(|a, b| {
                track_sort::locale_compare(
                    &js::str_or_empty(a.get("title")),
                    &js::str_or_empty(b.get("title")),
                )
            });
            later(move |a| {
                let n = out.len();
                a.local = out;
                a.refresh_local();
                if a.ui.get_status().starts_with("Looking for music") {
                    a.toast(&format!("{} on this device", plural(n, "song")));
                }
            });
        });
    }

    fn download_all(&mut self) {
        let list = self.fav_view.clone();
        if list.is_empty() {
            return;
        }
        let (net, cache) = (self.net.clone(), self.platform.data_dir.join("cache"));
        let limit = self.store.number("pulse_cache_limit_mb", 2048.0);
        let timeout = self.stream_timeout();
        self.toast(&format!("Downloading {}…", plural(list.len(), "song")));
        self.rt.spawn(async move {
            let mut ok = 0;
            for t in &list {
                if audio_file(&net, t, &cache, limit, timeout).await.is_ok() {
                    ok += 1;
                }
            }
            later(move |a| a.toast(&format!("{ok} of {} songs ready offline", list.len())));
        });
    }

    // ---------- queue and playback ----------

    fn play_from(&mut self, list: i32, i: usize) {
        if list == QUEUE {
            self.play_index(i);
            return;
        }
        let l = self.list(list);
        self.play_list(l, i);
    }

    fn play_list(&mut self, list: Vec<Value>, i: usize) {
        if i >= list.len() {
            return;
        }
        self.queue = list;
        self.play_index(i);
    }

    fn save_queue(&mut self) {
        let q = json!({ "queue": self.queue.iter().take(500).collect::<Vec<_>>(), "index": self.index });
        self.store.set_json("pulse_queue_state", &q);
    }

    fn show_now(&mut self, t: &Value) {
        self.ui.set_has_track(true);
        self.ui
            .set_now_title(js::str_or_empty(t.get("title")).into());
        self.ui
            .set_now_artist(js::str_or_empty(t.get("artist")).into());
        let url = js::str_or_empty(t.get("thumbnail"));
        let art = self.art(&url);
        self.ui.set_now_art(art);
        let tint = self.arts.get(&url).map_or(DEFAULT_TINT, |a| a.1);
        self.ui.set_tint(Color::from_rgb_u8(tint.0, tint.1, tint.2));
        self.ui.set_tint2(tint2_of(tint));
        self.apply_theme();
        self.ui.set_now_favorite(
            self.favorite_keys()
                .contains(&track_sort::favorite_track_key(t)),
        );
        let d = js::num_or_zero(t.get("duration"));
        self.ui.set_duration(d as f32);
        self.ui.set_duration_text(clock_text(d).into());
        self.ui.set_position(0.0);
        self.ui.set_position_text("0:00".into());
    }

    fn play_index(&mut self, i: usize) {
        let Some(t) = self.queue.get(i).cloned() else {
            return;
        };
        self.index = Some(i);
        self.token += 1;
        let token = self.token;
        self.current = Some(t.clone());
        self.loaded = false;
        self.player.stop();
        self.engine_file = None;
        self.offload_refused = false;
        if self.offloaded
            && let Some(o) = &self.platform.offload
        {
            o.pause();
        }
        self.show_now(&t);
        self.ui.set_busy(true);
        self.ui
            .set_lyrics(ModelRc::new(VecModel::from(Vec::<LyricLine>::new())));
        self.lyrics.clear();
        self.lyric_index = -1;
        // history: newest first, one entry per song (app.js playTrack)
        let mut hist = self.store.list(HISTORY);
        let id = canonical_id(&t);
        hist.retain(|h| canonical_id(h) != id);
        let mut played = t.clone();
        played["playedAt"] = json!(now_ms());
        hist.insert(0, played);
        hist.truncate(HISTORY_CAP);
        self.store.set_json(HISTORY, &Value::Array(hist));
        self.save_queue();
        self.persist();
        self.refresh_home();
        self.refresh_rows();

        let (net, cache) = (self.net.clone(), self.platform.data_dir.join("cache"));
        let limit_mb = self.store.number("pulse_cache_limit_mb", 2048.0);
        let timeout = self.stream_timeout();
        let track = t.clone();
        let downloads = self.downloads.clone();
        self.current_growing = None;
        self.rt.spawn(async move {
            let r = audio_source(&net, &track, &cache, limit_mb, timeout, &downloads).await;
            if let Err(e) = &r {
                journal(&format!("play: failed: {e}"));
            }
            later(move |a| {
                if a.token != token {
                    return;
                }
                a.ui.set_busy(false);
                match r {
                    Ok(Source::Growing { part, done, growing }) => {
                        // plays while the rest downloads; Android's screen-off player only gets
                        // whole files, so that waits until it has all come in (see tick)
                        a.offloaded = false;
                        a.current_file = None;
                        a.current_growing = Some((done, growing.clone()));
                        a.engine_file = Some(part.clone());
                        a.player.load_growing(part, 0.0, growing);
                        a.player.play();
                        a.prefetch_next();
                        a.schedule_tick();
                    }
                    Ok(Source::File(path)) => {
                        a.current_file = Some(path.clone());
                        // screen off and handed over: the next song goes to Android's player too
                        let vol = a.volume * a.gain;
                        let handed = a.offloaded
                            && a.platform
                                .offload
                                .as_ref()
                                .is_some_and(|o| o.play(&path.to_string_lossy(), 0.0, vol));
                        if handed {
                            a.loaded = true;
                        } else {
                            a.offloaded = false;
                            a.engine_file = Some(path.clone());
                            a.player.load(path, 0.0);
                            a.player.play();
                        }
                        a.prefetch_next();
                        a.schedule_tick();
                    }
                    Err(e) => a.song_failed(&format!("Can't play this song: {e}")),
                }
            });
        });
        self.load_lyrics(&t);
    }

    /// The song's lyrics: custom ones (Edit / Sync lyrics) first, then an .lrc file next to a local
    /// song, then the online lookup (app.js loadSyncedLyrics + server.js /api/lyrics).
    fn load_lyrics(&mut self, t: &Value) {
        let token = self.token;
        self.ui.set_lyrics(ModelRc::new(VecModel::from(Vec::<LyricLine>::new())));
        self.lyrics.clear();
        self.lyric_index = -1;
        self.ui.set_lyric_index(-1);
        let (net, title, artist, d) = (
            self.net.clone(),
            js::str_or_empty(t.get("title")),
            js::str_or_empty(t.get("artist")),
            js::num_or_zero(t.get("duration")),
        );
        let online = self.adv("online_lyrics", true);
        let custom = self.custom_lyrics_path(&title, &artist);
        let local_lrc = t
            .get("path")
            .and_then(Value::as_str)
            .map(|p| Path::new(p).with_extension("lrc"));
        self.rt.spawn(async move {
            let text = match std::fs::read_to_string(&custom).ok().filter(|s| !s.trim().is_empty()) {
                Some(c) => Some(c),
                None => match local_lrc.and_then(|p| std::fs::read_to_string(p).ok()) {
                    Some(t) => Some(t),
                    None if online => lyrics::fetch(&net, &title, &artist, d).await,
                    None => None,
                },
            };
            let lines = text.map(|t| lyrics::parse_lrc(&t)).unwrap_or_default();
            later(move |a| {
                if a.token != token {
                    return;
                }
                let rows: Vec<LyricLine> = lines
                    .iter()
                    .map(|l| LyricLine {
                        text: l.text.clone().into(),
                        synced: l.time >= 0.0,
                    })
                    .collect();
                a.ui.set_lyrics(ModelRc::new(VecModel::from(rows)));
                a.lyrics = lines;
            });
        });
    }

    fn toggle_play(&mut self) {
        if self.p_is_playing() {
            self.p_pause();
        } else if self.loaded {
            self.p_play();
        } else if let Some(i) = self.index {
            self.play_index(i);
        }
    }

    fn random(&mut self, n: usize) -> usize {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed % n as u64) as usize
    }

    /// `user`: the next button (repeat-one only holds for songs that end on their own).
    fn next(&mut self, user: bool) {
        // app.js: blocked artists and songs are skipped when the queue moves on
        let blocks = self.blocks();
        let start = self.index;
        for _ in 0..self.queue.len().max(1) {
            self.next_once(user);
            let Some(i) = self.index else { return };
            let blocked = self
                .queue
                .get(i)
                .is_some_and(|t| pulse_core::library_pages::is_track_blocked(&blocks, t));
            if !blocked || self.index == start {
                return;
            }
        }
    }

    fn next_once(&mut self, user: bool) {
        let n = self.queue.len();
        let Some(i) = self.index else { return };
        if n == 0 {
            return;
        }
        if !user && self.repeat == Repeat::One {
            self.repeat_current();
            return;
        }
        let next = if self.shuffle && n > 1 {
            let mut j = self.random(n - 1);
            if j >= i {
                j += 1;
            }
            Some(j)
        } else if i + 1 < n {
            Some(i + 1)
        } else if self.repeat != Repeat::Off {
            Some(0)
        } else {
            None
        };
        match next {
            Some(j) => self.play_index(j),
            None => self.p_pause(),
        }
    }

    fn prev(&mut self) {
        if self.p_position() > 3.0 {
            self.p_seek(0.0);
            return;
        }
        match self.index {
            Some(i) if i > 0 => self.play_index(i - 1),
            _ => self.p_seek(0.0),
        }
    }

    /// Repeat one: the same song again from the top. The engine lets go of a song when it ends,
    /// so it loads the file again (rewinding an unloaded song played nothing); Android's player
    /// keeps its song and just starts over.
    fn repeat_current(&mut self) {
        if self.offloaded {
            self.p_seek(0.0);
            self.p_play();
            return;
        }
        match self.current_file.clone().or_else(|| self.engine_file.clone()) {
            Some(path) => {
                self.engine_file = Some(path.clone());
                self.player.load(path, 0.0);
                self.player.play();
                self.lyric_index = -1;
                self.ui.set_lyric_index(-1);
            }
            // no file to hand (it never finished loading): play it again from the start
            None => {
                if let Some(i) = self.index {
                    self.play_index(i);
                }
            }
        }
    }

    /// A song couldn't be loaded or stopped with an error: say so everywhere (the lock screen,
    /// notification and HyperIsland kept showing "playing" while nothing played), then go on with
    /// the next song, like other players; three in a row and it stops (no network, most likely).
    fn song_failed(&mut self, why: &str) {
        journal(&format!("play: {why}"));
        self.fail_streak += 1;
        self.ui.set_playing(false);
        let pos = self.p_position();
        self.publish_media(false, pos);
        let more = self.index.is_some_and(|i| i + 1 < self.queue.len() || self.repeat == Repeat::All);
        if self.fail_streak < 3 && more {
            self.toast(&format!("{why}. Playing the next song"));
            // as the next button does: repeat-one would only retry the same song
            self.next(true);
        } else {
            self.toast(why);
            self.fail_streak = 0;
        }
        self.schedule_tick();
    }

    /// The next song in the queue starts downloading now, so it begins at once when its turn comes.
    fn prefetch_next(&mut self) {
        self.fail_streak = 0;
        let Some(i) = self.index else { return };
        if self.shuffle || self.queue.is_empty() {
            return;
        }
        let j = if i + 1 < self.queue.len() {
            i + 1
        } else if self.repeat == Repeat::All {
            0
        } else {
            return;
        };
        let Some(t) = self.queue.get(j).cloned() else { return };
        if t.get("path").is_some() {
            return;
        }
        let (net, cache, downloads) = (self.net.clone(), self.platform.data_dir.join("cache"), self.downloads.clone());
        let limit_mb = self.store.number("pulse_cache_limit_mb", 2048.0);
        let timeout = self.stream_timeout();
        self.rt.spawn(async move {
            if let Err(e) = audio_source(&net, &t, &cache, limit_mb, timeout, &downloads).await {
                journal(&format!("prefetch: {e}"));
            }
        });
    }

    // ---------- playback: the app's engine, or Android's player while the screen is off ----------

    fn p_is_playing(&self) -> bool {
        match (&self.platform.offload, self.offloaded) {
            (Some(o), true) => o.is_playing(),
            _ => self.player.is_playing(),
        }
    }

    fn p_position(&self) -> f64 {
        match (&self.platform.offload, self.offloaded) {
            (Some(o), true) => o.position().unwrap_or(0.0),
            _ => self.player.position(),
        }
    }

    fn p_play(&self) {
        match (&self.platform.offload, self.offloaded) {
            (Some(o), true) => o.resume(),
            _ => self.player.play(),
        }
    }

    fn p_pause(&self) {
        match (&self.platform.offload, self.offloaded) {
            (Some(o), true) => o.pause(),
            _ => self.player.pause(),
        }
    }

    fn p_seek(&self, secs: f64) {
        match (&self.platform.offload, self.offloaded) {
            (Some(o), true) => o.seek(secs),
            _ => self.player.seek(secs),
        }
    }

    /// Nothing changes the sound: no equaliser, speed, pitch, volume boost or sleep fade. Only then
    /// may Android's player take over (it can't apply any of them).
    fn sound_untouched(&self) -> bool {
        self.eq_flat
            && (self.ui.get_speed() - 1.0).abs() < 1e-3
            && (self.ui.get_speed_pitch() - 1.0).abs() < 1e-3
            && self.ui.get_pitch().abs() < 1e-3
            && self.ui.get_gain() <= 1.0 + 1e-3
            && self.sleep == Sleep::Off
    }

    /// Screen off: hand the playing song to Android's player at the same spot, so the phone can
    /// decode it on its audio processor while the CPU sleeps.
    fn hand_over(&mut self) {
        if self.offloaded || self.offload_refused || !self.player.is_playing() || !self.loaded || !self.sound_untouched() {
            return;
        }
        let (Some(o), Some(path)) = (&self.platform.offload, &self.current_file) else { return };
        let pos = self.player.position();
        self.player.pause();
        if o.play(&path.to_string_lossy(), pos, self.volume * self.gain) {
            self.offloaded = true;
            journal(&format!("playback: Android's player from {pos:.1} s"));
        } else {
            self.player.play();
            self.offload_refused = true;
            journal("playback: Android's player refused the song, the app keeps playing");
        }
    }

    /// Screen on (or Android's player failed): the app's engine continues where it was.
    fn take_back(&mut self) {
        if !self.offloaded {
            return;
        }
        self.offloaded = false;
        let (pos, playing) = match &self.platform.offload {
            Some(o) => {
                let r = (o.position(), o.is_playing());
                o.release();
                r
            }
            None => (None, false),
        };
        let pos = pos.unwrap_or(0.0);
        if let Some(path) = self.current_file.clone() {
            if self.engine_file.as_ref() == Some(&path) {
                self.player.seek(pos);
            } else {
                self.engine_file = Some(path.clone());
                self.player.load(path, pos);
            }
            if playing {
                self.player.play();
            }
        }
        journal(&format!("playback: back to the app's engine at {pos:.1} s"));
    }

    /// Commands from the notification, lock screen, headphones and the activity's lifecycle.
    fn poll_commands(&mut self) {
        let cmds: Vec<String> = self
            .platform
            .media_commands
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        let handled = !cmds.is_empty();
        for c in cmds {
            match c.as_str() {
                "play" => {
                    if !self.p_is_playing() {
                        self.toggle_play();
                    }
                }
                "pause" => {
                    self.p_pause();
                    journal("pause (from Android)");
                }
                // Android's player finished the song / gave up on it
                "offload-ended" => {
                    if self.offloaded {
                        self.next(false);
                    }
                }
                "offload-error" => {
                    self.take_back();
                    self.offload_refused = true;
                }
                "toggle" => self.toggle_play(),
                "next" => self.next(true),
                "prev" => self.prev(),
                // the app went to the background (MainActivity.onPause): no text field keeps the
                // keyboard, or it pops up again on return
                "hidden" => self.ui.invoke_drop_focus(),
                // Android onStop / onStart: the app left the screen or came back
                "background" => self.set_background(true),
                "foreground" => self.set_background(false),
                s if s.starts_with("focus:") => journal(&format!("audio {s}")),
                s => {
                    if let Some(t) = s.strip_prefix("seek:").and_then(|t| t.parse::<f64>().ok()) {
                        self.p_seek(t);
                    }
                }
            }
        }
        if handled {
            // at once: a pause in the background stops the heartbeat, which used to be what told
            // the system, so the notification and HyperIsland went on showing "playing"
            let (playing, pos) = (self.p_is_playing(), self.p_position());
            self.ui.set_playing(playing);
            self.publish_media(playing, pos);
        }
        self.schedule_tick();
    }

    /// On screen: 4 beats a second. In the background while playing (or loading): one a second,
    /// enough to start the next song and keep the notification right. In the background and
    /// paused: none at all; a notification button wakes the app itself (see `wake`).
    fn schedule_tick(&mut self) {
        let busy = self.p_is_playing() || self.ui.get_busy() || matches!(self.sleep, Sleep::At(_));
        let ms = if !self.background { 250 } else if busy { 1000 } else { 0 };
        if ms == self.ticker_ms {
            return;
        }
        self.ticker_ms = ms;
        if ms == 0 {
            self.ticker.stop();
        } else {
            self.ticker.start(slint::TimerMode::Repeated, Duration::from_millis(ms), || {
                with(App::tick);
            });
        }
    }

    fn set_background(&mut self, bg: bool) {
        if bg == self.background {
            return;
        }
        self.background = bg;
        self.ui.global::<Prefs>().set_awake(!bg);
        if bg {
            self.hand_over();
        } else {
            self.take_back();
        }
        journal(if bg { "background" } else { "foreground" });
        self.schedule_tick();
        if !bg {
            // catch the screen up at once
            self.tick();
        }
    }

    fn tick(&mut self) {
        self.schedule_tick();
        if let Some((done, g)) = &self.current_growing
            && g.done.load(std::sync::atomic::Ordering::Acquire)
        {
            self.current_file = Some(done.clone());
            self.current_growing = None;
        }
        // started from the lock screen or a new song in the background: hand it over too
        if self.background {
            self.hand_over();
        }
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                Event::Loaded { duration } => {
                    self.loaded = true;
                    if duration > 0.0 {
                        self.ui.set_duration(duration as f32);
                        self.ui.set_duration_text(clock_text(duration).into());
                    }
                }
                Event::Ended => {
                    if self.sleep == Sleep::EndOfTrack {
                        self.set_sleep(0);
                    } else {
                        self.next(false);
                    }
                }
                Event::Error(e) => {
                    self.ui.set_busy(false);
                    self.song_failed(&format!("Playback stopped: {e}"));
                }
            }
        }
        self.poll_commands();
        // the sleep timer fades out over its last 10 seconds, then pauses
        if let Sleep::At(at) = self.sleep {
            let left = at.saturating_duration_since(Instant::now()).as_secs_f32();
            if left <= 0.0 {
                self.player.pause();
                self.set_sleep(0);
                self.apply_volume();
            } else if left < 10.0 {
                self.player
                    .set_volume(self.volume * self.gain * left / 10.0);
            }
        }
        let playing = self.p_is_playing();
        self.ui.set_playing(playing);
        if !self.loaded {
            return;
        }
        let pos = self.p_position();
        // nobody sees the progress or the lyrics in the background: caught up on return
        if !self.background {
            self.ui.set_position(pos as f32);
            self.ui.set_position_text(clock_text(pos).into());
            let li = lyrics::current_line(&self.lyrics, pos + 0.15).map_or(-1, |i| i as i32);
            if li != self.lyric_index {
                self.lyric_index = li;
                self.ui.set_lyric_index(li);
            }
        }
        self.publish_media(playing, pos);
    }

    /// Tells the system (notification, lock screen, HyperIsland / Dynamic Island) what is playing:
    /// on changes and every few seconds, not 4 times a second.
    fn publish_media(&mut self, playing: bool, pos: f64) {
        let Some(t) = &self.current else { return };
        let info = MediaInfo {
            title: js::str_or_empty(t.get("title")),
            artist: js::str_or_empty(t.get("artist")),
            artwork_url: js::str_or_empty(t.get("thumbnail")),
            playing,
            position: (pos * 2.0).round() / 2.0,
            duration: self
                .player
                .duration()
                .max(js::num_or_zero(t.get("duration"))),
        };
        if info.title != self.last_media.title
            || info.playing != self.last_media.playing
            || (info.position - self.last_media.position).abs() >= 5.0
        {
            (self.platform.media)(&info);
            self.last_media = info;
        }
    }

    // ---------- storage ----------

    fn restore_backup(&mut self) {
        self.ui
            .set_status("Choose a Pulse backup (.json) or Riff account file (.rfacc)".into());
        (self.platform.pick_backup)(Box::new(|bytes| {
            later(
                move |a| match a.store.restore_backup(&bytes, now_ms() as u64) {
                    Ok(c) => {
                        a.persist();
                        a.restore_settings();
                        a.apply_prefs();
                        a.apply_theme();
                        a.load_background();
                        a.refresh_all();
                        a.ui.set_status(
                            format!(
                                "Restored {}, {} and {}",
                                plural(c.playlists, "playlist"),
                                plural(c.favorites, "favorite"),
                                plural(c.history, "history song")
                            )
                            .into(),
                        );
                    }
                    Err(e) => a.ui.set_status(e.into()),
                },
            );
        }));
    }

    fn export_backup(&mut self) {
        let when = iso_now();
        let b = self.store.make_backup("", APP_VERSION, &when);
        let name = pulse_core::backup::file_name(&format!("pulse-backup-{}", &when[..10]), "json");
        match (self.platform.save_file)(&name, js::stringify(&b).as_bytes()) {
            Ok(where_) => self.ui.set_status(format!("Saved {where_}").into()),
            Err(e) => self
                .ui
                .set_status(format!("Couldn't save the backup: {e}").into()),
        }
    }

    /// Empties the app's own song cache (never the person's music files).
    fn clear_cache(&mut self) {
        let dir = self.platform.data_dir.join("cache");
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().is_file() && std::fs::remove_file(e.path()).is_ok() {
                    n += 1;
                }
            }
        }
        self.refresh_settings();
        self.ui
            .set_status(format!("Removed {} from the cache", plural(n, "song")).into());
    }
}

/// (files, bytes) in the song cache.
fn cache_stats(dir: &Path) -> (usize, u64) {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .fold((0, 0), |(n, b), m| (n + 1, b + m.len()))
        })
        .unwrap_or((0, 0))
}

fn regex_list_id(u: &str) -> Option<String> {
    let i = u.find("list=")? + 5;
    let id: String = u[i..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    (!id.is_empty()).then_some(id)
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` (Date#toISOString).
pub fn iso_now() -> String {
    let ms = now_ms() as i64;
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

/// A music file -> a library track (tags via lofty).
fn local_track(p: &Path) -> Value {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::Accessor;
    let name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let mut t = json!({ "title": name, "artist": "", "platform": "local", "isLocal": true, "path": p.display().to_string() });
    let hash = p
        .display()
        .to_string()
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
    t["id"] = json!(format!("local_{hash:016x}"));
    if let Ok(f) = lofty::read_from_path(p) {
        t["duration"] = json!(f.properties().duration().as_secs_f64().round());
        if let Some(tag) = f.primary_tag().or_else(|| f.first_tag()) {
            if let Some(x) = tag.title() {
                t["title"] = json!(x.to_string());
            }
            if let Some(x) = tag.artist() {
                t["artist"] = json!(x.to_string());
            }
            if let Some(x) = tag.album() {
                t["album"] = json!(x.to_string());
            }
        }
    }
    t
}

/// The song as a playable file: the local file, the cached copy, or a fresh download (cached).
type Downloads = Arc<std::sync::Mutex<HashMap<String, (PathBuf, PathBuf, Arc<Growing>)>>>;

/// Where a song's audio comes from.
enum Source {
    /// a whole file (local, cached)
    File(PathBuf),
    /// still downloading into `part`; `done` is where it ends up
    Growing { part: PathBuf, done: PathBuf, growing: Arc<Growing> },
}

/// How much has to be in before a downloading song starts (about a second of YouTube's AAC).
const START_BYTES: u64 = 192 * 1024;

/// The song's audio, as soon as it can play: the local file, the cached copy, a download already
/// under way, or a new download. `timeout` (Settings > Advanced > Stream timeout) bounds finding
/// the stream and how long the download may sit without anything arriving.
async fn audio_source(
    net: &Arc<Net>,
    t: &Value,
    cache: &Path,
    limit_mb: f64,
    timeout: Duration,
    downloads: &Downloads,
) -> Result<Source, String> {
    if let Some(p) = t.get("path").and_then(Value::as_str) {
        journal("play: local file");
        return Ok(Source::File(PathBuf::from(p)));
    }
    for ext in ["aac", "m4a", "mp3", "webm"] {
        let p = cache.join(cache_name(t, ext));
        if p.exists() {
            journal("play: cached copy");
            return Ok(Source::File(p));
        }
    }
    let key = cache_name(t, "");
    let existing = downloads.lock().unwrap_or_else(|e| e.into_inner()).get(&key).cloned();
    let (part, done, growing) = match existing {
        Some(d) => {
            journal("play: joining the download under way");
            d
        }
        None => {
            let target = js::str_or_empty(js::or(t, "url").or_else(|| t.get("id")));
            let target = if target.is_empty() {
                format!("ytsearch1:{} {}", js::str_or_empty(t.get("artist")), js::str_or_empty(t.get("title")))
            } else {
                target
            };
            journal("play: finding the stream");
            let audio = match tokio::time::timeout(timeout, pulse_net::resolve_audio(net, &target)).await {
                Ok(r) => r.map_err(|e| e.to_string())?,
                Err(_) => return Err(format!("no stream found in {} s", timeout.as_secs())),
            };
            std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
            let done = cache.join(cache_name(t, &audio.ext));
            let part = done.with_extension(format!("{}.part", audio.ext));
            let growing = Arc::new(Growing::default());
            downloads
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key.clone(), (part.clone(), done.clone(), growing.clone()));
            journal(&format!("play: downloading ({})", audio.ext));
            let (net, g, part2, done2, cache2, downloads2) =
                (net.clone(), growing.clone(), part.clone(), done.clone(), cache.to_path_buf(), downloads.clone());
            tokio::spawn(async move {
                let r = pulse_net::download_to(&net, &audio, &part2, |written, total| {
                    use std::sync::atomic::Ordering;
                    g.total.store(total.unwrap_or(0), Ordering::Release);
                    g.written.store(written, Ordering::Release);
                })
                .await;
                use std::sync::atomic::Ordering;
                match r {
                    Ok(()) => {
                        // renaming leaves an open file readable: the player keeps playing it
                        let _ = std::fs::rename(&part2, &done2);
                        g.done.store(true, Ordering::Release);
                        journal(&format!("play: downloaded {} KB", g.written.load(Ordering::Acquire) / 1024));
                        prune_cache(&cache2, (limit_mb.max(64.0) * 1024.0 * 1024.0) as u64, &done2);
                    }
                    Err(e) => {
                        g.failed.store(true, Ordering::Release);
                        let _ = std::fs::remove_file(&part2);
                        journal(&format!("play: download failed: {e}"));
                    }
                }
                downloads2.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
            });
            (part, done, growing)
        }
    };
    // wait for the first second or so (or the whole song, if it is small or already in)
    use std::sync::atomic::Ordering;
    let (mut last, mut since) = (0u64, std::time::Instant::now());
    loop {
        if growing.done.load(Ordering::Acquire) {
            return Ok(Source::File(done));
        }
        if growing.failed.load(Ordering::Acquire) {
            return Err("the download failed".into());
        }
        let w = growing.written.load(Ordering::Acquire);
        if w >= START_BYTES {
            return Ok(Source::Growing { part, done, growing });
        }
        if w != last {
            (last, since) = (w, std::time::Instant::now());
        } else if since.elapsed() > timeout {
            return Err(format!("nothing loaded in {} s", timeout.as_secs()));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The song's audio as a file: the local file, the cached copy, or found and downloaded.
/// `timeout` (Settings > Advanced > Stream timeout, pulse_stream_timeout) bounds finding the stream:
/// the download itself is many short requests, each with its own limit and retries, so a slow but
/// moving download isn't cut off (cutting it off at 12 s stopped most online songs from playing).
async fn audio_file(net: &Net, t: &Value, cache: &Path, limit_mb: f64, timeout: Duration) -> Result<PathBuf, String> {
    if let Some(p) = t.get("path").and_then(Value::as_str) {
        journal("play: local file");
        return Ok(PathBuf::from(p));
    }
    for ext in ["aac", "m4a", "mp3", "webm"] {
        let p = cache.join(cache_name(t, ext));
        if p.exists() {
            journal("play: cached copy");
            return Ok(p);
        }
    }
    let target = js::str_or_empty(js::or(t, "url").or_else(|| t.get("id")));
    let target = if target.is_empty() {
        format!(
            "ytsearch1:{} {}",
            js::str_or_empty(t.get("artist")),
            js::str_or_empty(t.get("title"))
        )
    } else {
        target
    };
    journal("play: finding the stream");
    let audio = match tokio::time::timeout(timeout, pulse_net::resolve_audio(net, &target)).await {
        Ok(r) => r.map_err(|e| e.to_string())?,
        Err(_) => return Err(format!("no stream found in {} s", timeout.as_secs())),
    };
    journal(&format!("play: downloading ({})", audio.ext));
    let bytes = pulse_net::download(net, &audio)
        .await
        .map_err(|e| e.to_string())?;
    journal(&format!("play: downloaded {} KB", bytes.len() / 1024));
    tokio::fs::create_dir_all(cache)
        .await
        .map_err(|e| e.to_string())?;
    let p = cache.join(cache_name(t, &audio.ext));
    tokio::fs::write(&p, bytes)
        .await
        .map_err(|e| e.to_string())?;
    prune_cache(cache, (limit_mb.max(64.0) * 1024.0 * 1024.0) as u64, &p);
    Ok(p)
}

/// Oldest cached songs go once the cache is over its limit (pulse_cache_limit_mb).
fn prune_cache(dir: &Path, limit: u64, keep: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, p) in files {
        if total <= limit {
            break;
        }
        if p != keep && std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

async fn fetch_thumb(net: &Net, url: &str, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = net.client.get(url).send().await.ok()?.bytes().await.ok()?;
    tokio::task::spawn_blocking(move || {
        let img = image::load_from_memory(&bytes)
            .ok()?
            .thumbnail(size, size)
            .to_rgba8();
        Some((img.width(), img.height(), img.into_raw()))
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ids_like_the_app() {
        assert_eq!(
            canonical_id(&json!({"id": "abc", "platform": "youtube"})),
            "youtube:abc"
        );
        assert_eq!(canonical_id(&json!({"url": "https://x"})), "yt:https://x");
        assert_eq!(
            canonical_id(&json!({"title": " Song ", "artist": "A", "duration": 200.4})),
            "yt:song|a|200"
        );
    }

    #[test]
    fn clock_and_iso() {
        assert_eq!(clock_text(0.0), "0:00");
        assert_eq!(clock_text(65.4), "1:05");
        assert_eq!(clock_text(3725.0), "1:02:05");
        let iso = iso_now();
        assert_eq!(iso.len(), 24, "{iso}");
        assert!(iso.ends_with('Z') && iso.starts_with("20"));
    }

    #[test]
    fn thumbs_lists_tints() {
        assert_eq!(
            big_thumb("https://lh3.googleusercontent.com/abc=w60-h60-l90-rj"),
            "https://lh3.googleusercontent.com/abc=w544-h544-l90-rj"
        );
        assert_eq!(
            regex_list_id("https://www.youtube.com/playlist?list=PL123_x-y&si=1").as_deref(),
            Some("PL123_x-y")
        );
        assert_eq!(
            cache_name(&json!({"id": "a/b", "platform": "youtube"}), "aac"),
            "youtube_a_b.aac"
        );
        assert_eq!(tint_of(&[200, 100, 0, 255]), (200, 100, 0));
        assert_eq!(tint_of(&[]), DEFAULT_TINT);
        assert_eq!(plural(1, "song"), "1 song");
    }
}
