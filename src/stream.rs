//! Play songs while they download. yt-dlp writes the audio to a file in the
//! cache, and a tiny HTTP server on 127.0.0.1 hands that file to the player
//! as it grows, so playback starts after the first few seconds of audio
//! instead of after the whole song. The finished file stays in the cache.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::youtube;

/// One song being downloaded.
pub struct Download {
    partial: PathBuf,
    done: AtomicBool,
    error: Mutex<Option<String>>,
}

impl Download {
    fn finished(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    fn failure(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    fn bytes(&self) -> u64 {
        std::fs::metadata(&self.partial).map(|meta| meta.len()).unwrap_or(0)
    }
}

pub enum Source {
    /// Already in the cache: play the file directly (seekable).
    File(PathBuf),
    /// Downloading: play from the local stream.
    Stream(String, Arc<Download>),
}

fn downloads() -> &'static Mutex<HashMap<String, Arc<Download>>> {
    static DOWNLOADS: OnceLock<Mutex<HashMap<String, Arc<Download>>>> = OnceLock::new();
    DOWNLOADS.get_or_init(Default::default)
}

/// The cached file for a video, or a stream of its download (started if needed).
pub fn source(video: &str) -> Result<Source, String> {
    let cache = youtube::cache_dir();
    if let Some(path) = youtube::cached_file(&cache, video) {
        return Ok(Source::File(path));
    }
    let mut map = downloads().lock().unwrap();
    if let Some(existing) = map.get(video) {
        if existing.failure().is_none() {
            return Ok(Source::Stream(stream_url(video)?, existing.clone()));
        }
    }
    let partial_dir = cache.join("partial");
    std::fs::create_dir_all(&partial_dir).map_err(|error| format!("Couldn't create the audio cache: {error}"))?;
    let partial = partial_dir.join(format!("{video}.audio"));
    let _ = std::fs::remove_file(&partial);
    let download = Arc::new(Download { partial: partial.clone(), done: AtomicBool::new(false), error: Mutex::new(None) });
    map.insert(video.to_owned(), download.clone());
    drop(map);

    let worker = download.clone();
    let video_id = video.to_owned();
    std::thread::spawn(move || {
        let result = fetch(&video_id, &worker.partial, &cache);
        if let Err(message) = result {
            *worker.error.lock().unwrap() = Some(message);
            let _ = std::fs::remove_file(&worker.partial);
        }
        worker.done.store(true, Ordering::Release);
        downloads().lock().unwrap().remove(&video_id);
    });
    Ok(Source::Stream(stream_url(video)?, download))
}

/// Runs yt-dlp for one video, then moves the finished audio into the cache.
fn fetch(video: &str, partial: &PathBuf, cache: &PathBuf) -> Result<(), String> {
    let output = Command::new(youtube::program())
        .args(["--no-warnings", "--socket-timeout", "15", "--ignore-config"])
        .args(["-f", "bestaudio[ext=m4a]/bestaudio/best", "--no-part", "--no-mtime", "--no-playlist"])
        .args(["--print", "after_move:%(ext)s", "-o"])
        .arg(partial)
        .arg("--")
        .arg(format!("https://www.youtube.com/watch?v={video}"))
        .output()
        .map_err(|error| format!("Couldn't start yt-dlp: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr.lines().rev().find(|line| line.contains("ERROR")).unwrap_or("yt-dlp couldn't download the audio");
        return Err(line.trim_start_matches("ERROR: ").to_owned());
    }
    let ext = String::from_utf8_lossy(&output.stdout).lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("m4a").to_owned();
    std::fs::rename(partial, cache.join(format!("{video}.{ext}"))).map_err(|error| format!("Couldn't keep the downloaded audio: {error}"))?;
    youtube::trim_cache(cache);
    Ok(())
}

/// Waits until enough audio has arrived to start playing smoothly.
pub fn wait_ready(download: &Download, timeout: Duration) -> Result<(), String> {
    const START_BYTES: u64 = 256 * 1024;
    let started = Instant::now();
    loop {
        if let Some(message) = download.failure() {
            return Err(message);
        }
        if download.finished() || download.bytes() >= START_BYTES {
            return Ok(());
        }
        if started.elapsed() > timeout {
            return Err("YouTube is taking too long to send this song.".to_owned());
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// Waits for the whole download (needed to jump into the middle of a long video).
pub fn wait_done(download: &Download) -> Result<(), String> {
    while !download.finished() {
        std::thread::sleep(Duration::from_millis(100));
    }
    download.failure().map_or(Ok(()), Err)
}

fn stream_url(video: &str) -> Result<String, String> {
    Ok(format!("http://127.0.0.1:{}/stream/{video}", server_port()?))
}

fn server_port() -> Result<u16, String> {
    static PORT: OnceLock<Result<u16, String>> = OnceLock::new();
    PORT.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| format!("Couldn't start the audio stream: {error}"))?;
        let port = listener.local_addr().map_err(|error| error.to_string())?.port();
        std::thread::spawn(move || {
            for connection in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let _ = serve(connection);
                });
            }
        });
        Ok(port)
    })
    .clone()
}

/// Answers one request for /stream/<video> by sending the file as it grows.
fn serve(mut connection: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(connection.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    let video = request.split_whitespace().nth(1).and_then(|path| path.strip_prefix("/stream/")).unwrap_or("").to_owned();
    if video.is_empty() || !video.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return connection.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    }

    let download = downloads().lock().unwrap().get(&video).cloned();
    let (mut file, download) = match download {
        Some(download) => {
            // The file appears a moment after yt-dlp starts.
            let started = Instant::now();
            loop {
                if let Ok(file) = File::open(&download.partial) {
                    break (file, Some(download));
                }
                if download.finished() || started.elapsed() > Duration::from_secs(30) {
                    // Finished meanwhile: it's in the cache now.
                    match youtube::cached_file(&youtube::cache_dir(), &video) {
                        Some(path) => break (File::open(path)?, None),
                        None => return connection.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"),
                    }
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        }
        None => match youtube::cached_file(&youtube::cache_dir(), &video) {
            Some(path) => (File::open(path)?, None),
            None => return connection.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"),
        },
    };

    connection.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n")?;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read > 0 {
            connection.write_all(&buffer[..read])?;
            continue;
        }
        match &download {
            // Caught up with the download: wait for more, or finish once it's complete.
            Some(download) if !download.finished() => std::thread::sleep(Duration::from_millis(40)),
            Some(download) if download.failure().is_some() => return Ok(()),
            Some(_) => {
                // One last read picks up anything written just before it finished.
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    return Ok(());
                }
                connection.write_all(&buffer[..read])?;
            }
            None => return Ok(()),
        }
    }
}
