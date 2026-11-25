//! The player's lyric tools (app.js): Edit lyrics (custom lyrics for a song), Sync lyrics (tap each
//! line as it is sung), and tapping a synced line to jump there. Custom lyrics live in
//! `<data>/lyrics/`, one .lrc per song, and win over the found ones (server.js getCustomLyricsPath).

use super::*;

/// The song being synced and each line's tapped time.
pub(super) struct Sync {
    title: String,
    artist: String,
    lines: Vec<String>,
    times: Vec<Option<f64>>,
}

/// app.js lrcStamp: `[m:ss.xx]`.
fn lrc_stamp(t: f64) -> String {
    let t = t.max(0.0);
    format!("[{}:{:05.2}]", (t / 60.0).floor() as u64, t % 60.0)
}

impl App {
    /// Where a song's custom lyrics are kept: the artist and title, made safe for a file name.
    pub(super) fn custom_lyrics_path(&self, title: &str, artist: &str) -> PathBuf {
        let clean = |s: &str| -> String {
            s.trim()
                .chars()
                .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
                .collect::<String>()
                .to_lowercase()
        };
        let mut name = format!("{} - {}", clean(artist), clean(title));
        name.truncate(name.char_indices().nth(150).map_or(name.len(), |(i, _)| i));
        self.platform.data_dir.join("lyrics").join(format!("{name}.lrc"))
    }

    fn now_song(&self) -> Option<(String, String)> {
        self.current
            .as_ref()
            .map(|t| (js::str_or_empty(t.get("title")), js::str_or_empty(t.get("artist"))))
    }

    /// Tap a synced line: play from there.
    pub(super) fn lyric_seek(&mut self, i: usize) {
        if let Some(l) = self.lyrics.get(i).filter(|l| l.time >= 0.0) {
            self.player.seek(l.time);
        }
    }

    /// app.js openCustomLyricsEditor: the lyrics on screen, with their times kept.
    pub(super) fn lyrics_edit(&mut self) {
        if self.current.is_none() {
            self.toast("Play a track first to add lyrics");
            return;
        }
        let text: Vec<String> = self
            .lyrics
            .iter()
            .map(|l| if l.time >= 0.0 { format!("{}{}", lrc_stamp(l.time), l.text) } else { l.text.clone() })
            .collect();
        self.ui.set_lyrics_draft(text.join("\n").into());
        self.ui.set_lyrics_editing(true);
    }

    /// app.js saveCustomLyrics: saved for this song; empty removes them (the found lyrics come back).
    pub(super) fn lyrics_save(&mut self, text: &str) {
        let Some((title, artist)) = self.now_song() else { return };
        let path = self.custom_lyrics_path(&title, &artist);
        let text = text.trim();
        let r = if text.is_empty() {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else {
            path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, text))
        };
        match r {
            Ok(()) => {
                self.ui.set_lyrics_editing(false);
                self.toast(if text.is_empty() { "Lyrics removed" } else { "Lyrics saved successfully" });
                if let Some(t) = self.current.clone() {
                    self.load_lyrics(&t);
                }
            }
            Err(e) => self.toast(&format!("Failed to save lyrics: {e}")),
        }
    }

    /// app.js openSyncMode: every line untimed, the first one next; the song starts if paused.
    pub(super) fn sync_start(&mut self) {
        let Some((title, artist)) = self.now_song() else {
            self.toast("Play a track first to sync lyrics");
            return;
        };
        if self.lyrics.is_empty() {
            self.toast("Add lyrics first using the edit button, then sync them");
            return;
        }
        let lines: Vec<String> = self.lyrics.iter().map(|l| l.text.clone()).collect();
        self.sync = Some(Sync { title, artist, times: vec![None; lines.len()], lines });
        self.show_sync();
        self.ui.set_syncing(true);
        if !self.player.is_playing() {
            self.toggle_play();
        }
        self.toast("Press play, then tap each line when you hear it");
    }

    fn show_sync(&self) {
        let Some(s) = &self.sync else { return };
        let next = s.times.iter().position(Option::is_none);
        let rows: Vec<SyncLine> = s
            .lines
            .iter()
            .zip(&s.times)
            .enumerate()
            .map(|(i, (text, t))| SyncLine {
                text: text.as_str().into(),
                stamp: t.map(lrc_stamp).unwrap_or_default().into(),
                next: next == Some(i),
            })
            .collect();
        let done = s.times.iter().filter(|t| t.is_some()).count();
        self.ui.set_sync_lines(ModelRc::new(VecModel::from(rows)));
        self.ui.set_sync_progress(format!("{done} / {} lines synced", s.lines.len()).into());
    }

    fn sync_is_current(&self) -> bool {
        match (&self.sync, self.now_song()) {
            (Some(s), Some((t, a))) => s.title == t && s.artist == a,
            _ => false,
        }
    }

    /// app.js handleSyncLineClick: the line gets the song's current time.
    pub(super) fn sync_tap(&mut self, i: usize) {
        if !self.sync_is_current() {
            self.toast("The song changed. Save or restart the sync.");
            return;
        }
        let now = self.player.position();
        if let Some(t) = self.sync.as_mut().and_then(|s| s.times.get_mut(i)) {
            *t = Some(now);
        }
        self.show_sync();
    }

    pub(super) fn sync_restart(&mut self) {
        if let Some(s) = &mut self.sync {
            s.times.iter_mut().for_each(|t| *t = None);
        }
        if self.sync_is_current() {
            self.player.seek(0.0);
        }
        self.show_sync();
        self.toast("Sync restarted");
    }

    /// app.js saveSyncAndExit: the timed lines as LRC, saved as this song's custom lyrics.
    pub(super) fn sync_save(&mut self) {
        let Some(s) = self.sync.take() else { return };
        let lrc: Vec<String> = s
            .lines
            .iter()
            .zip(&s.times)
            .map(|(text, t)| t.map_or_else(|| text.clone(), |t| format!("{}{text}", lrc_stamp(t))))
            .collect();
        let path = self.custom_lyrics_path(&s.title, &s.artist);
        match path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, lrc.join("\n"))) {
            Ok(()) => {
                self.ui.set_syncing(false);
                self.toast("Synced lyrics saved");
                if self.now_song() == Some((s.title.clone(), s.artist.clone()))
                    && let Some(t) = self.current.clone()
                {
                    self.load_lyrics(&t);
                }
            }
            Err(e) => {
                self.toast(&format!("Failed to save synced lyrics: {e}"));
                self.sync = Some(s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stamps_like_app_js() {
        assert_eq!(super::lrc_stamp(4.0), "[0:04.00]");
        assert_eq!(super::lrc_stamp(83.456), "[1:23.46]");
    }
}
