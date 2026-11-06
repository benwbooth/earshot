//! Serves the web version (the WebAssembly interface in `web_dir`) and answers
//! its requests with the same core the Qt app uses:
//!
//! - `POST /api/<name>` with JSON arguments → the JSON answer, or 502 and `{ "error": … }`
//! - `GET /api/events` → a server-sent event `library` whenever playlists,
//!   liked songs, saved albums or the listening history change

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use earshot_core::api::Earshot;
use futures_util::Stream;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::compression::CompressionLayer;
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
        .route("/api/{method}", post(call))
        .fallback_service(files)
        .layer(CompressionLayer::new())
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
