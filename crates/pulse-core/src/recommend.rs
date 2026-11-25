//! recommend-core.js: YouTube Music radio and Last.fm similar songs/artists, minus blocked and
//! just-played songs.
use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

use crate::js::{self, list, re, re_flags};
use crate::library_pages;

static VIDEO_ID: LazyLock<Regex> = LazyLock::new(|| re(r"^[\w-]{11}$"));
static CHANNEL: LazyLock<Regex> = LazyLock::new(|| re(r"^UC[\w-]{22}$"));
static LENGTH: LazyLock<Regex> = LazyLock::new(|| re(r"^\d+(:\d{1,2}){1,2}$"));
static TITLE_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\s*[([][^)\]]*\b(?:official|video|audio|lyrics?|visuali[sz]er|hd|hq|4k|remaster(?:ed)?|upgrade|mv|m\/v)\b[^)\]]*[)\]]",
        "i",
    )
});
static NOT_MUSIC: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\b(?:documentary|interview|trailer|reaction|behind the scenes|making of|podcast|full album|live stream)\b",
        "i",
    )
});
static TOPIC: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s*-\s*Topic$", "i"));
static VEVO: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s*(?:VEVO|Official)$", "i"));
static ARTIST_DASH: LazyLock<Regex> = LazyLock::new(|| re(r"^(.+?)\s+[-–—]\s+(.+)$"));
static TRAILING_DASH: LazyLock<Regex> = LazyLock::new(|| re(r"\s+[-–—]\s+$"));

fn runs_text(r: Option<&Value>) -> String {
    list(r.and_then(|r| r.get("runs")))
        .iter()
        .map(|x| js::str_or_empty(x.get("text")))
        .collect()
}

/// "3:45" or "1:02:03" in seconds; 0 for anything else.
pub fn parse_length(text: &str) -> f64 {
    let s = js::trim(text);
    if !LENGTH.is_match(s) {
        return 0.0;
    }
    s.split(':').fold(0.0, |acc, n| {
        acc * 60.0 + n.parse::<f64>().unwrap_or(f64::NAN)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NamedVideo {
    pub title: String,
    pub artist: String,
    pub renamed: bool,
}

/// The song's own name and the artist a video title names ("Radiohead - No Surprises (Official Video)").
pub fn clean_video_title(title: &str, artist: &str) -> NamedVideo {
    let mut t = js::trim(&js::collapse_ws(&TITLE_NOISE.replace_all(title, ""))).to_string();
    let given = js::trim(&TOPIC.replace(artist, "")).to_string();
    let stripped = js::trim(&VEVO.replace(&given, "")).to_string();
    let mut a = if stripped.is_empty() {
        given.clone()
    } else {
        stripped
    };
    let fold = library_pages::fold_str;
    for i in 0..2 {
        let Some(m) = ARTIST_DASH.captures(&t) else {
            break;
        };
        let (left, right) = (js::trim(&m[1]).to_string(), js::trim(&m[2]).to_string());
        let same = |x: &str| {
            let (fx, fa) = (fold(x), fold(&a));
            !fx.is_empty() && !fa.is_empty() && (fx == fa || fa.contains(&fx) || fx.contains(&fa))
        };
        if same(&left) {
            t = right;
        } else if same(&right) {
            t = left;
            break;
        } else if i == 0 && !fold(&left).is_empty() {
            a = left;
            t = right;
        } else {
            break;
        }
    }
    let cleaned = js::trim(&TRAILING_DASH.replace(&t, "")).to_string();
    let title = if cleaned.is_empty() {
        js::trim(title).to_string()
    } else {
        cleaned
    };
    let renamed = a != given;
    NamedVideo {
        title,
        artist: a,
        renamed,
    }
}

fn path<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .try_fold(v, |v, k| v.get(*k).filter(|x| js::truthy(x)))
}

/// YouTube Music's radio for a song (`next` with playlistId RDAMVM<videoId>): the songs it would
/// play after the seed, of a normal length.
pub fn parse_watch_playlist(json: &Value, seed_id: &str) -> Vec<Value> {
    let tabs = path(
        json,
        &[
            "contents",
            "singleColumnMusicWatchNextResultsRenderer",
            "tabbedRenderer",
            "watchNextTabbedResultsRenderer",
            "tabs",
        ],
    );
    let first = list(tabs).first().unwrap_or(&Value::Null);
    let panel = path(
        first,
        &[
            "tabRenderer",
            "content",
            "musicQueueRenderer",
            "content",
            "playlistPanelRenderer",
        ],
    );
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for it in list(panel.and_then(|p| p.get("contents"))) {
        let r = path(it, &["playlistPanelVideoRenderer"]).or_else(|| {
            path(
                it,
                &[
                    "playlistPanelVideoWrapperRenderer",
                    "primaryRenderer",
                    "playlistPanelVideoRenderer",
                ],
            )
        });
        let Some(r) = r else { continue };
        let vid = js::str_or_empty(r.get("videoId"));
        if !VIDEO_ID.is_match(&vid) || vid == seed_id || seen.contains(&vid) {
            continue;
        }
        let title = js::trim(&runs_text(r.get("title"))).to_string();
        let runs = list(r.get("longBylineText").and_then(|b| b.get("runs")));
        let cut = runs
            .iter()
            .position(|x| x.get("text").and_then(Value::as_str) == Some(" • "));
        let artist_runs = cut.map_or(runs, |c| &runs[..c]);
        let joined: String = artist_runs
            .iter()
            .map(|x| js::str_nullish(x.get("text")))
            .collect();
        let byline = if joined.is_empty() {
            runs_text(r.get("shortBylineText"))
        } else {
            joined
        };
        let artist = js::trim(&TOPIC.replace(&byline, "")).to_string();
        let channel = artist_runs
            .iter()
            .filter_map(|x| {
                path(x, &["navigationEndpoint", "browseEndpoint", "browseId"])
                    .and_then(Value::as_str)
            })
            .find(|id| CHANNEL.is_match(id));
        let duration = parse_length(&runs_text(r.get("lengthText")));
        if title.is_empty()
            || artist.is_empty()
            || (duration != 0.0 && !(30.0..=600.0).contains(&duration))
            || NOT_MUSIC.is_match(&title)
        {
            continue;
        }
        let named = clean_video_title(&title, &artist);
        let thumbs = list(r.get("thumbnail").and_then(|t| t.get("thumbnails")));
        let thumb = thumbs
            .last()
            .and_then(|t| js::or(t, "url"))
            .map(js::to_string)
            .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{vid}/hqdefault.jpg"));
        seen.insert(vid.clone());
        let mut item = json!({ "id": vid, "title": named.title, "artist": named.artist });
        if let Some(ch) = channel.filter(|_| !named.renamed) {
            item["artistId"] = json!(ch);
        }
        item["duration"] = json!(duration);
        item["thumbnail"] = json!(thumb);
        item["url"] = json!(format!("https://www.youtube.com/watch?v={vid}"));
        item["platform"] = json!("youtube");
        out.push(item);
    }
    out
}

fn num(v: Option<&Value>) -> f64 {
    let n = js::to_number(v);
    if n.is_finite() { n } else { 0.0 }
}

/// Last.fm sends one item as an object instead of a list.
fn one(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(o @ Value::Object(_)) => vec![o],
        _ => Vec::new(),
    }
}

/// track.getSimilar: [{ title, artist, match (0..1), duration }].
pub fn parse_similar_tracks(json: &Value) -> Vec<Value> {
    one(json.get("similartracks").and_then(|s| s.get("track")))
        .into_iter()
        .map(|t| {
            let artist = t
                .get("artist")
                .map(|a| js::str_or_empty(js::or(a, "name").or_else(|| a.get("#text"))))
                .unwrap_or_default();
            json!({
                "title": js::trim(&js::str_or_empty(t.get("name"))),
                "artist": js::trim(&artist),
                "match": num(t.get("match")).clamp(0.0, 1.0),
                "duration": num(t.get("duration")),
            })
        })
        .filter(|t| js::truthy(&t["title"]) && js::truthy(&t["artist"]))
        .collect()
}

/// artist.getSimilar: [{ name, match (0..1) }].
pub fn parse_similar_artists(json: &Value) -> Vec<Value> {
    one(json.get("similarartists").and_then(|s| s.get("artist")))
        .into_iter()
        .map(|a| json!({ "name": js::trim(&js::str_or_empty(a.get("name"))), "match": num(a.get("match")).clamp(0.0, 1.0) }))
        .filter(|a| js::truthy(&a["name"]))
        .collect()
}

/// artist.getTopTracks: [{ title, artist }].
pub fn parse_top_tracks(json: &Value, fallback_artist: &str) -> Vec<Value> {
    one(json.get("toptracks").and_then(|s| s.get("track")))
        .into_iter()
        .map(|t| {
            let a = t
                .get("artist")
                .map(|a| js::str_or_empty(a.get("name")))
                .unwrap_or_default();
            let a = if a.is_empty() {
                fallback_artist.to_string()
            } else {
                a
            };
            json!({ "title": js::trim(&js::str_or_empty(t.get("name"))), "artist": js::trim(&a) })
        })
        .filter(|t| js::truthy(&t["title"]) && js::truthy(&t["artist"]))
        .collect()
}

/// `<artist name key>::<title name key>`.
pub fn default_key(t: &Value) -> String {
    format!(
        "{}::{}",
        library_pages::name_key(t.get("artist")),
        library_pages::name_key(t.get("title"))
    )
}

#[derive(Default)]
pub struct Unwanted<'a> {
    pub blocks: Option<&'a Value>,
    pub recent_keys: HashSet<String>,
    pub recent_ids: HashSet<String>,
    pub key_of: Option<&'a dyn Fn(&Value) -> String>,
}

/// Keeps out blocked artists and songs, recently played keys or ids, and repeats within the list.
pub fn drop_unwanted(tracks: &[Value], o: &Unwanted) -> Vec<Value> {
    let blocks = o.blocks.unwrap_or(&Value::Null);
    let b = json!({ "artists": list(blocks.get("artists")), "songs": list(blocks.get("songs")) });
    let has_blocks = !list(b.get("artists")).is_empty() || !list(b.get("songs")).is_empty();
    let (mut seen_keys, mut seen_ids) = (HashSet::new(), HashSet::new());
    let mut out = Vec::new();
    for t in tracks {
        if !js::truthy(t) || js::or(t, "title").is_none() {
            continue;
        }
        let k = o.key_of.map_or_else(|| default_key(t), |f| f(t));
        let id = js::or(t, "id").map(js::to_string).unwrap_or_default();
        if (!k.is_empty() && (o.recent_keys.contains(&k) || seen_keys.contains(&k)))
            || (!id.is_empty() && (o.recent_ids.contains(&id) || seen_ids.contains(&id)))
        {
            continue;
        }
        if has_blocks && library_pages::is_track_blocked(&b, t) {
            continue;
        }
        if !k.is_empty() {
            seen_keys.insert(k);
        }
        if !id.is_empty() {
            seen_ids.insert(id);
        }
        out.push(t.clone());
    }
    out
}
