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
        fn search_youtube(self: Pin<&mut Backend>, query: &QString) -> i32;
        #[qinvokable]
        fn search_youtube_albums(self: Pin<&mut Backend>, query: &QString) -> i32;
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
use crate::{catalog, radio, youtube};

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

    pub fn search_youtube(self: Pin<&mut Self>, query: &QString) -> i32 {
        let query = query.to_string();
        self.spawn(move || youtube::search(&query, 25).map(|tracks| json!({ "tracks": tracks })))
    }

    pub fn search_youtube_albums(self: Pin<&mut Self>, query: &QString) -> i32 {
        let query = query.to_string();
        self.spawn(move || youtube::search_albums(&query))
    }

    pub fn album(self: Pin<&mut Self>, id: &QString) -> i32 {
        let id = id.to_string();
        self.spawn(move || if id.starts_with("yt:") { youtube::album(&id) } else { catalog::album_detail(&id) })
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
        self.spawn(move || youtube::playlist(&url))
    }

    /// Which desktop the app runs on, to draw matching window buttons:
    /// "windows", "mac", "gnome", "kde" or "other".
    pub fn desktop(&self) -> QString {
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
