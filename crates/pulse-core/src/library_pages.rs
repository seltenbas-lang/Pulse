//! library-pages.js: saved albums, followed artists, Daily Mixes and their ratings, the explicit
//! badge, and blocked artists and songs. Entries stay JSON (ADR-005).
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::js::{self, list, re, re_flags};

static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| re(r"[^\p{L}\p{N}]"));
static TOPIC: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s*-\s*Topic$", "i"));
static EXPLICIT: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\bexplicit\b|[[(]\s*(?:e|18\+|\+18|dirty)\s*[)\]]|\buncensored\b|без\s+цензуры|нецензурн",
        "i",
    )
});
static ARTIST_SPLIT: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\s*(?:,|&|\+|\/|;|\bx\b|\bfeat\.?|\bft\.?|\bfeaturing\b)\s*",
        "i",
    )
});

pub const CAP_TRACKS: usize = 300;
pub const CAP_ARTISTS: usize = 150;
pub const CAP_RATINGS: usize = 60;

/// Case, accents, spaces and punctuation aside.
pub fn fold_str(s: &str) -> String {
    NON_ALNUM
        .replace_all(&js::nfkd(s).to_lowercase(), "")
        .into_owned()
}

fn fold(v: Option<&Value>) -> String {
    fold_str(&js::str_nullish(v))
}

/// "NOBODYKNOWS - Topic" is "nobodyknows".
pub fn name_key_str(s: &str) -> String {
    fold_str(&TOPIC.replace(s, ""))
}

pub fn name_key(v: Option<&Value>) -> String {
    name_key_str(&js::str_nullish(v))
}

fn get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    v.get(k)
}

fn same_id(a: &Value, b: &Value) -> bool {
    match (js::or(a, "id"), js::or(b, "id")) {
        (Some(x), Some(y)) => js::to_string(x) == js::to_string(y),
        _ => false,
    }
}

// ---------- saved albums ----------

pub fn same_album(a: &Value, b: &Value) -> bool {
    if !js::truthy(a) || !js::truthy(b) {
        return false;
    }
    if same_id(a, b) {
        return true;
    }
    let t = fold(get(a, "title"));
    !t.is_empty()
        && t == fold(get(b, "title"))
        && name_key(get(a, "artist")) == name_key(get(b, "artist"))
}

pub fn is_album_saved(saved: &[Value], album: &Value) -> bool {
    saved.iter().any(|a| same_album(a, album))
}

/// What a saved album keeps: enough to draw its card and open it again.
pub fn album_entry(album: &Value, now: f64) -> Value {
    let tracks = list(get(album, "_tracks")).len() as f64;
    let count = js::num_or_zero(get(album, "trackCount"));
    let mut out = json!({
        "id": js::str_or_empty(get(album, "id")),
        "title": js::or_default(album, "title", json!("")),
        "artist": js::or_default(album, "artist", json!("")),
        "cover": js::or_default(album, "cover", json!("")),
        "type": js::or_default(album, "type", json!("Album")),
        "year": js::or_default(album, "year", json!("")),
        "trackCount": if count != 0.0 { count } else { tracks },
        "savedAt": now,
    });
    if let Some(src) = js::or(album, "source") {
        out["source"] = src.clone();
    }
    out
}

/// Saving puts the album first; saving it again removes it. Returns (list, saved).
pub fn toggle_saved_album(saved: &[Value], album: &Value, now: f64) -> (Vec<Value>, bool) {
    if is_album_saved(saved, album) {
        return (
            saved
                .iter()
                .filter(|a| !same_album(a, album))
                .cloned()
                .collect(),
            false,
        );
    }
    let mut out = vec![album_entry(album, now)];
    out.extend_from_slice(saved);
    (out, true)
}

// ---------- followed artists ----------

pub fn follow_matches(entry: &Value, artist: &Value) -> bool {
    if !js::truthy(entry) || !js::truthy(artist) {
        return false;
    }
    let ids: Vec<&Value> = std::iter::once(get(artist, "id").unwrap_or(&Value::Null))
        .chain(list(get(artist, "ids")))
        .filter(|x| js::truthy(x))
        .collect();
    if let Some(id) = js::or(entry, "id")
        && !ids.is_empty()
    {
        return ids.contains(&id);
    }
    let k = name_key(get(artist, "name"));
    !k.is_empty() && name_key(get(entry, "name")) == k
}

pub fn is_artist_followed(followed: &[Value], artist: &Value) -> bool {
    followed.iter().any(|e| follow_matches(e, artist))
}

pub fn follow_entry(artist: &Value, now: f64) -> Value {
    let mut out = json!({
        "name": js::trim(&js::str_or_empty(get(artist, "name"))),
        "avatar": js::or_default(artist, "avatar", json!("")),
        "fans": js::num_or_zero(get(artist, "fans")),
        "followedAt": now,
    });
    if let Some(id) = js::or(artist, "id") {
        out["id"] = id.clone();
    }
    out
}

/// Following puts the artist first; following again unfollows every entry that is this artist.
pub fn toggle_follow_artist(followed: &[Value], artist: &Value, now: f64) -> (Vec<Value>, bool) {
    if is_artist_followed(followed, artist) {
        return (
            followed
                .iter()
                .filter(|e| !follow_matches(e, artist))
                .cloned()
                .collect(),
            false,
        );
    }
    if name_key(get(artist, "name")).is_empty() {
        return (followed.to_vec(), false);
    }
    let mut out = vec![follow_entry(artist, now)];
    out.extend_from_slice(followed);
    (out, true)
}

/// Entries of `artist` learn what its page knows (channel id, picture, fans). Returns (list, changed).
pub fn refresh_followed(followed: &[Value], artist: &Value) -> (Vec<Value>, bool) {
    let mut changed = false;
    let out = followed
        .iter()
        .map(|e| {
            if !follow_matches(e, artist) {
                return e.clone();
            }
            let mut next = e.as_object().cloned().unwrap_or_default();
            if let Some(id) = js::or(artist, "id")
                && js::or(e, "id").is_none()
            {
                next.insert("id".into(), id.clone());
            }
            if let Some(av) = js::or(artist, "avatar")
                && Some(av) != get(e, "avatar")
            {
                next.insert("avatar".into(), av.clone());
            }
            if js::or(artist, "fans").is_some() {
                let fans = js::to_number(get(artist, "fans"));
                if get(e, "fans").and_then(Value::as_f64) != Some(fans) {
                    next.insert("fans".into(), json!(fans));
                }
            }
            if let Some(name) = js::or(artist, "name")
                && js::or(e, "name").is_none()
            {
                next.insert("name".into(), name.clone());
            }
            let next = Value::Object(next);
            if next.as_object().is_some_and(|n| {
                n.iter()
                    .all(|(k, v)| get(e, k).is_some_and(|w| js_same(v, w)))
            }) {
                return e.clone();
            }
            changed = true;
            next
        })
        .collect();
    (out, changed)
}

/// `===` for JSON values that came from the same object (numbers by value).
fn js_same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

// ---------- Daily Mixes ----------

/// `YYYY-MM-DD`.
pub fn local_day(year: i32, month: u32, day: u32) -> String {
    format!("{year}-{month:02}-{day:02}")
}

fn weight(t: &Value) -> f64 {
    js::or(t, "weight").map_or(0.0, |w| js::to_number(Some(w)))
}

/// The most played artists spread over up to `max` mixes, each with its two most played songs as seeds.
pub fn plan_mixes(seeds: &[Value], day: &str, max: usize) -> Vec<Value> {
    let mut order: Vec<String> = Vec::new();
    let mut by_artist: HashMap<String, (Value, Vec<Value>)> = HashMap::new();
    for s in seeds {
        let Some(artist) = js::or(s, "artist") else {
            continue;
        };
        let k = name_key(Some(artist));
        if k.is_empty() {
            continue;
        }
        by_artist
            .entry(k.clone())
            .or_insert_with(|| {
                order.push(k.clone());
                (artist.clone(), Vec::new())
            })
            .1
            .push(s.clone());
    }
    let mut artists: Vec<(Value, Vec<Value>, f64)> = order
        .iter()
        .filter_map(|k| by_artist.remove(k))
        .map(|(name, mut tracks)| {
            tracks.sort_by(|x, y| {
                (weight(y) - weight(x))
                    .partial_cmp(&0.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            tracks.truncate(2);
            let w = tracks.iter().map(weight).sum();
            (name, tracks, w)
        })
        .collect();
    artists.sort_by(|a, b| {
        (b.2 - a.2)
            .partial_cmp(&0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    artists.truncate(15);
    if artists.len() < 4 {
        return Vec::new();
    }
    let count = max.min(artists.len() / 2);
    if count == 0 {
        return Vec::new();
    }
    let mut buckets: Vec<(Vec<Value>, Vec<Value>)> = vec![(Vec::new(), Vec::new()); count];
    for (rank, (name, tracks, _)) in artists.into_iter().enumerate() {
        let b = &mut buckets[rank % count];
        b.0.push(name);
        b.1.extend(tracks);
    }
    buckets
        .into_iter()
        .enumerate()
        .map(|(i, (artists, mut seeds))| {
            seeds.truncate(6);
            json!({ "id": format!("daily-mix-{day}-{}", i + 1), "title": format!("Daily Mix {}", i + 1), "artists": artists, "seeds": seeds })
        })
        .collect()
}

// ---------- thumbs up / down on a Daily Mix ----------

pub fn clean_feedback(v: &Value) -> Value {
    let o = if v.is_object() || v.is_array() {
        v
    } else {
        &Value::Null
    };
    let strings = |k: &str| {
        list(o.get(k))
            .iter()
            .filter(|a| a.as_str().is_some_and(|s| !js::trim(s).is_empty()))
            .cloned()
            .collect::<Vec<_>>()
    };
    let objects = |k: &str| {
        list(o.get(k))
            .iter()
            .filter(|t| t.is_object() || t.is_array())
            .cloned()
            .collect::<Vec<_>>()
    };
    let ratings = o
        .get("ratingsByMix")
        .filter(|r| r.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    json!({
        "likedArtists": strings("likedArtists"),
        "dislikedArtists": strings("dislikedArtists"),
        "likedTracks": objects("likedTracks"),
        "dislikedTracks": objects("dislikedTracks"),
        "ratingsByMix": ratings,
    })
}

pub fn is_mix_rated(feedback: &Value, mix_id: &str) -> bool {
    !mix_id.is_empty()
        && clean_feedback(feedback)["ratingsByMix"]
            .get(mix_id)
            .is_some_and(js::truthy)
}

/// The default track key: `<artist name key>::<folded title>`.
pub fn default_track_key(t: &Value) -> String {
    format!("{}::{}", name_key(get(t, "artist")), fold(get(t, "title")))
}

/// A liked mix: its songs and artists count for more; a disliked one: they are avoided (never the
/// artists the mix was built from).
pub fn rate_mix(feedback: &Value, mix: &Value, is_like: bool) -> Value {
    let mut fb = clean_feedback(feedback);
    let Some(mix_id) = js::or(mix, "id") else {
        return fb;
    };
    let seed_artists: HashSet<String> = list(get(mix, "artists"))
        .iter()
        .map(|a| name_key(Some(a)))
        .collect();
    let tracks: Vec<&Value> = list(get(mix, "tracks"))
        .iter()
        .filter(|t| js::truthy(t) && js::or(t, "title").is_some())
        .collect();
    let entries: Vec<Value> = tracks
        .iter()
        .map(|t| {
            let key = if js::or(t, "artist").is_some() && js::or(t, "title").is_some() { json!(default_track_key(t)) } else { js::or_default(t, "id", json!("")) };
            json!({ "id": js::or_default(t, "id", json!("")), "title": js::or_default(t, "title", json!("")), "artist": js::or_default(t, "artist", json!("")), "key": key })
        })
        .collect();
    let same_track = |a: &Value, b: &Value| {
        js::or(a, "id").is_some_and(|id| get(b, "id").is_some_and(|x| js_same(id, x)))
            || js::or(a, "key").is_some_and(|k| get(b, "key").is_some_and(|x| js_same(k, x)))
    };
    let mut names: Vec<(String, String)> = Vec::new();
    let mix_artists: &[Value] = if is_like {
        list(get(mix, "artists"))
    } else {
        &[]
    };
    for name in mix_artists.iter().chain(
        tracks
            .iter()
            .map(|t| get(t, "artist").unwrap_or(&Value::Null)),
    ) {
        let k = name_key(Some(name));
        if k.is_empty()
            || (!is_like && seed_artists.contains(&k))
            || names.iter().any(|(n, _)| *n == k)
        {
            continue;
        }
        names.push((k, js::trim(&js::to_string(name)).to_string()));
    }
    let (keep_t, drop_t, keep_a, drop_a) = if is_like {
        (
            "likedTracks",
            "dislikedTracks",
            "likedArtists",
            "dislikedArtists",
        )
    } else {
        (
            "dislikedTracks",
            "likedTracks",
            "dislikedArtists",
            "likedArtists",
        )
    };
    let arr = |fb: &Value, k: &str| fb[k].as_array().cloned().unwrap_or_default();
    let dropped: Vec<Value> = arr(&fb, drop_t)
        .into_iter()
        .filter(|t| !entries.iter().any(|e| same_track(t, e)))
        .collect();
    fb[drop_t] = Value::Array(dropped);
    let kept = arr(&fb, keep_t);
    let mut next: Vec<Value> = entries
        .iter()
        .filter(|e| !kept.iter().any(|t| same_track(t, e)))
        .cloned()
        .collect();
    next.extend(kept);
    next.truncate(CAP_TRACKS);
    fb[keep_t] = Value::Array(next);
    let dropped: Vec<Value> = arr(&fb, drop_a)
        .into_iter()
        .filter(|a| !names.iter().any(|(k, _)| *k == name_key(Some(a))))
        .collect();
    fb[drop_a] = Value::Array(dropped);
    let kept = arr(&fb, keep_a);
    let mut next: Vec<Value> = names
        .iter()
        .filter(|(_, a)| !kept.iter().any(|x| name_key(Some(x)) == name_key_str(a)))
        .map(|(_, a)| json!(a))
        .collect();
    next.extend(kept);
    next.truncate(CAP_ARTISTS);
    fb[keep_a] = Value::Array(next);
    let mut ratings: Map<String, Value> =
        fb["ratingsByMix"].as_object().cloned().unwrap_or_default();
    ratings.insert(
        js::to_string(mix_id),
        json!(if is_like { "like" } else { "dislike" }),
    );
    let skip = ratings.len().saturating_sub(CAP_RATINGS);
    fb["ratingsByMix"] = Value::Object(ratings.into_iter().skip(skip).collect());
    fb
}

/// What the disliked mixes add to My Wave's avoid list: (keys, artists).
pub fn feedback_avoid(feedback: &Value) -> (Vec<String>, Vec<Value>) {
    let fb = clean_feedback(feedback);
    let mut keys: Vec<String> = Vec::new();
    for t in list(fb.get("dislikedTracks")) {
        for k in ["key", "id"] {
            if let Some(v) = js::or(t, k) {
                let s = js::to_string(v);
                if !keys.contains(&s) {
                    keys.push(s);
                }
            }
        }
    }
    (keys, list(fb.get("dislikedArtists")).to_vec())
}

// ---------- the explicit [E] badge ----------

pub fn is_explicit_text(s: Option<&Value>) -> bool {
    s.filter(|x| js::truthy(x))
        .is_some_and(|x| EXPLICIT.is_match(&js::to_string(x)))
}

pub fn is_track_explicit(t: &Value) -> bool {
    if !js::truthy(t) {
        return false;
    }
    let is_true = |k: &str| get(t, k) == Some(&Value::Bool(true));
    if is_true("isExplicit")
        || is_true("explicit")
        || is_true("explicit_lyrics")
        || get(t, "explicit_content_lyrics").and_then(Value::as_f64) == Some(1.0)
    {
        return true;
    }
    is_explicit_text(get(t, "title")) || is_explicit_text(get(t, "album"))
}

// ---------- blocked artists and songs ----------

fn artists_of(s: &str) -> Vec<String> {
    ARTIST_SPLIT
        .split(s)
        .map(name_key_str)
        .filter(|k| !k.is_empty())
        .collect()
}

/// "Queen" also blocks "Queen & David Bowie".
pub fn is_artist_blocked(blocked: &[Value], artist: Option<&Value>) -> bool {
    let keys: HashSet<String> = blocked
        .iter()
        .map(|b| name_key(Some(b)))
        .filter(|k| !k.is_empty())
        .collect();
    let Some(artist) = artist.filter(|a| js::truthy(a)) else {
        return false;
    };
    if keys.is_empty() {
        return false;
    }
    keys.contains(&name_key(Some(artist)))
        || artists_of(&js::to_string(artist))
            .iter()
            .any(|k| keys.contains(k))
}

fn blocked_song_matches(entry: &Value, track: &Value) -> bool {
    if !js::truthy(entry) || !js::truthy(track) {
        return false;
    }
    if same_id(entry, track) {
        return true;
    }
    let t = fold(get(entry, "title"));
    if t.is_empty() || t != fold(get(track, "title")) {
        return false;
    }
    let (a, b) = (
        name_key(get(entry, "artist")),
        name_key(get(track, "artist")),
    );
    a.is_empty() || b.is_empty() || a == b
}

pub fn is_song_blocked(blocked: &[Value], track: &Value) -> bool {
    blocked.iter().any(|e| blocked_song_matches(e, track))
}

pub fn is_track_blocked(blocks: &Value, track: &Value) -> bool {
    js::truthy(track)
        && js::truthy(blocks)
        && (is_song_blocked(list(get(blocks, "songs")), track)
            || is_artist_blocked(list(get(blocks, "artists")), get(track, "artist")))
}

/// Returns (list, blocked).
pub fn toggle_blocked_artist(blocked: &[Value], artist: Option<&Value>) -> (Vec<Value>, bool) {
    let full = js::str_nullish(artist);
    let name = js::slice16(js::trim(&full), 200).to_string();
    let k = name_key_str(&name);
    if k.is_empty() {
        return (blocked.to_vec(), false);
    }
    if blocked.iter().any(|a| name_key(Some(a)) == k) {
        return (
            blocked
                .iter()
                .filter(|a| name_key(Some(a)) != k)
                .cloned()
                .collect(),
            false,
        );
    }
    let mut out = blocked.to_vec();
    out.push(json!(name));
    (out, true)
}

/// Returns (list, blocked).
pub fn toggle_blocked_song(blocked: &[Value], track: &Value) -> (Vec<Value>, bool) {
    if !js::truthy(track) || fold(get(track, "title")).is_empty() {
        return (blocked.to_vec(), false);
    }
    if blocked.iter().any(|e| blocked_song_matches(e, track)) {
        return (
            blocked
                .iter()
                .filter(|e| !blocked_song_matches(e, track))
                .cloned()
                .collect(),
            false,
        );
    }
    let title = js::str_nullish(get(track, "title"));
    let artist = js::str_or_empty(get(track, "artist"));
    let entry = json!({
        "id": js::or(track, "id").map(js::to_string).unwrap_or_default(),
        "title": js::slice16(&title, 300),
        "artist": js::slice16(&artist, 300),
    });
    let mut out = blocked.to_vec();
    out.push(entry);
    (out, true)
}
