//! YouTube search and audio streams, through the `yt-dlp` program.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use serde_json::{Value, json};

pub fn program() -> String {
    std::env::var("EARSHOT_YTDLP").unwrap_or_else(|_| "yt-dlp".to_owned())
}

pub fn available() -> bool {
    let program = program();
    if program.contains('/') {
        return std::path::Path::new(&program).is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(&program).is_file()))
        .unwrap_or(false)
}

fn run(args: &[&str]) -> Result<String, String> {
    let output = Command::new(program())
        .args(["--no-warnings", "--socket-timeout", "15", "--ignore-config"])
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "Earshot plays music through yt-dlp, which isn't installed. Install yt-dlp and restart.".to_owned(),
            _ => format!("Couldn't start yt-dlp: {error}"),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr.lines().rev().find(|line| line.contains("ERROR")).unwrap_or("yt-dlp failed");
        return Err(line.trim_start_matches("ERROR: ").to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Splits "Artist - Title (Official Video)" into its parts.
pub fn split_title(raw: &str, channel: &str) -> (String, String) {
    let mut title = raw.to_owned();
    for noise in [
        "(Official Video)", "(Official Music Video)", "(Official Audio)", "[Official Video]", "[Official Music Video]",
        "[Official Audio]", "(Official Lyric Video)", "(Lyric Video)", "(Lyrics)", "[Lyrics]", "(Audio)", "(Visualizer)",
        "(Official Visualizer)", "(HD)", "[HD]", "(Video)", "(Official HD Video)", "(Music Video)",
    ] {
        if let Some(pos) = title.to_ascii_lowercase().find(&noise.to_ascii_lowercase()) {
            title.replace_range(pos..pos + noise.len(), "");
        }
    }
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    for separator in [" -- ", " - ", " – ", " — "] {
        if let Some((artist, song)) = title.split_once(separator) {
            return (artist.trim().to_owned(), song.trim().to_owned());
        }
    }
    let channel = channel.trim_end_matches(" - Topic").trim_end_matches("VEVO").to_owned();
    (channel, title)
}

pub fn thumbnail(id: &str) -> String {
    format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")
}

fn entry_track(entry: &Value) -> Option<Value> {
    let id = entry.get("id")?.as_str()?;
    if id.len() != 11 {
        return None;
    }
    let raw_title = entry.get("title").and_then(Value::as_str).unwrap_or("");
    let channel = entry
        .get("channel")
        .or_else(|| entry.get("uploader"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let (artist, title) = split_title(raw_title, channel);
    Some(json!({
        "key": format!("yt:{id}"),
        "youtubeId": id,
        "title": title,
        "artist": artist,
        "album": "",
        "albumId": "",
        "artistId": "",
        "cover": thumbnail(id),
        "coverBig": thumbnail(id),
        "duration": entry.get("duration").and_then(Value::as_f64).unwrap_or(0.0) as u64,
        "channel": channel,
        "rawTitle": raw_title,
    }))
}

fn flat(target: &str) -> Result<Value, String> {
    let text = run(&["--flat-playlist", "-J", "--", target])?;
    serde_json::from_str(&text).map_err(|error| format!("Unexpected answer from yt-dlp: {error}"))
}

pub fn search(query: &str, count: usize) -> Result<Vec<Value>, String> {
    let found = flat(&format!("ytsearch{count}:{query}"))?;
    Ok(found
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(entry_track).collect())
        .unwrap_or_default())
}

pub fn playlist(url: &str) -> Result<Value, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Paste a YouTube playlist link that starts with https://".to_owned());
    }
    let found = flat(url)?;
    let tracks: Vec<Value> = found
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(entry_track).collect())
        .unwrap_or_default();
    if tracks.is_empty() {
        return Err("That link has no playable videos.".to_owned());
    }
    Ok(json!({
        "title": found.get("title").and_then(Value::as_str).unwrap_or("Imported playlist"),
        "tracks": tracks,
    }))
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Scores a search result against the wanted track; higher is better.
fn score(candidate: &Value, artist: &str, title: &str, duration: u64) -> i64 {
    let raw = candidate["rawTitle"].as_str().unwrap_or("").to_lowercase();
    let channel = candidate["channel"].as_str().unwrap_or("").to_lowercase();
    let mut points = 0i64;
    let wanted_title = words(title);
    let hits = wanted_title.iter().filter(|word| raw.contains(word.as_str())).count() as i64;
    points += hits * 10 - (wanted_title.len() as i64 - hits) * 15;
    let artist_lower = artist.to_lowercase();
    if !artist_lower.is_empty() && (channel.contains(&artist_lower) || raw.contains(&artist_lower)) {
        points += 25;
    }
    if channel.ends_with(" - topic") {
        points += 20;
    }
    let wanted = format!("{} {}", artist, title).to_lowercase();
    for unwanted in ["live", "cover", "karaoke", "remix", "sped up", "slowed", "8d", "reverb", "instrumental", "nightcore", "reaction", "tutorial"] {
        if raw.contains(unwanted) && !wanted.contains(unwanted) {
            points -= 40;
        }
    }
    let found = candidate["duration"].as_u64().unwrap_or(0);
    if duration > 0 && found > 0 {
        let diff = (found as i64 - duration as i64).abs();
        points -= diff.min(600) / 3;
    }
    points
}

/// Where downloaded audio is kept so songs only download once.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("EARSHOT_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("", "", "earshot")
        .map(|dirs| dirs.cache_dir().join("audio"))
        .unwrap_or_else(|| std::env::temp_dir().join("earshot-audio"))
}

const CACHE_LIMIT_BYTES: u64 = 3 * 1024 * 1024 * 1024;

fn cached_file(dir: &Path, video: &str) -> Option<PathBuf> {
    let prefix = format!("{video}.");
    std::fs::read_dir(dir).ok()?.filter_map(Result::ok).map(|entry| entry.path()).find(|path| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        name.starts_with(&prefix) && !name.ends_with(".part") && !name.ends_with(".ytdl")
    })
}

/// Removes the least recently used songs once the cache grows past its limit.
fn trim_cache(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(PathBuf, u64, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let used = meta.accessed().or_else(|_| meta.modified()).ok()?;
            meta.is_file().then(|| (entry.path(), meta.len(), used))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    files.sort_by_key(|(_, _, used)| *used);
    for (path, size, _) in files {
        if total <= CACHE_LIMIT_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

fn download_lock(video: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(Default::default).lock().unwrap().entry(video.to_owned()).or_default().clone()
}

/// Downloads a video's audio (once) and returns the local file.
fn audio_file(video: &str) -> Result<PathBuf, String> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("Couldn't create the audio cache: {error}"))?;
    let lock = download_lock(video);
    let _guard = lock.lock().unwrap();
    if let Some(path) = cached_file(&dir, video) {
        return Ok(path);
    }
    let template = dir.join("%(id)s.%(ext)s");
    let output = run(&[
        "-f",
        "bestaudio[ext=m4a]/bestaudio/best",
        "-N",
        "4",
        "--no-playlist",
        "--no-mtime",
        "-o",
        &template.to_string_lossy(),
        "--print",
        "after_move:filepath",
        "--",
        &format!("https://www.youtube.com/watch?v={video}"),
    ])?;
    let path = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| cached_file(&dir, video))
        .ok_or("yt-dlp didn't save any audio")?;
    trim_cache(&dir);
    Ok(path)
}

/// Finds the YouTube video for a track and returns a playable local audio file URL.
pub fn resolve(track: &Value, known_video: Option<String>) -> Result<(String, String), String> {
    let video = match known_video.or_else(|| track["youtubeId"].as_str().filter(|id| !id.is_empty()).map(str::to_owned)) {
        Some(video) => video,
        None => {
            let artist = track["artist"].as_str().unwrap_or("");
            let title = track["title"].as_str().unwrap_or("");
            let duration = track["duration"].as_u64().unwrap_or(0);
            let candidates = search(&format!("{artist} - {title}"), 6)?;
            candidates
                .iter()
                .max_by_key(|candidate| score(candidate, artist, title, duration))
                .and_then(|best| best["youtubeId"].as_str().map(str::to_owned))
                .ok_or_else(|| format!("Couldn't find \"{title}\" on YouTube."))?
        }
    };
    // YouTube's stream links only work for yt-dlp's own downloader now, so
    // fetch the audio into the cache and play the file.
    let file = audio_file(&video)?;
    Ok((video, format!("file://{}", file.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_titles() {
        assert_eq!(split_title("Daft Punk - Around The World (Official Music Video)", "Daft Punk"), ("Daft Punk".into(), "Around The World".into()));
        assert_eq!(split_title("Harder Better", "Daft Punk - Topic"), ("Daft Punk".into(), "Harder Better".into()));
    }

    #[test]
    fn prefers_matching_studio_versions() {
        let studio = json!({"rawTitle": "Daft Punk - Around the World", "channel": "Daft Punk - Topic", "duration": 430});
        let live = json!({"rawTitle": "Daft Punk - Around the World (Live 2007)", "channel": "fan", "duration": 600});
        assert!(score(&studio, "Daft Punk", "Around the World", 429) > score(&live, "Daft Punk", "Around the World", 429));
    }
}
