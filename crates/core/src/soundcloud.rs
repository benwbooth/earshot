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
    }))
}

fn playlist_card(item: &Value) -> Option<Value> {
    let id = item["id"].as_u64()?;
    Some(json!({
        "id": format!("sc:pl:{id}"),
        "title": item["title"],
        "artist": item.pointer("/user/username").and_then(Value::as_str).unwrap_or(""),
        "artistId": "",
        "cover": artwork(item, "t300x300"),
        "coverBig": artwork(item, "t500x500"),
        "year": item["release_date"].as_str().or(item["created_at"].as_str()).map(|d| d.chars().take(4).collect::<String>()).unwrap_or_default(),
        "kind": if item["is_album"] == true { "album" } else { "playlist" },
        "trackCount": item["track_count"],
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

/// A playlist or album ("sc:pl:<id>").
pub fn album(id: &str) -> Result<Value, String> {
    let number = id.strip_prefix("sc:pl:").ok_or("Unknown SoundCloud playlist")?;
    playlist_album(&api(&format!("/playlists/{number}?representation=full"))?)
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

    #[test]
    fn skips_previews_and_blocked_tracks() {
        let base = json!({ "id": 1, "title": "Song", "policy": "ALLOW", "streamable": true, "embeddable_by": "all", "user": { "username": "dj" } });
        assert_eq!(track(&base).unwrap()["artist"], "dj");
        let mut preview = base.clone();
        preview["policy"] = json!("SNIP");
        assert!(track(&preview).is_none());
        let mut private = base;
        private["embeddable_by"] = json!("me");
        assert!(track(&private).is_none());
    }
}
