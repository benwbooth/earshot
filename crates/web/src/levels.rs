//! The waveform's six bar heights (bass first), measured from the music
//! itself, never made up:
//!
//! - Bandcamp and Internet Archive songs play from an audio file (passed
//!   through Earshot's server), which the browser can measure directly.
//! - YouTube, SoundCloud and Mixcloud play in those sites' own players, which
//!   a page can't hear. When the browser is on the computer running Earshot's
//!   server, the server measures the speakers instead, as the Qt app does.
//! - Otherwise (a phone playing YouTube, say) the bars stay still.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{AnalyserNode, AudioContext, EventSource, MessageEvent};

use crate::engine;
use crate::player::Player;
use crate::state::window;

pub const BANDS: usize = 6;
/// Centre frequencies of the six bars, as in the Qt app.
const CENTRES: [f64; BANDS] = [70.0, 180.0, 450.0, 1100.0, 2800.0, 7000.0];

/// Why the bars aren't moving, when they can't be measured.
pub const CANT_HEAR: &str = "The waveform can't hear this site's player in a browser on this device";

/// The browser's own measuring of the audio file, made on the first click
/// (browsers only allow sound through it after one).
struct Meter {
    context: AudioContext,
    analyser: AnalyserNode,
    bins: Vec<f32>,
    peaks: [f64; BANDS],
}

impl Meter {
    fn new() -> Option<Self> {
        let context = AudioContext::new().ok()?;
        let source = context.create_media_element_source(&engine::audio_element()).ok()?;
        let analyser = context.create_analyser().ok()?;
        analyser.set_fft_size(2048);
        analyser.set_smoothing_time_constant(0.0);
        source.connect_with_audio_node(&analyser).ok()?;
        analyser.connect_with_audio_node(&context.destination()).ok()?;
        let bins = vec![0.0; analyser.frequency_bin_count() as usize];
        Some(Self { context, analyser, bins, peaks: [1e-4; BANDS] })
    }

    /// Bar targets 0..1: each band against its own loudest recent level.
    fn measure(&mut self) -> [f64; BANDS] {
        if self.context.state() != web_sys::AudioContextState::Running {
            let _ = self.context.resume();
        }
        self.analyser.get_float_frequency_data(&mut self.bins);
        let rate = self.context.sample_rate() as f64;
        let size = self.analyser.fft_size() as f64;
        let magnitude = |frequency: f64| {
            let bin = ((frequency * size / rate).round() as usize).min(self.bins.len().saturating_sub(1));
            10f64.powf(self.bins.get(bin).copied().unwrap_or(-200.0) as f64 / 20.0)
        };
        let silent = self.bins.iter().all(|db| *db < -110.0);
        let mut targets = [0.0; BANDS];
        for (band, centre) in CENTRES.iter().enumerate() {
            let level = [0.8, 1.0, 1.25].iter().map(|spread| magnitude(centre * spread)).sum::<f64>() / 3.0;
            self.peaks[band] = (self.peaks[band] * 0.99).max(level).max(1e-4);
            targets[band] = if silent { 0.0 } else { (level / self.peaks[band]).clamp(0.0, 1.0) };
        }
        targets
    }
}

thread_local! {
    static METER: std::cell::RefCell<Option<Meter>> = const { std::cell::RefCell::new(None) };
    static SPEAKERS: std::cell::RefCell<Option<EventSource>> = const { std::cell::RefCell::new(None) };
}

/// Whether this browser is on the computer running Earshot's server.
fn on_this_computer() -> bool {
    let host = window().location().hostname().unwrap_or_default();
    matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// Listens to the server's speaker measurements while needed.
fn speakers(on: bool, levels: RwSignal<[f64; BANDS]>) {
    SPEAKERS.with(|cell| {
        let mut source = cell.borrow_mut();
        if on && source.is_none() {
            if let Ok(events) = EventSource::new("/api/levels") {
                let update = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
                    let text = event.data().as_string().unwrap_or_default();
                    let mut bars = [0.0; BANDS];
                    for (bar, value) in bars.iter_mut().zip(text.split(',')) {
                        *bar = value.parse().unwrap_or(0.0);
                    }
                    levels.set(bars);
                });
                let _ = events.add_event_listener_with_callback("levels", update.as_ref().unchecked_ref());
                update.forget();
                *source = Some(events);
            }
        } else if !on {
            if let Some(events) = source.take() {
                events.close();
            }
        }
    });
}

/// Keeps `player.levels` (and `player.levels_note`) up to date.
pub fn run(player: Player) {
    // The audio file's meter needs a click first; the play button counts.
    let start = Closure::<dyn Fn()>::new(|| {
        METER.with(|meter| {
            if meter.borrow().is_none() {
                *meter.borrow_mut() = Meter::new();
            }
        });
    });
    for event in ["pointerdown", "keydown"] {
        let _ = window().add_event_listener_with_callback(event, start.as_ref().unchecked_ref());
    }
    start.forget();

    let tick = Closure::<dyn Fn()>::new(move || {
        let playing = player.engine.state.get_untracked() == 1;
        let from_file = player.engine.playing_file();
        let local = on_this_computer();
        speakers(playing && !from_file && local, player.levels);
        if !playing {
            if player.levels.with_untracked(|l| l.iter().any(|v| *v > 0.0)) {
                player.levels.set([0.0; BANDS]);
            }
            player.levels_note.set(String::new());
            return;
        }
        if from_file {
            let targets = METER.with(|meter| meter.borrow_mut().as_mut().map(Meter::measure));
            match targets {
                Some(targets) => {
                    // Rise fast, fall gently, like a VU meter (the Qt app's speeds, per 40 ms).
                    player.levels.update(|levels| {
                        for (level, target) in levels.iter_mut().zip(targets) {
                            let speed = if target > *level { 0.75 } else { 0.3 };
                            *level += (target - *level) * speed;
                        }
                    });
                    player.levels_note.set(String::new());
                }
                None => player.levels_note.set(CANT_HEAR.to_owned()),
            }
        } else if !local {
            player.levels.set([0.0; BANDS]);
            player.levels_note.set(CANT_HEAR.to_owned());
        } else {
            player.levels_note.set(String::new());
        }
    });
    let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 40);
    tick.forget();
}
