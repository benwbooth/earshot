//! The web version's waveform, measured from each song's own audio: the
//! server reads the song (YouTube's AAC, SoundCloud's stream, a Bandcamp or
//! Internet Archive MP3), decodes it and measures the same six bands the Qt
//! app measures from the speakers, [`FPS`] times a second. The browser plays
//! those levels back at the song's playing position, so the bars move with
//! the music on any device, even one whose sound the server can't hear.
//!
//! Levels are one byte per band per frame (0 = silent, 255 = the band's
//! loudest recent level), bass first. A song's levels are kept in memory for
//! a while, and given out as they're measured, so bars appear within moments
//! of a song starting.

use std::collections::VecDeque;
use std::io::{self, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as Decoding;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::{soundcloud, youtube};

pub const BANDS: usize = 6;
/// Frames of levels per second of music.
pub const FPS: u32 = 25;
/// Centre frequencies of the six bars, as in the Qt app and the browser.
const CENTRES: [f32; BANDS] = [70.0, 180.0, 450.0, 1100.0, 2800.0, 7000.0];
/// Samples measured for each frame, centred on its moment.
const WINDOW: usize = 2048;
/// Songs whose levels are kept.
const KEPT: usize = 40;
/// A song nobody has watched for this long stops being measured.
const UNWATCHED: Duration = Duration::from_secs(20);
/// A song that couldn't be measured is tried again after this long.
const RETRY: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Progress {
    levels: Vec<u8>,
    done: bool,
    error: Option<String>,
    /// Why the levels stop before the song does, if they do.
    cut_short: Option<String>,
    /// Where the audio came from, when it isn't the song's own player's
    /// ("the same song on SoundCloud").
    source: Option<String>,
}

/// One song's measuring, under way or finished.
pub struct Job {
    progress: Mutex<Progress>,
    watchers: AtomicUsize,
    unwatched_since: Mutex<Option<Instant>>,
    started: Instant,
}

/// What's new since an earlier look.
pub struct Snapshot {
    /// Levels from the byte offset asked for on (whole frames).
    pub levels: Vec<u8>,
    pub done: bool,
    pub error: Option<String>,
    /// Why the levels stop before the song does, if they do.
    pub cut_short: Option<String>,
    /// Where the audio came from, when it isn't the song's own player's.
    pub source: Option<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Job {
    fn new() -> Self {
        Self { progress: Mutex::default(), watchers: AtomicUsize::new(0), unwatched_since: Mutex::new(Some(Instant::now())), started: Instant::now() }
    }

    /// The levels from byte `offset` on, and whether that's all.
    pub fn since(&self, offset: usize) -> Snapshot {
        let progress = lock(&self.progress);
        Snapshot {
            levels: progress.levels.get(offset..).map(<[u8]>::to_vec).unwrap_or_default(),
            done: progress.done,
            error: progress.error.clone(),
            cut_short: progress.cut_short.clone(),
            source: progress.source.clone(),
        }
    }

    /// Keeps the song being measured while the returned value lives.
    pub fn watch(self: &Arc<Self>) -> Watching {
        self.watchers.fetch_add(1, Ordering::SeqCst);
        *lock(&self.unwatched_since) = None;
        Watching(self.clone())
    }

    fn abandoned(&self) -> bool {
        self.watchers.load(Ordering::SeqCst) == 0 && lock(&self.unwatched_since).is_some_and(|since| since.elapsed() > UNWATCHED)
    }

    fn add(&self, levels: &[u8]) {
        lock(&self.progress).levels.extend_from_slice(levels);
    }

    fn finish(&self, result: Result<Option<String>, String>) {
        let mut progress = lock(&self.progress);
        progress.done = true;
        match result {
            Ok(cut_short) => progress.cut_short = cut_short,
            Err(error) => progress.error = Some(error),
        }
    }
}

pub struct Watching(Arc<Job>);

impl Drop for Watching {
    fn drop(&mut self) {
        if self.0.watchers.fetch_sub(1, Ordering::SeqCst) == 1 {
            *lock(&self.0.unwatched_since) = Some(Instant::now());
        }
    }
}

fn jobs() -> &'static Mutex<VecDeque<(String, Arc<Job>)>> {
    static JOBS: OnceLock<Mutex<VecDeque<(String, Arc<Job>)>>> = OnceLock::new();
    JOBS.get_or_init(Default::default)
}

/// The levels of `id` (what the web player plays: a YouTube video id,
/// "soundcloud:<track id>", "mixcloud:<show>" or "audio:<file address>"),
/// measured already, under way, or started now. `song` (its title, artist
/// and length, or null) lets a YouTube video YouTube won't hand over in full
/// be measured from the same song on SoundCloud or Bandcamp instead.
pub fn measure(id: &str, song: &serde_json::Value) -> Arc<Job> {
    let mut known = lock(jobs());
    if let Some(index) = known.iter().position(|(key, _)| key == id) {
        let (key, job) = known.remove(index).expect("found");
        let failed = lock(&job.progress).error.is_some();
        if !(failed && job.started.elapsed() > RETRY) {
            known.push_back((key, job.clone()));
            return job;
        }
    }
    let job = Arc::new(Job::new());
    known.push_back((id.to_owned(), job.clone()));
    while known.len() > KEPT {
        known.pop_front();
    }
    drop(known);
    let (id, song, worker) = (id.to_owned(), song.clone(), job.clone());
    let started = std::thread::Builder::new().name("earshot-envelope".into()).spawn(move || {
        let result = open(&id, &song).and_then(|(reader, kind, site, source)| {
            lock(&worker.progress).source = source;
            decode(reader, kind, &site, &worker)
        });
        if worker.abandoned() {
            // Half-measured and nobody's listening: forget it, so it starts over if asked again.
            lock(jobs()).retain(|(_, job)| !Arc::ptr_eq(job, &worker));
        }
        worker.finish(result);
    });
    if let Err(error) = started {
        job.finish(Err(error.to_string()));
    }
    job
}

/// Why Mixcloud shows can't be measured.
pub const MIXCLOUD: &str = "Mixcloud keeps its shows' audio to its own player (it scrambles the address and asks apps not to download it), so Earshot can't measure it";

/// The song's audio as one stream of bytes, what kind it is, the site it's
/// from, and where that is when it isn't the song's own player's.
pub(crate) type Opened = (Parts, &'static str, String, Option<String>);

pub(crate) fn open(id: &str, song: &serde_json::Value) -> Result<Opened, String> {
    if let Some(track) = id.strip_prefix("soundcloud:") {
        let track: u64 = track.parse().map_err(|_| "Not a SoundCloud track.")?;
        let (parts, kind) = soundcloud::audio(track)?;
        return Ok((Parts::new(parts), kind, "SoundCloud".to_owned(), None));
    }
    if id.starts_with("mixcloud:") {
        return Err(MIXCLOUD.to_owned());
    }
    if let Some(url) = id.strip_prefix("audio:") {
        if !file_allowed(url) {
            return Err("Earshot only measures Bandcamp and Internet Archive audio files.".to_owned());
        }
        let kind = if url.split('?').next().unwrap_or("").to_ascii_lowercase().ends_with(".m4a") { "mp4" } else { "mp3" };
        let site = if url.contains("archive.org") { "The Internet Archive" } else { "Bandcamp" };
        return Ok((Parts::new(vec![url.to_owned()]), kind, site.to_owned(), None));
    }
    if id.len() != 11 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err("Not a YouTube video.".to_owned());
    }
    let youtube_pieces = |(url, length): (String, u64)| -> Opened {
        let pieces = (0..length)
            .step_by(youtube::AUDIO_PIECE as usize)
            .map(|start| format!("{url}&range={start}-{}", (start + youtube::AUDIO_PIECE).min(length) - 1))
            .collect();
        (Parts::new(pieces), "mp4", "YouTube".to_owned(), None)
    };
    let refused = match youtube::whole_audio(id) {
        Ok(found) => return Ok(youtube_pieces(found)),
        Err(why) => why,
    };
    // YouTube won't hand this video over in full: the same recording on
    // SoundCloud or Bandcamp has the same sound at the same moments.
    let others = if song.is_null() { Vec::new() } else { crate::api::elsewhere_all(song) };
    for other in others {
        let why = format!("{refused}, so they're measured from the same song on");
        match other["service"].as_str() {
            Some("soundcloud") => {
                if let Some(track) = other["soundcloudId"].as_u64()
                    && let Ok((parts, kind)) = soundcloud::audio(track)
                {
                    return Ok((Parts::new(parts), kind, "SoundCloud".to_owned(), Some(format!("{why} SoundCloud"))));
                }
            }
            Some("bandcamp") => {
                if let Ok(url) = crate::bandcamp::stream(&other) {
                    return Ok((Parts::new(vec![url]), "mp3", "Bandcamp".to_owned(), Some(format!("{why} Bandcamp"))));
                }
            }
            _ => {}
        }
    }
    // Neither has it (or only encrypted): the start YouTube does give is better than nothing.
    youtube::audio(id).map(youtube_pieces)
}

/// Only the sites Earshot plays audio files from.
pub fn file_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', ':']).next().unwrap_or("");
    host.ends_with(".bcbits.com") || host == "archive.org" || host.ends_with(".archive.org")
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_per_read(Some(Duration::from_secs(30)))
            .build()
            .into()
    })
}

/// Several files read one after another as one (an HLS stream's segments,
/// or a YouTube file's pieces).
pub(crate) struct Parts {
    urls: VecDeque<String>,
    current: Option<Box<dyn Read + Send + Sync>>,
}

impl Parts {
    fn new(urls: Vec<String>) -> Self {
        Self { urls: urls.into(), current: None }
    }
}

impl Read for Parts {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if let Some(reader) = self.current.as_mut() {
                let read = reader.read(buf)?;
                if read > 0 || buf.is_empty() {
                    return Ok(read);
                }
                self.current = None;
            }
            let Some(url) = self.urls.pop_front() else { return Ok(0) };
            let host = url.split("://").nth(1).and_then(|rest| rest.split('/').next()).unwrap_or("").to_owned();
            let response = agent().get(&url).call().map_err(|error| io::Error::other(format!("{host}: {error}")))?;
            self.current = Some(Box::new(response.into_body().into_reader()));
        }
    }
}

/// Decodes the audio and measures it as it arrives.
fn decode(reader: Parts, kind: &str, site: &str, job: &Job) -> Result<Option<String>, String> {
    let stream = MediaSourceStream::new(Box::new(ReadOnlySource::new(reader)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(kind);
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|error| format!("Couldn't read the song's audio: {error}"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("The song's audio has no sound in it.")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| format!("Couldn't decode the song's audio: {error}"))?;
    let mut analyser: Option<Analyser> = None;
    let mut samples: Option<SampleBuffer<f32>> = None;
    let mut cut_short = None;
    loop {
        if job.abandoned() {
            return Err("Nobody was listening any more.".to_owned());
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Decoding::IoError(error)) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(Decoding::ResetRequired) => break,
            Err(error) => {
                // Cut off part way (a link that expired, say): keep what was measured.
                if analyser.as_ref().is_some_and(|a| a.frames > 0) {
                    eprintln!("earshot: measuring the waveform stopped early: {error}");
                    cut_short = Some(format!("{site} stopped sending the song's audio part way"));
                    break;
                }
                return Err(format!("Couldn't read the song's audio: {error}"));
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Decoding::DecodeError(_)) | Err(Decoding::IoError(_)) => continue,
            Err(error) => return Err(format!("Couldn't decode the song's audio: {error}")),
        };
        let spec = *decoded.spec();
        if decoded.frames() == 0 {
            continue;
        }
        if samples.as_ref().is_none_or(|buffer| buffer.capacity() < decoded.capacity() * spec.channels.count()) {
            samples = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let Some(buffer) = samples.as_mut() else { continue };
        buffer.copy_interleaved_ref(decoded);
        let analyser = analyser.get_or_insert_with(|| Analyser::new(spec.rate as f32));
        let levels = analyser.push(buffer.samples(), spec.channels.count().max(1));
        if !levels.is_empty() {
            job.add(&levels);
        }
    }
    if analyser.is_none_or(|a| a.frames == 0) {
        return Err("The song's audio was empty.".to_owned());
    }
    Ok(cut_short)
}

/// Measures the six bands of a mono mix, [`FPS`] times a second, the way the
/// Qt app measures the speakers (Goertzel at three frequencies per band, a
/// Hann window, each band against its own loudest recent level).
struct Analyser {
    rate: f32,
    /// Mono samples not yet measured past; `base` is the first one's number
    /// (negative at first: the first frame's window starts before the song).
    mono: VecDeque<f32>,
    base: i64,
    frames: u64,
    window: Vec<f32>,
    coefficients: [[f32; 3]; BANDS],
    peaks: [f32; BANDS],
}

impl Analyser {
    fn new(rate: f32) -> Self {
        let window = (0..WINDOW).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (WINDOW - 1) as f32).cos()).collect();
        let mut coefficients = [[0.0; 3]; BANDS];
        for (band, centre) in CENTRES.iter().enumerate() {
            for (k, spread) in [0.8f32, 1.0, 1.25].iter().enumerate() {
                coefficients[band][k] = 2.0 * (2.0 * std::f32::consts::PI * centre * spread / rate).cos();
            }
        }
        let half = WINDOW / 2;
        Self { rate, mono: std::iter::repeat_n(0.0, half).collect(), base: -(half as i64), frames: 0, window, coefficients, peaks: [1e-4; BANDS] }
    }

    /// Where frame `n`'s window starts.
    fn start(&self, n: u64) -> i64 {
        (n as f64 * self.rate as f64 / FPS as f64).round() as i64 - (WINDOW / 2) as i64
    }

    /// Takes interleaved samples; gives the levels of every frame now complete.
    fn push(&mut self, interleaved: &[f32], channels: usize) -> Vec<u8> {
        for frame in interleaved.chunks(channels) {
            self.mono.push_back(frame.iter().sum::<f32>() / channels as f32);
        }
        let mut out = Vec::new();
        let mut block = vec![0.0f32; WINDOW];
        loop {
            let start = self.start(self.frames);
            let offset = (start - self.base) as usize;
            if offset + WINDOW > self.mono.len() {
                break;
            }
            for (i, slot) in block.iter_mut().enumerate() {
                *slot = self.mono[offset + i] * self.window[i];
            }
            out.extend(self.measure(&block));
            self.frames += 1;
            let next = self.start(self.frames);
            let drop = ((next - self.base).max(0) as usize).min(self.mono.len());
            self.mono.drain(..drop);
            self.base += drop as i64;
        }
        out
    }

    fn measure(&mut self, block: &[f32]) -> [u8; BANDS] {
        let energy = block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32;
        let mut levels = [0u8; BANDS];
        for band in 0..BANDS {
            let magnitude = self.coefficients[band].iter().map(|coeff| goertzel(block, *coeff)).sum::<f32>() / 3.0;
            // 0.99 per frame, as the browser's own meter fades each band's loudest level.
            self.peaks[band] = (self.peaks[band] * 0.99).max(magnitude).max(1e-4);
            let level = if energy < 1e-7 { 0.0 } else { (magnitude / self.peaks[band]).clamp(0.0, 1.0) };
            levels[band] = (level * 255.0).round() as u8;
        }
        levels
    }
}

/// Strength of one frequency in a (windowed) block.
fn goertzel(block: &[f32], coeff: f32) -> f32 {
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for sample in block {
        let s0 = sample + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_a_tone_in_its_band() {
        let rate = 44_100.0;
        let mut analyser = Analyser::new(rate);
        // One second of 450 Hz (the third bar), in stereo, then a second of silence.
        let mut samples = Vec::new();
        for i in 0..44_100 {
            let s = (2.0 * std::f32::consts::PI * 450.0 * i as f32 / rate).sin() * 0.5;
            samples.extend([s, s]);
        }
        samples.extend(std::iter::repeat_n(0.0, 2 * 44_100));
        let levels = analyser.push(&samples, 2);
        assert_eq!(levels.len() % BANDS, 0);
        let frames: Vec<&[u8]> = levels.chunks(BANDS).collect();
        // About 25 frames a second, minus the last half window.
        assert!(frames.len() >= 48 && frames.len() <= 50, "{} frames", frames.len());
        let middle = frames[12];
        assert_eq!(middle[2], 255, "{middle:?}");
        assert!(middle[0] < 255 && middle[5] < 255, "{middle:?}");
        assert!(frames[45].iter().all(|v| *v == 0), "{:?}", frames[45]);
    }

    #[test]
    fn mixcloud_says_why() {
        let job = measure("mixcloud:/someone/a-show/", &serde_json::Value::Null);
        let _watching = job.watch();
        for _ in 0..50 {
            if job.since(0).done {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(job.since(0).error.as_deref(), Some(MIXCLOUD));
    }

    fn measured(id: &str) -> Vec<u8> {
        let job = measure(id, &serde_json::Value::Null);
        let _watching = job.watch();
        let begun = Instant::now();
        while !job.since(0).done && begun.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let snapshot = job.since(0);
        assert_eq!(snapshot.error, None);
        snapshot.levels
    }

    #[test]
    #[ignore = "network"]
    fn live_youtube() {
        let levels = measured("dQw4w9WgXcQ");
        let frames = levels.len() / BANDS;
        // 3:33 long.
        assert!((210 * 25..215 * 25).contains(&frames), "{frames} frames");
        assert!(levels.iter().filter(|v| **v > 100).count() > levels.len() / 4);
    }

    #[test]
    #[ignore = "network"]
    fn live_soundcloud() {
        let found = soundcloud::search("lofi").unwrap();
        let id = found["tracks"][0]["soundcloudId"].as_u64().unwrap();
        let levels = measured(&format!("soundcloud:{id}"));
        assert!(levels.len() / BANDS > 25 * 30, "{} frames", levels.len() / BANDS);
    }
}

/// Decodes a whole audio file (`kind`: "mp3" or "mp4") to 16-bit stereo
/// samples, handing `each` the rate and a run of interleaved samples at a
/// time: for turning an AAC stream into a plain MP3 that any browser plays
/// (see the server's SoundCloud audio) without holding all of it decoded.
pub fn decode_stereo(bytes: Vec<u8>, kind: &str, each: impl FnMut(u32, &[i16]) -> Result<(), String>) -> Result<(), String> {
    decode_stream(MediaSourceStream::new(Box::new(io::Cursor::new(bytes)), Default::default()), kind, each)
}

/// [`decode_stereo`] reading as the bytes arrive (a download under way).
pub fn decode_stereo_from<R: Read + Send + Sync + 'static>(reader: R, kind: &str, each: impl FnMut(u32, &[i16]) -> Result<(), String>) -> Result<(), String> {
    decode_stream(MediaSourceStream::new(Box::new(ReadOnlySource::new(reader)), Default::default()), kind, each)
}

fn decode_stream(stream: MediaSourceStream, kind: &str, mut each: impl FnMut(u32, &[i16]) -> Result<(), String>) -> Result<(), String> {
    let mut hint = Hint::new();
    hint.with_extension(kind);
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|error| format!("Couldn't read the song's audio: {error}"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("The song's audio has no sound in it.")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| format!("Couldn't decode the song's audio: {error}"))?;
    let mut out: Vec<i16> = Vec::new();
    let mut any = false;
    let mut samples: Option<SampleBuffer<i16>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Decoding::IoError(error)) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(Decoding::ResetRequired) => break,
            Err(error) if any => {
                eprintln!("earshot: decoding stopped early: {error}");
                break;
            }
            Err(error) => return Err(format!("Couldn't read the song's audio: {error}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Decoding::DecodeError(_)) | Err(Decoding::IoError(_)) => continue,
            Err(error) => return Err(format!("Couldn't decode the song's audio: {error}")),
        };
        let spec = *decoded.spec();
        if decoded.frames() == 0 {
            continue;
        }
        let channels = spec.channels.count().max(1);
        if samples.as_ref().is_none_or(|buffer| buffer.capacity() < decoded.capacity() * channels) {
            samples = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let Some(buffer) = samples.as_mut() else { continue };
        buffer.copy_interleaved_ref(decoded);
        out.clear();
        for frame in buffer.samples().chunks(channels) {
            let left = frame[0];
            let right = if channels > 1 { frame[1] } else { left };
            out.push(left);
            out.push(right);
        }
        each(spec.rate, &out)?;
        any = true;
    }
    if !any {
        return Err("The song's audio was empty.".to_owned());
    }
    Ok(())
}
