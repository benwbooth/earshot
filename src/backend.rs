//! The bridge between the QML interface and the Rust side. Network work runs
//! on worker threads; each request returns an id right away and its answer
//! arrives later through `reply` or `failed`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, library_revision)]
        #[qproperty(bool, can_play)]
        type Backend = super::BackendRust;

        #[qsignal]
        fn reply(self: Pin<&mut Backend>, request: i32, json: QString);
        #[qsignal]
        fn failed(self: Pin<&mut Backend>, request: i32, message: QString);

        #[qinvokable]
        fn home(self: Pin<&mut Backend>) -> i32;
        #[qinvokable]
        fn genre(self: Pin<&mut Backend>, id: &QString) -> i32;
        #[qinvokable]
        fn era(self: Pin<&mut Backend>, id: &QString) -> i32;
        #[qinvokable]
        fn search(self: Pin<&mut Backend>, query: &QString) -> i32;
        #[qinvokable]
        fn search_youtube_albums(self: Pin<&mut Backend>, query: &QString, next: &QString) -> i32;
        #[qinvokable]
        fn search_bandcamp(self: Pin<&mut Backend>, query: &QString) -> i32;
        #[qinvokable]
        fn search_soundcloud(self: Pin<&mut Backend>, query: &QString) -> i32;
        #[qinvokable]
        fn search_mixcloud(self: Pin<&mut Backend>, query: &QString) -> i32;
        #[qinvokable]
        fn search_archive(self: Pin<&mut Backend>, query: &QString) -> i32;
        #[qinvokable]
        fn browse(self: Pin<&mut Backend>, service: &QString, tag: &QString) -> i32;
        #[qinvokable]
        fn bandcamp_tags(self: Pin<&mut Backend>) -> i32;
        #[qinvokable]
        fn bandcamp_tag(self: Pin<&mut Backend>, tag: &QString, slice: &QString, next: &QString) -> i32;
        #[qinvokable]
        fn stream(self: Pin<&mut Backend>, track: &QString) -> i32;
        #[qinvokable]
        fn radio_more(self: Pin<&mut Backend>, track: &QString, video: &QString) -> i32;
        #[qinvokable]
        fn record_play(self: Pin<&mut Backend>, track: &QString, video: &QString);
        #[qinvokable]
        fn for_you(self: Pin<&mut Backend>, refresh: bool) -> i32;
        #[qinvokable]
        fn album(self: Pin<&mut Backend>, id: &QString) -> i32;
        #[qinvokable]
        fn artist(self: Pin<&mut Backend>, id: &QString) -> i32;
        #[qinvokable]
        fn radio(self: Pin<&mut Backend>, subreddit: &QString) -> i32;
        #[qinvokable]
        fn radio_stations(self: &Backend) -> QString;
        #[qinvokable]
        fn resolve(self: Pin<&mut Backend>, track: &QString) -> i32;
        #[qinvokable]
        fn import_playlist(self: Pin<&mut Backend>, url: &QString) -> i32;

        #[qinvokable]
        fn remember_video(self: Pin<&mut Backend>, key: &QString, video: &QString);
        #[qinvokable]
        fn desktop(self: &Backend) -> QString;
        #[qinvokable]
        fn remember_window_with_kde(self: &Backend) -> bool;
        #[qinvokable]
        fn set_waveform_active(self: &Backend, active: bool);
        #[qinvokable]
        fn waveform_levels(self: &Backend) -> QString;

        #[qinvokable]
        fn playlists(self: &Backend) -> QString;
        #[qinvokable]
        fn playlist(self: &Backend, id: &QString) -> QString;
        #[qinvokable]
        fn create_playlist(self: Pin<&mut Backend>, name: &QString, tracks: &QString) -> QString;
        #[qinvokable]
        fn rename_playlist(self: Pin<&mut Backend>, id: &QString, name: &QString);
        #[qinvokable]
        fn delete_playlist(self: Pin<&mut Backend>, id: &QString);
        #[qinvokable]
        fn add_to_playlist(self: Pin<&mut Backend>, id: &QString, tracks: &QString) -> i32;
        #[qinvokable]
        fn remove_from_playlist(self: Pin<&mut Backend>, id: &QString, index: i32);
        #[qinvokable]
        fn move_in_playlist(self: Pin<&mut Backend>, id: &QString, from: i32, to: i32);
        #[qinvokable]
        fn is_liked(self: &Backend, key: &QString) -> bool;
        #[qinvokable]
        fn set_liked(self: Pin<&mut Backend>, track: &QString, liked: bool);
        #[qinvokable]
        fn saved_albums(self: &Backend) -> QString;
        #[qinvokable]
        fn is_album_saved(self: &Backend, id: &QString) -> bool;
        #[qinvokable]
        fn set_album_saved(self: Pin<&mut Backend>, album: &QString, saved: bool);
    }

    impl cxx_qt::Threading for Backend {}
}

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::{Value, json};

use crate::library::Library;
use crate::{archive, bandcamp, catalog, foryou, mixcloud, radio, soundcloud, youtube};

pub struct BackendRust {
    library_revision: i32,
    can_play: bool,
    next_request: i32,
    library: Arc<Mutex<Library>>,
}

impl Default for BackendRust {
    fn default() -> Self {
        Self {
            library_revision: 0,
            can_play: true,
            next_request: 0,
            library: Arc::new(Mutex::new(Library::load())),
        }
    }
}

fn parse(text: &QString) -> Value {
    serde_json::from_str(&text.to_string()).unwrap_or(Value::Null)
}

fn parse_list(text: &QString) -> Vec<Value> {
    match parse(text) {
        Value::Array(items) => items,
        Value::Null => Vec::new(),
        single => vec![single],
    }
}

impl qobject::Backend {
    fn spawn<F>(mut self: Pin<&mut Self>, job: F) -> i32
    where
        F: FnOnce() -> Result<Value, String> + Send + 'static,
    {
        let id = {
            let mut rust = self.as_mut().rust_mut();
            rust.next_request += 1;
            rust.next_request
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = job();
            let _ = thread.queue(move |backend| match result {
                Ok(value) => backend.reply(id, QString::from(&value.to_string())),
                Err(message) => backend.failed(id, QString::from(&message)),
            });
        });
        id
    }

    fn library_changed(mut self: Pin<&mut Self>) {
        let next = self.library_revision().wrapping_add(1);
        self.as_mut().set_library_revision(next);
    }

    fn library(&self) -> std::sync::MutexGuard<'_, Library> {
        self.rust().library.lock().unwrap()
    }

    pub fn home(self: Pin<&mut Self>) -> i32 {
        self.spawn(catalog::home)
    }

    pub fn genre(self: Pin<&mut Self>, id: &QString) -> i32 {
        let id = id.to_string();
        self.spawn(move || catalog::genre_chart(&id))
    }

    pub fn era(self: Pin<&mut Self>, id: &QString) -> i32 {
        let id = id.to_string();
        self.spawn(move || catalog::era_chart(&id))
    }

    pub fn search(self: Pin<&mut Self>, query: &QString) -> i32 {
        let query = query.to_string();
        self.spawn(move || catalog::search(&query))
    }

    /// One page of YouTube results; `next` is "" for the first page, then the token the previous page returned.
    pub fn search_youtube_albums(self: Pin<&mut Self>, query: &QString, next: &QString) -> i32 {
        let (query, next) = (query.to_string(), next.to_string());
        self.spawn(move || youtube::search_albums(&query, &next))
    }

    pub fn search_bandcamp(self: Pin<&mut Self>, query: &QString) -> i32 {
        let query = query.to_string();
        self.spawn(move || bandcamp::search(&query))
    }

    pub fn search_soundcloud(self: Pin<&mut Self>, query: &QString) -> i32 {
        let query = query.to_string();
        self.spawn(move || soundcloud::search(&query))
    }

    pub fn search_mixcloud(self: Pin<&mut Self>, query: &QString) -> i32 {
        let query = query.to_string();
        self.spawn(move || mixcloud::search(&query))
    }

    pub fn search_archive(self: Pin<&mut Self>, query: &QString) -> i32 {
        let query = query.to_string();
        self.spawn(move || archive::search(&query))
    }

    /// SoundCloud, Mixcloud or the Internet Archive by genre or tag: { albums, tracks }.
    pub fn browse(self: Pin<&mut Self>, service: &QString, tag: &QString) -> i32 {
        let (service, tag) = (service.to_string(), tag.to_string());
        self.spawn(move || match service.as_str() {
            "soundcloud" => soundcloud::browse(&tag),
            "mixcloud" => mixcloud::browse(&tag),
            "archive" => archive::browse(&tag),
            // YouTube's own search for the genre: playlists and full-length videos as albums, then songs.
            "youtube" => youtube::search_albums(&format!("{tag} music"), ""),
            other => Err(format!("Can't browse {other}")),
        })
    }

    /// Bandcamp's genres and sub-genres, for browsing.
    pub fn bandcamp_tags(self: Pin<&mut Self>) -> i32 {
        self.spawn(bandcamp::tags)
    }

    /// One page of albums with a Bandcamp tag; `slice` is top, new or rand.
    pub fn bandcamp_tag(self: Pin<&mut Self>, tag: &QString, slice: &QString, next: &QString) -> i32 {
        let (tag, slice, next) = (bandcamp::tag_slug(&tag.to_string()), slice.to_string(), next.to_string());
        self.spawn(move || bandcamp::tag(&tag, &slice, &next))
    }

    /// Random radio: songs like `track`, from its own service where that has
    /// suggestions (SoundCloud's related tracks, Mixcloud's similar shows,
    /// Bandcamp's recommended albums, the Internet Archive's other recordings
    /// by the artist), otherwise YouTube's mix for its video.
    pub fn radio_more(self: Pin<&mut Self>, track: &QString, video: &QString) -> i32 {
        let (track, video) = (parse(track), video.to_string());
        self.spawn(move || {
            let own = match track["service"].as_str() {
                Some("soundcloud") => track["soundcloudId"].as_u64().map(soundcloud::related),
                Some("mixcloud") => track["mixcloudKey"].as_str().map(mixcloud::similar),
                Some("bandcamp") => Some(bandcamp::related(&track)),
                Some("archive") => Some(archive::related(&track)),
                _ => None,
            };
            if let Some(Ok(tracks)) = own {
                if !tracks.is_empty() {
                    return Ok(json!({ "tracks": tracks }));
                }
            }
            let seed = if !video.is_empty() {
                video
            } else if let Some(id) = track["youtubeId"].as_str() {
                id.to_owned()
            } else {
                let query = format!("{} - {}", track["artist"].as_str().unwrap_or(""), track["title"].as_str().unwrap_or(""));
                youtube::search(&query, 1)?
                    .first()
                    .and_then(|found| found["youtubeId"].as_str().map(str::to_owned))
                    .ok_or("Couldn't find songs like this one.")?
            };
            Ok(json!({ "tracks": youtube::mix(&seed)? }))
        })
    }

    /// Notes a song as listened to (for Made for you); kept on this computer only.
    pub fn record_play(mut self: Pin<&mut Self>, track: &QString, video: &QString) {
        {
            let mut library = self.library();
            library.record_play(&parse(track), &video.to_string());
            library.save();
        }
        self.as_mut().library_changed();
    }

    /// Discover Weekly, Daily Mixes, Release Radar and Albums for you.
    pub fn for_you(self: Pin<&mut Self>, refresh: bool) -> i32 {
        let history = self.library().history.clone();
        self.spawn(move || foryou::build(&history, refresh))
    }

    /// A Bandcamp song's audio link, fetched fresh since Bandcamp's links expire.
    pub fn stream(self: Pin<&mut Self>, track: &QString) -> i32 {
        let track = parse(track);
        self.spawn(move || {
            let url = bandcamp::stream(&track)?;
            Ok(json!({ "key": track["key"], "url": url }))
        })
    }

    pub fn album(self: Pin<&mut Self>, id: &QString) -> i32 {
        let id = id.to_string();
        self.spawn(move || match id.split(':').next() {
            Some("yt") => youtube::album(&id),
            Some("bc") => bandcamp::album(&id),
            Some("sc") => soundcloud::album(&id),
            Some("ia") => archive::album(&id),
            _ => catalog::album_detail(&id),
        })
    }

    pub fn artist(self: Pin<&mut Self>, id: &QString) -> i32 {
        let id = id.to_string();
        self.spawn(move || catalog::artist_detail(&id))
    }

    pub fn radio(self: Pin<&mut Self>, subreddit: &QString) -> i32 {
        let subreddit = subreddit.to_string();
        self.spawn(move || radio::station(&subreddit))
    }

    pub fn radio_stations(&self) -> QString {
        QString::from(&radio::stations().to_string())
    }

    pub fn resolve(self: Pin<&mut Self>, track: &QString) -> i32 {
        let track = parse(track);
        let key = track["key"].as_str().unwrap_or("").to_owned();
        let library = self.rust().library.clone();
        self.spawn(move || {
            let known = library.lock().unwrap().videos.get(&key).cloned();
            let videos = youtube::find_videos(&track, known)?;
            Ok(json!({ "key": key, "videos": videos }))
        })
    }

    /// Remembers which video played for a track, so it's found instantly next time.
    pub fn remember_video(self: Pin<&mut Self>, key: &QString, video: &QString) {
        self.library().remember_video(&key.to_string(), &video.to_string());
    }

    pub fn import_playlist(self: Pin<&mut Self>, url: &QString) -> i32 {
        let url = url.to_string().trim().to_owned();
        self.spawn(move || {
            if url.contains("soundcloud.com/") {
                soundcloud::import(&url)
            } else if url.contains("bandcamp.com/") {
                bandcamp::import(&url)
            } else if url.contains("mixcloud.com/") {
                mixcloud::import(&url)
            } else if url.contains("archive.org/") {
                archive::import(&url)
            } else {
                youtube::playlist(&url)
            }
        })
    }

    /// Listens to the speakers while music plays, for the waveform.
    pub fn set_waveform_active(&self, active: bool) {
        crate::visualizer::set_active(active);
    }

    /// The six waveform bar heights (0..1, bass first), comma-separated.
    pub fn waveform_levels(&self) -> QString {
        let text = crate::visualizer::levels().iter().map(|level| format!("{level:.3}")).collect::<Vec<_>>().join(",");
        QString::from(&text)
    }

    /// On KDE Plasma (Wayland), where apps can't place their own windows: adds
    /// a window rule so KDE remembers where Earshot's window was. True the first
    /// time, when the rule is added. Not in Flatpak, whose sandbox can't (and
    /// shouldn't) change KDE's settings.
    pub fn remember_window_with_kde(&self) -> bool {
        if std::env::var_os("FLATPAK_ID").is_some() {
            return false;
        }
        match crate::kde::add_window_rule("earshot") {
            Ok(true) => {
                crate::native::reload_kwin_rules();
                true
            }
            Ok(false) => false,
            Err(error) => {
                eprintln!("{error}");
                false
            }
        }
    }

    /// Which desktop the app runs on, to draw matching window buttons:
    /// "windows", "mac", "gnome", "kde" or "other".
    pub fn desktop(&self) -> QString {
        if let Ok(forced) = std::env::var("EARSHOT_WINDOW_BUTTONS") {
            return QString::from(&forced);
        }
        let name = if cfg!(target_os = "windows") {
            "windows".to_owned()
        } else if cfg!(target_os = "macos") {
            "mac".to_owned()
        } else {
            let session = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
            if session.contains("kde") || session.contains("plasma") || session.contains("lxqt") {
                "kde".to_owned()
            } else if session.contains("gnome") || session.contains("unity") || session.contains("pantheon") || session.contains("cinnamon") || session.contains("budgie") {
                "gnome".to_owned()
            } else {
                "other".to_owned()
            }
        };
        QString::from(&name)
    }

    pub fn playlists(&self) -> QString {
        QString::from(&self.library().playlists_json().to_string())
    }

    pub fn playlist(&self, id: &QString) -> QString {
        QString::from(&self.library().playlist_json(&id.to_string()).to_string())
    }

    pub fn create_playlist(mut self: Pin<&mut Self>, name: &QString, tracks: &QString) -> QString {
        let id = {
            let mut library = self.library();
            let id = library.create(&name.to_string());
            library.add(&id, parse_list(tracks));
            id
        };
        self.as_mut().library_changed();
        QString::from(&id)
    }

    pub fn rename_playlist(mut self: Pin<&mut Self>, id: &QString, name: &QString) {
        self.library().rename(&id.to_string(), &name.to_string());
        self.as_mut().library_changed();
    }

    pub fn delete_playlist(mut self: Pin<&mut Self>, id: &QString) {
        self.library().delete(&id.to_string());
        self.as_mut().library_changed();
    }

    pub fn add_to_playlist(mut self: Pin<&mut Self>, id: &QString, tracks: &QString) -> i32 {
        let added = self.library().add(&id.to_string(), parse_list(tracks));
        self.as_mut().library_changed();
        added as i32
    }

    pub fn remove_from_playlist(mut self: Pin<&mut Self>, id: &QString, index: i32) {
        if index >= 0 {
            self.library().remove(&id.to_string(), index as usize);
            self.as_mut().library_changed();
        }
    }

    pub fn move_in_playlist(mut self: Pin<&mut Self>, id: &QString, from: i32, to: i32) {
        if from >= 0 && to >= 0 {
            self.library().move_track(&id.to_string(), from as usize, to as usize);
            self.as_mut().library_changed();
        }
    }

    pub fn is_liked(&self, key: &QString) -> bool {
        self.library().is_liked(&key.to_string())
    }

    pub fn set_liked(mut self: Pin<&mut Self>, track: &QString, liked: bool) {
        self.library().set_liked(parse(track), liked);
        self.as_mut().library_changed();
    }

    pub fn saved_albums(&self) -> QString {
        QString::from(&json!(self.library().albums).to_string())
    }

    pub fn is_album_saved(&self, id: &QString) -> bool {
        self.library().is_album_saved(&id.to_string())
    }

    pub fn set_album_saved(mut self: Pin<&mut Self>, album: &QString, saved: bool) {
        self.library().set_album_saved(parse(album), saved);
        self.as_mut().library_changed();
    }
}
