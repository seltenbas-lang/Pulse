//! Network side of Pulse (mobile/extract.js, mobile/shims/child_process.js, server.js lyrics):
//! YouTube / YouTube Music / SoundCloud search and audio, HLS joining, lrclib lyrics.
pub mod hls;
pub mod http;
pub mod lyrics;
pub mod soundcloud;
pub mod youtube;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Msg(s.to_string())
    }
}
impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Msg(s)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Thumbnail {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// A search or playlist result, yt-dlp style (the shape the original passes around).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub uploader: String,
    #[serde(default)]
    pub artists: Vec<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    /// Seconds.
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub thumbnails: Vec<Thumbnail>,
    pub webpage_url: String,
}

/// How a resolved song is fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stream {
    /// One file at `url`; `size` when known (googlevideo files are fetched in 1 MiB ranges).
    File { url: String, size: Option<u64> },
    /// An HLS media playlist whose segments are joined into one `container` file.
    Hls {
        url: String,
        container: hls::Container,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub stream: Stream,
    pub user_agent: String,
    /// File extension of the joined/downloaded audio: aac, m4a, mp3, webm.
    pub ext: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
}

/// Resolves anything yt-dlp would take: a URL, `ytsearch1:query`, `scsearch1:query`.
pub async fn resolve_audio(net: &http::Net, target: &str) -> Result<Audio> {
    if let Some(caps) = regex::Regex::new(r"^(yt|sc)search\d*:(.*)$")
        .ok()
        .and_then(|r| {
            r.captures(target)
                .map(|c| (c[1].to_string(), c[2].to_string()))
        })
    {
        let hit = if caps.0 == "sc" {
            soundcloud::search(net, &caps.1, 1).await?
        } else {
            youtube::search(net, &caps.1, 1).await?
        };
        let hit = hit.into_iter().next().ok_or("no search results")?;
        return Box::pin(resolve_audio(net, &hit.webpage_url)).await;
    }
    if target.contains("soundcloud.com") {
        return soundcloud::audio(net, target).await;
    }
    if let Some(id) = youtube::video_id(target) {
        return youtube::audio(net, &id).await;
    }
    Err(format!("unsupported target {target}").into())
}

/// Downloads a resolved song into `path` as it arrives, in order, so it can play before it has
/// all come in. `progress(written, total)` follows each write (`total` when known).
pub async fn download_to(
    net: &http::Net,
    audio: &Audio,
    path: &std::path::Path,
    mut progress: impl FnMut(u64, Option<u64>) + Send,
) -> Result<()> {
    use futures::stream::{self, StreamExt};
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    let mut written = 0u64;
    match &audio.stream {
        Stream::File { url, size: Some(size) } if url.contains("googlevideo.com") => {
            // 1 MiB ranges, 4 at a time, written in order as each arrives
            let size = *size;
            let step = 1u64 << 20;
            let jobs: Vec<(usize, String)> = (0..size)
                .step_by(step as usize)
                .enumerate()
                .map(|(i, a)| (i, format!("{url}&range={a}-{}", (a + step).min(size) - 1)))
                .collect();
            let (client, ua) = (net.client.clone(), audio.user_agent.clone());
            let mut chunks = stream::iter(jobs)
                .map(|(i, u)| hls::fetch_one(client.clone(), u, ua.clone(), i))
                .buffered(4);
            while let Some(chunk) = chunks.next().await {
                let chunk = chunk?;
                file.write_all(&chunk)?;
                written += chunk.len() as u64;
                progress(written, Some(size));
            }
            if written != size {
                return Err(format!("incomplete download: {written} of {size} bytes").into());
            }
        }
        Stream::File { url, size } => {
            let mut r = net.client.get(url).header("User-Agent", &audio.user_agent).send().await?;
            if !r.status().is_success() {
                return Err(format!("download failed: {}", r.status().as_u16()).into());
            }
            let total = size.or_else(|| r.content_length());
            while let Some(chunk) = r.chunk().await? {
                file.write_all(&chunk)?;
                written += chunk.len() as u64;
                progress(written, total);
            }
        }
        Stream::Hls { .. } => {
            // segments are joined into one container: written once they are all in
            let bytes = download(net, audio).await?;
            file.write_all(&bytes)?;
            written = bytes.len() as u64;
            progress(written, Some(written));
        }
    }
    file.flush()?;
    Ok(())
}

/// Downloads a resolved song into memory: HLS segments joined, googlevideo files in ranges.
pub async fn download(net: &http::Net, audio: &Audio) -> Result<Vec<u8>> {
    match &audio.stream {
        Stream::Hls { url, container } => {
            hls::download(net, url, &audio.user_agent, *container).await
        }
        Stream::File {
            url,
            size: Some(size),
        } if url.contains("googlevideo.com") => {
            hls::download_ranges(net, url, &audio.user_agent, *size).await
        }
        Stream::File { url, .. } => {
            let r = net
                .client
                .get(url)
                .header("User-Agent", &audio.user_agent)
                .send()
                .await?;
            if !r.status().is_success() {
                return Err(format!("download failed: {}", r.status().as_u16()).into());
            }
            Ok(r.bytes().await?.to_vec())
        }
    }
}
