//! The shared HTTP client (the original's `__pulseFetch`).
use std::time::Duration;

pub const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// HTTP client plus the per-service state the original keeps in module variables.
pub struct Net {
    pub client: reqwest::Client,
    pub youtube: tokio::sync::Mutex<crate::youtube::State>,
    pub soundcloud: tokio::sync::Mutex<Option<String>>,
}

impl Net {
    pub fn new() -> crate::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .gzip(true)
            .build()?;
        Ok(Self {
            client,
            youtube: Default::default(),
            soundcloud: Default::default(),
        })
    }

    pub async fn text(&self, url: &str, ua: &str) -> crate::Result<String> {
        Ok(self
            .client
            .get(url)
            .header("User-Agent", ua)
            .send()
            .await?
            .text()
            .await?)
    }
}
