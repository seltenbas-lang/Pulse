//! lyrics-match.js: turns a video title and channel into what lyrics sites list, and picks the
//! lrclib result that matches.
use std::collections::HashSet;
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::Value;

use crate::js::{self, re, re_flags};

static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\b(official|video|audio|lyrics?|visuali[sz]er|hd|hq|4k|8k|remaster(ed)?|clip|mv|m\/v|explicit|clean|color\s*coded|live\s*performance|performance\s*video|full\s*album|tiny\s*desk)\b",
        "i",
    )
});
static FEAT: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\b(feat\.?|ft\.?|featuring|with|prod\.?|produced\s+by)\s",
        "i",
    )
});
static BRACKETS: LazyLock<Regex> = LazyLock::new(|| re(r"\s*[([{【「『](.*?)[)\]}】」』]"));
static DASH: LazyLock<Regex> = LazyLock::new(|| re(r"\s+[-–—|]\s+"));
static QUOTES: LazyLock<Regex> = LazyLock::new(|| re(r"['’`]"));
static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| re(r"[^\p{L}\p{N}]+"));
static TITLE_FEAT: LazyLock<Regex> =
    LazyLock::new(|| re_flags(r"\s+(feat\.?|ft\.?|featuring)\s.*$", "i"));
static PIPE_TAIL: LazyLock<Regex> = LazyLock::new(|| re(r"\s*[|｜].*$"));
static EDGES: LazyLock<Regex> = LazyLock::new(|| re(r"^[\s\-–—:]+|[\s\-–—:]+$"));
static TOPIC: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s*-\s*topic$", "i"));
static VEVO: LazyLock<Regex> = LazyLock::new(|| re_flags(r"(\S)\s*vevo$", "i"));
static OFFICIAL: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s+(official|music)$", "i"));
static SPLIT: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\s*(?:,|&|\+|;|\/|×|\bx\b|\band\b|\bfeat\.?|\bft\.?|\bfeaturing\b|\bwith\b)\s*",
        "i",
    )
});
static LABEL: LazyLock<Regex> = LazyLock::new(|| re_flags("topic|vevo|records|music", "i"));
static CAMEL: LazyLock<Regex> = LazyLock::new(|| re("([a-z])([A-Z])"));
static ANY_WS: LazyLock<Regex> = LazyLock::new(|| re(r"\s"));

/// Comparable form: NFKD, no accents, lowercase, no apostrophes, words separated by one space.
pub fn norm(s: &str) -> String {
    let t = js::strip_marks(&js::nfkd(s)).to_lowercase();
    let t = QUOTES.replace_all(&t, "");
    js::trim(&NON_ALNUM.replace_all(&t, " ")).to_string()
}

/// Drops "(Official Video)", "[Lyrics]", "(feat. X)" but keeps "(Remix)", "(Pt. 2)"; `all` drops every bracket.
pub fn clean_title(s: &str, all: bool) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if matches!(c, '’' | '‘' | '`') {
                '\''
            } else {
                c
            }
        })
        .collect();
    let t = BRACKETS.replace_all(&t, |c: &Captures| {
        let inner = c.get(1).map_or("", |m| m.as_str());
        if all || NOISE.is_match(inner) || FEAT.is_match(&format!("{inner} ")) {
            String::new()
        } else {
            c[0].to_string()
        }
    });
    let t = TITLE_FEAT.replace(&t, "");
    let t = PIPE_TAIL.replace(&t, "");
    let t = js::collapse_ws(&t);
    js::trim(&EDGES.replace_all(&t, "")).to_string()
}

pub fn clean_artist(s: &str) -> String {
    let t = BRACKETS.replace_all(s, "");
    let t = TOPIC.replace(&t, "");
    let t = VEVO.replace(&t, "$1");
    let t = OFFICIAL.replace(&t, "");
    js::trim(&js::collapse_ws(&t)).to_string()
}

pub fn split_artists(s: &str) -> Vec<String> {
    SPLIT
        .split(s)
        .map(|a| js::trim(a).to_string())
        .filter(|a| !a.is_empty())
        .collect()
}

fn uniq(list: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    list.into_iter()
        .filter(|v| {
            let k = norm(v);
            !k.is_empty() && seen.insert(k)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LyricsQuery {
    pub titles: Vec<String>,
    pub artists: Vec<String>,
}

fn spaced(n: &str) -> String {
    if !n.is_empty() && !ANY_WS.is_match(n) {
        CAMEL.replace_all(n, "$1 $2").into_owned()
    } else {
        String::new()
    }
}

pub fn parse_for_lyrics(title: &str, artist: &str) -> LyricsQuery {
    let channel = clean_artist(artist);
    let mut song = title.to_string();
    let mut from_title = String::new();
    let parts: Vec<&str> = DASH.split(title).collect();
    if parts.len() >= 2 {
        let (left, right) = (parts[0], parts[1..].join(" - "));
        let (ch, l) = (norm(&channel), norm(left));
        let first = |s: &str| {
            split_artists(s)
                .into_iter()
                .next()
                .unwrap_or_else(|| s.to_string())
        };
        let left_is_artist = ch.is_empty()
            || (!l.is_empty()
                && (l.contains(&norm(&first(&channel))) || ch.contains(&norm(&first(left)))))
            || LABEL.is_match(artist);
        if left_is_artist {
            from_title = clean_artist(left);
            song = right;
        } else if NOISE.is_match(&right) {
            song = left.to_string();
        }
    }
    let titles = uniq(vec![clean_title(&song, false), clean_title(&song, true)]);
    let lead = if from_title.is_empty() {
        channel.clone()
    } else {
        from_title
    };
    let first = |s: &str| split_artists(s).into_iter().next();
    let artists = [
        Some(lead.clone()),
        first(&lead),
        Some(channel.clone()),
        first(&channel),
        Some(spaced(&lead)),
        Some(spaced(&channel)),
    ]
    .into_iter()
    .flatten()
    .filter(|a| !a.is_empty())
    .collect();
    LyricsQuery {
        titles,
        artists: uniq(artists),
    }
}

fn tokens(s: &str) -> HashSet<String> {
    norm(s)
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn overlap(a: &str, b: &str) -> f64 {
    let (a, b) = (tokens(a), tokens(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.iter().filter(|t| b.contains(*t)).count() as f64 / a.len().min(b.len()) as f64
}

/// The best lrclib search result for this song, or `None` when nothing really matches.
pub fn pick_best<'a>(list: &'a [Value], want: &LyricsQuery, duration: f64) -> Option<&'a Value> {
    let mut best = None;
    let mut best_score = 0.0;
    for item in list {
        if !js::truthy(item)
            || (js::or(item, "syncedLyrics").is_none() && js::or(item, "plainLyrics").is_none())
            || js::or(item, "instrumental").is_some()
        {
            continue;
        }
        let name = js::str_or_empty(js::or(item, "trackName").or_else(|| item.get("name")));
        let artist = js::str_or_empty(item.get("artistName"));
        let t = want
            .titles
            .iter()
            .map(|w| overlap(w, &name))
            .fold(f64::NEG_INFINITY, f64::max);
        let a = want
            .artists
            .iter()
            .map(|w| overlap(w, &artist))
            .fold(0.0, f64::max);
        let item_d = item
            .get("duration")
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN);
        let d = (duration > 0.0 && item_d > 0.0).then(|| (item_d - duration).abs());
        if t < 0.6 || (a < 0.5 && !d.is_some_and(|d| d <= 3.0)) {
            continue;
        }
        let mut score = t * 3.0
            + a * 2.0
            + if js::or(item, "syncedLyrics").is_some() {
                1.0
            } else {
                0.0
            };
        if let Some(d) = d {
            score += if d <= 2.0 {
                1.5
            } else if d <= 8.0 {
                0.5
            } else {
                -1.0
            };
        }
        if score > best_score {
            best = Some(item);
            best_score = score;
        }
    }
    best
}
