//! The Internet Archive's music: the Live Music Archive's concert recordings,
//! netlabel releases, 78 rpm records and other freely shared audio. Items
//! open as albums; songs stream straight from archive.org.

use serde_json::{Value, json};

use crate::net::{encode, get_json};

/// The archive.org collections that hold music (not audiobooks, radio shows or podcasts).
const MUSIC: &str = "collection:(etree OR audio_music OR netlabels OR 78rpm OR georgeblood OR opensource_audio)";

fn cover(identifier: &str) -> String {
    format!("https://archive.org/services/img/{identifier}")
}

/// A field that archive.org stores as either a string or a list of strings.
fn first_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.first().map(first_text).unwrap_or_default(),
        _ => String::new(),
    }
}

/// Recordings and releases matching a search, most downloaded first.
pub fn search(query: &str) -> Result<Value, String> {
    let words = query.replace(['(', ')', ':', '"'], " ");
    items(&format!("(title:({words}) OR creator:({words}) OR subject:({words})) AND mediatype:(audio OR etree) AND {MUSIC}"))
}

/// The most downloaded items in a collection ("collection:etree") or with a subject ("jazz").
pub fn browse(tag: &str) -> Result<Value, String> {
    match tag.strip_prefix("collection:") {
        Some(collection) => items(&format!("collection:({collection}) AND mediatype:(audio OR etree)")),
        None => {
            let words = tag.replace(['(', ')', ':', '"'], " ");
            items(&format!("subject:({words}) AND mediatype:(audio OR etree) AND {MUSIC}"))
        }
    }
}

fn items(q: &str) -> Result<Value, String> {
    sorted(q, "downloads desc", 36)
}

/// Items matching `q` in archive.org's order `sort` ("week desc" is the most
/// downloaded this week, "addeddate desc" the newest), as album cards.
fn sorted(q: &str, sort: &str, rows: usize) -> Result<Value, String> {
    let url = format!(
        "https://archive.org/advancedsearch.php?q={}&fl%5B%5D=identifier&fl%5B%5D=title&fl%5B%5D=creator&fl%5B%5D=date&rows={rows}&sort%5B%5D={}&output=json",
        encode(q),
        encode(sort)
    );
    let answer = get_json(&url)?;
    let albums: Vec<Value> = answer
        .pointer("/response/docs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|doc| {
            let id = doc["identifier"].as_str()?;
            Some(json!({
                "id": format!("ia:{id}"),
                "title": first_text(&doc["title"]),
                "artist": first_text(&doc["creator"]),
                "artistId": "",
                "cover": cover(id),
                "coverBig": cover(id),
                "year": first_text(&doc["date"]).chars().take(4).collect::<String>(),
                "kind": "archive",
            }))
        })
        .collect();
    Ok(json!({ "albums": albums, "tracks": [] }))
}

/// "06:36" or "396.5" -> seconds.
fn seconds(length: &str) -> u64 {
    if length.contains(':') {
        length.split(':').fold(0, |total, part| total * 60 + part.trim().parse::<f64>().unwrap_or(0.0) as u64)
    } else {
        length.parse::<f64>().unwrap_or(0.0).round() as u64
    }
}

/// An item ("ia:<identifier>") as an album of its audio files.
pub fn album(id: &str) -> Result<Value, String> {
    let identifier = id.strip_prefix("ia:").ok_or("Unknown Internet Archive item")?;
    let item = get_json(&format!("https://archive.org/metadata/{identifier}"))?;
    let meta = &item["metadata"];
    let title = first_text(&meta["title"]);
    let creator = first_text(&meta["creator"]);
    let files = item["files"].as_array().cloned().unwrap_or_default();
    // MP3 copies play everywhere; fall back to Ogg where an item has no MP3s.
    let format = ["VBR MP3", "MP3", "128Kbps MP3", "64Kbps MP3", "Ogg Vorbis"]
        .into_iter()
        .find(|format| files.iter().any(|file| file["format"] == *format))
        .ok_or("This Internet Archive item has no audio that can play here.")?;
    let mut songs: Vec<&Value> = files.iter().filter(|file| file["format"] == format).collect();
    let number = |file: &Value| first_text(&file["track"]).split('/').next().and_then(|n| n.trim().parse::<u32>().ok());
    songs.sort_by(|a, b| {
        (number(a).unwrap_or(u32::MAX), a["name"].as_str().unwrap_or("")).cmp(&(number(b).unwrap_or(u32::MAX), b["name"].as_str().unwrap_or("")))
    });
    let tracks: Vec<Value> = songs
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            let name = file["name"].as_str()?;
            let path: Vec<String> = name.split('/').map(encode).collect();
            let song_title = Some(first_text(&file["title"])).filter(|t| !t.is_empty()).unwrap_or_else(|| {
                name.rsplit('/').next().unwrap_or(name).rsplit_once('.').map(|(stem, _)| stem.to_owned()).unwrap_or_else(|| name.to_owned())
            });
            Some(json!({
                "key": format!("ia:{identifier}/{name}"),
                "service": "archive",
                "streamUrl": format!("https://archive.org/download/{identifier}/{}", path.join("/")),
                "pageUrl": format!("https://archive.org/details/{identifier}"),
                "title": song_title,
                "artist": Some(first_text(&file["creator"])).filter(|c| !c.is_empty()).unwrap_or_else(|| creator.clone()),
                "artistId": "",
                "album": title,
                "albumId": id,
                "cover": cover(identifier),
                "coverBig": cover(identifier),
                "duration": seconds(&first_text(&file["length"])),
                "trackNumber": index + 1,
            }))
        })
        .collect();
    let total: u64 = tracks.iter().filter_map(|t| t["duration"].as_u64()).sum();
    let date = first_text(&meta["date"]);
    Ok(json!({
        "id": id, "title": title, "artist": creator, "artistId": "",
        "cover": cover(identifier), "coverBig": cover(identifier),
        "year": date.chars().take(4).collect::<String>(), "kind": "archive",
        "trackCount": tracks.len(), "duration": total,
        "label": first_text(&meta["venue"]).trim().to_owned(),
        "releaseDate": date, "genres": [], "tracks": tracks,
        "source": "archive", "pageUrl": format!("https://archive.org/details/{identifier}"),
    }))
}

/// Songs like one (for Random radio): random tracks from other well-loved
/// recordings by the same artist.
pub fn related(track: &Value) -> Result<Vec<Value>, String> {
    let identifier = track["key"].as_str().and_then(|key| key.strip_prefix("ia:")).and_then(|rest| rest.split('/').next()).unwrap_or("");
    let artist = track["artist"].as_str().unwrap_or("").replace(['(', ')', ':', '"'], " ");
    if artist.trim().is_empty() {
        return Ok(Vec::new());
    }
    let found = items(&format!("creator:(\"{}\") AND mediatype:(audio OR etree) AND NOT identifier:({identifier})", artist.trim()))?;
    let mut others: Vec<String> = found["albums"].as_array().map(|a| a.iter().filter_map(|x| x["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    crate::random::shuffle(&mut others);
    let mut songs = Vec::new();
    for id in others.iter().take(2) {
        let Ok(item) = album(id) else { continue };
        let mut tracks = item["tracks"].as_array().cloned().unwrap_or_default();
        crate::random::shuffle(&mut tracks);
        songs.extend(tracks.into_iter().take(6));
    }
    crate::random::shuffle(&mut songs);
    Ok(songs)
}

/// An archive.org item link (".../details/<identifier>"), imported as a playlist.
pub fn import(url: &str) -> Result<Value, String> {
    let identifier = url
        .split("/details/")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .filter(|id| !id.is_empty())
        .ok_or("Paste a link to an archive.org item (it contains \"/details/\").")?;
    let album = album(&format!("ia:{identifier}"))?;
    Ok(json!({ "title": album["title"], "tracks": album["tracks"] }))
}

/// Today's (month, day), in UTC.
fn today() -> (u32, u32) {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0) as i64;
    month_day(days)
}

/// The (month, day) of a day counted from 1970-01-01 (Howard Hinnant's civil_from_days).
fn month_day(days: i64) -> (u32, u32) {
    let z = days + 719_468;
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted + 2) / 5 + 1) as u32;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 } as u32;
    (month, day)
}

/// Concerts in the Live Music Archive played on this day of the year, in the
/// forty years when most of its tapes were made (archive.org takes no more
/// dates than that in one search).
fn on_this_day(month: u32, day: u32) -> String {
    let leap = |year: i32| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days: Vec<String> = (1966..2006)
        .filter(|year| month != 2 || day != 29 || leap(*year))
        .map(|year| format!("date:[{year}-{month:02}-{day:02} TO {year}-{month:02}-{day:02}]"))
        .collect();
    format!("collection:etree AND mediatype:etree AND ({})", days.join(" OR "))
}

/// The Internet Archive's music front page: concerts played on this day,
/// what's popular this week and newly added at the Live Music Archive, the
/// Grateful Dead's most played shows, 78 rpm records and netlabel releases.
pub fn featured() -> Result<Vec<Value>, String> {
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let (month, day) = today();
    let lma = "collection:etree AND mediatype:etree";
    let wanted: [(&str, &str, String, String, &str); 7] = [
        ("on-this-day", "On this day", format!("Concerts played on {} {day}, from the Live Music Archive.", MONTHS[month as usize - 1]), on_this_day(month, day), "downloads desc"),
        ("live-week", "Popular this week", "The Live Music Archive's most played concert recordings this week.".into(), format!("{lma} AND NOT collection:(GratefulDead)"), "week desc"),
        ("grateful-dead", "Grateful Dead", "The Dead's most played shows this week.".into(), "collection:(GratefulDead) AND mediatype:etree".into(), "week desc"),
        ("live-new", "Just added", "Fresh tapes at the Live Music Archive, newest first.".into(), lma.into(), "addeddate desc"),
        ("78rpm", "78s this week", "Records from the Great 78 Project people are playing this week.".into(), "collection:(georgeblood) AND mediatype:audio".into(), "week desc"),
        ("netlabels-new", "New on netlabels", "Freely shared releases from netlabels, newest first.".into(), "collection:(netlabels) AND mediatype:audio".into(), "addeddate desc"),
        ("netlabels-week", "Netlabel favourites", "The netlabel releases people are playing this week.".into(), "collection:(netlabels) AND mediatype:audio".into(), "week desc"),
    ];
    let answers: Vec<Option<Value>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = wanted.iter().map(|(_, _, _, q, sort)| scope.spawn(move || sorted(q, sort, 30).ok())).collect();
        jobs.into_iter().map(|job| job.join().ok().flatten()).collect()
    });
    Ok(wanted
        .iter()
        .zip(answers)
        .filter_map(|((id, title, about, _, _), answer)| {
            let albums = answer.and_then(|a| a["albums"].as_array().cloned()).unwrap_or_default();
            crate::featured::section(id, title, about, albums, Vec::new())
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dates() {
        assert_eq!(month_day(0), (1, 1));
        assert_eq!(month_day(20_398), (11, 6)); // 2025-11-06
        assert_eq!(month_day(11_016), (2, 29)); // 2000-02-29
        assert!(on_this_day(2, 29).matches(" OR ").count() < 15);
    }

    /// Talks to archive.org: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_featured() {
        let sections = featured().unwrap();
        let ids: Vec<&str> = sections.iter().filter_map(|s| s["id"].as_str()).collect();
        assert_eq!(ids, ["on-this-day", "live-week", "grateful-dead", "live-new", "78rpm", "netlabels-new", "netlabels-week"]);
        assert!(sections.iter().all(|s| s["albums"].as_array().unwrap().len() >= 10));
        let week = sections[1]["albums"].as_array().unwrap();
        assert!(week.iter().all(|a| a["artist"] != "Grateful Dead"), "the Dead have their own shelf");
    }

    #[test]
    fn reads_lengths() {
        assert_eq!(seconds("06:36"), 396);
        assert_eq!(seconds("1:02:03"), 3723);
        assert_eq!(seconds("396.5"), 397);
    }

    /// Talks to archive.org, Mixcloud and SoundCloud: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_browse() {
        assert!(!browse("collection:etree").unwrap()["albums"].as_array().unwrap().is_empty());
        assert!(!browse("jazz").unwrap()["albums"].as_array().unwrap().is_empty());
        assert!(!crate::mixcloud::browse("Deep House").unwrap()["tracks"].as_array().unwrap().is_empty());
        let sc = crate::soundcloud::browse("Lo-fi").unwrap();
        assert!(!sc["albums"].as_array().unwrap().is_empty() && !sc["tracks"].as_array().unwrap().is_empty());
    }

    /// Talks to archive.org: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_search_and_album() {
        let found = search("grateful dead").unwrap();
        assert!(!found["albums"].as_array().unwrap().is_empty());
        let show = album("ia:gd1977-05-08.111493.mtx.seamons.sbeok.flac16").unwrap();
        let tracks = show["tracks"].as_array().unwrap();
        assert!(tracks.len() > 10);
        assert_eq!(tracks[0]["title"], "Minglewood Blues");
        assert!(tracks[0]["streamUrl"].as_str().unwrap().ends_with(".mp3"));
    }
}
