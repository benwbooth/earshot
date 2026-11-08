//! Plays YouTube videos whose owners keep them on youtube.com (no embedded
//! players) for the web version: the server opens the video's watch page in a
//! headless Chromium of its own, takes the sound from the page's <video>
//! element with WebAudio, and streams it to the browser as MP3
//! (`GET /api/relay?v=…&t=…`). The page itself never plays out loud: the
//! browser's sound goes to a fake output (and is muted as well).
//!
//! - One tab per video being listened to, at most [`MAX_TABS`]. A tab stays a
//!   minute after its listener goes away (paused), so resuming or seeking
//!   (a new request with `t`) carries on at once; then it closes, and the
//!   browser closes a couple of minutes after its last tab (or with the
//!   server). A seek to a part the page hasn't loaded opens the page afresh
//!   there, since YouTube's player stalls on those.
//! - The stream starts within a few hundredths of a second of `t`, so the
//!   listener's position is `t` plus what their player has played.
//! - Ads never reach the listener. Like uBlock Origin's YouTube filters, the
//!   player's data has its ad schedule taken out before the player reads it
//!   (`"adPlacements"`/`"adSlots"` renamed in YouTube's answers, and the
//!   same keys pruned from what the page parses). Should an ad play anyway,
//!   nothing is sent while it shows (the listener's player waits, as when
//!   loading), and it's skipped with a real click as soon as YouTube offers.
//! - The browser is driven over the DevTools protocol through a pipe
//!   (`--remote-debugging-pipe`), so no port is opened.
//! - Which browser: `EARSHOT_CHROMIUM`, or chromium / google-chrome on PATH.
//!   Without one (or without LAME, the MP3 encoder) the relay isn't offered
//!   (requests get 503).
//! - What happens is logged ("earshot: relaying …"); `EARSHOT_RELAY_DEBUG=1`
//!   logs the page's state at every check as well.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use base64::Engine as _;
use crate::mp3::Encoder;
use serde_json::{Value, json};

/// Videos relayed at once (each is a browser tab, a few hundred MB).
pub const MAX_TABS: usize = 2;
/// How long a tab waits, paused, for its listener to come back.
const IDLE_TAB: Duration = Duration::from_secs(60);
/// How long the browser stays open after its last tab closes.
const IDLE_BROWSER: Duration = Duration::from_secs(120);
/// The page's sound: 44.1 kHz stereo, sent as 16-bit samples.
const RATE: u32 = 44_100;
/// MP3 bytes waiting for the listener (a message is about 0.1 s of sound).
pub const BACKLOG: usize = 256;

pub type Body = tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>;

/// Whether this server can relay: it needs a browser and an MP3 encoder.
pub fn available() -> Result<(), &'static str> {
    if !crate::mp3::AVAILABLE {
        return Err("This build of Earshot's server has no MP3 encoder to relay videos with.");
    }
    if chromium().is_none() {
        return Err("There's no Chromium on Earshot's server to play this video with.");
    }
    Ok(())
}

/// The browser program, if there is one.
pub fn chromium() -> Option<PathBuf> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("EARSHOT_CHROMIUM").filter(|p| !p.is_empty()) {
                return Some(PathBuf::from(path));
            }
            let paths = std::env::var_os("PATH")?;
            for dir in std::env::split_paths(&paths) {
                for name in ["chromium", "chromium-browser", "google-chrome-stable", "google-chrome"] {
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
            None
        })
        .clone()
}

// The browser and the DevTools protocol

enum Msg {
    Event(Value),
    Attach(Listener),
    Close,
}

struct Browser {
    child: Mutex<Child>,
    to: Mutex<std::io::PipeWriter>,
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, Sender<Result<Value, String>>>>,
    sessions: Mutex<HashMap<String, Sender<Msg>>>,
    alive: AtomicBool,
}

impl Browser {
    fn start() -> Result<Arc<Browser>, String> {
        let program = chromium().ok_or("No Chromium on this server to play it with")?;
        let profile = earshot_core::library::data_dir().unwrap_or_else(std::env::temp_dir).join("youtube-browser");
        let root = is_root();
        // The profile keeps youtube.com's cookies (its consent answer); if
        // it's in use (another Earshot on this computer) a fresh one will do.
        let fresh = std::env::temp_dir().join(format!("earshot-youtube-{}", std::process::id()));
        let mut last = String::new();
        for (dir, sandbox) in [(&profile, !root), (&profile, false), (&fresh, false)] {
            let _ = std::fs::create_dir_all(dir);
            match Self::launch(&program, dir, sandbox) {
                Ok(browser) => match browser.call(None, "Browser.getVersion", json!({})) {
                    Ok(_) => return Ok(browser),
                    Err(error) => {
                        last = error;
                        browser.close();
                    }
                },
                Err(error) => last = error,
            }
        }
        Err(format!("Chromium didn't start ({last})"))
    }

    #[cfg(unix)]
    fn launch(program: &std::path::Path, profile: &std::path::Path, sandbox: bool) -> Result<Arc<Browser>, String> {
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;
        let (to_read, to_write) = std::io::pipe().map_err(|e| e.to_string())?;
        let (from_read, from_write) = std::io::pipe().map_err(|e| e.to_string())?;
        let mut command = Command::new(program);
        command
            .args([
                "--headless=new",
                "--remote-debugging-pipe",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-gpu",
                "--disable-dev-shm-usage",
                "--disable-extensions",
                "--disable-sync",
                // Never out loud: a fake sound output, muted too.
                "--disable-audio-output",
                "--mute-audio",
                "--autoplay-policy=no-user-gesture-required",
                "--disable-background-media-suspend",
                "--disable-renderer-backgrounding",
                "--disable-background-timer-throttling",
                "--disable-backgrounding-occluded-windows",
                "--window-size=1280,800",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .args(if sandbox { &[][..] } else { &["--no-sandbox"][..] })
            .arg("about:blank")
            // No sound server either, whatever the flags do.
            .env("PULSE_SERVER", "unix:/nonexistent")
            .env_remove("PIPEWIRE_REMOTE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let (child_in, child_out) = (to_read.as_raw_fd(), from_write.as_raw_fd());
        // The protocol pipe is the child's fds 3 (commands) and 4 (answers).
        unsafe {
            command.pre_exec(move || {
                let a = libc::fcntl(child_in, libc::F_DUPFD_CLOEXEC, 10);
                let b = libc::fcntl(child_out, libc::F_DUPFD_CLOEXEC, 10);
                if a < 0 || b < 0 || libc::dup2(a, 3) < 0 || libc::dup2(b, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().map_err(|e| format!("{}: {e}", program.display()))?;
        drop(to_read);
        drop(from_write);
        let browser = Arc::new(Browser {
            child: Mutex::new(child),
            to: Mutex::new(to_write),
            next: AtomicU64::new(1),
            waiting: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
        });
        let reader = browser.clone();
        std::thread::Builder::new().name("earshot-relay-browser".into()).spawn(move || reader.read(from_read)).map_err(|e| e.to_string())?;
        Ok(browser)
    }

    #[cfg(not(unix))]
    fn launch(_: &std::path::Path, _: &std::path::Path, _: bool) -> Result<Arc<Browser>, String> {
        Err("not on this system".into())
    }

    /// Answers and events from the browser, one JSON message per NUL.
    fn read(self: Arc<Self>, from: std::io::PipeReader) {
        let mut from = BufReader::with_capacity(1 << 20, from);
        let mut message = Vec::new();
        loop {
            message.clear();
            match from.read_until(0, &mut message) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if message.last() == Some(&0) {
                message.pop();
            }
            let Ok(value) = serde_json::from_slice::<Value>(&message) else { continue };
            if let Some(id) = value["id"].as_u64() {
                if let Some(waiter) = self.waiting.lock().unwrap().remove(&id) {
                    let answer = match value.get("error") {
                        Some(error) => Err(error["message"].as_str().unwrap_or("DevTools error").to_owned()),
                        None => Ok(value["result"].clone()),
                    };
                    let _ = waiter.send(answer);
                }
                continue;
            }
            let method = value["method"].as_str().unwrap_or("");
            // A tab's session ends (closed, or crashed).
            let session = if method == "Target.detachedFromTarget" { value["params"]["sessionId"].as_str() } else { value["sessionId"].as_str() };
            let Some(session) = session.map(str::to_owned) else { continue };
            if method == "Fetch.requestPaused" {
                // Answered on the side: the tab may be waiting on this very page.
                let browser = self.clone();
                std::thread::spawn(move || browser.without_ads(&session, &value["params"]));
                continue;
            }
            let sender = self.sessions.lock().unwrap().get(&session).cloned();
            if let Some(sender) = sender {
                let gone = method == "Target.detachedFromTarget" || method == "Inspector.targetCrashed";
                let _ = sender.send(if gone { Msg::Close } else { Msg::Event(value) });
            }
        }
        self.alive.store(false, Ordering::SeqCst);
        self.waiting.lock().unwrap().clear();
        self.sessions.lock().unwrap().clear();
    }

    fn call(&self, session: Option<&str>, method: &str, params: Value) -> Result<Value, String> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err("Chromium closed".into());
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        let (tx, rx) = mpsc::channel();
        self.waiting.lock().unwrap().insert(id, tx);
        let mut bytes = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
        bytes.push(0);
        if let Err(error) = self.to.lock().unwrap().write_all(&bytes) {
            self.waiting.lock().unwrap().remove(&id);
            return Err(error.to_string());
        }
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(answer) => answer,
            Err(RecvTimeoutError::Timeout) => {
                self.waiting.lock().unwrap().remove(&id);
                Err(format!("Chromium didn't answer {method}"))
            }
            Err(RecvTimeoutError::Disconnected) => Err("Chromium closed".into()),
        }
    }

    /// YouTube's answers with the player's data, with its ad schedule taken
    /// out (uBlock Origin's `replace=/"adPlacements"/"no_ads"/`).
    fn without_ads(&self, session: &str, paused: &Value) {
        let request = paused["requestId"].as_str().unwrap_or("").to_owned();
        let status = paused["responseStatusCode"].as_u64().unwrap_or(0);
        let pass = || {
            let _ = self.call(Some(session), "Fetch.continueRequest", json!({ "requestId": request }));
        };
        if !(200..300).contains(&status) {
            return pass();
        }
        let Ok(body) = self.call(Some(session), "Fetch.getResponseBody", json!({ "requestId": request })) else { return pass() };
        let text = if body["base64Encoded"].as_bool().unwrap_or(false) {
            match base64::engine::general_purpose::STANDARD.decode(body["body"].as_str().unwrap_or("")).ok().and_then(|b| String::from_utf8(b).ok()) {
                Some(text) => text,
                None => return pass(),
            }
        } else {
            body["body"].as_str().unwrap_or("").to_owned()
        };
        let cleaned = strip_ads(&text);
        if cleaned == text {
            return pass();
        }
        // The body is sent whole and plain now: its old length and encoding no longer apply.
        let headers: Vec<Value> = paused["responseHeaders"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|h| !matches!(h["name"].as_str().unwrap_or("").to_ascii_lowercase().as_str(), "content-length" | "content-encoding"))
            .cloned()
            .collect();
        let fulfilled = self.call(
            Some(session),
            "Fetch.fulfillRequest",
            json!({
                "requestId": request,
                "responseCode": status,
                "responseHeaders": headers,
                "body": base64::engine::general_purpose::STANDARD.encode(cleaned.as_bytes()),
            }),
        );
        if fulfilled.is_err() {
            pass();
        }
    }

    fn close(&self) {
        let _ = self.call(None, "Browser.close", json!({}));
        let mut child = self.child.lock().unwrap();
        for _ in 0..20 {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(unix)]
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}
#[cfg(not(unix))]
fn is_root() -> bool {
    false
}

/// The player's ad schedule renamed out of YouTube's answer, so the player
/// finds no ads to play.
pub fn strip_ads(text: &str) -> String {
    text.replace("\"adPlacements\"", "\"no_ads\"").replace("\"adSlots\"", "\"no_ads\"").replace("\"playerAds\"", "\"no_ads\"")
}

/// Runs in the watch page before YouTube's own scripts: the same ad keys
/// pruned from whatever the page parses (uBlock Origin's `json-prune`).
const PRUNE_ADS: &str = r#"(function () {
  var keys = ['adPlacements', 'adSlots', 'playerAds'];
  function prune(o) {
    try {
      if (!o || typeof o !== 'object') return o;
      if (Array.isArray(o)) { o.forEach(prune); return o; }
      keys.forEach(function (k) { if (k in o) delete o[k]; });
      if (o.playerResponse && typeof o.playerResponse === 'object') prune(o.playerResponse);
    } catch (e) {}
    return o;
  }
  var parse = JSON.parse;
  JSON.parse = function () { return prune(parse.apply(this, arguments)); };
  var json = Response.prototype.json;
  Response.prototype.json = function () { return json.apply(this, arguments).then(prune); };
  var initial;
  try {
    Object.defineProperty(window, 'ytInitialPlayerResponse', {
      configurable: true,
      get: function () { return initial; },
      set: function (v) { initial = prune(v); }
    });
  } catch (e) {}
})();"#;

/// Asked a few times a second: answers the cookie question, keeps the player
/// unmuted and playing (or paused), taps its sound and reports its state.
/// `gen` numbers the listener; `target` is where it asked to start; `want`
/// is the video (nothing else's sound is sent, should YouTube move on).
const POLL: &str = r#"(function (gen, target, play, want) {
  var r = {};
  function label(b) { return ((b.innerText || '') + ' ' + (b.getAttribute('aria-label') || '')).toLowerCase(); }
  function answer(scope) {
    var buttons = Array.prototype.slice.call(scope.querySelectorAll('button, tp-yt-paper-button, [role=button]'));
    var b = buttons.find(function (b) { return /reject|decline|ablehnen|refuser|rechazar|rifiuta|weiger|recusar|odrzu/.test(label(b)); })
      || buttons.find(function (b) { return /accept|agree|akzeptieren|accepter|aceptar|accetta|accepteren|aceitar/.test(label(b)); });
    if (b) b.click();
    return !!b;
  }
  function shown(e) { return !!e && e.offsetParent !== null; }
  if (/^consent\./.test(location.hostname)) { r.consent = answer(document); return JSON.stringify(r); }
  var bump = document.querySelector('ytd-consent-bump-v2-lightbox');
  if (shown(bump)) { answer(bump); r.consent = true; }
  var still = document.querySelector('yt-confirm-dialog-renderer #confirm-button');
  if (shown(still)) still.click();
  var p = document.getElementById('movie_player');
  var error = document.querySelector('.ytp-error-content-wrap-reason, .ytp-error');
  if (shown(error) && error.innerText.trim()) { r.error = error.innerText.trim().split('\n')[0]; return JSON.stringify(r); }
  if (!p || !p.getPlayerState) { r.ready = false; return JSON.stringify(r); }
  var playability = p.getPlayerResponse && p.getPlayerResponse() && p.getPlayerResponse().playabilityStatus;
  if (playability && /^(ERROR|UNPLAYABLE|LOGIN_REQUIRED|AGE_CHECK_REQUIRED|CONTENT_CHECK_REQUIRED)$/.test(playability.status)) {
    r.error = playability.reason || playability.status; return JSON.stringify(r);
  }
  var v = p.querySelector('video.html5-main-video') || p.querySelector('video');
  // An ad (and which sign of one): the player says so, or its element plays
  // something of another length or at another time.
  window.__earshotAd = window.__earshotAd || function (p, v) {
    if (p.classList.contains('ad-showing') || p.classList.contains('ad-interrupting')) return 'the player shows an ad';
    if (document.querySelector('.ytp-ad-player-overlay, .ytp-ad-player-overlay-layout, .ytp-ad-preview-container')) return 'an ad overlay is up';
    var d = p.getDuration && p.getDuration();
    if (v && isFinite(v.duration) && d > 0 && Math.abs(v.duration - d) > 3) return 'the video element is ' + Math.round(v.duration) + ' s long, not ' + Math.round(d);
    if (v && p.getCurrentTime && Math.abs(v.currentTime - p.getCurrentTime()) > 3) return 'the video element is at ' + Math.round(v.currentTime) + ' s, not ' + Math.round(p.getCurrentTime());
    return '';
  };
  var isAd = window.__earshotAd;
  var E = window.__earshot;
  if (v && (!E || E.v !== v)) {
    try {
      if (E && E.ctx) E.ctx.close();
      var ctx = new AudioContext({ sampleRate: 44100 });
      var tap = ctx.createScriptProcessor(4096, 2, 2);
      ctx.createMediaElementSource(v).connect(tap);
      tap.connect(ctx.destination); // its output stays silent
      E = window.__earshot = { v: v, ctx: ctx, tap: tap, gen: 0, target: 0, sending: false, held: null, quietUntil: 0, seekAt: 0 };
      tap.onaudioprocess = function (e) {
        var E = window.__earshot;
        if (!E || E.v !== v) return;
        var p = document.getElementById('movie_player');
        var data = p && p.getVideoData && p.getVideoData();
        var ad = !p || isAd(p, v) || !data || data.video_id !== E.want;
        if (ad) E.quietUntil = Date.now() + 600;
        var ok = E.gen > 0 && !ad && !v.paused && !v.seeking && !v.muted && v.readyState >= 3 && Date.now() >= E.quietUntil;
        // The stream starts right where the listener asked (it's re-sought otherwise).
        if (ok && !E.sending) { ok = Math.abs(v.currentTime - E.target) < 0.4; if (ok) E.sending = true; }
        // Held back one buffer, so a buffer just before an ad is dropped too.
        if (!ok) { E.held = null; return; }
        var left = e.inputBuffer.getChannelData(0);
        var right = e.inputBuffer.numberOfChannels > 1 ? e.inputBuffer.getChannelData(1) : left;
        var out = new Int16Array(left.length * 2);
        for (var i = 0; i < left.length; i++) {
          out[2 * i] = Math.max(-1, Math.min(1, left[i])) * 32767;
          out[2 * i + 1] = Math.max(-1, Math.min(1, right[i])) * 32767;
        }
        var bytes = new Uint8Array(out.buffer), text = '';
        for (var j = 0; j < bytes.length; j += 0x8000) text += String.fromCharCode.apply(null, bytes.subarray(j, j + 0x8000));
        if (E.held) earshotRelay(E.held);
        E.held = E.gen + ' ' + btoa(text);
      };
    } catch (e) { r.error = 'Earshot couldn\'t take the page\'s sound: ' + e; return JSON.stringify(r); }
  }
  var why = isAd(p, v);
  var ad = !!why;
  r.ad = why;
  if (E) E.want = want;
  if (E && E.gen !== gen) { E.gen = gen; E.target = target; E.sending = false; E.held = null; E.seekAt = 0; }
  if (ad) {
    var skip = Array.prototype.slice.call(document.querySelectorAll('.ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-ad-skip-button-container button')).find(shown);
    if (skip) { var b = skip.getBoundingClientRect(); if (b.width > 0) r.skip = [b.left + b.width / 2, b.top + b.height / 2]; }
  }
  if (p.isMuted && p.isMuted()) p.unMute();
  if (p.getVolume && p.getVolume() < 100) p.setVolume(100);
  if (v) { v.muted = false; v.volume = 1; }
  var state = p.getPlayerState();
  // To where the listener is, until the sound from there is on its way:
  // a seek when the player has that part, otherwise the page has to be
  // opened there afresh (`far`).
  if (play && E && !E.sending && !ad && state !== -1 && Math.abs(p.getCurrentTime() - E.target) > 0.4 && Date.now() - E.seekAt > 2000) {
    var have = false;
    for (var i = 0; i < v.buffered.length; i++) if (v.buffered.start(i) - 0.5 <= E.target && E.target < v.buffered.end(i) - 1) have = true;
    if (have) { p.seekTo(E.target, true); E.seekAt = Date.now(); } else r.far = true;
  }
  if (play && (state === 2 || state === 5 || state === -1)) p.playVideo();
  if (!play && (state === 1 || state === 3)) p.pauseVideo();
  if (E && E.ctx.state !== 'running') E.ctx.resume();
  var data = p.getVideoData && p.getVideoData();
  r.ready = true; r.state = state; r.time = p.getCurrentTime(); r.duration = p.getDuration();
  r.video = data && data.video_id; r.sending = !!(E && E.sending);
  return JSON.stringify(r);
})"#;

// Relays

/// Someone listening: the MP3 stream going to them.
pub struct Listener {
    t: f64,
    /// The page's number for this stream (`n`): the same number again is the
    /// browser asking for the same stream anew (Safari does, more than once).
    request: u64,
    body: Body,
    /// Told once the first sound is on its way, or why there won't be any.
    head: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
}

struct TabHandle {
    id: u64,
    video: String,
    sender: Sender<Msg>,
    attached: Mutex<Instant>,
}

#[derive(Default)]
struct Relays {
    browser: Option<Arc<Browser>>,
    tabs: Vec<Arc<TabHandle>>,
}

fn relays() -> &'static Mutex<Relays> {
    static RELAYS: OnceLock<Mutex<Relays>> = OnceLock::new();
    RELAYS.get_or_init(Default::default)
}

/// Starts relaying `video` from `t` seconds into `body`; `head` hears when
/// the sound starts or why it won't. Blocks while a tab opens.
pub fn listen(video: &str, t: f64, request: u64, body: Body, head: tokio::sync::oneshot::Sender<Result<(), String>>) -> Result<(), String> {
    let listener = Listener { t, request, body, head: Some(head) };
    let mut relays = relays().lock().unwrap();
    relays.tabs.retain(|tab| tab.sender.send(Msg::Event(Value::Null)).is_ok());
    // The video's tab already open (resuming, seeking): carry on there.
    if let Some(tab) = relays.tabs.iter().find(|tab| tab.video == video) {
        *tab.attached.lock().unwrap() = Instant::now();
        return tab.sender.send(Msg::Attach(listener)).map_err(|_| "The relay closed".to_owned());
    }
    // Room for another: the tab listened to longest ago makes way.
    while relays.tabs.len() >= MAX_TABS {
        let oldest = relays.tabs.iter().enumerate().min_by_key(|(_, tab)| *tab.attached.lock().unwrap()).map(|(i, _)| i).unwrap_or(0);
        let _ = relays.tabs.remove(oldest).sender.send(Msg::Close);
    }
    let browser = match relays.browser.clone().filter(|b| b.alive.load(Ordering::SeqCst)) {
        Some(browser) => browser,
        None => {
            let browser = Browser::start()?;
            relays.browser = Some(browser.clone());
            browser
        }
    };
    let (sender, receiver) = mpsc::channel();
    let tab = Tab::open(browser, video, t, sender.clone())?;
    relays.tabs.push(Arc::new(TabHandle { id: tab.id, video: video.to_owned(), sender: sender.clone(), attached: Mutex::new(Instant::now()) }));
    drop(relays);
    let _ = sender.send(Msg::Attach(listener));
    std::thread::Builder::new().name("earshot-relay-tab".into()).spawn(move || tab.run(receiver)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Closes the browser (the server is stopping).
pub fn shut_down() {
    let browser = relays().lock().unwrap().browser.take();
    if let Some(browser) = browser {
        browser.close();
    }
}

/// After a tab closes: the browser closes too once nothing's relayed for a while.
fn tab_closed(id: u64) {
    let mut open = relays().lock().unwrap();
    open.tabs.retain(|tab| tab.id != id);
    if !open.tabs.is_empty() {
        return;
    }
    let Some(browser) = open.browser.clone() else { return };
    drop(open);
    std::thread::spawn(move || {
        std::thread::sleep(IDLE_BROWSER);
        let mut open = relays().lock().unwrap();
        if open.tabs.is_empty() && open.browser.as_ref().is_some_and(|b| Arc::ptr_eq(b, &browser)) {
            open.browser = None;
            drop(open);
            browser.close();
        }
    });
}

struct Playing {
    listener: Listener,
    listener_no: u64,
    encoder: Encoder,
    since: Instant,
    /// When the deadline for the first sound is (ads push it back).
    deadline: Instant,
    last_sound: Instant,
    started: bool,
    /// MP3 the listener hasn't taken yet (they paused without hanging up).
    backlog: VecDeque<Bytes>,
}

struct Tab {
    id: u64,
    browser: Arc<Browser>,
    session: String,
    target: String,
    video: String,
    listener_no: u64,
    playing: Option<Playing>,
    idle_since: Instant,
    last_skip: Instant,
    /// An ad is showing (for the log).
    in_ad: bool,
    /// The listener the page was last (re)opened for: a page is opened
    /// afresh at most once per listener (see `poll`).
    opened_for: u64,
}

fn encoder() -> Result<Encoder, String> {
    Encoder::new(RATE, 160)
}

impl Tab {
    fn open(browser: Arc<Browser>, video: &str, t: f64, sender: Sender<Msg>) -> Result<Tab, String> {
        let target = browser.call(None, "Target.createTarget", json!({ "url": "about:blank" }))?["targetId"].as_str().unwrap_or("").to_owned();
        let attached = browser.call(None, "Target.attachToTarget", json!({ "targetId": target, "flatten": true }));
        let session = match attached {
            Ok(answer) => answer["sessionId"].as_str().unwrap_or("").to_owned(),
            Err(error) => {
                let _ = browser.call(None, "Target.closeTarget", json!({ "targetId": target }));
                return Err(error);
            }
        };
        browser.sessions.lock().unwrap().insert(session.clone(), sender);
        static TABS: AtomicU64 = AtomicU64::new(1);
        let tab = Tab {
            id: TABS.fetch_add(1, Ordering::SeqCst),
            browser,
            session,
            target,
            video: video.to_owned(),
            listener_no: 0,
            playing: None,
            idle_since: Instant::now(),
            last_skip: Instant::now() - Duration::from_secs(10),
            in_ad: false,
            // The first listener's page opens where they asked.
            opened_for: 1,
        };
        eprintln!("earshot: relaying {video}: opening youtube.com's page in Chromium");
        let ready = (|| {
            tab.cdp("Runtime.enable", json!({}))?;
            tab.cdp("Runtime.addBinding", json!({ "name": "earshotRelay" }))?;
            tab.cdp("Page.addScriptToEvaluateOnNewDocument", json!({ "source": PRUNE_ADS }))?;
            let patterns: Vec<Value> = ["*://www.youtube.com/watch?*", "*://www.youtube.com/youtubei/v1/player*", "*://www.youtube.com/youtubei/v1/get_watch*"]
                .iter()
                .map(|url| json!({ "urlPattern": url, "requestStage": "Response" }))
                .collect();
            tab.cdp("Fetch.enable", json!({ "patterns": patterns }))?;
            tab.navigate(t)
        })();
        if let Err(error) = ready {
            tab.close();
            return Err(error);
        }
        Ok(tab)
    }

    /// The video's watch page, from `t` seconds.
    fn navigate(&self, t: f64) -> Result<Value, String> {
        let from = if t >= 1.0 { format!("&t={}s", t.floor()) } else { String::new() };
        self.cdp("Page.navigate", json!({ "url": format!("https://www.youtube.com/watch?v={}{from}", self.video) }))
    }

    fn cdp(&self, method: &str, params: Value) -> Result<Value, String> {
        self.browser.call(Some(&self.session), method, params)
    }

    fn close(&self) {
        self.browser.sessions.lock().unwrap().remove(&self.session);
        let _ = self.browser.call(None, "Target.closeTarget", json!({ "targetId": self.target }));
    }

    fn run(mut self, receiver: Receiver<Msg>) {
        let poll_every = Duration::from_millis(250);
        let mut next_poll = Instant::now();
        loop {
            let wait = next_poll.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(wait) {
                Ok(Msg::Event(event)) => self.on_event(&event),
                Ok(Msg::Attach(listener)) => self.attach(listener),
                Ok(Msg::Close) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    next_poll = Instant::now() + poll_every;
                    if !self.poll() {
                        break;
                    }
                }
            }
        }
        self.stop(Err("The relay closed".into()));
        eprintln!("earshot: relaying {}: closing its tab", self.video);
        self.close();
        tab_closed(self.id);
    }

    fn attach(&mut self, mut listener: Listener) {
        // The same stream asked for again (Safari opens it more than once): it
        // carries on, live, on the new connection rather than starting over.
        if let Some(playing) = self.playing.as_mut()
            && listener.request != 0
            && playing.listener.request == listener.request
        {
            eprintln!("earshot: relaying {}: listener {} carries on on a new connection", self.video, playing.listener_no);
            if playing.started {
                if let Some(head) = listener.head.take() {
                    let _ = head.send(Ok(()));
                }
            }
            playing.listener = listener;
            return;
        }
        self.stop(Err("Another listener took over".into()));
        let encoder = match encoder() {
            Ok(encoder) => encoder,
            Err(error) => {
                let mut listener = listener;
                if let Some(head) = listener.head.take() {
                    let _ = head.send(Err(error));
                }
                return;
            }
        };
        self.listener_no += 1;
        eprintln!("earshot: relaying {}: listener {} from {:.1}s", self.video, self.listener_no, listener.t);
        let now = Instant::now();
        self.playing = Some(Playing {
            listener,
            listener_no: self.listener_no,
            encoder,
            since: now,
            deadline: now + Duration::from_secs(45),
            last_sound: now,
            started: false,
            backlog: VecDeque::new(),
        });
    }

    /// The listener's stream ends: finished (`Ok`), or broken off with why.
    fn stop(&mut self, how: Result<(), String>) {
        let Some(mut playing) = self.playing.take() else { return };
        self.idle_since = Instant::now();
        match &how {
            Ok(()) => eprintln!("earshot: relaying {}: the video ended", self.video),
            Err(why) => eprintln!("earshot: relaying {}: stopped: {why}", self.video),
        }
        match how {
            Ok(()) => {
                let mut tail = Vec::new();
                playing.encoder.flush(&mut tail);
                playing.backlog.push_back(Bytes::from(tail));
                // What's left goes out (waiting a little for a listener who's slow to take it).
                let body = playing.listener.body.clone();
                let backlog = std::mem::take(&mut playing.backlog);
                std::thread::spawn(move || {
                    for bytes in backlog {
                        if body.blocking_send(Ok(bytes)).is_err() {
                            break;
                        }
                    }
                });
            }
            Err(why) => {
                if let Some(head) = playing.listener.head.take() {
                    let _ = head.send(Err(why));
                } else {
                    // An error, not an end, so the listener's player knows to try something else.
                    let body = playing.listener.body.clone();
                    std::thread::spawn(move || body.blocking_send(Err(std::io::Error::other(why))));
                }
            }
        }
    }

    fn on_event(&mut self, event: &Value) {
        if event["method"] != "Runtime.bindingCalled" || event["params"]["name"] != "earshotRelay" {
            return;
        }
        let Some(playing) = self.playing.as_mut() else { return };
        let payload = event["params"]["payload"].as_str().unwrap_or("");
        let Some((listener_no, data)) = payload.split_once(' ') else { return };
        if listener_no.parse::<u64>().ok() != Some(playing.listener_no) {
            return;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else { return };
        let samples: Vec<i16> = bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
        let mut mp3 = Vec::new();
        if playing.encoder.encode(&samples, &mut mp3).is_err() {
            return;
        }
        playing.last_sound = Instant::now();
        if !playing.started {
            playing.started = true;
            eprintln!("earshot: relaying {}: sound on its way after {:.1}s", self.video, playing.since.elapsed().as_secs_f64());
            if let Some(head) = playing.listener.head.take() {
                let _ = head.send(Ok(()));
            }
        }
        if !mp3.is_empty() {
            playing.backlog.push_back(Bytes::from(mp3));
        }
        self.send_backlog();
    }

    /// Hands the listener what's waiting; false if they've gone.
    fn send_backlog(&mut self) -> bool {
        let Some(playing) = self.playing.as_mut() else { return true };
        while let Some(bytes) = playing.backlog.pop_front() {
            match playing.listener.body.try_send(Ok(bytes)) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(Ok(bytes))) => {
                    playing.backlog.push_front(bytes);
                    return true;
                }
                Err(_) => {
                    self.playing = None;
                    self.idle_since = Instant::now();
                    return false;
                }
            }
        }
        true
    }

    /// Checks on the page; false when the tab should close.
    fn poll(&mut self) -> bool {
        if !self.browser.alive.load(Ordering::SeqCst) {
            return false;
        }
        // The listener hung up (paused, or went elsewhere).
        if self.playing.as_ref().is_some_and(|p| p.listener.body.is_closed()) {
            self.playing = None;
            self.idle_since = Instant::now();
        }
        self.send_backlog();
        if self.playing.is_none() && self.idle_since.elapsed() > IDLE_TAB {
            return false;
        }
        // Paused while a listener stops taking sound, so none is lost.
        let wanted = self.playing.as_ref().is_some_and(|p| p.backlog.len() < 16 && p.listener.body.capacity() > BACKLOG / 4);
        let (listener_no, t) = self.playing.as_ref().map(|p| (p.listener_no, p.listener.t)).unwrap_or((self.listener_no, 0.0));
        let script = format!("({POLL})({listener_no}, {t}, {wanted}, {})", json!(self.video));
        let status = match self.cdp("Runtime.evaluate", json!({ "expression": script, "returnByValue": true })) {
            Ok(answer) => answer["result"]["value"].as_str().and_then(|text| serde_json::from_str::<Value>(text).ok()).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        };
        if std::env::var_os("EARSHOT_RELAY_DEBUG").is_some() {
            eprintln!("relay poll: {status}");
        }
        // YouTube's page won't go far from where it was opened (its player
        // stalls, then gives up), so a listener asking for a place it hasn't
        // loaded, or whose start it fails, gets the page opened afresh there
        // (once).
        let fresh = status["far"] == true || status["error"].is_string();
        if fresh && let Some(playing) = self.playing.as_ref().filter(|p| !p.started && self.opened_for != p.listener_no) {
            let (listener_no, t) = (playing.listener_no, playing.listener.t);
            self.opened_for = listener_no;
            eprintln!("earshot: relaying {}: opening youtube.com's page afresh at {t:.1}s", self.video);
            if self.navigate(t).is_ok() {
                return true;
            }
        }
        if let Some(error) = status["error"].as_str() {
            eprintln!("earshot: relaying {}: youtube.com's page says: {error}", self.video);
            self.stop(Err(format!("YouTube won't play this video here: {error}")));
            return false;
        }
        let why = status["ad"].as_str().unwrap_or("");
        let ad = !why.is_empty();
        if ad != self.in_ad && status["ready"] == true {
            self.in_ad = ad;
            if ad {
                eprintln!("earshot: relaying {}: an ad is showing ({why}); sending nothing", self.video);
            } else {
                eprintln!("earshot: relaying {}: the ad is over", self.video);
            }
        }
        let Some(playing) = self.playing.as_mut() else { return true };
        let now = Instant::now();
        if ad {
            // The wait for the first sound doesn't count ads.
            playing.deadline = playing.deadline.max(now + Duration::from_secs(30));
            playing.last_sound = now;
            let skip = status["skip"].as_array().and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
            if let Some((x, y)) = skip
                && self.last_skip.elapsed() > Duration::from_secs(1)
            {
                self.last_skip = now;
                eprintln!("earshot: relaying {}: clicking YouTube's Skip button", self.video);
                // A real click: YouTube ignores clicks made by scripts.
                for kind in ["mousePressed", "mouseReleased"] {
                    let _ = self.cdp("Input.dispatchMouseEvent", json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1 }));
                }
            }
            return true;
        }
        let (started, since, deadline, last_sound) = (playing.started, playing.since, playing.deadline, playing.last_sound);
        let state = status["state"].as_i64().unwrap_or(-1);
        let other_video = status["video"].as_str().is_some_and(|v| !v.is_empty() && v != self.video);
        if state == 0 || (other_video && started) {
            self.stop(Ok(()));
            return false;
        }
        if !started && now > deadline.min(since + Duration::from_secs(180)) {
            self.stop(Err("youtube.com's page didn't start this video".into()));
            return false;
        }
        if started && wanted && last_sound.elapsed() > Duration::from_secs(45) {
            self.stop(Err("youtube.com's page stopped playing this video".into()));
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ad_schedule_is_renamed_away() {
        let answer = r#"{"playabilityStatus":{"status":"OK"},"adPlacements":[{"x":1}],"playerAds":[],"adSlots":[]}"#;
        let cleaned = strip_ads(answer);
        assert!(!cleaned.contains("adPlacements") && !cleaned.contains("playerAds") && !cleaned.contains("adSlots"));
        assert!(cleaned.contains(r#""playabilityStatus":{"status":"OK"}"#));
    }
}
