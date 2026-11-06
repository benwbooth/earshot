//! Mixcloud: DJ mixes and radio shows, through Mixcloud's public API. They
//! play in Mixcloud's own embedded player inside the app, like YouTube's.

use serde_json::{Value, json};

use crate::net::{encode, get_json, post_json};

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

/// A show as an album card ("mc:/user/show/", the same as its song's key):
/// it opens as an album of one, like a long YouTube video.
fn mix_card(item: &Value) -> Option<Value> {
    let song = mix(item)?;
    Some(json!({
        "id": song["key"], "title": song["title"], "artist": song["artist"], "artistId": "",
        "cover": song["cover"], "coverBig": song["coverBig"],
        "year": item["created_time"].as_str().unwrap_or("").chars().take(4).collect::<String>(),
        "kind": "mix", "duration": song["duration"], "trackCount": 1,
    }))
}

/// A show ("mc:/user/show/") as an album of one, or someone's latest shows ("mc:user:<name>").
pub fn album(id: &str) -> Result<Value, String> {
    if let Some(name) = id.strip_prefix("mc:user:") {
        let path = format!("/{}/", encode(name));
        let (user, shows) = std::thread::scope(|scope| {
            let user = scope.spawn(|| get_json(&format!("{API}{path}")));
            let shows = scope.spawn(|| get_json(&format!("{API}{path}cloudcasts/?limit=50")));
            (user.join().unwrap_or_else(|_| Err("Mixcloud didn't answer".to_owned())), shows.join().unwrap_or_else(|_| Err("Mixcloud didn't answer".to_owned())))
        });
        let (user, mut tracks) = (user?, mixes(&shows?));
        if tracks.is_empty() {
            return Err("This Mixcloud profile has no shows.".to_owned());
        }
        let title = user["name"].as_str().unwrap_or(name).to_owned();
        for (index, song) in tracks.iter_mut().enumerate() {
            song["album"] = json!(title);
            song["albumId"] = json!(id);
            song["trackNumber"] = json!(index + 1);
        }
        let picture = |size: &str| user.pointer(&format!("/pictures/{size}")).and_then(Value::as_str).unwrap_or("").to_owned();
        let total: u64 = tracks.iter().filter_map(|t| t["duration"].as_u64()).sum();
        return Ok(json!({
            "id": id, "title": title, "artist": title, "artistId": "",
            "cover": picture("large"), "coverBig": picture("extra_large"), "year": "", "kind": "playlist",
            "trackCount": tracks.len(), "duration": total, "label": "Latest shows on Mixcloud",
            "releaseDate": "", "genres": [], "tracks": tracks, "source": "mixcloud", "pageUrl": user["url"],
        }));
    }
    let key = id.strip_prefix("mc:").filter(|key| key.starts_with('/')).ok_or("Unknown Mixcloud show")?;
    let item = get_json(&format!("{API}{key}"))?;
    let mut song = mix(&item).ok_or("Couldn't read this Mixcloud show.")?;
    song["album"] = song["title"].clone();
    song["albumId"] = json!(id);
    song["trackNumber"] = json!(1);
    let date = item["created_time"].as_str().unwrap_or("").chars().take(10).collect::<String>();
    let genres: Vec<Value> = item["tags"].as_array().map(|tags| tags.iter().filter_map(|tag| tag.get("name").cloned()).collect()).unwrap_or_default();
    Ok(json!({
        "id": id, "title": song["title"], "artist": song["artist"], "artistId": "",
        "cover": song["cover"], "coverBig": song["coverBig"], "year": date.chars().take(4).collect::<String>(), "kind": "mix",
        "trackCount": 1, "duration": song["duration"], "label": "A show on Mixcloud",
        "releaseDate": date, "genres": genres, "tracks": [song], "source": "mixcloud", "pageUrl": item["url"],
    }))
}

/// Asks the API mixcloud.com's own pages use (the public one has no trending list).
fn graphql(query: &str) -> Result<Value, String> {
    let answer = post_json("https://app.mixcloud.com/graphql", &json!({ "query": query }))?;
    if let Some(message) = answer.pointer("/errors/0/message").and_then(Value::as_str) {
        return Err(format!("Mixcloud: {message}"));
    }
    Ok(answer)
}

fn thumbnail(root: &Value, size: u32) -> String {
    root.as_str().filter(|root| !root.is_empty()).map(|root| format!("https://thumbnailer.mixcloud.com/unsafe/{size}x{size}/{root}")).unwrap_or_default()
}

/// What's asked about each show.
const SHOW: &str = "slug name audioLength publishDate owner { username displayName picture { urlRoot } } picture { urlRoot }";

/// The shows on Mixcloud's Trending page, as the public API would describe them.
fn trending() -> Result<Vec<Value>, String> {
    let answer = graphql(&format!("{{ viewer {{ trending(first: 40) {{ edges {{ node {{ {SHOW} }} }} }} }} }}"))?;
    Ok(shows(answer.pointer("/data/viewer/trending/edges")))
}

/// The hot shows in one of Mixcloud's categories, as its genre pages list
/// them (the public API only has the all-time most played, years old).
fn hot(slug: &str) -> Result<Vec<Value>, String> {
    let answer = graphql(&format!(
        "{{ viewer {{ discover(discoverTags: [{{ slug: \"{slug}\", type: TAG }}]) {{ shows(first: 24, orderBy: HOT) {{ edges {{ node {{ {SHOW} }} }} }} }} }} }}"
    ))?;
    Ok(shows(answer.pointer("/data/viewer/discover/shows/edges")))
}

fn shows(edges: Option<&Value>) -> Vec<Value> {
    edges
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|edge| {
            let show = edge.get("node")?;
            let (user, slug) = (show.pointer("/owner/username")?.as_str()?, show.get("slug")?.as_str()?);
            let picture = show.pointer("/picture/urlRoot").unwrap_or(&Value::Null);
            Some(json!({
                "key": format!("/{user}/{slug}/"),
                "url": format!("https://www.mixcloud.com/{user}/{slug}/"),
                "name": show["name"],
                "user": { "username": user, "name": show.pointer("/owner/displayName").cloned().unwrap_or(json!(user)),
                          "picture": thumbnail(show.pointer("/owner/picture/urlRoot").unwrap_or(&Value::Null), 300) },
                "audio_length": show["audioLength"],
                "created_time": show["publishDate"],
                "pictures": { "large": thumbnail(picture, 300), "extra_large": thumbnail(picture, 600) },
            }))
        })
        .collect()
}

/// A few of Mixcloud's main categories.
const CATEGORIES: [(&str, &str); 8] = [
    ("house", "House"), ("techno", "Techno"), ("drum-bass", "Drum & Bass"), ("hip-hop", "Hip Hop"),
    ("soul", "Soul"), ("jazz", "Jazz"), ("ambient", "Ambient"), ("funk", "Funk"),
];

/// Mixcloud's front page: the Trending shows, the DJs and stations behind
/// them, and the hot shows in its main categories.
pub fn featured() -> Result<Vec<Value>, String> {
    let (trending, categories) = std::thread::scope(|scope| {
        let trending = scope.spawn(trending);
        let categories: Vec<_> = CATEGORIES
            .iter()
            .map(|(slug, name)| (slug, name, scope.spawn(move || hot(slug))))
            .collect();
        let categories: Vec<_> =
            categories.into_iter().map(|(slug, name, job)| (*slug, *name, job.join().ok().and_then(Result::ok))).collect();
        (trending.join().ok().and_then(Result::ok).unwrap_or_default(), categories)
    });
    let mut sections = Vec::new();
    let songs: Vec<Value> = trending.iter().filter_map(mix).collect();
    sections.extend(crate::featured::section("trending", "Trending", "The shows everyone's playing on Mixcloud right now.", Vec::new(), songs));
    let mut seen = std::collections::HashSet::new();
    let creators: Vec<Value> = trending
        .iter()
        .filter(|show| seen.insert(show.pointer("/user/username").and_then(Value::as_str).unwrap_or("").to_owned()))
        .map(|show| {
            let user = &show["user"];
            json!({
                "id": format!("mc:user:{}", user["username"].as_str().unwrap_or("")), "title": user["name"],
                "artist": "Latest shows", "artistId": "", "cover": user["picture"], "coverBig": user["picture"],
                "year": "", "kind": "playlist",
            })
        })
        .collect();
    if let Some(mut section) = crate::featured::section("creators", "Trending DJs and stations", "Who's behind this week's Trending shows. Each opens their latest shows.", creators, Vec::new()) {
        section["round"] = json!(true);
        sections.push(section);
    }
    for (slug, name, found) in categories {
        let shows = found.unwrap_or_default().iter().filter_map(mix_card).collect();
        let about = format!("Hot {} shows on Mixcloud right now.", name.to_lowercase());
        sections.extend(crate::featured::section(slug, name, &about, shows, Vec::new()));
    }
    Ok(sections)
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

    /// Talks to mixcloud.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_featured() {
        let sections = featured().unwrap();
        assert_eq!(sections[0]["id"], "trending");
        let trending = sections[0]["tracks"].as_array().unwrap();
        assert!(trending.len() >= 10 && trending.iter().all(|t| t["mixcloudKey"].as_str().unwrap().starts_with('/') && t["cover"] != ""));
        assert!(sections.iter().any(|s| s["id"] == "house"), "the categories");
        // A creator opens as their latest shows, a show as an album of one.
        let creators = sections.iter().find(|s| s["id"] == "creators").expect("creators");
        let latest = album(creators["albums"][0]["id"].as_str().unwrap()).unwrap();
        assert!(!latest["tracks"].as_array().unwrap().is_empty());
        let show = sections.iter().find(|s| s["id"] == "house").unwrap()["albums"][0]["id"].as_str().unwrap().to_owned();
        let one = album(&show).unwrap();
        assert_eq!(one["tracks"][0]["key"], show);
    }
}
