//! Playback: the queue, shuffle and repeat, random radio and the listening
//! history, driving the engine. The same rules as the Qt app's Player.

use std::collections::{HashMap, HashSet};

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{Value, json};

use crate::api;
use crate::engine::Engine;
use crate::state::{app, list, num, persisted, s};

#[derive(Clone, Copy)]
pub struct Player {
    pub engine: Engine,
    pub queue: RwSignal<Vec<Value>>,
    original: RwSignal<Vec<Value>>,
    pub index: RwSignal<i64>,
    pub context: RwSignal<String>,
    /// The page the music is playing from, so the player bar can go back there.
    pub context_route: RwSignal<Option<String>>,
    pub shuffle: RwSignal<bool>,
    /// 0 off, 1 all, 2 one
    pub repeat: RwSignal<u8>,
    pub volume: RwSignal<f64>,
    pub muted: RwSignal<bool>,
    pub radio: RwSignal<bool>,
    pub radio_loading: RwSignal<bool>,
    radio_waiting: RwSignal<bool>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<String>,
    failures: StoredValue<u32>,
    request: StoredValue<u64>,
    candidates: StoredValue<Vec<String>>,
    candidate: StoredValue<usize>,
    /// Track key -> videos found earlier, so upcoming songs start instantly.
    pub found: RwSignal<HashMap<String, Vec<String>>>,
    started: RwSignal<bool>,
    unrecorded: StoredValue<Option<Value>>,
    /// Bandcamp songs' audio links, with when they were fetched.
    streams: StoredValue<HashMap<String, (String, f64)>>,
    stream_retried: StoredValue<String>,
    looking: StoredValue<HashSet<String>>,
    /// Counts up when a new song starts (for the now-playing card).
    pub started_count: RwSignal<u32>,
    /// The waveform's six bar heights, 0..1, bass first (see levels.rs).
    pub levels: RwSignal<[f64; crate::levels::BANDS]>,
    /// Why the waveform is still, when the music can't be measured.
    pub levels_note: RwSignal<String>,
}

fn shuffled(mut items: Vec<Value>) -> Vec<Value> {
    for i in (1..items.len()).rev() {
        let j = (js_sys::Math::random() * (i + 1) as f64).floor() as usize;
        items.swap(i, j);
    }
    items
}

fn copy(tracks: Vec<Value>) -> Vec<Value> {
    tracks.into_iter().filter(|t| t["key"].is_string()).collect()
}

impl Player {
    pub fn new() -> Self {
        let player = Self {
            engine: Engine::new(),
            queue: RwSignal::new(Vec::new()),
            original: RwSignal::new(Vec::new()),
            index: RwSignal::new(-1),
            context: RwSignal::new(String::new()),
            context_route: RwSignal::new(None),
            shuffle: persisted("shuffle", false),
            repeat: persisted("repeat", 0),
            volume: persisted("volume", 0.8),
            muted: RwSignal::new(false),
            radio: persisted("radio", false),
            radio_loading: RwSignal::new(false),
            radio_waiting: RwSignal::new(false),
            loading: RwSignal::new(false),
            error: RwSignal::new(String::new()),
            failures: StoredValue::new(0),
            request: StoredValue::new(0),
            candidates: StoredValue::new(Vec::new()),
            candidate: StoredValue::new(0),
            found: RwSignal::new(HashMap::new()),
            started: RwSignal::new(false),
            unrecorded: StoredValue::new(None),
            streams: StoredValue::new(HashMap::new()),
            stream_retried: StoredValue::new(String::new()),
            looking: StoredValue::new(HashSet::new()),
            started_count: RwSignal::new(0),
            levels: RwSignal::new([0.0; crate::levels::BANDS]),
            levels_note: RwSignal::new(String::new()),
        };
        player.wire();
        player
    }

    pub fn current(&self) -> Option<Value> {
        let index = self.index.get();
        self.queue.with(|q| if index >= 0 { q.get(index as usize).cloned() } else { None })
    }

    pub fn current_untracked(&self) -> Option<Value> {
        let index = self.index.get_untracked();
        self.queue.with_untracked(|q| if index >= 0 { q.get(index as usize).cloned() } else { None })
    }

    pub fn current_key(&self) -> String {
        self.current().map(|t| s(&t, "key")).unwrap_or_default()
    }

    pub fn playing(&self) -> bool {
        self.engine.playing()
    }

    // A track can be a chapter of a longer video: it starts and ends inside it.
    fn track_start(&self) -> f64 {
        self.current_untracked().map(|t| num(&t, "start")).unwrap_or(0.0)
    }
    fn track_end(&self) -> f64 {
        self.current_untracked().map(|t| num(&t, "end")).unwrap_or(0.0)
    }

    /// Seconds into the current track.
    pub fn position(&self) -> f64 {
        let start = self.current().map(|t| num(&t, "start")).unwrap_or(0.0);
        (self.engine.position.get() - start).max(0.0)
    }

    pub fn length(&self) -> f64 {
        let Some(track) = self.current() else { return 0.0 };
        let (start, end) = (num(&track, "start"), num(&track, "end"));
        let duration = self.engine.duration.get();
        if end > 0.0 {
            end - start
        } else if duration > 0.0 {
            duration - start
        } else {
            num(&track, "duration")
        }
    }

    pub fn has_next(&self) -> bool {
        let (index, length) = (self.index.get(), self.queue.with(Vec::len) as i64);
        index + 1 < length || (self.repeat.get() == 1 && length > 0) || (self.radio.get() && index >= 0)
    }

    /// Reacts to the engine: songs starting, finishing and failing; chapters ending; volume.
    fn wire(self) {
        let engine = self.engine;
        Effect::new(move |_| {
            let volume = self.volume.get();
            let muted = self.muted.get();
            engine.set_volume(if muted { 0.0 } else { volume * volume * 100.0 });
        });
        Effect::new(move |_| {
            if engine.state.get() == 1 && !self.started.get_untracked() {
                if let Some(track) = self.current_untracked() {
                    self.started.set(true);
                    self.loading.set(false);
                    self.failures.set_value(0);
                    self.track_started(&track);
                    let video = engine.video_id.get_untracked();
                    if track["youtubeId"].is_null() && track["service"].is_null() {
                        api::send("rememberVideo", json!({ "key": s(&track, "key"), "video": video }));
                    }
                    self.prefetch();
                }
            }
        });
        Effect::new(move |_| {
            let position = engine.position.get();
            self.record_if_listened();
            let end = self.track_end();
            if end > 0.0 && self.started.get_untracked() && position >= end - 0.2 {
                self.next(true);
            }
        });
        Effect::new(move |previous: Option<u32>| {
            let ended = engine.ended.get();
            if previous.is_some_and(|p| p != ended) {
                self.next(true);
            }
            ended
        });
        Effect::new(move |_| {
            let Some((_, code, message)) = engine.failure.get() else { return };
            let Some(track) = self.current_untracked() else { return };
            let key = s(&track, "key");
            // A Bandcamp link may have expired: fetch a fresh one once.
            if s(&track, "service") == "bandcamp" && self.streams.with_value(|m| m.contains_key(&key)) && self.stream_retried.get_value() != key {
                self.stream_retried.set_value(key.clone());
                self.streams.update_value(|m| {
                    m.remove(&key);
                });
                self.resolve_current();
                return;
            }
            // Another upload of the same song may be allowed to play here.
            let next = self.candidate.get_value() + 1;
            if track["service"].is_null() && next < self.candidates.with_value(Vec::len) {
                self.candidate.set_value(next);
                let video = self.candidates.with_value(|c| c[next].clone());
                engine.load(&video, self.track_start());
                return;
            }
            let _ = code;
            self.fail(format!("Couldn't play \"{}\": {message}", s(&track, "title")));
        });
        Effect::new(move |_| {
            if self.radio.get() {
                let length = self.queue.with_untracked(Vec::len) as i64;
                if self.current_untracked().is_some() && self.index.get_untracked() >= length - 2 {
                    self.extend_radio();
                }
            }
        });
    }

    fn track_started(&self, track: &Value) {
        self.unrecorded.set_value(Some(track.clone()));
        self.started_count.update(|n| *n += 1);
    }

    /// Listening history (for Made for you): a song counts once it has played
    /// 30 seconds, or half of a shorter one, so skipped songs don't.
    fn record_if_listened(&self) {
        let Some(unrecorded) = self.unrecorded.get_value() else { return };
        let Some(current) = self.current_untracked() else { return };
        if s(&unrecorded, "key") != s(&current, "key") {
            return;
        }
        let position = (self.engine.position.get_untracked() - num(&current, "start")).max(0.0);
        let length = {
            let end = num(&current, "end");
            let duration = self.engine.duration.get_untracked();
            if end > 0.0 { end - num(&current, "start") } else if duration > 0.0 { duration } else { num(&current, "duration") }
        };
        if position < 30f64.min((length / 2.0).max(5.0)) {
            return;
        }
        let played = self.engine.video_id.get_untracked();
        let video = if played.contains(':') { String::new() } else { played };
        api::send("recordPlay", json!({ "track": unrecorded, "video": video }));
        self.unrecorded.set_value(None);
    }

    fn extend_radio(&self) {
        if !self.radio.get_untracked() || self.radio_loading.get_untracked() {
            return;
        }
        let Some(seed) = self.current_untracked() else { return };
        let playing = self.engine.video_id.get_untracked();
        let found = self.found.with_untracked(|f| f.get(&s(&seed, "key")).and_then(|v| v.first().cloned()));
        let video = seed["youtubeId"].as_str().map(str::to_owned).or(found).unwrap_or_else(|| {
            if seed["service"].is_null() && !playing.contains(':') { playing } else { String::new() }
        });
        self.radio_loading.set(true);
        let player = *self;
        spawn_local(async move {
            let result = api::call("radioMore", json!({ "track": seed, "video": video })).await;
            player.radio_loading.set(false);
            match result {
                Ok(result) => {
                    let mut seen = HashSet::new();
                    player.queue.with_untracked(|q| {
                        for track in q {
                            seen.insert(s(track, "key"));
                            if let Some(id) = track["youtubeId"].as_str() {
                                seen.insert(format!("yt:{id}"));
                            }
                        }
                    });
                    let added: Vec<Value> = list(&result, "tracks").into_iter().filter(|t| !seen.contains(&s(t, "key"))).take(15).collect();
                    if !added.is_empty() {
                        player.queue.update(|q| q.extend(added.clone()));
                        player.original.update(|q| q.extend(added));
                    }
                    if player.radio_waiting.get_untracked() {
                        player.radio_waiting.set(false);
                        let next = player.index.get_untracked() + 1;
                        if next < player.queue.with_untracked(Vec::len) as i64 {
                            player.jump(next);
                        }
                    }
                }
                Err(message) => {
                    if player.radio_waiting.get_untracked() {
                        player.radio_waiting.set(false);
                        player.loading.set(false);
                        app().notify(format!("Random radio: {message}"));
                    }
                }
            }
        });
    }

    pub fn play_tracks(&self, tracks: Vec<Value>, start: usize, context: String, route: Option<String>) {
        let mut items = copy(tracks);
        if items.is_empty() {
            return;
        }
        let path = window_path();
        self.context_route.set(Some(route.unwrap_or(path)));
        let mut start = start.min(items.len() - 1);
        self.original.set(items.clone());
        if self.shuffle.get_untracked() {
            let first = items.remove(start);
            items = std::iter::once(first).chain(shuffled(items)).collect();
            start = 0;
        }
        self.context.set(context);
        self.queue.set(items);
        self.failures.set_value(0);
        self.jump(start as i64);
    }

    pub fn shuffle_tracks(&self, tracks: Vec<Value>, context: String, route: Option<String>) {
        self.shuffle.set(true);
        let items = copy(tracks);
        let start = (js_sys::Math::random() * items.len() as f64).floor() as usize;
        self.play_tracks(items, start, context, route);
    }

    /// Opens the album, playlist or page the music is playing from.
    pub fn open_source(&self) {
        let app = app();
        if let Some(route) = self.context_route.get_untracked() {
            app.go(route);
        } else if let Some(album) = self.current_untracked().map(|t| s(&t, "albumId")).filter(|a| !a.is_empty()) {
            app.go(crate::state::album_href(&album));
        } else {
            app.go("/queue");
        }
    }

    pub fn jump(&self, i: i64) {
        if i < 0 || i >= self.queue.with_untracked(Vec::len) as i64 {
            return;
        }
        self.index.set(i);
        self.resolve_current();
    }

    fn start_videos(&self, videos: Vec<String>) {
        let Some(first) = videos.first().cloned() else { return };
        self.candidates.set_value(videos);
        self.candidate.set_value(0);
        self.engine.load(&first, self.track_start());
    }

    fn next_request(&self) -> u64 {
        self.request.update_value(|n| *n += 1);
        self.request.get_value()
    }

    fn resolve_current(&self) {
        let Some(track) = self.current_untracked() else { return };
        let engine = self.engine;
        self.error.set(String::new());
        let start = num(&track, "start");
        let key = s(&track, "key");
        // Another chapter of the video that's already playing: just move there.
        if let Some(video) = track["youtubeId"].as_str() {
            if video == engine.video_id.get_untracked() && self.started.get_untracked() {
                if (engine.position.get_untracked() - start).abs() > 1.5 {
                    engine.seek(start);
                }
                engine.play();
                self.track_started(&track);
                self.prefetch();
                return;
            }
        }
        self.started.set(false);
        self.loading.set(true);
        let request = self.next_request();
        match s(&track, "service").as_str() {
            // SoundCloud songs play in SoundCloud's own player, Mixcloud shows in Mixcloud's.
            "soundcloud" => engine.load_soundcloud(&s(&track, "soundcloudId"), start),
            "mixcloud" => engine.load_mixcloud(&s(&track, "mixcloudKey"), start),
            // The Internet Archive's files stream straight from archive.org.
            "archive" => engine.load_audio(&s(&track, "streamUrl"), start),
            // Bandcamp songs stream from Bandcamp, with a link fetched just before.
            "bandcamp" => {
                if let Some(url) = self.stream_for(&key) {
                    engine.load_audio(&url, start);
                    return;
                }
                engine.stop();
                let player = *self;
                spawn_local(async move {
                    let result = api::call("stream", json!({ "track": track })).await;
                    if player.request.get_value() != request {
                        return;
                    }
                    match result {
                        Ok(result) => {
                            player.remember_stream(&result);
                            engine.load_audio(&s(&result, "url"), start);
                        }
                        Err(message) => player.fail(message),
                    }
                });
            }
            // YouTube tracks already know their video; others were often looked up ahead of time.
            _ => {
                let known = track["youtubeId"].as_str().map(|v| vec![v.to_owned()]).or_else(|| self.found.with_untracked(|f| f.get(&key).cloned()));
                if let Some(videos) = known.filter(|v| !v.is_empty()) {
                    self.start_videos(videos);
                    return;
                }
                engine.stop();
                let player = *self;
                spawn_local(async move {
                    let result = api::call("resolve", json!({ "track": track })).await;
                    match result {
                        Ok(result) => {
                            let videos: Vec<String> = list(&result, "videos").iter().filter_map(|v| v.as_str().map(str::to_owned)).collect();
                            player.found.update(|f| {
                                f.insert(key.clone(), videos.clone());
                            });
                            if player.request.get_value() == request {
                                player.start_videos(videos);
                            }
                        }
                        Err(message) => {
                            if player.request.get_value() == request {
                                player.fail(message);
                            }
                        }
                    }
                });
            }
        }
    }

    fn stream_for(&self, key: &str) -> Option<String> {
        let now = js_sys::Date::now();
        self.streams.with_value(|m| m.get(key).filter(|(_, at)| now - at < 3_600_000.0).map(|(url, _)| url.clone()))
    }

    fn remember_stream(&self, result: &Value) {
        let now = js_sys::Date::now();
        self.streams.update_value(|m| {
            m.insert(s(result, "key"), (s(result, "url"), now));
        });
    }

    /// Looks up the next songs' videos while this one plays, so they start at once.
    fn prefetch(&self) {
        let index = self.index.get_untracked();
        let upcoming: Vec<Value> = self.queue.with_untracked(|q| q.iter().skip((index + 1).max(0) as usize).take(2).cloned().collect());
        for track in upcoming {
            self.look_ahead(&track);
        }
        // Random radio: fetch more songs before the queue runs out.
        if self.radio.get_untracked() && index >= self.queue.with_untracked(Vec::len) as i64 - 2 {
            self.extend_radio();
        }
    }

    /// Finds a track's video ahead of time (songs you point at, the next songs).
    pub fn look_ahead(&self, track: &Value) {
        let service = s(track, "service");
        if track["youtubeId"].is_string() || ["soundcloud", "mixcloud", "archive"].contains(&service.as_str()) {
            return;
        }
        let key = s(track, "key");
        if self.looking.with_value(|l| l.contains(&key)) {
            return;
        }
        self.looking.update_value(|l| {
            l.insert(key.clone());
        });
        let player = *self;
        let track = track.clone();
        if service == "bandcamp" {
            if self.stream_for(&key).is_some() {
                return;
            }
            spawn_local(async move {
                if let Ok(result) = api::call("stream", json!({ "track": track })).await {
                    player.remember_stream(&result);
                }
                player.looking.update_value(|l| {
                    l.remove(&key);
                });
            });
            return;
        }
        if self.found.with_untracked(|f| f.contains_key(&key)) {
            return;
        }
        spawn_local(async move {
            if let Ok(result) = api::call("resolve", json!({ "track": track })).await {
                let videos: Vec<String> = list(&result, "videos").iter().filter_map(|v| v.as_str().map(str::to_owned)).collect();
                player.found.update(|f| {
                    f.insert(key.clone(), videos);
                });
            }
            player.looking.update_value(|l| {
                l.remove(&key);
            });
        });
    }

    fn fail(&self, message: String) {
        self.loading.set(false);
        self.error.set(message.clone());
        app().notify(message);
        self.failures.update_value(|n| *n += 1);
        if self.failures.get_value() < 3 && self.index.get_untracked() + 1 < self.queue.with_untracked(Vec::len) as i64 {
            let player = *self;
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(1200).await;
                player.next(true);
            });
        }
    }

    pub fn toggle(&self) {
        if self.current_untracked().is_none() {
            return;
        }
        if self.engine.state.get_untracked() == 1 {
            self.engine.pause();
        } else if !self.started.get_untracked() || !self.error.get_untracked().is_empty() {
            self.resolve_current();
        } else {
            self.engine.play();
        }
    }

    pub fn next(&self, automatic: bool) {
        let length = self.queue.with_untracked(Vec::len) as i64;
        if length == 0 {
            return;
        }
        if automatic && self.repeat.get_untracked() == 2 {
            self.seek(0.0);
            self.engine.play();
            return;
        }
        let index = self.index.get_untracked();
        if index + 1 < length {
            self.jump(index + 1);
        } else if self.repeat.get_untracked() == 1 {
            self.jump(0);
        } else if self.radio.get_untracked() {
            // More songs are on their way (or about to be fetched): play the first when they come.
            self.radio_waiting.set(true);
            self.loading.set(true);
            self.extend_radio();
        } else if automatic {
            self.engine.pause();
            self.loading.set(false);
        }
    }

    pub fn previous(&self) {
        let position = (self.engine.position.get_untracked() - self.track_start()).max(0.0);
        if position > 3.0 || self.index.get_untracked() <= 0 {
            self.seek(0.0);
            return;
        }
        self.jump(self.index.get_untracked() - 1);
    }

    /// Seconds from the start of the current track (which may be a chapter).
    pub fn seek(&self, seconds: f64) {
        self.engine.seek(self.track_start() + seconds.max(0.0));
    }

    pub fn set_shuffle(&self, on: bool) {
        if on == self.shuffle.get_untracked() {
            return;
        }
        self.shuffle.set(on);
        if self.queue.with_untracked(Vec::is_empty) {
            return;
        }
        let index = self.index.get_untracked().max(0) as usize;
        if on {
            let queue = self.queue.get_untracked();
            self.original.set(queue.clone());
            let (done, rest) = queue.split_at((index + 1).min(queue.len()));
            self.queue.set(done.iter().cloned().chain(shuffled(rest.to_vec())).collect());
        } else {
            let playing = self.current_untracked().map(|t| s(&t, "key"));
            let keys: HashSet<String> = self.queue.with_untracked(|q| q.iter().map(|t| s(t, "key")).collect());
            let restored: Vec<Value> = self.original.get_untracked().into_iter().filter(|t| keys.contains(&s(t, "key"))).collect();
            let at = playing.and_then(|key| restored.iter().position(|t| s(t, "key") == key)).unwrap_or(0);
            self.queue.set(restored);
            self.index.set(at as i64);
        }
    }

    pub fn cycle_repeat(&self) {
        self.repeat.update(|r| *r = (*r + 1) % 3);
    }

    pub fn play_next(&self, tracks: Vec<Value>) {
        let items = copy(tracks);
        if items.is_empty() {
            return;
        }
        if self.current_untracked().is_none() {
            self.play_tracks(items, 0, String::new(), None);
            return;
        }
        let at = (self.index.get_untracked() + 1) as usize;
        let message = if items.len() == 1 { format!("Playing \"{}\" next", s(&items[0], "title")) } else { format!("{} songs will play next", items.len()) };
        self.queue.update(|q| {
            let at = at.min(q.len());
            q.splice(at..at, items.clone());
        });
        self.original.update(|q| q.extend(items));
        app().notify(message);
    }

    pub fn enqueue(&self, tracks: Vec<Value>) {
        let items = copy(tracks);
        if items.is_empty() {
            return;
        }
        if self.current_untracked().is_none() {
            self.play_tracks(items, 0, String::new(), None);
            return;
        }
        let message = if items.len() == 1 { format!("Added \"{}\" to the queue", s(&items[0], "title")) } else { format!("Added {} songs to the queue", items.len()) };
        self.queue.update(|q| q.extend(items.clone()));
        self.original.update(|q| q.extend(items));
        app().notify(message);
    }

    pub fn remove_at(&self, i: i64) {
        let index = self.index.get_untracked();
        if i < 0 || i >= self.queue.with_untracked(Vec::len) as i64 || i == index {
            return;
        }
        let removed = self.queue.with_untracked(|q| s(&q[i as usize], "key"));
        if i < index {
            self.index.set(index - 1);
        }
        self.queue.update(|q| {
            q.remove(i as usize);
        });
        self.original.update(|q| q.retain(|t| s(t, "key") != removed));
    }

    pub fn clear_upcoming(&self) {
        let keep = (self.index.get_untracked() + 1).max(0) as usize;
        self.queue.update(|q| q.truncate(keep));
    }
}

fn window_path() -> String {
    let location = crate::state::window().location();
    format!("{}{}", location.pathname().unwrap_or_default(), location.search().unwrap_or_default())
}
