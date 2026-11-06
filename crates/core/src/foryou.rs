//! Made for you: Discover Weekly, Daily Mixes, Release Radar and albums
//! picked for you, built from the listening history kept on this computer.
//! New songs come from YouTube's own mixes for the songs you play most; new
//! releases and related artists from Deezer's catalogue.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::net::{encode, get_json};
use crate::{archive, bandcamp, catalog, mixcloud, musicbrainz, random, soundcloud, youtube};

const DAY_MS: u64 = 86_400_000;
/// Fewer different songs than this and there's nothing to go on yet.
pub const SONGS_NEEDED: usize = 1;
/// How much a liked song counts, in plays (a song in a playlist counts one).
pub const LIKED_WEIGHT: usize = 2;
/// Which way of making the mixes saved ones came from (2: from every source).
const MADE_BY: u64 = 2;

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

fn tracks_of(found: Result<Value, String>) -> Vec<Value> {
    found.ok().and_then(|value| value["tracks"].as_array().cloned()).unwrap_or_default()
}

/// Songs like `seed` from every source that has some, one list per source:
/// the seed's own site's suggestions (SoundCloud's related tracks, Mixcloud's
/// similar shows, Bandcamp's recommendations, the Internet Archive's other
/// recordings), YouTube's mix for it, and, for songs from YouTube or the
/// catalogue, the artist on Bandcamp and SoundCloud.
fn similar(seed: &Value, video: &str) -> Vec<Vec<Value>> {
    let artist = seed["artist"].as_str().unwrap_or("").trim().to_owned();
    let own = match seed["service"].as_str() {
        Some("soundcloud") => seed["soundcloudId"].as_u64().map(soundcloud::related),
        Some("mixcloud") => seed["mixcloudKey"].as_str().map(mixcloud::similar),
        Some("bandcamp") => Some(bandcamp::related(seed)),
        Some("archive") => Some(archive::related(seed)),
        _ => None,
    };
    let elsewhere = own.is_none() && !artist.is_empty();
    std::thread::scope(|scope| {
        let youtube = scope.spawn(|| video_for(seed, video).and_then(|video| youtube::mix(&video).ok()).unwrap_or_default());
        let bandcamp = elsewhere.then(|| scope.spawn(|| tracks_of(bandcamp::search(&artist))));
        let soundcloud = elsewhere.then(|| scope.spawn(|| tracks_of(soundcloud::search(&artist))));
        let mut lists = vec![own.and_then(Result::ok).unwrap_or_default(), youtube.join().unwrap_or_default()];
        for job in [bandcamp, soundcloud].into_iter().flatten() {
            // Only songs actually by the artist, not everything the search matched.
            let found: Vec<Value> = job.join().unwrap_or_default();
            let wanted = artist.to_lowercase();
            let by_artist = |name: &str| {
                let name = name.trim().to_lowercase();
                !name.is_empty() && (name.contains(&wanted) || wanted.contains(&name))
            };
            lists.push(found.into_iter().filter(|t| t["artist"].as_str().is_some_and(by_artist)).collect());
        }
        lists
    })
}

/// Takes up to `count` songs that haven't been heard (and aren't in
/// `taken`), a source at a time in turn, so every source gets a say.
fn pick(mut lists: Vec<Vec<Value>>, heard: &HashSet<String>, taken: &mut HashSet<String>, count: usize) -> Vec<Value> {
    for list in &mut lists {
        random::shuffle(list);
        list.reverse(); // popped from the end
    }
    let mut picked = Vec::new();
    while picked.len() < count && lists.iter().any(|list| !list.is_empty()) {
        for list in &mut lists {
            while let Some(song) = list.pop() {
                let key = song["key"].as_str().unwrap_or("").to_owned();
                let video = song["youtubeId"].as_str().map(|v| format!("yt:{v}")).unwrap_or_default();
                if !key.is_empty() && !heard.contains(&key) && !heard.contains(&video) && taken.insert(key) {
                    picked.push(song);
                    break;
                }
            }
            if picked.len() >= count {
                break;
            }
        }
    }
    picked
}

/// New songs like `seed`, from all the sources, that haven't been heard.
fn new_like(seed: &Value, video: &str, heard: &HashSet<String>, taken: &mut HashSet<String>, count: usize) -> Vec<Value> {
    pick(similar(seed, video), heard, taken, count)
}

fn discover_weekly(taste: &Taste) -> Value {
    let mut taken = HashSet::new();
    let mut tracks = Vec::new();
    // About 30 songs, from up to six favourites (more from each when there are few).
    let seeds: Vec<_> = taste.songs.iter().take(6).collect();
    let per_seed = (30 / seeds.len().max(1)).max(5);
    // Every favourite's suggestions are looked up at the same time.
    let found: Vec<Vec<Vec<Value>>> =
        std::thread::scope(|scope| seeds.iter().map(|(seed, video, _)| scope.spawn(move || similar(seed, video))).collect::<Vec<_>>().into_iter().map(|job| job.join().unwrap_or_default()).collect());
    for lists in found {
        tracks.extend(pick(lists, &taste.heard, &mut taken, per_seed));
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

/// Where the listener's artists can be found: Deezer's catalogue, MusicBrainz,
/// and the YouTube channels their songs come from (many aren't anywhere else).
struct Artists {
    deezer: Vec<(String, String)>,
    musicbrainz: Vec<(String, String)>,
    channels: Vec<String>,
}

fn find_artists(taste: &Taste) -> Artists {
    let names: Vec<(String, String)> = taste.artists.iter().take(10).map(|(name, _, _, known)| (name.clone(), known.clone())).collect();
    let mut channels: Vec<String> = Vec::new();
    for (song, video, _) in &taste.songs {
        let on_youtube = !video.is_empty() || song["key"].as_str().is_some_and(|k| k.starts_with("yt:"));
        let channel = song["channel"].as_str().or(song["artist"].as_str()).unwrap_or("").trim().to_owned();
        if on_youtube && song["service"].is_null() && !channel.is_empty() && !channels.iter().any(|c| c.eq_ignore_ascii_case(&channel)) {
            channels.push(channel);
        }
    }
    channels.truncate(6);
    std::thread::scope(|scope| {
        let deezer = scope.spawn(|| names.iter().filter_map(|(name, known)| deezer_artist(name, known).map(|id| (name.clone(), id))).collect());
        let musicbrainz = scope.spawn(|| names.iter().filter_map(|(name, _)| musicbrainz::artist_id(name).map(|id| (name.clone(), id))).collect());
        Artists { deezer: deezer.join().unwrap_or_default(), musicbrainz: musicbrainz.join().unwrap_or_default(), channels }
    })
}

/// Albums (with release days where known) by the listener's artists from
/// every source; with `within` days, only releases that recent.
fn releases_from(artists: &Artists, today: i64, within: Option<i64>) -> Vec<(i64, Value)> {
    let recent = |day: i64| within.is_none_or(|limit| day <= today && today - day <= limit);
    std::thread::scope(|scope| {
        let deezer = scope.spawn(|| {
            let mut found = Vec::new();
            for (name, id) in &artists.deezer {
                let Ok(list) = get_json(&format!("https://api.deezer.com/artist/{id}/albums?limit=50")) else { continue };
                let artist = json!({ "id": id, "name": name });
                for item in list["data"].as_array().cloned().unwrap_or_default() {
                    if let Some(day) = item["release_date"].as_str().and_then(days).filter(|day| recent(*day)) {
                        found.push((day, catalog::album(&item, Some(&artist))));
                    }
                }
            }
            found
        });
        let musicbrainz = scope.spawn(|| {
            let mut found = Vec::new();
            for (name, id) in &artists.musicbrainz {
                for release in musicbrainz::releases(id, name) {
                    if let Some(day) = release["releaseDate"].as_str().and_then(days).filter(|day| recent(*day)) {
                        found.push((day, release));
                    }
                }
            }
            found
        });
        // YouTube's filter gives this month's uploads; they count as new.
        let youtube: Vec<_> = artists
            .channels
            .iter()
            .map(|channel| scope.spawn(move || youtube::uploads(channel, within.is_some())))
            .collect();
        let mut all: Vec<(i64, Value)> = deezer.join().unwrap_or_default();
        all.extend(musicbrainz.join().unwrap_or_default());
        for job in youtube {
            all.extend(job.join().unwrap_or_default().into_iter().take(4).map(|album| (today, album)));
        }
        all
    })
}

/// The same release seen in two places counts once (the first, dated, one).
fn distinct(albums: Vec<(i64, Value)>) -> Vec<(i64, Value)> {
    let mut seen = HashSet::new();
    albums
        .into_iter()
        .filter(|(_, album)| {
            let name = |key: &str| album[key].as_str().unwrap_or("").trim().to_lowercase();
            seen.insert((name("artist"), name("title")))
        })
        .collect()
}

fn release_radar(artists: &Artists, today: i64) -> Value {
    let mut albums = distinct(releases_from(artists, today, Some(90)));
    let mut subtitle = "New from artists you listen to, from the last three months: albums, singles and uploads.";
    if albums.is_empty() {
        // Nothing that recent: each artist's latest instead, so there's always something.
        let mut latest = distinct(releases_from(artists, today, None));
        latest.sort_by(|a, b| b.0.cmp(&a.0));
        let mut per_artist: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        albums = latest
            .into_iter()
            .filter(|(_, album)| {
                let count = per_artist.entry(album["artist"].as_str().unwrap_or("").to_lowercase()).or_insert(0);
                *count += 1;
                *count <= 3
            })
            .collect();
        subtitle = "Nothing new in the last three months, so here's the latest from artists you listen to.";
    }
    albums.sort_by(|a, b| b.0.cmp(&a.0));
    json!({
        "id": "release-radar", "title": "Release Radar",
        "subtitle": subtitle,
        "albums": albums.into_iter().map(|(_, album)| album).take(24).collect::<Vec<_>>(),
    })
}

/// An artist's best-known album: Deezer's most popular, or MusicBrainz's newest.
fn best_album(name: &str, musicbrainz_id: Option<&str>) -> Option<Value> {
    if let Some(id) = deezer_artist(name, "") {
        let list = get_json(&format!("https://api.deezer.com/artist/{id}/albums?limit=25")).ok()?;
        let best = list["data"]
            .as_array()?
            .iter()
            .filter(|album| album["record_type"] == "album")
            .max_by_key(|album| album["fans"].as_u64().unwrap_or(0))?
            .clone();
        return Some(catalog::album(&best, Some(&json!({ "id": id, "name": name }))));
    }
    musicbrainz::releases(musicbrainz_id?, name).into_iter().find(|release| release["kind"] == "album")
}

fn albums_for_you(taste: &Taste, artists: &Artists) -> Value {
    let known: HashSet<String> = taste.artists.iter().map(|a| a.0.to_lowercase()).collect();
    let seen = Mutex::new(known.clone());
    // Each artist once, and none the listener already plays.
    let fresh = |name: &str| !name.trim().is_empty() && seen.lock().unwrap().insert(name.trim().to_lowercase());
    let picks: Vec<Value> = std::thread::scope(|scope| {
        // Deezer's related artists.
        let deezer = scope.spawn(|| {
            let mut found = Vec::new();
            for (_, id) in artists.deezer.iter().take(4) {
                let Ok(related) = get_json(&format!("https://api.deezer.com/artist/{id}/related?limit=4")) else { continue };
                for other in related["data"].as_array().cloned().unwrap_or_default() {
                    let name = other["name"].as_str().unwrap_or("").to_owned();
                    if fresh(&name) {
                        found.extend(best_album(&name, None));
                    }
                }
            }
            found
        });
        // ListenBrainz: artists people listen to alongside yours.
        let listenbrainz = scope.spawn(|| {
            let mut found = Vec::new();
            for (_, id) in artists.musicbrainz.iter().take(3) {
                for (name, mbid) in musicbrainz::similar_artists(id).into_iter().take(8) {
                    if found.len() >= 8 {
                        break;
                    }
                    if fresh(&name) {
                        found.extend(best_album(&name, Some(&mbid)));
                    }
                }
            }
            found
        });
        // YouTube: the other channels in the mixes of the listener's favourite songs.
        let youtube = scope.spawn(|| {
            let mut found = Vec::new();
            for (seed, video, _) in taste.songs.iter().take(3) {
                let Some(video) = video_for(seed, video) else { continue };
                for track in youtube::mix(&video).unwrap_or_default().into_iter().take(15) {
                    let channel = track["channel"].as_str().or(track["artist"].as_str()).unwrap_or("").to_owned();
                    if found.len() < 10 && fresh(&channel) {
                        found.extend(youtube::uploads(&channel, false).into_iter().next());
                    }
                }
            }
            found
        });
        // Bandcamp: what the pages of the listener's Bandcamp songs recommend.
        let bandcamp = scope.spawn(|| {
            taste
                .songs
                .iter()
                .filter(|(song, _, _)| song["service"] == "bandcamp")
                .take(2)
                .flat_map(|(song, _, _)| bandcamp::recommended_albums(song, 4))
                .collect::<Vec<_>>()
        });
        let mut all: Vec<Value> = Vec::new();
        for job in [deezer, listenbrainz, youtube, bandcamp] {
            all.extend(job.join().unwrap_or_default());
        }
        all
    });
    let mut picks = picks;
    random::shuffle(&mut picks);
    picks.truncate(24);
    json!({
        "id": "albums-for-you", "title": "Albums for you",
        "subtitle": "By artists like the ones you play: from YouTube's mixes, Bandcamp's recommendations, ListenBrainz and Deezer.",
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
    // Mixes saved by an Earshot that made them differently are made again too.
    let grown = saved["seeds"].as_u64().unwrap_or(0) < seeds || saved["made"].as_u64() != Some(MADE_BY);
    let weekly_fresh = !refresh && !grown && saved["week"].as_i64() == Some(week) && saved["weekly"].is_object();
    let daily_fresh = !refresh && !grown && saved["day"].as_i64() == Some(today) && saved["daily"].is_array();

    // The slow parts (dozens of requests) run side by side.
    let (weekly, daily, radar, albums) = std::thread::scope(|scope| {
        let weekly = (!weekly_fresh).then(|| scope.spawn(|| discover_weekly(&taste)));
        let daily = (!daily_fresh).then(|| scope.spawn(|| daily_mixes(&taste)));
        let catalogue = (!weekly_fresh).then(|| {
            scope.spawn(|| {
                let artists = find_artists(&taste);
                std::thread::scope(|inner| {
                    let radar = inner.spawn(|| release_radar(&artists, today));
                    let albums = albums_for_you(&taste, &artists);
                    (radar.join().unwrap_or(Value::Null), albums)
                })
            })
        });
        let weekly = weekly.map(|job| job.join().unwrap_or(Value::Null)).unwrap_or_else(|| saved["weekly"].clone());
        let daily = daily.map(|job| json!(job.join().unwrap_or_default())).unwrap_or_else(|| saved["daily"].clone());
        let (radar, albums) = catalogue
            .map(|job| job.join().unwrap_or((Value::Null, Value::Null)))
            .unwrap_or_else(|| (saved["radar"].clone(), saved["albums"].clone()));
        (weekly, daily, radar, albums)
    });

    let result = json!({ "week": week, "day": today, "seeds": seeds, "made": MADE_BY, "weekly": weekly, "daily": daily, "radar": radar, "albums": albums });
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
