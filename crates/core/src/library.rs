//! The listener's own music: playlists, liked songs, saved albums, the videos
//! found for songs and the listening history, kept in an SQLite database
//! (earshot.db) in the user's data folder.
//!
//! The Qt app and the web version's server may use the same database at the
//! same time, so every change is one transaction that takes SQLite's write
//! lock before reading what it changes (BEGIN IMMEDIATE); readers never wait
//! for writers (write-ahead log). The first time, a library.json from earlier
//! versions is imported and kept as library.json.imported.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};

pub const LIKED: &str = "liked";

/// How many plays the history keeps.
const HISTORY_LIMIT: i64 = 3000;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS playlists (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        created INTEGER NOT NULL,
        position INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS playlist_tracks (
        playlist TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        key TEXT NOT NULL,
        track TEXT NOT NULL,
        PRIMARY KEY (playlist, position)
    );
    CREATE TABLE IF NOT EXISTS albums (
        id TEXT PRIMARY KEY,
        album TEXT NOT NULL,
        saved_at INTEGER NOT NULL
    );
    -- Track key -> YouTube video id, so a song only has to be looked up once.
    CREATE TABLE IF NOT EXISTS videos (
        key TEXT PRIMARY KEY,
        video TEXT NOT NULL
    );
    -- Songs listened to (each with \"playedAt\" and the \"video\" that played);
    -- Made for you is built from these. Never leaves this computer.
    CREATE TABLE IF NOT EXISTS history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        key TEXT NOT NULL,
        played_at INTEGER NOT NULL,
        entry TEXT NOT NULL
    );
    -- Subreddits the listener added to Reddit radio (\"Your stations\").
    CREATE TABLE IF NOT EXISTS stations (
        subreddit TEXT PRIMARY KEY COLLATE NOCASE,
        added INTEGER NOT NULL
    );
";

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Earshot's data folder (where the library and saved stations live).
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("EARSHOT_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("", "", "earshot").map(|dirs| dirs.data_dir().to_path_buf())
}

fn text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

fn parse(text: String) -> Value {
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

pub struct Library {
    db: Connection,
}

/// A playlist as earlier versions kept it in library.json.
#[derive(Deserialize)]
struct OldPlaylist {
    id: String,
    name: String,
    #[serde(default)]
    created: u64,
    #[serde(default)]
    tracks: Vec<Value>,
}

#[derive(Deserialize, Default)]
struct OldLibrary {
    #[serde(default)]
    playlists: Vec<OldPlaylist>,
    #[serde(default)]
    albums: Vec<Value>,
    #[serde(default)]
    videos: std::collections::HashMap<String, String>,
    #[serde(default)]
    history: Vec<Value>,
}

impl Library {
    /// The library in Earshot's data folder.
    pub fn load() -> Self {
        Self::open(data_dir().map(|dir| dir.join("earshot.db")))
    }

    /// The library in `path` (kept in memory only if there's no path or it can't be opened).
    pub fn open(path: Option<PathBuf>) -> Self {
        let db = path
            .as_ref()
            .and_then(|path| {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                Connection::open(path).map_err(|error| eprintln!("earshot: can't open {}: {error}", path.display())).ok()
            })
            .unwrap_or_else(|| Connection::open_in_memory().expect("in-memory database"));
        // Another Earshot holding the write lock briefly makes this wait, not fail.
        let _ = db.busy_timeout(Duration::from_secs(10));
        let _ = db.pragma_update(None, "journal_mode", "WAL");
        let _ = db.pragma_update(None, "foreign_keys", true);
        let _ = db.execute_batch(SCHEMA);
        let mut library = Self { db };
        if let Some(path) = &path {
            library.import_json(&path.with_file_name("library.json"));
        }
        library.write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO playlists (id, name, created, position) VALUES (?1, 'Liked songs', ?2, -1)",
                params![LIKED, now() as i64],
            )
        });
        library
    }

    /// Brings in library.json from earlier versions, once.
    fn import_json(&mut self, old: &Path) {
        let Ok(saved) = std::fs::read_to_string(old) else { return };
        let Ok(old_library) = serde_json::from_str::<OldLibrary>(&saved) else { return };
        let imported = self.write(|tx| {
            for (position, playlist) in old_library.playlists.iter().enumerate() {
                tx.execute(
                    "INSERT OR REPLACE INTO playlists (id, name, created, position) VALUES (?1, ?2, ?3, ?4)",
                    params![playlist.id, playlist.name, playlist.created as i64, if playlist.id == LIKED { -1 } else { position as i64 }],
                )?;
                write_tracks(tx, &playlist.id, &playlist.tracks)?;
            }
            for (age, album) in old_library.albums.iter().enumerate() {
                let saved = album["savedAt"].as_u64().unwrap_or(now().saturating_sub(age as u64));
                tx.execute(
                    "INSERT OR REPLACE INTO albums (id, album, saved_at) VALUES (?1, ?2, ?3)",
                    params![album["id"].as_str().unwrap_or(""), text(album), saved as i64],
                )?;
            }
            for (key, video) in &old_library.videos {
                tx.execute("INSERT OR REPLACE INTO videos (key, video) VALUES (?1, ?2)", params![key, video])?;
            }
            for entry in &old_library.history {
                tx.execute(
                    "INSERT INTO history (key, played_at, entry) VALUES (?1, ?2, ?3)",
                    params![entry["key"].as_str().unwrap_or(""), entry["playedAt"].as_u64().unwrap_or(0) as i64, text(entry)],
                )?;
            }
            Ok(())
        });
        if imported.is_some() {
            let _ = std::fs::rename(old, old.with_extension("json.imported"));
        }
    }

    /// One change: takes the write lock first, so nothing another Earshot
    /// writes in between is lost, then commits all of it or none.
    fn write<T>(&mut self, change: impl FnOnce(&Transaction) -> rusqlite::Result<T>) -> Option<T> {
        let result = self.db.transaction_with_behavior(TransactionBehavior::Immediate).and_then(|tx| {
            let value = change(&tx)?;
            tx.commit()?;
            Ok(value)
        });
        result.map_err(|error| eprintln!("earshot: couldn't save the library: {error}")).ok()
    }

    /// Changes whenever another connection (another Earshot) commits a change.
    pub fn data_version(&self) -> i64 {
        self.db.query_row("PRAGMA data_version", [], |row| row.get(0)).unwrap_or(0)
    }

    fn tracks(&self, playlist: &str) -> Vec<Value> {
        let Ok(mut query) = self.db.prepare_cached("SELECT track FROM playlist_tracks WHERE playlist = ?1 ORDER BY position") else {
            return Vec::new();
        };
        query
            .query_map([playlist], |row| row.get::<_, String>(0))
            .map(|rows| rows.filter_map(Result::ok).map(parse).collect())
            .unwrap_or_default()
    }

    fn playlist_rows(&self) -> Vec<(String, String)> {
        let Ok(mut query) = self.db.prepare_cached("SELECT id, name FROM playlists ORDER BY position, created") else {
            return Vec::new();
        };
        query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    fn summary(id: &str, name: &str, tracks: &[Value]) -> Value {
        let mut covers: Vec<&Value> = Vec::new();
        for track in tracks {
            let cover = &track["cover"];
            if cover.as_str().is_some_and(|c| !c.is_empty()) && !covers.contains(&cover) {
                covers.push(cover);
            }
            if covers.len() == 4 {
                break;
            }
        }
        let duration: u64 = tracks.iter().filter_map(|t| t["duration"].as_u64()).sum();
        json!({
            "id": id,
            "name": name,
            "count": tracks.len(),
            "duration": duration,
            "covers": covers,
            "special": id == LIKED,
        })
    }

    pub fn playlists_json(&self) -> Value {
        json!(self.playlist_rows().iter().map(|(id, name)| Self::summary(id, name, &self.tracks(id))).collect::<Vec<_>>())
    }

    pub fn playlist_json(&self, id: &str) -> Value {
        let name: Option<String> = self.db.query_row("SELECT name FROM playlists WHERE id = ?1", [id], |row| row.get(0)).optional().ok().flatten();
        let Some(name) = name else { return Value::Null };
        let tracks = self.tracks(id);
        let mut value = Self::summary(id, &name, &tracks);
        value["tracks"] = json!(tracks);
        value
    }

    /// Every song in every playlist, with its playlist's id (for Made for you).
    pub fn all_playlist_tracks(&self) -> Vec<(String, Value)> {
        let Ok(mut query) = self.db.prepare_cached("SELECT playlist, track FROM playlist_tracks ORDER BY playlist, position") else {
            return Vec::new();
        };
        query
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map(|rows| rows.filter_map(Result::ok).map(|(id, track)| (id, parse(track))).collect())
            .unwrap_or_default()
    }

    pub fn create(&mut self, name: &str) -> String {
        let id = format!("p{}", now());
        let name = name.trim().to_owned();
        self.write(|tx| {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM playlists", [], |row| row.get(0))?;
            let name = if name.is_empty() { format!("Playlist {count}") } else { name };
            tx.execute(
                "INSERT INTO playlists (id, name, created, position) VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM playlists))",
                params![id, name, now() as i64],
            )
        });
        id
    }

    pub fn rename(&mut self, id: &str, name: &str) {
        if id == LIKED || name.trim().is_empty() {
            return;
        }
        self.write(|tx| tx.execute("UPDATE playlists SET name = ?2 WHERE id = ?1", params![id, name.trim()]));
    }

    pub fn delete(&mut self, id: &str) {
        if id == LIKED {
            return;
        }
        self.write(|tx| tx.execute("DELETE FROM playlists WHERE id = ?1", [id]));
    }

    /// Changes one playlist's songs inside a transaction (so a change made
    /// meanwhile by another Earshot is read first, not overwritten).
    fn change_tracks<T>(&mut self, id: &str, change: impl FnOnce(&mut Vec<Value>) -> T) -> Option<T> {
        self.write(|tx| {
            let exists: Option<i64> = tx.query_row("SELECT 1 FROM playlists WHERE id = ?1", [id], |row| row.get(0)).optional()?;
            if exists.is_none() {
                return Ok(None);
            }
            let mut tracks: Vec<Value> = {
                let mut query = tx.prepare("SELECT track FROM playlist_tracks WHERE playlist = ?1 ORDER BY position")?;
                query.query_map([id], |row| row.get::<_, String>(0))?.filter_map(Result::ok).map(parse).collect()
            };
            let result = change(&mut tracks);
            write_tracks(tx, id, &tracks)?;
            Ok(Some(result))
        })
        .flatten()
    }

    /// Adds tracks, skipping ones already in the playlist. Returns how many were added.
    pub fn add(&mut self, id: &str, tracks: Vec<Value>) -> usize {
        self.change_tracks(id, |list| {
            let mut added = 0;
            for track in tracks {
                if track["key"].as_str().is_none() || list.iter().any(|t| t["key"] == track["key"]) {
                    continue;
                }
                list.push(track);
                added += 1;
            }
            added
        })
        .unwrap_or(0)
    }

    pub fn remove(&mut self, id: &str, index: usize) {
        self.change_tracks(id, |list| {
            if index < list.len() {
                list.remove(index);
            }
        });
    }

    pub fn move_track(&mut self, id: &str, from: usize, to: usize) {
        self.change_tracks(id, |list| {
            if from < list.len() && to < list.len() && from != to {
                let track = list.remove(from);
                list.insert(to, track);
            }
        });
    }

    pub fn is_liked(&self, key: &str) -> bool {
        self.db
            .query_row("SELECT 1 FROM playlist_tracks WHERE playlist = ?1 AND key = ?2", params![LIKED, key], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    pub fn set_liked(&mut self, track: Value, liked: bool) {
        let Some(key) = track["key"].as_str().map(str::to_owned) else { return };
        self.change_tracks(LIKED, |list| {
            if liked {
                if !list.iter().any(|t| t["key"] == key.as_str()) {
                    list.insert(0, track);
                }
            } else {
                list.retain(|t| t["key"] != key.as_str());
            }
        });
    }

    /// Saved albums, newest first.
    pub fn albums(&self) -> Vec<Value> {
        let Ok(mut query) = self.db.prepare_cached("SELECT album FROM albums ORDER BY saved_at DESC") else {
            return Vec::new();
        };
        query.query_map([], |row| row.get::<_, String>(0)).map(|rows| rows.filter_map(Result::ok).map(parse).collect()).unwrap_or_default()
    }

    pub fn is_album_saved(&self, id: &str) -> bool {
        self.db.query_row("SELECT 1 FROM albums WHERE id = ?1", [id], |_| Ok(())).optional().ok().flatten().is_some()
    }

    pub fn set_album_saved(&mut self, mut album: Value, saved: bool) {
        let Some(id) = album["id"].as_str().map(str::to_owned) else { return };
        let saved_at = now();
        if let Some(object) = album.as_object_mut() {
            object.remove("tracks");
            object.insert("savedAt".into(), json!(saved_at));
        }
        self.write(|tx| {
            if saved {
                tx.execute("INSERT OR REPLACE INTO albums (id, album, saved_at) VALUES (?1, ?2, ?3)", params![id, text(&album), saved_at as i64])
            } else {
                tx.execute("DELETE FROM albums WHERE id = ?1", [&id])
            }
        });
    }

    /// The video found earlier for a song, if any.
    pub fn video(&self, key: &str) -> Option<String> {
        self.db.query_row("SELECT video FROM videos WHERE key = ?1", [key], |row| row.get(0)).optional().ok().flatten()
    }

    pub fn remember_video(&mut self, key: &str, video: &str) {
        if key.starts_with("yt:") || video.is_empty() || self.video(key).as_deref() == Some(video) {
            return;
        }
        self.write(|tx| tx.execute("INSERT OR REPLACE INTO videos (key, video) VALUES (?1, ?2)", params![key, video]));
    }

    /// Notes that a song was listened to.
    pub fn record_play(&mut self, track: &Value, video: &str) {
        let mut entry = track.clone();
        let played_at = now();
        entry["playedAt"] = json!(played_at);
        entry["video"] = json!(video);
        self.write(|tx| {
            tx.execute(
                "INSERT INTO history (key, played_at, entry) VALUES (?1, ?2, ?3)",
                params![entry["key"].as_str().unwrap_or(""), played_at as i64, text(&entry)],
            )?;
            tx.execute("DELETE FROM history WHERE id <= (SELECT MAX(id) FROM history) - ?1", [HISTORY_LIMIT])
        });
    }

    /// The listening history, oldest first.
    pub fn history(&self) -> Vec<Value> {
        let Ok(mut query) = self.db.prepare_cached("SELECT entry FROM history ORDER BY id") else {
            return Vec::new();
        };
        query.query_map([], |row| row.get::<_, String>(0)).map(|rows| rows.filter_map(Result::ok).map(parse).collect()).unwrap_or_default()
    }

    /// The songs listened to, newest first, each once (at its latest play),
    /// with "playedAt" and how many times it played ("plays"); at most `limit`.
    pub fn recently_played(&self, limit: usize) -> Vec<Value> {
        let Ok(mut query) = self.db.prepare_cached(
            "SELECT h.entry, g.plays FROM history h \
             JOIN (SELECT key, MAX(id) AS last, COUNT(*) AS plays FROM history GROUP BY key) g ON h.id = g.last \
             ORDER BY h.id DESC LIMIT ?1",
        ) else {
            return Vec::new();
        };
        query
            .query_map([limit as i64], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .map(|rows| {
                rows.filter_map(Result::ok)
                    .map(|(entry, plays)| {
                        let mut track = parse(entry);
                        if let Some(map) = track.as_object_mut() {
                            map.remove("video");
                            map.insert("plays".into(), json!(plays));
                        }
                        track
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Forgets every play of one song.
    pub fn forget_play(&mut self, key: &str) {
        self.write(|tx| tx.execute("DELETE FROM history WHERE key = ?1", [key]));
    }

    /// Forgets the whole listening history.
    pub fn clear_history(&mut self) {
        self.write(|tx| tx.execute("DELETE FROM history", []));
    }

    /// Subreddits added to Reddit radio, in the order they were added.
    pub fn stations(&self) -> Vec<String> {
        let Ok(mut query) = self.db.prepare_cached("SELECT subreddit FROM stations ORDER BY added, rowid") else {
            return Vec::new();
        };
        query.query_map([], |row| row.get::<_, String>(0)).map(|rows| rows.filter_map(Result::ok).collect()).unwrap_or_default()
    }

    /// Keeps a subreddit among the listener's stations (once, whatever its
    /// capitals). Returns whether it was new.
    pub fn save_station(&mut self, subreddit: &str) -> bool {
        let subreddit = subreddit.trim();
        if subreddit.is_empty() {
            return false;
        }
        self.write(|tx| tx.execute("INSERT OR IGNORE INTO stations (subreddit, added) VALUES (?1, ?2)", params![subreddit, now() as i64]))
            .is_some_and(|added| added > 0)
    }

    pub fn remove_station(&mut self, subreddit: &str) {
        self.write(|tx| tx.execute("DELETE FROM stations WHERE subreddit = ?1", [subreddit.trim()]));
    }
}

/// Replaces a playlist's songs (inside the caller's transaction).
fn write_tracks(tx: &Transaction, id: &str, tracks: &[Value]) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM playlist_tracks WHERE playlist = ?1", [id])?;
    let mut insert = tx.prepare_cached("INSERT INTO playlist_tracks (playlist, position, key, track) VALUES (?1, ?2, ?3, ?4)")?;
    for (position, track) in tracks.iter().enumerate() {
        insert.execute(params![id, position as i64, track["key"].as_str().unwrap_or(""), text(track)])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(key: &str) -> Value {
        json!({ "key": key, "title": key, "duration": 10, "cover": format!("c-{key}") })
    }

    /// A folder of its own for each test (they run at the same time).
    fn temp_dir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("earshot-test-{}-{}-{n}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn playlists_round_trip() {
        let path = temp_dir().join("earshot.db");
        let mut library = Library::open(Some(path.clone()));
        let id = library.create("Road trip");
        assert_eq!(library.add(&id, vec![track("a"), track("b"), track("a")]), 2);
        library.move_track(&id, 1, 0);
        library.set_liked(track("z"), true);
        library.set_album_saved(json!({"id": "9", "title": "Record", "tracks": []}), true);
        library.record_play(&track("a"), "v1");

        let reopened = Library::open(Some(path));
        let playlist = reopened.playlist_json(&id);
        assert_eq!(playlist["name"], "Road trip");
        assert_eq!(playlist["tracks"][0]["key"], "b");
        assert_eq!(playlist["duration"], 20);
        assert!(reopened.is_liked("z"));
        assert!(reopened.is_album_saved("9"));
        assert!(reopened.albums()[0].get("tracks").is_none());
        assert_eq!(reopened.history()[0]["video"], "v1");
        assert_eq!(reopened.playlists_json()[0]["id"], LIKED, "Liked songs comes first");
    }

    /// Two Earshots (the app and the web version's server) on one database:
    /// neither loses the other's changes, and each can tell the other changed it.
    #[test]
    fn two_earshots_share_one_library() {
        let path = temp_dir().join("earshot.db");
        let mut app = Library::open(Some(path.clone()));
        let mut web = Library::open(Some(path));
        let id = app.create("Shared");
        let before = app.data_version();
        assert_eq!(web.add(&id, vec![track("a")]), 1);
        assert_eq!(app.add(&id, vec![track("b")]), 1);
        assert_eq!(web.playlist_json(&id)["count"], 2);
        assert_ne!(app.data_version(), before, "the app sees the web version's change");
    }

    /// Subreddits added to Reddit radio: kept once each, in order, shared, removable.
    #[test]
    fn lists_each_played_song_once_newest_first() {
        let path = temp_dir().join("earshot.db");
        let mut library = Library::open(Some(path));
        for key in ["a", "b", "a", "c"] {
            library.record_play(&track(key), "v");
        }
        let recent = library.recently_played(10);
        let keys: Vec<&str> = recent.iter().map(|t| t["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["c", "a", "b"]);
        assert_eq!(recent[1]["plays"], 2);
        assert!(recent[0].get("video").is_none());
        library.forget_play("a");
        assert_eq!(library.recently_played(10).len(), 2);
        library.clear_history();
        assert!(library.recently_played(10).is_empty());
        assert!(library.history().is_empty());
    }

    #[test]
    fn keeps_added_radio_stations() {
        let path = temp_dir().join("earshot.db");
        let mut app = Library::open(Some(path.clone()));
        assert!(app.stations().is_empty());
        assert!(app.save_station("Vaporwave"));
        assert!(app.save_station("chiptune"));
        assert!(!app.save_station("vaporwave"), "the same subreddit in other capitals isn't kept twice");
        assert!(!app.save_station("  "));

        let mut web = Library::open(Some(path));
        assert_eq!(web.stations(), ["Vaporwave", "chiptune"]);
        web.remove_station("VAPORWAVE");
        assert_eq!(app.stations(), ["chiptune"]);
    }

    #[test]
    fn imports_the_old_json_library() {
        let dir = temp_dir();
        std::fs::write(
            dir.join("library.json"),
            json!({
                "playlists": [{ "id": "liked", "name": "Liked songs", "tracks": [track("l")] }, { "id": "p1", "name": "Old", "tracks": [track("x")] }],
                "albums": [{ "id": "7", "title": "Kept" }],
                "videos": { "dz:1": "vid" },
                "history": [{ "key": "x", "playedAt": 5 }]
            })
            .to_string(),
        )
        .unwrap();
        let library = Library::open(Some(dir.join("earshot.db")));
        assert!(library.is_liked("l"));
        assert_eq!(library.playlist_json("p1")["tracks"][0]["key"], "x");
        assert!(library.is_album_saved("7"));
        assert_eq!(library.video("dz:1").as_deref(), Some("vid"));
        assert_eq!(library.history().len(), 1);
        assert!(dir.join("library.json.imported").exists());
        assert!(!dir.join("library.json").exists());
    }
}
