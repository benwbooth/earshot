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
        "youtube" => youtube::search_albums(&format!("{tag} music"), "", true),
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
        // A Mixcloud show ("mc:/user/show/") or someone's latest shows ("mc:user:name").
        Some("mc") => mixcloud::album(id),
        Some("mb") => crate::musicbrainz::album(id),
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

/// Lower-case words only, for comparing titles and names across sites.
fn plain(text: &str) -> String {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Whether `other` (from SoundCloud or Bandcamp) is the same recording as
/// `track`: the same title (sites sometimes put "Artist - " in front), by the
/// same artist, and as long, give or take a few seconds.
fn same_song(track: &Value, other: &Value) -> bool {
    let text = |value: &Value, key: &str| plain(value[key].as_str().unwrap_or(""));
    let (title, found) = (text(track, "title"), text(other, "title"));
    if title.is_empty() || !(found == title || found.ends_with(&format!(" {title}"))) {
        return false;
    }
    // The first named artist ("A, B & C" → "a"), anywhere in the other's names or title.
    let artist = plain(track["artist"].as_str().unwrap_or("").split([',', '&']).next().unwrap_or(""));
    if artist.is_empty() || !(text(other, "artist").contains(&artist) || found.contains(&artist)) {
        return false;
    }
    let (length, other_length) = (track["duration"].as_f64().unwrap_or(0.0), other["duration"].as_f64().unwrap_or(0.0));
    length <= 0.0 || other_length <= 0.0 || (length - other_length).abs() <= (length * 0.03).max(8.0)
}

/// The same song on SoundCloud or Bandcamp, for when no YouTube upload of it
/// will play (owners can keep videos on youtube.com). The song keeps its own
/// name, key and artwork, so likes, playlists and history stay with it; only
/// where it plays from changes. Null when neither site has it.
pub fn elsewhere(track: &Value) -> Result<Value, String> {
    Ok(found_elsewhere(track, true).into_iter().next().unwrap_or(Value::Null))
}

/// [`elsewhere`]'s finds on each site, SoundCloud first, for when the first
/// one can't be used.
pub fn elsewhere_all(track: &Value) -> Vec<Value> {
    found_elsewhere(track, false)
}

fn found_elsewhere(track: &Value, first_only: bool) -> Vec<Value> {
    let query = format!("{} {}", track["artist"].as_str().unwrap_or(""), track["title"].as_str().unwrap_or(""));
    let mut songs = Vec::new();
    for search in [soundcloud::search, bandcamp::search] {
        if first_only && !songs.is_empty() {
            break;
        }
        let Ok(found) = search(query.trim()) else { continue };
        // SoundCloud's own player can't play an encrypted song (labels' releases
        // usually are) in most browsers, nor can the waveform read it: skip those.
        let playable = |other: &Value| match other["soundcloudId"].as_u64() {
            Some(id) => soundcloud::audio(id).is_ok(),
            None => true,
        };
        let Some(other) = found["tracks"].as_array().into_iter().flatten().filter(|other| same_song(track, other)).take(3).find(|other| playable(other))
        else {
            continue;
        };
        let mut song = other.clone();
        for key in ["key", "title", "artist", "artistId", "album", "albumId", "cover", "coverBig", "explicit", "trackNumber", "disc"] {
            if !track[key].is_null() {
                song[key] = track[key].clone();
            }
        }
        songs.push(song);
    }
    songs
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

type Listeners = Arc<Mutex<Vec<Box<dyn Fn() + Send + Sync>>>>;

fn tell(listeners: &Listeners) {
    for listener in listeners.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).iter() {
        listener();
    }
}

/// The music services plus the listener's library.
pub struct Earshot {
    library: Arc<Mutex<Library>>,
    on_change: Listeners,
}

impl Earshot {
    pub fn new(library: Arc<Mutex<Library>>) -> Self {
        let on_change: Listeners = Arc::default();
        // The app and the web version's server can share one database (the
        // Docker image mounts the app's data folder): notice the other's changes.
        let (watched, listeners) = (library.clone(), on_change.clone());
        std::thread::spawn(move || {
            let version = || watched.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).data_version();
            let mut seen = version();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let now = version();
                if now != seen {
                    seen = now;
                    tell(&listeners);
                }
            }
        });
        Self { library, on_change }
    }

    pub fn library(&self) -> MutexGuard<'_, Library> {
        self.library.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn shared_library(&self) -> Arc<Mutex<Library>> {
        self.library.clone()
    }

    /// Runs `listener` whenever playlists, liked songs, saved albums or the
    /// listening history change, here or in another Earshot sharing the file.
    pub fn on_change(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.on_change.lock().unwrap().push(Box::new(listener));
    }

    fn changed(&self) {
        tell(&self.on_change);
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
            "searchYoutubeAlbums" => youtube::search_albums(&text("query"), &text("next"), args["preferMusic"].as_bool().unwrap_or(true)),
            "searchBandcamp" => bandcamp::search(&text("query")),
            "searchSoundcloud" => soundcloud::search(&text("query")),
            "searchMixcloud" => mixcloud::search(&text("query")),
            "searchArchive" => archive::search(&text("query")),
            "browse" => browse(&text("service"), &text("tag")),
            // YouTube's, SoundCloud's, Mixcloud's or the Internet Archive's front page:
            // { sections: [{ id, title, subtitle, albums, tracks, round? }] }.
            "featured" => crate::featured::featured(&text("service")),
            "bandcampTags" => bandcamp::tags(),
            "bandcampTag" => bandcamp::tag(&bandcamp::tag_slug(&text("tag")), &text("slice"), &text("next")),
            // Bandcamp's front page: { highlight, sections: [{ id, title, about, link, rows, albums, tracks, articles }] }.
            "bandcampFeatured" => bandcamp::featured(),
            "stream" => stream(&args["track"]),
            "elsewhere" => elsewhere(&args["track"]),
            // Volume leveling: how much to turn this song up or down.
            "loudness" => Ok(crate::loudness::level(&text("id"), &args["track"])),
            "soundcloudEncrypted" => soundcloud::encrypted(args["id"].as_u64().ok_or("No SoundCloud id")?).map(Value::from),
            "radioMore" => radio_more(&args["track"], &text("video")),
            "forYou" => {
                // Liked songs and playlist songs count too, not only plays.
                let (history, favourites) = {
                    let library = self.library();
                    let kept: Vec<(Value, usize)> = library
                        .all_playlist_tracks()
                        .into_iter()
                        .map(|(playlist, track)| (track, if playlist == crate::library::LIKED { foryou::LIKED_WEIGHT } else { 1 }))
                        .collect();
                    (library.history(), kept)
                };
                foryou::build(&history, &favourites, args["refresh"].as_bool().unwrap_or(false))
            }
            "album" => album(&text("id")),
            "artist" => catalog::artist_detail(&text("id")),
            "radio" => radio::station(&text("subreddit")),
            "radioStations" => Ok(radio::stations()),
            // What the listener typed ("r/name", "name" or a reddit.com link) as a subreddit name.
            "radioStationName" => radio::subreddit_name(&text("text")).map(Value::from),
            // Subreddits the listener added ("Your stations"): [{ id, name, about }].
            "savedStations" => Ok(radio::custom_stations(&self.library().stations())),
            // Keeps a subreddit that isn't a built-in station; answers whether it's new.
            "saveStation" => {
                let name = radio::subreddit_name(&text("subreddit"))?;
                let added = radio::built_in(&name).is_none() && self.library().save_station(&name);
                if added { changes(json!(true)) } else { Ok(json!(false)) }
            }
            "removeStation" => {
                self.library().remove_station(&text("subreddit"));
                changes(Value::Null)
            }
            "resolve" => {
                let key = args["track"]["key"].as_str().unwrap_or("").to_owned();
                let known = self.library().video(&key);
                let videos = youtube::find_videos(&args["track"], known)?;
                Ok(json!({ "key": key, "videos": videos }))
            }
            "importPlaylist" => import_playlist(&text("url")),

            "recordPlay" => {
                self.library().record_play(&args["track"], &text("video"));
                changes(Value::Null)
            }
            // The songs listened to, newest first, each once: [track + { playedAt, plays }].
            "recentlyPlayed" => Ok(json!(self.library().recently_played(args["limit"].as_u64().unwrap_or(500) as usize))),
            "forgetPlay" => {
                self.library().forget_play(&text("key"));
                changes(Value::Null)
            }
            "clearHistory" => {
                self.library().clear_history();
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
            "savedAlbums" => Ok(json!(self.library().albums())),
            "isAlbumSaved" => Ok(json!(self.library().is_album_saved(&text("id")))),
            "setAlbumSaved" => {
                self.library().set_album_saved(args["album"].clone(), args["saved"].as_bool().unwrap_or(false));
                changes(Value::Null)
            }
            // Everything the library holds that the interface shows at once.
            "library" => {
                let library = self.library();
                Ok(json!({ "playlists": library.playlists_json(), "albums": library.albums() }))
            }
            other => Err(format!("Earshot doesn't know \"{other}\"")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_same_song_elsewhere() {
        let song = json!({ "title": "Autumn Leaves (Extended Version)", "artist": "Lofi Fruits Music", "duration": 310 });
        let same = json!({ "title": "Autumn Leaves (Extended Version)", "artist": "Lofi Fruits Music", "duration": 312 });
        let named = json!({ "title": "Lofi Fruits Music - Autumn Leaves (Extended Version)", "artist": "someone", "duration": 0 });
        let shorter = json!({ "title": "Autumn Leaves", "artist": "Lofi Fruits Music, Chill Fruits Music", "duration": 124 });
        let longer = json!({ "title": "Autumn Leaves (Extended Version)", "artist": "Lofi Fruits Music", "duration": 400 });
        let other_artist = json!({ "title": "Autumn Leaves (Extended Version)", "artist": "Yoofi Beats", "duration": 310 });
        assert!(same_song(&song, &same));
        assert!(same_song(&song, &named));
        assert!(!same_song(&song, &shorter));
        assert!(!same_song(&song, &longer));
        assert!(!same_song(&song, &other_artist));
    }
}
