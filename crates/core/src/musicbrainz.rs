//! MusicBrainz, the open music encyclopedia: release dates for artists Deezer
//! doesn't have (for Release Radar), and ListenBrainz's similar artists,
//! worked out from what people listen to (for Albums for you). Releases are
//! "mb:<release group id>" albums, found on Deezer or YouTube when opened.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::net::{encode, get_json};
use crate::{catalog, youtube};

const API: &str = "https://musicbrainz.org/ws/2";

/// MusicBrainz asks for at most one request a second.
fn ask(path: &str) -> Result<Value, String> {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(at) = *last {
            let wait = Duration::from_millis(1100).saturating_sub(at.elapsed());
            std::thread::sleep(wait);
        }
        *last = Some(Instant::now());
    }
    get_json(&format!("{API}{path}"))
}

/// The artist's MusicBrainz id, if one artist has exactly that name.
pub fn artist_id(name: &str) -> Option<String> {
    let found = ask(&format!("/artist/?query={}&fmt=json&limit=3", encode(&format!("artist:\"{name}\"")))).ok()?;
    found["artists"]
        .as_array()?
        .iter()
        .find(|artist| artist["score"].as_u64() == Some(100) && artist["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .and_then(|artist| artist["id"].as_str().map(str::to_owned))
}

fn release(group: &Value, artist: &str) -> Value {
    let id = group["id"].as_str().unwrap_or("");
    let date = group["first-release-date"].as_str().unwrap_or("");
    json!({
        "id": format!("mb:{id}"),
        "title": group["title"],
        "artist": artist,
        "artistId": "",
        "cover": format!("https://coverartarchive.org/release-group/{id}/front-250"),
        "coverBig": format!("https://coverartarchive.org/release-group/{id}/front-500"),
        "year": date.get(..4).unwrap_or(""),
        "releaseDate": date,
        "kind": group["primary-type"].as_str().unwrap_or("album").to_lowercase(),
    })
}

/// The artist's albums, EPs and singles (not compilations or live), newest first.
pub fn releases(artist_mbid: &str, artist: &str) -> Vec<Value> {
    let Ok(found) = ask(&format!("/release-group?artist={artist_mbid}&fmt=json&limit=100")) else { return Vec::new() };
    let mut groups: Vec<Value> = found["release-groups"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|group| group["secondary-types"].as_array().is_none_or(|types| types.is_empty()))
        .filter(|group| group["first-release-date"].as_str().is_some_and(|date| !date.is_empty()))
        .collect();
    groups.sort_by(|a, b| b["first-release-date"].as_str().cmp(&a["first-release-date"].as_str()));
    groups.iter().map(|group| release(group, artist)).collect()
}

/// Artists people listen to alongside this one (ListenBrainz): (name, MusicBrainz id).
pub fn similar_artists(artist_mbid: &str) -> Vec<(String, String)> {
    let url = format!(
        "https://labs.api.listenbrainz.org/similar-artists/json?artist_mbids={artist_mbid}&algorithm=session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30"
    );
    get_json(&url)
        .ok()
        .and_then(|found| found.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|artist| Some((artist["name"].as_str()?.to_owned(), artist["artist_mbid"].as_str()?.to_owned())))
        .collect()
}

/// An "mb:" release, opened from Deezer's catalogue or, failing that, YouTube
/// (a playlist or a full-length video of it).
pub fn album(id: &str) -> Result<Value, String> {
    let group_id = id.strip_prefix("mb:").ok_or("Not a MusicBrainz release.")?;
    let group = ask(&format!("/release-group/{group_id}?inc=artist-credits&fmt=json"))?;
    let title = group["title"].as_str().unwrap_or("").to_owned();
    let artist = group["artist-credit"][0]["name"].as_str().unwrap_or("").to_owned();
    let query = format!("artist:\"{artist}\" album:\"{title}\"");
    if let Ok(found) = get_json(&format!("https://api.deezer.com/search/album?q={}&limit=1", encode(&query))) {
        if let Some(deezer) = found["data"][0]["id"].as_u64() {
            return catalog::album_detail(&deezer.to_string());
        }
    }
    let found = youtube::search_albums(&format!("{artist} {title}"), "")?;
    let first = found["albums"][0]["id"].as_str().ok_or_else(|| format!("Couldn't find \"{title}\" to play."))?.to_owned();
    youtube::album(&first)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to MusicBrainz and ListenBrainz: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_releases_and_similar() {
        let daft_punk = artist_id("Daft Punk").expect("Daft Punk's id");
        let found = releases(&daft_punk, "Daft Punk");
        assert!(found.iter().any(|r| r["title"] == "Discovery"));
        assert!(!similar_artists(&daft_punk).is_empty());
        let discovery = found.iter().find(|r| r["title"] == "Discovery").unwrap();
        assert!(!album(discovery["id"].as_str().unwrap()).unwrap()["tracks"].as_array().unwrap().is_empty());
    }
}
