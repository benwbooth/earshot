//! Short notes about what the player is doing (songs starting, playing and
//! failing, what the play buttons and the waveform show), written to the
//! server's log through `/api/report`, so a problem seen in someone's own
//! browser can be traced.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::player::Player;
use crate::state::window;

pub fn send(line: String) {
    spawn_local(async move {
        if let Ok(request) = gloo_net::http::Request::post("/api/report").body(line) {
            let _ = request.send().await;
        }
    });
}

fn state_name(state: i32) -> &'static str {
    match state {
        -1 => "not started",
        0 => "ended",
        1 => "playing",
        2 => "paused",
        3 => "buffering",
        5 => "ready",
        _ => "unknown",
    }
}

pub fn watch(player: Player) {
    send(format!("page opened: {}", window().location().href().unwrap_or_default()));
    let engine = player.engine;
    Effect::new(move |_| {
        let state = engine.state.get();
        send(format!("player {} ({state}): {}", state_name(state), engine.video_id.get_untracked()));
    });
    Effect::new(move |previous: Option<bool>| {
        let busy = player.busy();
        if previous != Some(busy) {
            send(format!("play buttons show {}", if busy { "pause" } else { "play" }));
        }
        busy
    });
    Effect::new(move |_| {
        if let Some((_, code, message)) = engine.failure.get() {
            send(format!("player error {code}: {message}"));
        }
    });
    // The waveform's explanation, when it says something new (its clock
    // readings aside).
    Effect::new(move |previous: Option<String>| {
        let note = player.levels_note.get();
        let gist: String = note.chars().filter(|c| !c.is_ascii_digit()).collect();
        if previous.as_deref() != Some(gist.as_str()) && !note.is_empty() {
            send(format!("waveform: {note}"));
        }
        gist
    });
    // Every 10 seconds of playing, the bars' heights, to show whether they move.
    let sample = Closure::<dyn Fn()>::new(move || {
        if engine.state.get_untracked() == 1 {
            let bars = player.levels.get_untracked().iter().map(|v| format!("{:.0}", v * 100.0)).collect::<Vec<_>>().join(",");
            send(format!("bars at {:.0}s: {bars}", engine.position.get_untracked()));
        }
    });
    let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(sample.as_ref().unchecked_ref(), 10_000);
    sample.forget();
}
