//! track-sort.js: song and library sorting, favourite keys and duplicate detection.
//! Tracks stay JSON objects so stored fields the app does not know survive (ADR-005).
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use icu_collator::CollatorPreferences;
use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed};
use regex::Regex;
use serde_json::Value;

use crate::js::{self, re, re_flags};

pub const TRACK_SORT_MODES: &[&str] = &[
    "manual",
    "added-desc",
    "added-asc",
    "title-asc",
    "title-desc",
    "artist-asc",
    "album-asc",
    "duration-asc",
];

/// `a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true })`.
static COLLATOR: LazyLock<CollatorBorrowed<'static>> = LazyLock::new(|| {
    let mut prefs = CollatorPreferences::default();
    prefs.numeric_ordering = Some(CollationNumericOrdering::True);
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Primary);
    // INFALLIBLE: compiled data always has the root collation.
    Collator::try_new(prefs, options).unwrap_or_else(|e| panic!("collator: {e}"))
});

pub fn locale_compare(a: &str, b: &str) -> Ordering {
    COLLATOR.compare(a, b)
}

fn text<'a>(t: &'a Value, key: &str) -> &'a str {
    t.get(key)
        .and_then(Value::as_str)
        .map(js::trim)
        .unwrap_or("")
}

fn compare_text(a: &Value, b: &Value, key: &str) -> Ordering {
    let (av, bv) = (text(a, key), text(b, key));
    match (av.is_empty(), bv.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => locale_compare(av, bv),
    }
}

/// Z to A, still with the untitled songs at the end.
fn compare_text_desc(a: &Value, b: &Value, key: &str) -> Ordering {
    let (av, bv) = (text(a, key), text(b, key));
    if av.is_empty() || bv.is_empty() {
        return compare_text(a, b, key);
    }
    locale_compare(bv, av)
}

fn added_time(t: &Value) -> f64 {
    t.get("addedAt").and_then(Value::as_f64).unwrap_or(0.0)
}

fn compare_number(a: &Value, b: &Value, key: &str) -> Ordering {
    let v = |t: &Value| {
        js::finite(t.get(key))
            .filter(|x| *x > 0.0)
            .unwrap_or(f64::INFINITY)
    };
    v(a).partial_cmp(&v(b)).unwrap_or(Ordering::Equal)
}

fn sign(x: f64) -> Ordering {
    if x > 0.0 {
        Ordering::Greater
    } else if x < 0.0 {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}

pub fn sort_tracks(tracks: &[Value], mode: &str) -> Vec<Value> {
    let mut list = tracks.to_vec();
    match mode {
        "title-asc" => list.sort_by(|a, b| compare_text(a, b, "title")),
        "title-desc" => list.sort_by(|a, b| compare_text_desc(a, b, "title")),
        "album-asc" => list.sort_by(|a, b| {
            compare_text(a, b, "album")
                .then_with(|| compare_number(a, b, "disc"))
                .then_with(|| compare_number(a, b, "no"))
                .then_with(|| sign(added_time(a) - added_time(b)))
        }),
        "artist-asc" => list.sort_by(|a, b| compare_text(a, b, "artist")),
        "duration-asc" => list.sort_by(|a, b| compare_number(a, b, "duration")),
        "added-desc" => list.sort_by(|a, b| {
            added_time(b)
                .partial_cmp(&added_time(a))
                .unwrap_or(Ordering::Equal)
        }),
        "added-asc" => list.sort_by(|a, b| {
            added_time(a)
                .partial_cmp(&added_time(b))
                .unwrap_or(Ordering::Equal)
        }),
        _ => {}
    }
    list
}

/// Gives tracks without `addedAt` one, spaced a second apart from `now` (ms), keeping list order.
pub fn backfill_added_at(tracks: &[Value], newest_first: bool, now: f64) -> Vec<Value> {
    let len = tracks.len() as f64;
    tracks
        .iter()
        .enumerate()
        .map(|(i, t)| match t {
            Value::Object(o) if !o.get("addedAt").is_some_and(Value::is_number) => {
                let mut o = o.clone();
                let back = if newest_first {
                    i as f64
                } else {
                    len - i as f64
                };
                o.insert("addedAt".into(), Value::from(now - back * 1000.0));
                Value::Object(o)
            }
            t => t.clone(),
        })
        .collect()
}

pub fn library_sort_modes(kind: &str) -> &'static [&'static str] {
    match kind {
        "albums" => &["custom", "title-asc", "artist-asc", "year-desc"],
        "playlists" => &["custom", "name-asc", "name-desc", "size-desc"],
        "artists" => &["custom", "name-asc", "plays-desc"],
        _ => &["custom"],
    }
}

pub fn library_sort_mode(kind: &str, mode: &str) -> &'static str {
    library_sort_modes(kind)
        .iter()
        .copied()
        .find(|m| *m == mode)
        .unwrap_or("custom")
}

/// Sorts the Albums, playlists or Artists page; ties keep the stored order. `plays` counts an
/// artist's plays for `plays-desc`.
pub fn sort_library(
    kind: &str,
    items: &[Value],
    mode: &str,
    plays: Option<&dyn Fn(&Value) -> f64>,
) -> Vec<Value> {
    let list: Vec<&Value> = items.iter().filter(|x| js::truthy(x)).collect();
    let m = library_sort_mode(kind, mode);
    if m == "custom" {
        return list.into_iter().cloned().collect();
    }
    let year = |x: &Value| {
        let y = js::parse_int(&js::str_or_empty(x.get("year")));
        if y.is_finite() { y } else { f64::NEG_INFINITY }
    };
    let size = |x: &Value| {
        x.get("tracks")
            .and_then(Value::as_array)
            .map_or(0.0, |a| a.len() as f64)
    };
    let counts: Vec<f64> = list
        .iter()
        .map(|x| {
            let n = plays.map_or(0.0, |p| p(x));
            if n.is_nan() { 0.0 } else { n }
        })
        .collect();
    let num = |x: f64| if x.is_nan() { Ordering::Equal } else { sign(x) };
    let mut idx: Vec<usize> = (0..list.len()).collect();
    idx.sort_by(|&i, &j| {
        let (a, b) = (list[i], list[j]);
        let c = match m {
            "title-asc" => compare_text(a, b, "title").then_with(|| compare_text(a, b, "artist")),
            "artist-asc" => compare_text(a, b, "artist").then_with(|| compare_text(a, b, "title")),
            "year-desc" => num(year(b) - year(a)).then_with(|| compare_text(a, b, "title")),
            "name-asc" => compare_text(a, b, "name"),
            "name-desc" => compare_text_desc(a, b, "name"),
            "size-desc" => num(size(b) - size(a)).then_with(|| compare_text(a, b, "name")),
            "plays-desc" => num(counts[j] - counts[i]).then_with(|| compare_text(a, b, "name")),
            _ => Ordering::Equal,
        };
        c.then(i.cmp(&j))
    });
    idx.into_iter().map(|i| list[i].clone()).collect()
}

static PREFIX: LazyLock<Regex> = LazyLock::new(|| re_flags("^(?:yt|youtube|ytm|ytmusic):", "i"));
static PREFIX_SC: LazyLock<Regex> =
    LazyLock::new(|| re_flags("^(?:yt|youtube|ytm|ytmusic|sc|soundcloud):", "i"));
static VIDEO_ID: LazyLock<Regex> = LazyLock::new(|| re("^[A-Za-z0-9_-]{11}$"));
static YT_URL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"^https?:\/\/(?:(?:www|m|music)\.)?(?:youtube\.com\/(?:watch\?(?:.*&)?v=|shorts\/|embed\/)|youtu\.be\/)([A-Za-z0-9_-]{11})",
    )
});
static TITLE_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\s*[([](?:[^)\]]*\b(?:official|audio|video|lyrics?|visuali[sz]er|hd|hq|4k|remaster(?:ed)?|explicit|clean|mv|m\/v)\b[^)\]]*)[)\]]",
        "i",
    )
});
static DASH_NOISE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\s+-\s+(?:official|audio|video|lyrics?).*$"));
static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| re(r"[^\p{L}\p{N}]+"));
static TOPIC: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s*-\s*topic$", "i"));
static ARTIST_SPLIT: LazyLock<Regex> =
    LazyLock::new(|| re_flags(r"\s*(?:,|&|\bfeat\.?|\bft\.?|\bx\b|\band\b|;)\s*", "i"));

fn youtube_id_of(t: &Value) -> String {
    let raw = js::str_or_empty(js::or(t, "id").or_else(|| t.get("videoId")));
    let id = PREFIX.replace(&raw, "");
    if VIDEO_ID.is_match(&id) {
        return id.into_owned();
    }
    let url = js::str_or_empty(t.get("url"));
    YT_URL
        .captures(&url)
        .map(|c| c[1].to_string())
        .unwrap_or_default()
}

/// A favourite's identity, the same for one song reached from an album row, a search or the player.
pub fn favorite_track_key(t: &Value) -> String {
    if !js::truthy(t) {
        return String::new();
    }
    let platform = js::str_or_empty(t.get("platform")).to_lowercase();
    if matches!(platform.as_str(), "" | "youtube" | "ytmusic" | "yt") {
        let yt = youtube_id_of(t);
        if !yt.is_empty() {
            return format!("youtube:{yt}");
        }
    }
    let p = if matches!(platform.as_str(), "" | "ytmusic" | "yt") {
        "youtube"
    } else {
        platform.as_str()
    };
    if let Some(id) = js::or(t, "id") {
        return format!("{p}:{}", PREFIX_SC.replace(&js::to_string(id), ""));
    }
    if let Some(url) = js::or(t, "url") {
        return format!("{p}:{}", js::to_string(url));
    }
    format!(
        "{p}:{}|{}",
        song_key(t),
        js::num_to_string(js::round(js::num_or_zero(t.get("duration"))))
    )
}

fn norm_title(s: &str) -> String {
    let t = TITLE_NOISE.replace_all(s, "").to_lowercase();
    let t = js::strip_marks(&js::nfkd(&t));
    let t = DASH_NOISE.replace(&t, "");
    js::trim(&NON_ALNUM.replace_all(&t, " ")).to_string()
}

fn norm_artist(s: &str) -> String {
    let a = TOPIC.replace(s, "");
    norm_title(ARTIST_SPLIT.split(&a).next().unwrap_or(""))
}

fn song_key(t: &Value) -> String {
    if !js::truthy(t) {
        return String::new();
    }
    format!(
        "{}|{}",
        norm_title(&js::str_or_empty(t.get("title"))),
        norm_artist(&js::str_or_empty(t.get("artist")))
    )
}

/// Same song: same favourite key, or (not local files) same title and artist and a close length.
pub fn is_same_song(a: &Value, b: &Value) -> bool {
    if !js::truthy(a) || !js::truthy(b) {
        return false;
    }
    if favorite_track_key(a) == favorite_track_key(b) {
        return true;
    }
    let local = |t: &Value| js::str_or_empty(t.get("platform")).to_lowercase() == "local";
    if local(a) || local(b) {
        return false;
    }
    let title = js::str_or_empty(a.get("title"));
    if norm_title(&title).is_empty()
        || norm_artist(&js::str_or_empty(a.get("artist"))).is_empty()
        || song_key(a) != song_key(b)
    {
        return false;
    }
    let (da, db) = (
        js::num_or_zero(a.get("duration")),
        js::num_or_zero(b.get("duration")),
    );
    !(da > 0.0 && db > 0.0 && (da - db).abs() > 6.0)
}

/// Keeps the first copy of every song (favourites are newest first, so the newest heart wins).
pub fn dedupe_favorite_tracks(tracks: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut keys = HashSet::new();
    let mut by_song: HashMap<String, Vec<&Value>> = HashMap::new();
    for t in tracks.iter().filter(|t| js::truthy(t)) {
        let k = favorite_track_key(t);
        let same = by_song.entry(song_key(t)).or_default();
        if keys.contains(&k) || same.iter().any(|o| is_same_song(o, t)) {
            continue;
        }
        keys.insert(k);
        same.push(t);
        out.push(t.clone());
    }
    out
}
