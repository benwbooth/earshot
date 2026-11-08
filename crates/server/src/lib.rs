//! Serves the web version (the WebAssembly interface in `web_dir`) and answers
//! its requests with the same core the Qt app uses:
//!
//! - `POST /api/<name>` with JSON arguments → the JSON answer, or 502 and `{ "error": … }`
//! - `GET /api/events` → a server-sent event `library` whenever playlists,
//!   liked songs, saved albums or the listening history change
//! - `GET /api/levels` → server-sent `levels` (six bar heights, bass first)
//!   measured from this computer's speakers, for browsers on this computer
//! - `GET /api/audio?url=…` → a Bandcamp or Internet Archive audio file passed
//!   through, so the browser may measure it for the waveform
//! - `GET /api/envelope?id=…` → server-sent waveform levels measured from the
//!   song's own audio (see `earshot_core::envelope`), for any device
//! - `GET /api/youtube-audio?v=…` → a YouTube video's audio, for videos whose
//!   owners don't let YouTube's player play them on other sites
//! - `GET /api/relay?v=…&t=…` → the same, played on youtube.com's own page in
//!   the server's headless Chromium and streamed live as MP3 from `t` seconds
//!   (see [`relay`]), for when YouTube won't hand the audio over directly

mod mp3;
pub mod relay;

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Json, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use earshot_core::api::Earshot;
use earshot_core::envelope;
use futures_util::Stream;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream, ReceiverStream};
use tower_http::compression::predicate::{NotForContentType, Predicate};
use tower_http::compression::{CompressionLayer, DefaultPredicate};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
struct App {
    core: Arc<Earshot>,
    changes: broadcast::Sender<()>,
}

/// Where the web interface's files are: `EARSHOT_WEB_DIR`, or next to the
/// program (`../share/earshot/web` as installed, or `web`).
pub fn web_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("EARSHOT_WEB_DIR") {
        return PathBuf::from(dir);
    }
    let exe = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)).unwrap_or_default();
    for candidate in [exe.join("../share/earshot/web"), exe.join("web")] {
        if candidate.join("index.html").exists() {
            return candidate;
        }
    }
    PathBuf::from("web")
}

/// A line from a web page about what its player is doing (songs starting,
/// playing, failing; what the waveform measures), written to the server's
/// log, so a problem seen in someone's own browser can be traced.
async fn report(headers: HeaderMap, body: String) -> StatusCode {
    let browser = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let browser = browser.rsplit(' ').take(2).collect::<Vec<_>>().join(" ");
    let line: String = body.chars().filter(|c| !c.is_control()).take(400).collect();
    eprintln!("earshot web [{browser}]: {line}");
    StatusCode::NO_CONTENT
}

async fn call(State(app): State<App>, Path(method): Path<String>, body: Option<Json<Value>>) -> (StatusCode, Json<Value>) {
    let args = body.map(|Json(args)| args).unwrap_or(Value::Null);
    let core = app.core.clone();
    match tokio::task::spawn_blocking(move || core.call(&method, &args)).await {
        Ok(Ok(value)) => (StatusCode::OK, Json(value)),
        Ok(Err(message)) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": message }))),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Earshot's server stopped answering this." }))),
    }
}

async fn events(State(app): State<App>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let changes = BroadcastStream::new(app.changes.subscribe()).map(|_| Ok(Event::default().event("library").data("changed")));
    Sse::new(changes).keep_alive(KeepAlive::default())
}

/// This computer's speakers, about 25 times a second, while someone watches.
async fn levels() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let listening = earshot_speakers::listen();
    let ticks = IntervalStream::new(tokio::time::interval(Duration::from_millis(40)));
    let levels = ticks.map(move |_| {
        let _ = &listening; // the recording stays on while this stream lives
        let text = earshot_speakers::levels().iter().map(|level| format!("{level:.3}")).collect::<Vec<_>>().join(",");
        Ok(Event::default().event("levels").data(text))
    });
    Sse::new(levels).keep_alive(KeepAlive::default())
}

/// The levels of a song (`id` is what the player plays: a YouTube video id,
/// "soundcloud:…", "mixcloud:…" or "audio:…"), measured from its own audio:
/// `frames` events ("<first frame> <levels in base64>", six bytes a frame,
/// 25 frames a second) as they're measured, then `done` ("<frames>", then why
/// they end before the song does, if they do), or `failed` with why.
async fn envelope(Query(query): Query<HashMap<String, String>>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use base64::Engine as _;
    let id = query.get("id").cloned().unwrap_or_default();
    // The song itself, so a video YouTube won't hand over can be measured from the same song elsewhere.
    let song = match query.get("title") {
        Some(title) => json!({
            "title": title,
            "artist": query.get("artist").cloned().unwrap_or_default(),
            "duration": query.get("duration").and_then(|d| d.parse::<f64>().ok()).unwrap_or(0.0),
        }),
        None => Value::Null,
    };
    let job = envelope::measure(&id, &song);
    let watching = job.watch();
    let ticks = IntervalStream::new(tokio::time::interval(Duration::from_millis(200)));
    let events = futures_util::stream::unfold((ticks, watching, job, 0usize, false, false), |(mut ticks, watching, job, sent, told, over)| async move {
        if over {
            return None;
        }
        loop {
            ticks.next().await?;
            let snapshot = job.since(sent);
            // `source`: where the audio came from, when it isn't the song's own player's.
            if !told && let Some(source) = snapshot.source {
                return Some((Ok(Event::default().event("source").data(source)), (ticks, watching, job, sent, true, false)));
            }
            let whole = snapshot.levels.len() / envelope::BANDS * envelope::BANDS;
            if whole > 0 {
                let text = format!("{} {}", sent / envelope::BANDS, base64::engine::general_purpose::STANDARD.encode(&snapshot.levels[..whole]));
                return Some((Ok(Event::default().event("frames").data(text)), (ticks, watching, job, sent + whole, told, false)));
            }
            if snapshot.done {
                let event = match snapshot.error {
                    Some(error) => Event::default().event("failed").data(error),
                    // The frame count, then why the levels end early, if they do.
                    None => Event::default().event("done").data(format!("{} {}", sent / envelope::BANDS, snapshot.cut_short.unwrap_or_default()).trim_end().to_owned()),
                };
                return Some((Ok(event), (ticks, watching, job, sent, told, true)));
            }
        }
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}

/// Passes a Bandcamp or Internet Archive audio file through (with seeking),
/// since those sites don't let a browser measure their sound themselves.
async fn audio(Query(query): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    let url = query.get("url").cloned().unwrap_or_default();
    if !envelope::file_allowed(&url) {
        return (StatusCode::FORBIDDEN, "Earshot only passes through Bandcamp and Internet Archive audio.").into_response();
    }
    let range = headers.get(header::RANGE).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let (head_tx, head_rx) = tokio::sync::oneshot::channel();
    let (body_tx, body_rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);
    tokio::task::spawn_blocking(move || {
        static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
        let agent = AGENT.get_or_init(|| ureq::Agent::config_builder().http_status_as_error(false).timeout_connect(Some(Duration::from_secs(15))).build().into());
        let mut request = agent.get(&url);
        if let Some(range) = &range {
            request = request.header("Range", range);
        }
        let response = match request.call() {
            Ok(response) => response,
            Err(error) => {
                let _ = head_tx.send(Err(error.to_string()));
                return;
            }
        };
        let kept: Vec<(header::HeaderName, String)> = [header::CONTENT_TYPE, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES]
            .into_iter()
            .filter_map(|name| response.headers().get(&name).and_then(|v| v.to_str().ok()).map(|v| (name, v.to_owned())))
            .collect();
        let _ = head_tx.send(Ok((response.status().as_u16(), kept)));
        let mut reader = response.into_body().into_reader();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if body_tx.blocking_send(Ok(Bytes::copy_from_slice(&buffer[..n]))).is_err() {
                        break; // the browser stopped listening
                    }
                }
                Err(error) => {
                    let _ = body_tx.blocking_send(Err(error));
                    break;
                }
            }
        }
    });
    match head_rx.await {
        Ok(Ok((status, kept))) => {
            let mut response = Response::builder().status(status);
            for (name, value) in kept {
                response = response.header(name, value);
            }
            response.body(Body::from_stream(ReceiverStream::new(body_rx))).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
        }
        Ok(Err(message)) => (StatusCode::BAD_GATEWAY, message).into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// Where a video's audio is, kept for two hours (YouTube's addresses last six).
/// Only an address that reaches the end will do: without a sign-in YouTube
/// sometimes gives just the first minute, and a song that stops part way is
/// worse than one found elsewhere.
fn youtube_address(video: &str, fresh: bool) -> Result<(String, u64), String> {
    static FOUND: OnceLock<std::sync::Mutex<HashMap<String, (std::time::Instant, Result<(String, u64), String>)>>> = OnceLock::new();
    // One look per video at a time: a song being warmed up (see `youtube_warm`)
    // and then asked for waits for that look rather than starting another.
    static LOOKING: OnceLock<std::sync::Mutex<HashMap<String, std::sync::Arc<std::sync::Mutex<()>>>>> = OnceLock::new();
    let turn = LOOKING.get_or_init(Default::default).lock().unwrap().entry(video.to_owned()).or_default().clone();
    let _turn = turn.lock().unwrap();
    let found = FOUND.get_or_init(Default::default);
    if !fresh && let Some((when, answer)) = found.lock().unwrap().get(video).cloned() && when.elapsed() < Duration::from_secs(7200) {
        return answer;
    }
    // YouTube often hands only the start over without a sign-in; yt-dlp
    // (when it's there) knows how to get the whole of it.
    let answer = earshot_core::youtube::whole_audio(video).or_else(|first| ytdlp_address(video).map_err(|why| {
        eprintln!("earshot: yt-dlp couldn't get {video}'s audio either: {why}");
        first
    }));
    found.lock().unwrap().insert(video.to_owned(), (std::time::Instant::now(), answer.clone()));
    answer
}

/// A video's AAC audio address and size, found by yt-dlp (`EARSHOT_YTDLP`, or
/// yt-dlp on PATH). YouTube's addresses now work only from a moment after
/// they're handed out (`available_at`), so this waits until then.
fn ytdlp_address(video: &str) -> Result<(String, u64), String> {
    let program = std::env::var_os("EARSHOT_YTDLP").filter(|p| !p.is_empty()).unwrap_or_else(|| "yt-dlp".into());
    let started = std::time::Instant::now();
    let mut child = std::process::Command::new(&program)
        .args(["-q", "--no-warnings", "--no-playlist", "-f", "140/bestaudio[ext=m4a]", "-j"])
        .arg(format!("https://www.youtube.com/watch?v={video}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("{}: {error}", std::path::Path::new(&program).display()))?;
    let mut stdout = child.stdout.take().ok_or("no output")?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut text);
        text
    });
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(40) {
            let _ = child.kill();
            return Err("yt-dlp took too long".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let text = reader.join().unwrap_or_default();
    let info: serde_json::Value = serde_json::from_str(text.trim()).map_err(|_| "yt-dlp didn't find the audio".to_owned())?;
    let url = info["url"].as_str().ok_or("yt-dlp gave no address")?.to_owned();
    let length = info["filesize"]
        .as_u64()
        .or_else(|| url.split("clen=").nth(1).and_then(|rest| rest.split('&').next()).and_then(|n| n.parse().ok()))
        .ok_or("yt-dlp gave no size")?;
    if let Some(at) = info["available_at"].as_f64() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let wait = (at - now + 0.3).clamp(0.0, 15.0);
        std::thread::sleep(Duration::from_secs_f64(wait));
    }
    eprintln!("earshot: yt-dlp found {video}'s audio in {:.1}s", started.elapsed().as_secs_f64());
    Ok((url, length))
}

/// A SoundCloud track's audio as one file, for browsers that can't start
/// SoundCloud's own player by themselves (iPhones). The pieces (a whole MP3,
/// or an HLS stream's init and segments) are fetched and joined in memory,
/// then served with byte ranges, which Safari needs for seeking and for MP4.
async fn soundcloud_audio(Query(query): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    let Some(track) = query.get("id").and_then(|id| id.parse::<u64>().ok()) else {
        return (StatusCode::BAD_REQUEST, "Which track?").into_response();
    };
    let range = headers.get(header::RANGE).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let file = match tokio::task::spawn_blocking(move || soundcloud_file(track)).await {
        Ok(Ok(file)) => file,
        Ok(Err(message)) => return (StatusCode::BAD_GATEWAY, message).into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let (mime, bytes) = (&file.0, &file.1);
    let length = bytes.len() as u64;
    let (start, end) = byte_range(range.as_deref(), length);
    if start > end || start >= length {
        return Response::builder()
            .status(416)
            .header(header::CONTENT_RANGE, format!("bytes */{length}"))
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
    }
    let mut response = Response::builder()
        .status(if range.is_some() { 206 } else { 200 })
        .header(header::CONTENT_TYPE, mime.as_str())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, (end - start + 1).to_string());
    if range.is_some() {
        response = response.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{length}"));
    }
    response
        .body(Body::from(bytes.slice(start as usize..=end as usize)))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

/// A SoundCloud track's whole audio file and its type, kept for the last few
/// tracks (a browser asks for the same file in several ranges).
fn soundcloud_file(track: u64) -> Result<std::sync::Arc<(String, Bytes)>, String> {
    static KEPT: OnceLock<std::sync::Mutex<VecDeque<(u64, std::sync::Arc<(String, Bytes)>)>>> = OnceLock::new();
    static LOOKING: OnceLock<std::sync::Mutex<HashMap<u64, std::sync::Arc<std::sync::Mutex<()>>>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    let turn = LOOKING.get_or_init(Default::default).lock().unwrap().entry(track).or_default().clone();
    let _turn = turn.lock().unwrap();
    if let Some((_, file)) = kept.lock().unwrap().iter().find(|(id, _)| *id == track) {
        return Ok(file.clone());
    }
    let (parts, kind) = earshot_core::soundcloud::audio(track)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut joined = Vec::new();
    for part in &parts {
        let mut response = agent.get(part).call().map_err(|error| format!("SoundCloud: {error}"))?;
        let piece = response.body_mut().with_config().limit(200 * 1024 * 1024).read_to_vec().map_err(|error| format!("SoundCloud: {error}"))?;
        joined.extend_from_slice(&piece);
    }
    // AAC comes as HLS's fragmented MP4, which browsers don't reliably play
    // as one file: make it a plain MP3 (decoded, then encoded with LAME).
    let joined = if kind == "mp3" {
        joined
    } else {
        let mut encoder: Option<mp3::Encoder> = None;
        let mut out = Vec::new();
        envelope::decode_stereo(joined, "mp4", |rate, samples| {
            if encoder.is_none() {
                encoder = Some(mp3::Encoder::new(rate, 192)?);
            }
            encoder.as_mut().map(|e| e.encode(samples, &mut out)).unwrap_or(Ok(()))
        })?;
        if let Some(encoder) = encoder.as_mut() {
            encoder.flush(&mut out);
        }
        out
    };
    let file = std::sync::Arc::new(("audio/mpeg".to_owned(), Bytes::from(joined)));
    let mut kept = kept.lock().unwrap();
    kept.push_back((track, file.clone()));
    while kept.len() > 4 {
        kept.pop_front();
    }
    Ok(file)
}

/// Looks up a YouTube video's audio address ahead of time (the next song in
/// the queue), so playing it later starts at once. Answers straight away.
async fn youtube_warm(Query(query): Query<HashMap<String, String>>) -> StatusCode {
    let video = query.get("v").cloned().unwrap_or_default();
    if video.is_empty() || video.len() > 20 || !video.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return StatusCode::BAD_REQUEST;
    }
    tokio::task::spawn_blocking(move || {
        let _ = youtube_address(&video, false);
    });
    StatusCode::ACCEPTED
}

/// "bytes=10-" or "bytes=10-99" as the first and last byte wanted.
fn byte_range(range: Option<&str>, length: u64) -> (u64, u64) {
    let last = length.saturating_sub(1);
    let Some((start, end)) = range.and_then(|r| r.strip_prefix("bytes=")).and_then(|r| r.split(',').next()).and_then(|r| r.split_once('-')) else {
        return (0, last);
    };
    match (start.trim().parse::<u64>(), end.trim().parse::<u64>()) {
        (Ok(start), Ok(end)) => (start, end.min(last)),
        (Ok(start), Err(_)) => (start, last),
        // "bytes=-500": the last 500 bytes.
        (Err(_), Ok(tail)) => (length.saturating_sub(tail), last),
        _ => (0, last),
    }
}

/// A YouTube video's audio straight from YouTube (with seeking), for videos
/// whose owners don't let YouTube's player play them on other sites. YouTube
/// hands it out a megabyte at a time, so the pieces are asked for in turn.
async fn youtube_audio(Query(query): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    use earshot_core::youtube::AUDIO_PIECE;
    let video = query.get("v").cloned().unwrap_or_default();
    if video.is_empty() || !video.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return (StatusCode::BAD_REQUEST, "Which video?").into_response();
    }
    let range = headers.get(header::RANGE).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let (head_tx, head_rx) = tokio::sync::oneshot::channel();
    let (body_tx, body_rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);
    tokio::task::spawn_blocking(move || {
        static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
        let agent = AGENT.get_or_init(|| {
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .timeout_connect(Some(Duration::from_secs(15)))
                .timeout_per_read(Some(Duration::from_secs(30)))
                .build()
                .into()
        });
        let piece = |url: &str, from: u64, to: u64| agent.get(&format!("{url}&range={from}-{to}")).call().map_err(|error| error.to_string());
        // The first piece decides whether YouTube will hand the audio over;
        // an address kept from earlier may have run out, so look again once.
        let mut first = None;
        for fresh in [false, true] {
            let (url, length) = match youtube_address(&video, fresh) {
                Ok(found) => found,
                Err(message) => {
                    let _ = head_tx.send(Err(message));
                    return;
                }
            };
            let (start, end) = byte_range(range.as_deref(), length);
            if start > end {
                let _ = head_tx.send(Ok((416, vec![(header::CONTENT_RANGE, format!("bytes */{length}"))])));
                return;
            }
            match piece(&url, start, (start + AUDIO_PIECE - 1).min(end)) {
                Ok(response) if response.status().is_success() => {
                    first = Some((url, length, start, end, response));
                    break;
                }
                Ok(response) if fresh => {
                    let _ = head_tx.send(Err(format!("YouTube refused this video's audio ({})", response.status())));
                    return;
                }
                Err(message) if fresh => {
                    let _ = head_tx.send(Err(message));
                    return;
                }
                _ => {}
            }
        }
        let Some((url, length, start, end, mut response)) = first else { return };
        let mut kept = vec![
            (header::CONTENT_TYPE, "audio/mp4".to_owned()),
            (header::ACCEPT_RANGES, "bytes".to_owned()),
            (header::CONTENT_LENGTH, (end - start + 1).to_string()),
        ];
        if range.is_some() {
            kept.push((header::CONTENT_RANGE, format!("bytes {start}-{end}/{length}")));
        }
        let _ = head_tx.send(Ok((if range.is_some() { 206 } else { 200 }, kept)));
        let mut at = start;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let before = at;
            let mut reader = response.into_body().into_reader();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        at += n as u64;
                        if body_tx.blocking_send(Ok(Bytes::copy_from_slice(&buffer[..n]))).is_err() {
                            return; // the browser stopped listening
                        }
                    }
                    Err(error) => {
                        let _ = body_tx.blocking_send(Err(error));
                        return;
                    }
                }
            }
            if at > end || at == before {
                return;
            }
            response = match piece(&url, at, (at + AUDIO_PIECE - 1).min(end)) {
                Ok(next) if next.status().is_success() => next,
                Ok(next) => {
                    let _ = body_tx.blocking_send(Err(std::io::Error::other(format!("YouTube stopped sending the audio ({})", next.status()))));
                    return;
                }
                Err(message) => {
                    let _ = body_tx.blocking_send(Err(std::io::Error::other(message)));
                    return;
                }
            };
        }
    });
    match head_rx.await {
        Ok(Ok((status, kept))) => {
            let mut response = Response::builder().status(status);
            for (name, value) in kept {
                response = response.header(name, value);
            }
            response.body(Body::from_stream(ReceiverStream::new(body_rx))).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
        }
        Ok(Err(message)) => (StatusCode::BAD_GATEWAY, message).into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// A video played on youtube.com's page by the server's own browser, its
/// sound streamed live as MP3 from `t` seconds in. The answer waits until
/// the sound starts (ads and loading take a while), so a video that won't
/// play is an error the page can act on. Pausing is hanging up; resuming or
/// seeking asks again with a new `t`.
async fn relay_audio(Query(query): Query<HashMap<String, String>>) -> Response {
    let video = query.get("v").cloned().unwrap_or_default();
    // A YouTube video's id, or a Mixcloud show ("mc:/user/show/").
    let youtube = !video.is_empty() && video.len() <= 20 && video.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let mixcloud = video.strip_prefix("mc:/").is_some_and(|path| {
        path.len() <= 300 && path.chars().all(|c| c.is_alphanumeric() || "-_/.%".contains(c)) && !path.contains("..")
    });
    if !youtube && !mixcloud {
        return (StatusCode::BAD_REQUEST, "Which video?").into_response();
    }
    if let Err(why) = relay::available() {
        return (StatusCode::SERVICE_UNAVAILABLE, why).into_response();
    }
    let t = query.get("t").and_then(|t| t.parse::<f64>().ok()).filter(|t| t.is_finite()).unwrap_or(0.0).clamp(0.0, 86_400.0);
    let (body_tx, body_rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(relay::BACKLOG);
    let (head_tx, head_rx) = tokio::sync::oneshot::channel();
    let request = query.get("n").and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
    let opened = tokio::task::spawn_blocking(move || relay::listen(&video, t, request, body_tx, head_tx)).await;
    match opened {
        Ok(Ok(())) => {}
        Ok(Err(message)) => return (StatusCode::BAD_GATEWAY, message).into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    match tokio::time::timeout(Duration::from_secs(200), head_rx).await {
        Ok(Ok(Ok(()))) => Response::builder()
            .header(header::CONTENT_TYPE, "audio/mpeg")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from_stream(ReceiverStream::new(body_rx)))
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response()),
        Ok(Ok(Err(message))) => (StatusCode::BAD_GATEWAY, message).into_response(),
        _ => (StatusCode::GATEWAY_TIMEOUT, "youtube.com's page didn't start this video.").into_response(),
    }
}

// The interface's files come from the Nix store, where every file is dated
// 1970, so browsers asking "changed since?" would always hear no and keep an
// old version after an update. Answer in full every time instead.
async fn ask_fresh(mut request: axum::extract::Request) -> axum::extract::Request {
    request.headers_mut().remove(header::IF_MODIFIED_SINCE);
    request.headers_mut().remove(header::IF_NONE_MATCH);
    request
}

async fn answer_fresh(mut response: Response) -> Response {
    response.headers_mut().remove(header::LAST_MODIFIED);
    response.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-cache"));
    response
}

/// Which interface this server gives out: a fingerprint of its program and
/// script files. A page left open since before an update sees it change and
/// knows to reload (a web page otherwise keeps running the old version).
fn interface_version(web_dir: &std::path::Path) -> String {
    use std::hash::{Hash, Hasher};
    fn walk(dir: &std::path::Path, hasher: &mut std::collections::hash_map::DefaultHasher) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = entries.filter_map(|entry| entry.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, hasher);
            } else if path.extension().is_some_and(|ext| ext == "wasm" || ext == "js" || ext == "css" || ext == "html") {
                path.file_name().hash(hasher);
                std::fs::read(&path).unwrap_or_default().hash(hasher);
            }
        }
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    walk(web_dir, &mut hasher);
    format!("{:016x}", hasher.finish())
}

/// The whole site: the API, then the interface's files, with every other
/// path answered by index.html so links into pages work.
pub fn router(core: Arc<Earshot>, web_dir: PathBuf) -> Router {
    let version = interface_version(&web_dir);
    let (changes, _) = broadcast::channel(16);
    let sender = changes.clone();
    core.on_change(move || {
        let _ = sender.send(());
    });
    let files = Router::new()
        .fallback_service(ServeDir::new(&web_dir).fallback(ServeFile::new(web_dir.join("index.html"))))
        .layer(axum::middleware::map_request(ask_fresh))
        .layer(axum::middleware::map_response(answer_fresh));
    Router::new()
        .route("/api/version", get(move || std::future::ready(version.clone())))
        .route("/api/report", post(report))
        .route("/api/events", get(events))
        .route("/api/levels", get(levels))
        .route("/api/audio", get(audio))
        .route("/api/envelope", get(envelope))
        .route("/api/youtube-audio", get(youtube_audio))
        .route("/api/youtube-warm", get(youtube_warm))
        .route("/api/soundcloud-audio", get(soundcloud_audio))
        .route("/api/relay", get(relay_audio))
        .route("/api/{method}", post(call))
        .fallback_service(files)
        // Audio passes through as it is, so seeking (byte ranges) keeps working.
        .layer(CompressionLayer::new().compress_when(DefaultPredicate::new().and(NotForContentType::const_new("audio/"))))
        .with_state(App { core, changes })
}

pub async fn serve(listener: tokio::net::TcpListener, core: Arc<Earshot>, web_dir: PathBuf) -> std::io::Result<()> {
    axum::serve(listener, router(core, web_dir)).await
}

/// Starts the web version on this computer only (once per run) and returns
/// its address: http://127.0.0.1:8790/, or another free port if that one's taken.
pub fn start_local(core: Arc<Earshot>) -> Result<String, String> {
    static STARTED: OnceLock<Result<String, String>> = OnceLock::new();
    STARTED
        .get_or_init(|| {
            let web = web_dir();
            if !web.join("index.html").exists() {
                return Err(format!("The web version isn't installed (looked in {}).", web.display()));
            }
            let listener = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 8790)))
                .or_else(|_| std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))))
                .map_err(|error| format!("Couldn't start the web version: {error}"))?;
            listener.set_nonblocking(true).map_err(|error| error.to_string())?;
            let address = listener.local_addr().map_err(|error| error.to_string())?;
            std::thread::Builder::new()
                .name("earshot-web".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime");
                    runtime.block_on(async move {
                        let listener = tokio::net::TcpListener::from_std(listener).expect("listener");
                        if let Err(error) = serve(listener, core, web).await {
                            eprintln!("earshot: the web version stopped: {error}");
                        }
                    });
                })
                .map_err(|error| error.to_string())?;
            Ok(format!("http://{address}/"))
        })
        .clone()
}
