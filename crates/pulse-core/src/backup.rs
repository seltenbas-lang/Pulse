//! backup-core.js: shared playlist files and profile backups (what goes in, what is let back in),
//! and Riff's `.rfacc` account files.
use std::collections::HashSet;
use std::io::Read;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::js::{self, re};

pub const FILE_BYTES: usize = 30_000_000;
pub const PLAYLISTS: usize = 2000;
pub const TRACKS_PER_PLAYLIST: usize = 20000;
pub const TRACKS: usize = 200_000;
pub const KEY_BYTES: usize = 8_000_000;
pub const TOTAL_BYTES: usize = 24_000_000;
const PLATFORMS: &[&str] = &["youtube", "ytmusic", "soundcloud", "local"];

static YT_ID: LazyLock<Regex> = LazyLock::new(|| re("^[A-Za-z0-9_-]{11}$"));
static LOCAL_ID: LazyLock<Regex> = LazyLock::new(|| re("^local_[A-Za-z0-9_-]{1,64}$"));
static SAFE_KEY: LazyLock<Regex> = LazyLock::new(|| re("^[A-Za-z_][A-Za-z0-9_]{0,39}$"));
static PLAYLIST_ID: LazyLock<Regex> = LazyLock::new(|| re("^[A-Za-z0-9_-]{1,64}$"));
static YT_HOSTS: LazyLock<Regex> =
    LazyLock::new(|| js::re_flags(r"^(?:www\.|m\.|music\.)?youtube\.com$|^youtu\.be$", "i"));
static SC_HOSTS: LazyLock<Regex> =
    LazyLock::new(|| js::re_flags(r"^(?:www\.|m\.)?soundcloud\.com$", "i"));
static SHORTS: LazyLock<Regex> = LazyLock::new(|| re(r"^\/(?:shorts|embed)\/([^/]+)"));
static FILE_CHARS: LazyLock<Regex> = LazyLock::new(|| re("[\\\\/:*?\"<>|\\u{0}-\\u{1f}]+"));
static KEY_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:pulse|devsize)_[A-Za-z0-9_.:-]{1,80}$"));
static NEVER: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        "^pulse_sync_",
        "^pulse_p:",
        "^pulse_secret_",
        "^pulse_profiles?(?:_|$)",
        "^pulse_debug_log$",
        "^pulse_custom_css",
        r"^pulse_bg_(?:url|vpath)$",
        "^pulse_banner_media$",
    ]
    .into_iter()
    .map(re)
    .collect()
});

/// Object.prototype names: `k in pl` is true for them, and `out.__proto__ = v` stores nothing.
const PROTO_KEYS: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
    "__proto__",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
];

fn is_obj(v: &Value) -> bool {
    v.is_object()
}

/// Text from a file: only strings and numbers count; control characters become spaces.
pub fn clip(v: Option<&Value>, n: usize) -> String {
    let s = match v {
        Some(Value::String(s)) => s.clone(),
        Some(x @ Value::Number(_)) => js::to_string(x),
        _ => String::new(),
    };
    let s: String = s
        .chars()
        .map(|c| {
            if (c as u32) < 0x20 || c == '\u{7f}' {
                ' '
            } else {
                c
            }
        })
        .collect();
    js::slice16(js::trim(&s), n).to_string()
}

fn parse_url(s: &str) -> Option<url::Url> {
    url::Url::parse(s).ok()
}

fn https_url(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) if js::len16(s) <= 2048 => parse_url(s)
            .filter(|u| u.scheme() == "https")
            .map(|u| u.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn host(u: &url::Url) -> String {
    u.host_str().unwrap_or("").to_string()
}

fn yt_id_from(url: &str) -> String {
    let Some(u) = parse_url(url) else {
        return String::new();
    };
    if !YT_HOSTS.is_match(&host(&u)) {
        return String::new();
    }
    let id = if host(&u).eq_ignore_ascii_case("youtu.be") {
        u.path().get(1..).unwrap_or("").to_string()
    } else {
        let v = u
            .query_pairs()
            .find(|(k, _)| k == "v")
            .map(|(_, v)| v.into_owned())
            .filter(|v| !v.is_empty());
        v.or_else(|| SHORTS.captures(u.path()).map(|c| c[1].to_string()))
            .unwrap_or_default()
    };
    if YT_ID.is_match(&id) {
        id
    } else {
        String::new()
    }
}

#[expect(
    clippy::neg_cmp_op_on_partial_ord,
    reason = "JS `!(d > 0)` is also true for NaN, on purpose"
)]
fn duration(t: &Value) -> f64 {
    let mut d = js::to_number(t.get("duration"));
    for (k, scale) in [("durationMs", 1000.0), ("duration_ms", 1000.0)] {
        let x = js::to_number(t.get(k));
        if !(d > 0.0) && x > 0.0 {
            d = x / scale;
        }
    }
    if d > 0.0 && d < 86400.0 {
        js::round(d)
    } else {
        0.0
    }
}

fn first_truthy<'a>(t: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| js::or(t, k))
}

fn merge(base: &Value, extra: Value) -> Value {
    let mut o = base.as_object().cloned().unwrap_or_default();
    if let Value::Object(e) = extra {
        o.extend(e);
    }
    Value::Object(o)
}

/// A song from a shared playlist file -> a Pulse track, or `None`. A song with only a title and
/// artist gets `needsMatch: true` (looked up on YouTube like an import).
pub fn clean_shared_track(t: &Value) -> Option<Value> {
    if !is_obj(t) {
        return None;
    }
    let title = clip(first_truthy(t, &["title", "name", "trackName"]), 300);
    let artist = clip(
        first_truthy(t, &["artist", "artistName", "author", "uploader"]),
        300,
    );
    let mut base =
        json!({ "title": if title.is_empty() { "Unknown" } else { &title }, "artist": artist });
    let album = clip(t.get("album"), 300);
    if !album.is_empty() {
        base["album"] = json!(album);
    }
    let d = duration(t);
    if d != 0.0 {
        base["duration"] = json!(d);
    }
    let thumb = ["thumbnail", "cover", "image", "artworkUrl"]
        .iter()
        .map(|k| https_url(t.get(*k)))
        .find(|u| !u.is_empty())
        .unwrap_or_default();
    let platform = js::str_or_empty(first_truthy(t, &["platform", "source"])).to_lowercase();
    let id = match t.get("id") {
        Some(x @ (Value::String(_) | Value::Number(_))) => js::to_string(x),
        _ => String::new(),
    };
    if platform == "local" || LOCAL_ID.is_match(&id) {
        if LOCAL_ID.is_match(&id) {
            return Some(merge(&base, json!({ "id": id, "platform": "local" })));
        }
        return (!title.is_empty()).then(|| merge(&base, json!({ "needsMatch": true })));
    }
    let url = https_url(t.get("url"));
    if let Some(u) = (!url.is_empty()).then(|| parse_url(&url)).flatten()
        && SC_HOSTS.is_match(&host(&u))
        && js::len16(u.path()) > 1
    {
        let cid = clip(Some(&json!(id)), 200);
        return Some(merge(
            &base,
            json!({ "id": if cid.is_empty() { &url } else { &cid }, "platform": "soundcloud", "url": url, "thumbnail": thumb }),
        ));
    }
    let vid = if YT_ID.is_match(&id) && platform != "soundcloud" {
        id
    } else {
        yt_id_from(&url)
    };
    if !vid.is_empty() {
        let thumbnail = if thumb.is_empty() {
            format!("https://i.ytimg.com/vi/{vid}/hqdefault.jpg")
        } else {
            thumb
        };
        return Some(merge(
            &base,
            json!({ "id": vid, "platform": "youtube", "url": format!("https://www.youtube.com/watch?v={vid}"), "thumbnail": thumbnail }),
        ));
    }
    if title.is_empty() {
        return None;
    }
    let mut extra = json!({});
    if !thumb.is_empty() {
        extra["thumbnail"] = json!(thumb);
    }
    extra["needsMatch"] = json!(true);
    Some(merge(&base, extra))
}

/// `JSON.parse(text.replace(/^﻿/, ''))`.
pub fn json_parse(text: &str) -> Option<Value> {
    serde_json::from_str(text.strip_prefix('\u{FEFF}').unwrap_or(text)).ok()
}

/// The playlists in a playlist file (Pulse's, Riff's, or a bare list of songs); empty when none.
pub fn parse_playlist_file(text: &str) -> Vec<Value> {
    if js::trim(text).is_empty() || js::len16(text) > FILE_BYTES {
        return Vec::new();
    }
    let Some(data) = json_parse(text) else {
        return Vec::new();
    };
    let raw: Vec<Value> = match &data {
        Value::Array(a)
            if a.iter()
                .any(|x| is_obj(x) && x.get("tracks").is_some_and(Value::is_array)) =>
        {
            a.clone()
        }
        Value::Array(a)
            if a.iter()
                .any(|x| is_obj(x) && first_truthy(x, &["title", "id", "url"]).is_some()) =>
        {
            vec![json!({ "name": "Imported playlist", "tracks": a })]
        }
        Value::Object(o) if o.get("playlists").is_some_and(Value::is_array) => {
            o["playlists"].as_array().cloned().unwrap_or_default()
        }
        Value::Object(o) if o.get("tracks").is_some_and(Value::is_array) => vec![data.clone()],
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    let mut total = 0;
    for p in raw.iter().take(PLAYLISTS) {
        let Some(list) = p
            .get("tracks")
            .and_then(Value::as_array)
            .filter(|_| is_obj(p))
        else {
            continue;
        };
        let mut tracks = Vec::new();
        for t in list.iter().take(TRACKS_PER_PLAYLIST) {
            if total >= TRACKS {
                break;
            }
            if let Some(c) = clean_shared_track(t) {
                tracks.push(c);
                total += 1;
            }
        }
        let name = clip(first_truthy(p, &["name", "title"]), 100);
        let mut pl = json!({ "name": if name.is_empty() { "Imported playlist" } else { &name }, "tracks": tracks });
        let description = clip(p.get("description"), 500);
        if !description.is_empty() {
            pl["description"] = json!(description);
        }
        let cover = https_url(p.get("cover"));
        if !cover.is_empty() {
            pl["cover"] = json!(cover);
        }
        out.push(pl);
    }
    out
}

/// A Pulse track -> a playlist-file song. Local files keep only their library id.
pub fn export_track(t: &Value) -> Option<Value> {
    if !is_obj(t) {
        return None;
    }
    let mut out =
        json!({ "title": clip(t.get("title"), 300), "artist": clip(t.get("artist"), 300) });
    let album = clip(t.get("album"), 300);
    if !album.is_empty() {
        out["album"] = json!(album);
    }
    let d = duration(t);
    if d != 0.0 {
        out["duration"] = json!(d);
    }
    let platform = t
        .get("platform")
        .and_then(Value::as_str)
        .filter(|p| PLATFORMS.contains(p))
        .unwrap_or("youtube");
    let id = clip(t.get("id"), 200);
    if platform == "local" || js::or(t, "isLocal").is_some() || LOCAL_ID.is_match(&id) {
        return Some(merge(
            &out,
            json!({ "id": id, "platform": "local", "source": "local" }),
        ));
    }
    let mut url = https_url(t.get("url"));
    if url.is_empty() && YT_ID.is_match(&id) {
        url = format!("https://www.youtube.com/watch?v={id}");
    }
    let cover = Some(https_url(t.get("thumbnail")))
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| https_url(t.get("cover")));
    let mut extra = json!({ "id": id, "url": url, "platform": platform, "source": platform });
    if !cover.is_empty() {
        extra["thumbnail"] = json!(cover);
        extra["cover"] = json!(cover);
    }
    Some(merge(&out, extra))
}

/// Pulse playlists -> a playlist file. `exported_at` is an ISO time.
pub fn playlist_file(playlists: &[Value], app_version: &str, exported_at: &str) -> Value {
    let pls: Vec<Value> = playlists
        .iter()
        .filter(|p| is_obj(p))
        .map(|p| {
            let name = clip(p.get("name"), 100);
            let tracks: Vec<Value> = js::list(p.get("tracks")).iter().filter_map(export_track).collect();
            let mut pl = json!({ "name": if name.is_empty() { "untitled" } else { &name }, "tracks": tracks });
            let description = clip(p.get("description"), 500);
            if !description.is_empty() {
                pl["description"] = json!(description);
            }
            let cover = https_url(p.get("cover"));
            if !cover.is_empty() {
                pl["cover"] = json!(cover);
            }
            pl
        })
        .collect();
    json!({ "app": "Pulse", "appVersion": app_version, "exportedAt": exported_at, "playlists": pls })
}

/// A file name from a playlist or profile name.
pub fn file_name(name: &str, ext: &str) -> String {
    let n = clip(Some(&json!(name)), 60);
    let n = FILE_CHARS.replace_all(&n, "_");
    let n = n.trim_start_matches('.');
    format!("{}.{ext}", if n.is_empty() { "pulse" } else { n })
}

fn never_key(k: &str) -> bool {
    NEVER.iter().any(|r| r.is_match(k))
        || k.strip_prefix("pulse_lastfm_")
            .is_some_and(|rest| rest != "enabled")
}

/// Keys a backup carries and a restore writes: Pulse's own data and settings, never the sync link,
/// profiles, logins or `is_device` keys.
pub fn backup_key_allowed(key: &str, is_device: &dyn Fn(&str) -> bool) -> bool {
    KEY_RE.is_match(key) && !never_key(key) && !is_device(key)
}

pub fn keep_on_restore(key: &str, is_device: &dyn Fn(&str) -> bool) -> bool {
    !backup_key_allowed(key, is_device)
}

/// The open profile's own keys -> a backup file.
pub fn make_backup<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a Value)>,
    profile_name: &str,
    app_version: &str,
    exported_at: &str,
    is_device: &dyn Fn(&str) -> bool,
) -> Value {
    let mut data = Map::new();
    for (k, v) in entries {
        if v.is_string() && backup_key_allowed(k, is_device) {
            data.insert(k.to_string(), v.clone());
        }
    }
    json!({ "app": "Pulse", "kind": "pulse-backup", "version": 1, "appVersion": app_version, "exportedAt": exported_at,
        "profile": { "name": clip(Some(&json!(profile_name)), 24) }, "data": data })
}

const URL_FIELDS: &[&str] = &[
    "url",
    "thumbnail",
    "cover",
    "image",
    "artworkUrl",
    "localThumbnail",
];

fn stored_url(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    if s.starts_with("data:image/") {
        return (js::len16(s) < 300_000).then(|| s.to_string());
    }
    let u = if js::len16(s) <= 4096 {
        parse_url(s)
    } else {
        None
    }?;
    matches!(u.scheme(), "https" | "http" | "file").then(|| s.to_string())
}

/// A stored track as the app saves it: flat text, numbers and booleans; only real picture and song
/// addresses. `None` without an id, title or url.
pub fn clean_stored_track(t: &Value) -> Option<Value> {
    let o = t.as_object()?;
    let mut out = Map::new();
    for (n, k) in js::key_order(o).into_iter().enumerate() {
        if n >= 60 {
            break;
        }
        if !SAFE_KEY.is_match(k) || k == "__proto__" {
            continue;
        }
        let v = &o[k];
        if URL_FIELDS.contains(&k.as_str()) {
            if let Some(u) = stored_url(v) {
                out.insert(k.clone(), json!(u));
            }
            continue;
        }
        match v {
            Value::String(s) => {
                out.insert(k.clone(), json!(js::slice16(s, 2048)));
            }
            Value::Number(x) if x.as_f64().is_some_and(f64::is_finite) => {
                out.insert(k.clone(), v.clone());
            }
            Value::Bool(_) => {
                out.insert(k.clone(), v.clone());
            }
            _ => {}
        }
    }
    let has = |k: &str| out.get(k).is_some_and(js::truthy);
    if !has("id") && !has("title") && !has("url") {
        return None;
    }
    if out
        .get("platform")
        .is_some_and(|p| !p.as_str().is_some_and(|p| PLATFORMS.contains(&p)))
    {
        out.insert("platform".into(), json!("youtube"));
    }
    Some(Value::Object(out))
}

fn clean_track_list(list: &Value, cap: usize) -> Option<Value> {
    Some(Value::Array(
        list.as_array()?
            .iter()
            .take(cap)
            .filter_map(clean_stored_track)
            .collect(),
    ))
}

/// Stored playlists, each with a unique id (`new_id` makes one for those without).
pub fn clean_stored_playlists(list: &Value, new_id: &mut dyn FnMut() -> String) -> Option<Value> {
    let list = list.as_array()?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for p in list.iter().take(PLAYLISTS) {
        let Some(po) = p.as_object() else { continue };
        let mut id = po
            .get("id")
            .and_then(Value::as_str)
            .filter(|i| PLAYLIST_ID.is_match(i))
            .unwrap_or("")
            .to_string();
        while id.is_empty() || seen.contains(&id) {
            id = new_id();
        }
        seen.insert(id.clone());
        let name = clip(po.get("name"), 100);
        let tracks = po
            .get("tracks")
            .and_then(|t| clean_track_list(t, TRACKS_PER_PLAYLIST))
            .unwrap_or_else(|| json!([]));
        let mut pl = Map::new();
        pl.insert("id".into(), json!(id));
        pl.insert(
            "name".into(),
            json!(if name.is_empty() { "untitled" } else { &name }),
        );
        pl.insert("tracks".into(), tracks);
        for k in js::key_order(po) {
            if pl.contains_key(k) || PROTO_KEYS.contains(&k.as_str()) || !SAFE_KEY.is_match(k) {
                continue;
            }
            let v = &po[k];
            if k == "cover" {
                if let Some(u) = stored_url(v) {
                    pl.insert("cover".into(), json!(u));
                }
            } else if let Value::String(s) = v {
                pl.insert(k.clone(), json!(js::slice16(s, 2000)));
            } else if v.as_f64().is_some_and(f64::is_finite) || v.is_boolean() {
                pl.insert(k.clone(), v.clone());
            }
        }
        out.push(Value::Object(pl));
    }
    Some(Value::Array(out))
}

/// A playlist id like the app makes: `pl_<time base36><random>`.
pub fn new_playlist_id(now_ms: u64, random: u64) -> String {
    format!(
        "pl_{}{}",
        base36(now_ms),
        &base36(random % 60_466_176 + 60_466_176)[1..]
    )
}

pub fn base36(mut n: u64) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Checks one structured key's parsed value through (playlists, favourites, history).
type Cleaner = Box<dyn Fn(&Value, &mut dyn FnMut() -> String) -> Option<Value>>;

/// Upstream Riff's key names: riff_x is pulse_x.
fn from_riff(k: &str) -> String {
    k.strip_prefix("riff_")
        .map_or_else(|| k.to_string(), |r| format!("pulse_{r}"))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Restore {
    pub source: String,
    pub entries: Map<String, Value>,
    pub skipped: Vec<String>,
    pub profile_name: String,
    pub counts: Counts,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Counts {
    pub playlists: usize,
    pub favorites: usize,
    pub history: usize,
    pub keys: usize,
}

/// A backup file's contents -> what a restore writes. Errors are messages for people.
pub fn read_backup(
    obj: &Value,
    is_device: &dyn Fn(&str) -> bool,
    new_id: &mut dyn FnMut() -> String,
) -> Result<Restore, String> {
    if !is_obj(obj) {
        return Err("That file isn't a Pulse backup".into());
    }
    let (source, raw) = if obj.get("kind").and_then(Value::as_str) == Some("pulse-backup")
        && obj.get("data").is_some_and(is_obj)
    {
        ("pulse", &obj["data"])
    } else if obj.get("localStorage").is_some_and(is_obj)
        && matches!(
            obj.get("app").and_then(Value::as_str),
            Some("Riff" | "Pulse")
        )
    {
        ("riff", &obj["localStorage"])
    } else {
        return Err("That file isn't a Pulse backup or a Riff account file".into());
    };
    let raw = raw.as_object().ok_or("That file isn't a Pulse backup")?;
    let mut entries = Map::new();
    let mut skipped = Vec::new();
    let mut total = 0;
    for k0 in js::key_order(raw).into_iter().take(5000) {
        let k = if source == "riff" {
            from_riff(k0)
        } else {
            k0.clone()
        };
        let v = &raw[k0];
        if !backup_key_allowed(&k, is_device) || v.is_null() {
            skipped.push(k);
            continue;
        }
        let mut v = match v {
            Value::String(s) => s.clone(),
            other => js::stringify(other),
        };
        if js::len16(&v) > KEY_BYTES {
            skipped.push(k);
            continue;
        }
        let structured: Option<Cleaner> = match k.as_str() {
            "devsize_playlists" => Some(Box::new(|v, id| clean_stored_playlists(v, id))),
            "devsize_favorites" => Some(Box::new(|v, _| clean_track_list(v, TRACKS))),
            "devsize_history" => Some(Box::new(|v, _| clean_track_list(v, 5000))),
            _ => None,
        };
        if let Some(clean) = structured {
            let Some(parsed) = serde_json::from_str::<Value>(&v).ok() else {
                skipped.push(k);
                continue;
            };
            let Some(c) = clean(&parsed, new_id) else {
                skipped.push(k);
                continue;
            };
            v = js::stringify(&c);
        }
        total += js::len16(&k) + js::len16(&v);
        if total > TOTAL_BYTES {
            return Err("That backup is too big to restore".into());
        }
        entries.insert(k, json!(v));
    }
    let count = |k: &str| {
        entries.get(k).and_then(Value::as_str).map_or(0, |s| {
            serde_json::from_str::<Value>(s)
                .ok()
                .and_then(|v| v.as_array().map(Vec::len))
                .unwrap_or(0)
        })
    };
    let counts = Counts {
        playlists: count("devsize_playlists"),
        favorites: count("devsize_favorites"),
        history: count("devsize_history"),
        keys: entries.len(),
    };
    let profile_name = if source == "pulse" && obj.get("profile").is_some_and(is_obj) {
        clip(obj["profile"].get("name"), 24)
    } else {
        String::new()
    };
    Ok(Restore {
        source: source.into(),
        entries,
        skipped,
        profile_name,
        counts,
    })
}

// ---------- Riff account files (.rfacc) ----------

/// "RFAC", a 12-byte IV, a 16-byte GCM tag, then AES-256-GCM of deflate-raw(JSON).
pub fn is_riff_account(bytes: &[u8]) -> bool {
    bytes.len() > 32 && bytes.starts_with(b"RFAC")
}

/// Opens a Riff account file. The key is sha256("riff-account-key-v1"), public in Riff's source.
pub fn read_riff_account(bytes: &[u8]) -> Result<Value, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use sha2::Digest;
    if !is_riff_account(bytes) {
        return Err("That file isn't a Riff account file".into());
    }
    let key = sha2::Sha256::digest(b"riff-account-key-v1");
    let cipher = aes_gcm::Aes256Gcm::new_from_slice(&key)
        .map_err(|_| "That Riff account file is damaged")?;
    let mut sealed = bytes[32..].to_vec();
    sealed.extend_from_slice(&bytes[16..32]);
    let plain = cipher
        .decrypt(aes_gcm::Nonce::from_slice(&bytes[4..16]), sealed.as_slice())
        .map_err(|_| "That Riff account file is damaged")?;
    let mut json = Vec::new();
    let max = (TOTAL_BYTES * 2) as u64;
    flate2::read::DeflateDecoder::new(plain.as_slice())
        .take(max + 1)
        .read_to_end(&mut json)
        .map_err(|_| "That Riff account file is damaged")?;
    if json.len() as u64 > max {
        return Err("That account file is too big".into());
    }
    serde_json::from_str(&String::from_utf8_lossy(&json))
        .map_err(|_| "That Riff account file is damaged".into())
}

/// Bytes of a picked backup file -> its parsed contents.
pub fn parse_backup_file(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > FILE_BYTES {
        return Err("That file is too big".into());
    }
    if is_riff_account(bytes) {
        return read_riff_account(bytes);
    }
    json_parse(&String::from_utf8_lossy(bytes))
        .ok_or_else(|| "That file isn't a Pulse backup".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riff_account_round_trip() {
        use aes_gcm::aead::{Aead, KeyInit};
        use sha2::Digest;
        use std::io::Write;
        let body = br#"{"app":"Riff","localStorage":{"riff_theme":"dark"}}"#;
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(body).unwrap();
        let deflated = z.finish().unwrap();
        let key = sha2::Sha256::digest(b"riff-account-key-v1");
        let iv = [7u8; 12];
        let sealed = aes_gcm::Aes256Gcm::new_from_slice(&key)
            .unwrap()
            .encrypt(aes_gcm::Nonce::from_slice(&iv), deflated.as_slice())
            .unwrap();
        let (ct, tag) = sealed.split_at(sealed.len() - 16);
        let mut file = b"RFAC".to_vec();
        file.extend_from_slice(&iv);
        file.extend_from_slice(tag);
        file.extend_from_slice(ct);
        let v = parse_backup_file(&file).unwrap();
        let r = read_backup(&v, &|_| false, &mut || "x".into()).unwrap();
        assert_eq!(r.source, "riff");
        assert_eq!(r.entries["pulse_theme"], "dark");
        file[40] ^= 1;
        assert_eq!(
            parse_backup_file(&file).unwrap_err(),
            "That Riff account file is damaged"
        );
    }

    #[test]
    fn playlist_ids_look_like_the_apps() {
        let id = new_playlist_id(1_700_000_000_000, 12345);
        assert!(id.starts_with("pl_") && id.len() == 3 + 8 + 5, "{id}");
    }
}
