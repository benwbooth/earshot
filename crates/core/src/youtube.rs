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

/// A video's AAC audio (itag 140, an MP4), for measuring the waveform: its
/// address and size in bytes. Asked as YouTube's VR app (version 1.62)
/// asks, which is given plain addresses (later versions get only the first
/// megabyte); they're read in pieces of [`AUDIO_PIECE`] bytes
/// (`&range=start-end`), as YouTube's own players do.
///
/// YouTube asks the VR app to "sign in to confirm you're not a bot" for many
/// videos; the iPhone app is asked then, whose address gives only the first
/// megabyte (about a minute) without a sign-in token, so only that much can
/// be measured.
pub fn audio(video: &str) -> Result<(String, u64), String> {
    let clients = [
        json!({
            "clientName": "ANDROID_VR", "clientVersion": "1.62.27", "hl": "en", "gl": "US",
            "androidSdkVersion": 32, "deviceMake": "Oculus", "deviceModel": "Quest 3", "osName": "Android", "osVersion": "12L",
        }),
        json!({
            "clientName": "IOS", "clientVersion": "20.10.4", "hl": "en", "gl": "US",
            "deviceMake": "Apple", "deviceModel": "iPhone16,2", "osName": "iPhone", "osVersion": "18.3.2.22D82",
        }),
    ];
    let mut refused = String::new();
    for client in clients {
        let body = json!({ "videoId": video, "contentCheckOk": true, "racyCheckOk": true, "context": { "client": client } });
        let answer = post_json("https://www.youtube.com/youtubei/v1/player?prettyPrint=false", &body)?;
        if answer.pointer("/playabilityStatus/status").and_then(Value::as_str) == Some("OK") {
            return audio_format(&answer);
        }
        refused = answer.pointer("/playabilityStatus/reason").and_then(Value::as_str).unwrap_or("it isn't available").to_owned();
    }
    Err(format!("YouTube won't share this video's audio with Earshot's server ({refused})"))
}

/// [`audio`], but only if YouTube will hand over all of it: without a
/// sign-in it sometimes gives just the first megabyte (about a minute), and
/// the very end, so a few bytes from past the middle are asked for.
pub fn whole_audio(video: &str) -> Result<(String, u64), String> {
    let (url, length) = audio(video)?;
    let middle = (length / 2).max(AUDIO_PIECE + 1).min(length.saturating_sub(1000));
    match ureq::get(&format!("{url}&range={middle}-{}", middle + 999)).call() {
        Ok(response) if response.status().is_success() => Ok((url, length)),
        _ => Err("YouTube shares only the start of this video's audio without a sign-in".to_owned()),
    }
}

fn audio_format(answer: &Value) -> Result<(String, u64), String> {
    let formats = answer.pointer("/streamingData/adaptiveFormats").and_then(Value::as_array).cloned().unwrap_or_default();
    let aac = |f: &&Value| f["mimeType"].as_str().is_some_and(|m| m.starts_with("audio/mp4") && m.contains("mp4a.40.2"));
    let chosen = formats.iter().find(|f| f["itag"] == 140).or_else(|| formats.iter().find(aac)).ok_or("YouTube didn't offer this video's audio as AAC.")?;
    let url = chosen["url"].as_str().ok_or("YouTube's address for this video's audio is scrambled.")?;
    let length = chosen["contentLength"].as_str().and_then(|l| l.parse().ok()).ok_or("YouTube didn't say how long this video's audio is.")?;
    Ok((url.to_owned(), length))
}

/// How much of a video's audio is asked for at once (YouTube refuses a
/// whole file in one request).
pub const AUDIO_PIECE: u64 = 1_000_000;

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
    let mut track = video_track(id, &text(video.get("title")?), &channel, duration);
    // Search results list a video's chapters (the strip of moments under it).
    let mut chapters = Vec::new();
    if let Some(expandable) = video.get("expandableMetadata") {
        collect(expandable, &["macroMarkersListItemRenderer"], &mut chapters);
    }
    track["chapters"] = json!(chapters.len());
    Some(track)
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

/// One page of what youtube.com shows for a search, in its order (videos and
/// playlists), plus the token for the next page ("" after the last one). An
/// empty `continuation` asks for the first page.
fn search_page(query: &str, continuation: &str) -> Result<(Vec<Lockup>, String), String> {
    search_filtered(query, continuation, None)
}

/// The same with one of YouTube's search filters (`params`, as youtube.com sends them).
fn search_filtered(query: &str, continuation: &str, params: Option<&str>) -> Result<(Vec<Lockup>, String), String> {
    let answer = if !continuation.is_empty() {
        innertube("search", json!({ "continuation": continuation }))?
    } else if let Some(params) = params {
        innertube("search", json!({ "query": query, "params": params }))?
    } else {
        innertube("search", json!({ "query": query }))?
    };
    let mut found = Vec::new();
    collect(&answer, &["videoRenderer", "lockupViewModel", "continuationItemRenderer"], &mut found);
    let mut next = String::new();
    let items = found
        .into_iter()
        .filter_map(|(kind, item)| match kind {
            "videoRenderer" => from_video_renderer(item).map(Lockup::Video),
            "continuationItemRenderer" => {
                if let Some(token) = item.pointer("/continuationEndpoint/continuationCommand/token").and_then(Value::as_str) {
                    next = token.to_owned();
                }
                None
            }
            _ => from_lockup(item),
        })
        .collect();
    Ok((items, next))
}

/// YouTube's own "Mix" for a video: songs like it, the radio YouTube Music
/// plays after it. Leaves out the video itself, live streams and long mixes.
pub fn mix(video: &str) -> Result<Vec<Value>, String> {
    let answer = innertube("next", json!({ "videoId": video, "playlistId": format!("RD{video}") }))?;
    let mut found = Vec::new();
    collect(&answer, &["playlistPanelVideoRenderer"], &mut found);
    Ok(found
        .iter()
        .filter_map(|(_, item)| {
            let id = item.get("videoId")?.as_str()?;
            let duration = item.get("lengthText").map(|t| parse_clock(&text(t))).unwrap_or(0);
            if id == video || duration == 0 || duration > ALBUM_VIDEO_SECONDS {
                return None;
            }
            let channel = item
                .pointer("/longBylineText/runs/0/text")
                .or_else(|| item.pointer("/shortBylineText/runs/0/text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            Some(video_track(id, &text(item.get("title")?), channel, duration))
        })
        .collect())
}

/// YouTube videos for a search, in YouTube's order.
pub fn search(query: &str, count: usize) -> Result<Vec<Value>, String> {
    Ok(search_page(query, "")?
        .0
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

/// Shorter videos still count as albums when they're split into chapters.
const ALBUM_CHAPTERS: u64 = 3;
const ALBUM_CHAPTERED_SECONDS: u64 = 5 * 60;

fn is_album_video(video: &Value) -> bool {
    let duration = video["duration"].as_u64().unwrap_or(0);
    let chapters = video["chapters"].as_u64().unwrap_or(0);
    duration >= ALBUM_VIDEO_SECONDS || (chapters >= ALBUM_CHAPTERS && duration >= ALBUM_CHAPTERED_SECONDS)
}

/// One page of a real YouTube search, kept in YouTube's order: playlists and
/// full-length or chaptered videos come back as albums, other videos as songs.
/// `next` is the token for the following page (pass it back as `continuation`).
pub fn search_albums(query: &str, continuation: &str) -> Result<Value, String> {
    let mut albums = Vec::new();
    let mut tracks = Vec::new();
    let (items, next) = search_page(query, continuation)?;
    for item in items {
        match item {
            Lockup::Playlist(playlist) => albums.push(playlist),
            Lockup::Video(video) if is_album_video(&video) => albums.push(video_album(&video)),
            Lockup::Video(video) => tracks.push(video),
        }
    }
    Ok(json!({ "albums": albums, "tracks": tracks, "next": next }))
}

/// A video as an album card (it opens as one, split at its chapters).
fn video_album(video: &Value) -> Value {
    let id = video["youtubeId"].as_str().unwrap_or("");
    json!({
        "id": format!("yt:v:{id}"),
        "title": video["rawTitle"],
        "artist": video["channel"],
        "artistId": "",
        "cover": thumbnail(id),
        "coverBig": thumbnail(id),
        "year": "",
        "kind": "video",
        "duration": video["duration"],
        "trackCount": video["chapters"],
    })
}

/// What a channel has put up (playlists and videos, as album cards): only
/// this month's when `recent` (YouTube's "Upload date: This month" filter).
pub fn uploads(channel: &str, recent: bool) -> Vec<Value> {
    let Ok((items, _)) = search_filtered(channel, "", recent.then_some("EgIIBA==")) else { return Vec::new() };
    let wanted = channel.trim().to_lowercase();
    let theirs = |name: &Value| name.as_str().is_some_and(|name| name.trim().to_lowercase() == wanted);
    items
        .into_iter()
        .filter_map(|item| match item {
            Lockup::Playlist(playlist) if theirs(&playlist["artist"]) => Some(playlist),
            Lockup::Video(video) if theirs(&video["channel"]) => Some(video_album(&video)),
            _ => None,
        })
        .collect()
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
    // No chapters from YouTube: many soundtrack uploads list their songs with
    // times in the description instead ("0:02:07 - Abandoned Castle").
    if chapters.len() < 2 {
        let listed = description_chapters(&detail("shortDescription").unwrap_or_default(), length);
        if listed.len() >= 2 {
            chapters = listed;
        }
    }
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

/// "1:02:03" or "02:07" (also in brackets) as seconds, if the word is a time.
fn timestamp(word: &str) -> Option<f64> {
    let word = word.trim_matches(|c: char| "()[]{}<>.,;".contains(c));
    let parts: Vec<&str> = word.split(':').collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || p.len() > 2 || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    if parts[1..].iter().any(|p| p.len() != 2) {
        return None;
    }
    Some(parts.iter().fold(0.0, |total, part| total * 60.0 + part.parse::<f64>().unwrap_or(0.0)))
}

/// Songs listed with their start times in a video's description, one per
/// line, the time before or after the name. Needs at least three, in order,
/// all within the video.
fn description_chapters(description: &str, length: f64) -> Vec<(f64, String)> {
    let separators: &[char] = &['-', '–', '—', '|', ':', '.', '•', '~', '·', ' ', '\t'];
    let mut found: Vec<(f64, String)> = Vec::new();
    for line in description.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(at) = words.iter().position(|word| timestamp(word).is_some()) else { continue };
        // Only a time at the start or the end of the line marks a song.
        if at != 0 && at != words.len() - 1 && !(at == 1 && words[0].trim_end_matches(['.', ')']).chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        let start = timestamp(words[at]).unwrap_or(0.0);
        let mut rest: Vec<&str> = words.clone();
        rest.remove(at);
        let name = rest.join(" ").trim_matches(separators).to_owned();
        if name.is_empty() || (length > 0.0 && start >= length) {
            continue;
        }
        found.push((start, name));
    }
    let in_order = found.windows(2).all(|pair| pair[0].0 < pair[1].0);
    if found.len() < 3 || !in_order {
        return Vec::new();
    }
    found
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
    // A different song with some of the same words ("Autumn's End (Extended
    // Version)" for "Autumn Leaves (Extended Version)") must not stand in for
    // it: keep uploads naming the whole title, before any "(…)" or "[…]".
    // Nor a song of the same name by someone else: most of the first artist's
    // name must be in the title or channel ("Lofi Fruits" for "Lofi Fruits Music").
    let core = words(title.split(['(', '[']).next().unwrap_or(title));
    let by = words(artist.split([',', '&']).next().unwrap_or(artist));
    let named = |candidate: &Value| {
        let raw = candidate["rawTitle"].as_str().unwrap_or("").to_lowercase();
        let place = format!("{raw} {}", candidate["channel"].as_str().unwrap_or("").to_lowercase());
        let artist_words = by.iter().filter(|word| place.contains(word.as_str())).count();
        core.iter().all(|word| raw.contains(word.as_str())) && artist_words * 2 >= by.len()
    };
    if candidates.iter().any(named) {
        candidates.retain(named);
    }
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

/// Calls YouTube Music's own API (music.youtube.com): it has the charts, new
/// releases and moods that youtube.com doesn't show.
fn music_browse(browse_id: &str, params: Option<&str>) -> Result<Value, String> {
    let mut body = json!({
        "browseId": browse_id,
        "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": "1.20251101.01.00", "hl": "en", "gl": "US" } },
    });
    if let Some(params) = params {
        body["params"] = json!(params);
    }
    let answer = post_json("https://music.youtube.com/youtubei/v1/browse?prettyPrint=false", &body)?;
    if let Some(error) = answer.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("request failed");
        return Err(format!("YouTube Music: {message}"));
    }
    Ok(answer)
}

/// A YouTube Music page's shelves, as (title, items) in page order.
fn music_shelves(page: &Value) -> Vec<(String, Vec<Value>)> {
    let mut found = Vec::new();
    collect(page, &["musicCarouselShelfRenderer", "gridRenderer"], &mut found);
    found
        .into_iter()
        .map(|(_, shelf)| {
            let title = shelf.get("header").and_then(|header| first(header, "title")).map(text).unwrap_or_default();
            let items = shelf.get("contents").or_else(|| shelf.get("items")).and_then(Value::as_array).cloned().unwrap_or_default();
            (title, items)
        })
        .collect()
}

/// The items of the first shelf with this title.
fn music_shelf(page: &Value, title: &str) -> Vec<Value> {
    music_shelves(page).into_iter().find(|(name, _)| name == title).map(|(_, items)| items).unwrap_or_default()
}

/// What a YouTube Music card or row stands for.
enum MusicItem {
    Album(Value),
    Song(Value),
}

/// "Album • Artist", "Artist • 2026" or "Chart • YouTube Charts" as (kind, who, year).
fn music_byline(line: &str) -> (String, String, String) {
    let (mut kind, mut year, mut who) = (String::new(), String::new(), Vec::new());
    for part in line.split(" • ").map(str::trim).filter(|part| !part.is_empty()) {
        match part {
            "Album" | "Single" | "EP" | "Playlist" | "Chart" | "Song" | "Video" => kind = part.to_lowercase(),
            _ if part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()) => year = part.to_owned(),
            _ if part.ends_with(" views") || part.ends_with(" plays") || part.ends_with(" sec") || part.ends_with(" songs") => {}
            _ => who.push(part),
        }
    }
    (kind, who.join(" • "), year)
}

/// One card (musicTwoRowItemRenderer) or row (musicResponsiveListItemRenderer)
/// from YouTube Music: albums open as their YouTube playlist ("OLAK5uy_…"),
/// which the album page already reads.
fn music_item(item: &Value) -> Option<MusicItem> {
    let (renderer, title, byline) = if let Some(card) = item.get("musicTwoRowItemRenderer") {
        (card, card.get("title").map(text)?, card.get("subtitle").map(text).unwrap_or_default())
    } else {
        let row = item.get("musicResponsiveListItemRenderer")?;
        let columns = row.get("flexColumns").and_then(Value::as_array)?;
        let column = |at: usize| columns.get(at).and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text")).map(text).unwrap_or_default();
        (row, column(0), column(1))
    };
    let (kind, who, year) = music_byline(&byline);
    let cover = renderer
        .get("thumbnailRenderer")
        .or_else(|| renderer.get("thumbnail"))
        .and_then(|thumbnail| first(thumbnail, "thumbnails"))
        .and_then(Value::as_array)
        .and_then(|sizes| sizes.last())
        .and_then(|size| size.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    // Where the card itself leads (not its artist's link).
    let page_type = renderer
        .get("navigationEndpoint")
        .or_else(|| renderer.pointer("/flexColumns/0"))
        .and_then(|target| first(target, "pageType"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let browse_id = renderer.pointer("/navigationEndpoint/browseEndpoint/browseId").and_then(Value::as_str).unwrap_or("");
    let mut ids = Vec::new();
    collect(renderer, &["playlistId", "videoId"], &mut ids);
    let video = ids.iter().find(|(key, _)| *key == "videoId").and_then(|(_, id)| id.as_str());
    let album = |id: String, kind: &str| {
        MusicItem::Album(json!({
            "id": format!("yt:pl:{id}"), "title": title, "artist": who, "artistId": "",
            "cover": cover, "coverBig": cover, "year": year, "kind": kind,
        }))
    };
    match page_type {
        "MUSIC_PAGE_TYPE_ALBUM" => {
            let list = ids.iter().filter_map(|(_, id)| id.as_str()).find(|id| id.starts_with("OLAK5uy_"))?;
            Some(album(list.to_owned(), if kind == "single" || kind == "ep" { &kind } else { "album" }))
        }
        "MUSIC_PAGE_TYPE_PLAYLIST" => Some(album(browse_id.strip_prefix("VL").unwrap_or(browse_id).to_owned(), "playlist")),
        _ => video.map(|id| MusicItem::Song(video_track(id, &title, &who, 0))),
    }
}

fn music_albums(items: &[Value]) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .filter_map(|item| match music_item(item) {
            Some(MusicItem::Album(album)) => seen.insert(album["id"].as_str().unwrap_or("").to_owned()).then_some(album),
            _ => None,
        })
        .collect()
}

/// YouTube Music's trending songs, from the chart playlist behind them (which
/// has the songs' lengths), or as the shelf lists them.
fn trending(explore: &Value) -> Vec<Value> {
    let items = music_shelf(explore, "Trending");
    let mut lists = Vec::new();
    for item in &items {
        collect(item, &["playlistId"], &mut lists);
    }
    let chart = lists.iter().filter_map(|(_, id)| id.as_str()).find(|id| !id.starts_with("RD"));
    if let Some(Ok((_, _, tracks))) = chart.map(playlist_contents) {
        return tracks;
    }
    items
        .iter()
        .filter_map(|item| match music_item(item) {
            Some(MusicItem::Song(song)) => Some(song),
            _ => None,
        })
        .collect()
}

/// The moods YouTube Music's front page offers, in this order when there.
const MOODS: [&str; 4] = ["Feel good", "Chill", "Focus", "Party"];

/// Playlists YouTube Music picks for each of a few moods: [(mood, playlists)].
fn moods() -> Vec<(String, Vec<Value>)> {
    let Ok(page) = music_browse("FEmusic_moods_and_genres", None) else { return Vec::new() };
    let mut buttons = Vec::new();
    collect(&page, &["musicNavigationButtonRenderer"], &mut buttons);
    let wanted: Vec<(String, String)> = MOODS
        .iter()
        .filter_map(|mood| {
            buttons.iter().find_map(|(_, button)| {
                let name = button.get("buttonText").map(text)?;
                let params = button.pointer("/clickCommand/browseEndpoint/params")?.as_str()?;
                (name == *mood).then(|| (name, params.to_owned()))
            })
        })
        .collect();
    std::thread::scope(|scope| {
        let jobs: Vec<_> = wanted
            .iter()
            .map(|(name, params)| {
                scope.spawn(move || {
                    let page = music_browse("FEmusic_moods_and_genres_category", Some(params)).ok()?;
                    let items: Vec<Value> = music_shelves(&page).into_iter().flat_map(|(_, items)| items).collect();
                    Some((name.clone(), music_albums(&items).into_iter().take(24).collect::<Vec<_>>()))
                })
            })
            .collect();
        jobs.into_iter().filter_map(|job| job.join().ok().flatten()).collect()
    })
}

/// YouTube Music's front page: trending songs, new albums and singles, the
/// charts, the top albums and playlists for a few moods.
pub fn featured() -> Result<Vec<Value>, String> {
    let (explore, releases, charts, moods) = std::thread::scope(|scope| {
        let explore = scope.spawn(|| music_browse("FEmusic_explore", None).ok());
        let releases = scope.spawn(|| music_browse("FEmusic_new_releases_albums", None).ok());
        let charts = scope.spawn(|| music_browse("FEmusic_charts", None).ok());
        let moods = scope.spawn(moods);
        let explore = explore.join().ok().flatten().map(|page| (trending(&page), page));
        (explore, releases.join().ok().flatten(), charts.join().ok().flatten(), moods.join().unwrap_or_default())
    });
    let mut sections = Vec::new();
    let (songs, explore) = explore.unzip();
    sections.extend(crate::featured::section("trending", "Trending", "The songs taking off on YouTube Music right now.", Vec::new(), songs.unwrap_or_default()));
    let mut new_albums = releases.map(|page| music_albums(&music_shelves(&page).into_iter().flat_map(|(_, items)| items).collect::<Vec<_>>())).unwrap_or_default();
    if new_albums.is_empty() {
        new_albums = explore.as_ref().map(|page| music_albums(&music_shelf(page, "New albums & singles"))).unwrap_or_default();
    }
    new_albums.truncate(48);
    sections.extend(crate::featured::section("new", "New albums & singles", "Just out on YouTube Music.", new_albums, Vec::new()));
    if let Some(charts) = &charts {
        let mut lists = music_albums(&music_shelf(charts, "Video charts"));
        lists.extend(music_albums(&music_shelf(charts, "Genres")));
        sections.extend(crate::featured::section("charts", "Charts", "YouTube's weekly charts, overall and by genre.", lists, Vec::new()));
        let top = music_albums(&music_shelf(charts, "Top albums"));
        sections.extend(crate::featured::section("top-albums", "Top albums", "The most played albums on YouTube this week.", top, Vec::new()));
    }
    for (mood, playlists) in moods {
        let id = format!("mood-{}", mood.to_lowercase().replace(' ', "-"));
        sections.extend(crate::featured::section(&id, &mood, "Playlists for the mood, picked by YouTube Music.", playlists, Vec::new()));
    }
    Ok(sections)
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
    fn reads_song_lists_in_descriptions() {
        let description = "Music from the games...\nFor more: https://youtu.be/x?t=1:00\n\n\
            0:00:00 - A Fleeting Respite\n0:02:07 - Abandoned Castle\n0:07:43 - After Confession\n\
            1. Bloody Tears (12:30)\n[1:00:59] Emerald Mist";
        let songs = description_chapters(description, 3.0 * 3600.0);
        assert_eq!(songs.len(), 5);
        assert_eq!(songs[1], (127.0, "Abandoned Castle".to_owned()));
        assert_eq!(songs[3], (750.0, "1. Bloody Tears".to_owned()));
        assert_eq!(songs[4], (3659.0, "Emerald Mist".to_owned()));
        // Times inside sentences, or too few, aren't a song list.
        assert!(description_chapters("We met at 10:30 and left at 11:45.\n0:00 Intro", 600.0).is_empty());
    }

    /// Talks to YouTube: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_mix() {
        let songs = mix("jx-kcjc_v00").unwrap();
        assert!(songs.len() > 10, "{} songs", songs.len());
        assert!(songs.iter().all(|s| s["youtubeId"] != "jx-kcjc_v00"));
    }

    /// Talks to YouTube Music: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_featured() {
        let sections = featured().unwrap();
        let ids: Vec<&str> = sections.iter().filter_map(|s| s["id"].as_str()).collect();
        for wanted in ["trending", "new", "charts", "top-albums", "mood-chill"] {
            assert!(ids.contains(&wanted), "{wanted} missing from {ids:?}");
        }
        let trending = sections[0]["tracks"].as_array().unwrap();
        assert!(trending.len() >= 10 && trending.iter().all(|t| t["youtubeId"].is_string()), "{} trending songs", trending.len());
        let new = sections.iter().find(|s| s["id"] == "new").unwrap()["albums"].as_array().unwrap();
        assert!(new.len() >= 20 && new.iter().all(|a| a["id"].as_str().unwrap().starts_with("yt:pl:OLAK5uy_") && a["artist"] != ""));
        // Albums, charts and mood playlists all open as YouTube playlists.
        let album = album(new[0]["id"].as_str().unwrap()).unwrap();
        assert!(!album["tracks"].as_array().unwrap().is_empty());
        let chart = sections.iter().find(|s| s["id"] == "charts").unwrap()["albums"][0]["id"].as_str().unwrap().to_owned();
        assert!(super::album(&chart).unwrap()["tracks"].as_array().unwrap().len() >= 20);
    }

    /// Talks to YouTube: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_description_album() {
        let album = album("yt:v:pmUpdxXlqiY").unwrap();
        let tracks = album["tracks"].as_array().unwrap();
        assert!(tracks.len() > 40, "{} songs", tracks.len());
        assert_eq!(tracks[1]["title"], "Abandoned Castle");
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
