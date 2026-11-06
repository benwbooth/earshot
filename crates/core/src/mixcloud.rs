//! Mixcloud: DJ mixes and radio shows, through Mixcloud's public API. They
//! play in Mixcloud's own embedded player inside the app, like YouTube's.

use serde_json::{Value, json};

use crate::net::{encode, get_json};

const API: &str = "https://api.mixcloud.com";

/// A mix ("cloudcast") as a playable song.
fn mix(item: &Value) -> Option<Value> {
    let key = item["key"].as_str()?;
    let pictures = &item["pictures"];
    let picture = |size: &str| pictures[size].as_str().unwrap_or("").to_owned();
    Some(json!({
        "key": format!("mc:{key}"),
        "service": "mixcloud",
        "mixcloudKey": key,
        "pageUrl": item["url"],
        "title": item["name"],
        "artist": item.pointer("/user/name").and_then(Value::as_str).unwrap_or(""),
        "artistId": "",
        "album": "",
        "albumId": "",
        "cover": picture("large"),
        "coverBig": picture("extra_large"),
        "duration": item["audio_length"].as_u64().unwrap_or(0),
    }))
}

fn mixes(answer: &Value) -> Vec<Value> {
    answer["data"].as_array().map(|items| items.iter().filter_map(mix).collect()).unwrap_or_default()
}

/// Mixes and shows on Mixcloud, in Mixcloud's order.
pub fn search(query: &str) -> Result<Value, String> {
    let answer = get_json(&format!("{API}/search/?q={}&type=cloudcast&limit=40", encode(query)))?;
    Ok(json!({ "albums": [], "tracks": mixes(&answer) }))
}

/// Shows Mixcloud finds similar to one (for Random radio).
pub fn similar(key: &str) -> Result<Vec<Value>, String> {
    let key = if key.ends_with('/') { key.to_owned() } else { format!("{key}/") };
    Ok(mixes(&get_json(&format!("{API}{key}similar/?limit=20"))?))
}

/// Popular shows with a genre or tag, for browsing.
pub fn browse(tag: &str) -> Result<Value, String> {
    let slug: String = tag
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let answer = get_json(&format!("{API}/discover/{}/popular/?limit=40", encode(&slug)))?;
    Ok(json!({ "albums": [], "tracks": mixes(&answer) }))
}

/// A Mixcloud link, imported as a playlist: a mix, a playlist, or someone's uploads.
pub fn import(url: &str) -> Result<Value, String> {
    let path = url
        .split("mixcloud.com")
        .nth(1)
        .map(|rest| rest.split(['?', '#']).next().unwrap_or(rest))
        .filter(|path| path.len() > 1)
        .ok_or("Paste a link to a Mixcloud show, playlist or profile.")?;
    let path = if path.ends_with('/') { path.to_owned() } else { format!("{path}/") };
    let item = get_json(&format!("{API}{path}"))?;
    if item["audio_length"].is_number() {
        let song = mix(&item).ok_or("Couldn't read this Mixcloud show.")?;
        return Ok(json!({ "title": song["title"], "tracks": [song] }));
    }
    let title = item["name"].as_str().unwrap_or("Mixcloud").to_owned();
    let mut tracks = Vec::new();
    let mut next = Some(format!("{API}{path}cloudcasts/?limit=100"));
    while let Some(page) = next.take() {
        let answer = get_json(&page)?;
        tracks.extend(mixes(&answer));
        next = answer.pointer("/paging/next").and_then(Value::as_str).map(str::to_owned).filter(|_| tracks.len() < 500);
    }
    if tracks.is_empty() {
        return Err("This Mixcloud link has no shows.".to_owned());
    }
    Ok(json!({ "title": title, "tracks": tracks }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_mix() {
        let item = json!({ "key": "/dj/show/", "name": "Show", "url": "https://www.mixcloud.com/dj/show/",
            "user": { "name": "DJ" }, "audio_length": 3600, "pictures": { "large": "a", "extra_large": "b" } });
        let song = mix(&item).unwrap();
        assert_eq!(song["key"], "mc:/dj/show/");
        assert_eq!(song["duration"], 3600);
    }

    /// Talks to mixcloud.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_search_and_import() {
        let found = search("dragon quest").unwrap();
        let first = found["tracks"][0].clone();
        assert!(first["mixcloudKey"].as_str().unwrap().starts_with('/'));
        let single = import(first["pageUrl"].as_str().unwrap()).unwrap();
        assert_eq!(single["tracks"].as_array().unwrap().len(), 1);
        let user = first["mixcloudKey"].as_str().unwrap().split('/').nth(1).unwrap().to_owned();
        let uploads = import(&format!("https://www.mixcloud.com/{user}/")).unwrap();
        assert!(!uploads["tracks"].as_array().unwrap().is_empty());
    }
}
