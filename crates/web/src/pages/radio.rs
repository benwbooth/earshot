//! Reddit radio: pick a music subreddit and its shared songs play in order
//! (qml/RadioPage.qml).

use std::cell::RefCell;
use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use serde_json::{Value, json};

use crate::api;
use crate::components::{Icon, IconButton, PillButton, StatusView, TrackList};
use crate::state::{App, app, list, radio_href, s};

thread_local! {
    // Kept for the visit, like the Qt app's Api.lastStation and Api.radioCache.
    static LAST: RefCell<String> = RefCell::new("listentothis".to_owned());
    static CACHE: RefCell<HashMap<String, Vec<Value>>> = RefCell::new(HashMap::new());
}

/// What the page shows; every load or countdown takes a new ticket, so
/// answers and ticks from before it are ignored.
#[derive(Clone, Copy)]
struct Radio {
    station: RwSignal<String>,
    tracks: RwSignal<Option<Vec<Value>>>,
    loading: RwSignal<bool>,
    error: RwSignal<String>,
    // While Reddit asks for a pause: seconds left, and how old the saved list shown meanwhile is.
    wait_seconds: RwSignal<u64>,
    saved_minutes_ago: RwSignal<i64>,
    ticket: StoredValue<u32>,
    // A subreddit typed into the box, kept among "Your stations" once it loads.
    adding: StoredValue<String>,
    app: App,
}

impl Radio {
    fn next_ticket(self) -> u32 {
        self.ticket.update_value(|t| *t += 1);
        self.ticket.get_value()
    }
    fn current(self, ticket: u32) -> bool {
        self.ticket.try_get_value() == Some(ticket)
    }
}

/// A typed-in station that loaded with songs joins "Your stations" (the
/// core skips built-in ones and ones already kept).
fn keep_if_typed(radio: Radio, name: &str, subreddit: String) {
    if !radio.adding.with_value(|adding| adding.eq_ignore_ascii_case(name)) {
        return;
    }
    radio.adding.set_value(String::new());
    let app = radio.app;
    spawn_local(async move {
        if let Ok(Value::Bool(true)) = api::call("saveStation", json!({ "subreddit": subreddit })).await {
            app.revision.update(|r| *r += 1);
            app.notify(format!("Added r/{subreddit} to your stations"));
        }
    });
}

fn load(radio: Radio, name: String, force: bool) {
    radio.station.set(name.clone());
    LAST.with(|last| *last.borrow_mut() = name.clone());
    radio.error.set(String::new());
    radio.wait_seconds.set(0);
    radio.saved_minutes_ago.set(-1);
    if !radio.adding.with_value(|adding| adding.eq_ignore_ascii_case(&name)) {
        radio.adding.set_value(String::new());
    }
    let ticket = radio.next_ticket();
    if !force {
        if let Some(tracks) = CACHE.with(|cache| cache.borrow().get(&name).cloned()) {
            radio.loading.set(false);
            radio.tracks.set(Some(tracks));
            keep_if_typed(radio, &name, name.clone());
            return;
        }
    }
    radio.tracks.set(None);
    radio.loading.set(true);
    spawn_local(async move {
        let result = api::call("radio", json!({ "subreddit": name })).await;
        if !radio.current(ticket) {
            return;
        }
        radio.loading.set(false);
        match result {
            Ok(value) => {
                let tracks = list(&value, "tracks");
                if let Some(retry) = value["retryIn"].as_u64().filter(|n| *n > 0) {
                    // Show the saved list (if any) and try again when Reddit allows.
                    radio.tracks.set((!tracks.is_empty()).then_some(tracks));
                    radio.saved_minutes_ago.set(value["savedMinutesAgo"].as_i64().unwrap_or(-1));
                    radio.wait_seconds.set(retry);
                    countdown(radio);
                    return;
                }
                let subreddit = value["subreddit"].as_str().unwrap_or(&name).to_owned();
                keep_if_typed(radio, &name, subreddit);
                CACHE.with(|cache| cache.borrow_mut().insert(name, tracks.clone()));
                radio.tracks.set(Some(tracks));
            }
            Err(message) => radio.error.set(message),
        }
    });
}

fn countdown(radio: Radio) {
    let ticket = radio.next_ticket();
    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;
            if !radio.current(ticket) {
                return;
            }
            let left = radio.wait_seconds.get_untracked().saturating_sub(1);
            radio.wait_seconds.set(left);
            if left == 0 {
                load(radio, radio.station.get_untracked(), true);
                return;
            }
        }
    });
}

/// "Nov 5", for a saved station's playlist name.
fn today() -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let date = js_sys::Date::new_0();
    format!("{} {}", MONTHS[date.get_month() as usize % 12], date.get_date())
}

#[component]
pub fn RadioPage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let query = use_query_map();
    let wanted = Memo::new(move |_| {
        query.with(|q| q.get("station")).filter(|station| !station.is_empty()).unwrap_or_else(|| LAST.with(|last| last.borrow().clone()))
    });
    let radio = Radio {
        station: RwSignal::new(wanted.get_untracked()),
        tracks: RwSignal::new(None),
        loading: RwSignal::new(false),
        error: RwSignal::new(String::new()),
        wait_seconds: RwSignal::new(0),
        saved_minutes_ago: RwSignal::new(-1),
        ticket: StoredValue::new(0),
        adding: StoredValue::new(String::new()),
        app,
    };
    Effect::new(move |_| load(radio, wanted.get(), false));

    // "Your stations", again whenever the library changes (here or in the app).
    let saved = RwSignal::new(Vec::<Value>::new());
    Effect::new(move |_| {
        app.revision.track();
        spawn_local(async move {
            if let Ok(value) = api::call("savedStations", json!({})).await {
                saved.try_set(value.as_array().cloned().unwrap_or_default());
            }
        });
    });
    let remove = move |id: String| {
        saved.update(|list| list.retain(|item| s(item, "id") != id));
        app.change("removeStation", json!({ "subreddit": id }));
        app.notify(format!("Removed r/{id} from your stations"));
    };

    // The box for any subreddit: "r/name", "name" or a link to it.
    let typed = RwSignal::new(String::new());
    let typed_error = RwSignal::new(String::new());
    let tune = move || {
        let text = typed.get_untracked();
        spawn_local(async move {
            match api::call("radioStationName", json!({ "text": text })).await {
                Ok(value) => {
                    let name = value.as_str().unwrap_or("").to_owned();
                    typed_error.try_set(String::new());
                    typed.try_set(String::new());
                    radio.adding.try_set_value(name.clone());
                    if radio.station.get_untracked().eq_ignore_ascii_case(&name) {
                        load(radio, name, false);
                    } else {
                        app.go(radio_href(&name));
                    }
                }
                Err(message) => {
                    typed_error.try_set(message);
                }
            }
        });
    };

    let station = radio.station;
    let context = move || format!("r/{}", station.get());
    let tracks = Signal::derive(move || radio.tracks.get().unwrap_or_default());
    let has_tracks = Signal::derive(move || radio.tracks.with(|t| t.as_ref().is_some_and(|t| !t.is_empty())));
    let playing_this = move || player.context.get() == context() && player.current().is_some();
    let route = move || Some(radio_href(&station.get_untracked()));
    let save = move || {
        let tracks = tracks.get_untracked();
        let count = tracks.len();
        app.change("createPlaylist", json!({ "name": format!("r/{} · {}", station.get_untracked(), today()), "tracks": tracks }));
        app.notify(format!("Saved {count} songs as a playlist"));
    };
    let wait_text = move || {
        let ago = radio.saved_minutes_ago.get();
        let since = if ago < 0 {
            String::new()
        } else if ago < 60 {
            format!(" Until then, here's the list from {ago} min ago.")
        } else {
            format!(" Until then, here's the list from {} h ago.", (ago as f64 / 60.0).round())
        };
        format!("Reddit lets apps load about one station a minute, so r/{} will load in {} s.{since}", station.get(), radio.wait_seconds.get())
    };

    view! {
        <div class="page radio">
            <div class="wash" style="--wash: rgba(255, 69, 0, 0.30)"></div>
            <h1 class="hero">"Reddit radio"</h1>
            <p class="lead">"Songs shared by people in music communities on Reddit, chosen by humans rather than algorithms. Pick a station and press play."</p>

            // Any subreddit; ones that load with songs are kept under "Your stations".
            <div class="station-add">
                <label class="station-field">
                    <Icon name="radio" size=18/>
                    <input
                        type="text"
                        autocomplete="off"
                        spellcheck="false"
                        placeholder="Tune in to any subreddit: r/vaporwave, or a link to one"
                        prop:value=move || typed.get()
                        on:input=move |event| {
                            typed.set(event_target_value(&event));
                            typed_error.set(String::new());
                        }
                        on:keydown=move |event: web_sys::KeyboardEvent| {
                            if event.key() == "Enter" {
                                tune()
                            }
                        }
                    />
                </label>
                <PillButton text="Tune in" disabled=Signal::derive(move || typed.with(|t| t.trim().is_empty())) on_click=move |_| tune()/>
            </div>
            <Show when=move || typed_error.with(|e| !e.is_empty())>
                <div class="station-field-error">{move || typed_error.get()}</div>
            </Show>

            // The listener's own stations, each removable
            <Show when=move || saved.with(|list| !list.is_empty())>
                <div class="station-group">
                    <div class="station-group-name">"YOUR STATIONS"</div>
                    <div class="stations">
                        {move || saved.get().into_iter().map(|item| {
                            let id = s(&item, "id");
                            let (checked_id, go_id, remove_id) = (id.clone(), id.clone(), id.clone());
                            view! {
                                <span class="station saved" title=s(&item, "about")>
                                    <PillButton text=s(&item, "name") checked=Signal::derive(move || station.get().eq_ignore_ascii_case(&checked_id))
                                        on_click=move |_| app.go(radio_href(&go_id))/>
                                    <IconButton icon="close" size=13 tip=format!("Remove r/{id} from your stations")
                                        on_click=move |_| remove(remove_id.clone())/>
                                </span>
                            }
                        }).collect_view()}
                    </div>
                </div>
            </Show>

            // Station picker, grouped
            {move || app.stations.with(|groups| groups.as_array().cloned().unwrap_or_default()).into_iter().map(|group| view! {
                <div class="station-group">
                    <div class="station-group-name">{s(&group, "name").to_uppercase()}</div>
                    <div class="stations">
                        {list(&group, "stations").into_iter().map(|item| {
                            let id = s(&item, "id");
                            let checked_id = id.clone();
                            view! {
                                <span class="station" title=s(&item, "about")>
                                    <PillButton text=s(&item, "name") checked=Signal::derive(move || station.get() == checked_id)
                                        on_click=move |_| app.go(radio_href(&id))/>
                                </span>
                            }
                        }).collect_view()}
                    </div>
                </div>
            }).collect_view()}

            <div class="radio-head">
                <div class="radio-title">
                    <IconButton filled=true size=24 tip="Play" disabled=Signal::derive(move || !has_tracks.get())
                        icon=Signal::derive(move || if playing_this() && player.playing() { "pause".to_owned() } else { "play".to_owned() })
                        on_click=move |_| if playing_this() {
                            player.toggle()
                        } else if has_tracks.get_untracked() {
                            player.play_tracks(tracks.get_untracked(), 0, format!("r/{}", station.get_untracked()), route())
                        }/>
                    <div class="radio-name">
                        <div class="name">{context}</div>
                        <div class="muted">{move || {
                            let count = radio.tracks.with(|t| t.as_ref().map_or(0, Vec::len));
                            if count > 0 {
                                format!("{count} songs in the hot list")
                            } else if radio.loading.get() || radio.wait_seconds.get() > 0 {
                                "Tuning in…".to_owned()
                            } else {
                                String::new()
                            }
                        }}</div>
                    </div>
                </div>
                <div class="radio-actions">
                    <PillButton text="Shuffle" icon="shuffle" disabled=Signal::derive(move || !has_tracks.get())
                        on_click=move |_| player.shuffle_tracks(tracks.get_untracked(), format!("r/{}", station.get_untracked()), route())/>
                    <PillButton text="Refresh" on_click=move |_| load(radio, station.get_untracked(), true)/>
                    <PillButton text="Save as playlist" icon="plus" disabled=Signal::derive(move || !has_tracks.get()) on_click=move |_| save()/>
                </div>
            </div>

            // Reddit lets apps without an account load about one station a minute.
            <Show when=move || { radio.wait_seconds.get() > 0 }>
                <div class="radio-wait">{wait_text}</div>
            </Show>

            <StatusView
                loading=Signal::derive(move || radio.loading.get() && radio.wait_seconds.get() == 0)
                error=radio.error
                empty=Signal::derive(move || {
                    if radio.tracks.with(|t| t.as_ref().is_some_and(Vec::is_empty)) && radio.wait_seconds.get() == 0 {
                        "No songs in this station right now.".to_owned()
                    } else {
                        String::new()
                    }
                })
                on_retry=Callback::new(move |_| load(radio, station.get_untracked(), true))/>

            <Show when=move || radio.tracks.with(Option::is_some)>
                <TrackList tracks=tracks show_album=false context=Signal::derive(context)/>
            </Show>
        </div>
    }
}
