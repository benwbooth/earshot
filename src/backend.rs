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
        fn search_youtube_albums(self: Pin<&mut Backend>, query: &QString, next: &QString, prefer_music: bool) -> i32;
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
        fn record_play(self: &Backend, track: &QString, video: &QString);
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
        fn call(self: Pin<&mut Backend>, method: &QString, args: &QString) -> i32;

        #[qinvokable]
        fn remember_video(self: &Backend, key: &QString, video: &QString);
        #[qinvokable]
        fn desktop(self: &Backend) -> QString;
        #[qinvokable]
        fn remember_window_with_kde(self: &Backend) -> bool;
        #[qinvokable]
        fn set_waveform_active(self: &Backend, active: bool);
        #[qinvokable]
        fn waveform_levels(self: &Backend) -> QString;
        #[qinvokable]
        fn open_web_ui(self: Pin<&mut Backend>) -> i32;

        #[qinvokable]
        fn playlists(self: &Backend) -> QString;
        #[qinvokable]
        fn playlist(self: &Backend, id: &QString) -> QString;
        #[qinvokable]
        fn create_playlist(self: &Backend, name: &QString, tracks: &QString) -> QString;
        #[qinvokable]
        fn rename_playlist(self: &Backend, id: &QString, name: &QString);
        #[qinvokable]
        fn delete_playlist(self: &Backend, id: &QString);
        #[qinvokable]
        fn add_to_playlist(self: &Backend, id: &QString, tracks: &QString) -> i32;
        #[qinvokable]
        fn remove_from_playlist(self: &Backend, id: &QString, index: i32);
        #[qinvokable]
        fn move_in_playlist(self: &Backend, id: &QString, from: i32, to: i32);
        #[qinvokable]
        fn is_liked(self: &Backend, key: &QString) -> bool;
        #[qinvokable]
        fn set_liked(self: &Backend, track: &QString, liked: bool);
        #[qinvokable]
        fn saved_albums(self: &Backend) -> QString;
        #[qinvokable]
        fn is_album_saved(self: &Backend, id: &QString) -> bool;
        #[qinvokable]
        fn set_album_saved(self: &Backend, album: &QString, saved: bool);
    }

    impl cxx_qt::Threading for Backend {}
    impl cxx_qt::Initialize for Backend {}
}

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use earshot_core::api::Earshot;
use earshot_core::library::Library;
use serde_json::{Value, json};

pub struct BackendRust {
    library_revision: i32,
    can_play: bool,
    next_request: i32,
    core: Arc<Earshot>,
}

impl Default for BackendRust {
    fn default() -> Self {
        Self {
            library_revision: 0,
            can_play: true,
            next_request: 0,
            core: Arc::new(Earshot::new(Arc::new(Mutex::new(Library::load())))),
        }
    }
}

fn parse(text: &QString) -> Value {
    serde_json::from_str(&text.to_string()).unwrap_or(Value::Null)
}

fn text(value: &QString) -> Value {
    json!(value.to_string())
}

impl cxx_qt::Initialize for qobject::Backend {
    // Library changes, from this window or the web version, move libraryRevision on.
    fn initialize(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        self.rust().core.on_change(move || {
            let _ = thread.queue(|backend| backend.library_changed());
        });
    }
}

impl qobject::Backend {
    /// Asks the shared core on a worker thread; the answer arrives through `reply` or `failed`.
    fn ask(mut self: Pin<&mut Self>, method: &'static str, args: Value) -> i32 {
        let id = {
            let mut rust = self.as_mut().rust_mut();
            rust.next_request += 1;
            rust.next_request
        };
        let core = self.rust().core.clone();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = core.call(method, &args);
            let _ = thread.queue(move |backend| match result {
                Ok(value) => backend.reply(id, QString::from(&value.to_string())),
                Err(message) => backend.failed(id, QString::from(&message)),
            });
        });
        id
    }

    /// Asks the shared core right away (library lookups and changes, which are quick).
    fn now(&self, method: &str, args: Value) -> Value {
        self.rust().core.call(method, &args).unwrap_or(Value::Null)
    }

    fn library_changed(mut self: Pin<&mut Self>) {
        let next = self.library_revision().wrapping_add(1);
        self.as_mut().set_library_revision(next);
    }

    pub fn home(self: Pin<&mut Self>) -> i32 {
        self.ask("home", json!({}))
    }

    pub fn genre(self: Pin<&mut Self>, id: &QString) -> i32 {
        self.ask("genre", json!({ "id": text(id) }))
    }

    pub fn era(self: Pin<&mut Self>, id: &QString) -> i32 {
        self.ask("era", json!({ "id": text(id) }))
    }

    pub fn search(self: Pin<&mut Self>, query: &QString) -> i32 {
        self.ask("search", json!({ "query": text(query) }))
    }

    /// One page of YouTube results; `next` is "" for the first page, then the token the previous page returned.
    /// With `prefer_music`, songs, music videos and albums come before other videos.
    pub fn search_youtube_albums(self: Pin<&mut Self>, query: &QString, next: &QString, prefer_music: bool) -> i32 {
        self.ask("searchYoutubeAlbums", json!({ "query": text(query), "next": text(next), "preferMusic": prefer_music }))
    }

    pub fn search_bandcamp(self: Pin<&mut Self>, query: &QString) -> i32 {
        self.ask("searchBandcamp", json!({ "query": text(query) }))
    }

    pub fn search_soundcloud(self: Pin<&mut Self>, query: &QString) -> i32 {
        self.ask("searchSoundcloud", json!({ "query": text(query) }))
    }

    pub fn search_mixcloud(self: Pin<&mut Self>, query: &QString) -> i32 {
        self.ask("searchMixcloud", json!({ "query": text(query) }))
    }

    pub fn search_archive(self: Pin<&mut Self>, query: &QString) -> i32 {
        self.ask("searchArchive", json!({ "query": text(query) }))
    }

    /// SoundCloud, Mixcloud, the Internet Archive or YouTube by genre or tag: { albums, tracks }.
    pub fn browse(self: Pin<&mut Self>, service: &QString, tag: &QString) -> i32 {
        self.ask("browse", json!({ "service": text(service), "tag": text(tag) }))
    }

    /// Bandcamp's genres and sub-genres, for browsing.
    pub fn bandcamp_tags(self: Pin<&mut Self>) -> i32 {
        self.ask("bandcampTags", json!({}))
    }

    /// One page of albums with a Bandcamp tag; `slice` is top, new or rand.
    pub fn bandcamp_tag(self: Pin<&mut Self>, tag: &QString, slice: &QString, next: &QString) -> i32 {
        self.ask("bandcampTag", json!({ "tag": text(tag), "slice": text(slice), "next": text(next) }))
    }

    /// Random radio: songs like `track` (see `earshot_core::api::radio_more`).
    pub fn radio_more(self: Pin<&mut Self>, track: &QString, video: &QString) -> i32 {
        self.ask("radioMore", json!({ "track": parse(track), "video": text(video) }))
    }

    /// Notes a song as listened to (for Made for you); kept on this computer only.
    pub fn record_play(&self, track: &QString, video: &QString) {
        self.now("recordPlay", json!({ "track": parse(track), "video": text(video) }));
    }

    /// Discover Weekly, Daily Mixes, Release Radar and Albums for you.
    pub fn for_you(self: Pin<&mut Self>, refresh: bool) -> i32 {
        self.ask("forYou", json!({ "refresh": refresh }))
    }

    /// A Bandcamp song's audio link, fetched fresh since Bandcamp's links expire.
    pub fn stream(self: Pin<&mut Self>, track: &QString) -> i32 {
        self.ask("stream", json!({ "track": parse(track) }))
    }

    pub fn album(self: Pin<&mut Self>, id: &QString) -> i32 {
        self.ask("album", json!({ "id": text(id) }))
    }

    pub fn artist(self: Pin<&mut Self>, id: &QString) -> i32 {
        self.ask("artist", json!({ "id": text(id) }))
    }

    pub fn radio(self: Pin<&mut Self>, subreddit: &QString) -> i32 {
        self.ask("radio", json!({ "subreddit": text(subreddit) }))
    }

    pub fn radio_stations(&self) -> QString {
        QString::from(&self.now("radioStations", json!({})).to_string())
    }

    pub fn resolve(self: Pin<&mut Self>, track: &QString) -> i32 {
        self.ask("resolve", json!({ "track": parse(track) }))
    }

    /// Remembers which video played for a track, so it's found instantly next time.
    pub fn remember_video(&self, key: &QString, video: &QString) {
        self.now("rememberVideo", json!({ "key": text(key), "video": text(video) }));
    }

    pub fn import_playlist(self: Pin<&mut Self>, url: &QString) -> i32 {
        self.ask("importPlaylist", json!({ "url": text(url) }))
    }

    /// Any request the shared core answers, by name with JSON arguments (as
    /// the web version asks): newer features need no method of their own here.
    pub fn call(mut self: Pin<&mut Self>, method: &QString, args: &QString) -> i32 {
        let id = {
            let mut rust = self.as_mut().rust_mut();
            rust.next_request += 1;
            rust.next_request
        };
        let (method, args) = (method.to_string(), parse(args));
        let core = self.rust().core.clone();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = core.call(&method, &args);
            let _ = thread.queue(move |backend| match result {
                Ok(value) => backend.reply(id, QString::from(&value.to_string())),
                Err(message) => backend.failed(id, QString::from(&message)),
            });
        });
        id
    }

    /// Starts the web version on this computer (once) and opens it in the
    /// browser; the answer is its address. It shares this window's library.
    pub fn open_web_ui(mut self: Pin<&mut Self>) -> i32 {
        let id = {
            let mut rust = self.as_mut().rust_mut();
            rust.next_request += 1;
            rust.next_request
        };
        let core = self.rust().core.clone();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = earshot_server::start_local(core);
            let _ = thread.queue(move |backend| match result {
                Ok(url) => backend.reply(id, QString::from(&json!({ "url": url }).to_string())),
                Err(message) => backend.failed(id, QString::from(&message)),
            });
        });
        id
    }

    /// Listens to the speakers while music plays, for the waveform.
    pub fn set_waveform_active(&self, active: bool) {
        earshot_speakers::set_active(active);
    }

    /// The six waveform bar heights (0..1, bass first), comma-separated.
    pub fn waveform_levels(&self) -> QString {
        let text = earshot_speakers::levels().iter().map(|level| format!("{level:.3}")).collect::<Vec<_>>().join(",");
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
        QString::from(&self.now("playlists", json!({})).to_string())
    }

    pub fn playlist(&self, id: &QString) -> QString {
        QString::from(&self.now("playlist", json!({ "id": text(id) })).to_string())
    }

    pub fn create_playlist(&self, name: &QString, tracks: &QString) -> QString {
        let id = self.now("createPlaylist", json!({ "name": text(name), "tracks": parse(tracks) }));
        QString::from(id.as_str().unwrap_or(""))
    }

    pub fn rename_playlist(&self, id: &QString, name: &QString) {
        self.now("renamePlaylist", json!({ "id": text(id), "name": text(name) }));
    }

    pub fn delete_playlist(&self, id: &QString) {
        self.now("deletePlaylist", json!({ "id": text(id) }));
    }

    pub fn add_to_playlist(&self, id: &QString, tracks: &QString) -> i32 {
        self.now("addToPlaylist", json!({ "id": text(id), "tracks": parse(tracks) })).as_i64().unwrap_or(0) as i32
    }

    pub fn remove_from_playlist(&self, id: &QString, index: i32) {
        if index >= 0 {
            self.now("removeFromPlaylist", json!({ "id": text(id), "index": index }));
        }
    }

    pub fn move_in_playlist(&self, id: &QString, from: i32, to: i32) {
        if from >= 0 && to >= 0 {
            self.now("moveInPlaylist", json!({ "id": text(id), "from": from, "to": to }));
        }
    }

    pub fn is_liked(&self, key: &QString) -> bool {
        self.now("isLiked", json!({ "key": text(key) })).as_bool().unwrap_or(false)
    }

    pub fn set_liked(&self, track: &QString, liked: bool) {
        self.now("setLiked", json!({ "track": parse(track), "liked": liked }));
    }

    pub fn saved_albums(&self) -> QString {
        QString::from(&self.now("savedAlbums", json!({})).to_string())
    }

    pub fn is_album_saved(&self, id: &QString) -> bool {
        self.now("isAlbumSaved", json!({ "id": text(id) })).as_bool().unwrap_or(false)
    }

    pub fn set_album_saved(&self, album: &QString, saved: bool) {
        self.now("setAlbumSaved", json!({ "album": parse(album), "saved": saved }));
    }
}
