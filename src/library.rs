//! The listener's own music: playlists, liked songs and saved albums,
//! kept in one JSON file in the user's data folder.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const LIKED: &str = "liked";

#[derive(Serialize, Deserialize, Clone)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub tracks: Vec<Value>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Library {
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub albums: Vec<Value>,
    /// Track key -> YouTube video id, so a song only has to be looked up once.
    #[serde(default)]
    pub videos: HashMap<String, String>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Earshot's data folder (where the library and saved stations live).
pub fn data_dir() -> Option<PathBuf> {
    default_path().and_then(|path| path.parent().map(PathBuf::from))
}

fn default_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("EARSHOT_DATA_DIR") {
        return Some(PathBuf::from(dir).join("library.json"));
    }
    directories::ProjectDirs::from("", "", "earshot").map(|dirs| dirs.data_dir().join("library.json"))
}

impl Library {
    pub fn load() -> Self {
        Self::load_from(default_path())
    }

    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut library: Library = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        library.path = path;
        if !library.playlists.iter().any(|p| p.id == LIKED) {
            library.playlists.insert(0, Playlist { id: LIKED.into(), name: "Liked songs".into(), created: now(), tracks: Vec::new() });
        }
        library
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, text).is_ok() {
            let _ = std::fs::rename(&temp, path);
        }
    }

    fn summary(playlist: &Playlist) -> Value {
        let covers: Vec<&Value> = {
            let mut seen = Vec::new();
            for track in &playlist.tracks {
                let cover = &track["cover"];
                if cover.as_str().is_some_and(|c| !c.is_empty()) && !seen.contains(&cover) {
                    seen.push(cover);
                }
                if seen.len() == 4 {
                    break;
                }
            }
            seen
        };
        let duration: u64 = playlist.tracks.iter().filter_map(|t| t["duration"].as_u64()).sum();
        json!({
            "id": playlist.id,
            "name": playlist.name,
            "count": playlist.tracks.len(),
            "duration": duration,
            "covers": covers,
            "special": playlist.id == LIKED,
        })
    }

    pub fn playlists_json(&self) -> Value {
        json!(self.playlists.iter().map(Self::summary).collect::<Vec<_>>())
    }

    pub fn playlist_json(&self, id: &str) -> Value {
        match self.playlists.iter().find(|p| p.id == id) {
            Some(playlist) => {
                let mut value = Self::summary(playlist);
                value["tracks"] = json!(playlist.tracks);
                value
            }
            None => Value::Null,
        }
    }

    pub fn create(&mut self, name: &str) -> String {
        let id = format!("p{}", now());
        let name = if name.trim().is_empty() { format!("Playlist {}", self.playlists.len()) } else { name.trim().to_owned() };
        self.playlists.push(Playlist { id: id.clone(), name, created: now(), tracks: Vec::new() });
        self.save();
        id
    }

    fn find(&mut self, id: &str) -> Option<&mut Playlist> {
        self.playlists.iter_mut().find(|p| p.id == id)
    }

    pub fn rename(&mut self, id: &str, name: &str) {
        if id == LIKED || name.trim().is_empty() {
            return;
        }
        if let Some(playlist) = self.find(id) {
            playlist.name = name.trim().to_owned();
            self.save();
        }
    }

    pub fn delete(&mut self, id: &str) {
        if id == LIKED {
            return;
        }
        self.playlists.retain(|p| p.id != id);
        self.save();
    }

    /// Adds tracks, skipping ones already in the playlist. Returns how many were added.
    pub fn add(&mut self, id: &str, tracks: Vec<Value>) -> usize {
        let Some(playlist) = self.find(id) else { return 0 };
        let mut added = 0;
        for track in tracks {
            if track["key"].as_str().is_none() || playlist.tracks.iter().any(|t| t["key"] == track["key"]) {
                continue;
            }
            playlist.tracks.push(track);
            added += 1;
        }
        self.save();
        added
    }

    pub fn remove(&mut self, id: &str, index: usize) {
        if let Some(playlist) = self.find(id) {
            if index < playlist.tracks.len() {
                playlist.tracks.remove(index);
                self.save();
            }
        }
    }

    pub fn move_track(&mut self, id: &str, from: usize, to: usize) {
        if let Some(playlist) = self.find(id) {
            if from < playlist.tracks.len() && to < playlist.tracks.len() && from != to {
                let track = playlist.tracks.remove(from);
                playlist.tracks.insert(to, track);
                self.save();
            }
        }
    }

    pub fn is_liked(&self, key: &str) -> bool {
        self.playlists.iter().find(|p| p.id == LIKED).is_some_and(|p| p.tracks.iter().any(|t| t["key"] == key))
    }

    pub fn set_liked(&mut self, track: Value, liked: bool) {
        let Some(key) = track["key"].as_str().map(str::to_owned) else { return };
        if liked {
            if let Some(playlist) = self.find(LIKED) {
                if !playlist.tracks.iter().any(|t| t["key"] == key.as_str()) {
                    playlist.tracks.insert(0, track);
                }
            }
        } else if let Some(playlist) = self.find(LIKED) {
            playlist.tracks.retain(|t| t["key"] != key.as_str());
        }
        self.save();
    }

    pub fn is_album_saved(&self, id: &str) -> bool {
        self.albums.iter().any(|a| a["id"] == id)
    }

    pub fn set_album_saved(&mut self, mut album: Value, saved: bool) {
        let Some(id) = album["id"].as_str().map(str::to_owned) else { return };
        self.albums.retain(|a| a["id"] != id.as_str());
        if saved {
            if let Some(object) = album.as_object_mut() {
                object.remove("tracks");
                object.insert("savedAt".into(), json!(now()));
            }
            self.albums.insert(0, album);
        }
        self.save();
    }

    pub fn remember_video(&mut self, key: &str, video: &str) {
        if key.starts_with("yt:") || self.videos.get(key).is_some_and(|v| v == video) {
            return;
        }
        self.videos.insert(key.to_owned(), video.to_owned());
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(key: &str) -> Value {
        json!({ "key": key, "title": key, "duration": 10, "cover": format!("c-{key}") })
    }

    #[test]
    fn playlists_round_trip() {
        let dir = std::env::temp_dir().join(format!("earshot-test-{}", now()));
        let path = Some(dir.join("library.json"));
        let mut library = Library::load_from(path.clone());
        let id = library.create("Road trip");
        assert_eq!(library.add(&id, vec![track("a"), track("b"), track("a")]), 2);
        library.move_track(&id, 1, 0);
        library.set_liked(track("z"), true);
        library.set_album_saved(json!({"id": "9", "title": "Record", "tracks": []}), true);

        let reloaded = Library::load_from(path);
        let playlist = reloaded.playlist_json(&id);
        assert_eq!(playlist["name"], "Road trip");
        assert_eq!(playlist["tracks"][0]["key"], "b");
        assert_eq!(playlist["duration"], 20);
        assert!(reloaded.is_liked("z"));
        assert!(reloaded.is_album_saved("9"));
        assert!(reloaded.albums[0].get("tracks").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
