//! What YouTube, SoundCloud, Mixcloud and the Internet Archive put on their
//! own front pages (charts, what's trending, new releases, curated picks), for
//! the top of each service's page. Every service answers
//! `{ sections: [{ id, title, subtitle, albums, tracks, round? }] }`; the
//! answers are kept for half an hour, since they change slowly and take
//! several requests to gather.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{archive, mixcloud, soundcloud, youtube};

const KEEP: Duration = Duration::from_secs(30 * 60);

fn cache() -> &'static Mutex<HashMap<String, (Instant, Value)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Value)>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// One section of a front page; empty ones are left out.
pub fn section(id: &str, title: &str, subtitle: &str, albums: Vec<Value>, tracks: Vec<Value>) -> Option<Value> {
    (!albums.is_empty() || !tracks.is_empty())
        .then(|| json!({ "id": id, "title": title, "subtitle": subtitle, "albums": albums, "tracks": tracks }))
}

/// The service's front page ("youtube", "soundcloud", "mixcloud" or "archive").
pub fn featured(service: &str) -> Result<Value, String> {
    if let Some((when, value)) = cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(service) {
        if when.elapsed() < KEEP {
            return Ok(value.clone());
        }
    }
    let sections = match service {
        "youtube" => youtube::featured(),
        "soundcloud" => soundcloud::featured(),
        "mixcloud" => mixcloud::featured(),
        "archive" => archive::featured(),
        other => return Err(format!("{other} has no front page")),
    }?;
    if sections.is_empty() {
        return Err("Couldn't reach this service's front page right now.".to_owned());
    }
    let value = json!({ "sections": sections });
    cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(service.to_owned(), (Instant::now(), value.clone()));
    Ok(value)
}
