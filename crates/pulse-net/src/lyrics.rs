//! Lyrics: lrclib lookups (server.js fetchLyricsDirect) and LRC parsing (app.js loadLyrics,
//! lyrics-sources.js parseEnhancedLrcWords).
use std::sync::LazyLock;

use pulse_core::lyrics_match::{self, LyricsQuery};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::http::Net;

const LRCLIB_UA: &str = "Pulse";
static LRC_LINE: LazyLock<Regex> =
    LazyLock::new(|| pulse_core::js::re(r"\[(\d+):(\d+(\.\d+)?)\](.*)"));
static WORD_TAG: LazyLock<Regex> = LazyLock::new(|| pulse_core::js::re(r"<(\d+):(\d+(?:\.\d+)?)>"));
static NO_PUNCT: LazyLock<Regex> = LazyLock::new(|| Regex::new("[,&]").expect("literal"));

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    /// Seconds; -1 for an unsynced line.
    pub time: f64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<Word>>,
}

/// "<00:12.34>word <00:12.80>word" inside an LRC line -> words, or `None` without word tags.
pub fn parse_enhanced_words(line: &str, line_end: f64) -> Option<Vec<Word>> {
    let tags: Vec<(f64, usize, usize)> = WORD_TAG
        .captures_iter(line)
        .map(|c| {
            let m = c.get(0).map_or(0..0, |m| m.range());
            let t = pulse_core::js::parse_int(&c[1]) * 60.0 + c[2].parse::<f64>().unwrap_or(0.0);
            (t, m.start, m.end)
        })
        .collect();
    let mut words = Vec::new();
    for (i, &(time, _, to)) in tags.iter().enumerate() {
        let next = tags.get(i + 1);
        let text = pulse_core::js::trim(&line[to..next.map_or(line.len(), |n| n.1)]);
        if text.is_empty() {
            continue;
        }
        let end = next.map_or(
            if line_end > time {
                line_end
            } else {
                time + 0.5
            },
            |n| n.0,
        );
        for part in text.split(pulse_core::js::is_ws).filter(|p| !p.is_empty()) {
            words.push(Word {
                text: part.to_string(),
                start: time,
                end,
            });
        }
    }
    (!words.is_empty()).then_some(words)
}

/// LRC (or plain) lyrics text -> lines; synced lines carry their time, plain ones -1.
pub fn parse_lrc(raw: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for line in raw.split('\n') {
        if let Some(m) = LRC_LINE.captures(line) {
            let time = pulse_core::js::parse_int(&m[1]) * 60.0 + m[2].parse::<f64>().unwrap_or(0.0);
            let body = pulse_core::js::trim(&m[4]);
            let words = if body.contains('<') {
                parse_enhanced_words(body, time + 4.0)
            } else {
                None
            };
            let text = if words.is_some() {
                let t = pulse_core::js::re(r"<\d+:\d+(?:\.\d+)?>")
                    .replace_all(body, "")
                    .into_owned();
                pulse_core::js::trim(&pulse_core::js::collapse_ws(&t)).to_string()
            } else {
                body.to_string()
            };
            if !text.is_empty() {
                out.push(Line { time, text, words });
            }
        } else if !pulse_core::js::trim(line).is_empty() {
            out.push(Line {
                time: -1.0,
                text: pulse_core::js::trim(line).to_string(),
                words: None,
            });
        }
    }
    out
}

/// The line playing at `t` seconds (the last synced line at or before it).
pub fn current_line(lines: &[Line], t: f64) -> Option<usize> {
    lines.iter().rposition(|l| l.time >= 0.0 && l.time <= t)
}

async fn lrclib(net: &Net, path_and_query: &str) -> Option<Value> {
    let r = net
        .client
        .get(format!("https://lrclib.net{path_and_query}"))
        .header("User-Agent", LRCLIB_UA)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    if r.status().as_u16() != 200 {
        return None;
    }
    r.json().await.ok()
}

fn query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| {
            format!(
                "{k}={}",
                url::form_urlencoded::byte_serialize(v.as_bytes())
                    .collect::<String>()
                    .replace('+', "%20")
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Synced lyrics when lrclib has them, else plain ones: exact lookups first (with the length when
/// known), then searches where only a real match counts.
pub async fn fetch(net: &Net, title: &str, artist: &str, duration: f64) -> Option<String> {
    let want: LyricsQuery = lyrics_match::parse_for_lyrics(title, artist);
    let t0 = want.titles.first()?.clone();
    let t1 = want.titles.get(1).cloned().unwrap_or_default();
    let a0 = want.artists.first().cloned().unwrap_or_default();
    let a1 = want.artists.get(1).cloned().unwrap_or_default();
    let dur = if duration > 0.0 {
        pulse_core::js::round(duration)
    } else {
        0.0
    };
    let d = if dur > 0.0 {
        pulse_core::js::num_to_string(dur)
    } else {
        String::new()
    };
    let tries = [
        (&t0, &a0, d.as_str()),
        (&t0, &a0, ""),
        (&t0, &a1, ""),
        (&t1, &a0, ""),
    ];
    let tries: Vec<_> = tries
        .iter()
        .enumerate()
        .filter(|(i, (t, a, _))| !t.is_empty() && !a.is_empty() && (*i != 1 || dur > 0.0))
        .map(|(_, x)| *x)
        .collect();
    let exact = futures::future::join_all(tries.iter().map(|(t, a, d)| {
        let q = query(&[("track_name", t), ("artist_name", a), ("duration", d)]);
        async move { lrclib(net, &format!("/api/get?{q}")).await }
    }))
    .await;
    let mut plain: Option<String> = None;
    let text = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    for hit in exact.iter().flatten() {
        if let Some(s) = text(hit, "syncedLyrics") {
            return Some(s);
        }
        if plain.is_none() {
            plain = text(hit, "plainLyrics");
        }
    }
    let primary = want
        .artists
        .iter()
        .find(|a| !NO_PUNCT.is_match(a))
        .cloned()
        .unwrap_or(a0);
    let both = pulse_core::js::trim(&format!("{primary} {t0}")).to_string();
    let mut searches = futures::future::join_all([
        lrclib(
            net,
            &format!(
                "/api/search?{}",
                query(&[("track_name", &t0), ("artist_name", &primary)])
            ),
        ),
        lrclib(net, &format!("/api/search?{}", query(&[("q", &both)]))),
    ])
    .await;
    let best = |list: &Option<Value>| {
        list.as_ref()
            .and_then(Value::as_array)
            .and_then(|l| lyrics_match::pick_best(l, &want, dur).cloned())
    };
    if plain.is_none() && !searches.iter().any(|l| best(l).is_some()) {
        searches.push(
            lrclib(
                net,
                &format!("/api/search?{}", query(&[("track_name", &t0)])),
            )
            .await,
        );
    }
    for list in &searches {
        if let Some(b) = best(list) {
            if let Some(s) = text(&b, "syncedLyrics") {
                return Some(s);
            }
            if plain.is_none() {
                plain = text(&b, "plainLyrics");
            }
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_lines_and_words() {
        let l = parse_lrc(
            "[ar:x]\n[00:01.50]Hello world\n[00:03.00]<00:03.00>Word <00:03.50>by word\nplain line\n[01:02]\n\n",
        );
        assert_eq!(l.len(), 4, "{l:?}");
        // "[ar:x]" has no digits: a plain line, as in the original; "[01:02]" has no text: dropped
        assert_eq!((l[0].time, l[0].text.as_str()), (-1.0, "[ar:x]"));
        assert_eq!((l[1].time, l[1].text.as_str()), (1.5, "Hello world"));
        let w = l[2].words.as_ref().unwrap();
        assert_eq!(l[2].text, "Word by word");
        assert_eq!(
            w.iter()
                .map(|w| (w.text.as_str(), w.start, w.end))
                .collect::<Vec<_>>(),
            [("Word", 3.0, 3.5), ("by", 3.5, 7.0), ("word", 3.5, 7.0)]
        );
        assert_eq!((l[3].time, l[3].text.as_str()), (-1.0, "plain line"));
        assert_eq!(current_line(&l, 3.2), Some(2));
        assert_eq!(current_line(&l, 0.5), None);
    }
}
