//! The player: one thread decodes, resamples (rate and device sample rate), runs the EQ and fills
//! a buffer the output callback drains with the volume applied.
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use crate::dsp::{EqChain, EqSettings};

/// What the UI hears back.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Loaded { duration: f64 },
    Ended,
    Error(String),
}

enum Cmd {
    Load(PathBuf, f64, Option<Arc<Growing>>),
    Play,
    Pause,
    Seek(f64),
    Eq(EqSettings),
    Rate(f64),
    Stop,
}

struct Shared {
    buf: Mutex<VecDeque<f32>>,
    playing: AtomicBool,
    /// f32 bits.
    volume: AtomicU32,
    /// Output frames played since the last load or seek.
    played: AtomicU64,
    /// when `played` last moved (µs since `epoch`): the position runs on between the sound card's
    /// big callbacks instead of jumping by a whole callback (the progress bar stuttered)
    played_at: AtomicU64,
    epoch: Instant,
    /// the output said it broke (Android disconnected it: a route change, or it sat paused in the
    /// background): the audio thread opens a new one
    broken: AtomicBool,
}

/// A song still downloading into its file: how much has arrived, and whether it has all come in.
/// The downloader fills it in; the player reads up to `written` and waits for more.
#[derive(Default)]
pub struct Growing {
    pub written: AtomicU64,
    /// 0 when not known
    pub total: AtomicU64,
    pub done: AtomicBool,
    pub failed: AtomicBool,
}

/// A file that is still being written: reads past what has arrived wait for it (or end with the
/// download), so the song plays while it downloads.
struct GrowingFile {
    file: std::fs::File,
    growing: Arc<Growing>,
    pos: u64,
}

impl std::io::Read for GrowingFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::io::{Seek, SeekFrom};
        let started = Instant::now();
        loop {
            let written = self.growing.written.load(Ordering::Acquire);
            if written > self.pos {
                let n = buf.len().min((written - self.pos) as usize);
                self.file.seek(SeekFrom::Start(self.pos))?;
                let got = self.file.read(&mut buf[..n])?;
                self.pos += got as u64;
                return Ok(got);
            }
            if self.growing.done.load(Ordering::Acquire) {
                return Ok(0);
            }
            if self.growing.failed.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(60) {
                return Err(std::io::Error::other("the download stopped"));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl std::io::Seek for GrowingFile {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        let total = self.growing.total.load(Ordering::Acquire);
        let end = if total > 0 { total } else { self.growing.written.load(Ordering::Acquire) };
        let p = match to {
            std::io::SeekFrom::Start(n) => n as i64,
            std::io::SeekFrom::End(d) => end as i64 + d,
            std::io::SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if p < 0 {
            return Err(std::io::Error::other("seek before the start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

impl symphonia::core::io::MediaSource for GrowingFile {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        match self.growing.total.load(Ordering::Acquire) {
            0 => None,
            n => Some(n),
        }
    }
}

/// A handle; cheap to clone. Dropping the last one stops playback.
#[derive(Clone)]
pub struct Player {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
    state: Arc<Mutex<Clock>>,
}

#[derive(Default, Clone, Copy)]
struct Clock {
    /// Source time at the last load or seek.
    origin: f64,
    duration: f64,
    rate: f64,
    device_rate: f64,
}

impl Player {
    /// Starts the audio thread on the default output device. Events arrive on the returned receiver.
    pub fn start() -> Result<(Self, Receiver<Event>), String> {
        let (tx, rx) = mpsc::channel();
        let (etx, erx) = mpsc::channel();
        let shared = Arc::new(Shared {
            buf: Mutex::new(VecDeque::new()),
            playing: AtomicBool::new(false),
            volume: AtomicU32::new(0.85f32.to_bits()),
            played: AtomicU64::new(0),
            played_at: AtomicU64::new(0),
            epoch: Instant::now(),
            broken: AtomicBool::new(false),
        });
        let state = Arc::new(Mutex::new(Clock {
            rate: 1.0,
            ..Default::default()
        }));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (s2, st2) = (shared.clone(), state.clone());
        std::thread::Builder::new()
            .name("pulse-audio".into())
            .spawn(move || run(rx, etx, s2, st2, ready_tx))
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok((Self { tx, shared, state }, erx))
    }

    /// Opens a file and starts at `start` seconds (paused until `play`).
    pub fn load(&self, path: impl Into<PathBuf>, start: f64) {
        let _ = self.tx.send(Cmd::Load(path.into(), start, None));
    }
    /// Plays a file that is still downloading (see [`Growing`]).
    pub fn load_growing(&self, path: impl Into<PathBuf>, start: f64, growing: Arc<Growing>) {
        let _ = self.tx.send(Cmd::Load(path.into(), start, Some(growing)));
    }
    pub fn play(&self) {
        let _ = self.tx.send(Cmd::Play);
    }
    pub fn pause(&self) {
        // at once: the audio thread may be busy waiting for a download
        self.shared.playing.store(false, Ordering::Relaxed);
        let _ = self.tx.send(Cmd::Pause);
    }
    pub fn seek(&self, secs: f64) {
        let _ = self.tx.send(Cmd::Seek(secs));
    }
    pub fn set_eq(&self, eq: EqSettings) {
        let _ = self.tx.send(Cmd::Eq(eq));
    }
    /// Playback rate (speed and pitch change together, like `preservesPitch = false`).
    pub fn set_rate(&self, rate: f64) {
        let _ = self.tx.send(Cmd::Rate(rate));
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
    pub fn set_volume(&self, v: f32) {
        self.shared
            .volume
            .store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }
    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }
    /// Seconds into the song.
    pub fn position(&self) -> f64 {
        let c = *self.state.lock().unwrap_or_else(|e| e.into_inner());
        if c.device_rate <= 0.0 {
            return 0.0;
        }
        let mut pos =
            c.origin + self.shared.played.load(Ordering::Relaxed) as f64 / c.device_rate * c.rate;
        // the sound card takes audio in big chunks: between them, run on with the clock (at most
        // one chunk's worth, so a stall doesn't run ahead)
        if self.shared.playing.load(Ordering::Relaxed) && self.shared.played.load(Ordering::Relaxed) > 0 {
            let at = self.shared.played_at.load(Ordering::Relaxed);
            let now = self.shared.epoch.elapsed().as_micros() as u64;
            let since = now.saturating_sub(at) as f64 / 1e6;
            pos += since.min(0.1) * c.rate;
        }
        if c.duration > 0.0 {
            pos.min(c.duration)
        } else {
            pos
        }
    }
    pub fn duration(&self) -> f64 {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .duration
    }
}

struct Track {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    rate: f64,
    channels: usize,
}

fn open(path: &PathBuf, growing: Option<Arc<Growing>>) -> Result<(Track, f64), String> {
    // a download can finish (and its `.part` be renamed to the final name) just before this opens it
    let (file, growing) = match std::fs::File::open(path) {
        Ok(f) => (f, growing),
        Err(e) => match path.to_str().and_then(|p| p.strip_suffix(".part")) {
            Some(done) => (std::fs::File::open(done).map_err(|_| e.to_string())?, None),
            None => return Err(e.to_string()),
        },
    };
    let source: Box<dyn symphonia::core::io::MediaSource> = match growing {
        Some(g) => Box::new(GrowingFile { file, growing: g, pos: 0 }),
        None => Box::new(file),
    };
    let mss = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("unsupported audio: {e}"))?;
    let format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no audio track")?;
    let params = &track.codec_params;
    let rate = f64::from(params.sample_rate.unwrap_or(44100));
    let duration = match (params.n_frames, params.time_base) {
        (Some(n), Some(tb)) => {
            let t = tb.calc_time(n);
            t.seconds as f64 + t.frac
        }
        (Some(n), None) => n as f64 / rate,
        _ => 0.0,
    };
    let channels = params.channels.map_or(2, |c| c.count());
    let decoder = symphonia::default::get_codecs()
        .make(params, &DecoderOptions::default())
        .map_err(|e| format!("unsupported codec: {e}"))?;
    let track_id = track.id;
    Ok((
        Track {
            format,
            decoder,
            track_id,
            rate,
            channels,
        },
        duration,
    ))
}

/// Linear resampler from the source rate to the device rate, times the playback rate.
struct Resampler {
    pos: f64,
    prev: [f32; 2],
}

impl Resampler {
    fn run(&mut self, input: &[f32], step: f64, out: &mut Vec<f32>) {
        let frames = input.len() / 2;
        let at = |i: isize, ch: usize, prev: &[f32; 2]| {
            if i < 0 {
                prev[ch]
            } else {
                input[i as usize * 2 + ch]
            }
        };
        while self.pos < frames as f64 {
            let i = self.pos.floor() as isize;
            let f = (self.pos - i as f64) as f32;
            for ch in 0..2 {
                let a = at(i - 1, ch, &self.prev);
                let b = at(i, ch, &self.prev);
                out.push(a + (b - a) * f);
            }
            self.pos += step;
        }
        self.pos -= frames as f64;
        if frames > 0 {
            self.prev = [input[(frames - 1) * 2], input[(frames - 1) * 2 + 1]];
        }
    }
}

/// Opens the default output; `None` when there is none (the player then keeps time silently).
fn open_output(shared: &Arc<Shared>) -> Option<(cpal::Stream, f64)> {
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    let device_rate = f64::from(config.sample_rate());
    let out_channels = usize::from(config.channels());
    // big callbacks (~85 ms at 48 kHz): the audio system wakes the app ~12 times a second instead
    // of 50-100. Music doesn't need low latency; a device that refuses the size gets its default.
    let build = |size: cpal::BufferSize| {
        let mut c = config.config();
        c.buffer_size = size;
        let cb = shared.clone();
        device
            .build_output_stream(
                c,
                move |data: &mut [f32], _| drain(&cb, data, out_channels),
                {
                    let s = shared.clone();
                    move |e| {
                        eprintln!("[pulse-audio] output error: {e}");
                        s.broken.store(true, Ordering::Relaxed);
                    }
                },
                None,
            )
            .ok()
    };
    let stream = build(cpal::BufferSize::Fixed(4096)).or_else(|| build(cpal::BufferSize::Default))?;
    // starts paused: the stream only runs while a song plays (a running stream keeps the audio
    // hardware and the CPU awake, even when it only plays silence)
    stream.pause().ok();
    Some((stream, device_rate))
}

/// Fills an output buffer from the decoded audio, volume applied.
fn drain(cb: &Shared, data: &mut [f32], out_channels: usize) {
    let vol = f32::from_bits(cb.volume.load(Ordering::Relaxed));
    let playing = cb.playing.load(Ordering::Relaxed);
    let mut buf = cb.buf.lock().unwrap_or_else(|e| e.into_inner());
    let mut frames = 0u64;
    for frame in data.chunks_mut(out_channels.max(1)) {
        let (l, r) = if playing && buf.len() >= 2 {
            frames += 1;
            (
                buf.pop_front().unwrap_or(0.0),
                buf.pop_front().unwrap_or(0.0),
            )
        } else {
            (0.0, 0.0)
        };
        for (c, s) in frame.iter_mut().enumerate() {
            *s = if c % 2 == 0 { l } else { r } * vol;
        }
    }
    cb.played.fetch_add(frames, Ordering::Relaxed);
    if frames > 0 {
        cb.played_at.store(cb.epoch.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
}

fn run(
    rx: Receiver<Cmd>,
    events: Sender<Event>,
    shared: Arc<Shared>,
    clock: Arc<Mutex<Clock>>,
    ready: Sender<Result<(), String>>,
) {
    let output = open_output(&shared);
    let device_rate = output.as_ref().map_or(48000.0, |o| o.1);
    if output.is_none() {
        eprintln!("[pulse-audio] no audio output: playing silently");
        let s = shared.clone();
        std::thread::spawn(move || {
            let mut sink = vec![0f32; 960 * 2];
            while Arc::strong_count(&s) > 1 {
                drain(&s, &mut sink, 2);
                std::thread::sleep(Duration::from_millis(20));
            }
        });
    }
    let has_output = output.is_some();
    let mut output = output.map(|o| o.0);
    let mut device_rate = device_rate;
    // the stream runs only while playing (see open_output)
    let mut running = false;
    let mut reconnect = false;
    clock.lock().unwrap_or_else(|e| e.into_inner()).device_rate = device_rate;
    let _ = ready.send(Ok(()));

    let mut track: Option<Track> = None;
    let mut eq = EqChain::new(device_rate);
    let mut eq_settings = EqSettings::default();
    let mut rate = 1.0f64;
    let mut rs = Resampler {
        pos: 0.0,
        prev: [0.0; 2],
    };
    let mut scratch: Option<SampleBuffer<f32>> = None;
    let mut out = Vec::new();
    let target = (device_rate * 0.5) as usize * 2; // half a second buffered
    let reset = |origin: f64, shared: &Shared, clock: &Mutex<Clock>| {
        shared.buf.lock().unwrap_or_else(|e| e.into_inner()).clear();
        shared.played.store(0, Ordering::Relaxed);
        clock.lock().unwrap_or_else(|e| e.into_inner()).origin = origin;
    };
    loop {
        let need =
            track.is_some() && shared.buf.lock().unwrap_or_else(|e| e.into_inner()).len() < target;
        let cmd = if need {
            rx.try_recv()
                .map_err(|e| matches!(e, mpsc::TryRecvError::Disconnected))
        } else if shared.playing.load(Ordering::Relaxed) {
            // buffered half a second ahead: checking 10 times a second keeps it topped up
            rx.recv_timeout(Duration::from_millis(100))
                .map_err(|e| matches!(e, RecvTimeoutError::Disconnected))
        } else {
            // paused, stopped or nothing loaded: sleep until told otherwise (no polling)
            rx.recv().map_err(|_| true)
        };
        match cmd {
            Err(true) => return,
            Err(false) => {}
            Ok(Cmd::Load(path, start, growing)) => {
                shared.playing.store(false, Ordering::Relaxed);
                if running {
                    if let Some(st) = &output {
                        let _ = st.pause();
                    }
                    running = false;
                }
                match open(&path, growing) {
                    Ok((mut t, duration)) => {
                        if start > 0.0 {
                            let _ = t.format.seek(
                                SeekMode::Accurate,
                                SeekTo::Time {
                                    time: Time::from(start),
                                    track_id: Some(t.track_id),
                                },
                            );
                        }
                        reset(start.max(0.0), &shared, &clock);
                        clock.lock().unwrap_or_else(|e| e.into_inner()).duration = duration;
                        rs = Resampler {
                            pos: 0.0,
                            prev: [0.0; 2],
                        };
                        scratch = None;
                        track = Some(t);
                        let _ = events.send(Event::Loaded { duration });
                    }
                    Err(e) => {
                        track = None;
                        let _ = events.send(Event::Error(e));
                    }
                }
                continue;
            }
            Ok(Cmd::Play) => {
                shared.playing.store(true, Ordering::Relaxed);
                if !running {
                    reconnect = true;
                }
            }
            Ok(Cmd::Pause) => {
                shared.playing.store(false, Ordering::Relaxed);
                if running {
                    if let Some(st) = &output {
                        let _ = st.pause();
                    }
                    running = false;
                }
            }
            Ok(Cmd::Stop) => {
                shared.playing.store(false, Ordering::Relaxed);
                if running {
                    if let Some(st) = &output {
                        let _ = st.pause();
                    }
                    running = false;
                }
                track = None;
                reset(0.0, &shared, &clock);
            }
            Ok(Cmd::Seek(secs)) => {
                if let Some(t) = &mut track {
                    let to = secs.max(0.0);
                    let actual = t
                        .format
                        .seek(
                            SeekMode::Accurate,
                            SeekTo::Time {
                                time: Time::from(to),
                                track_id: Some(t.track_id),
                            },
                        )
                        .map(|s| s.actual_ts as f64 / t.rate)
                        .unwrap_or(to);
                    t.decoder.reset();
                    reset(
                        if (actual - to).abs() < 1.0 {
                            to
                        } else {
                            actual
                        },
                        &shared,
                        &clock,
                    );
                }
            }
            Ok(Cmd::Eq(s)) => {
                eq_settings = s;
                eq.set(s);
            }
            Ok(Cmd::Rate(r)) => {
                let r = if r.is_finite() && r > 0.0 {
                    r.clamp(0.25, 4.0)
                } else {
                    1.0
                };
                // keep the position continuous: fold what was played at the old rate into the origin
                let mut c = clock.lock().unwrap_or_else(|e| e.into_inner());
                let played = shared.played.swap(0, Ordering::Relaxed) as f64;
                c.origin += played / c.device_rate * c.rate;
                c.rate = r;
                rate = r;
            }
        }
        // a fresh output each time playback starts (one left paused in the background can be dead
        // without saying so: playing, but silent) and whenever the output reports it broke
        if shared.broken.swap(false, Ordering::Relaxed) && shared.playing.load(Ordering::Relaxed) {
            reconnect = true;
        }
        if reconnect && has_output {
            reconnect = false;
            drop(output.take());
            running = false;
            if let Some((st, new_rate)) = open_output(&shared) {
                if (new_rate - device_rate).abs() > 0.5 {
                    // another sample rate (Bluetooth often runs at 44.1 kHz): carry on from the
                    // same spot, re-decoded for the new rate
                    let pos = {
                        let mut c = clock.lock().unwrap_or_else(|e| e.into_inner());
                        let played = shared.played.swap(0, Ordering::Relaxed) as f64;
                        c.origin += played / c.device_rate * c.rate;
                        c.device_rate = new_rate;
                        c.origin
                    };
                    device_rate = new_rate;
                    eq = EqChain::new(device_rate);
                    eq.set(eq_settings);
                    shared.buf.lock().unwrap_or_else(|e| e.into_inner()).clear();
                    if let Some(t) = &mut track {
                        let _ = t.format.seek(
                            SeekMode::Accurate,
                            SeekTo::Time { time: Time::from(pos), track_id: Some(t.track_id) },
                        );
                        t.decoder.reset();
                    }
                    rs = Resampler { pos: 0.0, prev: [0.0; 2] };
                }
                if shared.playing.load(Ordering::Relaxed) && st.play().is_ok() {
                    running = true;
                }
                output = Some(st);
            } else {
                // no output at the moment: try again shortly
                std::thread::sleep(Duration::from_millis(200));
                reconnect = shared.playing.load(Ordering::Relaxed);
            }
        }
        if !need {
            continue;
        }
        let Some(t) = &mut track else { continue };
        let packet = match t.format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // let the buffer drain, then report the end
                while !shared
                    .buf
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .is_empty()
                    && shared.playing.load(Ordering::Relaxed)
                {
                    std::thread::sleep(Duration::from_millis(20));
                }
                track = None;
                shared.playing.store(false, Ordering::Relaxed);
                if running {
                    if let Some(st) = &output {
                        let _ = st.pause();
                    }
                    running = false;
                }
                let _ = events.send(Event::Ended);
                continue;
            }
            Err(SymError::ResetRequired) => {
                t.decoder.reset();
                continue;
            }
            Err(e) => {
                track = None;
                let _ = events.send(Event::Error(e.to_string()));
                continue;
            }
        };
        if packet.track_id() != t.track_id {
            continue;
        }
        let decoded = match t.decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // a damaged frame: skip it
            Err(e) => {
                track = None;
                let _ = events.send(Event::Error(e.to_string()));
                continue;
            }
        };
        let spec = *decoded.spec();
        let sb = scratch.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if sb.capacity() < decoded.capacity() * spec.channels.count() {
            *sb = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        sb.copy_interleaved_ref(decoded);
        let ch = t.channels.max(1);
        let stereo: Vec<f32> = sb
            .samples()
            .chunks(ch)
            .flat_map(|f| [f[0], *f.get(1).unwrap_or(&f[0])])
            .collect();
        out.clear();
        rs.run(&stereo, t.rate / device_rate * rate, &mut out);
        eq.process(&mut out, 2);
        shared
            .buf
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(out.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 0.5 s 1 kHz stereo 16-bit WAV.
    fn wav(path: &std::path::Path, rate: u32) {
        let n = rate / 2;
        let mut d = Vec::new();
        for i in 0..n {
            let s =
                ((i as f32 / rate as f32 * 1000.0 * std::f32::consts::TAU).sin() * 12000.0) as i16;
            d.extend_from_slice(&s.to_le_bytes());
            d.extend_from_slice(&s.to_le_bytes());
        }
        let mut f = Vec::new();
        f.extend_from_slice(b"RIFF");
        f.extend_from_slice(&(36 + d.len() as u32).to_le_bytes());
        f.extend_from_slice(b"WAVEfmt ");
        f.extend_from_slice(&[16, 0, 0, 0, 1, 0, 2, 0]);
        f.extend_from_slice(&rate.to_le_bytes());
        f.extend_from_slice(&(rate * 4).to_le_bytes());
        f.extend_from_slice(&[4, 0, 16, 0]);
        f.extend_from_slice(b"data");
        f.extend_from_slice(&(d.len() as u32).to_le_bytes());
        f.extend(d);
        std::fs::write(path, f).unwrap();
    }

    #[test]
    fn opens_and_decodes_a_file() {
        let path = std::env::temp_dir().join("pulse-audio-test.wav");
        wav(&path, 44100);
        let (mut t, duration) = open(&path, None).unwrap();
        assert!((duration - 0.5).abs() < 0.01, "{duration}");
        assert_eq!((t.rate, t.channels), (44100.0, 2));
        let mut frames = 0;
        while let Ok(p) = t.format.next_packet() {
            frames += t.decoder.decode(&p).unwrap().frames();
        }
        assert_eq!(frames, 22050);
        assert!(open(&std::env::temp_dir().join("missing.mp3"), None).is_err());
    }

    /// A song still downloading: the decoder reads what has arrived and waits for the rest.
    #[test]
    fn decodes_a_file_while_it_downloads() {
        let full = std::env::temp_dir().join("pulse-audio-full.wav");
        wav(&full, 44100);
        let bytes = std::fs::read(&full).unwrap();
        let part = std::env::temp_dir().join("pulse-audio-growing.wav.part");
        std::fs::write(&part, &bytes[..4096]).unwrap();
        let g = Arc::new(Growing::default());
        g.total.store(bytes.len() as u64, Ordering::Release);
        g.written.store(4096, Ordering::Release);
        // the rest arrives in pieces while the decoder runs
        let (g2, part2, bytes2) = (g.clone(), part.clone(), bytes.clone());
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().append(true).open(&part2).unwrap();
            for chunk in bytes2[4096..].chunks(8192) {
                std::thread::sleep(Duration::from_millis(10));
                f.write_all(chunk).unwrap();
                f.flush().unwrap();
                g2.written.fetch_add(chunk.len() as u64, Ordering::Release);
            }
            g2.done.store(true, Ordering::Release);
        });
        let (mut t, duration) = open(&part, Some(g)).unwrap();
        assert!((duration - 0.5).abs() < 0.01, "{duration}");
        let mut frames = 0;
        while let Ok(p) = t.format.next_packet() {
            frames += t.decoder.decode(&p).unwrap().frames();
        }
        writer.join().unwrap();
        assert_eq!(frames, 22050);
    }

    #[test]
    fn resampler_doubles_and_halves() {
        let input: Vec<f32> = (0..8).flat_map(|i| [i as f32, -(i as f32)]).collect();
        let mut out = Vec::new();
        Resampler {
            pos: 0.0,
            prev: [0.0; 2],
        }
        .run(&input, 0.5, &mut out);
        assert_eq!(out.len(), 32);
        let mut out = Vec::new();
        Resampler {
            pos: 0.0,
            prev: [0.0; 2],
        }
        .run(&input, 2.0, &mut out);
        assert_eq!(out.len(), 8);
    }
}
