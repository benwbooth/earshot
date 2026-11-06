//! Made for you: Discover Weekly, Daily Mixes, Release Radar and albums
//! picked for you, built from the listening history kept on this computer.
//! New songs come from YouTube's own mixes for the songs you play most; new
//! releases and related artists from Deezer's catalogue.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::net::{encode, get_json};
use crate::{catalog, random, youtube};

const DAY_MS: u64 = 86_400_000;
/// Fewer different songs than this and there's nothing to go on yet.
pub const SONGS_NEEDED: usize = 1;
/// How much a liked song counts, in plays (a song in a playlist counts one).
pub const LIKED_WEIGHT: usize = 2;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// What the history and the library say about the listener.
struct Taste {
    /// Songs by how much they count lately: (track, its video, plays).
    songs: Vec<(Value, String, usize)>,
    /// Artists by plays: (name, their songs played, plays, Deezer id if known).
    artists: Vec<(String, Vec<Value>, usize, String)>,
    /// Every song heard or kept (track keys and "yt:<video>"), so mixes bring new ones.
    heard: HashSet<String>,
}

/// From the plays of the last 60 days, plus `favourites`: songs liked or
/// kept in playlists, with how many plays each counts as.
fn taste(history: &[Value], favourites: &[(Value, usize)], now: u64) -> Taste {
    let recent_from = now.saturating_sub(60 * DAY_MS);
    let mut heard = HashSet::new();
    let mut songs: HashMap<String, (Value, String, usize)> = HashMap::new();
    let mut artists: HashMap<String, (String, Vec<Value>, usize, String)> = HashMap::new();
    let recent = history.iter().map(|play| (play, 1, play["playedAt"].as_u64().unwrap_or(0) >= recent_from));
    let kept = favourites.iter().map(|(track, weight)| (track, *weight, true));
    for (play, weight, counts) in recent.chain(kept) {
        let key = play["key"].as_str().unwrap_or("").to_owned();
        let video = play["video"].as_str().filter(|v| !v.is_empty()).or(play["youtubeId"].as_str()).unwrap_or("").to_owned();
        heard.insert(key.clone());
        if !video.is_empty() {
            heard.insert(format!("yt:{video}"));
        }
        if !counts || key.is_empty() {
            continue;
        }
        let song = songs.entry(key.clone()).or_insert_with(|| (play.clone(), video.clone(), 0));
        song.2 += weight;
        if song.1.is_empty() {
            song.1 = video;
        }
        let name = play["artist"].as_str().unwrap_or("").trim().to_owned();
        if !name.is_empty() {
            let artist = artists.entry(name.to_lowercase()).or_insert_with(|| (name.clone(), Vec::new(), 0, String::new()));
            artist.2 += weight;
            if !artist.1.iter().any(|t| t["key"] == play["key"]) {
                artist.1.push(play.clone());
            }
            if artist.3.is_empty() && play["key"].as_str().is_some_and(|k| k.starts_with("dz:")) {
                artist.3 = play["artistId"].as_str().unwrap_or("").to_owned();
            }
        }
    }
    let mut songs: Vec<_> = songs.into_values().collect();
    songs.sort_by(|a, b| b.2.cmp(&a.2));
    let mut artists: Vec<_> = artists.into_values().collect();
    artists.sort_by(|a, b| b.2.cmp(&a.2));
    Taste { songs, artists, heard }
}

/// A clean copy of a played song (without the history's own fields).
fn clean(track: &Value) -> Value {
    let mut track = track.clone();
    if let Some(map) = track.as_object_mut() {
        map.remove("playedAt");
        map.remove("video");
    }
    track
}

/// The YouTube video for a played song: the one that played, or a search.
fn video_for(track: &Value, video: &str) -> Option<String> {
    if !video.is_empty() {
        return Some(video.to_owned());
    }
    if let Some(id) = track["youtubeId"].as_str() {
        return Some(id.to_owned());
    }
    let query = format!("{} - {}", track["artist"].as_str().unwrap_or(""), track["title"].as_str().unwrap_or(""));
    youtube::search(&query, 1).ok()?.first()?["youtubeId"].as_str().map(str::to_owned)
}

/// New songs like `seed` that haven't been heard (and aren't in `taken`).
fn new_like(seed: &Value, video: &str, heard: &HashSet<String>, taken: &mut HashSet<String>, count: usize) -> Vec<Value> {
    let Some(video) = video_for(seed, video) else { return Vec::new() };
    let mut found = youtube::mix(&video).unwrap_or_default();
    random::shuffle(&mut found);
    found
        .into_iter()
        .filter(|song| {
            let key = song["key"].as_str().unwrap_or("").to_owned();
            !heard.contains(&key) && taken.insert(key)
        })
        .take(count)
        .collect()
}

fn discover_weekly(taste: &Taste) -> Value {
    let mut taken = HashSet::new();
    let mut tracks = Vec::new();
    // About 30 songs, from up to six favourites (more from each when there are few).
    let seeds = taste.songs.len().clamp(1, 6);
    for (seed, video, _) in taste.songs.iter().take(6) {
        tracks.extend(new_like(seed, video, &taste.heard, &mut taken, (30 / seeds).max(5)));
    }
    random::shuffle(&mut tracks);
    json!({
        "id": "discover-weekly", "title": "Discover Weekly",
        "subtitle": "Songs you haven't heard, like the ones you've been playing. New every Monday.",
        "tracks": tracks,
    })
}

fn daily_mixes(taste: &Taste) -> Vec<Value> {
    let mut taken = HashSet::new();
    taste
        .artists
        .iter()
        .take(3)
        .enumerate()
        .filter_map(|(index, (name, played, _, _))| {
            let mut tracks: Vec<Value> = played.iter().take(8).map(clean).collect();
            for track in &tracks {
                taken.insert(track["key"].as_str().unwrap_or("").to_owned());
            }
            let favourite = taste.songs.iter().find(|(song, _, _)| song["artist"].as_str() == Some(name.as_str()))?;
            tracks.extend(new_like(&favourite.0, &favourite.1, &taste.heard, &mut taken, 17));
            random::shuffle(&mut tracks);
            Some(json!({
                "id": format!("daily-mix-{}", index + 1),
                "title": format!("Daily Mix {}", index + 1),
                "subtitle": format!("{name} and similar artists. New every day."),
                "tracks": tracks,
            }))
        })
        .collect()
}

/// An artist's Deezer id: known from catalogue songs, or found by name.
fn deezer_artist(name: &str, known: &str) -> Option<String> {
    if !known.is_empty() {
        return Some(known.to_owned());
    }
    let found = get_json(&format!("https://api.deezer.com/search/artist?q={}&limit=1", encode(name))).ok()?;
    let artist = found["data"].as_array()?.first()?;
    (artist["name"].as_str()?.to_lowercase() == name.to_lowercase()).then(|| artist["id"].to_string())
}

/// Days since 1970 for "2026-11-05".
fn days(date: &str) -> Option<i64> {
    let mut parts = date.get(..10)?.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn release_radar(taste: &Taste, ids: &[(String, String)], today: i64) -> Value {
    let mut albums: Vec<(i64, Value)> = Vec::new();
    for (name, id) in ids.iter().take(10) {
        let Ok(list) = get_json(&format!("https://api.deezer.com/artist/{id}/albums?limit=50")) else { continue };
        let artist = json!({ "id": id, "name": name });
        for item in list["data"].as_array().cloned().unwrap_or_default() {
            let Some(released) = item["release_date"].as_str().and_then(days) else { continue };
            if released <= today && today - released <= 60 {
                albums.push((released, catalog::album(&item, Some(&artist))));
            }
        }
    }
    albums.sort_by(|a, b| b.0.cmp(&a.0));
    let _ = taste;
    json!({
        "id": "release-radar", "title": "Release Radar",
        "subtitle": "New releases from artists you listen to, from the last two months.",
        "albums": albums.into_iter().map(|(_, album)| album).take(24).collect::<Vec<_>>(),
    })
}

fn albums_for_you(taste: &Taste, ids: &[(String, String)]) -> Value {
    let known: HashSet<String> = taste.artists.iter().map(|a| a.0.to_lowercase()).collect();
    let mut picks = Vec::new();
    let mut seen = HashSet::new();
    for (_, id) in ids.iter().take(4) {
        let Ok(related) = get_json(&format!("https://api.deezer.com/artist/{id}/related?limit=4")) else { continue };
        for other in related["data"].as_array().cloned().unwrap_or_default() {
            let name = other["name"].as_str().unwrap_or("").to_owned();
            if known.contains(&name.to_lowercase()) || !seen.insert(name.clone()) {
                continue;
            }
            let Ok(list) = get_json(&format!("https://api.deezer.com/artist/{}/albums?limit=25", other["id"])) else { continue };
            let best = list["data"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|album| album["record_type"] == "album")
                .max_by_key(|album| album["fans"].as_u64().unwrap_or(0));
            if let Some(album) = best {
                picks.push(catalog::album(&album, Some(&other)));
            }
        }
    }
    random::shuffle(&mut picks);
    json!({
        "id": "albums-for-you", "title": "Albums for you",
        "subtitle": "By artists like the ones you play.",
        "albums": picks,
    })
}

fn saved_path() -> Option<PathBuf> {
    crate::library::data_dir().map(|dir| dir.join("for-you.json"))
}

/// Made for you, from the listening history and `favourites` (liked songs and
/// playlist songs, with what each counts as in plays). Weekly picks are kept
/// for the week (starting Monday) and Daily Mixes for the day, unless
/// `refresh`, or unless there's more to go on than when they were made.
pub fn build(history: &[Value], favourites: &[(Value, usize)], refresh: bool) -> Result<Value, String> {
    let now = now_ms();
    let today = (now / DAY_MS) as i64;
    let week = (today + 3) / 7; // weeks starting on Monday (1970-01-01 was a Thursday)
    let taste = taste(history, favourites, now);
    if taste.songs.len() < SONGS_NEEDED {
        return Ok(json!({ "needMore": true, "songs": taste.songs.len(), "songsNeeded": SONGS_NEEDED }));
    }

    let saved: Value = saved_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    // Mixes made from only a song or two are made again once there's more to go on.
    let seeds = taste.songs.len().min(6) as u64;
    let grown = saved["seeds"].as_u64().unwrap_or(0) < seeds;
    let weekly_fresh = !refresh && !grown && saved["week"].as_i64() == Some(week) && saved["weekly"].is_object();
    let daily_fresh = !refresh && !grown && saved["day"].as_i64() == Some(today) && saved["daily"].is_array();

    // The slow parts (dozens of requests) run side by side.
    let (weekly, daily, radar, albums) = std::thread::scope(|scope| {
        let weekly = (!weekly_fresh).then(|| scope.spawn(|| discover_weekly(&taste)));
        let daily = (!daily_fresh).then(|| scope.spawn(|| daily_mixes(&taste)));
        let catalogue = (!weekly_fresh).then(|| {
            scope.spawn(|| {
                let ids: Vec<(String, String)> = taste
                    .artists
                    .iter()
                    .take(10)
                    .filter_map(|(name, _, _, known)| deezer_artist(name, known).map(|id| (name.clone(), id)))
                    .collect();
                (release_radar(&taste, &ids, today), albums_for_you(&taste, &ids))
            })
        });
        let weekly = weekly.map(|job| job.join().unwrap_or(Value::Null)).unwrap_or_else(|| saved["weekly"].clone());
        let daily = daily.map(|job| json!(job.join().unwrap_or_default())).unwrap_or_else(|| saved["daily"].clone());
        let (radar, albums) = catalogue
            .map(|job| job.join().unwrap_or((Value::Null, Value::Null)))
            .unwrap_or_else(|| (saved["radar"].clone(), saved["albums"].clone()));
        (weekly, daily, radar, albums)
    });

    let result = json!({ "week": week, "day": today, "seeds": seeds, "weekly": weekly, "daily": daily, "radar": radar, "albums": albums });
    if let Some(path) = saved_path() {
        let _ = std::fs::write(path, result.to_string());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(key: &str, artist: &str, video: &str, at: u64) -> Value {
        json!({ "key": key, "title": key, "artist": artist, "video": video, "playedAt": at })
    }

    #[test]
    fn counts_songs_and_artists() {
        let now = 100 * DAY_MS;
        let history = vec![
            play("a", "Koichi Sugiyama", "v1", now - 1000),
            play("a", "Koichi Sugiyama", "v1", now - 900),
            play("b", "Hans Zimmer", "v2", now - 800),
            play("old", "Someone", "v3", now - 90 * DAY_MS),
        ];
        let t = taste(&history, &[], now);
        assert_eq!(t.songs[0].0["key"], "a");
        assert_eq!(t.songs[0].2, 2);
        assert_eq!(t.artists[0].0, "Koichi Sugiyama");
        // A liked song counts double, however long ago it was liked.
        let liked = taste(&history, &[(play("liked", "Hans Zimmer", "v4", 0), LIKED_WEIGHT)], now);
        assert_eq!(liked.artists[0].0, "Hans Zimmer");
        assert!(liked.heard.contains("yt:v4"), "liked songs don't come back as new");
        assert!(t.heard.contains("yt:v3"), "old plays still count as heard");
        assert!(!t.songs.iter().any(|s| s.0["key"] == "old"), "but not as recent taste");
    }

    /// Builds everything from a small history: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_build() {
        // SAFETY: only this ignored test touches the variable.
        unsafe { std::env::set_var("EARSHOT_DATA_DIR", std::env::temp_dir().join("earshot-foryou-test")) };
        let now = now_ms();
        let mut history = Vec::new();
        for (i, (key, artist, artist_id, video)) in [
            ("dz:3135556", "Daft Punk", "27", "5NV6Rdv1a3I"),
            ("dz:3135553", "Daft Punk", "27", "FGBhQbmPwH8"),
            ("yt:jx-kcjc_v00", "Hans Zimmer", "", "jx-kcjc_v00"),
            ("dz:916424", "Justice", "6404", "sy1dYFGkPUE"),
            ("yt:WqdpKnx6TfY", "Koichi Sugiyama", "", "WqdpKnx6TfY"),
            ("dz:3135554", "Daft Punk", "27", "LKYPYj2XX80"),
        ]
        .iter()
        .enumerate()
        {
            history.push(json!({ "key": key, "title": key, "artist": artist, "artistId": artist_id, "video": video, "playedAt": now - (i as u64) * 60_000 }));
        }
        let built = build(&history, &[], true).unwrap();
        let weekly = built["weekly"]["tracks"].as_array().unwrap();
        assert!(weekly.len() >= 10, "Discover Weekly has {} songs", weekly.len());
        assert!(weekly.iter().all(|t| !["yt:jx-kcjc_v00", "yt:WqdpKnx6TfY"].contains(&t["key"].as_str().unwrap())));
        assert_eq!(built["daily"].as_array().unwrap().len(), 3);
        assert!(!built["albums"]["albums"].as_array().unwrap().is_empty(), "Albums for you");
        assert!(built["radar"]["albums"].is_array());
    }

    #[test]
    fn reads_dates() {
        assert_eq!(days("1970-01-01"), Some(0));
        assert_eq!(days("2026-11-05"), Some(20_762));
        assert_eq!(days("nope"), None);
    }
}
