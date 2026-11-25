//! HLS media playlists and joining their segments into one file (mobile/ts2aac.js,
//! downloadHls/joinSegments in mobile/shims/child_process.js).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Container {
    /// YouTube: packed audio (ADTS behind ID3) or MPEG-TS -> raw ADTS AAC.
    Aac,
    /// SoundCloud: MP3 segments, ID3 stripped.
    Mp3,
    /// SoundCloud: init segment + fragments -> fragmented MP4.
    Fmp4,
}

/// The segment URLs of a media playlist, init segment (EXT-X-MAP) first. Errors on encrypted or
/// empty playlists.
pub fn segment_urls(m3u: &str, playlist_url: &str) -> Result<Vec<String>, String> {
    let base = url::Url::parse(playlist_url).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = m3u.split('\n').map(str::trim).collect();
    if lines
        .iter()
        .any(|l| l.starts_with("#EXT-X-KEY") && !l.contains("METHOD=NONE"))
    {
        return Err("this song is only available encrypted (DRM)".into());
    }
    let join = |u: &str| {
        base.join(u)
            .map(|u| u.to_string())
            .map_err(|e| e.to_string())
    };
    let mut urls = lines
        .iter()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| join(l))
        .collect::<Result<Vec<_>, _>>()?;
    let map = lines
        .iter()
        .filter(|l| l.starts_with("#EXT-X-MAP"))
        .find_map(|l| attr(l, "URI"));
    if let Some(m) = map {
        urls.insert(0, join(&m)?);
    }
    if urls.is_empty() {
        return Err("empty HLS playlist".into());
    }
    Ok(urls)
}

/// `NAME="value"` from an EXT-X tag line.
pub fn attr(line: &str, name: &str) -> Option<String> {
    let start = line.find(&format!("{name}=\""))? + name.len() + 2;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

/// MPEG-TS -> the audio elementary stream (ADTS).
pub fn ts_to_aac(ts: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut audio_pid: i32 = -1;
    let mut i = 0usize;
    while i + 188 <= ts.len() {
        if ts[i] != 0x47 {
            // resync after garbage
            match ts[i + 1..].iter().position(|&b| b == 0x47) {
                Some(n) => {
                    i = i + 1 + n;
                    continue;
                }
                None => break,
            }
        }
        let pkt = &ts[i..i + 188];
        i += 188;
        let pusi = pkt[1] & 0x40 != 0;
        let pid = (i32::from(pkt[1] & 0x1f) << 8) | i32::from(pkt[2]);
        let afc = (pkt[3] >> 4) & 3;
        let mut p = 4usize;
        if afc == 2 {
            continue;
        }
        if afc == 3 {
            p += 1 + usize::from(pkt[4]);
        }
        if p >= 188 {
            continue;
        }
        if pusi && p + 3 < 188 && pkt[p] == 0 && pkt[p + 1] == 0 && pkt[p + 2] == 1 {
            let stream_id = pkt[p + 3];
            if (0xc0..=0xdf).contains(&stream_id) {
                audio_pid = pid;
                p += 9 + usize::from(*pkt.get(p + 8).unwrap_or(&0));
            } else if pid == audio_pid {
                continue;
            }
        }
        if pid == audio_pid && audio_pid >= 0 && p < 188 {
            out.extend_from_slice(&pkt[p..]);
        }
    }
    out
}

/// Drops leading ID3 tags.
pub fn strip_id3(seg: &[u8]) -> &[u8] {
    let mut p = 0usize;
    while seg.len() >= p + 10 && &seg[p..p + 3] == b"ID3" {
        let size = (usize::from(seg[p + 6]) << 21)
            | (usize::from(seg[p + 7]) << 14)
            | (usize::from(seg[p + 8]) << 7)
            | usize::from(seg[p + 9]);
        p += 10 + size + if seg[p + 5] & 0x10 != 0 { 10 } else { 0 };
    }
    seg.get(p..).unwrap_or(&[])
}

/// Downloaded segments -> one playable file.
pub fn join_segments(parts: &[Vec<u8>], container: Container) -> Vec<u8> {
    let mut out = Vec::with_capacity(parts.iter().map(Vec::len).sum());
    for seg in parts {
        match container {
            Container::Aac if seg.first() == Some(&0x47) => out.extend(ts_to_aac(seg)),
            Container::Aac | Container::Mp3 => out.extend_from_slice(strip_id3(seg)),
            Container::Fmp4 => out.extend_from_slice(seg),
        }
    }
    out
}

/// One URL, retried twice when flaky (never on a 4xx).
pub(crate) async fn fetch_one(client: reqwest::Client, u: String, ua: String, i: usize) -> crate::Result<Vec<u8>> {
    let mut attempt = 0;
    loop {
        let r = client.get(&u).header("User-Agent", &ua).send().await;
        let err = match r {
            Ok(r) if r.status().is_success() => match r.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => crate::Error::from(e),
            },
            Ok(r) if r.status().is_client_error() => {
                return Err(format!("segment {i} failed: {}", r.status().as_u16()).into());
            }
            Ok(r) => format!("segment {i} failed: {}", r.status().as_u16()).into(),
            Err(e) => e.into(),
        };
        if attempt >= 2 {
            return Err(err);
        }
        attempt += 1;
        tokio::time::sleep(std::time::Duration::from_millis(600 * attempt)).await;
    }
}

/// Fetches every URL, 4 at a time, retrying flaky ones twice (never a 4xx), in order.
pub async fn fetch_all(
    net: &crate::http::Net,
    urls: &[String],
    ua: &str,
) -> crate::Result<Vec<Vec<u8>>> {
    use futures::stream::{self, StreamExt, TryStreamExt};
    let jobs: Vec<(usize, String, reqwest::Client, String)> = urls
        .iter()
        .enumerate()
        .map(|(i, u)| (i, u.clone(), net.client.clone(), ua.to_string()))
        .collect();
    stream::iter(jobs)
        .map(|(i, u, client, ua)| fetch_one(client, u, ua, i))
        .buffered(4)
        .try_collect()
        .await
}

/// Fetches an HLS media playlist and joins its segments.
pub async fn download(
    net: &crate::http::Net,
    playlist_url: &str,
    ua: &str,
    container: Container,
) -> crate::Result<Vec<u8>> {
    let m3u = net.text(playlist_url, ua).await?;
    let urls = segment_urls(&m3u, playlist_url)?;
    Ok(join_segments(&fetch_all(net, &urls, ua).await?, container))
}

/// A googlevideo file in 1 MiB ranges (long single requests get throttled), checked to be whole.
pub async fn download_ranges(
    net: &crate::http::Net,
    url: &str,
    ua: &str,
    size: u64,
) -> crate::Result<Vec<u8>> {
    let step = 1u64 << 20;
    let urls: Vec<String> = (0..size)
        .step_by(step as usize)
        .map(|a| format!("{url}&range={a}-{}", (a + step).min(size) - 1))
        .collect();
    let bytes: Vec<u8> = fetch_all(net, &urls, ua).await?.concat();
    if bytes.len() as u64 != size {
        return Err(format!("incomplete download: {} of {size} bytes", bytes.len()).into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_urls_and_drm() {
        let m3u = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:10,\nseg1.m4s\n#EXTINF:10,\nhttps://cdn/x/seg2.m4s\n";
        assert_eq!(
            segment_urls(m3u, "https://h/a/b/list.m3u8").unwrap(),
            [
                "https://h/a/b/init.mp4",
                "https://h/a/b/seg1.m4s",
                "https://cdn/x/seg2.m4s"
            ]
        );
        assert!(segment_urls("#EXT-X-KEY:METHOD=SAMPLE-AES\nx", "https://h/").is_err());
        assert!(segment_urls("#EXT-X-KEY:METHOD=NONE\nx", "https://h/").is_ok());
        assert!(segment_urls("#EXTM3U\n", "https://h/").is_err());
    }

    #[test]
    fn id3_and_ts() {
        let mut seg = b"ID3\x04\x00\x00\x00\x00\x00\x02ab".to_vec();
        seg.extend_from_slice(&[0xff, 0xf1]);
        assert_eq!(strip_id3(&seg), &[0xff, 0xf1]);
        // one TS packet: PES audio header (stream 0xc0) then payload
        let mut pkt = vec![0x47, 0x40, 0x101u16 as u8, 0x10];
        pkt.extend_from_slice(&[0, 0, 1, 0xc0, 0, 0, 0x80, 0, 0]);
        pkt.resize(188, 0xaa);
        let mut two = pkt.clone();
        let mut cont = vec![0x47, 0x00, 0x01, 0x10];
        cont.resize(188, 0xbb);
        two.extend(cont);
        let aac = ts_to_aac(&two);
        assert_eq!(aac.len(), (188 - 13) + 184);
        assert!(aac.iter().take(175).all(|&b| b == 0xaa));
    }
}
