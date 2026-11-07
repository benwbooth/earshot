//! What every part of the interface shares: the library (playlists, liked
//! songs, saved albums), settings kept in the browser, notices, the open menu
//! and dialog, and helpers matching the Qt app's Theme and Api.

use std::collections::HashSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::api;
use crate::player::Player;

/// A text field of a JSON object ("" when missing).
pub fn s(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

pub fn num(value: &Value, key: &str) -> f64 {
    value[key].as_f64().unwrap_or(0.0)
}

pub fn list(value: &Value, key: &str) -> Vec<Value> {
    value[key].as_array().cloned().unwrap_or_default()
}

/// 3:07, or 1:02:03.
pub fn duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0).floor() as u64;
    let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// 42 min, or 1 hr 5 min.
pub fn long_duration(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as u64;
    if minutes < 60 { format!("{minutes} min") } else { format!("{} hr {} min", minutes / 60, minutes % 60) }
}

pub fn encode(text: &str) -> String {
    js_sys::encode_uri_component(text).as_string().unwrap_or_default()
}

// Addresses of pages, the same names the Qt app uses.
pub fn album_href(id: &str) -> String {
    format!("/album?id={}", encode(id))
}
pub fn artist_href(id: &str) -> String {
    format!("/artist?id={}", encode(id))
}
pub fn playlist_href(id: &str) -> String {
    format!("/playlist?id={}", encode(id))
}
pub fn chart_href(kind: &str, id: &str) -> String {
    format!("/chart?kind={}&id={}", encode(kind), encode(id))
}
pub fn radio_href(station: &str) -> String {
    if station.is_empty() { "/radio".to_owned() } else { format!("/radio?station={}", encode(station)) }
}
pub fn service_href(id: &str) -> String {
    format!("/service?id={}", encode(id))
}
pub fn bandcamp_tag_href(tag: &str) -> String {
    format!("/bandcamp/tag?tag={}", encode(tag))
}
pub fn search_href(query: &str) -> String {
    format!("/search?q={}", encode(query))
}

/// Where a song lives on the web: its YouTube video (at its chapter), or its
/// page on Bandcamp, SoundCloud, Mixcloud or the Internet Archive.
pub fn song_page(track: &Value, found: Option<&String>) -> String {
    if track.is_null() {
        return String::new();
    }
    let page = s(track, "pageUrl");
    if !page.is_empty() {
        return page;
    }
    let video = track["youtubeId"].as_str().map(str::to_owned).or_else(|| found.cloned());
    if let Some(video) = video {
        let start = num(track, "start");
        return format!("https://www.youtube.com/watch?v={video}{}", if start > 0.0 { format!("&t={}s", start.floor()) } else { String::new() });
    }
    let key = s(track, "key");
    if let Some(id) = key.strip_prefix("dz:") {
        return format!("https://www.deezer.com/track/{id}");
    }
    String::new()
}

pub fn album_page(album: &Value) -> String {
    let page = s(album, "pageUrl");
    if !page.is_empty() {
        return page;
    }
    let id = s(album, "id");
    if let Some(list) = id.strip_prefix("yt:pl:") {
        return format!("https://www.youtube.com/playlist?list={list}");
    }
    if let Some(video) = id.strip_prefix("yt:v:") {
        return format!("https://www.youtube.com/watch?v={video}");
    }
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
        return format!("https://www.deezer.com/album/{id}");
    }
    String::new()
}

/// "YouTube", "Bandcamp"… for "Open on …".
pub fn site_name(url: &str) -> &'static str {
    for (host, name) in [
        ("youtube.com", "YouTube"),
        ("soundcloud.com", "SoundCloud"),
        ("bandcamp.com", "Bandcamp"),
        ("mixcloud.com", "Mixcloud"),
        ("archive.org", "the Internet Archive"),
        ("deezer.com", "Deezer"),
    ] {
        if url.contains(host) {
            return name;
        }
    }
    "the web"
}

pub fn window() -> web_sys::Window {
    web_sys::window().expect("window")
}

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

/// A setting kept in the browser between visits.
pub fn stored<T: serde::de::DeserializeOwned>(key: &str, default: T) -> T {
    storage()
        .and_then(|storage| storage.get_item(&format!("earshot.{key}")).ok().flatten())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(default)
}

pub fn store<T: serde::Serialize>(key: &str, value: &T) {
    if let (Some(storage), Ok(text)) = (storage(), serde_json::to_string(value)) {
        let _ = storage.set_item(&format!("earshot.{key}"), &text);
    }
}

/// A signal saved to the browser whenever it changes.
pub fn persisted<T>(key: &'static str, default: T) -> RwSignal<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
{
    let signal = RwSignal::new(stored(key, default));
    Effect::new(move |_| store(key, &signal.get()));
    signal
}

/// The song menu (Play next, Add to playlist…) and what it acts on.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuRequest {
    pub tracks: Vec<Value>,
    pub x: f64,
    pub y: f64,
    pub playlist_id: String,
    pub playlist_index: i64,
    pub playlist_length: usize,
    pub queue_index: i64,
}

impl MenuRequest {
    pub fn new(tracks: Vec<Value>, x: f64, y: f64) -> Self {
        Self { tracks, x, y, playlist_id: String::new(), playlist_index: -1, playlist_length: 0, queue_index: -1 }
    }
}

/// A question with a text field: a new playlist's name, a link to import…
#[derive(Clone, Debug, PartialEq)]
pub enum Prompt {
    NewPlaylist(Vec<Value>),
    Import,
    Rename(String, String),
}

#[derive(Clone, Copy)]
pub struct App {
    /// Moves on whenever the library changes (here, in another tab or in the Qt app).
    pub revision: RwSignal<u32>,
    /// { playlists: [summaries], albums: [saved albums] }
    pub library: RwSignal<Value>,
    pub liked: RwSignal<HashSet<String>>,
    /// Albums as a list instead of a grid of covers.
    pub album_list: RwSignal<bool>,
    /// "always", "background" or "never" (the default).
    pub now_playing_card: RwSignal<String>,
    /// Notices as the system's notifications instead of inside the page (off by default).
    pub desktop_notices: RwSignal<bool>,
    /// YouTube searches list music (songs, music videos, albums) before other videos.
    pub prefer_music: RwSignal<bool>,
    pub recent_searches: RwSignal<Vec<String>>,
    pub toast: RwSignal<Option<(u32, String)>>,
    /// Artwork shown large over the page (a cover was clicked).
    pub art: RwSignal<Option<String>>,
    pub menu: RwSignal<Option<MenuRequest>>,
    pub prompt: RwSignal<Option<Prompt>>,
    pub sidebar_width: RwSignal<f64>,
    /// Radio stations (they never change while running).
    pub stations: RwSignal<Value>,
    pub player: Player,
    navigate: StoredValue<Option<Callback<String>>>,
    /// The search box's text, kept so it matches the search page.
    pub query: RwSignal<String>,
    /// Tags seen on Bandcamp albums opened this visit ({slug, name}), for the tag filter.
    pub seen_bandcamp_tags: RwSignal<Vec<Value>>,
}

pub fn app() -> App {
    expect_context::<App>()
}

impl App {
    pub fn new() -> Self {
        let app = Self {
            revision: RwSignal::new(0),
            library: RwSignal::new(json!({ "playlists": [], "albums": [] })),
            liked: RwSignal::new(HashSet::new()),
            album_list: persisted("albumList", false),
            now_playing_card: persisted("songCard", "never".to_owned()),
            desktop_notices: persisted("desktopNotices", false),
            prefer_music: persisted("preferMusic", true),
            recent_searches: persisted("recentSearches", Vec::new()),
            toast: RwSignal::new(None),
            art: RwSignal::new(None),
            menu: RwSignal::new(None),
            prompt: RwSignal::new(None),
            sidebar_width: persisted("sidebarWidth", 248.0),
            stations: RwSignal::new(Value::Null),
            player: Player::new(),
            navigate: StoredValue::new(None),
            query: RwSignal::new(String::new()),
            seen_bandcamp_tags: RwSignal::new(Vec::new()),
        };
        // The library, again whenever it changes.
        Effect::new(move |_| {
            app.revision.track();
            spawn_local(async move {
                if let Ok(library) = api::call("library", json!({})).await {
                    app.library.set(library);
                }
                if let Ok(liked) = api::call("playlist", json!({ "id": "liked" })).await {
                    app.liked.set(list(&liked, "tracks").iter().map(|t| s(t, "key")).collect());
                }
            });
        });
        spawn_local(async move {
            if let Ok(stations) = api::call("radioStations", json!({})).await {
                app.stations.set(stations);
            }
        });
        app.listen_for_changes();
        app.check_for_update();
        app
    }

    /// The server says when the library changes, so every window stays current.
    fn listen_for_changes(self) {
        let Ok(source) = web_sys::EventSource::new("/api/events") else { return };
        let revision = self.revision;
        let on_change = Closure::<dyn Fn(web_sys::Event)>::new(move |_| revision.update(|r| *r += 1));
        let _ = source.add_event_listener_with_callback("library", on_change.as_ref().unchecked_ref());
        on_change.forget();
        // Connecting again (the server restarted, perhaps updated), or coming
        // back to the page: see whether Earshot was updated meanwhile.
        let on_open = Closure::<dyn Fn(web_sys::Event)>::new(move |_| self.check_for_update());
        let _ = source.add_event_listener_with_callback("open", on_open.as_ref().unchecked_ref());
        let _ = window().document().map(|d| d.add_event_listener_with_callback("visibilitychange", on_open.as_ref().unchecked_ref()));
        on_open.forget();
        std::mem::forget(source);
    }

    /// A page left open keeps running the version it loaded. When the server
    /// gives out a newer one, reload at once if nothing is playing; otherwise
    /// say so, once.
    fn check_for_update(self) {
        thread_local! {
            static LOADED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
            static TOLD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        spawn_local(async move {
            let Ok(response) = gloo_net::http::Request::get("/api/version").send().await else { return };
            if !response.ok() {
                return;
            }
            let Ok(version) = response.text().await else { return };
            let changed = LOADED.with(|loaded| {
                let mut loaded = loaded.borrow_mut();
                match loaded.as_deref() {
                    None => {
                        *loaded = Some(version.clone());
                        false
                    }
                    Some(known) => known != version,
                }
            });
            if !changed {
                return;
            }
            if self.player.engine.state.get_untracked() != 1 {
                let _ = window().location().reload();
            } else if !TOLD.with(|told| told.replace(true)) {
                self.notify("Earshot has been updated: reload this page when it suits you to get the new version.");
            }
        });
    }

    pub fn set_navigate(&self, navigate: Callback<String>) {
        self.navigate.set_value(Some(navigate));
    }

    pub fn go(&self, path: impl Into<String>) {
        if let Some(navigate) = self.navigate.get_value() {
            navigate.run(path.into());
        }
    }

    /// A short notice: the system's own notification where the browser allows
    /// it (asked on the first click), otherwise a notice inside the page.
    pub fn notify(&self, message: impl Into<String>) {
        let message = message.into();
        if self.desktop_notices.get_untracked() && web_sys::Notification::permission() == web_sys::NotificationPermission::Granted {
            let options = web_sys::NotificationOptions::new();
            options.set_body(&message);
            options.set_icon("/earshot-256.png");
            options.set_tag("earshot-notice");
            options.set_silent(Some(true));
            if web_sys::Notification::new_with_options("Earshot", &options).is_ok() {
                return;
            }
        }
        let id = self.toast.with_untracked(|t| t.as_ref().map(|(id, _)| id + 1).unwrap_or(1));
        self.toast.set(Some((id, message)));
        let toast = self.toast;
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3200).await;
            if toast.with_untracked(|t| t.as_ref().map(|(current, _)| *current) == Some(id)) {
                toast.set(None);
            }
        });
    }

    /// A library change: tell the server, then show the result.
    pub fn change(&self, method: &'static str, args: Value) {
        let revision = self.revision;
        spawn_local(async move {
            let _ = api::call(method, args).await;
            revision.update(|r| *r += 1);
        });
    }

    pub fn is_liked(&self, key: &str) -> bool {
        self.liked.with(|liked| liked.contains(key))
    }

    pub fn set_liked(&self, track: &Value, liked: bool) {
        let key = s(track, "key");
        self.liked.update(|set| {
            if liked {
                set.insert(key);
            } else {
                set.remove(&key);
            }
        });
        self.change("setLiked", json!({ "track": track, "liked": liked }));
    }

    pub fn playlists(&self) -> Vec<Value> {
        self.library.with(|library| list(library, "playlists"))
    }

    pub fn saved_albums(&self) -> Vec<Value> {
        self.library.with(|library| list(library, "albums"))
    }

    pub fn is_album_saved(&self, id: &str) -> bool {
        self.library.with(|library| library["albums"].as_array().is_some_and(|albums| albums.iter().any(|a| s(a, "id") == id)))
    }

    pub fn add_to_playlist(&self, id: String, name: String, tracks: Vec<Value>) {
        let app = *self;
        spawn_local(async move {
            let added = api::call("addToPlaylist", json!({ "id": id, "tracks": tracks })).await.ok().and_then(|v| v.as_u64()).unwrap_or(0);
            app.revision.update(|r| *r += 1);
            app.notify(match added {
                0 => format!("Already in {name}"),
                1 => format!("Added 1 song to {name}"),
                n => format!("Added {n} songs to {name}"),
            });
        });
    }

    /// Fetches an album's songs and plays them from the top (or shuffled).
    pub fn play_album(&self, id: String, title: String, shuffle: bool) {
        let app = *self;
        app.notify(format!("Loading {}…", if title.is_empty() { "album" } else { &title }));
        spawn_local(async move {
            match api::call("album", json!({ "id": id })).await {
                Ok(album) => {
                    let route = album_href(&id);
                    let tracks = list(&album, "tracks");
                    if shuffle {
                        app.player.shuffle_tracks(tracks, s(&album, "title"), Some(route));
                    } else {
                        app.player.play_tracks(tracks, 0, s(&album, "title"), Some(route));
                    }
                }
                Err(message) => app.notify(message),
            }
        });
    }

    pub fn remember_bandcamp_tags(&self, tags: &[Value]) {
        self.seen_bandcamp_tags.update(|seen| {
            for tag in tags {
                if !seen.iter().any(|known| known["slug"] == tag["slug"]) {
                    seen.push(tag.clone());
                }
            }
        });
    }

    pub fn add_search(&self, query: &str) {
        let query = query.trim().to_owned();
        if query.chars().count() < 2 {
            return;
        }
        let lower = query.to_lowercase();
        self.recent_searches.update(|items| {
            // A longer search replaces the half-typed one it grew from.
            items.retain(|item| item.to_lowercase() != lower && !lower.starts_with(&item.to_lowercase()));
            items.insert(0, query);
            items.truncate(30);
        });
    }
}

pub fn open_web(url: &str) {
    if !url.is_empty() {
        let _ = window().open_with_url_and_target(url, "_blank");
    }
}

/// The biggest versions of a cover the service has, biggest first (the art
/// viewer tries each in turn): YouTube's maxres thumbnail, Bandcamp's
/// 1200-pixel art, YouTube Music's and Deezer's 1200/1000-pixel covers…
pub fn big_art(url: &str) -> Vec<String> {
    let mut sizes = Vec::new();
    let swap_size = |url: &str, marker: &str, new: &str| -> Option<String> {
        // Replaces the "WxH" (or "tWxH") after `marker` with `new`.
        let at = url.find(marker)? + marker.len();
        let end = at + url[at..].find(|c: char| !(c.is_ascii_digit() || c == 'x' || c == 't'))?;
        (end > at).then(|| format!("{}{}{}", &url[..at], new, &url[end..]))
    };
    if let Some(at) = url.find("ytimg.com/vi") {
        let start = at + url[at..].find("/vi").unwrap_or(0);
        let rest = &url[start..];
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 3 {
            let base = format!("{}/vi/{}/", &url[..start], parts[2]);
            for name in ["maxresdefault.jpg", "sddefault.jpg", "hqdefault.jpg"] {
                sizes.push(format!("{base}{name}"));
            }
        }
    } else if url.contains("bcbits.com/img/") && url.ends_with(".jpg") {
        if let Some(underscore) = url.rfind('_') {
            sizes.push(format!("{}_10.jpg", &url[..underscore]));
        }
    } else if url.contains("dzcdn.net/images/") {
        // ".../250x250-000000-80-0-0.jpg"
        let at = url.rfind('/').map(|i| i + 1).unwrap_or(0);
        if let Some(dash) = url[at..].find('-') {
            sizes.push(format!("{}1000x1000{}", &url[..at], &url[at + dash..]));
        }
    } else if url.contains("sndcdn.com") {
        for small in ["-large.", "-t300x300.", "-t200x200.", "-small.", "-crop."] {
            if url.contains(small) {
                sizes.push(url.replacen(small, "-t500x500.", 1));
            }
        }
    } else if url.contains("googleusercontent.com") || url.contains("ggpht.com") {
        if let Some(at) = url.rfind("=w") {
            sizes.push(format!("{}=w1200-h1200{}", &url[..at], url[at..].find("-l").map(|i| &url[at + i..]).unwrap_or("")));
        }
    } else if let Some(bigger) = swap_size(url, "thumbnailer.mixcloud.com/unsafe/", "1000x1000") {
        sizes.push(bigger);
    }
    sizes.push(url.to_owned());
    sizes
}
