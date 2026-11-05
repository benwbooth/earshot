//! Bandcamp: search, albums and tracks from bandcamp.com's own pages and
//! search API. Songs stream from Bandcamp itself (the same 128 kbps MP3s its
//! web player uses); the links expire, so they're fetched when a song plays.

use serde_json::{Value, json};

use crate::net::{get_text_as_browser as get_text, post_json, unescape_html};

/// Cover art in a size Bandcamp serves: 2 is 350 px, 16 is 700 px.
fn art(art_id: u64, size: u32) -> String {
    if art_id == 0 { String::new() } else { format!("https://f4.bcbits.com/img/a{art_id:010}_{size}.jpg") }
}

fn search_kind(query: &str, filter: &str) -> Result<Vec<Value>, String> {
    let answer = post_json(
        "https://bandcamp.com/api/bcsearch_public_api/1/autocomplete_elastic",
        &json!({ "search_text": query, "search_filter": filter, "full_page": true, "fan_id": null }),
    )?;
    Ok(answer.pointer("/auto/results").and_then(Value::as_array).cloned().unwrap_or_default())
}

fn album_from_result(result: &Value) -> Option<Value> {
    let url = result["item_url_path"].as_str()?;
    let art_id = result["art_id"].as_u64().unwrap_or(0);
    Some(json!({
        "id": format!("bc:{url}"),
        "title": result["name"],
        "artist": result["band_name"],
        "artistId": "",
        "cover": art(art_id, 2),
        "coverBig": art(art_id, 16),
        "year": "",
        "kind": "bandcamp",
    }))
}

fn track_from_result(result: &Value) -> Option<Value> {
    let url = result["item_url_path"].as_str()?;
    let art_id = result["art_id"].as_u64().unwrap_or(0);
    Some(json!({
        "key": format!("bc:{url}"),
        "service": "bandcamp",
        "pageUrl": url,
        "bandcampTrackId": result["id"],
        "title": result["name"],
        "artist": result["band_name"],
        "artistId": "",
        "album": result["album_name"].as_str().unwrap_or(""),
        "albumId": "",
        "cover": art(art_id, 2),
        "coverBig": art(art_id, 16),
        "duration": 0,
    }))
}

/// Albums, songs and tags on Bandcamp, in Bandcamp's order.
pub fn search(query: &str) -> Result<Value, String> {
    let albums: Vec<Value> = search_kind(query, "a")?.iter().filter_map(album_from_result).collect();
    let tracks: Vec<Value> = search_kind(query, "t")?.iter().filter_map(track_from_result).collect();
    Ok(json!({ "albums": albums, "tracks": tracks, "tags": matching_tags(query) }))
}

/// Bandcamp's tag for some words: "Dragon Quest" -> "dragon-quest".
pub fn tag_slug(words: &str) -> String {
    let mut slug = String::new();
    for ch in words.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

fn tag_label(slug: &str) -> String {
    slug.replace('-', " ")
}

/// Bandcamp's genres and their sub-genres, as listed on bandcamp.com/discover.
pub fn tags() -> Result<Value, String> {
    static TAGS: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);
    if let Some(known) = TAGS.lock().unwrap().as_ref() {
        return Ok(known.clone());
    }
    let page = get_text("https://bandcamp.com/discover")?;
    let state = attribute_json(&page, "data-blob")?;
    let state = state.pointer("/appData/initialState").ok_or("Couldn't read Bandcamp's list of genres.")?;
    let list = |key: &str| state[key].as_array().cloned().unwrap_or_default();
    let genres: Vec<Value> = list("genres").iter().map(|g| json!({ "slug": g["slug"], "label": g["label"] })).collect();
    let subgenres: Vec<Value> = list("subgenres")
        .iter()
        .map(|g| json!({ "slug": g["slug"], "label": g["label"], "parent": g["parentSlug"] }))
        .collect();
    let tags = json!({ "genres": genres, "subgenres": subgenres });
    *TAGS.lock().unwrap() = Some(tags.clone());
    Ok(tags)
}

/// Tags that match a search: the search itself as a tag, then known genres containing it.
fn matching_tags(query: &str) -> Vec<Value> {
    let wanted = tag_slug(query);
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut found = vec![json!({ "slug": wanted, "label": tag_label(&wanted) })];
    if let Ok(known) = tags() {
        for tag in known["genres"].as_array().into_iter().chain(known["subgenres"].as_array()).flatten() {
            let slug = tag["slug"].as_str().unwrap_or("");
            let fits = slug.contains(&wanted) || (wanted.len() > 3 && wanted.contains(slug) && slug.len() > 3);
            if fits && found.iter().all(|f| f["slug"] != slug) {
                found.push(json!({ "slug": slug, "label": tag["label"] }));
            }
        }
    }
    found.truncate(12);
    found
}

/// Best-selling, new or random albums with a tag; `cursor` is "" for the first page.
pub fn tag(slug: &str, slice: &str, cursor: &str) -> Result<Value, String> {
    let answer = post_json(
        "https://bandcamp.com/api/discover/1/discover_web",
        &json!({
            "category_id": 0, "tag_norm_names": [slug], "geoname_id": 0,
            "slice": if slice.is_empty() { "top" } else { slice },
            "time_facet_id": null, "cursor": if cursor.is_empty() { "*" } else { cursor },
            "size": 48, "include_result_types": ["a"],
        }),
    )?;
    let albums: Vec<Value> = answer["results"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            let url = item["item_url"].as_str()?.split('?').next()?.to_owned();
            let image = item.pointer("/primary_image/image_id").and_then(Value::as_u64).unwrap_or(0);
            Some(json!({
                "id": format!("bc:{url}"),
                "title": item["title"],
                "artist": item["album_artist"].as_str().or(item["band_name"].as_str()).unwrap_or(""),
                "artistId": "",
                "cover": art(image, 2),
                "coverBig": art(image, 16),
                "year": "",
                "kind": "bandcamp",
                "location": item["band_location"],
            }))
        })
        .collect();
    let next = if albums.is_empty() { "" } else { answer["cursor"].as_str().unwrap_or("") };
    Ok(json!({ "slug": slug, "label": tag_label(slug), "albums": albums, "next": next }))
}

/// JSON kept in an HTML attribute, like data-tralbum="{&quot;…".
fn attribute_json(page: &str, attribute: &str) -> Result<Value, String> {
    let marker = format!("{attribute}=\"");
    let start = page.find(&marker).ok_or("This Bandcamp page has no music on it.")? + marker.len();
    let end = page[start..].find('"').ok_or("Couldn't read this Bandcamp page.")? + start;
    serde_json::from_str(&unescape_html(&page[start..end])).map_err(|error| format!("Couldn't read this Bandcamp page: {error}"))
}

/// The tags an album or track page lists ("…/discover/<tag>" links).
fn page_tags(page: &str) -> Vec<Value> {
    let mut tags: Vec<Value> = Vec::new();
    for (at, _) in page.match_indices("class=\"tag\" href=\"https://bandcamp.com/discover/") {
        let rest = &page[at + "class=\"tag\" href=\"https://bandcamp.com/discover/".len()..];
        let slug: String = rest.chars().take_while(|c| *c != '?' && *c != '"').collect();
        if !slug.is_empty() && tags.iter().all(|t| t["slug"] != slug) {
            tags.push(json!({ "slug": slug, "label": tag_label(&slug) }));
        }
    }
    tags
}

/// The release data Bandcamp puts in every album and track page, and the page's tags.
fn page_data(url: &str) -> Result<(Value, Vec<Value>), String> {
    let page = get_text(url)?;
    Ok((attribute_json(&page, "data-tralbum")?, page_tags(&page)))
}

fn page_root(url: &str) -> &str {
    let after_scheme = url.find("://").map(|i| i + 3).unwrap_or(0);
    url[after_scheme..].find('/').map(|i| &url[..after_scheme + i]).unwrap_or(url)
}

/// An album ("bc:<album or track page>") with its playable songs.
pub fn album(id: &str) -> Result<Value, String> {
    let url = id.strip_prefix("bc:").ok_or("Unknown Bandcamp album")?;
    let (data, tags) = page_data(url)?;
    let title = data.pointer("/current/title").and_then(Value::as_str).unwrap_or("Album").to_owned();
    let artist = data["artist"].as_str().unwrap_or("").to_owned();
    let art_id = data["art_id"].as_u64().unwrap_or(0);
    let date = data["album_release_date"].as_str().or(data.pointer("/current/release_date").and_then(Value::as_str)).unwrap_or("");
    let year: String = date.split_whitespace().nth(2).unwrap_or("").to_owned();
    let root = page_root(url);
    let tracks: Vec<Value> = data["trackinfo"]
        .as_array()
        .map(|list| list.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|track| track["file"].is_object())
        .enumerate()
        .map(|(index, track)| {
            let page = track["title_link"].as_str().map(|link| format!("{root}{link}")).unwrap_or_else(|| url.to_owned());
            json!({
                "key": format!("bc:{page}"),
                "service": "bandcamp",
                "pageUrl": page,
                "bandcampTrackId": track["track_id"],
                "title": track["title"],
                "artist": track["artist"].as_str().filter(|a| !a.is_empty()).unwrap_or(&artist),
                "artistId": "",
                "album": title,
                "albumId": id,
                "cover": art(art_id, 2),
                "coverBig": art(art_id, 16),
                "duration": track["duration"].as_f64().unwrap_or(0.0).round() as u64,
                "trackNumber": track["track_num"].as_u64().unwrap_or(index as u64 + 1),
            })
        })
        .collect();
    if tracks.is_empty() {
        return Err("Bandcamp doesn't stream any songs from this release.".to_owned());
    }
    let total: u64 = tracks.iter().filter_map(|t| t["duration"].as_u64()).sum();
    Ok(json!({
        "id": id, "title": title, "artist": artist, "artistId": "",
        "cover": art(art_id, 2), "coverBig": art(art_id, 16), "year": year, "kind": "bandcamp",
        "trackCount": tracks.len(), "duration": total, "label": "On Bandcamp",
        "releaseDate": date.split(" 00:").next().unwrap_or(""), "genres": [], "tracks": tracks,
        "source": "bandcamp", "pageUrl": url, "tags": tags,
    }))
}

/// A fresh stream link for one song (Bandcamp's links expire after a while).
pub fn stream(track: &Value) -> Result<String, String> {
    let page = track["pageUrl"].as_str().ok_or("This song has no Bandcamp page.")?;
    let wanted = track["bandcampTrackId"].as_u64();
    let (data, _) = page_data(page)?;
    let songs = data["trackinfo"].as_array().cloned().unwrap_or_default();
    let song = songs
        .iter()
        .find(|song| wanted.is_some() && song["track_id"].as_u64() == wanted)
        .or_else(|| songs.iter().find(|song| song["file"].is_object()))
        .ok_or("Bandcamp doesn't stream this song.")?;
    song.pointer("/file/mp3-128")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Bandcamp doesn't stream this song.".to_owned())
}

/// A Bandcamp album or track link, imported as a playlist.
pub fn import(url: &str) -> Result<Value, String> {
    let album = album(&format!("bc:{url}"))?;
    Ok(json!({ "title": album["title"], "tracks": album["tracks"] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_site_root() {
        assert_eq!(page_root("https://band.bandcamp.com/album/x"), "https://band.bandcamp.com");
    }

    /// Talks to bandcamp.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_search_album_and_stream() {
        let found = search("dragon quest").unwrap();
        let first = found["albums"][0]["id"].as_str().unwrap().to_owned();
        let album = album(&first).unwrap();
        assert!(!album["tracks"].as_array().unwrap().is_empty());
        assert!(stream(&album["tracks"][0]).unwrap().starts_with("https://"));
        assert!(stream(&found["tracks"][0]).unwrap().starts_with("https://"));
    }

    /// Artists on their own domains: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_custom_domain() {
        let album = album("bc:https://din.org.uk/album/fracture-din28").unwrap();
        assert!(!album["tracks"].as_array().unwrap().is_empty());
        assert!(stream(&album["tracks"][0]).unwrap().starts_with("https://"));
    }

    #[test]
    fn makes_tag_slugs() {
        assert_eq!(tag_slug("Dragon Quest"), "dragon-quest");
        assert_eq!(tag_slug("  hip-hop/rap "), "hip-hop-rap");
    }

    #[test]
    fn reads_page_tags() {
        let page = r#"<a class="tag" href="https://bandcamp.com/discover/classical?from=tralbum">classical</a>
            <a class="tag" href="https://bandcamp.com/discover/dragon-quest?from=tralbum">dragon quest</a>"#;
        let tags = page_tags(page);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[1]["label"], "dragon quest");
    }

    /// Talks to bandcamp.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_tags() {
        let known = tags().unwrap();
        assert!(known["genres"].as_array().unwrap().len() > 10);
        let first = tag("chiptune", "top", "").unwrap();
        assert!(!first["albums"].as_array().unwrap().is_empty());
        let more = tag("chiptune", "top", first["next"].as_str().unwrap()).unwrap();
        assert_ne!(first["albums"][0]["id"], more["albums"][0]["id"]);
        assert_eq!(matching_tags("chip")[0]["slug"], "chip");
    }

    #[test]
    fn pads_art_ids() {
        assert_eq!(art(392901771, 2), "https://f4.bcbits.com/img/a0392901771_2.jpg");
    }
}
