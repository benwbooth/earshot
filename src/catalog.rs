//! Music catalogue: charts by genre and era, albums, artists and search.
//! Metadata and artwork come from Deezer's public API (no key needed);
//! the audio itself is found on YouTube when a track is played.

use serde_json::{Value, json};

use crate::net::{encode, get_json};

const API: &str = "https://api.deezer.com";

fn s(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn list(value: &Value) -> &[Value] {
    value.get("data").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// A track in the shape the interface uses everywhere.
pub fn track(value: &Value, album_fallback: Option<&Value>) -> Value {
    let album = value.get("album").filter(|album| album.is_object()).or(album_fallback);
    let artist = value.get("artist").cloned().unwrap_or(Value::Null);
    let empty = Value::Null;
    let album = album.unwrap_or(&empty);
    json!({
        "key": format!("dz:{}", s(value, "id")),
        "title": s(value, "title"),
        "artist": s(&artist, "name"),
        "artistId": s(&artist, "id"),
        "album": s(album, "title"),
        "albumId": s(album, "id"),
        "cover": s(album, "cover_medium"),
        "coverBig": s(album, "cover_xl"),
        "duration": value.get("duration").and_then(Value::as_u64).unwrap_or(0),
        "explicit": value.get("explicit_lyrics").and_then(Value::as_bool).unwrap_or(false),
        "trackNumber": value.get("track_position").and_then(Value::as_u64).unwrap_or(0),
        "disc": value.get("disk_number").and_then(Value::as_u64).unwrap_or(1),
    })
}

pub fn album(value: &Value, artist_fallback: Option<&Value>) -> Value {
    let artist = value.get("artist").filter(|artist| artist.is_object()).or(artist_fallback).cloned().unwrap_or(Value::Null);
    let release = s(value, "release_date");
    json!({
        "id": s(value, "id"),
        "title": s(value, "title"),
        "artist": s(&artist, "name"),
        "artistId": s(&artist, "id"),
        "cover": s(value, "cover_medium"),
        "coverBig": s(value, "cover_xl"),
        "year": release.get(..4).unwrap_or(""),
        "kind": s(value, "record_type"),
        "trackCount": value.get("nb_tracks").and_then(Value::as_u64).unwrap_or(0),
    })
}

fn artist(value: &Value) -> Value {
    json!({
        "id": s(value, "id"),
        "name": s(value, "name"),
        "picture": s(value, "picture_medium"),
        "pictureBig": s(value, "picture_xl"),
        "fans": value.get("nb_fan").and_then(Value::as_u64).unwrap_or(0),
        "albumCount": value.get("nb_album").and_then(Value::as_u64).unwrap_or(0),
    })
}

fn tracks(value: &Value) -> Vec<Value> {
    list(value).iter().map(|item| track(item, None)).collect()
}

fn albums(value: &Value) -> Vec<Value> {
    list(value).iter().map(|item| album(item, None)).collect()
}

/// The eras on the home screen, newest first.
pub const ERAS: &[(&str, &str, &str)] = &[
    ("2020s", "2020s hits", "#ff4f6d"),
    ("2010s", "2010s hits", "#ff8a3d"),
    ("2000s", "2000s hits", "#f5c542"),
    ("90s", "90s hits", "#3ddc97"),
    ("80s", "80s hits", "#3db8ff"),
    ("70s", "70s hits", "#8b6cff"),
    ("60s", "60s hits", "#d36cff"),
    ("50s", "50s hits", "#ff6cb5"),
];

pub fn home() -> Result<Value, String> {
    let chart = get_json(&format!("{API}/chart/0?limit=20"))?;
    let genres = get_json(&format!("{API}/genre"))?;
    let genres: Vec<Value> = list(&genres)
        .iter()
        .filter(|genre| s(genre, "id") != "0")
        .map(|genre| json!({ "id": s(genre, "id"), "name": s(genre, "name"), "picture": s(genre, "picture_medium") }))
        .collect();
    let eras: Vec<Value> = ERAS.iter().map(|(name, _, color)| json!({ "id": name, "name": name, "color": color })).collect();
    Ok(json!({
        "tracks": chart.get("tracks").map(tracks).unwrap_or_default(),
        "albums": chart.get("albums").map(albums).unwrap_or_default(),
        "genres": genres,
        "eras": eras,
    }))
}

pub fn genre_chart(genre: &str) -> Result<Value, String> {
    let genre: u64 = genre.parse().map_err(|_| "Unknown genre".to_owned())?;
    let info = get_json(&format!("{API}/genre/{genre}"))?;
    let chart = get_json(&format!("{API}/chart/{genre}?limit=50"))?;
    Ok(json!({
        "title": s(&info, "name"),
        "subtitle": "Most popular right now",
        "picture": s(&info, "picture_xl"),
        "tracks": chart.get("tracks").map(tracks).unwrap_or_default(),
        "albums": chart.get("albums").map(albums).unwrap_or_default(),
    }))
}

pub fn era_chart(era: &str) -> Result<Value, String> {
    let (name, query, color) = ERAS.iter().find(|(name, _, _)| *name == era).ok_or("Unknown era")?;
    let found = get_json(&format!("{API}/search/playlist?q={}&limit=10", encode(query)))?;
    let playlist = list(&found)
        .iter()
        .max_by_key(|item| {
            let size = item.get("nb_tracks").and_then(Value::as_u64).unwrap_or(0).min(100);
            let editorial = item.get("user").map(|user| s(user, "name").to_lowercase().contains("deezer")).unwrap_or(false);
            (editorial as u64) * 1000 + size
        })
        .ok_or("No chart found for that era")?;
    let tracks_value = get_json(&format!("{API}/playlist/{}/tracks?limit=100", s(playlist, "id")))?;
    Ok(json!({
        "title": format!("Best of the {name}"),
        "subtitle": format!("The biggest songs of the {name}"),
        "color": color,
        "tracks": tracks(&tracks_value),
        "albums": [],
    }))
}

pub fn search(query: &str) -> Result<Value, String> {
    let q = encode(query);
    let found_tracks = get_json(&format!("{API}/search/track?q={q}&limit=30"))?;
    let found_albums = get_json(&format!("{API}/search/album?q={q}&limit=24"))?;
    let found_artists = get_json(&format!("{API}/search/artist?q={q}&limit=12"))?;
    Ok(json!({
        "tracks": tracks(&found_tracks),
        "albums": albums(&found_albums),
        "artists": list(&found_artists).iter().map(artist).collect::<Vec<_>>(),
    }))
}

pub fn album_detail(id: &str) -> Result<Value, String> {
    let id: u64 = id.parse().map_err(|_| "Unknown album".to_owned())?;
    let info = get_json(&format!("{API}/album/{id}"))?;
    let listing = get_json(&format!("{API}/album/{id}/tracks?limit=300"))?;
    let mut summary = album(&info, None);
    let album_ref = json!({
        "id": s(&info, "id"), "title": s(&info, "title"),
        "cover_medium": s(&info, "cover_medium"), "cover_xl": s(&info, "cover_xl"),
    });
    let track_list: Vec<Value> = list(&listing).iter().map(|item| track(item, Some(&album_ref))).collect();
    let total: u64 = track_list.iter().filter_map(|t| t.get("duration").and_then(Value::as_u64)).sum();
    let genres: Vec<String> = info
        .get("genres")
        .map(|g| list(g).iter().map(|genre| s(genre, "name")).collect())
        .unwrap_or_default();
    summary["label"] = json!(s(&info, "label"));
    summary["releaseDate"] = json!(s(&info, "release_date"));
    summary["duration"] = json!(total);
    summary["genres"] = json!(genres);
    summary["trackCount"] = json!(track_list.len());
    summary["tracks"] = json!(track_list);
    Ok(summary)
}

pub fn artist_detail(id: &str) -> Result<Value, String> {
    let id: u64 = id.parse().map_err(|_| "Unknown artist".to_owned())?;
    let info = get_json(&format!("{API}/artist/{id}"))?;
    let top = get_json(&format!("{API}/artist/{id}/top?limit=10"))?;
    let discography = get_json(&format!("{API}/artist/{id}/albums?limit=100"))?;
    let mut result = artist(&info);
    let mut all: Vec<Value> = list(&discography).iter().map(|item| album(item, Some(&info))).collect();
    all.sort_by(|a, b| b["year"].as_str().cmp(&a["year"].as_str()));
    let (main, other): (Vec<Value>, Vec<Value>) = all.into_iter().partition(|item| item["kind"] == "album");
    result["topTracks"] = json!(tracks(&top));
    result["albums"] = json!(main);
    result["singles"] = json!(other);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_tracks() {
        let raw = json!({
            "id": 42, "title": "Song", "duration": 200,
            "artist": { "id": 7, "name": "Band" },
            "album": { "id": 9, "title": "Record", "cover_medium": "c" }
        });
        let t = track(&raw, None);
        assert_eq!(t["key"], "dz:42");
        assert_eq!(t["artist"], "Band");
        assert_eq!(t["albumId"], "9");
        assert_eq!(t["duration"], 200);
    }
}
