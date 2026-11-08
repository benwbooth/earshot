//! Bandcamp: search, albums and tracks from bandcamp.com's own pages and
//! search API. Songs stream from Bandcamp itself (the same 128 kbps MP3s its
//! web player uses); the links expire, so they're fetched when a song plays.

use serde_json::{Value, json};

use crate::net::{get_text_as_browser as get_text, post_json, unescape_html};

/// Cover art in a size Bandcamp serves: 2 is 350 px, 16 is 700 px.
fn art(art_id: u64, size: u32) -> String {
    if art_id == 0 { String::new() } else { format!("https://f4.bcbits.com/img/a{art_id:010}_{size}.jpg") }
}

/// Any other picture (a Bandcamp Daily story's, a show's), cut square: 2 is 350 px, 16 is 700 px.
fn picture(image_id: u64, size: u32) -> String {
    if image_id == 0 { String::new() } else { format!("https://f4.bcbits.com/img/{image_id:010}_{size}.jpg") }
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

/// Bandcamp's genres and their sub-genres. They come from bandcamp.com/tags:
/// bandcamp.com/discover now answers programs with a browser check
/// ("Client Challenge") instead of the page, so it's only a fallback.
pub fn tags() -> Result<Value, String> {
    static TAGS: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);
    if let Some(known) = TAGS.lock().unwrap().as_ref() {
        return Ok(known.clone());
    }
    let tags = get_text("https://bandcamp.com/tags")
        .and_then(|page| tags_from_tags_page(&page))
        .or_else(|first| get_text("https://bandcamp.com/discover").and_then(|page| tags_from_discover_page(&page)).map_err(|_| first))?;
    *TAGS.lock().unwrap() = Some(tags.clone());
    Ok(tags)
}

/// The genres and sub-genres in bandcamp.com/tags (its sign-up form's lists).
fn tags_from_tags_page(page: &str) -> Result<Value, String> {
    let blob = attribute_json(page, "data-blob")?;
    let lists = blob.get("signup_params").ok_or("Couldn't read Bandcamp's list of genres.")?;
    let genres: Vec<Value> = lists["genres"]
        .as_array()
        .ok_or("Couldn't read Bandcamp's list of genres.")?
        .iter()
        .map(|g| json!({ "slug": g["value"], "label": g["name"] }))
        .collect();
    let mut subgenres = Vec::new();
    for genre in &genres {
        let slug = genre["slug"].as_str().unwrap_or("");
        for sub in lists["subgenres"][slug].as_array().into_iter().flatten() {
            subgenres.push(json!({ "slug": sub["value"], "label": sub["name"], "parent": slug }));
        }
    }
    if genres.is_empty() {
        return Err("Couldn't read Bandcamp's list of genres.".to_owned());
    }
    Ok(json!({ "genres": genres, "subgenres": subgenres }))
}

/// The genres and sub-genres in bandcamp.com/discover's page state.
fn tags_from_discover_page(page: &str) -> Result<Value, String> {
    let state = attribute_json(page, "data-blob")?;
    let state = state.pointer("/appData/initialState").ok_or("Couldn't read Bandcamp's list of genres.")?;
    let list = |key: &str| state[key].as_array().cloned().unwrap_or_default();
    let genres: Vec<Value> = list("genres").iter().map(|g| json!({ "slug": g["slug"], "label": g["label"] })).collect();
    let subgenres: Vec<Value> = list("subgenres")
        .iter()
        .map(|g| json!({ "slug": g["slug"], "label": g["label"], "parent": g["parentSlug"] }))
        .collect();
    Ok(json!({ "genres": genres, "subgenres": subgenres }))
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
    let albums: Vec<Value> = answer["results"].as_array().map(|list| list.as_slice()).unwrap_or_default().iter().filter_map(album_from_discover).collect();
    let next = if albums.is_empty() { "" } else { answer["cursor"].as_str().unwrap_or("") };
    Ok(json!({ "slug": slug, "label": tag_label(slug), "albums": albums, "next": next }))
}

/// One album from Bandcamp's discover API.
fn album_from_discover(item: &Value) -> Option<Value> {
    // Bandcamp has answered both with snake_case names and, lately, camelCase ones.
    let field = |snake: &str, camel: &str| if item[snake].is_null() { item[camel].clone() } else { item[snake].clone() };
    let url = field("item_url", "itemUrl").as_str()?.split('?').next()?.to_owned();
    let image = item
        .pointer("/primary_image/image_id")
        .or_else(|| item.pointer("/primaryImage/imageId"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let artist = field("album_artist", "albumArtist");
    let band = field("band_name", "bandName");
    Some(json!({
        "id": format!("bc:{url}"),
        "title": item["title"],
        "artist": artist.as_str().filter(|a| !a.is_empty()).or(band.as_str()).unwrap_or(""),
        "artistId": "",
        "cover": art(image, 2),
        "coverBig": art(image, 16),
        "year": "",
        "kind": "bandcamp",
        "location": field("band_location", "bandLocation"),
    }))
}

/// Best-selling ("top") or newest ("new") albums across all of Bandcamp.
fn discover_all(slice: &str, size: u32) -> Result<Vec<Value>, String> {
    let answer = post_json(
        "https://bandcamp.com/api/discover/1/discover_web",
        &json!({
            "category_id": 0, "tag_norm_names": [], "geoname_id": 0, "slice": slice,
            "time_facet_id": null, "cursor": "*", "size": size, "include_result_types": ["a"],
        }),
    )?;
    let albums: Vec<Value> = answer["results"].as_array().map(|list| list.as_slice()).unwrap_or_default().iter().filter_map(album_from_discover).collect();
    if albums.is_empty() { Err("Bandcamp didn't list any albums.".to_owned()) } else { Ok(albums) }
}

/// "06 Nov 2026 14:20:15 GMT" -> "06 Nov 2026".
fn short_date(date: &str) -> String {
    date.split_whitespace().take(3).collect::<Vec<_>>().join(" ")
}

/// Bandcamp's editors' picks (Album of the Day, New & Notable): albums with
/// the editors' words about them, and a song from each.
fn picks(method: &str, count: u32) -> Result<Vec<(Value, Option<Value>)>, String> {
    let answer = post_json(&format!("https://bandcamp.com/api/editorial/3/{method}"), &json!({ "page_size": count, "next_cursor": null }))?;
    let items = answer["items"].as_array().map(|list| list.as_slice()).unwrap_or_default();
    let found: Vec<(Value, Option<Value>)> = items.iter().filter_map(pick).collect();
    if found.is_empty() { Err("Bandcamp didn't list any picks.".to_owned()) } else { Ok(found) }
}

fn pick(item: &Value) -> Option<(Value, Option<Value>)> {
    let player = &item["playerData"];
    let release = &player["album"];
    // A single song has no album: its own page stands in.
    let url = release["url"].as_str().or(player["url"].as_str())?.split('?').next()?.to_owned();
    let title = release["title"].as_str().or(player["title"].as_str()).unwrap_or("");
    let artist = player["artistName"].as_str().or(item.pointer("/attribution/name").and_then(Value::as_str)).unwrap_or("");
    let art_id = release["artId"].as_u64().or(player["artId"].as_u64()).or(item["imageId"].as_u64()).unwrap_or(0);
    let article = item["itemUrl"].as_str().filter(|link| link.contains("daily.bandcamp.com")).unwrap_or("");
    let genre = item["genre"].as_str().unwrap_or("");
    let album = json!({
        "id": format!("bc:{url}"),
        "title": title,
        "artist": artist,
        "artistId": "",
        "cover": art(art_id, 2),
        "coverBig": art(art_id, 16),
        "year": "",
        "kind": "bandcamp",
        "note": genre,
        "genre": genre,
        "blurb": item["description"].as_str().unwrap_or("").trim(),
        "article": article,
        "date": short_date(item["date"].as_str().unwrap_or("")),
    });
    let song = player["url"].as_str().filter(|_| !player["isStreamingDisabled"].as_bool().unwrap_or(false)).map(|page| {
        json!({
            "key": format!("bc:{page}"),
            "service": "bandcamp",
            "pageUrl": page,
            "bandcampTrackId": player["id"],
            "title": player["title"],
            "artist": artist,
            "artistId": "",
            "album": title,
            "albumId": format!("bc:{url}"),
            "cover": art(art_id, 2),
            "coverBig": art(art_id, 16),
            "duration": player["duration"].as_f64().unwrap_or(0.0).round() as u64,
        })
    });
    Some((album, song))
}

/// What's selling on Bandcamp right now (the front page's live feed): the
/// latest albums and songs bought, newest first, each once.
fn selling_now(count: usize) -> Result<Vec<Value>, String> {
    let text = get_text("https://bandcamp.com/api/salesfeed/1/get_initial")?;
    let answer: Value = serde_json::from_str(&text).map_err(|error| format!("Couldn't read Bandcamp's sales: {error}"))?;
    let events = answer.pointer("/feed_data/events").and_then(Value::as_array).map(|list| list.as_slice()).unwrap_or_default();
    let mut albums: Vec<Value> = Vec::new();
    for sale in events.iter().rev().flat_map(|event| event["items"].as_array().map(|list| list.as_slice()).unwrap_or_default()) {
        // Downloads ("a" albums, "t" songs) have the release's own art; merch has a photo of the thing.
        let digital = matches!(sale["item_type"].as_str(), Some("a" | "t"));
        let (Some(art_id), Some(url)) = (sale["art_id"].as_u64(), sale["url"].as_str()) else { continue };
        if !digital {
            continue;
        }
        let url = if url.starts_with("//") { format!("https:{url}") } else { url.to_owned() };
        let id = format!("bc:{url}");
        if albums.iter().any(|known| known["id"] == id.as_str()) {
            continue;
        }
        let paid = sale["amount_paid_fmt"].as_str().unwrap_or("");
        let country = sale["country"].as_str().unwrap_or("");
        let note = match (paid.is_empty(), country.is_empty()) {
            (false, false) => format!("{paid} in {country}"),
            (false, true) => paid.to_owned(),
            (true, false) => format!("In {country}"),
            (true, true) => String::new(),
        };
        albums.push(json!({
            "id": id,
            "title": sale["item_description"],
            "artist": sale["artist_name"],
            "artistId": "",
            "cover": art(art_id, 2),
            "coverBig": art(art_id, 16),
            "year": "",
            "kind": "bandcamp",
            "note": note,
        }));
        if albums.len() >= count {
            break;
        }
    }
    if albums.is_empty() { Err("Bandcamp's sales feed is quiet.".to_owned()) } else { Ok(albums) }
}

/// The text inside the first `<tag>…</tag>` in `xml` (CDATA unwrapped).
fn xml_text<'a>(xml: &'a str, tag: &str) -> &'a str {
    let open = format!("<{tag}>");
    let Some(start) = xml.find(&open).map(|at| at + open.len()) else { return "" };
    let Some(end) = xml[start..].find(&format!("</{tag}>")) else { return "" };
    let text = xml[start..start + end].trim();
    text.strip_prefix("<![CDATA[").and_then(|t| t.strip_suffix("]]>")).unwrap_or(text)
}

/// Words with any HTML tags left out and entities decoded.
fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for ch in html.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    unescape_html(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Bandcamp Daily's latest stories from its RSS feed (Albums of the Day left
/// out: they're shown as albums): title, blurb, picture, link.
fn daily_stories(feed: &str, count: usize) -> Vec<Value> {
    let mut stories = Vec::new();
    for item in feed.split("<item>").skip(1) {
        let category = plain_text(xml_text(item, "category"));
        if category == "Album of the Day" {
            continue;
        }
        let description = xml_text(item, "description");
        // The picture comes first, then the blurb, then a "Read full story" line.
        let image_id = description
            .split("/img/")
            .nth(1)
            .and_then(|rest| rest.split('_').next())
            .and_then(|id| id.parse::<u64>().ok())
            .unwrap_or(0);
        let blurb = description
            .split("<p>")
            .map(plain_text)
            .find(|text| !text.is_empty() && !text.starts_with("Read full story"))
            .unwrap_or_default();
        let url = plain_text(xml_text(item, "link"));
        if url.is_empty() {
            continue;
        }
        stories.push(json!({
            "title": plain_text(xml_text(item, "title")),
            "blurb": blurb,
            "image": picture(image_id, 16),
            "url": url,
            "category": category,
            "author": plain_text(xml_text(item, "dc:creator")),
            "date": short_date(plain_text(xml_text(item, "pubDate")).split_once(", ").map(|(_, rest)| rest).unwrap_or("")),
        }));
        if stories.len() >= count {
            break;
        }
    }
    stories
}

/// The newest Bandcamp Weekly (Bandcamp's radio show), as a story to open on the web.
fn weekly_show() -> Result<Value, String> {
    let answer = post_json("https://bandcamp.com/api/design_system/1/editorial_recommendations", &json!({}))?;
    let show = &answer["weekly"];
    let url = show["url"].as_str().ok_or("Bandcamp didn't say what's on Bandcamp Weekly.")?;
    Ok(json!({
        "title": "Bandcamp Weekly",
        "blurb": show["title"].as_str().unwrap_or("").trim(),
        "image": picture(show["imageId"].as_u64().unwrap_or(0), 16),
        "url": url,
        "category": "Bandcamp Radio",
        "author": "",
        "date": "",
    }))
}

/// Bandcamp's front page, for discovery: today's Album of the Day as the
/// highlight, then sections of albums (editors' picks, what's selling right
/// now, best-selling, new arrivals), songs from the picks and Bandcamp
/// Daily's stories (to open on the web). Each section: { id, title, about,
/// link, rows, albums, tracks, articles }. Fetched side by side and kept for
/// half an hour.
pub fn featured() -> Result<Value, String> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static FEATURED: Mutex<Option<(Instant, Value)>> = Mutex::new(None);
    if let Some((at, known)) = FEATURED.lock().unwrap().as_ref() {
        if at.elapsed() < Duration::from_secs(30 * 60) {
            return Ok(known.clone());
        }
    }

    let (aotd, notable, selling, top, new, daily, weekly) = std::thread::scope(|scope| {
        let aotd = scope.spawn(|| picks("get_aotd", 13));
        let notable = scope.spawn(|| picks("get_new_and_notable", 24));
        let selling = scope.spawn(|| selling_now(24));
        let top = scope.spawn(|| discover_all("top", 24));
        let new = scope.spawn(|| discover_all("new", 24));
        let daily = scope.spawn(|| get_text("https://daily.bandcamp.com/feed").map(|feed| daily_stories(&feed, 12)));
        let weekly = scope.spawn(weekly_show);
        fn done<T>(job: std::thread::ScopedJoinHandle<'_, Result<T, String>>) -> Result<T, String> {
            job.join().unwrap_or_else(|_| Err("Bandcamp's page failed.".to_owned()))
        }
        (done(aotd), done(notable), done(selling), done(top), done(new), done(daily), done(weekly))
    });
    let complete = [aotd.is_ok(), notable.is_ok(), selling.is_ok(), top.is_ok(), new.is_ok(), daily.is_ok(), weekly.is_ok()].iter().all(|ok| *ok);
    let first_error = [aotd.as_ref().err(), notable.as_ref().err(), selling.as_ref().err(), top.as_ref().err(), new.as_ref().err()]
        .into_iter()
        .flatten()
        .next()
        .cloned();

    let section = |id: &str, title: &str, about: &str, link: &str, rows: u32| {
        json!({ "id": id, "title": title, "about": about, "link": link, "rows": rows, "albums": [], "tracks": [], "articles": [] })
    };
    let mut sections: Vec<Value> = Vec::new();
    let mut highlight = Value::Null;
    let (aotd, notable) = (aotd.unwrap_or_default(), notable.unwrap_or_default());
    if let Some(((today, _), earlier)) = aotd.split_first().map(|(first, rest)| (first.clone(), rest)) {
        highlight = today;
        if !earlier.is_empty() {
            let mut more = section(
                "aotd",
                "Albums of the Day",
                "Bandcamp Daily's pick of the day, every weekday.",
                "https://daily.bandcamp.com/album-of-the-day",
                1,
            );
            more["albums"] = earlier.iter().map(|(album, _)| album.clone()).collect();
            sections.push(more);
        }
    }
    if !notable.is_empty() {
        let mut picked = section("notable", "New & Notable", "New releases hand-picked by Bandcamp's editors.", "", 2);
        picked["albums"] = notable.iter().map(|(album, _)| album.clone()).collect();
        sections.push(picked);
    }
    // A song from each pick, the two lists woven together.
    let mut songs: Vec<Value> = Vec::new();
    for index in 0..notable.len().max(aotd.len()) {
        for list in [&notable, &aotd] {
            if let Some((_, Some(song))) = list.get(index) {
                if songs.iter().all(|known| known["key"] != song["key"]) {
                    songs.push(song.clone());
                }
            }
        }
    }
    if !songs.is_empty() {
        let mut list = section("songs", "Songs from the picks", "A song from each of the editors' picks.", "", 1);
        songs.truncate(20);
        list["tracks"] = Value::from(songs);
        sections.push(list);
    }
    if let Ok(albums) = selling {
        let mut live = section("selling", "Selling right now", "What fans around the world are buying on Bandcamp this minute.", "https://bandcamp.com/", 1);
        live["albums"] = Value::from(albums);
        sections.push(live);
    }
    if let Ok(albums) = top {
        let mut best = section("top", "Best-selling", "The most-bought albums on all of Bandcamp.", "https://bandcamp.com/discover", 2);
        best["albums"] = Value::from(albums);
        sections.push(best);
    }
    if let Ok(albums) = new {
        let mut fresh = section("new", "New arrivals", "Just released on Bandcamp, from every genre.", "https://bandcamp.com/discover", 1);
        fresh["albums"] = Value::from(albums);
        sections.push(fresh);
    }
    let mut stories: Vec<Value> = weekly.into_iter().collect();
    stories.extend(daily.unwrap_or_default());
    if !stories.is_empty() {
        let mut read = section("daily", "Bandcamp Daily", "Stories, reviews and guides from Bandcamp's editors (they open on the web).", "https://daily.bandcamp.com/", 1);
        read["articles"] = Value::from(stories);
        sections.push(read);
    }

    if sections.is_empty() && highlight.is_null() {
        return Err(first_error.unwrap_or_else(|| "Couldn't reach Bandcamp.".to_owned()));
    }
    let featured = json!({ "highlight": highlight, "sections": sections });
    if complete {
        *FEATURED.lock().unwrap() = Some((Instant::now(), featured.clone()));
    }
    Ok(featured)
}

/// JSON kept in an HTML attribute, like data-tralbum="{&quot;…".
fn attribute_json(page: &str, attribute: &str) -> Result<Value, String> {
    let marker = format!("{attribute}=\"");
    let Some(start) = page.find(&marker).map(|at| at + marker.len()) else {
        // Bandcamp sometimes answers with a browser check instead of the page.
        return Err(if page.contains("Client Challenge") {
            "Bandcamp asked for a browser check instead of showing this page. Try again in a little while.".to_owned()
        } else {
            "This Bandcamp page has no music on it.".to_owned()
        });
    };
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

/// The albums a Bandcamp page recommends ("If you like this, you may also like").
fn recommended(page: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for (at, _) in page.match_indices("class=\"album-link\" href=\"") {
        let rest = &page[at + "class=\"album-link\" href=\"".len()..];
        let url: String = rest.chars().take_while(|c| *c != '?' && *c != '"').collect();
        if url.starts_with("http") && !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls
}

/// Songs like one (for Random radio): a couple of random tracks from each of
/// a few albums its page recommends.
pub fn related(track: &Value) -> Result<Vec<Value>, String> {
    let page_url = track["pageUrl"].as_str().ok_or("This song has no Bandcamp page.")?;
    let mut albums = recommended(&get_text(page_url)?);
    crate::random::shuffle(&mut albums);
    let mut songs = Vec::new();
    for url in albums.iter().take(4) {
        let Ok(album) = album(&format!("bc:{url}")) else { continue };
        let mut tracks = album["tracks"].as_array().cloned().unwrap_or_default();
        crate::random::shuffle(&mut tracks);
        songs.extend(tracks.into_iter().take(2));
    }
    crate::random::shuffle(&mut songs);
    Ok(songs)
}

/// The albums a song's Bandcamp page recommends, as album cards (for Albums for you).
pub fn recommended_albums(track: &Value, count: usize) -> Vec<Value> {
    let Some(page_url) = track["pageUrl"].as_str() else { return Vec::new() };
    let Ok(page) = get_text(page_url) else { return Vec::new() };
    recommended(&page)
        .iter()
        .take(count)
        .filter_map(|url| album(&format!("bc:{url}")).ok())
        .map(|mut album| {
            if let Some(object) = album.as_object_mut() {
                object.remove("tracks");
            }
            album
        })
        .collect()
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

/// Fills in the length of Bandcamp songs that came without one (Bandcamp's
/// search doesn't say), from each song's own page, a few at a time.
pub fn fill_durations(tracks: &mut [Value]) {
    let missing: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t["service"] == "bandcamp" && t["duration"].as_f64().unwrap_or(0.0) <= 0.0 && t["pageUrl"].is_string())
        .map(|(index, _)| index)
        .collect();
    for chunk in missing.chunks(6) {
        let found: Vec<(usize, Option<u64>)> = std::thread::scope(|scope| {
            let jobs: Vec<_> = chunk
                .iter()
                .map(|&index| {
                    let track = &tracks[index];
                    scope.spawn(move || {
                        let page = track["pageUrl"].as_str()?;
                        let wanted = track["bandcampTrackId"].as_u64();
                        let (data, _) = page_data(page).ok()?;
                        let songs = data["trackinfo"].as_array()?;
                        let song = songs.iter().find(|s| wanted.is_some() && s["track_id"].as_u64() == wanted).or_else(|| songs.first())?;
                        song["duration"].as_f64().map(|seconds| seconds.round() as u64).filter(|seconds| *seconds > 0)
                    })
                })
                .collect();
            chunk.iter().copied().zip(jobs.into_iter().map(|job| job.join().ok().flatten())).collect()
        });
        for (index, duration) in found {
            if let Some(duration) = duration {
                tracks[index]["duration"] = json!(duration);
            }
        }
    }
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
        assert_eq!(picture(48423610, 16), "https://f4.bcbits.com/img/0048423610_16.jpg");
    }

    #[test]
    fn reads_daily_stories() {
        let feed = r#"<rss><channel><title>Bandcamp Updates</title>
            <item>
              <title>Darryl Jenifer Looks Back at the Early Days of Bad Brains</title>
              <link>https://daily.bandcamp.com/features/darryl-jenifer-bad-brains-interview</link>
              <description><![CDATA[<p><a href="https://daily.bandcamp.com/features/x"><img src="https://f4.bcbits.com/img/0048423610_10.jpg"></a></p>
<p>The band&#8217;s first ever studio sessions finally see the light of day.</p>
<p>Read full story on the <a href="https://daily.bandcamp.com/features/x">Bandcamp Daily</a>.</p>
]]></description>
              <category>Features</category>
              <pubDate>Fri, 06 Nov 2026 16:36:04 -0000</pubDate>
              <dc:creator>Erick Bradshaw</dc:creator>
            </item>
            <item>
              <title>PISS, &#8220;you made me&#8221;</title>
              <link>https://daily.bandcamp.com/album-of-the-day/piss</link>
              <category>Album of the Day</category>
            </item></channel></rss>"#;
        let stories = daily_stories(feed, 10);
        assert_eq!(stories.len(), 1);
        assert_eq!(stories[0]["blurb"], "The band\u{2019}s first ever studio sessions finally see the light of day.");
        assert_eq!(stories[0]["image"], "https://f4.bcbits.com/img/0048423610_16.jpg");
        assert_eq!(stories[0]["category"], "Features");
        assert_eq!(stories[0]["author"], "Erick Bradshaw");
        assert_eq!(stories[0]["date"], "06 Nov 2026");
    }

    /// Talks to bandcamp.com and daily.bandcamp.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_featured() {
        let started = std::time::Instant::now();
        let front = featured().unwrap();
        eprintln!("featured in {:?}", started.elapsed());
        let highlight = &front["highlight"];
        assert!(highlight["id"].as_str().unwrap().starts_with("bc:https://"));
        assert!(!highlight["blurb"].as_str().unwrap().is_empty());
        let sections = front["sections"].as_array().unwrap();
        let ids: Vec<&str> = sections.iter().filter_map(|s| s["id"].as_str()).collect();
        for wanted in ["aotd", "notable", "songs", "selling", "top", "new", "daily"] {
            assert!(ids.contains(&wanted), "no {wanted} section in {ids:?}");
        }
        for section in sections {
            let total = ["albums", "tracks", "articles"].iter().map(|k| section[k].as_array().unwrap().len()).sum::<usize>();
            assert!(total > 0, "{} is empty", section["id"]);
            for album in section["albums"].as_array().unwrap() {
                assert!(album["id"].as_str().unwrap().starts_with("bc:https://"), "{album}");
                assert!(album["cover"].as_str().unwrap().starts_with("https://"), "{album}");
            }
            for story in section["articles"].as_array().unwrap() {
                assert!(story["url"].as_str().unwrap().starts_with("https://"), "{story}");
            }
        }
        // A pick opens as an album, and its song streams.
        assert!(!album(highlight["id"].as_str().unwrap()).unwrap()["tracks"].as_array().unwrap().is_empty());
        let songs = sections.iter().find(|s| s["id"] == "songs").unwrap();
        assert!(stream(&songs["tracks"][0]).unwrap().starts_with("https://"));
        // Kept for a while: the second ask is instant.
        let again = std::time::Instant::now();
        assert_eq!(featured().unwrap(), front);
        assert!(again.elapsed() < std::time::Duration::from_millis(50));
    }

    /// A trimmed bandcamp.com/tags page: its sign-up form lists the genres.
    const TAGS_PAGE: &str = r#"<div id="pagedata" data-blob="{&quot;signup_params&quot;:{&quot;genres&quot;:[{&quot;id&quot;:10,&quot;name&quot;:&quot;electronic&quot;,&quot;value&quot;:&quot;electronic&quot;},{&quot;id&quot;:2,&quot;name&quot;:&quot;ambient&quot;,&quot;value&quot;:&quot;ambient&quot;},{&quot;id&quot;:14,&quot;name&quot;:&quot;hip-hop/rap&quot;,&quot;value&quot;:&quot;hip-hop-rap&quot;}],&quot;subgenres&quot;:{&quot;electronic&quot;:[{&quot;name&quot;:&quot;chiptune&quot;,&quot;value&quot;:&quot;chiptune&quot;}],&quot;ambient&quot;:[{&quot;name&quot;:&quot;dark ambient&quot;,&quot;value&quot;:&quot;dark-ambient&quot;}]}}}"></div>"#;

    #[test]
    fn reads_genres_from_the_tags_page() {
        let tags = tags_from_tags_page(TAGS_PAGE).unwrap();
        assert_eq!(tags["genres"][2], json!({ "slug": "hip-hop-rap", "label": "hip-hop/rap" }));
        assert_eq!(tags["subgenres"][0], json!({ "slug": "chiptune", "label": "chiptune", "parent": "electronic" }));
        assert_eq!(tags["subgenres"][1]["parent"], "ambient");
    }

    #[test]
    fn says_when_bandcamp_shows_a_browser_check() {
        let challenge = "<html><head><title>Client Challenge</title></head><body>JavaScript is disabled</body></html>";
        let error = tags_from_tags_page(challenge).unwrap_err();
        assert!(error.contains("browser check"), "{error}");
    }

    /// Talks to bandcamp.com: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_genres_and_a_tag() {
        let tags = tags().unwrap();
        assert!(tags["genres"].as_array().unwrap().len() >= 20, "{tags}");
        assert!(tags["subgenres"].as_array().unwrap().len() >= 100);
        let ambient = tag("ambient", "", "").unwrap();
        assert!(!ambient["albums"].as_array().unwrap().is_empty());
    }
}
