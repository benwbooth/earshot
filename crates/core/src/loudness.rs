//! Volume leveling: how loud each song is, so the players can turn loud
//! songs down and quiet ones up until they all sound about as loud.
//!
//! The players are YouTube's, SoundCloud's and Mixcloud's own (and a plain
//! audio element for Bandcamp and the Internet Archive), so the sound itself
//! can't be processed; instead each song gets a gain in decibels that the
//! interfaces fold into the volume they give the player.
//!
//! - YouTube measures every video itself and says so in its player details
//!   (the loudness in LUFS, as ITU-R BS.1770 defines it). YouTube's players
//!   already turn videos louder than -14 LUFS down to that, so only the rest
//!   of the way to [`TARGET`] is left to do.
//! - SoundCloud, Bandcamp and Internet Archive songs are read and measured
//!   here (BS.1770: K-weighting, 400 ms blocks, the -70 LUFS and -10 LU
//!   gates), from the first [`MEASURED`] seconds.
//! - Mixcloud's audio can't be read, and a song that couldn't be measured is
//!   taken to be as loud as a typical YouTube video is after YouTube's
//!   leveling, so it isn't much louder or quieter than the songs around it.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use serde_json::{Value, json};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as Decoding;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::envelope;
use crate::net::post_json;

/// How loud every song is made to sound, in LUFS. Below YouTube's -14 so
/// quiet songs can be turned up while the volume slider isn't all the way up.
pub const TARGET: f64 = -16.0;
/// What YouTube's players turn louder videos down to.
const YOUTUBE_CEILING: f64 = -14.0;
/// Seconds of a song measured (enough to know how loud it is, and quick).
const MEASURED: f64 = 240.0;
/// Songs whose loudness is remembered.
const KEPT: usize = 2000;

type Measured = Result<f64, String>;
/// One song's loudness: None until it's known.
type Slot = Arc<Mutex<Option<Measured>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Each song's loudness, once known. A second ask while it's being measured
/// waits for the first one's answer instead of measuring again.
fn known() -> &'static Mutex<VecDeque<(String, Slot)>> {
    static KNOWN: OnceLock<Mutex<VecDeque<(String, Slot)>>> = OnceLock::new();
    KNOWN.get_or_init(Default::default)
}

/// How to level `id` (what the player plays: a YouTube video id,
/// "soundcloud:<track id>", "mixcloud:<show>" or "audio:<file address>"):
/// `{gain}` in decibels, the song's `lufs` (null if unknown) and where that
/// came from. `song` (title, artist, length) lets a YouTube video YouTube
/// says nothing about be measured from the same song elsewhere.
pub fn level(id: &str, song: &Value) -> Value {
    let youtube = is_youtube(id);
    let (lufs, how) = match loudness(id, song) {
        Ok(lufs) => (Some(lufs), if youtube { "youtube" } else { "measured" }),
        Err(why) => {
            eprintln!("earshot: loudness of {id}: {why}");
            (None, "guessed")
        }
    };
    // `raw`: for a YouTube video's audio played as it is, not by YouTube's
    // player (the web version's way round videos kept on youtube.com).
    let rounded = lufs.map(|l| (l * 10.0).round() / 10.0);
    json!({ "gain": gain(youtube, lufs), "raw": gain(false, lufs), "lufs": rounded, "how": how })
}

/// The gain in decibels that brings a song of `lufs` to [`TARGET`].
fn gain(youtube: bool, lufs: Option<f64>) -> f64 {
    let heard = match lufs {
        // YouTube has already turned it down to its ceiling.
        Some(lufs) if youtube => lufs.min(YOUTUBE_CEILING),
        Some(lufs) => lufs,
        None => YOUTUBE_CEILING,
    };
    ((TARGET - heard).clamp(-24.0, 12.0) * 10.0).round() / 10.0
}

fn is_youtube(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The song's loudness in LUFS, remembered after the first time.
pub fn loudness(id: &str, song: &Value) -> Measured {
    let slot = {
        let mut known = lock(known());
        match known.iter().position(|(key, _)| key == id) {
            Some(index) => {
                let entry = known.remove(index).expect("found");
                let slot = entry.1.clone();
                known.push_back(entry);
                slot
            }
            None => {
                let slot = Arc::new(Mutex::new(None));
                known.push_back((id.to_owned(), slot.clone()));
                while known.len() > KEPT {
                    known.pop_front();
                }
                slot
            }
        }
    };
    let mut answer = lock(&slot);
    if let Some(answer) = answer.as_ref() {
        return answer.clone();
    }
    let found = find(id, song);
    if found.is_ok() {
        *answer = Some(found.clone());
    }
    found
}

fn find(id: &str, song: &Value) -> Measured {
    if id.starts_with("mixcloud:") {
        return Err(envelope::MIXCLOUD.to_owned());
    }
    if is_youtube(id) {
        match youtube_loudness(id) {
            Ok(lufs) => return Ok(lufs),
            // Measured from the same song elsewhere, or YouTube's start of it.
            Err(why) => eprintln!("earshot: YouTube's loudness of {id}: {why}; measuring it instead"),
        }
    }
    let (reader, kind, _site, _source) = envelope::open(id, song)?;
    measure(reader, kind)
}

/// YouTube's own measurement of a video, from its player details.
fn youtube_loudness(video: &str) -> Measured {
    // The clients YouTube answers without a sign-in (as in youtube::audio).
    let clients = [
        json!({
            "clientName": "ANDROID_VR", "clientVersion": "1.62.27", "hl": "en", "gl": "US",
            "androidSdkVersion": 32, "deviceMake": "Oculus", "deviceModel": "Quest 3", "osName": "Android", "osVersion": "12L",
        }),
        json!({
            "clientName": "IOS", "clientVersion": "20.10.4", "hl": "en", "gl": "US",
            "deviceMake": "Apple", "deviceModel": "iPhone16,2", "osName": "iPhone", "osVersion": "18.3.2.22D82",
        }),
    ];
    for client in clients {
        let body = json!({ "videoId": video, "contentCheckOk": true, "racyCheckOk": true, "context": { "client": client } });
        let Ok(answer) = post_json("https://www.youtube.com/youtubei/v1/player?prettyPrint=false", &body) else { continue };
        let config = &answer["playerConfig"]["audioConfig"];
        if let Some(lufs) = config["trackAbsoluteLoudnessLkfs"].as_f64().or_else(|| config["perceptualLoudnessDb"].as_f64()) {
            return Ok(lufs);
        }
        // How far above YouTube's target (-14 LUFS) it is.
        if let Some(above) = config["loudnessDb"].as_f64() {
            return Ok(config["loudnessTargetLkfs"].as_f64().unwrap_or(YOUTUBE_CEILING) + above);
        }
    }
    Err("YouTube didn't say how loud it is".to_owned())
}

/// Reads and decodes the audio, and measures its first [`MEASURED`] seconds.
fn measure(reader: envelope::Parts, kind: &str) -> Measured {
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
    let mut meter: Option<Meter> = None;
    let mut samples: Option<SampleBuffer<f32>> = None;
    loop {
        if meter.as_ref().is_some_and(|m| m.seconds() >= MEASURED) {
            break;
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Decoding::ResetRequired) => break,
            // The end, or cut off part way: what was measured will do.
            Err(error) => {
                if meter.as_ref().is_some_and(|m| m.seconds() >= 10.0) {
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
        let channels = spec.channels.count().max(1);
        meter.get_or_insert_with(|| Meter::new(spec.rate as f64, channels)).push(buffer.samples());
    }
    meter.ok_or("The song's audio was empty.")?.integrated().ok_or_else(|| "The song is silent.".to_owned())
}

/// One biquad filter, run on one channel.
#[derive(Clone)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 3],
    state: [f64; 2],
}

impl Biquad {
    fn run(&mut self, x: f64) -> f64 {
        // Transposed direct form II.
        let y = self.b[0] * x + self.state[0];
        self.state[0] = self.b[1] * x - self.a[1] * y + self.state[1];
        self.state[1] = self.b[2] * x - self.a[2] * y;
        y
    }
}

/// BS.1770's loudness meter: K-weighting (a high shelf, then a high pass)
/// at any sample rate, mean squares over 100 ms steps, gated 400 ms blocks.
struct Meter {
    rate: f64,
    channels: usize,
    filters: Vec<(Biquad, Biquad)>,
    step: usize,
    in_step: usize,
    energy: f64,
    steps: Vec<f64>,
    frames: u64,
}

impl Meter {
    fn new(rate: f64, channels: usize) -> Self {
        let pi = std::f64::consts::PI;
        // The shelf (the head's effect), as libebur128 derives it for any rate.
        let (f0, g, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (pi * f0 / rate).tan();
        let vh = 10f64.powf(g / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        let shelf = Biquad {
            b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
            a: [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            state: [0.0; 2],
        };
        // The high pass (RLB weighting).
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (pi * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        let pass = Biquad { b: [1.0, -2.0, 1.0], a: [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0], state: [0.0; 2] };
        Self {
            rate,
            channels,
            filters: vec![(shelf, pass); channels],
            step: ((rate / 10.0).round() as usize).max(1),
            in_step: 0,
            energy: 0.0,
            steps: Vec::new(),
            frames: 0,
        }
    }

    fn seconds(&self) -> f64 {
        self.frames as f64 / self.rate
    }

    fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(self.channels) {
            for (sample, (shelf, pass)) in frame.iter().zip(self.filters.iter_mut()) {
                let weighted = pass.run(shelf.run(*sample as f64));
                // Surround channels aren't weighted apart: songs are stereo.
                self.energy += weighted * weighted;
            }
            self.frames += 1;
            self.in_step += 1;
            if self.in_step == self.step {
                self.steps.push(self.energy / self.step as f64);
                self.energy = 0.0;
                self.in_step = 0;
            }
        }
    }

    /// The gated loudness in LUFS, or None if it's all below the gates.
    fn integrated(&self) -> Option<f64> {
        let lufs = |energy: f64| -0.691 + 10.0 * energy.log10();
        let blocks: Vec<f64> = self.steps.windows(4).map(|w| w.iter().sum::<f64>() / 4.0).filter(|e| *e > 0.0 && lufs(*e) > -70.0).collect();
        if blocks.is_empty() {
            return None;
        }
        let relative = lufs(blocks.iter().sum::<f64>() / blocks.len() as f64) - 10.0;
        let kept: Vec<f64> = blocks.into_iter().filter(|e| lufs(*e) > relative).collect();
        (!kept.is_empty()).then(|| lufs(kept.iter().sum::<f64>() / kept.len() as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1 kHz sine at -20 dBFS in both channels reads about -20 LUFS
    /// (BS.1770's own check: K-weighting is about flat there, two channels add 3 dB).
    #[test]
    fn sine_reads_as_bs1770_says() {
        let rate = 48000.0;
        let amplitude = 10f64.powf(-20.0 / 20.0);
        let mut meter = Meter::new(rate, 2);
        let samples: Vec<f32> = (0..(rate as usize * 5))
            .flat_map(|n| {
                let s = (amplitude * (2.0 * std::f64::consts::PI * 997.0 * n as f64 / rate).sin()) as f32;
                [s, s]
            })
            .collect();
        meter.push(&samples);
        let lufs = meter.integrated().expect("loud enough");
        assert!((lufs - -20.0).abs() < 0.2, "{lufs}");
    }

    #[test]
    fn gains() {
        // YouTube already turned a loud video down to -14.
        assert_eq!(gain(true, Some(-8.0)), -2.0);
        assert_eq!(gain(true, Some(-22.0)), 6.0);
        assert_eq!(gain(false, Some(-8.0)), -8.0);
        assert_eq!(gain(false, None), -2.0);
    }

    #[test]
    #[ignore = "reads YouTube"]
    fn youtube_says() {
        let lufs = youtube_loudness("dQw4w9WgXcQ").expect("YouTube answers");
        assert!((-20.0..-8.0).contains(&lufs), "{lufs}");
    }

    #[test]
    #[ignore = "reads SoundCloud"]
    fn soundcloud_measured() {
        let id = std::env::var("EARSHOT_TEST_SC").unwrap_or_else(|_| "soundcloud:293".into());
        let lufs = loudness(&id, &Value::Null).expect("measured");
        eprintln!("{id}: {lufs:.1} LUFS");
        assert!((-30.0..-3.0).contains(&lufs), "{lufs}");
    }
}
