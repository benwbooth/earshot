//! SoundCloud: search, tracks and playlists through the API soundcloud.com's
//! own web app uses. Songs play in SoundCloud's own embedded player inside the
//! app, the same way YouTube's do.

use std::sync::Mutex;

use serde_json::{Value, json};

use crate::net::{encode, get_json, get_text};

const API: &str = "https://api-v2.soundcloud.com";

/// soundcloud.com's web app identifies itself with a public client id, kept in
/// one of its scripts; it changes now and then, so it's looked up when needed.
fn client_id(refresh: bool) -> Result<String, String> {
    static ID: Mutex<Option<String>> = Mutex::new(None);
    let mut id = ID.lock().unwrap();
    if refresh {
        *id = None;
    }
    if let Some(known) = id.as_ref() {
        return Ok(known.clone());
    }
    let home = get_text("https://soundcloud.com/")?;
    let scripts: Vec<&str> = home
        .match_indices("https://a-v2.sndcdn.com/assets/")
        .filter_map(|(start, _)| home[start..].find(".js").map(|end| &home[start..start + end + 3]))
        .collect();
    for script in scripts.iter().rev() {
        let Ok(code) = get_text(script) else { continue };
        if let Some(found) = find_client_id(&code) {
            *id = Some(found.clone());
            return Ok(found);
        }
    }
    Err("Couldn't connect to SoundCloud right now.".to_owned())
}

fn find_client_id(code: &str) -> Option<String> {
    code.match_indices("client_id").find_map(|(at, _)| {
        let rest = code[at + "client_id".len()..].trim_start_matches([':', '=', '"', ' ']);
        let id: String = rest.chars().take_while(char::is_ascii_alphanumeric).collect();
        (id.len() == 32).then_some(id)
    })
}

/// Calls SoundCloud's API; `path` already has its query string started.
fn api(path: &str) -> Result<Value, String> {
    let call = |id: &str| get_json(&format!("{API}{path}&client_id={id}"));
    match call(&client_id(false)?) {
        Err(error) if error.contains("HTTP 401") || error.contains("HTTP 403") => call(&client_id(true)?),
        other => other,
    }
}

fn artwork(item: &Value, size: &str) -> String {
    item["artwork_url"]
        .as_str()
        .or_else(|| item["calculated_artwork_url"].as_str())
        .or_else(|| item.pointer("/user/avatar_url").and_then(Value::as_str))
        .map(|url| url.replace("-large.", &format!("-{size}.")))
        .unwrap_or_default()
}

/// A SoundCloud track as a playable song, or None if SoundCloud won't play it
/// in its embedded player (previews of paid-only tracks, blocked or private ones).
fn track(item: &Value) -> Option<Value> {
    let id = item["id"].as_u64()?;
    let title = item["title"].as_str()?;
    let policy = item["policy"].as_str().unwrap_or("ALLOW");
    if policy == "BLOCK" || policy == "SNIP" || item["streamable"] == false || item["embeddable_by"].as_str().is_some_and(|by| by != "all") {
        return None;
    }
    let artist = item
        .pointer("/publisher_metadata/artist")
        .and_then(Value::as_str)
        .map(|a| a.trim_matches(|c: char| c == '-' || c.is_whitespace()))
        .filter(|a| !a.is_empty())
        .or_else(|| item.pointer("/user/username").and_then(Value::as_str))
        .unwrap_or("");
    Some(json!({
        "key": format!("sc:{id}"),
        "service": "soundcloud",
        "soundcloudId": id,
        "pageUrl": item["permalink_url"],
        "title": title,
        "artist": artist,
        "artistId": "",
        "album": item.pointer("/publisher_metadata/album_title").and_then(Value::as_str).unwrap_or(""),
        "albumId": "",
        "cover": artwork(item, "t300x300"),
        "coverBig": artwork(item, "t500x500"),
        "duration": item["duration"].as_u64().unwrap_or(0) / 1000,
        "encrypted": only_encrypted(item),
    }))
}

/// Whether SoundCloud offers a track only as DRM-encrypted streams (labels'
/// ad-supported releases), which its player can't play without a Widevine
/// module, as in the app's built-in browser.
fn only_encrypted(item: &Value) -> bool {
    let protocols: Vec<&str> = item
        .pointer("/media/transcodings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| t.pointer("/format/protocol").and_then(Value::as_str))
        .collect();
    !protocols.is_empty() && protocols.iter().all(|p| p.contains("encrypted"))
}

/// [`only_encrypted`], looked up by id, for songs saved before it was noted.
pub fn encrypted(track_id: u64) -> Result<bool, String> {
    Ok(only_encrypted(&api(&format!("/tracks/{track_id}?"))?))
}

/// SoundCloud's own made-up playlists (trending by genre and the like) have a
/// name instead of a number: "sc:sp:trending-by-genre:house".
const SYSTEM_PLAYLIST: &str = "soundcloud:system-playlists:";

fn playlist_card(item: &Value) -> Option<Value> {
    let id = match item["urn"].as_str().and_then(|urn| urn.strip_prefix(SYSTEM_PLAYLIST)) {
        Some(name) => format!("sc:sp:{name}"),
        None => format!("sc:pl:{}", item["id"].as_u64()?),
    };
    let count = item["track_count"].as_u64().or_else(|| item["tracks"].as_array().map(|tracks| tracks.len() as u64));
    Some(json!({
        "id": id,
        "title": item["title"],
        "artist": item.pointer("/user/username").and_then(Value::as_str).unwrap_or(if id.starts_with("sc:sp:") { "SoundCloud" } else { "" }),
        "artistId": "",
        "cover": artwork(item, "t300x300"),
        "coverBig": artwork(item, "t500x500"),
        "year": item["release_date"].as_str().or(item["created_at"].as_str()).map(|d| d.chars().take(4).collect::<String>()).unwrap_or_default(),
        "kind": if item["is_album"] == true { "album" } else { "playlist" },
        "trackCount": count,
    }))
}

/// Songs and playlists or albums on SoundCloud, in SoundCloud's order.
pub fn search(query: &str) -> Result<Value, String> {
    let q = encode(query);
    let tracks = api(&format!("/search/tracks?q={q}&limit=40"))?;
    let playlists = api(&format!("/search/playlists?q={q}&limit=20"))?;
    let list = |value: &Value| value["collection"].as_array().cloned().unwrap_or_default();
    Ok(json!({
        "tracks": list(&tracks).iter().filter_map(track).collect::<Vec<_>>(),
        "albums": list(&playlists).iter().filter(|p| p["track_count"].as_u64().unwrap_or(0) > 0).filter_map(playlist_card).collect::<Vec<_>>(),
    }))
}

/// Playlists and songs tagged with a genre or tag, for browsing.
pub fn browse(tag: &str) -> Result<Value, String> {
    // SoundCloud only matches tags in lower case.
    let t = encode(&tag.trim().to_lowercase());
    let playlists = api(&format!("/search/playlists?q=*&filter.genre_or_tag={t}&limit=24"))?;
    let tracks = api(&format!("/search/tracks?q={t}&filter.genre_or_tag={t}&limit=40"))?;
    let list = |value: &Value| value["collection"].as_array().cloned().unwrap_or_default();
    Ok(json!({
        "albums": list(&playlists).iter().filter(|p| p["track_count"].as_u64().unwrap_or(0) > 0).filter_map(playlist_card).collect::<Vec<_>>(),
        "tracks": list(&tracks).iter().filter_map(track).collect::<Vec<_>>(),
    }))
}

/// Tracks SoundCloud suggests after one (for Random radio).
pub fn related(track_id: u64) -> Result<Vec<Value>, String> {
    let answer = api(&format!("/tracks/{track_id}/related?limit=30"))?;
    Ok(answer["collection"].as_array().map(|items| items.iter().filter_map(track).collect()).unwrap_or_default())
}

/// Where a track's audio is, for measuring the waveform: the files to read
/// one after another (a whole MP3, or an HLS stream's init and segments) and
/// what kind they are ("mp3" or "mp4"). Asked the way soundcloud.com's own
/// player asks, preferring MP3, then the smallest AAC.
pub fn audio(track_id: u64) -> Result<(Vec<String>, &'static str), String> {
    let item = api(&format!("/tracks/{track_id}?"))?;
    let authorization = item["track_authorization"].as_str().unwrap_or("");
    let mut transcodings: Vec<&Value> = item.pointer("/media/transcodings").and_then(Value::as_array).map(|t| t.iter().collect()).unwrap_or_default();
    let rank = |t: &Value| {
        let mime = t.pointer("/format/mime_type").and_then(Value::as_str).unwrap_or("");
        let progressive = t.pointer("/format/protocol").and_then(Value::as_str) == Some("progressive");
        let small = t["quality"].as_str() == Some("lq");
        match (mime.starts_with("audio/mpeg"), progressive, mime.contains("mp4a")) {
            (true, true, _) => 0,
            (true, false, _) => 1,
            (_, _, true) if small => 2,
            (_, _, true) => 3,
            _ => 9, // Opus, which can't be decoded here
        }
    };
    transcodings.sort_by_key(|t| rank(t));
    let mut last = "SoundCloud didn't offer this track's audio.".to_owned();
    for transcoding in transcodings.into_iter().filter(|t| rank(t) < 9) {
        let Some(url) = transcoding["url"].as_str() else { continue };
        let kind = if rank(transcoding) < 2 { "mp3" } else { "mp4" };
        let located = (|| {
            let id = client_id(false)?;
            let answer = get_json(&format!("{url}?client_id={id}&track_authorization={}", encode(authorization)))?;
            let stream = answer["url"].as_str().ok_or("SoundCloud didn't say where the audio is.")?.to_owned();
            if transcoding.pointer("/format/protocol").and_then(Value::as_str) == Some("progressive") {
                return Ok(vec![stream]);
            }
            hls_parts(&stream, &get_text(&stream)?)
        })();
        match located {
            Ok(parts) => return Ok((parts, kind)),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// An HLS playlist's files in order (its init section first), or an error if
/// it's encrypted.
fn hls_parts(base: &str, playlist: &str) -> Result<Vec<String>, String> {
    let absolute = |uri: &str| {
        if uri.starts_with("http") {
            uri.to_owned()
        } else {
            let dir = base.split('?').next().unwrap_or(base);
            format!("{}/{uri}", &dir[..dir.rfind('/').unwrap_or(dir.len())])
        }
    };
    let mut parts = Vec::new();
    for line in playlist.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(key) = line.strip_prefix("#EXT-X-KEY:") {
            if !key.contains("METHOD=NONE") {
                return Err("SoundCloud's stream for this track is encrypted.".to_owned());
            }
        } else if let Some(map) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = map.split("URI=\"").nth(1).and_then(|rest| rest.split('"').next()) {
                parts.push(absolute(uri));
            }
        } else if !line.starts_with('#') {
            parts.push(absolute(line));
        }
    }
    if parts.is_empty() { Err("SoundCloud's stream for this track was empty.".to_owned()) } else { Ok(parts) }
}

/// Playlists only list the first few tracks in full; fetch the rest by id.
fn full_tracks(items: &[Value]) -> Result<Vec<Value>, String> {
    let missing: Vec<u64> = items.iter().filter(|t| t["title"].is_null()).filter_map(|t| t["id"].as_u64()).collect();
    let mut fetched = std::collections::HashMap::new();
    for chunk in missing.chunks(50) {
        let ids: Vec<String> = chunk.iter().map(u64::to_string).collect();
        let answer = api(&format!("/tracks?ids={}", ids.join("%2C")))?;
        for item in answer.as_array().cloned().unwrap_or_default() {
            if let Some(id) = item["id"].as_u64() {
                fetched.insert(id, item);
            }
        }
    }
    Ok(items
        .iter()
        .filter_map(|item| if item["title"].is_null() { item["id"].as_u64().and_then(|id| fetched.get(&id)) } else { Some(item) })
        .filter_map(track)
        .collect())
}

fn playlist_album(item: &Value) -> Result<Value, String> {
    let card = playlist_card(item).ok_or("Couldn't read this SoundCloud playlist.")?;
    let id = card["id"].as_str().unwrap_or("").to_owned();
    let mut tracks = full_tracks(item["tracks"].as_array().map(|t| t.as_slice()).unwrap_or_default())?;
    if tracks.is_empty() {
        return Err("SoundCloud doesn't let any of this playlist's songs play here.".to_owned());
    }
    let title = card["title"].as_str().unwrap_or("Playlist").to_owned();
    for (index, song) in tracks.iter_mut().enumerate() {
        song["album"] = json!(title);
        song["albumId"] = json!(id);
        song["trackNumber"] = json!(index + 1);
    }
    let total: u64 = tracks.iter().filter_map(|t| t["duration"].as_u64()).sum();
    Ok(json!({
        "id": id, "title": title, "artist": card["artist"], "artistId": "",
        "cover": card["cover"], "coverBig": card["coverBig"], "year": card["year"],
        "kind": card["kind"], "trackCount": tracks.len(), "duration": total,
        "label": if card["kind"] == "album" { "Album on SoundCloud" } else { "Playlist on SoundCloud" },
        "releaseDate": item["release_date"].as_str().unwrap_or(""), "genres": [], "tracks": tracks,
        "source": "soundcloud", "pageUrl": item["permalink_url"],
    }))
}

/// A playlist or album ("sc:pl:<id>"), or one of SoundCloud's own ("sc:sp:<name>").
pub fn album(id: &str) -> Result<Value, String> {
    if let Some(name) = id.strip_prefix("sc:sp:") {
        return playlist_album(&api(&format!("/system-playlists/{SYSTEM_PLAYLIST}{}?representation=full", encode(name)))?);
    }
    let number = id.strip_prefix("sc:pl:").ok_or("Unknown SoundCloud playlist")?;
    playlist_album(&api(&format!("/playlists/{number}?representation=full"))?)
}

/// Every collection in an answer (the items of a "selection").
fn items(answer: &Value) -> Vec<Value> {
    answer["collection"].as_array().cloned().unwrap_or_default()
}

/// Playlists in a selection that open here (SoundCloud's own included).
fn selection_cards(selection: &Value) -> Vec<Value> {
    items(&selection["items"])
        .iter()
        .filter(|item| matches!(item["kind"].as_str(), Some("playlist" | "system-playlist")))
        .filter(|item| item["track_count"].as_u64() != Some(0))
        .filter_map(playlist_card)
        .collect()
}

/// SoundCloud's front page: what's trending, its charts, and the selections
/// on its Discover page (artists to watch, curated playlists, trending by genre).
pub fn featured() -> Result<Vec<Value>, String> {
    let (trending, charts, selections) = std::thread::scope(|scope| {
        let trending = scope.spawn(|| api("/charts?kind=trending&genre=soundcloud%3Agenres%3Aall-music&limit=50"));
        let charts = scope.spawn(|| api("/charts/selections?limit=10"));
        let selections = scope.spawn(|| api("/mixed-selections?limit=20"));
        let done = |job: std::thread::ScopedJoinHandle<'_, Result<Value, String>>| job.join().ok().and_then(Result::ok);
        (done(trending), done(charts), done(selections))
    });
    let mut sections = Vec::new();
    let songs = trending.map(|answer| items(&answer).iter().filter_map(|entry| track(&entry["track"])).collect()).unwrap_or_default();
    sections.extend(crate::featured::section("trending", "Trending", "The tracks taking off on SoundCloud right now.", Vec::new(), songs));
    // One country's charts (the first SoundCloud offers, the US's), not every country's.
    if let Some(chart) = charts.as_ref().and_then(|answer| items(answer).into_iter().next()) {
        sections.extend(crate::featured::section("charts", "Charts", "SoundCloud's charts: the top tracks, new and hot, and by genre.", selection_cards(&chart), Vec::new()));
    }
    for selection in selections.as_ref().map(items).unwrap_or_default() {
        let urn = selection["urn"].as_str().unwrap_or("");
        let id = urn.rsplit(':').next().unwrap_or(urn);
        let title = selection["title"].as_str().unwrap_or("").trim();
        let about = selection["description"].as_str().unwrap_or("").trim();
        // Spotlights on one artist have a sentence for a title.
        let (title, about) = if title.chars().count() > 40 { ("Spotlight", title) } else { (title, about) };
        sections.extend(crate::featured::section(id, title, about, selection_cards(&selection), Vec::new()));
    }
    Ok(sections)
}

/// A SoundCloud playlist, album or track link, imported as a playlist.
pub fn import(url: &str) -> Result<Value, String> {
    let item = api(&format!("/resolve?url={}", encode(url)))?;
    match item["kind"].as_str() {
        Some("playlist") => {
            let album = playlist_album(&item)?;
            Ok(json!({ "title": album["title"], "tracks": album["tracks"] }))
        }
        Some("track") => {
            let song = track(&item).ok_or("SoundCloud doesn't let this song play here.")?;
            Ok(json!({ "title": song["title"], "tracks": [song] }))
        }
        _ => Err("Paste a link to a SoundCloud playlist, album or song.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_client_id() {
        let code = r#"a.b,{client_id:"N5nHxilyZaG4XKY57pV0M6qx6JysgIFW",env:"production"}"#;
        assert_eq!(find_client_id(code).as_deref(), Some("N5nHxilyZaG4XKY57pV0M6qx6JysgIFW"));
        assert_eq!(find_client_id("client_id=short"), None);
    }

    /// Talks to soundcloud.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_search_and_playlist() {
        let found = search("dragon quest").unwrap();
        assert!(!found["tracks"].as_array().unwrap().is_empty());
        let first = found["albums"][0]["id"].as_str().unwrap().to_owned();
        let playlist = album(&first).unwrap();
        let tracks = playlist["tracks"].as_array().unwrap();
        assert!(tracks.len() > 5, "fills in the tracks past the first few: {}", tracks.len());
        let imported = import(playlist["pageUrl"].as_str().unwrap()).unwrap();
        assert_eq!(imported["tracks"].as_array().unwrap().len(), tracks.len());
    }

    /// Talks to soundcloud.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_featured() {
        let sections = featured().unwrap();
        assert_eq!(sections[0]["id"], "trending");
        assert!(sections[0]["tracks"].as_array().unwrap().len() >= 10);
        let charts = sections.iter().find(|s| s["id"] == "charts").expect("charts");
        let chart = charts["albums"][0]["id"].as_str().unwrap().to_owned();
        assert!(album(&chart).unwrap()["tracks"].as_array().unwrap().len() >= 10, "{chart}");
        // SoundCloud's own playlists (trending by genre) open too.
        let own = sections.iter().flat_map(|s| s["albums"].as_array().unwrap()).find(|a| a["id"].as_str().unwrap().starts_with("sc:sp:"));
        let own = own.expect("a SoundCloud playlist")["id"].as_str().unwrap().to_owned();
        assert!(album(&own).unwrap()["tracks"].as_array().unwrap().len() >= 10, "{own}");
        assert!(sections.len() >= 4, "{} sections", sections.len());
    }

    #[test]
    fn skips_previews_and_blocked_tracks() {
        let base = json!({ "id": 1, "title": "Song", "policy": "ALLOW", "streamable": true, "embeddable_by": "all", "user": { "username": "dj" } });
        assert_eq!(track(&base).unwrap()["artist"], "dj");
        let mut preview = base.clone();
        preview["policy"] = json!("SNIP");
        assert!(track(&preview).is_none());
        let mut locked = base.clone();
        locked["media"] = json!({ "transcodings": [{ "format": { "protocol": "cbc-encrypted-hls" } }, { "format": { "protocol": "ctr-encrypted-hls" } }] });
        assert_eq!(track(&locked).unwrap()["encrypted"], true);
        locked["media"]["transcodings"][1]["format"]["protocol"] = json!("hls");
        assert_eq!(track(&locked).unwrap()["encrypted"], false);
        let mut private = base;
        private["embeddable_by"] = json!("me");
        assert!(track(&private).is_none());
    }
}
