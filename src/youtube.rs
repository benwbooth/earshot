//! YouTube through its own web API (the one youtube.com itself uses):
//! search, playlists and the chapter timestamps of long videos. Nothing is
//! downloaded; songs play in YouTube's own player inside the app.

use serde_json::{Value, json};

use crate::net::post_json;

const CLIENT_VERSION: &str = "2.20251101.00.00";

/// Calls one of youtube.com's own API endpoints.
fn innertube(endpoint: &str, mut body: Value) -> Result<Value, String> {
    body["context"] = json!({
        "client": { "clientName": "WEB", "clientVersion": CLIENT_VERSION, "hl": "en", "gl": "US" }
    });
    let answer = post_json(&format!("https://www.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false"), &body)?;
    if let Some(error) = answer.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("request failed");
        return Err(format!("YouTube: {message}"));
    }
    Ok(answer)
}

/// Every object stored under one of `keys`, in page order (matches aren't searched inside).
fn collect<'a>(value: &'a Value, keys: &[&'static str], out: &mut Vec<(&'static str, &'a Value)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if let Some(found) = keys.iter().find(|wanted| **wanted == key.as_str()) {
                    out.push((found, child));
                } else {
                    collect(child, keys, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect(item, keys, out)),
        _ => {}
    }
}

/// The first object stored under `key`, searching depth-first.
fn first<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map.get(key).or_else(|| map.values().find_map(|child| first(child, key))),
        Value::Array(items) => items.iter().find_map(|item| first(item, key)),
        _ => None,
    }
}

/// The readable text of a YouTube text object (simpleText, runs or content).
fn text(value: &Value) -> String {
    if let Some(simple) = value.get("simpleText").and_then(Value::as_str) {
        return simple.to_owned();
    }
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        return content.to_owned();
    }
    value
        .get("runs")
        .and_then(Value::as_array)
        .map(|runs| runs.iter().filter_map(|run| run.get("text").and_then(Value::as_str)).collect::<String>())
        .unwrap_or_default()
}

/// "1:30:03" -> 5403 seconds.
fn parse_clock(clock: &str) -> u64 {
    if !clock.contains(':') {
        return 0;
    }
    clock.trim().split(':').fold(0, |total, part| total * 60 + part.trim().parse::<u64>().unwrap_or(0))
}

/// Splits "Artist - Title (Official Video)" into its parts.
pub fn split_title(raw: &str, channel: &str) -> (String, String) {
    let mut title = raw.to_owned();
    for noise in [
        "(Official Video)", "(Official Music Video)", "(Official Audio)", "[Official Video]", "[Official Music Video]",
        "[Official Audio]", "(Official Lyric Video)", "(Lyric Video)", "(Lyrics)", "[Lyrics]", "(Audio)", "(Visualizer)",
        "(Official Visualizer)", "(HD)", "[HD]", "(Video)", "(Official HD Video)", "(Music Video)",
    ] {
        if let Some(pos) = title.to_ascii_lowercase().find(&noise.to_ascii_lowercase()) {
            title.replace_range(pos..pos + noise.len(), "");
        }
    }
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    for separator in [" -- ", " - ", " – ", " — "] {
        if let Some((artist, song)) = title.split_once(separator) {
            return (artist.trim().to_owned(), song.trim().to_owned());
        }
    }
    let channel = channel.trim_end_matches(" - Topic").trim_end_matches("VEVO").to_owned();
    (channel, title)
}

pub fn thumbnail(id: &str) -> String {
    format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")
}

/// A video as a playable track.
fn video_track(id: &str, raw_title: &str, channel: &str, duration: u64) -> Value {
    let (artist, title) = split_title(raw_title, channel);
    json!({
        "key": format!("yt:{id}"),
        "youtubeId": id,
        "title": title,
        "artist": artist,
        "album": "",
        "albumId": "",
        "artistId": "",
        "cover": thumbnail(id),
        "coverBig": thumbnail(id),
        "duration": duration,
        "channel": channel,
        "rawTitle": raw_title,
    })
}

fn from_video_renderer(video: &Value) -> Option<Value> {
    let id = video.get("videoId")?.as_str()?;
    let duration = video.get("lengthText").map(|t| parse_clock(&text(t))).unwrap_or(0);
    let channel = video.get("ownerText").or_else(|| video.get("longBylineText")).map(text).unwrap_or_default();
    Some(video_track(id, &text(video.get("title")?), &channel, duration))
}

/// Text parts of a lockup's metadata rows, e.g. [["GBelair"], ["88K views", "14 years ago"]].
fn lockup_rows(lockup: &Value) -> Vec<Vec<String>> {
    lockup
        .pointer("/metadata/lockupMetadataViewModel/metadata/contentMetadataViewModel/metadataRows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.get("metadataParts")
                        .and_then(Value::as_array)
                        .map(|parts| parts.iter().filter_map(|part| part.get("text").map(text)).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn lockup_badge(lockup: &Value) -> String {
    first(lockup, "thumbnailBadgeViewModel").and_then(|badge| badge.get("text")).and_then(Value::as_str).unwrap_or("").to_owned()
}

fn lockup_image(lockup: &Value) -> String {
    first(lockup, "sources")
        .and_then(Value::as_array)
        .and_then(|sources| sources.first())
        .and_then(|source| source.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

enum Lockup {
    Video(Value),
    Playlist(Value),
}

fn from_lockup(lockup: &Value) -> Option<Lockup> {
    let id = lockup.get("contentId")?.as_str()?;
    let kind = lockup.get("contentType").and_then(Value::as_str).unwrap_or("");
    let title = lockup.pointer("/metadata/lockupMetadataViewModel/title").map(text).unwrap_or_default();
    let rows = lockup_rows(lockup);
    let channel = rows.first().and_then(|row| row.first()).cloned().unwrap_or_default();
    match kind {
        "LOCKUP_CONTENT_TYPE_VIDEO" => Some(Lockup::Video(video_track(id, &title, &channel, parse_clock(&lockup_badge(lockup))))),
        "LOCKUP_CONTENT_TYPE_PLAYLIST" => {
            // "RD…" lists are YouTube's endless auto-generated mixes, not albums.
            if id.starts_with("RD") {
                return None;
            }
            let count: u64 = lockup_badge(lockup).split_whitespace().next().and_then(|n| n.replace(',', "").parse().ok()).unwrap_or(0);
            Some(Lockup::Playlist(json!({
                "id": format!("yt:pl:{id}"),
                "title": title,
                "artist": channel,
                "artistId": "",
                "cover": lockup_image(lockup),
                "coverBig": lockup_image(lockup),
                "year": "",
                "kind": "playlist",
                "trackCount": count,
            })))
        }
        _ => None,
    }
}

/// What youtube.com shows for a search, in its order: videos and playlists.
fn search_page(query: &str) -> Result<Vec<Lockup>, String> {
    let answer = innertube("search", json!({ "query": query }))?;
    let mut found = Vec::new();
    collect(&answer, &["videoRenderer", "lockupViewModel"], &mut found);
    Ok(found
        .into_iter()
        .filter_map(|(kind, item)| match kind {
            "videoRenderer" => from_video_renderer(item).map(Lockup::Video),
            _ => from_lockup(item),
        })
        .collect())
}

/// YouTube videos for a search, in YouTube's order.
pub fn search(query: &str, count: usize) -> Result<Vec<Value>, String> {
    Ok(search_page(query)?
        .into_iter()
        .filter_map(|item| match item {
            Lockup::Video(video) => Some(video),
            Lockup::Playlist(_) => None,
        })
        .take(count)
        .collect())
}

/// Videos at least this long are offered as albums (full soundtracks, live sets).
const ALBUM_VIDEO_SECONDS: u64 = 15 * 60;

/// A real YouTube search, kept in YouTube's order: playlists and full-length
/// videos come back as albums, shorter videos as songs.
pub fn search_albums(query: &str) -> Result<Value, String> {
    let mut albums = Vec::new();
    let mut tracks = Vec::new();
    for item in search_page(query)? {
        match item {
            Lockup::Playlist(playlist) => albums.push(playlist),
            Lockup::Video(video) if video["duration"].as_u64().unwrap_or(0) >= ALBUM_VIDEO_SECONDS => {
                let id = video["youtubeId"].as_str().unwrap_or("");
                albums.push(json!({
                    "id": format!("yt:v:{id}"),
                    "title": video["rawTitle"],
                    "artist": video["channel"],
                    "artistId": "",
                    "cover": thumbnail(id),
                    "coverBig": thumbnail(id),
                    "year": "",
                    "kind": "video",
                    "duration": video["duration"],
                }));
            }
            Lockup::Video(video) => tracks.push(video),
        }
    }
    Ok(json!({ "albums": albums, "tracks": tracks }))
}

/// A YouTube playlist ("yt:pl:ID") or a long video split at its chapters ("yt:v:ID"), as an album.
pub fn album(id: &str) -> Result<Value, String> {
    if let Some(list) = id.strip_prefix("yt:pl:") {
        let (title, owner, mut tracks) = playlist_contents(list)?;
        for (index, track) in tracks.iter_mut().enumerate() {
            track["album"] = json!(title);
            track["albumId"] = json!(id);
            track["trackNumber"] = json!(index + 1);
        }
        let cover = tracks[0]["cover"].clone();
        let total: u64 = tracks.iter().filter_map(|t| t["duration"].as_u64()).sum();
        return Ok(json!({
            "id": id, "title": title, "artist": owner, "artistId": "",
            "cover": cover, "coverBig": cover, "year": "", "kind": "playlist",
            "trackCount": tracks.len(), "duration": total, "label": "YouTube playlist",
            "releaseDate": "", "genres": [], "tracks": tracks, "source": "youtube",
        }));
    }

    let video = id.strip_prefix("yt:v:").ok_or("Unknown YouTube album")?;
    let next = innertube("next", json!({ "videoId": video }))?;
    let details = innertube("player", json!({ "videoId": video })).ok();
    let detail = |field: &str| details.as_ref().and_then(|d| d.pointer(&format!("/videoDetails/{field}"))).and_then(Value::as_str).map(str::to_owned);
    let title = first(&next, "videoPrimaryInfoRenderer")
        .and_then(|info| info.get("title"))
        .map(text)
        .filter(|t| !t.is_empty())
        .or_else(|| detail("title"))
        .unwrap_or_else(|| "Album".to_owned());
    let channel = first(&next, "videoOwnerRenderer")
        .and_then(|owner| owner.get("title"))
        .map(text)
        .filter(|t| !t.is_empty())
        .or_else(|| detail("author"))
        .unwrap_or_default();
    let length: f64 = detail("lengthSeconds").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let date = first(&next, "videoPrimaryInfoRenderer").and_then(|info| info.get("dateText")).map(text).unwrap_or_default();
    let year: String = date.chars().skip(date.chars().count().saturating_sub(4)).collect();
    let year = if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) { year } else { String::new() };

    let mut markers = Vec::new();
    collect(&next, &["chapterRenderer"], &mut markers);
    let mut chapters: Vec<(f64, String)> = markers
        .iter()
        .filter_map(|(_, chapter)| {
            let start = chapter.get("timeRangeStartMillis")?.as_f64()? / 1000.0;
            Some((start, chapter.get("title").map(text).unwrap_or_default()))
        })
        .collect();
    chapters.sort_by(|a, b| a.0.total_cmp(&b.0));
    chapters.dedup_by(|a, b| a.0 == b.0);
    if chapters.is_empty() {
        chapters.push((0.0, title.clone()));
    }

    let cover = thumbnail(video);
    let count = chapters.len();
    let tracks: Vec<Value> = chapters
        .iter()
        .enumerate()
        .map(|(index, (start, name))| {
            let end = chapters.get(index + 1).map(|next| next.0).unwrap_or(length);
            json!({
                "key": format!("yt:{video}@{}", *start as u64),
                "youtubeId": video,
                "title": strip_track_number(name),
                "artist": channel,
                "artistId": "",
                "album": title,
                "albumId": id,
                "cover": cover,
                "coverBig": cover,
                "duration": (end - start).max(0.0) as u64,
                "start": start,
                "end": if index + 1 == count { 0.0 } else { end },
                "trackNumber": index + 1,
            })
        })
        .collect();
    Ok(json!({
        "id": id, "title": title, "artist": channel, "artistId": "",
        "cover": cover, "coverBig": cover, "year": year, "kind": "video",
        "trackCount": tracks.len(), "duration": length as u64,
        "label": if count > 1 { "One YouTube video, split at its chapters" } else { "One YouTube video" },
        "releaseDate": date, "genres": [], "tracks": tracks, "source": "youtube",
    }))
}

/// A playlist's title, owner and videos (the first page, up to about 100).
fn playlist_contents(list: &str) -> Result<(String, String, Vec<Value>), String> {
    let answer = innertube("browse", json!({ "browseId": format!("VL{list}") }))?;
    let title = first(&answer, "microformatDataRenderer")
        .and_then(|micro| micro.get("title"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| first(&answer, "pageHeaderViewModel").and_then(|header| header.get("title")).map(text))
        .unwrap_or_else(|| "Playlist".to_owned());
    let owner = first(&answer, "pageHeaderViewModel")
        .and_then(|header| first(header, "avatarStackViewModel"))
        .and_then(|stack| stack.get("text"))
        .map(text)
        .map(|by| by.trim_start_matches("by ").to_owned())
        .unwrap_or_default();
    let mut found = Vec::new();
    collect(&answer, &["lockupViewModel", "playlistVideoRenderer"], &mut found);
    let tracks: Vec<Value> = found
        .into_iter()
        .filter_map(|(kind, item)| {
            if kind == "lockupViewModel" {
                return match from_lockup(item) {
                    Some(Lockup::Video(video)) => Some(video),
                    _ => None,
                };
            }
            let id = item.get("videoId")?.as_str()?;
            let seconds = item.get("lengthSeconds").and_then(Value::as_str).and_then(|s| s.parse().ok()).unwrap_or(0);
            let channel = item.get("shortBylineText").map(text).unwrap_or_default();
            Some(video_track(id, &text(item.get("title")?), &channel, seconds))
        })
        .collect();
    if tracks.is_empty() {
        return Err("This playlist has no playable videos.".to_owned());
    }
    Ok((title, owner, tracks))
}

/// Imports a playlist from a YouTube link.
pub fn playlist(url: &str) -> Result<Value, String> {
    let list = url
        .split(['?', '&'])
        .find_map(|part| part.strip_prefix("list="))
        .ok_or("Paste a YouTube playlist link (it contains \"list=\").")?;
    let (title, _, tracks) = playlist_contents(list)?;
    Ok(json!({ "title": title, "tracks": tracks }))
}

/// "12. Town" or "03 - Town" -> "Town".
fn strip_track_number(name: &str) -> String {
    let rest = name.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() < name.len() {
        let rest = rest.trim_start_matches(['.', ')', '-', ':', ' ']);
        if !rest.is_empty() {
            return rest.to_owned();
        }
    }
    name.to_owned()
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Scores a search result against the wanted track; higher is better.
fn score(candidate: &Value, artist: &str, title: &str, duration: u64) -> i64 {
    let raw = candidate["rawTitle"].as_str().unwrap_or("").to_lowercase();
    let channel = candidate["channel"].as_str().unwrap_or("").to_lowercase();
    let mut points = 0i64;
    let wanted_title = words(title);
    let hits = wanted_title.iter().filter(|word| raw.contains(word.as_str())).count() as i64;
    points += hits * 10 - (wanted_title.len() as i64 - hits) * 15;
    let artist_lower = artist.to_lowercase();
    if !artist_lower.is_empty() && (channel.contains(&artist_lower) || raw.contains(&artist_lower)) {
        points += 25;
    }
    if channel.ends_with(" - topic") {
        points += 20;
    }
    let wanted = format!("{} {}", artist, title).to_lowercase();
    for unwanted in ["live", "cover", "karaoke", "remix", "sped up", "slowed", "8d", "reverb", "instrumental", "nightcore", "reaction", "tutorial"] {
        if raw.contains(unwanted) && !wanted.contains(unwanted) {
            points -= 40;
        }
    }
    let found = candidate["duration"].as_u64().unwrap_or(0);
    if duration > 0 && found > 0 {
        let diff = (found as i64 - duration as i64).abs();
        points -= diff.min(600) / 3;
    }
    points
}

/// The YouTube videos that best match a track, best first. Several come back
/// so the player can move on if one can't be played inside the app.
pub fn find_videos(track: &Value, known_video: Option<String>) -> Result<Vec<String>, String> {
    if let Some(video) = track["youtubeId"].as_str().filter(|id| !id.is_empty()).map(str::to_owned) {
        return Ok(vec![video]);
    }
    let artist = track["artist"].as_str().unwrap_or("");
    let title = track["title"].as_str().unwrap_or("");
    let duration = track["duration"].as_u64().unwrap_or(0);
    let mut candidates = search(&format!("{artist} - {title}"), 10)?;
    candidates.sort_by_key(|candidate| -score(candidate, artist, title, duration));
    let mut videos: Vec<String> = candidates.iter().filter_map(|c| c["youtubeId"].as_str().map(str::to_owned)).take(4).collect();
    // The video that worked last time goes first.
    if let Some(known) = known_video {
        videos.retain(|video| *video != known);
        videos.insert(0, known);
    }
    if videos.is_empty() {
        return Err(format!("Couldn't find \"{title}\" on YouTube."));
    }
    Ok(videos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_titles() {
        assert_eq!(split_title("Daft Punk - Around The World (Official Music Video)", "Daft Punk"), ("Daft Punk".into(), "Around The World".into()));
        assert_eq!(split_title("Harder Better", "Daft Punk - Topic"), ("Daft Punk".into(), "Harder Better".into()));
    }

    #[test]
    fn strips_chapter_numbers() {
        assert_eq!(strip_track_number("1. Overture - Roto's Theme"), "Overture - Roto's Theme");
        assert_eq!(strip_track_number("03 - Town"), "Town");
        assert_eq!(strip_track_number("1999"), "1999");
    }

    #[test]
    fn reads_clock_times() {
        assert_eq!(parse_clock("3:01"), 181);
        assert_eq!(parse_clock("1:30:03"), 5403);
        assert_eq!(parse_clock("26 videos"), 0);
    }

    #[test]
    fn prefers_matching_studio_versions() {
        let studio = json!({"rawTitle": "Daft Punk - Around the World", "channel": "Daft Punk - Topic", "duration": 430});
        let live = json!({"rawTitle": "Daft Punk - Around the World (Live 2007)", "channel": "fan", "duration": 600});
        assert!(score(&studio, "Daft Punk", "Around the World", 429) > score(&live, "Daft Punk", "Around the World", 429));
    }
}
