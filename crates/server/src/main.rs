//! earshot-server: Earshot's web version on its own, for a home server or Docker.
//!
//! - `EARSHOT_LISTEN`: address to listen on (default 127.0.0.1:8790; the Docker image uses 0.0.0.0:8790)
//! - `EARSHOT_DATA_DIR`: where the library and saved stations are kept
//! - `EARSHOT_WEB_DIR`: the web interface's files (found next to the program by default)

use std::sync::{Arc, Mutex};

use earshot_core::api::Earshot;
use earshot_core::library::Library;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let address = std::env::var("EARSHOT_LISTEN").unwrap_or_else(|_| "127.0.0.1:8790".to_owned());
    let web = earshot_server::web_dir();
    let core = Arc::new(Earshot::new(Arc::new(Mutex::new(Library::load()))));
    let listener = tokio::net::TcpListener::bind(&address).await?;
    eprintln!("Earshot's web version: http://{} (interface from {})", listener.local_addr()?, web.display());
    tokio::select! {
        result = earshot_server::serve(listener, core, web) => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}
