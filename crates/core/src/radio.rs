//! Reddit radio: songs people share in music subreddits, played in order.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::net::{Fetched, get_text_or_wait, unescape_html};
use crate::youtube::{split_title, thumbnail};

/// (group, subreddit, short description)
pub const STATIONS: &[(&str, &str, &str)] = &[
    ("Discover", "listentothis", "New and overlooked music"),
    ("Discover", "Music", "Everything people are sharing"),
    ("Discover", "ifyoulikeblank", "Recommendations"),
    ("Discover", "music_discovery", "Fresh finds"),
    ("Discover", "under10k", "Artists with under 10k plays"),
    ("Discover", "NewMusic", "Just released"),
    ("Rock", "indieheads", "Indie and alternative"),
    ("Rock", "Rock", "Rock of all kinds"),
    ("Rock", "classicrock", "The greats"),
    ("Rock", "postrock", "Crescendos"),
    ("Rock", "shoegaze", "Walls of sound"),
    ("Rock", "Punk", "Fast and loud"),
    ("Rock", "Metal", "Heavy"),
    ("Rock", "stonerrock", "Fuzz and riffs"),
    ("Rock", "progrockmusic", "Progressive rock"),
    ("Electronic", "electronicmusic", "Electronic of all kinds"),
    ("Electronic", "House", "Four to the floor"),
    ("Electronic", "techno", "Warehouse hours"),
    ("Electronic", "DnB", "Drum and bass"),
    ("Electronic", "ambientmusic", "Slow and spacious"),
    ("Electronic", "synthwave", "Neon nostalgia"),
    ("Electronic", "chillwave", "Hazy and warm"),
    ("Electronic", "outrun", "Night drives"),
    ("Hip-hop & R&B", "hiphopheads", "Hip-hop"),
    ("Hip-hop & R&B", "rnb", "Rhythm and blues"),
    ("Hip-hop & R&B", "trap", "Trap"),
    ("Hip-hop & R&B", "LofiHipHop", "Beats to study to"),
    ("Hip-hop & R&B", "90sHipHop", "Golden era"),
    ("Soul & Jazz", "Jazz", "Jazz"),
    ("Soul & Jazz", "soulmusic", "Soul"),
    ("Soul & Jazz", "funk", "Funk"),
    ("Soul & Jazz", "Blues", "Blues"),
    ("Soul & Jazz", "NuJazz", "Jazz meets electronics"),
    ("Folk & Country", "folk", "Folk"),
    ("Folk & Country", "indiefolk", "Indie folk"),
    ("Folk & Country", "country", "Country"),
    ("Folk & Country", "Bluegrass", "Bluegrass"),
    ("Pop", "popheads", "Pop"),
    ("Pop", "kpop", "K-pop"),
    ("Pop", "indie_pop", "Indie pop"),
    ("Pop", "Disco", "Disco"),
    ("World & more", "WorldMusic", "Sounds from everywhere"),
    ("World & more", "reggae", "Reggae"),
    ("World & more", "latinmusic", "Latin"),
    ("World & more", "Afrobeat", "Afrobeat"),
    ("World & more", "classicalmusic", "Classical"),
    ("World & more", "VintageObscura", "Rare and forgotten records"),
    ("World & more", "70sMusic", "The seventies"),
    ("World & more", "80sMusic", "The eighties"),
];

pub fn stations() -> Value {
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for (group, sub, about) in STATIONS {
        let entry = json!({ "id": sub, "name": format!("r/{sub}"), "about": about });
        match groups.iter_mut().find(|(name, _)| name == group) {
            Some((_, list)) => list.push(entry),
            None => groups.push(((*group).to_owned(), vec![entry])),
        }
    }
    json!(groups.into_iter().map(|(name, stations)| json!({ "name": name, "stations": stations })).collect::<Vec<_>>())
}

/// The built-in station for a subreddit, whatever its capitals.
pub fn built_in(subreddit: &str) -> Option<&'static str> {
    STATIONS.iter().map(|(_, sub, _)| *sub).find(|sub| sub.eq_ignore_ascii_case(subreddit))
}

/// The subreddit in what the listener typed: "r/name", "name" or a link to
/// it on reddit.com. Names are 2 to 21 letters, digits or underscores.
pub fn subreddit_name(text: &str) -> Result<String, String> {
    let text = text.trim();
    let rest = match text.find("/r/") {
        Some(at) => &text[at + 3..],
        None => text.strip_prefix("r/").or_else(|| text.strip_prefix("R/")).unwrap_or(text),
    };
    let name = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
    if name.is_empty() {
        return Err("Type a subreddit's name, like r/vaporwave.".to_owned());
    }
    if !(2..=21).contains(&name.len()) || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("\"{name}\" isn't a subreddit name: they're 2 to 21 letters, digits or underscores."));
    }
    Ok(built_in(name).map(str::to_owned).unwrap_or_else(|| name.to_owned()))
}

/// The listener's own stations, like the built-in ones.
pub fn custom_stations(subreddits: &[String]) -> Value {
    json!(subreddits.iter().map(|sub| json!({ "id": sub, "name": format!("r/{sub}"), "about": "Added by you" })).collect::<Vec<_>>())
}

fn cache() -> &'static Mutex<HashMap<String, (Instant, Value)>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, (Instant, Value)>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How long a station's songs count as fresh.
const FRESH: Duration = Duration::from_secs(15 * 60);

/// Reddit lets apps without an account read about one feed a minute; when
/// it says to wait, nothing is asked of it until then.
fn reddit_free_at() -> &'static Mutex<Option<Instant>> {
    static FREE_AT: Mutex<Option<Instant>> = Mutex::new(None);
    &FREE_AT
}

fn saved_path(subreddit: &str) -> Option<std::path::PathBuf> {
    crate::library::data_dir().map(|dir| dir.join("radio").join(format!("{}.json", subreddit.to_lowercase())))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The station as last loaded (kept on disk), with when it was loaded.
fn saved(subreddit: &str) -> Option<(u64, Value)> {
    let text = std::fs::read_to_string(saved_path(subreddit)?).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    Some((value["savedAt"].as_u64()?, value["station"].clone()))
}

fn save(subreddit: &str, station: &Value) {
    if let Some(path) = saved_path(subreddit) {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
        let _ = std::fs::write(path, json!({ "savedAt": unix_now(), "station": station }).to_string());
    }
}

/// The songs of every station kept on disk (the ones the listener has tuned in to).
pub fn saved_songs() -> Vec<Value> {
    let Some(Ok(entries)) = crate::library::data_dir().map(|dir| std::fs::read_dir(dir.join("radio"))) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        .flat_map(|saved| saved["station"]["tracks"].as_array().cloned().unwrap_or_default())
        .collect()
}

/// A station's songs. While Reddit asks for a pause, the answer carries
/// "retryIn" (seconds) and the last saved copy of the songs, if any, with its
/// "savedMinutesAgo"; the radio page counts down and asks again. Loaded
/// songs come with the "subreddit" name as Reddit writes it.
pub fn station(subreddit: &str) -> Result<Value, String> {
    let subreddit = &subreddit_name(subreddit)?;
    if let Some((when, value)) = cache().lock().unwrap().get(subreddit) {
        if when.elapsed() < FRESH {
            return Ok(value.clone());
        }
    }
    let saved = saved(subreddit);
    if let Some((at, value)) = &saved {
        if unix_now().saturating_sub(*at) < FRESH.as_secs() {
            return Ok(value.clone());
        }
    }

    let waiting = reddit_free_at().lock().unwrap().and_then(|free| free.checked_duration_since(Instant::now()));
    let fetched = match waiting {
        Some(left) => Fetched::Wait(left.as_secs().max(1)),
        None => get_text_or_wait(&format!("https://www.reddit.com/r/{subreddit}/hot/.rss?limit=100")).map_err(|error| {
            if error.contains("HTTP 404") {
                format!("Reddit has no subreddit called r/{subreddit}.")
            } else if error.contains("HTTP 403") {
                format!("r/{subreddit} is private, so its songs can't be played here.")
            } else {
                error
            }
        })?,
    };
    match fetched {
        Fetched::Text(feed) => {
            let head = feed.split("<entry>").next().unwrap_or("");
            // Reddit answers a subreddit that doesn't exist with a search for it.
            if head.contains("/subreddits/search") {
                return Err(format!("Reddit has no subreddit called r/{subreddit}."));
            }
            let tracks = parse_feed(&feed, subreddit);
            if tracks.is_empty() {
                return Err(format!("r/{subreddit} has no links to songs right now."));
            }
            // The feed names the subreddit with Reddit's own capitals.
            let name = between(head, "<category term=\"", "\"").filter(|name| name.eq_ignore_ascii_case(subreddit)).unwrap_or(subreddit);
            let result = json!({ "title": format!("r/{name}"), "subreddit": name, "tracks": tracks });
            cache().lock().unwrap().insert(subreddit.to_owned(), (Instant::now(), result.clone()));
            save(subreddit, &result);
            Ok(result)
        }
        Fetched::Wait(seconds) => {
            if waiting.is_none() {
                *reddit_free_at().lock().unwrap() = Some(Instant::now() + Duration::from_secs(seconds));
            }
            let mut result = match saved {
                Some((at, mut value)) => {
                    value["savedMinutesAgo"] = json!(unix_now().saturating_sub(at) / 60);
                    value
                }
                None => json!({ "title": format!("r/{subreddit}"), "tracks": [] }),
            };
            result["retryIn"] = json!(seconds);
            Ok(result)
        }
    }
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(&text[start..end])
}

pub fn youtube_id(text: &str) -> Option<String> {
    for marker in ["youtube.com/watch?v=", "youtube.com/watch?app=desktop&v=", "youtu.be/", "youtube.com/embed/", "youtube.com/shorts/", "music.youtube.com/watch?v="] {
        if let Some(pos) = text.find(marker) {
            let id: String = text[pos + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if id.len() == 11 {
                return Some(id);
            }
        }
    }
    None
}

/// Strips the "[Genre] (Year)" tags people add to post titles.
fn strip_tags(text: &str) -> (String, String) {
    let mut out = String::new();
    let mut tags = Vec::new();
    let mut depth = 0;
    let mut tag = String::new();
    for ch in text.chars() {
        match ch {
            '[' => {
                depth += 1;
                tag.clear();
            }
            ']' if depth > 0 => {
                depth -= 1;
                tags.push(tag.trim().to_owned());
            }
            _ if depth > 0 => tag.push(ch),
            _ => out.push(ch),
        }
    }
    let mut cleaned = out.trim().to_owned();
    if cleaned.ends_with(')') {
        if let Some(open) = cleaned.rfind('(') {
            let inside = &cleaned[open + 1..cleaned.len() - 1];
            if inside.len() == 4 && inside.chars().all(|c| c.is_ascii_digit()) {
                cleaned.truncate(open);
            }
        }
    }
    (cleaned.trim().to_owned(), tags.join(", "))
}

pub fn parse_feed(feed: &str, subreddit: &str) -> Vec<Value> {
    let mut tracks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in feed.split("<entry>").skip(1) {
        let Some(raw_title) = between(entry, "<title>", "</title>") else { continue };
        let title = unescape_html(&unescape_html(raw_title));
        let content = unescape_html(between(entry, "<content type=\"html\">", "</content>").unwrap_or(""));
        let video = youtube_id(&content);
        let (cleaned, tags) = strip_tags(&title);
        let has_separator = [" -- ", " - ", " – ", " — "].iter().any(|sep| cleaned.contains(sep));
        if video.is_none() && !has_separator {
            continue; // discussion posts, not songs
        }
        let (artist, song) = split_title(&cleaned, "");
        if song.is_empty() {
            continue;
        }
        let key = match &video {
            Some(id) => format!("yt:{id}"),
            None => format!("rd:{}:{}", artist.to_lowercase(), song.to_lowercase()),
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        let permalink = between(entry, "<link href=\"", "\"").unwrap_or("").to_owned();
        tracks.push(json!({
            "key": key,
            "youtubeId": video.clone().unwrap_or_default(),
            "title": song,
            "artist": artist,
            "album": "",
            "albumId": "",
            "artistId": "",
            "cover": video.as_deref().map(thumbnail).unwrap_or_default(),
            "coverBig": video.as_deref().map(thumbnail).unwrap_or_default(),
            "duration": 0,
            "tags": tags,
            "source": format!("r/{subreddit}"),
            "link": permalink,
        }));
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_feed_entries() {
        let feed = r#"<feed><entry><title>Khruangbin -- Maria Tambi&amp;eacute;n [psych/funk] (2018)</title>
<link href="https://www.reddit.com/r/listentothis/comments/x/" /><content type="html">&lt;a href=&quot;https://www.youtube.com/watch?v=dQw4w9WgXcQ&quot;&gt;link&lt;/a&gt;</content></entry>
<entry><title>Weekly discussion thread</title><content type="html">hi</content></entry>
<entry><title>Big Thief - Not [indie folk] (2022)</title><content type="html">&lt;a href=&quot;https://open.spotify.com/x&quot;&gt;</content></entry></feed>"#;
        let tracks = parse_feed(feed, "listentothis");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0]["artist"], "Khruangbin");
        assert_eq!(tracks[0]["youtubeId"], "dQw4w9WgXcQ");
        assert_eq!(tracks[0]["tags"], "psych/funk");
        assert_eq!(tracks[1]["title"], "Not");
        assert_eq!(tracks[1]["youtubeId"], "");
    }

    #[test]
    fn reads_typed_subreddits() {
        assert_eq!(subreddit_name(" r/Vaporwave ").unwrap(), "Vaporwave");
        assert_eq!(subreddit_name("chiptune").unwrap(), "chiptune");
        assert_eq!(subreddit_name("https://www.reddit.com/r/chiptune/comments/abc/x/").unwrap(), "chiptune");
        assert_eq!(subreddit_name("old.reddit.com/r/chiptune?sort=new").unwrap(), "chiptune");
        assert_eq!(subreddit_name("/r/LISTENTOTHIS").unwrap(), "listentothis", "built-in stations keep their own capitals");
        assert!(subreddit_name("").is_err());
        assert!(subreddit_name("a").is_err());
        assert!(subreddit_name("no spaces").is_err());
        assert!(subreddit_name("abcdefghijklmnopqrstuv").is_err(), "22 characters");
        assert!(subreddit_name("https://www.reddit.com/user/someone").is_err());
    }
}
