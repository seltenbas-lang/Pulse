# Pulse

A lightweight desktop music player for Windows, Linux and macOS. Stream from YouTube, YouTube Music and SoundCloud, download for offline listening, and keep a local library — no account, no subscription.

> Pulse is an independent, non-commercial open-source project and is not affiliated with YouTube, Google,
> SoundCloud, Spotify or Apple. It hosts no music. Use it for private listening and only download music you
> have the right to. See [LEGAL.md](LEGAL.md) (also for rights holders).

## Features

- **Streaming & downloads** — YouTube, YouTube Music and SoundCloud via yt-dlp and ffmpeg (installed automatically on first run)
- **Local library** — reads tags and album art, folders and playlists, original files stay untouched
- **Lyrics** — online synced lyrics, LRC files, manual timing, fullscreen karaoke
- **Audio** — EQ, presets, real-time speed, reverb, and a small editor for trimming and exporting clips
- **Compact layout** — slim navigation rail, slide-over drawer for now playing, lyrics and the queue
- **Custom background** — image, GIF or video, with optional glass blur on the interface; pauses while Pulse is hidden
- **Performance settings** — lightweight thumbnails, blur on/off, visualizer frame-rate cap, memory trimming in the background, hardware acceleration toggle

## Getting started

Requires Node.js 18 or newer.

```bash
cd pulse
npm install
npm start
```

## Building

```bash
npm run dist
```

Installers land in `dist/`. Builds are unsigned: on macOS, right-click Pulse in Applications and choose Open; if macOS says the app is damaged, run `xattr -cr /Applications/Pulse.app`.

## Data

- Windows: `%LOCALAPPDATA%\Pulse`
- macOS: `~/Library/Application Support/Pulse`
- Linux: `~/.cache/pulse` (settings in `~/.config/pulse`)

## Legal

Pulse is a player: it doesn't host, upload or sell music, has no ads, accounts or tracking, and nobody earns
money from it. You're responsible for using it in line with copyright and the terms of the services you use.
Rights holders can reach us by contacting the maintainers directly.
Details, also in German: [LEGAL.md](LEGAL.md).

## License

MIT. See [LICENSE](LICENSE).
