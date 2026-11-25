//! YouTube and YouTube Music (mobile/extract.js), straight against InnerTube.
//!
//! Audio: the visionOS client's audio-only HLS (itag 234/233, AAC) needs no proof-of-origin token
//! and no signature deciphering (same approach as yt-dlp). Fallback: the iOS client's plain
//! audio-only formats. Ciphered formats are skipped (ADR-010).
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use pulse_core::js;
use regex::Regex;
use serde_json::{Value, json};

use crate::hls::Container;
use crate::http::Net;
use crate::{Audio, Entry, Result, Stream, Thumbnail};

pub const VOS_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
const IOS_UA: &str =
    "com.google.ios.youtube/20.10.4 (iPhone16,2; U; CPU iOS 18_3_2 like Mac OS X;)";
const WEB_VERSION: &str = "2.20250101.00.00";
const MUSIC_VERSION: &str = "1.20250101.01.00";
/// A fresh visitor id at most every 10 minutes (the original fetched the homepage on every tap).
const VISITOR_REFRESH: Duration = Duration::from_secs(600);

static ID: LazyLock<Regex> =
    LazyLock::new(|| js::re(r"(?:v=|youtu\.be\/|\/shorts\/|^)([\w-]{11})(?:[&?#]|$)"));
static VISITOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""VISITOR_DATA":"([^"]+)""#).expect("literal"));
static TOPIC: LazyLock<Regex> = LazyLock::new(|| js::re_flags(r"\s*-\s*Topic$", "i"));

#[derive(Default)]
pub struct State {
    visitor: Option<(String, Instant)>,
}

/// The 11-character video id in a URL or bare id.
pub fn video_id(u: &str) -> Option<String> {
    ID.captures(u).map(|c| c[1].to_string())
}

fn text(v: &Value) -> String {
    if let Some(s) = v.get("simpleText").and_then(Value::as_str) {
        return s.to_string();
    }
    js::list(v.get("runs"))
        .iter()
        .map(|r| js::str_or_empty(r.get("text")))
        .collect()
}

fn thumbs(v: Option<&Value>) -> Vec<Thumbnail> {
    let mut t: Vec<Thumbnail> = js::list(v)
        .iter()
        .filter_map(|t| {
            Some(Thumbnail {
                url: t.get("url")?.as_str()?.to_string(),
                width: t.get("width").and_then(Value::as_u64).map(|w| w as u32),
                height: t.get("height").and_then(Value::as_u64).map(|h| h as u32),
            })
        })
        .collect();
    t.sort_by_key(|t| t.width.unwrap_or(0));
    t
}

fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

/// Every object under `v` that has `key`, depth first (InnerTube nests results unpredictably).
fn find_all<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(o) => {
            if let Some(x) = o.get(key) {
                out.push(x);
            }
            for (k, x) in o {
                if k != key {
                    find_all(x, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| find_all(x, key, out)),
        _ => {}
    }
}

async fn innertube(
    net: &Net,
    host: &str,
    endpoint: &str,
    client: Value,
    body: Value,
    ua: &str,
) -> Result<Value> {
    let mut body = body;
    body["context"] = json!({ "client": client });
    let r = net
        .client
        .post(format!(
            "https://{host}/youtubei/v1/{endpoint}?prettyPrint=false"
        ))
        .header("User-Agent", ua)
        .header("Origin", format!("https://{host}"))
        .json(&body)
        .send()
        .await?;
    Ok(r.json().await?)
}

/// YouTube video search.
pub async fn search(net: &Net, query: &str, limit: usize) -> Result<Vec<Entry>> {
    let client =
        json!({ "clientName": "WEB", "clientVersion": WEB_VERSION, "hl": "en", "gl": "US" });
    let j = innertube(
        net,
        "www.youtube.com",
        "search",
        client,
        json!({ "query": query, "params": "EgIQAQ==" }),
        crate::http::DESKTOP_UA,
    )
    .await?;
    let mut found = Vec::new();
    find_all(&j, "videoRenderer", &mut found);
    Ok(found
        .into_iter()
        .filter_map(|v| {
            let id = v.get("videoId")?.as_str()?.to_string();
            let owner = v
                .get("ownerText")
                .or_else(|| v.get("longBylineText"))
                .unwrap_or(&Value::Null);
            let channel_id = js::list(owner.get("runs")).iter().find_map(|r| {
                r.pointer("/navigationEndpoint/browseEndpoint/browseId")?
                    .as_str()
                    .map(str::to_string)
            });
            Some(Entry {
                title: text(v.get("title")?),
                uploader: text(owner),
                channel_id,
                duration: pulse_core::recommend::parse_length(&text(
                    v.get("lengthText").unwrap_or(&Value::Null),
                )),
                thumbnails: thumbs(v.pointer("/thumbnail/thumbnails")),
                webpage_url: watch_url(&id),
                id,
                artists: Vec::new(),
            })
        })
        .take(limit)
        .collect())
}

/// YouTube Music song search.
pub async fn music_search(net: &Net, query: &str, limit: usize) -> Result<Vec<Entry>> {
    let client = json!({ "clientName": "WEB_REMIX", "clientVersion": MUSIC_VERSION, "hl": "en", "gl": "US" });
    let j = innertube(
        net,
        "music.youtube.com",
        "search",
        client,
        json!({ "query": query, "params": "EgWKAQIIAWoMEA4QChADEAQQCRAF" }),
        crate::http::DESKTOP_UA,
    )
    .await?;
    let mut found = Vec::new();
    find_all(&j, "musicResponsiveListItemRenderer", &mut found);
    Ok(found
        .into_iter()
        .filter_map(parse_music_item)
        .take(limit)
        .collect())
}

fn parse_music_item(r: &Value) -> Option<Entry> {
    let cols = js::list(r.get("flexColumns"));
    let col = |i: usize| {
        cols.get(i)
            .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text"))
    };
    let id = r
        .pointer("/playlistItemData/videoId")
        .or_else(|| col(0)?.pointer("/runs/0/navigationEndpoint/watchEndpoint/videoId"))?
        .as_str()?
        .to_string();
    let title = text(col(0)?);
    // "Artist & Other • Album • 3:45": artists until the first " • ", the length last
    let runs = js::list(col(1).and_then(|c| c.get("runs")));
    let cut = runs
        .iter()
        .position(|x| x.get("text").and_then(Value::as_str) == Some(" • "))
        .unwrap_or(runs.len());
    let artist_runs = &runs[..cut];
    let artists: Vec<String> = artist_runs
        .iter()
        .filter(|x| x.pointer("/navigationEndpoint/browseEndpoint").is_some())
        .filter_map(|x| x.get("text")?.as_str().map(str::to_string))
        .collect();
    let artists = if artists.is_empty() {
        artist_runs
            .iter()
            .filter_map(|x| x.get("text")?.as_str())
            .filter(|t| !matches!(js::trim(t), "&" | "," | ""))
            .map(str::to_string)
            .collect()
    } else {
        artists
    };
    let channel_id = artist_runs.iter().find_map(|x| {
        x.pointer("/navigationEndpoint/browseEndpoint/browseId")?
            .as_str()
            .map(str::to_string)
    });
    let duration = runs
        .iter()
        .rev()
        .filter_map(|x| x.get("text")?.as_str())
        .map(pulse_core::recommend::parse_length)
        .find(|d| *d > 0.0)
        .unwrap_or(0.0);
    Some(Entry {
        uploader: artists.first().cloned().unwrap_or_default(),
        artists,
        channel_id,
        duration,
        thumbnails: thumbs(r.pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")),
        webpage_url: watch_url(&id),
        title,
        id,
    })
}

async fn visitor_id(net: &Net, force: bool) -> Result<Option<String>> {
    {
        let st = net.youtube.lock().await;
        if let Some((v, at)) = &st.visitor
            && (!force || at.elapsed() < VISITOR_REFRESH)
        {
            return Ok(Some(v.clone()));
        }
    }
    let html = net
        .client
        .get("https://www.youtube.com/")
        .header("User-Agent", VOS_UA)
        .header("Accept-Language", "en-US")
        .send()
        .await?
        .text()
        .await?;
    let v = VISITOR.captures(&html).map(|c| c[1].to_string());
    net.youtube.lock().await.visitor = v.clone().map(|v| (v, Instant::now()));
    Ok(v)
}

async fn visitor_is_fresh(net: &Net) -> bool {
    net.youtube
        .lock()
        .await
        .visitor
        .as_ref()
        .is_some_and(|(_, at)| at.elapsed() < VISITOR_REFRESH)
}

/// The rendition URI for `itag` in an HLS master playlist.
pub fn pick_rendition(m3u: &str, itag: u32) -> Option<String> {
    m3u.lines()
        .filter(|l| l.starts_with("#EXT-X-MEDIA:"))
        .filter_map(|l| crate::hls::attr(l, "URI"))
        .find(|u| u.contains(&format!("/itag/{itag}/")))
}

async fn hls_audio(net: &Net, id: &str) -> Result<Audio> {
    for force in [false, true] {
        let visitor = visitor_id(net, force).await?;
        let mut client = json!({ "clientName": "VISIONOS", "clientVersion": "1.02", "deviceMake": "Apple", "deviceModel": "RealityDevice17,1",
            "userAgent": VOS_UA, "osName": "visionOS", "osVersion": "26.5.23O471", "hl": "en", "timeZone": "UTC", "utcOffsetMinutes": 0 });
        if let Some(v) = &visitor {
            client["visitorData"] = json!(v);
        }
        let mut req = net
            .client
            .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
            .header("User-Agent", VOS_UA)
            .header("X-Youtube-Client-Name", "101")
            .header("X-Youtube-Client-Version", "1.02")
            .header("Origin", "https://www.youtube.com")
            .json(&json!({ "context": { "client": client }, "videoId": id, "contentCheckOk": true, "racyCheckOk": true }));
        if let Some(v) = &visitor {
            req = req.header("X-Goog-Visitor-Id", v);
        }
        let j: Value = req.send().await?.json().await?;
        let Some(master) = j
            .pointer("/streamingData/hlsManifestUrl")
            .and_then(Value::as_str)
        else {
            let status = j
                .pointer("/playabilityStatus/status")
                .and_then(Value::as_str);
            if status == Some("LOGIN_REQUIRED") && !force && !visitor_is_fresh(net).await {
                continue;
            }
            return Err(j
                .pointer("/playabilityStatus/reason")
                .and_then(Value::as_str)
                .unwrap_or("no HLS stream")
                .to_string()
                .into());
        };
        let m3u = net.text(master, VOS_UA).await?;
        let playlist = pick_rendition(&m3u, 234)
            .or_else(|| pick_rendition(&m3u, 233))
            .ok_or("no audio rendition")?;
        let d = j.get("videoDetails").unwrap_or(&Value::Null);
        return Ok(Audio {
            stream: Stream::Hls {
                url: playlist,
                container: Container::Aac,
            },
            user_agent: VOS_UA.into(),
            ext: "aac".into(),
            artist: TOPIC
                .replace(&js::str_or_empty(d.get("author")), "")
                .into_owned(),
            title: js::str_or_empty(d.get("title")),
            duration: js::num_or_zero(d.get("lengthSeconds")),
        });
    }
    Err("YouTube refused the request".into())
}

/// Fallback: the iOS client's plain audio-only formats (m4a preferred).
async fn direct_audio(net: &Net, id: &str) -> Result<Audio> {
    let client = json!({ "clientName": "IOS", "clientVersion": "20.10.4", "deviceMake": "Apple", "deviceModel": "iPhone16,2", "osName": "iPhone", "osVersion": "18.3.2.22D82", "hl": "en", "gl": "US" });
    let j: Value = net
        .client
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("User-Agent", IOS_UA)
        .header("X-Youtube-Client-Name", "5")
        .header("X-Youtube-Client-Version", "20.10.4")
        .json(&json!({ "context": { "client": client }, "videoId": id, "contentCheckOk": true, "racyCheckOk": true }))
        .send()
        .await?
        .json()
        .await?;
    let formats: Vec<&Value> = js::list(j.pointer("/streamingData/adaptiveFormats"))
        .iter()
        .filter(|f| {
            js::str_or_empty(f.get("mimeType")).starts_with("audio/") && f.get("url").is_some()
        })
        .collect();
    let bitrate = |f: &&&Value| f.get("bitrate").and_then(Value::as_u64).unwrap_or(0);
    let pick = formats
        .iter()
        .filter(|f| js::str_or_empty(f.get("mimeType")).contains("mp4"))
        .max_by_key(bitrate)
        .or_else(|| formats.iter().max_by_key(bitrate));
    let Some(f) = pick else {
        return Err(j
            .pointer("/playabilityStatus/reason")
            .and_then(Value::as_str)
            .unwrap_or("could not extract audio stream")
            .to_string()
            .into());
    };
    let d = j.get("videoDetails").unwrap_or(&Value::Null);
    let mp4 = js::str_or_empty(f.get("mimeType")).contains("mp4");
    let size = f
        .get("contentLength")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok());
    Ok(Audio {
        stream: Stream::File {
            url: js::str_or_empty(f.get("url")),
            size,
        },
        user_agent: IOS_UA.into(),
        ext: if mp4 { "m4a" } else { "webm" }.into(),
        artist: TOPIC
            .replace(&js::str_or_empty(d.get("author")), "")
            .into_owned(),
        title: js::str_or_empty(d.get("title")),
        duration: js::num_or_zero(d.get("lengthSeconds")),
    })
}

/// The audio of video `id`.
pub async fn audio(net: &Net, id: &str) -> Result<Audio> {
    match hls_audio(net, id).await {
        Ok(a) => Ok(a),
        Err(first) => direct_audio(net, id)
            .await
            .map_err(|e| format!("{first}; fallback: {e}").into()),
    }
}

/// A YouTube playlist's songs (link import), up to `limit`.
pub async fn playlist(net: &Net, list_id: &str, limit: usize) -> Result<(String, Vec<Entry>)> {
    let client =
        json!({ "clientName": "WEB", "clientVersion": WEB_VERSION, "hl": "en", "gl": "US" });
    let mut j = innertube(
        net,
        "www.youtube.com",
        "browse",
        client.clone(),
        json!({ "browseId": format!("VL{list_id}") }),
        crate::http::DESKTOP_UA,
    )
    .await?;
    let title = j
        .pointer("/metadata/playlistMetadataRenderer/title")
        .and_then(Value::as_str)
        .unwrap_or("Playlist")
        .to_string();
    let mut entries = Vec::new();
    loop {
        let mut items = Vec::new();
        find_all(&j, "playlistVideoRenderer", &mut items);
        for v in items {
            let Some(id) = v.get("videoId").and_then(Value::as_str) else {
                continue;
            };
            if entries.len() < limit {
                let owner = v.get("shortBylineText").unwrap_or(&Value::Null);
                entries.push(Entry {
                    id: id.into(),
                    title: text(v.get("title").unwrap_or(&Value::Null)),
                    uploader: text(owner),
                    duration: js::num_or_zero(v.get("lengthSeconds")),
                    thumbnails: thumbs(v.pointer("/thumbnail/thumbnails")),
                    webpage_url: watch_url(id),
                    ..Default::default()
                });
            }
        }
        let mut tokens = Vec::new();
        find_all(&j, "continuationCommand", &mut tokens);
        let token = tokens
            .iter()
            .find_map(|c| c.get("token")?.as_str().map(str::to_string));
        match token {
            Some(t) if entries.len() < limit => {
                j = innertube(
                    net,
                    "www.youtube.com",
                    "browse",
                    client.clone(),
                    json!({ "continuation": t }),
                    crate::http::DESKTOP_UA,
                )
                .await?;
            }
            _ => break,
        }
    }
    Ok((title, entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_renditions() {
        assert_eq!(
            video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            video_id("https://youtu.be/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(video_id("dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("https://x/watch?v=short"), None);
        let m3u = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",URI=\"https://m/itag/233/x/index.m3u8\"\n#EXT-X-MEDIA:TYPE=AUDIO,URI=\"https://m/itag/234/y/index.m3u8\"\n";
        assert_eq!(
            pick_rendition(m3u, 234).as_deref(),
            Some("https://m/itag/234/y/index.m3u8")
        );
        assert_eq!(pick_rendition(m3u, 140), None);
    }
}
