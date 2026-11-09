//! Plays music the way the Qt app does, through each site's own player kept
//! out of sight: YouTube's embedded player, Bandcamp's and the Internet
//! Archive's audio files, and SoundCloud's and Mixcloud's widgets. The players
//! are driven with the messages their own JavaScript libraries send, so the
//! page runs no JavaScript of its own.

use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlAudioElement, HtmlIFrameElement, MessageEvent};

use crate::state::window;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    None,
    YouTube,
    Audio,
    SoundCloud,
    Mixcloud,
}

/// YouTube's states, used for every player: -1 unstarted, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued.
#[derive(Clone, Copy)]
pub struct Engine {
    pub state: RwSignal<i32>,
    pub position: RwSignal<f64>,
    pub duration: RwSignal<f64>,
    /// The YouTube video loaded, or "audio:…", "soundcloud:…", "mixcloud:…".
    pub video_id: RwSignal<String>,
    /// Counts up each time a song finishes.
    pub ended: RwSignal<u32>,
    /// The last error: (code, message), counted so the same one twice still shows.
    pub failure: RwSignal<Option<(u32, i32, String)>>,
    mode: StoredValue<Mode>,
    volume: StoredValue<f64>,
    yt_ready: StoredValue<bool>,
    yt_pending: StoredValue<Option<(String, f64)>>,
    start: StoredValue<f64>,
    loaded_at: StoredValue<f64>,
    mc_method: StoredValue<u32>,
    /// The video Earshot's server plays on youtube.com's own page and streams
    /// here (see `load_relay`), and where in it the stream began.
    relay: StoredValue<Option<String>>,
    offset: StoredValue<f64>,
    relay_count: StoredValue<u32>,
}

fn now() -> f64 {
    js_sys::Date::now()
}

fn host() -> web_sys::Element {
    let document = window().document().expect("document");
    if let Some(host) = document.get_element_by_id("engine") {
        return host;
    }
    let host = document.create_element("div").expect("div");
    host.set_id("engine");
    document.body().expect("body").append_child(&host).expect("engine host");
    host
}

fn frame(id: &str) -> Option<HtmlIFrameElement> {
    window().document()?.get_element_by_id(id)?.dyn_into().ok()
}

fn post(id: &str, message: &Value, origin: &str) {
    if let Some(target) = frame(id).and_then(|f| f.content_window()) {
        let _ = target.post_message(&JsValue::from_str(&message.to_string()), origin);
    }
}

/// The element Bandcamp and Internet Archive songs play in.
pub fn audio_element() -> HtmlAudioElement {
    audio()
}

fn audio() -> HtmlAudioElement {
    thread_local! {
        static AUDIO: HtmlAudioElement = HtmlAudioElement::new().expect("audio");
    }
    AUDIO.with(|a| a.clone())
}

/// Whether the page can set how loud it plays. iPhones and iPads can't:
/// only the side buttons change it there. (Newer iOS reads back the volume a
/// page sets while ignoring it, so iOS is known by name; the probe is for
/// other browsers that can't.)
pub fn volume_works() -> bool {
    thread_local! {
        static WORKS: bool = !is_ios() && HtmlAudioElement::new().map(|probe| {
            probe.set_volume(0.5);
            (probe.volume() - 0.5).abs() < 0.01
        }).unwrap_or(true);
    }
    WORKS.with(|works| *works)
}

/// iPhones and iPads (iPadOS calls itself a Mac, but has a touch screen).
/// Their browsers only let a page start sound from a tap, so YouTube's
/// embedded player, started after the song is looked up, never plays there.
pub fn is_ios() -> bool {
    let navigator = window().navigator();
    let agent = navigator.user_agent().unwrap_or_default();
    agent.contains("iPhone") || agent.contains("iPad") || agent.contains("iPod") || (agent.contains("Macintosh") && navigator.max_touch_points() > 1)
}

/// A moment of silence, to play once in the first tap: after that, iOS lets
/// the page start the audio element itself (later songs start after a lookup).
const SILENCE: &str = "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQAAAAA=";

fn origin() -> String {
    window().location().origin().unwrap_or_default()
}

pub fn describe(code: i32) -> String {
    match code {
        2 => "YouTube didn't recognise this video.".into(),
        5 => "YouTube's player couldn't play this video.".into(),
        100 => "This video was removed or made private.".into(),
        101 | 150 | 153 => "This video's owner doesn't allow it to play outside youtube.com.".into(),
        1001 => "The song's audio stopped loading.".into(),
        1004 => "YouTube's player didn't start.".into(),
        1002 => "SoundCloud's player couldn't play this song.".into(),
        1003 => "Mixcloud's player couldn't play this show.".into(),
        other => format!("YouTube's player stopped (error {other})."),
    }
}

impl Engine {
    pub fn new() -> Self {
        let engine = Self {
            state: RwSignal::new(-1),
            position: RwSignal::new(0.0),
            duration: RwSignal::new(0.0),
            video_id: RwSignal::new(String::new()),
            ended: RwSignal::new(0),
            failure: RwSignal::new(None),
            mode: StoredValue::new(Mode::None),
            volume: StoredValue::new(80.0),
            yt_ready: StoredValue::new(false),
            yt_pending: StoredValue::new(None),
            start: StoredValue::new(0.0),
            loaded_at: StoredValue::new(0.0),
            mc_method: StoredValue::new(0),
            relay: StoredValue::new(None),
            offset: StoredValue::new(0.0),
            relay_count: StoredValue::new(0),
        };
        engine.listen();
        engine
    }

    /// Whether an audio file (Bandcamp, the Internet Archive) is what plays.
    pub fn playing_file(&self) -> bool {
        self.mode.get_value() == Mode::Audio
    }

    /// Whether the song plays through Earshot's server from youtube.com's own page.
    pub fn relaying(&self) -> bool {
        self.mode.get_value() == Mode::Audio && self.relay.with_value(Option::is_some)
    }

    pub fn playing(&self) -> bool {
        self.state.get() == 1
    }

    fn fail(&self, code: i32) {
        let count = self.failure.with_untracked(|f| f.as_ref().map(|(n, _, _)| n + 1).unwrap_or(1));
        self.failure.set(Some((count, code, describe(code))));
    }

    fn finish(&self) {
        self.ended.update(|n| *n += 1);
    }

    /// Messages from the players, and a clock for the plain audio file.
    fn listen(self) {
        let on_message = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(text) = event.data().as_string() else { return };
            let Ok(data) = serde_json::from_str::<Value>(&text) else { return };
            let from = event.origin();
            if from.contains("youtube.com") || from.contains("youtube-nocookie.com") {
                self.on_youtube(&data);
            } else if from.contains("soundcloud.com") {
                self.on_soundcloud(&data);
            } else if from.contains("mixcloud.com") {
                self.on_mixcloud(&data);
            }
        });
        let _ = window().add_event_listener_with_callback("message", on_message.as_ref().unchecked_ref());
        on_message.forget();

        let element = audio();
        element.set_preload("auto");
        // iOS: unlock the audio element in the first tap (see SILENCE).
        if is_ios() {
            let unlock = Closure::<dyn Fn()>::new(move || {
                let a = audio();
                if a.src().is_empty() {
                    a.set_src(SILENCE);
                    let _ = a.play();
                    let _ = a.pause();
                    let _ = a.remove_attribute("src");
                    a.load();
                }
            });
            let once = web_sys::AddEventListenerOptions::new();
            once.set_once(true);
            for event in ["touchend", "pointerup", "click"] {
                let _ = window().add_event_listener_with_callback_and_add_event_listener_options(event, unlock.as_ref().unchecked_ref(), &once);
            }
            unlock.forget();
        }
        let on_ended = Closure::<dyn Fn()>::new(move || {
            if self.mode.get_value() == Mode::Audio {
                self.state.set(0);
                self.finish();
            }
        });
        let _ = element.add_event_listener_with_callback("ended", on_ended.as_ref().unchecked_ref());
        on_ended.forget();
        let on_error = Closure::<dyn Fn()>::new(move || {
            if self.mode.get_value() == Mode::Audio && !audio().src().is_empty() {
                self.fail(1001);
            }
        });
        let _ = element.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref());
        on_error.forget();

        let tick = Closure::<dyn Fn()>::new(move || self.tick());
        let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 250);
        tick.forget();
    }

    fn tick(&self) {
        match self.mode.get_value() {
            Mode::Audio => {
                let a = audio();
                if a.src().is_empty() {
                    return;
                }
                let state = if a.ended() { 0 } else if a.paused() { 2 } else if a.ready_state() < 3 { 3 } else { 1 };
                self.state.set(state);
                // Still loading after 30 seconds: give up on this way of playing it.
                if state == 3 && a.current_time() == 0.0 && now() - self.loaded_at.get_value() > 30_000.0 {
                    self.loaded_at.set_value(now());
                    self.fail(1001);
                }
                // A relayed stream starts where it was asked to; its own length isn't the video's.
                self.position.set(self.offset.get_value() + a.current_time());
                if self.relay.with_value(Option::is_none) {
                    // YouTube's audio files don't always say how long they are,
                    // and Safari then reports forever, or twice the length:
                    // how far it can seek is the truth, when it says.
                    let seekable = a.seekable();
                    let reach = if seekable.length() > 0 { seekable.end(seekable.length() - 1).unwrap_or(0.0) } else { 0.0 };
                    let length = match (a.duration().is_finite() && a.duration() > 0.0, reach > 0.0) {
                        (true, true) => a.duration().min(reach),
                        (true, false) => a.duration(),
                        (false, true) => reach,
                        (false, false) => 0.0,
                    };
                    if length > 0.0 && (self.duration.get_untracked() - length).abs() > 0.5 {
                        self.duration.set(length);
                    }
                }
            }
            Mode::YouTube => {
                // YouTube's player that never starts (unstarted or cued) counts
                // as an error after 15 seconds, so the song plays another way.
                let state = self.state.get_untracked();
                if (state == -1 || state == 5) && !self.video_id.get_untracked().is_empty() && now() - self.loaded_at.get_value() > 15_000.0 {
                    self.loaded_at.set_value(now());
                    self.fail(1004);
                }
            }
            Mode::SoundCloud | Mode::Mixcloud => {
                // A widget that never starts counts as an error after 30 seconds.
                let state = self.state.get_untracked();
                if state < 1 && state != 2 && now() - self.loaded_at.get_value() > 30_000.0 {
                    self.loaded_at.set_value(now());
                    self.fail(if self.mode.get_value() == Mode::SoundCloud { 1002 } else { 1003 });
                }
            }
            _ => {}
        }
    }

    fn reset(&self, id: String, start: f64) {
        self.video_id.set(id);
        self.position.set(start);
        self.duration.set(0.0);
        self.state.set(-1);
        self.start.set_value(start);
        self.loaded_at.set_value(now());
        self.relay.set_value(None);
        self.offset.set_value(0.0);
    }

    /// Quiets every player but the one about to play.
    fn switch(&self, next: Mode) {
        let was = self.mode.get_value();
        if was == Mode::YouTube && next != Mode::YouTube {
            self.yt_command("stopVideo", json!([]));
        }
        if next != Mode::Audio {
            let a = audio();
            if !a.src().is_empty() {
                let _ = a.pause();
                a.remove_attribute("src").ok();
                a.load();
            }
        }
        if next != Mode::SoundCloud {
            if let Some(f) = frame("engine-sc") {
                f.remove();
            }
        }
        if next != Mode::Mixcloud {
            if let Some(f) = frame("engine-mc") {
                f.remove();
            }
        }
        self.mode.set_value(next);
    }

    // YouTube

    fn yt_command(&self, func: &str, args: Value) {
        post("engine-yt", &json!({ "event": "command", "func": func, "args": args, "id": 1, "channel": "widget" }), "https://www.youtube.com");
    }

    /// Makes YouTube's player with its video in it (an empty player never answers).
    fn youtube_frame(&self, id: &str, start: f64) {
        let document = window().document().expect("document");
        let f: HtmlIFrameElement = document.create_element("iframe").expect("iframe").unchecked_into();
        f.set_id("engine-yt");
        let _ = f.set_attribute("allow", "autoplay; encrypted-media");
        let o = encode(&origin());
        f.set_src(&format!(
            "https://www.youtube.com/embed/{}?enablejsapi=1&autoplay=1&start={}&controls=0&disablekb=1&fs=0&playsinline=1&rel=0&iv_load_policy=3&origin={o}&widget_referrer={o}",
            encode(id),
            start.floor()
        ));
        // The player answers once it knows someone is listening; keep asking until it does.
        let engine = *self;
        let on_load = Closure::<dyn Fn()>::new(move || {
            let tries = StoredValue::new(0u32);
            let ask = Closure::<dyn Fn()>::new(move || {
                if engine.yt_ready.get_value() || tries.get_value() > 80 {
                    return;
                }
                tries.update_value(|t| *t += 1);
                post("engine-yt", &json!({ "event": "listening", "id": 1, "channel": "widget" }), "https://www.youtube.com");
            });
            for i in 0..80 {
                let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(ask.as_ref().unchecked_ref(), 100 + i * 250);
            }
            ask.forget();
        });
        f.set_onload(Some(on_load.as_ref().unchecked_ref()));
        on_load.forget();
        let _ = host().append_child(&f);
    }

    fn on_youtube(&self, data: &Value) {
        match data["event"].as_str().unwrap_or("") {
            "onReady" | "initialDelivery" | "alreadyInitialized" => {
                if !self.yt_ready.get_value() {
                    self.yt_ready.set_value(true);
                    self.yt_command("setVolume", json!([self.volume.get_value()]));
                    if let Some((id, start)) = self.yt_pending.get_value() {
                        self.yt_pending.set_value(None);
                        self.yt_command("loadVideoById", json!([{ "videoId": id, "startSeconds": start, "suggestedQuality": "small" }]));
                    }
                }
            }
            "infoDelivery" => {
                if self.mode.get_value() != Mode::YouTube || self.video_id.get_untracked().is_empty() {
                    return;
                }
                let info = &data["info"];
                if let Some(state) = info["playerState"].as_i64() {
                    let state = state as i32;
                    let was = self.state.get_untracked();
                    self.state.set(state);
                    if state == 0 && was != 0 {
                        self.finish();
                    }
                }
                if let Some(time) = info["currentTime"].as_f64() {
                    self.position.set(time);
                }
                if let Some(length) = info["duration"].as_f64().filter(|d| *d > 0.0) {
                    self.duration.set(length);
                }
            }
            "onStateChange" => {
                if self.mode.get_value() == Mode::YouTube {
                    if let Some(state) = data["info"].as_i64() {
                        let was = self.state.get_untracked();
                        self.state.set(state as i32);
                        if state == 0 && was != 0 {
                            self.finish();
                        }
                    }
                }
            }
            "onError" => {
                if self.mode.get_value() == Mode::YouTube {
                    self.fail(data["info"].as_i64().unwrap_or(5) as i32);
                }
            }
            _ => {}
        }
    }

    /// A fresh player for every video: once YouTube's player has refused a
    /// video (its owner blocks embedding) it won't take another one.
    pub fn load(&self, id: &str, start: f64) {
        self.switch(Mode::YouTube);
        self.reset(id.to_owned(), start);
        if let Some(old) = frame("engine-yt") {
            old.remove();
        }
        self.yt_ready.set_value(false);
        self.yt_pending.set_value(None);
        self.youtube_frame(id, start);
    }

    // Plain audio files (Bandcamp, the Internet Archive)

    pub fn load_audio(&self, url: &str, start: f64) {
        self.switch(Mode::Audio);
        self.reset(format!("audio:{url}"), start);
        let a = audio();
        // Through Earshot's server, so the browser may measure it for the waveform.
        a.set_src(&format!("/api/audio?url={}", encode(url)));
        a.set_volume(self.volume.get_value() / 100.0);
        if start > 0.0 {
            a.set_current_time(start);
        }
        let _ = a.play();
    }

    /// A SoundCloud track's audio played from Earshot's server, on iPhones and
    /// iPads, where SoundCloud's own player can't start by itself.
    pub fn load_soundcloud_audio(&self, track_id: &str, start: f64) {
        self.switch(Mode::Audio);
        self.reset(format!("soundcloud:{track_id}"), start);
        let a = audio();
        a.set_src(&format!("/api/soundcloud-audio?id={}", encode(track_id)));
        a.set_volume(self.volume.get_value() / 100.0);
        if start > 0.0 {
            a.set_current_time(start);
        }
        let _ = a.play();
    }

    /// A YouTube video's audio played from Earshot's server, for videos whose
    /// owners don't let YouTube's player play them on other sites. It's still
    /// the video (its id is kept), so it's remembered and played as usual.
    pub fn load_youtube_audio(&self, id: &str, start: f64) {
        self.switch(Mode::Audio);
        self.reset(id.to_owned(), start);
        let a = audio();
        a.set_src(&format!("/api/youtube-audio?v={}", encode(id)));
        a.set_volume(self.volume.get_value() / 100.0);
        if start > 0.0 {
            a.set_current_time(start);
        }
        let _ = a.play();
    }

    /// A video whose owner keeps it on youtube.com, when YouTube won't hand
    /// its audio over either: Earshot's server plays it on youtube.com's own
    /// page and streams the sound here, live from `start` seconds. Pausing
    /// hangs up; playing or seeking asks again from that point. `duration`:
    /// the video's length, if known (the stream doesn't say).
    pub fn load_relay(&self, id: &str, start: f64, duration: f64) {
        self.switch(Mode::Audio);
        self.reset(id.to_owned(), start);
        if duration > 0.0 {
            self.duration.set(duration);
        }
        self.relay.set_value(Some(id.to_owned()));
        self.relay_from(start);
    }

    fn relay_from(&self, seconds: f64) {
        let Some(id) = self.relay.get_value() else { return };
        let seconds = seconds.max(0.0);
        self.offset.set_value(seconds);
        self.position.set(seconds);
        self.relay_count.update_value(|n| *n += 1);
        let a = audio();
        a.set_src(&format!("/api/relay?v={}&t={:.1}&n={}", encode(&id), seconds, self.relay_count.get_value()));
        a.set_volume(self.volume.get_value() / 100.0);
        let _ = a.play();
    }

    /// Asks for the relayed stream again from where it got to (it broke off).
    pub fn reconnect_relay(&self) {
        if self.relaying() {
            self.relay_from(self.position.get_untracked());
        }
    }


    // SoundCloud's widget

    fn sc(&self, method: &str, value: Value) {
        let mut message = json!({ "method": method });
        if !value.is_null() {
            message["value"] = value;
        }
        post("engine-sc", &message, "https://w.soundcloud.com");
    }

    pub fn load_soundcloud(&self, track_id: &str, start: f64) {
        self.switch(Mode::SoundCloud);
        self.reset(format!("soundcloud:{track_id}"), start);
        if let Some(old) = frame("engine-sc") {
            old.remove();
        }
        let document = window().document().expect("document");
        let f: HtmlIFrameElement = document.create_element("iframe").expect("iframe").unchecked_into();
        f.set_id("engine-sc");
        let _ = f.set_attribute("allow", "autoplay");
        f.set_src(&format!(
            "https://w.soundcloud.com/player/?url={}&auto_play=true&visual=false",
            encode(&format!("https://api.soundcloud.com/tracks/{track_id}"))
        ));
        let _ = host().append_child(&f);
    }

    fn on_soundcloud(&self, data: &Value) {
        if self.mode.get_value() != Mode::SoundCloud {
            return;
        }
        match data["method"].as_str().unwrap_or("") {
            "ready" => {
                // The widget only takes listeners once it's ready (it says so by itself).
                for event in ["play", "pause", "finish", "playProgress", "error"] {
                    self.sc("addEventListener", json!(event));
                }
                self.sc("setVolume", json!(self.volume.get_value()));
                self.sc("getDuration", Value::Null);
                let start = self.start.get_value();
                if start > 0.0 {
                    self.sc("seekTo", json!(start * 1000.0));
                }
                self.sc("play", Value::Null);
            }
            "getDuration" => {
                if let Some(ms) = data["value"].as_f64() {
                    self.duration.set(ms / 1000.0);
                }
            }
            "play" => {
                self.state.set(1);
                self.sc("getDuration", Value::Null);
            }
            "pause" => {
                if self.state.get_untracked() != 0 {
                    self.state.set(2);
                }
            }
            "playProgress" => {
                if let Some(ms) = data["value"]["currentPosition"].as_f64() {
                    self.position.set(ms / 1000.0);
                }
                if self.state.get_untracked() < 1 {
                    self.state.set(1);
                }
            }
            "finish" => {
                self.state.set(0);
                self.finish();
            }
            "error" => self.fail(1002),
            _ => {}
        }
    }

    // Mixcloud's widget

    fn mc(&self, method: &str, args: Value) {
        self.mc_method.update_value(|n| *n += 1);
        let id = self.mc_method.get_value();
        post(
            "engine-mc",
            &json!({ "type": "method", "data": { "methodId": id, "methodName": method, "args": args } }),
            "https://player-widget.mixcloud.com",
        );
    }

    pub fn load_mixcloud(&self, key: &str, start: f64) {
        self.switch(Mode::Mixcloud);
        self.reset(format!("mixcloud:{key}"), start);
        if let Some(old) = frame("engine-mc") {
            old.remove();
        }
        let document = window().document().expect("document");
        let f: HtmlIFrameElement = document.create_element("iframe").expect("iframe").unchecked_into();
        f.set_id("engine-mc");
        let _ = f.set_attribute("allow", "autoplay; encrypted-media");
        f.set_src(&format!("https://player-widget.mixcloud.com/?hide_cover=1&mini=1&autoplay=1&feed={}", encode(key)));
        let on_load = Closure::<dyn Fn()>::new(move || {
            post("engine-mc", &json!({ "type": "getApi" }), "https://player-widget.mixcloud.com");
        });
        f.set_onload(Some(on_load.as_ref().unchecked_ref()));
        on_load.forget();
        let _ = host().append_child(&f);
    }

    fn on_mixcloud(&self, data: &Value) {
        if data["mixcloud"] != "playerWidget" || self.mode.get_value() != Mode::Mixcloud {
            return;
        }
        match data["type"].as_str().unwrap_or("") {
            "ready" => post("engine-mc", &json!({ "type": "getApi" }), "https://player-widget.mixcloud.com"),
            "api" => {
                let start = self.start.get_value();
                if start > 0.0 {
                    self.mc("seek", json!([start]));
                }
                self.mc("play", json!([]));
                self.mc("setVolume", json!([self.volume.get_value() / 100.0]));
            }
            "event" => {
                let args = &data["data"]["args"];
                match data["data"]["eventName"].as_str().unwrap_or("") {
                    "progress" => {
                        if let Some(position) = args[0].as_f64() {
                            self.position.set(position);
                        }
                        if let Some(length) = args[1].as_f64().filter(|d| *d > 0.0) {
                            self.duration.set(length);
                        }
                        let state = self.state.get_untracked();
                        if state != 2 && state != 0 {
                            self.state.set(1);
                        }
                    }
                    "play" => self.state.set(1),
                    "pause" => {
                        if self.state.get_untracked() != 0 {
                            self.state.set(2);
                        }
                    }
                    "buffering" => {
                        if self.state.get_untracked() != 2 {
                            self.state.set(3);
                        }
                    }
                    "ended" => {
                        self.state.set(0);
                        self.finish();
                    }
                    "error" => self.fail(1003),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // The same controls for whichever player is in use

    pub fn play(&self) {
        match self.mode.get_value() {
            Mode::YouTube => self.yt_command("playVideo", json!([])),
            Mode::Audio if self.relaying() && audio().src().is_empty() => self.relay_from(self.offset.get_value()),
            Mode::Audio => {
                let _ = audio().play();
            }
            Mode::SoundCloud => self.sc("play", Value::Null),
            Mode::Mixcloud => self.mc("play", json!([])),
            Mode::None => {}
        }
    }

    pub fn pause(&self) {
        match self.mode.get_value() {
            Mode::YouTube => self.yt_command("pauseVideo", json!([])),
            // A relayed stream stays connected while paused (the server pauses
            // youtube.com's page when nobody's taking the sound), so playing
            // again carries straight on instead of reconnecting.
            Mode::Audio => {
                let _ = audio().pause();
            }
            Mode::SoundCloud => self.sc("pause", Value::Null),
            Mode::Mixcloud => self.mc("pause", json!([])),
            Mode::None => {}
        }
        // The widgets say so themselves, but show it at once.
        if self.state.get_untracked() == 1 {
            self.state.set(2);
        }
    }

    pub fn seek(&self, seconds: f64) {
        self.position.set(seconds);
        match self.mode.get_value() {
            Mode::YouTube => self.yt_command("seekTo", json!([seconds, true])),
            Mode::Audio if self.relaying() => {
                if audio().src().is_empty() {
                    self.offset.set_value(seconds);
                } else {
                    self.relay_from(seconds);
                }
            }
            // Past the end of the audio (the song's listed length can be longer
            // than the upload): the song is over, rather than playing silence.
            Mode::Audio => {
                let length = self.duration.get_untracked();
                if length > 0.0 && seconds >= length - 0.5 {
                    self.finish();
                } else {
                    audio().set_current_time(seconds);
                }
            }
            Mode::SoundCloud => self.sc("seekTo", json!(seconds * 1000.0)),
            Mode::Mixcloud => self.mc("seek", json!([seconds])),
            Mode::None => {}
        }
    }

    pub fn stop(&self) {
        self.switch(Mode::None);
        self.video_id.set(String::new());
        self.state.set(-1);
    }

    /// 0..100
    pub fn set_volume(&self, percent: f64) {
        self.volume.set_value(percent);
        self.yt_command("unMute", json!([]));
        self.yt_command("setVolume", json!([percent.round()]));
        audio().set_volume(percent / 100.0);
        self.sc("setVolume", json!(percent.round()));
        self.mc("setVolume", json!([percent / 100.0]));
    }
}

fn encode(text: &str) -> String {
    js_sys::encode_uri_component(text).as_string().unwrap_or_default()
}
