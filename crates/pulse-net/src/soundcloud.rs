//! SoundCloud through its public api-v2 with the web client's client_id (mobile/extract.js).
use std::sync::LazyLock;

use pulse_core::js;
use regex::Regex;
use serde_json::Value;

use crate::hls::Container;
use crate::http::{DESKTOP_UA, Net};
use crate::{Audio, Entry, Result, Stream, Thumbnail};

static SCRIPT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<script[^>]+src="(https://a-v2\.sndcdn\.com/assets/[^"]+\.js)""#)
        .expect("literal")
});
static CLIENT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"client_id\s*[:=]\s*"([a-zA-Z0-9]{20,})""#).expect("literal"));
static CANONICAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<link[^>]+rel="canonical"[^>]+href="([^"]+)""#).expect("literal")
});
static OG_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"property="og:url"[^>]+content="([^"]+)""#).expect("literal"));

async fn client_id(net: &Net, force: bool) -> Result<String> {
    if !force && let Some(id) = net.soundcloud.lock().await.clone() {
        return Ok(id);
    }
    let html = net.text("https://soundcloud.com/", DESKTOP_UA).await?;
    let scripts: Vec<String> = SCRIPT
        .captures_iter(&html)
        .map(|c| c[1].to_string())
        .collect();
    for s in scripts.iter().rev() {
        let js = net.text(s, DESKTOP_UA).await?;
        if let Some(c) = CLIENT_ID.captures(&js) {
            let id = c[1].to_string();
            *net.soundcloud.lock().await = Some(id.clone());
            return Ok(id);
        }
    }
    Err("SoundCloud client id not found".into())
}

async fn api(net: &Net, path: &str) -> Result<Value> {
    for force in [false, true] {
        let id = client_id(net, force).await?;
        let sep = if path.contains('?') { '&' } else { '?' };
        let r = net
            .client
            .get(format!(
                "https://api-v2.soundcloud.com{path}{sep}client_id={id}"
            ))
            .header("User-Agent", DESKTOP_UA)
            .send()
            .await?;
        let status = r.status().as_u16();
        if status == 401 || status == 403 {
            continue; // client_id rotated
        }
        if !r.status().is_success() {
            return Err(format!("SoundCloud {status}").into());
        }
        return Ok(r.json().await?);
    }
    Err("SoundCloud refused the request".into())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub url: String,
    pub rank: u8,
    /// `None` for a progressive file.
    pub hls: Option<Container>,
    pub ext: &'static str,
}

/// The transcodings Pulse can play, best first: progressive MP3, HLS MP3, HLS AAC (fMP4).
/// Encrypted HLS (DRM) and Opus-in-Ogg are left out.
pub fn candidates(list: &[Value]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = list
        .iter()
        .filter(|x| js::or(x, "url").is_some() && js::or(x, "snipped").is_none())
        .filter_map(|x| {
            let mime = js::str_or_empty(x.pointer("/format/mime_type")).to_lowercase();
            let proto = js::str_or_empty(x.pointer("/format/protocol"));
            let url = js::str_or_empty(x.get("url"));
            if proto == "progressive" {
                return Some(Candidate {
                    url,
                    rank: 0,
                    hls: None,
                    ext: "mp3",
                });
            }
            if proto != "hls" {
                return None;
            }
            // audio/mpegurl is an adaptive master playlist, not MP3 segments
            let mpeg = mime
                .match_indices("audio/mpeg")
                .any(|(i, _)| !mime[i + 10..].starts_with("url"));
            if mpeg {
                Some(Candidate {
                    url,
                    rank: 1,
                    hls: Some(Container::Mp3),
                    ext: "mp3",
                })
            } else if mime.contains("audio/mp4") || mime.contains("mp4a") || mime.contains("aac") {
                Some(Candidate {
                    url,
                    rank: 2,
                    hls: Some(Container::Fmp4),
                    ext: "m4a",
                })
            } else {
                None
            }
        })
        .collect();
    out.sort_by_key(|c| c.rank);
    out
}

fn transcodings(t: &Value) -> &[Value] {
    js::list(t.pointer("/media/transcodings"))
}

/// Go+ songs only stream a 30-second preview without a subscription.
pub fn is_preview(t: &Value) -> bool {
    t.get("policy").and_then(Value::as_str) == Some("SNIP")
        || transcodings(t)
            .iter()
            .any(|x| js::or(x, "snipped").is_some())
}

pub fn is_playable(t: &Value) -> bool {
    !is_preview(t) && !candidates(transcodings(t)).is_empty()
}

pub fn entry(t: &Value) -> Entry {
    let art = js::str_or_empty(t.get("artwork_url"));
    let dur = js::or(t, "full_duration")
        .or_else(|| js::or(t, "duration"))
        .map_or(0.0, |d| js::to_number(Some(d)));
    Entry {
        id: js::to_string(t.get("id").unwrap_or(&Value::Null)),
        title: js::str_or_empty(t.get("title")),
        uploader: js::str_or_empty(t.pointer("/user/username")),
        duration: js::round(dur / 1000.0),
        thumbnails: if art.is_empty() {
            Vec::new()
        } else {
            vec![Thumbnail {
                url: art.replace("-large.", "-t500x500."),
                ..Default::default()
            }]
        },
        webpage_url: js::str_or_empty(t.get("permalink_url")),
        ..Default::default()
    }
}

pub async fn search(net: &Net, query: &str, limit: usize) -> Result<Vec<Entry>> {
    let q: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let r = api(
        net,
        &format!("/search/tracks?q={q}&limit={}", (limit * 3).min(50)),
    )
    .await?;
    Ok(js::list(r.get("collection"))
        .iter()
        .filter(|t| t.get("kind").and_then(Value::as_str) == Some("track") && is_playable(t))
        .take(limit)
        .map(entry)
        .collect())
}

/// The canonical https://soundcloud.com/... link of a short (on.soundcloud.com) or mobile link,
/// without tracking parameters.
pub async fn canonical_url(net: &Net, page_url: &str) -> Result<String> {
    let Ok(mut u) = url::Url::parse(js::trim(page_url)) else {
        return Ok(page_url.to_string());
    };
    if u.host_str() == Some("on.soundcloud.com") {
        let r = net
            .client
            .get(u.as_str())
            .header("User-Agent", DESKTOP_UA)
            .send()
            .await?;
        let fin = r.url().to_string();
        let mut target = if !fin.starts_with("https://on.soundcloud.com")
            && !fin.starts_with("http://on.soundcloud.com")
        {
            fin
        } else {
            String::new()
        };
        if target.is_empty() {
            let html = r.text().await.unwrap_or_default();
            target = CANONICAL
                .captures(&html)
                .or_else(|| OG_URL.captures(&html))
                .map(|c| c[1].to_string())
                .unwrap_or_default();
        }
        if target.is_empty() {
            return Err("SoundCloud short link could not be opened".into());
        }
        u = url::Url::parse(&target).map_err(|e| e.to_string())?;
    }
    Ok(format!(
        "https://soundcloud.com{}",
        u.path().trim_end_matches('/')
    ))
}

pub async fn audio(net: &Net, page_url: &str) -> Result<Audio> {
    let canonical = canonical_url(net, page_url).await?;
    let q: String = url::form_urlencoded::byte_serialize(canonical.as_bytes()).collect();
    let t = api(net, &format!("/resolve?url={q}")).await?;
    if is_preview(&t) {
        return Err("SoundCloud only has a 30-second preview of this song".into());
    }
    if transcodings(&t).is_empty() {
        return Err("no SoundCloud stream".into());
    }
    let cands = candidates(transcodings(&t));
    if cands.is_empty() {
        return Err(
            "SoundCloud only offers this song encrypted (DRM), it can't be played here".into(),
        );
    }
    let auth = js::or(&t, "track_authorization")
        .map(|a| format!("?track_authorization={}", js::to_string(a)))
        .unwrap_or_default();
    let mut last = None;
    for c in cands {
        let path = c.url.replace("https://api-v2.soundcloud.com", "") + &auth;
        match api(net, &path).await {
            Ok(r) => {
                let Some(url) = r.get("url").and_then(Value::as_str).map(str::to_string) else {
                    last = Some("no SoundCloud stream URL".into());
                    continue;
                };
                let stream = match c.hls {
                    Some(container) => Stream::Hls { url, container },
                    None => Stream::File { url, size: None },
                };
                return Ok(Audio {
                    stream,
                    user_agent: DESKTOP_UA.into(),
                    ext: c.ext.into(),
                    title: js::str_or_empty(t.get("title")),
                    artist: js::str_or_empty(t.pointer("/user/username")),
                    duration: js::round(js::num_or_zero(t.get("duration")) / 1000.0),
                });
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| "no SoundCloud stream".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn candidates_rank_and_drm() {
        let list = vec![
            json!({"url": "a", "format": {"protocol": "hls", "mime_type": "audio/mp4; codecs=\"mp4a.40.2\""}}),
            json!({"url": "b", "format": {"protocol": "ctr-encrypted-hls", "mime_type": "audio/mp4"}}),
            json!({"url": "c", "format": {"protocol": "hls", "mime_type": "audio/mpeg"}}),
            json!({"url": "d", "format": {"protocol": "hls", "mime_type": "audio/mpegurl"}}),
            json!({"url": "e", "format": {"protocol": "progressive", "mime_type": "audio/mpeg"}}),
            json!({"url": "f", "snipped": true, "format": {"protocol": "progressive"}}),
            json!({"url": "g", "format": {"protocol": "hls", "mime_type": "audio/ogg; codecs=\"opus\""}}),
        ];
        let cands = candidates(&list);
        let c: Vec<&str> = cands.iter().map(|c| c.url.as_str()).collect();
        assert_eq!(c, ["e", "c", "a"]);
        assert!(is_preview(&json!({"policy": "SNIP"})));
        assert!(!is_playable(&json!({"media": {"transcodings": [list[1]]}})));
    }
}
