//! Made for you: Discover Weekly, Daily Mixes, Release Radar and albums
//! picked from what you listen to (qml/ForYouPage.qml).

use std::cell::RefCell;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{Value, json};
use web_sys::MouseEvent;

use crate::api;
use crate::components::{AlbumCard, IconButton, PillButton, SectionHeader, Shelf, StatusView, TileCard, TrackList};
use crate::state::{album_href, app, list, s};

const TINTS: [&str; 4] = ["#7a5cff", "#ff5f7e", "#3db8ff", "#3ddc97"];
const ROUTE: &str = "/foryou";

thread_local! {
    // Kept for the visit, like the Qt app's Api.forYouCache.
    static CACHE: RefCell<Option<Value>> = const { RefCell::new(None) };
}

/// Not enough listening history yet.
fn needs_more(made: &Value) -> bool {
    made["needMore"].as_bool().unwrap_or(false)
}

fn need_more(made: &Option<Value>) -> bool {
    made.as_ref().is_some_and(needs_more)
}

/// Brings the chosen mix's songs into view when they start below the screen,
/// so picking a mix always shows something happen.
fn show_songs() {
    let Some(songs) = document().query_selector(".foryou .mix-songs").ok().flatten() else { return };
    let below = window().inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(0.0) - 160.0;
    if songs.get_bounding_client_rect().top() > below {
        songs.scroll_into_view_with_bool(true);
    }
}

/// "Nov 5", for a saved mix's name.
fn short_date() -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let today = js_sys::Date::new_0();
    format!("{} {}", MONTHS[today.get_month() as usize % 12], today.get_date())
}

#[component]
pub fn ForYouPage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let made = RwSignal::new(CACHE.with(|c| c.borrow().clone()));
    let loading = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let selected = RwSignal::new("discover-weekly".to_owned());

    let load = move |refresh: bool| {
        loading.set(true);
        error.set(String::new());
        spawn_local(async move {
            let result = api::call("forYou", json!({ "refresh": refresh })).await;
            if let Ok(value) = &result {
                CACHE.with(|c| *c.borrow_mut() = Some(value.clone()));
            }
            match result {
                Ok(value) => _ = made.try_set(Some(value)),
                Err(message) => _ = error.try_set(message),
            }
            let _ = loading.try_set(false);
        });
    };
    // Until there's enough history, look again on every visit and after each song counted.
    if made.with_untracked(|m| m.is_none() || need_more(m)) {
        load(false);
    }
    Effect::new(move |previous: Option<u32>| {
        let revision = app.revision.get();
        if previous.is_some_and(|p| p != revision) && made.with_untracked(need_more) && !loading.get_untracked() {
            load(false);
        }
        revision
    });

    let mixes = Memo::new(move |_| {
        made.with(|m| match m {
            Some(m) if !needs_more(m) => std::iter::once(m["weekly"].clone())
                .chain(list(m, "daily"))
                .filter(|mix| !list(mix, "tracks").is_empty())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
    });
    let mix = Memo::new(move |_| {
        let mixes = mixes.get();
        let wanted = selected.get();
        mixes.iter().find(|m| s(m, "id") == wanted).or(mixes.first()).cloned()
    });
    let mix_tracks = Signal::derive(move || mix.with(|m| m.as_ref().map(|m| list(m, "tracks")).unwrap_or_default()));
    let mix_title = Signal::derive(move || mix.with(|m| m.as_ref().map(|m| s(m, "title")).unwrap_or_default()));
    let shelves = Memo::new(move |_| {
        made.with(|m| match m {
            Some(m) if !needs_more(m) => [m["radar"].clone(), m["albums"].clone()].into_iter().filter(|s| s["albums"].is_array()).collect(),
            _ => Vec::<Value>::new(),
        })
    });
    let ready = move || made.with(|m| m.is_some() && !need_more(m));

    view! {
        <div class="page foryou">
            <div class="wash" style="--wash: rgba(122, 92, 255, 0.45)"></div>
            <div class="intro">
                <div class="intro-text">
                    <div class="eyebrow">"MADE FOR YOU"</div>
                    <h1 class="hero">"From what you listen to"</h1>
                    <p class="blurb">"Mixes of new songs like your favourites, new releases from your artists and albums you might like. Your listening history stays on this computer."</p>
                </div>
                <Show when=ready>
                    <PillButton
                        text=Signal::derive(move || if loading.get() { "Making…".to_owned() } else { "Make new mixes".to_owned() })
                        icon="dice"
                        disabled=loading
                        on_click=move |_| load(true)/>
                </Show>
            </div>

            <StatusView
                loading=Signal::derive(move || loading.get() && made.with(Option::is_none))
                error=error
                empty=Signal::derive(move || made.with(|m| match m {
                    Some(m) if needs_more(m) =>
                        "Play a song for half a minute, or like one with the heart, and Earshot will make mixes from what you like.".to_owned(),
                    _ => String::new(),
                }))
                empty_icon="heart"
                on_retry=Callback::new(move |_| load(false))/>
            <Show when=move || loading.get() && made.with(Option::is_none)>
                <p class="making">"Making your mixes: finding new songs like your favourites…"</p>
            </Show>

            // The mixes: pick one to see and play its songs.
            <Show when=move || !mixes.with(Vec::is_empty)>
                <div class="mixes">
                    {move || mixes.get().into_iter().enumerate().map(|(index, m)| {
                        let id = s(&m, "id");
                        let tracks = list(&m, "tracks");
                        let image = tracks.first().map(|t| if s(t, "coverBig").is_empty() { s(t, "cover") } else { s(t, "coverBig") }).unwrap_or_default();
                        let pick = id.clone();
                        let is_selected = Memo::new(move |_| mix.with(|m| m.as_ref().is_some_and(|m| s(m, "id") == id)));
                        view! {
                            // A tile that picks the mix rather than opening a page.
                            <div class="mix" class:selected=move || is_selected.get() on:click=move |event: MouseEvent| {
                                event.prevent_default();
                                selected.set(pick.clone());
                                request_animation_frame(show_songs);
                            }>
                                <TileCard title=s(&m, "title") subtitle=format!("{} songs", tracks.len()) image=image
                                    tint=TINTS[index % TINTS.len()] height=110 href=ROUTE/>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Show>

            <Show when=move || mix.with(Option::is_some)>
                <div class="mix-songs">
                    <SectionHeader title=mix_title/>
                    <p class="note">{move || mix.with(|m| m.as_ref().map(|m| s(m, "subtitle")).unwrap_or_default())}</p>
                    <div class="actions">
                        <IconButton size=22 filled=true tip="Play"
                            icon=Signal::derive(move || mix_tracks.with(|tracks| player.play_icon(&mix_title.get(), tracks)))
                            on_click=move |_| {
                                let (tracks, title) = (mix_tracks.get_untracked(), mix_title.get_untracked());
                                player.play_or_toggle(&title, &tracks, || player.play_tracks(tracks.clone(), 0, title.clone(), Some(ROUTE.into())))
                            }/>
                        <PillButton text="Shuffle" icon="shuffle"
                            on_click=move |_| player.shuffle_tracks(mix_tracks.get_untracked(), mix_title.get_untracked(), Some(ROUTE.into()))/>
                        <PillButton text="Save as playlist" icon="plus" on_click=move |_| {
                            let tracks = mix_tracks.get_untracked();
                            let name = format!("{} · {}", mix_title.get_untracked(), short_date());
                            let count = tracks.len();
                            spawn_local(async move {
                                match api::call("createPlaylist", json!({ "name": name, "tracks": tracks })).await {
                                    Ok(_) => {
                                        app.revision.update(|r| *r += 1);
                                        app.notify(format!("Saved {count} songs as a playlist"));
                                    }
                                    Err(message) => app.notify(message),
                                }
                            });
                        }/>
                    </div>
                    <TrackList tracks=mix_tracks show_album=false context=mix_title route=ROUTE.to_owned()/>
                </div>
            </Show>

            // New releases and albums by related artists.
            {move || shelves.get().into_iter().map(|section| {
                let albums = list(&section, "albums");
                let note = if !albums.is_empty() {
                    s(&section, "subtitle")
                } else if s(&section, "id") == "release-radar" {
                    "No new releases from your artists in the last two months.".to_owned()
                } else {
                    "Nothing yet: play a few more artists.".to_owned()
                };
                view! {
                    <div class="album-section">
                        <SectionHeader title=s(&section, "title")/>
                        <p class="note">{note}</p>
                        {(!albums.is_empty()).then(|| view! {
                            <Shelf>
                                {albums.into_iter().map(|album| {
                                    let (id, title) = (s(&album, "id"), s(&album, "title"));
                                    let year = s(&album, "year");
                                    let subtitle = if year.is_empty() { s(&album, "artist") } else { format!("{year} · {}", s(&album, "artist")) };
                                    let image = if s(&album, "coverBig").is_empty() { s(&album, "cover") } else { s(&album, "coverBig") };
                                    let (play_id, play_title) = (id.clone(), title.clone());
                                    view! {
                                        <AlbumCard title=title subtitle=subtitle image=image seed=id.clone() href=album_href(&id) album_id=id.clone()
                                            on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
                                    }
                                }).collect_view()}
                            </Shelf>
                        })}
                    </div>
                }
            }).collect_view()}
        </div>
    }
}
