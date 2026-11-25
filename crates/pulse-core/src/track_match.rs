//! track-match.js: picks the search result that is the same recording as an imported song.
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::js::{self, re, re_flags};

/// Words that make an upload a different version of the song, unless the wanted title has them too.
pub const MATCH_VERSION_WORDS: &[&str] = &[
    "remix",
    "sped up",
    "speed up",
    "slowed",
    "nightcore",
    "reverb",
    "8d",
    "bass boosted",
    "bass boost",
    "cover",
    "flip",
    "mashup",
    "instrumental",
    "karaoke",
    "loop",
    "bootleg",
    "live",
    "acoustic",
    "tribute",
    "piano version",
    "reaction",
    "tutorial",
    "lesson",
    "chords",
    "drum cover",
    "guitar cover",
    "lyrics translation",
    "1 hour",
    "10 hours",
];

static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\b(official|video|music|audio|lyrics?|lyric video|visuali[sz]er|hd|hq|4k|mv|m\/v|remaster(ed)?(\s+\d{4})?|\d{4}\s+remaster(ed)?|explicit|clean|color coded|full song|from [^)]*)\b",
    )
});
static NON_ALNUM_SP: LazyLock<Regex> = LazyLock::new(|| re(r"[^\p{L}\p{N} ]+"));
static TOPIC: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s+-\s+topic\s*$", "i"));
static SPLIT: LazyLock<Regex> = LazyLock::new(|| {
    re_flags(
        r"\s*(?:,|;|\/|&|\bfeat\.?|\bft\.?|\bfeaturing\b|\bwith\b|\bx\b|\bvs\.?)\s*",
        "i",
    )
});
static VEVO: LazyLock<Regex> = LazyLock::new(|| re_flags("vevo$", "i"));
static OFFICIAL: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\bofficial\b", "i"));
static FEAT_BRACKET: LazyLock<Regex> =
    LazyLock::new(|| re_flags(r"[([]\s*(feat|ft|with)\.?\s[^)\]]*[)\]]", "i"));
static FEAT_TAIL: LazyLock<Regex> = LazyLock::new(|| re_flags(r"\s(feat|ft)\.?\s.*$", "i"));
static BRACKET: LazyLock<Regex> = LazyLock::new(|| re(r"[([]([^)\]]*)[)\]]"));
static DASH: LazyLock<Regex> = LazyLock::new(|| re(r"\s[-–—|]\s"));
static REMASTER_TAIL: LazyLock<Regex> =
    LazyLock::new(|| re_flags(r"\s+-\s+(\d{4}\s+)?remaster(ed)?(\s+\d{4})?.*$", "i"));
static MAIN_SPLIT: LazyLock<Regex> =
    LazyLock::new(|| re_flags(r"\s*(?:,|&|\bfeat\.?|\bft\.?)\s*", "i"));

fn cyr(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'д' => "d",
        'е' | 'ё' | 'э' => "e",
        'ж' => "zh",
        'з' => "z",
        'и' | 'і' => "i",
        'й' | 'ы' => "y",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "h",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "sch",
        'ъ' | 'ь' => "",
        'ю' => "yu",
        'я' => "ya",
        'ї' => "yi",
        'є' => "ye",
        'ґ' => "g",
        _ => return None,
    })
}

/// Comparable form: Cyrillic transliterated, no accents, lowercase, `&` as "and", letters/digits/spaces.
pub fn match_norm(s: &str) -> String {
    let mut t = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        match cyr(c) {
            Some(r) => t.push_str(r),
            None => t.push(c),
        }
    }
    let t = js::strip_marks(&js::nfkd(&t))
        .to_lowercase()
        .replace('&', " and ");
    js::trim(&js::collapse_ws(&NON_ALNUM_SP.replace_all(&t, " "))).to_string()
}

/// "Artist A, Artist B & C feat. D" -> ["artist a", "artist b", "c", "d"].
pub fn split_artists(artist: &str) -> Vec<String> {
    let a = TOPIC.replace(artist, "");
    SPLIT
        .split(&a)
        .map(|x| match_norm(&OFFICIAL.replace(&VEVO.replace(x, ""), "")))
        .filter(|x| !x.is_empty())
        .collect()
}

fn strip_feat(t: &str) -> String {
    FEAT_TAIL
        .replace(&FEAT_BRACKET.replace_all(t, " "), " ")
        .into_owned()
}

fn denoise(s: &str) -> String {
    js::trim(&js::collapse_ws(&NOISE.replace_all(s, " "))).to_string()
}

/// The song title without the artist prefix, featured artists and "(Official Video)" style noise.
pub fn clean_title(title: &str, artists: &[String]) -> String {
    let t = strip_feat(title);
    let t = BRACKET.replace_all(&t, |c: &Captures| {
        let inner = &c[1];
        if js::trim(&NOISE.replace_all(&match_norm(inner), "")).is_empty() {
            " ".to_string()
        } else {
            format!(" {inner} ")
        }
    });
    let mut n = denoise(&match_norm(&t));
    let stripped = strip_feat(title);
    let dash: Vec<&str> = DASH.split(&stripped).collect();
    if dash.len() > 1 {
        let head = match_norm(dash[0]);
        if artists.iter().any(|a| {
            !a.is_empty() && (head == *a || head.contains(a.as_str()) || a.contains(head.as_str()))
        }) {
            n = denoise(&match_norm(&dash[1..].join(" ")));
        }
    }
    n
}

fn tokens(s: &str) -> Vec<String> {
    js::uniq_ordered(
        match_norm(s)
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(str::to_string),
    )
}

pub fn title_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (tokens(a), tokens(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a.concat() == b.concat() {
        return 1.0;
    }
    let hit = a.iter().filter(|t| b.contains(t)).count() as f64;
    let cover = hit / a.len() as f64;
    let jac = hit / a.len().max(b.len()) as f64;
    jac.max(cover * 0.85)
}

/// `new RegExp('(^| )' + word.replace(/ /g, '[ -]?') + '( |$)').test(text)`.
fn has_word(text: &str, word: &str) -> bool {
    let body = word
        .split(' ')
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join("[ -]?");
    Regex::new(&format!("(^| ){body}( |$)")).is_ok_and(|r| r.is_match(text))
}

fn version_words(text: &str) -> Vec<&'static str> {
    let n = match_norm(text);
    MATCH_VERSION_WORDS
        .iter()
        .copied()
        .filter(|w| has_word(&n, w))
        .collect()
}

/// A search result or an imported song.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub uploader: String,
    /// Seconds; 0 when unknown.
    #[serde(default)]
    pub duration: f64,
}

fn compact(s: &str) -> String {
    s.strip_prefix("the ").unwrap_or(s).replace(' ', "")
}

/// How sure we are that the result is by one of the wanted artists: 0..1.
pub fn artist_score(wanted: &[String], r: &Candidate) -> f64 {
    if wanted.is_empty() {
        return 0.5;
    }
    let channel = split_artists(if r.artist.is_empty() {
        &r.uploader
    } else {
        &r.artist
    });
    let mut best: f64 = 0.0;
    for w in wanted {
        let cw = compact(w);
        for c in &channel {
            let cc = compact(c);
            if cc == cw {
                return 1.0;
            }
            if js::len16(&cw) >= 4 && (cc.starts_with(&cw) || cc.ends_with(&cw)) {
                best = best.max(0.9);
            }
        }
        let title_n = match_norm(&strip_feat(&r.title));
        let head: Vec<&str> = DASH.split(&r.title).collect();
        if head.len() > 1 && split_artists(head[0]).iter().any(|a| compact(a) == cw) {
            best = best.max(0.8);
        } else if js::len16(w) >= 3 && has_word(&title_n, w) {
            best = best.max(0.6);
        }
        if js::len16(w) >= 3 && has_word(&match_norm(&r.title), w) {
            best = best.max(0.6);
        }
    }
    best
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub score: f64,
    pub accept: bool,
    pub t_sim: f64,
    pub a_sim: f64,
    pub d_diff: Option<f64>,
}

pub fn score_candidate(sp: &Candidate, r: &Candidate) -> Score {
    let wanted = split_artists(&sp.artist);
    let want_title = clean_title(&sp.title, &wanted);
    let mut all = wanted.clone();
    all.extend(split_artists(&r.artist));
    let got_title = clean_title(&r.title, &all);
    let t_sim = title_similarity(&want_title, &got_title);
    let a_sim = artist_score(&wanted, r);
    let nz = |x: f64| if x.is_nan() { 0.0 } else { x };
    let (want, got) = (nz(sp.duration), nz(r.duration));
    let d_diff = (want != 0.0 && got != 0.0).then(|| (want - got).abs());
    let mut score = t_sim + a_sim * 1.2;
    if let Some(d) = d_diff {
        if d <= 3.0 {
            score += 0.4;
        } else if d <= 10.0 {
            score += 0.2;
        } else if d > 30.0 {
            score -= 0.5;
        }
    }
    let want_versions = version_words(&sp.title);
    let extra: Vec<_> = version_words(&r.title)
        .into_iter()
        .filter(|w| !want_versions.contains(w) && !want_title.contains(w))
        .collect();
    if !extra.is_empty() {
        score -= 1.0;
    }
    let r_norm = match_norm(&r.title);
    if want_versions.iter().any(|w| !has_word(&r_norm, w)) {
        score -= 0.5;
    }
    let duration_close = d_diff.is_some_and(|d| d <= 5.0);
    let accept = extra.is_empty()
        && t_sim >= 0.5
        && (a_sim >= 0.6
            || (wanted.is_empty() && t_sim >= 0.8)
            || (a_sim > 0.0 && duration_close && t_sim >= 0.9));
    Score {
        score,
        accept,
        t_sim,
        a_sim,
        d_diff,
    }
}

pub fn pick_best_match<'a>(sp: &Candidate, results: &'a [Candidate]) -> Option<&'a Candidate> {
    let mut best = None;
    let mut best_score = f64::NEG_INFINITY;
    for r in results.iter().filter(|r| !r.title.is_empty()) {
        let s = score_candidate(sp, r);
        if s.accept && s.score > best_score {
            best = Some(r);
            best_score = s.score;
        }
    }
    best
}

/// Search queries for an imported song, best first.
pub fn match_queries(sp: &Candidate) -> Vec<String> {
    let artists = split_artists(&sp.artist);
    let title = js::trim(&REMASTER_TAIL.replace(&strip_feat(&sp.title), "")).to_string();
    let mut out: Vec<String> = Vec::new();
    let mut add = |q: String| {
        let q = js::trim(&js::collapse_ws(&q)).to_string();
        if !q.is_empty() && !out.contains(&q) {
            out.push(q);
        }
    };
    let main = MAIN_SPLIT
        .split(&sp.artist)
        .next()
        .unwrap_or("")
        .to_string();
    add(format!("{main} {title}"));
    if artists.len() > 1 {
        add(format!("{title} {}", sp.artist));
    }
    out
}
