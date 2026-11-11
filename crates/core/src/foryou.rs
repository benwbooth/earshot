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
use crate::{archive, bandcamp, catalog, mixcloud, musicbrainz, radio, random, soundcloud, youtube};

const DAY_MS: u64 = 86_400_000;
/// Fewer different songs than this and there's nothing to go on yet.
pub const SONGS_NEEDED: usize = 1;
/// How much a liked song counts, in plays (a song in a playlist counts one).
pub const LIKED_WEIGHT: usize = 2;
/// Which way of making the mixes saved ones came from (4: every source,
/// Deezer, the Internet Archive and Reddit too, taking turns; 5: no one
/// game, artist or album taking over; 6: branching out to new artists).
/// Only a note: saved mixes are kept until they're due whichever Earshot
/// made them, since the app and the web server share them and may not be
/// the same version.
const MADE_BY: u64 = 6;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// How far this computer's clock is ahead of UTC, in ms, so days start at
/// local midnight rather than UTC's (4 or 5 pm in California).
#[cfg(unix)]
fn local_offset_ms(now: u64) -> i64 {
    let secs = (now / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff as i64 * 1000
}

#[cfg(not(unix))]
fn local_offset_ms(_now: u64) -> i64 {
    0
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
    // "Donkey Kong Country 2", "... 2 OST" and "Donkey Kong Country" are one
    // artist as far as the mixes go: the most played name takes the others in.
    let mut merged: Vec<(String, Vec<Value>, usize, String)> = Vec::new();
    for artist in artists {
        match merged.iter_mut().find(|other| same_name(&other.0, &artist.0)) {
            Some(other) => {
                other.2 += artist.2;
                for song in artist.1 {
                    if !other.1.iter().any(|t| t["key"] == song["key"]) {
                        other.1.push(song);
                    }
                }
                if other.3.is_empty() {
                    other.3 = artist.3;
                }
            }
            None => merged.push(artist),
        }
    }
    merged.sort_by(|a, b| b.2.cmp(&a.2));
    Taste { songs, artists: merged, heard }
}

impl Taste {
    /// Up to `count` favourite songs, by different artists while there are
    /// any (so one game's channels can't be every seed), then the rest.
    fn varied(&self, count: usize) -> Vec<&(Value, String, usize)> {
        let mut picked: Vec<&(Value, String, usize)> = Vec::new();
        for song in &self.songs {
            if picked.len() < count && !picked.iter().any(|other| same_name(other.0["artist"].as_str().unwrap_or(""), song.0["artist"].as_str().unwrap_or(""))) {
                picked.push(song);
            }
        }
        for song in &self.songs {
            if picked.len() < count && !picked.iter().any(|other| std::ptr::eq(*other, song)) {
                picked.push(song);
            }
        }
        picked
    }
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

/// The words of a name worth matching on: "Donkey Kong Country 2 OST" ->
/// donkey, kong, country, 2 (not "OST", "Music" or "- Topic").
fn name_words(name: &str) -> Vec<String> {
    const NOISE: [&str; 12] = ["ost", "music", "official", "topic", "vevo", "soundtrack", "records", "the", "and", "youtube", "soundcloud", "bandcamp"];
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !NOISE.contains(word))
        .map(str::to_owned)
        .collect()
}

/// Whether two names are the same artist, game or series as far as mixes go:
/// every word of one is in the other ("Donkey Kong Country 2 OST" and
/// "Donkey Kong Country"; "Lofi Fruits Music" and "Lofi Fruits Music, Chill Fruits Music").
fn same_name(a: &str, b: &str) -> bool {
    let (a, b) = (name_words(a), name_words(b));
    !a.is_empty() && !b.is_empty() && (a.iter().all(|w| b.contains(w)) || b.iter().all(|w| a.contains(w)))
}

/// A name without its numbers, for the series it's part of: "Donkey Kong
/// Country 2" -> "donkey kong country" (a name that's only numbers stays).
fn series(name: &str) -> String {
    let words: Vec<String> = name_words(name).into_iter().filter(|w| !w.chars().all(|c| c.is_ascii_digit())).collect();
    if words.is_empty() { name.to_owned() } else { words.join(" ") }
}

/// Whether a song a search for `artist` found is really to do with them: by
/// them, or with every word of their name in its title or album. (A YouTube
/// song's "artist" is often its channel, named after a game or a series,
/// so covers and remixes on Bandcamp count; songs that only share a word don't.)
fn about(track: &Value, artist: &str) -> bool {
    let wanted = artist.trim().to_lowercase();
    let by = track["artist"].as_str().unwrap_or("").trim().to_lowercase();
    if !by.is_empty() && !wanted.is_empty() && (by.contains(&wanted) || wanted.contains(&by)) {
        return true;
    }
    let words = name_words(artist);
    let text = format!("{} {}", track["title"].as_str().unwrap_or(""), track["album"].as_str().unwrap_or("")).to_lowercase();
    let found = name_words(&text);
    !words.is_empty() && words.iter().all(|word| found.contains(word))
}

/// Songs a source found, with the source's name ("bandcamp", "reddit"...).
type Found = Vec<(&'static str, Vec<Value>)>;

/// Deezer's songs for an artist: their own top songs and those of the
/// artists Deezer finds related, or songs a search finds about them.
fn deezer_like(seed: &Value, artist: &str) -> Vec<Value> {
    let known = if seed["key"].as_str().is_some_and(|k| k.starts_with("dz:")) { seed["artistId"].as_str().unwrap_or("") } else { "" };
    let top = |id: &str, count: usize| -> Vec<Value> {
        get_json(&format!("https://api.deezer.com/artist/{id}/top?limit={count}"))
            .ok()
            .and_then(|found| found["data"].as_array().map(|songs| songs.iter().map(|song| catalog::track(song, None)).collect()))
            .unwrap_or_default()
    };
    let Some(id) = deezer_artist(artist, known) else {
        return tracks_of(catalog::search(artist)).into_iter().filter(|t| about(t, artist)).collect();
    };
    let mut songs = top(&id, 5);
    if let Ok(related) = get_json(&format!("https://api.deezer.com/artist/{id}/related?limit=4")) {
        for other in related["data"].as_array().cloned().unwrap_or_default() {
            songs.extend(top(&other["id"].to_string(), 4));
        }
    }
    songs
}

/// A few songs from each of a couple of the Internet Archive's recordings about an artist.
fn archive_like(artist: &str) -> Vec<Value> {
    let Ok(found) = archive::search(artist) else { return Vec::new() };
    let mut items: Vec<Value> = found["albums"].as_array().cloned().unwrap_or_default().into_iter().filter(|item| about(item, artist)).take(6).collect();
    random::shuffle(&mut items);
    let mut songs = Vec::new();
    for item in items.iter().take(2) {
        let Some(Ok(album)) = item["id"].as_str().map(archive::album) else { continue };
        let mut tracks = album["tracks"].as_array().cloned().unwrap_or_default();
        random::shuffle(&mut tracks);
        songs.extend(tracks.into_iter().take(4));
    }
    songs
}

/// Songs shared on Reddit in the stations the listener has tuned in to:
/// ones about the artist first, then a few others (people's picks, not an
/// algorithm's; only a few, so they don't take over when other sources run dry).
fn reddit_like(artist: &str) -> Vec<Value> {
    let (mut matching, mut rest): (Vec<Value>, Vec<Value>) = radio::saved_songs().into_iter().partition(|song| about(song, artist));
    random::shuffle(&mut matching);
    random::shuffle(&mut rest);
    matching.extend(rest.into_iter().take(3));
    matching
}

/// Songs like `seed` from every source that has some, one list per source:
/// the seed's own site's suggestions (SoundCloud's related tracks, Mixcloud's
/// similar shows, Bandcamp's recommendations, the Internet Archive's other
/// recordings), YouTube's mix for it, Deezer's related artists, songs about
/// the artist on Bandcamp, SoundCloud, Mixcloud and the Internet Archive (each
/// but the seed's own site, which already had its say), and Reddit's.
fn similar(seed: &Value, video: &str) -> Found {
    similar_by(seed, video, true)
}

/// Like `similar`, but with `searches` false only what the sites themselves
/// relate to the seed (YouTube's mix, Deezer's related artists, the seed's own
/// site's suggestions), not searches for the artist's name: for artists only
/// just found, whose names may be a common word.
fn similar_by(seed: &Value, video: &str, searches: bool) -> Found {
    let artist = seed["artist"].as_str().unwrap_or("").trim().to_owned();
    // An "artist" that's only the site's name ("YouTube") is nobody to search for.
    let artist = if name_words(&artist).is_empty() { String::new() } else { artist };
    let service = seed["service"].as_str().unwrap_or("");
    let own: Option<(&'static str, Result<Vec<Value>, String>)> = match service {
        "soundcloud" => seed["soundcloudId"].as_u64().map(|id| ("soundcloud", soundcloud::related(id))),
        "mixcloud" => seed["mixcloudKey"].as_str().map(|key| ("mixcloud", mixcloud::similar(key))),
        "bandcamp" => Some(("bandcamp", bandcamp::related(seed))),
        "archive" => Some(("archive", archive::related(seed))),
        _ => None,
    };
    let search = |site: &str| searches && !artist.is_empty() && service != site;
    let about_artist = |found: Vec<Value>| -> Vec<Value> { found.into_iter().filter(|t| about(t, &artist)).collect() };
    let mut lists: Found = std::thread::scope(|scope| {
        let youtube = scope.spawn(|| video_for(seed, video).and_then(|video| youtube::mix(&video).ok()).unwrap_or_default());
        let deezer = (!artist.is_empty()).then(|| scope.spawn(|| deezer_like(seed, &artist)));
        let bandcamp = search("bandcamp").then(|| scope.spawn(|| tracks_of(bandcamp::search(&artist))));
        let soundcloud = search("soundcloud").then(|| scope.spawn(|| tracks_of(soundcloud::search(&artist))));
        let mixcloud = search("mixcloud").then(|| scope.spawn(|| tracks_of(mixcloud::search(&artist))));
        let archive = search("archive").then(|| scope.spawn(|| archive_like(&artist)));
        let mut lists: Found = vec![("youtube", youtube.join().unwrap_or_default())];
        lists.extend(own.map(|(source, found)| (source, found.unwrap_or_default())));
        lists.extend(deezer.map(|job| ("deezer", job.join().unwrap_or_default())));
        lists.extend(archive.map(|job| ("archive", job.join().unwrap_or_default())));
        // Only songs to do with the artist, not everything a search matched.
        for (source, job) in [("bandcamp", bandcamp), ("soundcloud", soundcloud), ("mixcloud", mixcloud)] {
            lists.extend(job.map(|job| (source, about_artist(job.join().unwrap_or_default()))));
        }
        lists
    });
    for (source, songs) in &mut lists {
        if *source != "reddit" {
            random::shuffle(songs);
        }
    }
    if searches && !artist.is_empty() {
        lists.push(("reddit", reddit_like(&artist)));
    }
    lists
}

/// Takes up to `count` songs that haven't been heard (and aren't in
/// `taken`) from what was found for each favourite, a source at a time in
/// turn so every source gets an equal say (a source that runs out leaves
/// its turns to the rest), and each favourite in turn within a source.
/// No more than `per_theme` songs to do with any one of `themes` (the
/// favourites' artists: every source searching for a game's name finds
/// covers, remixes and podcasts of that game), and only a few by any one
/// artist or from any one album.
fn blend(found: Vec<Found>, heard: &HashSet<String>, taken: &mut HashSet<String>, count: usize, themes: &[String], per_theme: usize) -> Vec<Value> {
    let per_artist = (count / 10).max(2);
    const PER_ALBUM: usize = 2;
    // Any game of a series counts towards it (Donkey Kong Country 3 for 2's).
    let themes: Vec<String> = themes.iter().map(|theme| series(theme)).collect();
    let mut by_theme = vec![0usize; themes.len()];
    let mut by_artist: Vec<(String, usize)> = Vec::new();
    let mut by_album: HashMap<String, usize> = HashMap::new();
    let mut by_source: Vec<(&str, Vec<Vec<Value>>)> = Vec::new();
    for lists in found {
        for (source, songs) in lists {
            match by_source.iter_mut().find(|(name, _)| *name == source) {
                Some((_, all)) => all.push(songs),
                None => by_source.push((source, vec![songs])),
            }
        }
    }
    let mut queues: Vec<std::vec::IntoIter<Value>> = by_source
        .into_iter()
        .map(|(_, lists)| {
            let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
            let mut lists: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
            (0..longest).flat_map(|_| lists.iter_mut().filter_map(Iterator::next).collect::<Vec<_>>()).collect::<Vec<_>>().into_iter()
        })
        .collect();
    random::shuffle(&mut queues);
    let mut picked = Vec::new();
    let mut going = true;
    while picked.len() < count && going {
        going = false;
        for queue in &mut queues {
            for song in queue.by_ref() {
                let key = song["key"].as_str().unwrap_or("").to_owned();
                let video = song["youtubeId"].as_str().filter(|v| !v.is_empty()).map(|v| format!("yt:{v}")).unwrap_or_default();
                if key.is_empty() || heard.contains(&key) || heard.contains(&video) || taken.contains(&key) {
                    continue;
                }
                let theme_hits: Vec<usize> = themes.iter().enumerate().filter(|(_, theme)| about(&song, theme)).map(|(i, _)| i).collect();
                if theme_hits.iter().any(|&i| by_theme[i] >= per_theme) {
                    continue;
                }
                let artist = song["artist"].as_str().unwrap_or("").trim().to_owned();
                let artist_at = by_artist.iter().position(|(name, _)| same_name(name, &artist));
                if artist_at.is_some_and(|at| by_artist[at].1 >= per_artist) {
                    continue;
                }
                let album = song["album"].as_str().unwrap_or("").trim().to_lowercase();
                if !album.is_empty() && by_album.get(&album).copied().unwrap_or(0) >= PER_ALBUM {
                    continue;
                }
                for i in theme_hits {
                    by_theme[i] += 1;
                }
                match artist_at {
                    Some(at) => by_artist[at].1 += 1,
                    None if !name_words(&artist).is_empty() => by_artist.push((artist, 1)),
                    None => {}
                }
                if !album.is_empty() {
                    *by_album.entry(album).or_default() += 1;
                }
                taken.insert(key);
                if !video.is_empty() {
                    taken.insert(video);
                }
                picked.push(song);
                going = true;
                break;
            }
            if picked.len() >= count {
                break;
            }
        }
    }
    picked
}

/// New songs like `seed`, from all the sources, that haven't been heard.
/// At most `per_theme` of them to do with the seed's artist.
fn new_like(seed: &Value, video: &str, heard: &HashSet<String>, taken: &mut HashSet<String>, count: usize, per_theme: usize) -> Vec<Value> {
    let themes = [seed["artist"].as_str().unwrap_or("").to_owned()];
    blend(vec![similar(seed, video)], heard, taken, count, &themes, per_theme)
}

/// Whether a song is by (or about: covers, remixes, podcasts of) an artist the listener already plays.
fn familiar(taste: &Taste, song: &Value) -> bool {
    taste.artists.iter().any(|(name, _, _, _)| about(song, &series(name)))
}

/// A few songs by artists the listener doesn't play yet, picked from what
/// was found for their favourites: each a different artist, the next step out.
fn branches(taste: &Taste, found: &[Found], count: usize) -> Vec<Value> {
    let mut songs: Vec<&Value> = found
        .iter()
        .flatten()
        .flat_map(|(_, songs)| songs)
        .filter(|song| {
            let key = song["key"].as_str().unwrap_or("");
            !key.is_empty() && !taste.heard.contains(key) && !name_words(song["artist"].as_str().unwrap_or("")).is_empty() && !familiar(taste, song)
        })
        .collect();
    random::shuffle(&mut songs);
    let mut picked: Vec<Value> = Vec::new();
    for song in songs {
        if picked.len() < count && !picked.iter().any(|other| same_name(other["artist"].as_str().unwrap_or(""), song["artist"].as_str().unwrap_or(""))) {
            picked.push(song.clone());
        }
    }
    picked
}

fn discover_weekly(taste: &Taste) -> Value {
    let mut taken = HashSet::new();
    // About 30 songs, from what's like up to six favourites (by different
    // artists where there are enough), every source in turn.
    let seeds = taste.varied(6);
    // Every favourite's suggestions are looked up at the same time.
    let mut found: Vec<Found> =
        std::thread::scope(|scope| seeds.iter().map(|(seed, video, _)| scope.spawn(move || similar(seed, video))).collect::<Vec<_>>().into_iter().map(|job| job.join().unwrap_or_default()).collect());
    // Then a step further out: what's like a few of the new artists found,
    // so the mix explores rather than circling the same favourites.
    let further = branches(taste, &found, 4);
    let more: Vec<Found> = std::thread::scope(|scope| {
        let video = |song: &Value| song["youtubeId"].as_str().unwrap_or("").to_owned();
        further.iter().map(|song| scope.spawn(move || similar_by(song, &video(song), false))).collect::<Vec<_>>().into_iter().map(|job| job.join().unwrap_or_default()).collect()
    });
    found.extend(more);
    // Mostly artists the listener doesn't play yet: only a couple of songs
    // by or about (covers, remixes) any artist they already do.
    let themes: Vec<String> = taste.artists.iter().take(20).map(|(name, _, _, _)| name.clone()).collect();
    let mut tracks = blend(found, &taste.heard, &mut taken, 30, &themes, 2);
    random::shuffle(&mut tracks);
    json!({
        "id": "discover-weekly", "title": "Discover Weekly",
        "subtitle": "Songs and artists you haven't heard, branching out from the ones you've been playing, from every source. New every Monday.",
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
            let favourite = taste.songs.iter().find(|(song, _, _)| same_name(song["artist"].as_str().unwrap_or(""), name))?;
            // Similar artists, not just more of this one (it already has the songs played).
            tracks.extend(new_like(&favourite.0, &favourite.1, &taste.heard, &mut taken, 17, 4));
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

/// Albums a site's search finds about each of `names` (by someone new to the listener), two for each name.
fn albums_about(names: &[String], search: impl Fn(&str) -> Result<Value, String>, fresh: &dyn Fn(&str) -> bool) -> Vec<Value> {
    let mut found = Vec::new();
    for name in names {
        let albums = search(name).ok().and_then(|value| value["albums"].as_array().cloned()).unwrap_or_default();
        found.extend(albums.into_iter().filter(|album| about(album, name) && fresh(album["artist"].as_str().unwrap_or(""))).take(2));
    }
    found
}

fn albums_for_you(taste: &Taste, artists: &Artists) -> Value {
    let known: HashSet<String> = taste.artists.iter().map(|a| a.0.to_lowercase()).collect();
    let seen = Mutex::new(known.clone());
    // Each artist once, and none the listener already plays.
    let fresh = |name: &str| !name.trim().is_empty() && seen.lock().unwrap().insert(name.trim().to_lowercase());
    // The artists searched for on the sites that have no "related" of their own.
    let names: Vec<String> = taste.artists.iter().take(4).map(|a| a.0.clone()).collect();
    let picks: Vec<Vec<Value>> = std::thread::scope(|scope| {
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
            for (seed, video, _) in taste.varied(3) {
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
        // Bandcamp: what the pages of the listener's Bandcamp songs recommend,
        // and albums about their other artists (covers, remixes, tributes).
        let bandcamp = scope.spawn(|| {
            let mut found: Vec<Value> = taste
                .songs
                .iter()
                .filter(|(song, _, _)| song["service"] == "bandcamp")
                .take(2)
                .flat_map(|(song, _, _)| bandcamp::recommended_albums(song, 4))
                .collect();
            found.extend(albums_about(&names, |name| bandcamp::search(name), &fresh));
            found
        });
        // SoundCloud's albums and playlists, and the Internet Archive's recordings, about the listener's artists.
        let soundcloud = scope.spawn(|| albums_about(&names, |name| soundcloud::search(name), &fresh));
        let archive = scope.spawn(|| albums_about(&names, |name| archive::search(name), &fresh));
        [deezer, listenbrainz, youtube, bandcamp, soundcloud, archive].map(|job| job.join().unwrap_or_default()).into_iter().collect::<Vec<_>>()
    });
    // A source at a time in turn, so every source gets an equal say.
    let mut lists = picks;
    for list in &mut lists {
        random::shuffle(list);
    }
    random::shuffle(&mut lists);
    let mut lists: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
    let mut picks = Vec::new();
    while picks.len() < 24 {
        let round: Vec<Value> = lists.iter_mut().filter_map(Iterator::next).collect();
        if round.is_empty() {
            break;
        }
        picks.extend(round);
    }
    picks.truncate(24);
    json!({
        "id": "albums-for-you", "title": "Albums for you",
        "subtitle": "By artists like the ones you play: from YouTube's mixes, Bandcamp, SoundCloud, the Internet Archive, ListenBrainz and Deezer.",
        "albums": picks,
    })
}

fn saved_path() -> Option<PathBuf> {
    crate::library::data_dir().map(|dir| dir.join("for-you.json"))
}

/// Made for you, from the listening history and `favourites` (liked songs and
/// playlist songs, with what each counts as in plays). Weekly picks are kept
/// for the week (starting Monday) and Daily Mixes for the day (local time),
/// unless `refresh`, or unless there's more to go on than when they were made.
/// Songs in `unplayable` (they wouldn't play anywhere) aren't picked.
pub fn build(history: &[Value], favourites: &[(Value, usize)], unplayable: &HashSet<String>, refresh: bool) -> Result<Value, String> {
    let now = now_ms();
    let today = (now as i64 + local_offset_ms(now)).div_euclid(DAY_MS as i64);
    let week = (today + 3) / 7; // weeks starting on Monday (1970-01-01 was a Thursday)
    let mut taste = taste(history, favourites, now);
    taste.heard.extend(unplayable.iter().cloned());
    if taste.songs.len() < SONGS_NEEDED {
        return Ok(json!({ "needMore": true, "songs": taste.songs.len(), "songsNeeded": SONGS_NEEDED }));
    }

    let saved: Value = saved_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    // Mixes made from only a song or two are made again once there's more to go on.
    let seeds = taste.songs.len().min(6) as u64;
    // Not on a new way of making them, though: the app and the web server
    // share these, and two versions would each throw out the other's mixes.
    let grown = saved["seeds"].as_u64().unwrap_or(0) < seeds;
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
    // Bandcamp songs found by search come without their length; fill it in
    // (once: the mixes are saved with it).
    let (mut weekly, mut daily) = (weekly, daily);
    if let Some(tracks) = weekly["tracks"].as_array_mut() {
        bandcamp::fill_durations(tracks);
    }
    for mix in daily.as_array_mut().into_iter().flatten() {
        if let Some(tracks) = mix["tracks"].as_array_mut() {
            bandcamp::fill_durations(tracks);
        }
    }

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

    #[cfg(unix)]
    #[test]
    fn days_start_at_local_midnight() {
        // 2026-11-10 12:00 UTC is 04:00 in California (PST, 8 hours behind).
        unsafe { std::env::set_var("TZ", "America/Los_Angeles") };
        assert_eq!(local_offset_ms(1_794_312_000_000), -8 * 3_600_000);
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
        let built = build(&history, &[], &HashSet::new(), true).unwrap();
        let weekly = built["weekly"]["tracks"].as_array().unwrap();
        assert!(weekly.len() >= 10, "Discover Weekly has {} songs", weekly.len());
        assert!(weekly.iter().all(|t| !["yt:jx-kcjc_v00", "yt:WqdpKnx6TfY"].contains(&t["key"].as_str().unwrap())));
        assert_eq!(built["daily"].as_array().unwrap().len(), 3);
        assert!(!built["albums"]["albums"].as_array().unwrap().is_empty(), "Albums for you");
        assert!(built["radar"]["albums"].is_array());
    }

    #[test]
    fn keeps_songs_about_the_artist() {
        let song = |artist: &str, title: &str| json!({ "artist": artist, "title": title, "album": "" });
        // A YouTube channel named after a game: covers and remixes of it count.
        assert!(about(&song("Banjo Guy Ollie", "Stickerbrush Symphony (Donkey Kong Country 2)"), "Donkey Kong Country 2"));
        assert!(about(&song("David Wise", "Lost World Anthem"), "David Wise"));
        // Ignoring "OST", "Music" and the like in the name.
        assert!(about(&song("Someone", "Donkey Kong Country 2 - Bramble Blast"), "Donkey Kong Country 2 OST"));
        // A song that only shares a word doesn't.
        assert!(!about(&song("JK Soul", "Music for the Soul"), "Collosia Music"));
        assert!(!about(&song("Someone", "Donkey Kong 64 - Fungi Forest"), "Donkey Kong Country 2"));
    }

    #[test]
    fn one_game_is_one_artist() {
        assert!(same_name("Donkey Kong Country 2 OST", "Donkey Kong Country 2"));
        assert!(same_name("Donkey Kong Country", "Donkey Kong Country 2"));
        assert!(same_name("Lofi Fruits Music", "Lofi Fruits Music, Chill Fruits Music"));
        assert!(!same_name("Donkey Kong Country 2", "Halo2playa"));
        assert!(!same_name("Music", "Official Music"));
        let now = 100 * DAY_MS;
        let history = vec![
            play("a", "Donkey Kong Country 2", "v1", now),
            play("a", "Donkey Kong Country 2", "v1", now),
            play("a", "Donkey Kong Country 2", "v1", now),
            play("b", "Donkey Kong Country 2 OST", "v2", now),
            play("b", "Donkey Kong Country 2 OST", "v2", now),
            play("c", "Toby Fox", "v3", now),
        ];
        let taste = taste(&history, &[], now);
        assert_eq!(taste.artists.len(), 2);
        assert_eq!((taste.artists[0].0.as_str(), taste.artists[0].2, taste.artists[0].1.len()), ("Donkey Kong Country 2", 5, 2));
        let seeds: Vec<&str> = taste.varied(2).iter().map(|s| s.0["key"].as_str().unwrap()).collect();
        assert_eq!(seeds, ["a", "c"]);
    }

    #[test]
    fn no_game_takes_over_a_mix() {
        let song = |key: &str, artist: &str, title: &str, album: &str| json!({ "key": key, "artist": artist, "title": title, "album": album });
        let mut dkc: Vec<Value> = (0..20).map(|i| song(&format!("d{i}"), &format!("Fan {i}"), &format!("Donkey Kong Country 2 - Remix {i}"), "")).collect();
        dkc.extend((0..10).map(|i| song(&format!("s{i}"), "Donkey Kong Country 2 OST", &format!("Track {i}"), "")));
        let others: Vec<Value> = (0..20).map(|i| song(&format!("o{i}"), &format!("Artist {}", i / 5), &format!("Song {i}"), &format!("Album {}", i / 3))).collect();
        let found = vec![vec![("youtube", dkc.clone()), ("bandcamp", dkc)], vec![("soundcloud", others)]];
        let themes = ["Donkey Kong Country 2".to_owned(), "Someone Else".to_owned()];
        let picked = blend(found, &HashSet::new(), &mut HashSet::new(), 30, &themes, 5);
        let about_dkc = picked.iter().filter(|t| about(t, "Donkey Kong Country 2")).count();
        assert_eq!(about_dkc, 5);
        for artist in 0..4 {
            assert!(picked.iter().filter(|t| t["artist"] == format!("Artist {artist}")).count() <= 3);
        }
        for album in 0..7 {
            assert!(picked.iter().filter(|t| t["album"] == format!("Album {album}")).count() <= 2);
        }
    }

    #[test]
    fn reads_dates() {
        assert_eq!(days("1970-01-01"), Some(0));
        assert_eq!(days("2026-11-05"), Some(20_762));
        assert_eq!(days("nope"), None);
    }
}

