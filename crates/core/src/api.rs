//! One way in for every interface: the Qt app and the web server both ask
//! [`Earshot::call`] by name, with JSON arguments, and get JSON back.

use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{Value, json};

use crate::library::Library;
use crate::{archive, bandcamp, catalog, foryou, mixcloud, radio, soundcloud, youtube};

/// SoundCloud, Mixcloud, the Internet Archive or YouTube by genre or tag: { albums, tracks }.
pub fn browse(service: &str, tag: &str) -> Result<Value, String> {
    match service {
        "soundcloud" => soundcloud::browse(tag),
        "mixcloud" => mixcloud::browse(tag),
        "archive" => archive::browse(tag),
        // YouTube's own search for the genre: playlists and full-length videos as albums, then songs.
        "youtube" => youtube::search_albums(&format!("{tag} music"), ""),
        other => Err(format!("Can't browse {other}")),
    }
}

/// An album, playlist or long video with its songs, by id ("yt:…", "bc:…", "sc:…", "ia:…" or Deezer's).
pub fn album(id: &str) -> Result<Value, String> {
    match id.split(':').next() {
        Some("yt") => youtube::album(id),
        Some("bc") => bandcamp::album(id),
        Some("sc") => soundcloud::album(id),
        Some("ia") => archive::album(id),
        _ => catalog::album_detail(id),
    }
}

/// A playlist from a link to YouTube, SoundCloud, Bandcamp, Mixcloud or the Internet Archive.
pub fn import_playlist(url: &str) -> Result<Value, String> {
    let url = url.trim();
    if url.contains("soundcloud.com/") {
        soundcloud::import(url)
    } else if url.contains("bandcamp.com/") {
        bandcamp::import(url)
    } else if url.contains("mixcloud.com/") {
        mixcloud::import(url)
    } else if url.contains("archive.org/") {
        archive::import(url)
    } else {
        youtube::playlist(url)
    }
}

/// Random radio: songs like `track`, from its own service where that has
/// suggestions (SoundCloud's related tracks, Mixcloud's similar shows,
/// Bandcamp's recommended albums, the Internet Archive's other recordings
/// by the artist), otherwise YouTube's mix for its video.
pub fn radio_more(track: &Value, video: &str) -> Result<Value, String> {
    let own = match track["service"].as_str() {
        Some("soundcloud") => track["soundcloudId"].as_u64().map(soundcloud::related),
        Some("mixcloud") => track["mixcloudKey"].as_str().map(mixcloud::similar),
        Some("bandcamp") => Some(bandcamp::related(track)),
        Some("archive") => Some(archive::related(track)),
        _ => None,
    };
    if let Some(Ok(tracks)) = own {
        if !tracks.is_empty() {
            return Ok(json!({ "tracks": tracks }));
        }
    }
    let seed = if !video.is_empty() {
        video.to_owned()
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
}

/// A Bandcamp song's audio link, fetched fresh since Bandcamp's links expire.
pub fn stream(track: &Value) -> Result<Value, String> {
    let url = bandcamp::stream(track)?;
    Ok(json!({ "key": track["key"], "url": url }))
}

fn list(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Null => Vec::new(),
        // The Qt side passes lists as JSON text.
        Value::String(text) => match serde_json::from_str(text) {
            Ok(Value::Array(items)) => items,
            Ok(Value::Null) | Err(_) => Vec::new(),
            Ok(single) => vec![single],
        },
        single => vec![single.clone()],
    }
}

/// The music services plus the listener's library.
pub struct Earshot {
    library: Arc<Mutex<Library>>,
    on_change: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}

impl Earshot {
    pub fn new(library: Arc<Mutex<Library>>) -> Self {
        Self { library, on_change: Mutex::new(Vec::new()) }
    }

    pub fn library(&self) -> MutexGuard<'_, Library> {
        self.library.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn shared_library(&self) -> Arc<Mutex<Library>> {
        self.library.clone()
    }

    /// Runs `listener` whenever playlists, liked songs, saved albums or the
    /// listening history change through [`Earshot::call`].
    pub fn on_change(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.on_change.lock().unwrap().push(Box::new(listener));
    }

    fn changed(&self) {
        for listener in self.on_change.lock().unwrap().iter() {
            listener();
        }
    }

    /// Answers one request by name. Slow ones (the network) block, so call
    /// this from a worker thread.
    pub fn call(&self, method: &str, args: &Value) -> Result<Value, String> {
        let text = |name: &str| args[name].as_str().unwrap_or("").to_owned();
        let changes = |result: Value| {
            self.changed();
            Ok(result)
        };
        match method {
            "home" => catalog::home(),
            "genre" => catalog::genre_chart(&text("id")),
            "era" => catalog::era_chart(&text("id")),
            "search" => catalog::search(&text("query")),
            "searchYoutubeAlbums" => youtube::search_albums(&text("query"), &text("next")),
            "searchBandcamp" => bandcamp::search(&text("query")),
            "searchSoundcloud" => soundcloud::search(&text("query")),
            "searchMixcloud" => mixcloud::search(&text("query")),
            "searchArchive" => archive::search(&text("query")),
            "browse" => browse(&text("service"), &text("tag")),
            "bandcampTags" => bandcamp::tags(),
            "bandcampTag" => bandcamp::tag(&bandcamp::tag_slug(&text("tag")), &text("slice"), &text("next")),
            "stream" => stream(&args["track"]),
            "radioMore" => radio_more(&args["track"], &text("video")),
            "forYou" => {
                let history = self.library().history.clone();
                foryou::build(&history, args["refresh"].as_bool().unwrap_or(false))
            }
            "album" => album(&text("id")),
            "artist" => catalog::artist_detail(&text("id")),
            "radio" => radio::station(&text("subreddit")),
            "radioStations" => Ok(radio::stations()),
            "resolve" => {
                let key = args["track"]["key"].as_str().unwrap_or("").to_owned();
                let known = self.library().videos.get(&key).cloned();
                let videos = youtube::find_videos(&args["track"], known)?;
                Ok(json!({ "key": key, "videos": videos }))
            }
            "importPlaylist" => import_playlist(&text("url")),

            "recordPlay" => {
                {
                    let mut library = self.library();
                    library.record_play(&args["track"], &text("video"));
                    library.save();
                }
                changes(Value::Null)
            }
            "rememberVideo" => {
                self.library().remember_video(&text("key"), &text("video"));
                Ok(Value::Null)
            }
            "playlists" => Ok(self.library().playlists_json()),
            "playlist" => Ok(self.library().playlist_json(&text("id"))),
            "createPlaylist" => {
                let id = {
                    let mut library = self.library();
                    let id = library.create(&text("name"));
                    library.add(&id, list(&args["tracks"]));
                    id
                };
                changes(json!(id))
            }
            "renamePlaylist" => {
                self.library().rename(&text("id"), &text("name"));
                changes(Value::Null)
            }
            "deletePlaylist" => {
                self.library().delete(&text("id"));
                changes(Value::Null)
            }
            "addToPlaylist" => {
                let added = self.library().add(&text("id"), list(&args["tracks"]));
                changes(json!(added))
            }
            "removeFromPlaylist" => {
                if let Some(index) = args["index"].as_u64() {
                    self.library().remove(&text("id"), index as usize);
                }
                changes(Value::Null)
            }
            "moveInPlaylist" => {
                if let (Some(from), Some(to)) = (args["from"].as_u64(), args["to"].as_u64()) {
                    self.library().move_track(&text("id"), from as usize, to as usize);
                }
                changes(Value::Null)
            }
            "isLiked" => Ok(json!(self.library().is_liked(&text("key")))),
            "setLiked" => {
                self.library().set_liked(args["track"].clone(), args["liked"].as_bool().unwrap_or(false));
                changes(Value::Null)
            }
            "savedAlbums" => Ok(json!(self.library().albums)),
            "isAlbumSaved" => Ok(json!(self.library().is_album_saved(&text("id")))),
            "setAlbumSaved" => {
                self.library().set_album_saved(args["album"].clone(), args["saved"].as_bool().unwrap_or(false));
                changes(Value::Null)
            }
            // Everything the library holds that the interface shows at once.
            "library" => {
                let library = self.library();
                Ok(json!({ "playlists": library.playlists_json(), "albums": library.albums }))
            }
            other => Err(format!("Earshot doesn't know \"{other}\"")),
        }
    }
}
