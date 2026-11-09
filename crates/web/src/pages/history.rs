//! The songs you've listened to, newest first, each once: play them again,
//! find one, forget one or clear them all (qml/HistoryPage.qml).

use leptos::portal::Portal;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{Value, json};

use crate::api;
use crate::components::{Icon, PillButton, PlayButton, StatusView, TrackList};
use crate::state::{app, s};

const ROUTE: &str = "/history";

#[component]
pub fn HistoryPage() -> impl IntoView {
    let app = app();
    let player = app.player;

    // Loaded again whenever the library changes (each song played counts).
    let plays: RwSignal<Option<Vec<Value>>> = RwSignal::new(None);
    let error = RwSignal::new(String::new());
    let retry = RwSignal::new(0u32);
    Effect::new(move |_| {
        app.revision.track();
        retry.track();
        spawn_local(async move {
            let result = api::call("recentlyPlayed", json!({})).await;
            if plays.is_disposed() {
                return;
            }
            match result {
                Ok(value) => {
                    error.set(String::new());
                    plays.set(Some(value.as_array().cloned().unwrap_or_default()));
                }
                Err(message) => error.set(message),
            }
        });
    });

    let filter = RwSignal::new(String::new());
    // Newest first (as the server answers), or oldest first.
    let newest_first = RwSignal::new(true);
    let all = Signal::derive(move || {
        let mut all = plays.get().unwrap_or_default();
        if !newest_first.get() {
            all.reverse();
        }
        all
    });
    let count = Signal::derive(move || plays.with(|p| p.as_ref().map_or(0, Vec::len)));
    let shown = Signal::derive(move || {
        let wanted = filter.with(|f| f.trim().to_lowercase());
        let all = all.get();
        if wanted.is_empty() {
            return all;
        }
        all.into_iter()
            .filter(|t| format!("{} {} {}", s(t, "title"), s(t, "artist"), s(t, "album")).to_lowercase().contains(&wanted))
            .collect()
    });
    let playing_this = Signal::derive(move || player.playing_route(ROUTE));
    let confirm_clear = RwSignal::new(false);
    let clear = move || {
        confirm_clear.set(false);
        plays.set(Some(Vec::new()));
        app.change("clearHistory", json!({}));
        app.notify("History cleared".to_owned());
    };

    view! {
        <div class="page history">
            <div class="history-page-head">
                <h1 class="hero">"History"</h1>
                <Show when=move || { count.get() > 8 }>
                    <label class="playlist-filter">
                        <Icon name="search".to_owned() size=15/>
                        <input type="search" placeholder="Find in history" prop:value=move || filter.get()
                            on:input=move |event| filter.set(event_target_value(&event))/>
                    </label>
                </Show>
            </div>

            <Show when=move || { count.get() > 0 }>
                <div class="playlist-actions">
                    <PlayButton tip="Play" ours=playing_this
                        on_play=move |_| player.play_tracks(shown.get_untracked(), 0, "History".to_owned(), Some(ROUTE.to_owned()))/>
                    <PillButton text="Shuffle" icon="shuffle"
                        on_click=move |_| player.shuffle_tracks(shown.get_untracked(), "History".to_owned(), Some(ROUTE.to_owned()))/>
                    <span class="muted">{move || { let n = count.get(); if n == 1 { "1 song".to_owned() } else { format!("{n} songs") } }}</span>
                    <span class="spacer"></span>
                    <PillButton text="Clear history" icon="trash" on_click=move |_| confirm_clear.set(true)/>
                </div>
            </Show>

            <StatusView loading=Signal::derive(move || plays.with(Option::is_none) && error.get().is_empty()) error=error
                empty_icon="history"
                empty=Signal::derive(move || if plays.with(|p| p.as_ref().is_some_and(Vec::is_empty)) {
                    "Songs you listen to show up here, newest first.".to_owned()
                } else {
                    String::new()
                })
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>

            <Show when=move || { count.get() > 0 }>
                <TrackList tracks=shown context="History".to_owned() route=ROUTE.to_owned() history_mode=true
                    newest_first=Signal::from(newest_first) on_sort_played=Callback::new(move |_| newest_first.update(|n| *n = !*n))/>
            </Show>

            // Placed on the document's body, so it covers the whole window.
            <Show when=move || confirm_clear.get()>
                <Portal><div class="dialog-backdrop" on:click=move |_| confirm_clear.set(false)>
                    <div class="dialog delete-dialog" on:click=|event| event.stop_propagation()>
                        <h3>"Clear history?"</h3>
                        <p>"Every song you've listened to will be forgotten. Made for you learns your taste from it, so it starts over too."</p>
                        <div class="dialog-buttons">
                            <PillButton text="Cancel" on_click=move |_| confirm_clear.set(false)/>
                            <PillButton text="Clear" primary=true on_click=move |_| clear()/>
                        </div>
                    </div>
                </div></Portal>
            </Show>
        </div>
    }
}
