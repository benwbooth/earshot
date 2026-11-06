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

use std::collections::HashMap;
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

/// Only the sites Earshot plays audio files from, so this isn't an open proxy.
fn audio_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', ':']).next().unwrap_or("");
    host.ends_with(".bcbits.com") || host == "archive.org" || host.ends_with(".archive.org")
}

/// Passes a Bandcamp or Internet Archive audio file through (with seeking),
/// since those sites don't let a browser measure their sound themselves.
async fn audio(Query(query): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    let url = query.get("url").cloned().unwrap_or_default();
    if !audio_allowed(&url) {
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

/// The whole site: the API, then the interface's files, with every other
/// path answered by index.html so links into pages work.
pub fn router(core: Arc<Earshot>, web_dir: PathBuf) -> Router {
    let (changes, _) = broadcast::channel(16);
    let sender = changes.clone();
    core.on_change(move || {
        let _ = sender.send(());
    });
    let files = ServeDir::new(&web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
    Router::new()
        .route("/api/events", get(events))
        .route("/api/levels", get(levels))
        .route("/api/audio", get(audio))
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
