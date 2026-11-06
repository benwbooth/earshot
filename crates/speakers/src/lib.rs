//! The real waveform: listens to what the speakers are playing (through the
//! cpal audio library, on Windows, macOS and Linux) while music plays, and
//! measures six frequency bands so the bars move with the actual song.
//! YouTube's player is a sealed web page, so its sound can't be read from
//! inside the app; the speaker output can. The Qt app and the web version's
//! server (for browsers on this computer) share one recording.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const BANDS: usize = 6;
/// Centre frequencies of the six bars, bass to treble.
const CENTRES: [f32; BANDS] = [70.0, 180.0, 450.0, 1100.0, 2800.0, 7000.0];
const BLOCK: usize = 1024;

#[derive(Default)]
struct Shared {
    levels: [f32; BANDS],
}

struct Analyser {
    rate: f32,
    mono: Vec<f32>,
    peaks: [f32; BANDS],
    smoothed: [f32; BANDS],
}

impl Analyser {
    fn new(rate: f32) -> Self {
        Self { rate, mono: Vec::with_capacity(BLOCK), peaks: [1e-4; BANDS], smoothed: [0.0; BANDS] }
    }

    /// Strength of one frequency in the block (Goertzel algorithm).
    fn magnitude(samples: &[f32], frequency: f32, rate: f32) -> f32 {
        let omega = 2.0 * std::f32::consts::PI * frequency / rate;
        let coeff = 2.0 * omega.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        let last = (samples.len().max(2) - 1) as f32;
        for (i, sample) in samples.iter().enumerate() {
            // A Hann window keeps neighbouring frequencies from leaking in.
            let window = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / last).cos();
            let s0 = sample * window + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt()
    }

    fn push(&mut self, frames: impl Iterator<Item = f32>, shared: &Mutex<Shared>) {
        for sample in frames {
            self.mono.push(sample);
            if self.mono.len() < BLOCK {
                continue;
            }
            let energy = self.mono.iter().map(|s| s * s).sum::<f32>() / BLOCK as f32;
            let mut levels = [0.0f32; BANDS];
            for (band, centre) in CENTRES.iter().enumerate() {
                // Average a few neighbouring frequencies so each bar covers a band.
                let magnitude = [0.8f32, 1.0, 1.25]
                    .iter()
                    .map(|spread| Self::magnitude(&self.mono, centre * spread, self.rate))
                    .sum::<f32>()
                    / 3.0;
                // Each bar adapts to its own loudest recent level.
                self.peaks[band] = (self.peaks[band] * 0.995).max(magnitude).max(1e-4);
                let target = if energy < 1e-7 { 0.0 } else { (magnitude / self.peaks[band]).clamp(0.0, 1.0) };
                // Rise fast, fall gently, like a VU meter.
                let speed = if target > self.smoothed[band] { 0.6 } else { 0.18 };
                self.smoothed[band] += (target - self.smoothed[band]) * speed;
                levels[band] = self.smoothed[band];
            }
            shared.lock().unwrap().levels = levels;
            self.mono.clear();
        }
    }
}

fn shared() -> &'static Arc<Mutex<Shared>> {
    static SHARED: OnceLock<Arc<Mutex<Shared>>> = OnceLock::new();
    SHARED.get_or_init(Default::default)
}

/// The current bar heights, 0..1, bass first.
pub fn levels() -> [f32; BANDS] {
    shared().lock().unwrap().levels
}

fn commands() -> &'static Mutex<Option<mpsc::Sender<bool>>> {
    static COMMANDS: OnceLock<Mutex<Option<mpsc::Sender<bool>>>> = OnceLock::new();
    COMMANDS.get_or_init(Default::default)
}

static APP_WANTS: AtomicBool = AtomicBool::new(false);
static LISTENERS: AtomicUsize = AtomicUsize::new(0);

/// The Qt app's say: listen while its music plays. The recording stays on
/// while the app or any web listener ([`listen`]) wants it.
pub fn set_active(active: bool) {
    APP_WANTS.store(active, Ordering::SeqCst);
    update();
}

/// Keeps the recording on until the returned value is dropped (a browser
/// on this computer showing the waveform).
pub fn listen() -> Listening {
    LISTENERS.fetch_add(1, Ordering::SeqCst);
    update();
    Listening
}

pub struct Listening;

impl Drop for Listening {
    fn drop(&mut self) {
        LISTENERS.fetch_sub(1, Ordering::SeqCst);
        update();
    }
}

fn update() {
    switch(APP_WANTS.load(Ordering::SeqCst) || LISTENERS.load(Ordering::SeqCst) > 0);
}

/// Turns the recording on or off. Off closes it entirely, so the desktop
/// stops showing it.
fn switch(active: bool) {
    let mut guard = commands().lock().unwrap();
    if guard.is_none() {
        if !active {
            return;
        }
        let (tx, rx) = mpsc::channel::<bool>();
        *guard = Some(tx);
        // Audio streams can't move between threads on every platform, so one
        // thread owns the stream for its whole life.
        std::thread::spawn(move || {
            let mut stream: Option<cpal::Stream> = None;
            while let Ok(on) = rx.recv() {
                if on && stream.is_none() {
                    match open() {
                        Ok(opened) => stream = Some(opened),
                        Err(error) => eprintln!("earshot: the waveform can't listen to the speakers: {error}"),
                    }
                } else if !on {
                    stream = None;
                    shared().lock().unwrap().levels = [0.0; BANDS];
                }
            }
        });
    }
    if let Some(tx) = guard.as_ref() {
        let _ = tx.send(active);
    }
}

/// Records what the default speakers play: opening an output device for input
/// is loopback recording on Windows (WASAPI) and macOS 14.6+ (CoreAudio), and
/// the speaker monitor on Linux (PipeWire).
fn open() -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no speakers found")?;
    let config: cpal::StreamConfig = device.default_output_config().map_err(|error| error.to_string())?.config();
    let channels = config.channels.max(1) as usize;
    let mut analyser = Analyser::new(config.sample_rate as f32);
    let state = shared().clone();
    let stream = device
        .build_input_stream::<f32, _, _>(
            config,
            move |samples: &[f32], _| {
                let frames = samples.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32);
                analyser.push(frames, &state);
            },
            |error| eprintln!("earshot: waveform stream error: {error}"),
            None,
        )
        .map_err(|error| error.to_string())?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_loud_band() {
        let rate = 48000.0;
        let tone: Vec<f32> = (0..BLOCK).map(|i| (2.0 * std::f32::consts::PI * 1100.0 * i as f32 / rate).sin()).collect();
        let at = |f| Analyser::magnitude(&tone, f, rate);
        assert!(at(1100.0) > 10.0 * at(70.0));
        assert!(at(1100.0) > 10.0 * at(7000.0));
    }

    #[test]
    fn silence_gives_flat_bars() {
        let shared = Mutex::new(Shared::default());
        let mut analyser = Analyser::new(48000.0);
        analyser.push(std::iter::repeat_n(0.0, BLOCK * 2), &shared);
        assert!(shared.lock().unwrap().levels.iter().all(|level| *level == 0.0));
    }
}
