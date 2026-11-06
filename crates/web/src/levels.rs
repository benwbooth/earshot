//! The waveform's six bar heights (bass first), measured from the music
//! itself, never made up:
//!
//! - Bandcamp and Internet Archive songs play from an audio file (passed
//!   through Earshot's server), which the browser measures as it plays.
//! - YouTube and SoundCloud play in those sites' own players, which a page
//!   can't hear. Earshot's server reads the same song's audio from the site,
//!   measures it ahead (`/api/envelope`, 25 frames a second) and the bars play
//!   those levels back at the player's position, so they move with the music
//!   on any device. (Audio files use this too if the browser can't measure.)
//!   When YouTube shares only the start of a video's audio with the server
//!   (it asks for a sign-in for many videos), the server measures the same
//!   song on SoundCloud or Bandcamp instead; failing that, only the start
//!   YouTube gave moves the bars.
//! - Mixcloud doesn't let its audio be read, and a song the server couldn't
//!   read falls back to the speakers of the computer the server runs on
//!   (`/api/levels`), which only hear it in a browser on that computer.
//!
//! When nothing can be measured the bars rest, and their tooltip says why.

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
/// Frames a second in the server's measurements.
const FPS: f64 = 25.0;

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

/// One song's levels from the server, as they arrive.
struct Envelope {
    id: String,
    events: EventSource,
    /// Six bytes a frame, 0..255.
    levels: Vec<u8>,
    done: bool,
    failed: Option<String>,
    /// Why the levels end before the song does, if they do.
    cut_short: Option<String>,
    /// Where the server read the audio, when it isn't the song's own player's.
    source: Option<String>,
    _handlers: Vec<Closure<dyn Fn(MessageEvent)>>,
}

impl Drop for Envelope {
    fn drop(&mut self) {
        self.events.close();
    }
}

/// Where the song's position was last heard from its player, to follow it
/// between the players' reports (a few a second).
#[derive(Default)]
struct Clock {
    id: String,
    position: f64,
    at: f64,
}

thread_local! {
    static METER: std::cell::RefCell<Option<Meter>> = const { std::cell::RefCell::new(None) };
    static SPEAKERS: std::cell::RefCell<Option<EventSource>> = const { std::cell::RefCell::new(None) };
    static ENVELOPE: std::cell::RefCell<Option<Envelope>> = const { std::cell::RefCell::new(None) };
    static CLOCK: std::cell::RefCell<Clock> = std::cell::RefCell::new(Clock::default());
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

/// Changes what [`ENVELOPE`] holds to `id`'s levels (asking the server for
/// them), or to nothing.
fn follow(id: Option<&str>, song: Option<&serde_json::Value>) {
    let current = ENVELOPE.with(|cell| cell.borrow().as_ref().map(|e| e.id.clone()));
    if current.as_deref() == id {
        return;
    }
    ENVELOPE.with(|cell| cell.borrow_mut().take());
    let Some(id) = id else { return };
    // A YouTube video's title, artist and length, so the server can measure
    // the same song elsewhere if YouTube won't hand the video over in full.
    let mut address = format!("/api/envelope?id={}", encode(id));
    if let Some(song) = song.filter(|_| !id.contains(':')) {
        let text = |key: &str| song[key].as_str().unwrap_or("").to_owned();
        address += &format!(
            "&title={}&artist={}&duration={}",
            encode(&text("title")),
            encode(&text("artist")),
            song["duration"].as_f64().unwrap_or(0.0)
        );
    }
    let Ok(events) = EventSource::new(&address) else { return };
    let with_mine = {
        let id = id.to_owned();
        move |change: &dyn Fn(&mut Envelope)| {
            ENVELOPE.with(|cell| {
                if let Some(envelope) = cell.borrow_mut().as_mut().filter(|e| e.id == id) {
                    change(envelope);
                }
            })
        }
    };
    let on_frames = {
        let with_mine = with_mine.clone();
        Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
            let text = event.data().as_string().unwrap_or_default();
            let Some((first, data)) = text.split_once(' ') else { return };
            let Ok(first) = first.parse::<usize>() else { return };
            let Ok(bytes) = window().atob(data) else { return };
            with_mine(&|envelope| {
                // Frames in order; a reconnection starts again from the beginning.
                if first * BANDS <= envelope.levels.len() {
                    envelope.levels.truncate(first * BANDS);
                    envelope.levels.extend(bytes.chars().map(|c| c as u32 as u8));
                }
            });
        })
    };
    let on_done = {
        let with_mine = with_mine.clone();
        Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
            let text = event.data().as_string().unwrap_or_default();
            let cut_short = text.split_once(' ').map(|(_, why)| why.to_owned()).filter(|why| !why.is_empty());
            with_mine(&|envelope| {
                envelope.done = true;
                envelope.cut_short = cut_short.clone();
                envelope.events.close();
            })
        })
    };
    let on_source = {
        let with_mine = with_mine.clone();
        Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
            let source = event.data().as_string();
            with_mine(&|envelope| envelope.source = source.clone())
        })
    };
    let on_failed = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
        let reason = event.data().as_string().unwrap_or_default();
        with_mine(&|envelope| {
            envelope.done = true;
            envelope.failed = Some(reason.clone());
            envelope.events.close();
        })
    });
    for (name, handler) in [("frames", &on_frames), ("done", &on_done), ("failed", &on_failed), ("source", &on_source)] {
        let _ = events.add_event_listener_with_callback(name, handler.as_ref().unchecked_ref());
    }
    ENVELOPE.with(|cell| {
        *cell.borrow_mut() = Some(Envelope {
            id: id.to_owned(),
            events,
            levels: Vec::new(),
            done: false,
            failed: None,
            cut_short: None,
            source: None,
            _handlers: vec![on_frames, on_done, on_failed, on_source],
        })
    });
}

/// What the bars show from the server's levels at `seconds` into the song.
enum FromEnvelope {
    Levels([f64; BANDS]),
    /// Still being measured, up to this many seconds so far.
    Measuring(f64),
    Failed(String),
    /// Past the end of what was measured (this many seconds), and why it
    /// ended before the song did, if the site stopped sending its audio.
    Ended(f64, Option<String>),
    Missing,
}

fn envelope_at(seconds: f64) -> FromEnvelope {
    ENVELOPE.with(|cell| {
        let cell = cell.borrow();
        let Some(envelope) = cell.as_ref() else { return FromEnvelope::Missing };
        let frame = (seconds.max(0.0) * FPS).floor() as usize;
        if let Some(bytes) = envelope.levels.get(frame * BANDS..(frame + 1) * BANDS) {
            let mut targets = [0.0; BANDS];
            for (target, byte) in targets.iter_mut().zip(bytes) {
                *target = *byte as f64 / 255.0;
            }
            return FromEnvelope::Levels(targets);
        }
        let measured = envelope.levels.len() as f64 / BANDS as f64 / FPS;
        match (&envelope.failed, envelope.done) {
            (Some(reason), _) => FromEnvelope::Failed(reason.clone()),
            (None, true) => FromEnvelope::Ended(measured, envelope.cut_short.clone()),
            (None, false) => FromEnvelope::Measuring(measured),
        }
    })
}

/// The song's position now: the last one its player reported, moved on by
/// the time since (players report a few times a second).
fn position_now(player: &Player, playing: bool) -> f64 {
    let id = player.engine.video_id.get_untracked();
    let reported = player.engine.position.get_untracked();
    let now = js_sys::Date::now();
    CLOCK.with(|clock| {
        let mut clock = clock.borrow_mut();
        if clock.id != id || clock.position != reported {
            *clock = Clock { id, position: reported, at: now };
        } else if !playing {
            clock.at = now;
        }
        // Never run on more than a few seconds past the last report.
        reported + if playing { ((now - clock.at) / 1000.0).clamp(0.0, 3.0) } else { 0.0 }
    })
}

/// Which site's audio the server measures, for the tooltip.
fn site(id: &str) -> &'static str {
    if id.starts_with("soundcloud:") {
        "SoundCloud"
    } else if id.starts_with("mixcloud:") {
        "Mixcloud"
    } else if id.starts_with("audio:") {
        "the audio file"
    } else {
        "YouTube"
    }
}

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Rise fast, fall gently, like a VU meter (the Qt app's speeds, per 40 ms).
fn approach(levels: RwSignal<[f64; BANDS]>, targets: [f64; BANDS]) {
    levels.update(|levels| {
        for (level, target) in levels.iter_mut().zip(targets) {
            let speed = if target > *level { 0.75 } else { 0.3 };
            *level += (target - *level) * speed;
        }
    });
}

fn rest(levels: RwSignal<[f64; BANDS]>) {
    if levels.with_untracked(|l| l.iter().any(|v| *v > 0.0)) {
        levels.set([0.0; BANDS]);
    }
}

fn note(player: &Player, text: String) {
    if player.levels_note.with_untracked(|note| *note != text) {
        player.levels_note.set(text);
    }
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
        let id = player.engine.video_id.get_untracked();
        let from_file = player.engine.playing_file();
        let has_meter = METER.with(|meter| meter.borrow().is_some());
        // The browser measures audio files itself when it can; everything else
        // comes measured from the server (kept while paused, for resuming).
        let measured_here = from_file && has_meter;
        let song = player.current_untracked();
        follow((!id.is_empty() && !measured_here).then_some(id.as_str()), song.as_ref());
        let seconds = position_now(&player, playing);
        let from_envelope = if measured_here { FromEnvelope::Missing } else { envelope_at(seconds) };
        let failed = match &from_envelope {
            FromEnvelope::Failed(reason) => Some(reason.clone()),
            _ => None,
        };
        // The speakers of the server's computer, only when the song itself can't be measured.
        speakers(playing && failed.is_some(), player.levels);
        if !playing {
            rest(player.levels);
            note(&player, String::new());
            return;
        }
        if measured_here {
            if let Some(targets) = METER.with(|meter| meter.borrow_mut().as_mut().map(Meter::measure)) {
                approach(player.levels, targets);
                note(&player, "Measured from the song's audio as it plays, in this browser".to_owned());
            }
            return;
        }
        let site = site(&id);
        match from_envelope {
            FromEnvelope::Levels(targets) => {
                approach(player.levels, targets);
                note(
                    &player,
                    match ENVELOPE.with(|cell| cell.borrow().as_ref().and_then(|e| e.source.clone())) {
                        Some(source) => format!("{source} (read by Earshot's server), at the playing position"),
                        None => format!("Measured from the song's own audio (read from {site} by Earshot's server), at the playing position"),
                    },
                );
            }
            FromEnvelope::Measuring(so_far) => {
                rest(player.levels);
                note(&player, format!("Earshot's server is measuring the song's audio from {site} (done up to {}); the bars move once it reaches {}", clock(so_far), clock(seconds)));
            }
            FromEnvelope::Ended(measured, cut_short) => {
                rest(player.levels);
                note(
                    &player,
                    match cut_short {
                        Some(why) => format!("{why}, so only its first {} could be measured; the bars rest from there", clock(measured)),
                        None => format!("The audio Earshot's server read from {site} ends at {}, before this point", clock(measured)),
                    },
                );
            }
            FromEnvelope::Missing => {
                rest(player.levels);
                note(&player, "Nothing to measure: no song is loaded".to_owned());
            }
            FromEnvelope::Failed(reason) => {
                // The speakers' feed sets the bars by itself.
                let silent = player.levels.with_untracked(|l| l.iter().all(|v| *v < 0.01));
                let reason = reason.trim_end_matches('.');
                note(
                    &player,
                    if silent {
                        format!("{reason}. The speakers of the computer running Earshot are silent, so the bars rest")
                    } else {
                        format!("{reason}. Measured from the speakers of the computer running Earshot instead")
                    },
                );
            }
        }
    });
    let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 40);
    tick.forget();
}

fn encode(text: &str) -> String {
    js_sys::encode_uri_component(text).as_string().unwrap_or_default()
}
