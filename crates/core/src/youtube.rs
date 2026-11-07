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
    let badge_styles = |key: &str| {
        let mut found = Vec::new();
        if let Some(badges) = video.get(key) {
            collect(badges, &["style", "label"], &mut found);
        }
        found.into_iter().filter_map(|(_, value)| value.as_str().map(str::to_owned)).collect::<Vec<_>>()
    };
    let snippet = video
        .get("detailedMetadataSnippets")
        .and_then(Value::as_array)
        .map(|snippets| snippets.iter().filter_map(|s| s.get("snippetText").map(text)).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let signals = Signals {
        artist_channel: badge_styles("ownerBadges").iter().any(|style| style == "BADGE_STYLE_TYPE_VERIFIED_ARTIST"),
        art_track: snippet.contains("Provided to YouTube by") || snippet.contains("Auto-generated by YouTube"),
        music_badge: false,
        live: video.get("lengthText").is_none() || badge_styles("badges").iter().any(|label| label == "LIVE"),
    };
    track[MUSIC] = json!(signals.score_track(&track));
    track[STRONG] = json!(signals.strong(&track));
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
        "LOCKUP_CONTENT_TYPE_VIDEO" => {
            let badge = lockup_badge(lockup);
            let mut track = video_track(id, &title, &channel, parse_clock(&badge));
            // YouTube puts a music note on the thumbnail of videos it knows are music.
            let mut images = Vec::new();
            collect(lockup.get("contentImage").unwrap_or(&Value::Null), &["imageName"], &mut images);
            let signals = Signals {
                music_badge: images.iter().any(|(_, name)| name.as_str() == Some("MUSIC")),
                live: badge.eq_ignore_ascii_case("live"),
                ..Signals::default()
            };
            track[MUSIC] = json!(signals.score_track(&track));
            track[STRONG] = json!(signals.strong(&track));
            Some(Lockup::Video(track))
        }
        "LOCKUP_CONTENT_TYPE_PLAYLIST" => {
            // "RD…" lists are YouTube's endless auto-generated mixes, not albums.
            if id.starts_with("RD") {
                return None;
            }
            let count: u64 = lockup_badge(lockup).split_whitespace().next().and_then(|n| n.replace(',', "").parse().ok()).unwrap_or(0);
            let mut playlist = json!({
                "id": format!("yt:pl:{id}"),
                "title": title,
                "artist": channel,
                "artistId": "",
                "cover": lockup_image(lockup),
                "coverBig": lockup_image(lockup),
                "year": "",
                "kind": "playlist",
                "trackCount": count,
            });
            playlist[MUSIC] = json!(playlist_score(&title));
            playlist[STRONG] = json!(playlist_score(&title) >= MUSIC_TIER);
            Some(Lockup::Playlist(playlist))
        }
        _ => None,
    }
}

/// One page of what youtube.com shows for a search, in its order (videos and
/// playlists), plus the token for the next page ("" after the last one), and,
/// for a first page, what YouTube Music's search found. With one of YouTube's
/// search filters (`params`, as youtube.com sends them) if given.
fn search_filtered(query: &str, continuation: &str, params: Option<&str>) -> Result<(Vec<Lockup>, String, MusicFound), String> {
    // YouTube Music's search for the same words, alongside (first page of a
    // plain search only): any video it lists is music.
    let music = (continuation.is_empty() && params.is_none()).then(|| music_search_ids(query));
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
    let mut items: Vec<Lockup> = items;
    let music = music.and_then(|wait| wait.recv_timeout(std::time::Duration::from_millis(2500)).ok()).unwrap_or_default();
    for item in &mut items {
        if let Lockup::Video(video) = item
            && video["youtubeId"].as_str().is_some_and(|id| music.ids.contains(id))
        {
            video[MUSIC] = json!(video[MUSIC].as_i64().unwrap_or(0) + ON_YOUTUBE_MUSIC);
            video[STRONG] = json!(true);
        }
    }
    penalise_unwanted(&mut items, query);
    Ok((items, next, music))
}

/// What YouTube Music's search found: the videos it lists (to know them as
/// music in YouTube's results), and its songs, albums and playlists themselves.
#[derive(Default)]
struct MusicFound {
    /// YouTube Music answered (so a video it doesn't list is likely not music).
    answered: bool,
    ids: std::collections::HashSet<String>,
    songs: Vec<Value>,
    albums: Vec<Value>,
}

/// Puts YouTube Music's songs, official videos, albums and playlists first,
/// in its own order, and leaves out YouTube's copies of them; YouTube's other
/// results follow (ordered by [`music_first`]).
fn add_music(items: &mut Vec<Lockup>, music: MusicFound) {
    let mut seen = std::collections::HashSet::new();
    let mut added: Vec<Lockup> = Vec::new();
    for mut song in music.songs {
        if seen.insert(song["youtubeId"].as_str().unwrap_or("").to_owned()) {
            song[MUSIC] = json!(ON_YOUTUBE_MUSIC);
            song[STRONG] = json!(true);
            added.push(Lockup::Video(song));
        }
    }
    for mut album in music.albums {
        if seen.insert(album["id"].as_str().unwrap_or("").to_owned()) {
            album[MUSIC] = json!(ON_YOUTUBE_MUSIC);
            album[STRONG] = json!(true);
            added.push(Lockup::Playlist(album));
        }
    }
    items.retain(|item| match item {
        Lockup::Video(value) => !seen.contains(value["youtubeId"].as_str().unwrap_or("")),
        Lockup::Playlist(value) => !seen.contains(value["id"].as_str().unwrap_or("")),
    });
    items.splice(0..0, added);
}

/// The songs and music videos YouTube Music's own search lists for these
/// words (not its podcast episodes), asked on a thread of its own so the
/// YouTube search doesn't wait for it.
fn music_search_ids(query: &str) -> std::sync::mpsc::Receiver<MusicFound> {
    let (send, receive) = std::sync::mpsc::channel();
    let query = query.to_owned();
    std::thread::spawn(move || {
        // Everything (to know which videos are music, and its albums and
        // playlists) and just songs (whose rows name the artist, album and length).
        let (all, songs) = std::thread::scope(|scope| {
            let songs = scope.spawn(|| music_search(&query, Some(SONGS_FILTER)));
            (music_search(&query, None), songs.join().ok().flatten())
        });
        let mut found = MusicFound { answered: all.is_some() || songs.is_some(), ..MusicFound::default() };
        for answer in all.iter().chain(songs.iter()) {
            let mut watches = Vec::new();
            collect(answer, &["watchEndpoint"], &mut watches);
            found.ids.extend(
                watches
                    .into_iter()
                    .filter(|(_, watch)| is_music_video(watch))
                    .filter_map(|(_, watch)| watch.get("videoId")?.as_str().map(str::to_owned)),
            );
        }
        if let Some(answer) = &all {
            let mut rows = Vec::new();
            collect(answer, &["musicResponsiveListItemRenderer"], &mut rows);
            let mut seen = std::collections::HashSet::new();
            let mut videos = Vec::new();
            for (_, row) in rows {
                match music_item(&json!({ "musicResponsiveListItemRenderer": row })) {
                    Some(MusicItem::Album(album)) if seen.insert(album["id"].to_string()) => found.albums.push(album),
                    // Official music videos (its songs come from the song search, below).
                    Some(MusicItem::Song(video)) if official_video(row) => videos.push(video),
                    _ => {}
                }
            }
            found.songs = videos;
        }
        if let Some(answer) = &songs {
            let mut rows = Vec::new();
            collect(answer, &["musicResponsiveListItemRenderer"], &mut rows);
            let videos = std::mem::take(&mut found.songs);
            found.songs = rows.into_iter().filter_map(|(_, row)| music_song_row(row)).chain(videos).collect();
        }
        let _ = send.send(found);
    });
    receive
}

/// Whether a YouTube Music row plays an artist's official music video.
fn official_video(row: &Value) -> bool {
    let mut watches = Vec::new();
    collect(row, &["watchEndpoint"], &mut watches);
    watches.iter().any(|(_, watch)| {
        watch.pointer("/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType").and_then(Value::as_str)
            == Some("MUSIC_VIDEO_TYPE_OMV")
    })
}

/// YouTube Music's "Songs" search filter.
const SONGS_FILTER: &str = "EgWKAQIIAWoMEA4QChADEAQQCRAF";

/// One YouTube Music search (with one of its filters), or nothing if it fails.
fn music_search(query: &str, params: Option<&str>) -> Option<Value> {
    let mut body = json!({
        "query": query,
        "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": "1.20251101.01.00", "hl": "en", "gl": "US" } },
    });
    if let Some(params) = params {
        body["params"] = json!(params);
    }
    post_json("https://music.youtube.com/youtubei/v1/search?prettyPrint=false", &body).ok()
}

/// A row of YouTube Music's song search, "Title" over "Artist • Album • 3:28", as a track.
fn music_song_row(row: &Value) -> Option<Value> {
    let mut watches = Vec::new();
    collect(row, &["watchEndpoint"], &mut watches);
    let id = watches.iter().find(|(_, watch)| is_music_video(watch))?.1.get("videoId")?.as_str()?;
    let columns = row.get("flexColumns").and_then(Value::as_array)?;
    let column = |at: usize| columns.get(at).and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text")).map(text).unwrap_or_default();
    let title = column(0);
    let byline = column(1);
    let parts: Vec<&str> = byline.split(" • ").map(str::trim).collect();
    let duration = parts.last().map(|part| parse_clock(part)).unwrap_or(0);
    let artist = parts.first().copied().unwrap_or("");
    let album = if parts.len() >= 3 { parts[1] } else { "" };
    let mut track = video_track(id, &title, artist, duration);
    track["title"] = json!(title);
    track["artist"] = json!(artist);
    track["album"] = json!(album);
    if let Some(cover) = first(row, "thumbnails").and_then(Value::as_array).and_then(|sizes| sizes.last()).and_then(|size| size.get("url")) {
        track["cover"] = cover.clone();
        track["coverBig"] = cover.clone();
    }
    Some(track)
}

/// Whether a YouTube Music play button is for a released song or an official
/// music video (not someone's upload, which can be anything, or a podcast episode).
fn is_music_video(watch: &Value) -> bool {
    let kind = watch.pointer("/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType").and_then(Value::as_str);
    matches!(kind, Some("MUSIC_VIDEO_TYPE_ATV" | "MUSIC_VIDEO_TYPE_OMV"))
}

/// Leaves a result's music score lower for words the search didn't ask for
/// ("reaction", "tutorial"…), which the score alone can't know about.
fn penalise_unwanted(items: &mut [Lockup], query: &str) {
    let query = query.to_lowercase();
    for item in items.iter_mut() {
        let value = match item {
            Lockup::Video(value) | Lockup::Playlist(value) => value,
        };
        let title = value["rawTitle"].as_str().or(value["title"].as_str()).unwrap_or("").to_lowercase();
        let unwanted = NOT_MUSIC.iter().filter(|word| contains_word(&title, word) && !query.contains(*word)).count() as i64;
        value[MUSIC] = json!(value[MUSIC].as_i64().unwrap_or(0) - unwanted * UNWANTED);
    }
}

/// Music first, then what only looks like music, then the rest, then what's
/// clearly not music; YouTube's own order within each. `music_known`: YouTube
/// Music answered, so a video it didn't list (with no other sure sign) is
/// taken as not music.
fn music_first(items: &mut [Lockup], prefer_music: bool, music_known: bool) {
    if prefer_music {
        items.sort_by_key(|item| match item {
            Lockup::Video(value) | Lockup::Playlist(value) => tier(value, music_known),
        });
    }
}

/// 0: surely music (a strong sign, see [`Signals::strong`]); 1: looks like
/// music (only weaker signs: words, "Artist - Title", a song's length); 2:
/// can't tell; 3: clearly something else.
fn tier(value: &Value, music_known: bool) -> u8 {
    let score = value[MUSIC].as_i64().unwrap_or(0);
    if value[STRONG] == true && score >= 0 {
        0
    } else if score >= MUSIC_TIER {
        1
    } else if score > -MUSIC_TIER && !music_known {
        2
    } else {
        3
    }
}

/// Whether a result is music enough to show a long video as an album.
fn looks_like_music(value: &Value) -> bool {
    tier(value, false) <= 1
}

/// Takes the ordering's notes out of a result before it reaches the app.
fn without_scores(mut value: Value) -> Value {
    if let Some(fields) = value.as_object_mut() {
        fields.remove(MUSIC);
        fields.remove(STRONG);
    }
    value
}

/// Where a search result's music score is kept until results are ordered.
const MUSIC: &str = "_music";
/// Where it's noted that a result has a strong sign of being music.
const STRONG: &str = "_strong";
/// A score from here up looks like music; this far below zero, it clearly isn't.
const MUSIC_TIER: i64 = 25;
/// YouTube Music lists the video.
const ON_YOUTUBE_MUSIC: i64 = 50;
/// Each word in the title that says it's about music rather than music.
const UNWANTED: i64 = 30;

/// Words in titles of videos that are about music (or not music at all).
const NOT_MUSIC: &[&str] = &[
    "reaction", "reacts", "reacting", "review", "tutorial", "how to", "lesson", "podcast", "interview", "trailer",
    "gameplay", "let's play", "walkthrough", "vlog", "unboxing", "explained", "analysis", "breakdown", "#shorts",
    "documentary", "behind the scenes", "teaser",
];

/// Words in titles of songs and albums.
const MUSIC_WORDS: &[&str] = &[
    "official audio", "official video", "official music video", "music video", "lyric video", "lyrics", "visualizer",
    "visualiser", "(audio)", "[audio]", "official visualizer", "official lyric", "remastered", "full album", "ost",
    "soundtrack", "original score", "(official", "[official",
];

/// Whether `word` is in `text` on its own (not inside a longer word).
fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// What a search result's own listing says about it being music.
#[derive(Default)]
struct Signals {
    /// The channel has YouTube's "Official Artist Channel" note.
    artist_channel: bool,
    /// "Provided to YouTube by…": a label's upload of a released track.
    art_track: bool,
    /// YouTube's music note on the thumbnail.
    music_badge: bool,
    /// Live right now (or a premiere waiting to start).
    live: bool,
}

impl Signals {
    /// A sure sign the video is music: a label's upload, an artist's channel,
    /// a "- Topic" or VEVO channel, or YouTube's own music note.
    fn strong(&self, track: &Value) -> bool {
        let channel = track["channel"].as_str().unwrap_or("").to_lowercase();
        self.art_track || self.artist_channel || self.music_badge || channel.ends_with(" - topic") || channel.contains("vevo")
    }

    /// How sure the listing makes it that a video is music: 25 and up is
    /// music, below zero probably isn't.
    fn score_track(&self, track: &Value) -> i64 {
        let title = track["rawTitle"].as_str().unwrap_or("").to_lowercase();
        let channel = track["channel"].as_str().unwrap_or("").to_lowercase();
        let duration = track["duration"].as_u64().unwrap_or(0);
        let chapters = track["chapters"].as_u64().unwrap_or(0);
        let mut points = 0;
        if channel.ends_with(" - topic") {
            points += 40;
        }
        if self.art_track {
            points += 40;
        }
        if self.artist_channel {
            points += 30;
        }
        if self.music_badge {
            points += 25;
        }
        if channel.contains("vevo") {
            points += 30;
        }
        if MUSIC_WORDS.iter().any(|word| title.contains(word)) {
            points += 20;
        }
        // "Artist - Title"
        if title.split_once(" - ").is_some_and(|(artist, song)| !artist.trim().is_empty() && !song.trim().is_empty()) {
            points += 10;
        }
        if (90..=600).contains(&duration) {
            points += 10;
        }
        // Long videos Earshot splits into an album's songs.
        if chapters >= ALBUM_CHAPTERS && duration >= ALBUM_CHAPTERED_SECONDS {
            points += 15;
        } else if duration > ALBUM_VIDEO_SECONDS * 2 && !MUSIC_WORDS.iter().any(|word| title.contains(word)) {
            points -= 15;
        }
        if self.live && !["concert", "live at", "live from", "radio", "24/7"].iter().any(|word| title.contains(word)) {
            points -= 20;
        }
        points
    }
}

/// Playlists in a search are usually music; ones that say so more surely.
fn playlist_score(title: &str) -> i64 {
    let title = title.to_lowercase();
    let mut points = 15;
    if ["album", "ost", "soundtrack", "songs", "music", "hits", "discography", "mixtape", "ep"]
        .iter()
        .any(|word| contains_word(&title, word))
    {
        points += 15;
    }
    points
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

/// Videos for a search: YouTube Music's songs and official videos first, then
/// YouTube's own results, music first (see [`add_music`] and [`music_first`]).
pub fn search(query: &str, count: usize) -> Result<Vec<Value>, String> {
    let (mut items, _, music) = search_filtered(query, "", None)?;
    let music_known = music.answered;
    add_music(&mut items, music);
    music_first(&mut items, true, music_known);
    Ok(items
        .into_iter()
        .filter_map(|item| match item {
            Lockup::Video(video) => Some(without_scores(video)),
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

/// One page of a real YouTube search: playlists and full-length or chaptered
/// videos come back as albums, other videos as songs. With `prefer_music`,
/// music comes first and everything else after it, otherwise it's in YouTube's
/// order. `next` is the token for the following page (pass it back as `continuation`).
pub fn search_albums(query: &str, continuation: &str, prefer_music: bool) -> Result<Value, String> {
    let mut albums = Vec::new();
    let mut tracks = Vec::new();
    let (mut items, next, music) = search_filtered(query, continuation, None)?;
    let music_known = music.answered;
    if prefer_music {
        add_music(&mut items, music);
    }
    music_first(&mut items, prefer_music, music_known);
    for item in items {
        match item {
            Lockup::Playlist(playlist) => albums.push(without_scores(playlist)),
            // Long videos open as albums, if they're music ("Drone Buying
            // Guide 2026" stays a video).
            Lockup::Video(video) if is_album_video(&video) && (!prefer_music || looks_like_music(&video)) => {
                albums.push(video_album(&without_scores(video)))
            }
            Lockup::Video(video) => tracks.push(without_scores(video)),
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
    let Ok((items, _, _)) = search_filtered(channel, "", recent.then_some("EgIIBA==")) else { return Vec::new() };
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
            // A song's length ("3:28").
            _ if part.contains(':') && part.chars().all(|c| c.is_ascii_digit() || c == ':') => {}
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
        _ => video.map(|id| {
            // The length is in the byline or a column of its own ("3:28").
            let mut fixed = Vec::new();
            collect(renderer.get("fixedColumns").unwrap_or(&Value::Null), &["text"], &mut fixed);
            let length = byline
                .split(" • ")
                .map(parse_clock)
                .chain(fixed.iter().map(|(_, value)| parse_clock(&text(value))))
                .find(|seconds| *seconds > 0)
                .unwrap_or(0);
            // A top result's byline can leave the artist out ("Song • 2:33"); its link names them.
            let who = if who.is_empty() { linked_artist(renderer) } else { who.clone() };
            MusicItem::Song(video_track(id, &title, &who, length))
        }),
    }
}

/// The first artist a YouTube Music row links to.
fn linked_artist(renderer: &Value) -> String {
    let mut runs = Vec::new();
    collect(renderer, &["runs"], &mut runs);
    runs.iter()
        .filter_map(|(_, runs)| runs.as_array())
        .flatten()
        .find(|run| first(run, "pageType").and_then(Value::as_str) == Some("MUSIC_PAGE_TYPE_ARTIST"))
        .and_then(|run| run.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
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

    fn video(title: &str, channel: &str, duration: u64, signals: Signals) -> Lockup {
        let mut track = video_track(&title.replace(' ', ""), title, channel, duration);
        track[MUSIC] = json!(signals.score_track(&track));
        track[STRONG] = json!(signals.strong(&track));
        Lockup::Video(track)
    }

    fn titles(items: &[Lockup]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Lockup::Video(value) | Lockup::Playlist(value) => value["rawTitle"].as_str().or(value["title"].as_str()).unwrap_or("").to_owned(),
            })
            .collect()
    }

    #[test]
    fn music_comes_first_and_nothing_is_hidden() {
        // As YouTube lists "autumn leaves": a lesson and a reaction among the songs.
        let artist = || Signals { artist_channel: true, ..Signals::default() };
        let label = || Signals { artist_channel: true, art_track: true, ..Signals::default() };
        let mut items = vec![
            video("How to play Autumn Leaves - jazz guitar lesson", "Jazz Guitar Online", 1260, Signals::default()),
            video("Autumn Leaves (Remastered)", "Nat King Cole", 160, label()),
            video("Vocal coach reacts to Autumn Leaves", "Singer Reacts", 900, Signals::default()),
            video("Beegie Adair Trio Performs \"Autumn Leaves\" at Nashville Jazz Workshop", "Nashvillejazz", 584, Signals::default()),
            video("Eva Cassidy - Autumn Leaves (Blues Alley performance)", "Eva Cassidy", 290, artist()),
            video("🍁 Enchanting Autumn Ambience 🍁 4K Nature Scenes", "Relax with TV Backgrounds", 43200, Signals::default()),
            video("Autumn Leaves", "Cannonball Adderley - Topic", 660, Signals::default()),
        ];
        // YouTube Music didn't answer: YouTube's own results, ranked.
        penalise_unwanted(&mut items, "autumn leaves");
        music_first(&mut items, true, false);
        assert_eq!(
            titles(&items),
            [
                "Autumn Leaves (Remastered)",
                "Eva Cassidy - Autumn Leaves (Blues Alley performance)",
                "Autumn Leaves",
                "Beegie Adair Trio Performs \"Autumn Leaves\" at Nashville Jazz Workshop",
                "🍁 Enchanting Autumn Ambience 🍁 4K Nature Scenes",
                "How to play Autumn Leaves - jazz guitar lesson",
                "Vocal coach reacts to Autumn Leaves",
            ]
        );
        // The scores don't reach the app.
        assert!(items.into_iter().all(|item| match item {
            Lockup::Video(value) | Lockup::Playlist(value) => without_scores(value).get(MUSIC).is_none(),
        }));
    }

    #[test]
    fn asked_for_words_are_not_held_against_results() {
        let mut items = vec![
            video("Daft Punk - One More Time (Official Video)", "Daft Punk", 322, Signals { artist_channel: true, ..Signals::default() }),
            video("Producer reacts to Daft Punk - One More Time", "Beat Breakdown", 840, Signals::default()),
        ];
        penalise_unwanted(&mut items, "daft punk reaction");
        let scores: Vec<i64> = items.iter().map(|item| match item { Lockup::Video(v) | Lockup::Playlist(v) => v[MUSIC].as_i64().unwrap() }).collect();
        // "reacts" still counts against it (only "reaction" was asked for), but it isn't buried.
        assert!(scores[0] >= MUSIC_TIER);
        assert!(scores[1] > -2 * UNWANTED);
    }

    #[test]
    fn youtube_order_without_preferring_music() {
        let mut items = vec![
            video("Vocal coach reacts to Autumn Leaves", "Singer Reacts", 900, Signals::default()),
            video("Autumn Leaves", "Cannonball Adderley - Topic", 660, Signals::default()),
        ];
        penalise_unwanted(&mut items, "autumn leaves");
        music_first(&mut items, false, true);
        assert_eq!(titles(&items), ["Vocal coach reacts to Autumn Leaves", "Autumn Leaves"]);
    }

    #[test]
    fn words_count_only_on_their_own() {
        assert!(contains_word("the ost of zelda", "ost"));
        assert!(!contains_word("lost in the music", "ost"));
        assert!(!contains_word("preview", "review"));
        assert!(contains_word("album review: ok computer", "review"));
    }

    #[test]
    fn live_streams_and_long_unsplit_videos_rank_lower() {
        let plain = |title: &str, duration: u64, live: bool| {
            let track = video_track("x", title, "Someone", duration);
            Signals { live, ..Signals::default() }.score_track(&track)
        };
        assert!(plain("Lofi Girl - lofi hip hop radio 📚 beats to relax/study to", 0, true) > plain("Live Q&A with the band", 0, true));
        assert!(plain("Rain sounds for sleeping", 36000, false) < plain("Rain - Song", 200, false));
    }

    fn song(id: &str, title: &str) -> Value {
        video_track(id, title, "Castlevania Sound Team", 74)
    }

    #[test]
    fn youtube_music_comes_first_whatever_youtube_found() {
        // "drone": YouTube lists drone-flying videos, one music track, and an
        // official video YouTube Music also has.
        let official = || Signals { artist_channel: true, ..Signals::default() };
        let mut items = vec![
            video("Ultimate Drone Buying Guide for Total Beginners 2026", "QuickAssTutorials", 1306, Signals::default()),
            video("Cello Drone A", "Musician's Practice Partner - Topic", 363, Signals::default()),
            video("DJI NEO 2 - The Best Beginner Drone EVER!", "Mark McGee Photos", 580, Signals::default()),
            video("Drone (Official Video)", "Some Band", 240, official()),
        ];
        let shared = match &items[3] { Lockup::Video(v) | Lockup::Playlist(v) => v["youtubeId"].as_str().unwrap().to_owned() };
        let music = MusicFound {
            answered: true,
            songs: vec![song("a", "Drone in E (440Hz)"), song("b", "Ambient Drone"), song(&shared, "Drone")],
            albums: vec![json!({ "id": "yt:pl:OLAK", "title": "Drones" })],
            ..MusicFound::default()
        };
        add_music(&mut items, music);
        penalise_unwanted(&mut items, "drone");
        music_first(&mut items, true, true);
        assert_eq!(
            titles(&items),
            [
                // YouTube Music's, in its order (its copy of the shared video).
                "Drone in E (440Hz)",
                "Ambient Drone",
                "Drone",
                "Drones",
                // YouTube's music.
                "Cello Drone A",
                // Then the rest, in YouTube's order.
                "Ultimate Drone Buying Guide for Total Beginners 2026",
                "DJI NEO 2 - The Best Beginner Drone EVER!",
            ]
        );
    }

    #[test]
    fn long_videos_are_albums_only_when_they_are_music() {
        let guide = video_track("g", "Ultimate Drone Buying Guide for Total Beginners 2026", "QuickAssTutorials", 1306);
        let mut guide = guide;
        guide[MUSIC] = json!(Signals::default().score_track(&guide));
        assert!(!looks_like_music(&guide));
        let mut ost = video_track("o", "Dragon Warrior (NES) Soundtrack - 8BitStereo", "Niko's 8BitStereo", 2400);
        ost["chapters"] = json!(20);
        ost[MUSIC] = json!(Signals::default().score_track(&ost));
        assert!(looks_like_music(&ost));
    }
}
