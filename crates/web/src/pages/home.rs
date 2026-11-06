//! Home: a greeting, quick picks, trending songs and albums, eras, genres and
//! Reddit radio stations (qml/HomePage.qml).

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{Load, fetch};
use crate::components::{AlbumCard, Cover, IconButton, SectionHeader, Shelf, StatusView, TileCard, TrackList};
use crate::state::{album_href, app, chart_href, list, radio_href, s};

thread_local! {
    // Kept for the visit, like the Qt app's Api.homeCache.
    static CACHE: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
}

fn greeting() -> &'static str {
    let hour = js_sys::Date::new_0().get_hours();
    if hour < 5 {
        "Up late?"
    } else if hour < 12 {
        "Good morning"
    } else if hour < 18 {
        "Good afternoon"
    } else {
        "Good evening"
    }
}

#[component]
pub fn HomePage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let retry = RwSignal::new(0u32);
    let cached = CACHE.with(|c| c.borrow().clone());
    let load = if let Some(value) = cached {
        RwSignal::new(Load::Ready(value))
    } else {
        fetch(move || {
            retry.track();
            Some(("home", json!({})))
        })
    };
    Effect::new(move |_| {
        if let Load::Ready(value) = load.get() {
            CACHE.with(|c| *c.borrow_mut() = Some(value));
        }
    });
    let data = Memo::new(move |_| load.with(|l| l.value().cloned()));
    let tracks = Signal::derive(move || data.with(|d| d.as_ref().map(|d| list(d, "tracks")).unwrap_or_default()));
    let ready = move || data.with(Option::is_some);

    view! {
        <div class="page home">
            <div class="wash" style="--wash: rgba(122, 92, 255, 0.28)"></div>
            <h1 class="hero">{greeting()}</h1>
            <p class="lead">"What's popular now, the hits of every decade, and songs people are sharing on Reddit."</p>
            <StatusView loading=Signal::derive(move || load.with(Load::loading)) error=Signal::derive(move || load.with(Load::error))
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>
            <Show when=ready>
                // Quick picks: the top songs as compact tiles
                <div class="quick-picks">
                    {move || tracks.get().into_iter().enumerate().filter(|(_, track)| player.allowed(track)).take(6).map(|(index, track)| {
                        let key = s(&track, "key");
                        let is_current = Memo::new(move |_| player.current_key() == key);
                        let play = move || if is_current.get_untracked() { player.toggle() } else {
                            player.play_tracks(tracks.get_untracked(), index, "Trending now".into(), Some("/".into()))
                        };
                        view! {
                            <div class="quick" class:current=move || is_current.get() on:click=move |_| play()>
                                <Cover src=s(&track, "cover") seed=s(&track, "key") radius=8/>
                                <div class="quick-text">
                                    <div class="title">{s(&track, "title")}</div>
                                    <div class="subtitle">{s(&track, "artist")}</div>
                                </div>
                                <IconButton filled=true size=16
                                    icon=Signal::derive(move || if is_current.get() && player.playing() { "pause".to_owned() } else { "play".to_owned() })
                                    on_click=move |_| play()/>
                            </div>
                        }
                    }).collect_view()}
                </div>

                <SectionHeader title="Trending songs" action="SEE ALL" href=chart_href("genre", "0")/>
                <TrackList tracks=tracks limit=10 context="Trending songs" show_header=false route="/".to_owned()/>

                <SectionHeader title="Popular albums" action="SEE ALL" href=chart_href("genre", "0")/>
                <Shelf rows=1>
                    {move || data.get().map(|d| list(&d, "albums")).unwrap_or_default().into_iter().map(|album| {
                        let (id, title) = (s(&album, "id"), s(&album, "title"));
                        let image = if s(&album, "coverBig").is_empty() { s(&album, "cover") } else { s(&album, "coverBig") };
                        let (play_id, play_title) = (id.clone(), title.clone());
                        view! {
                            <AlbumCard title=title subtitle=s(&album, "artist") image=image seed=id.clone() href=album_href(&id) album_id=id.clone()
                                on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
                        }
                    }).collect_view()}
                </Shelf>

                <SectionHeader title="Browse by era"/>
                <Shelf albums=false min=150 gap=14>
                    {move || data.get().map(|d| list(&d, "eras")).unwrap_or_default().into_iter().map(|era| view! {
                        <TileCard title=s(&era, "name") subtitle="Greatest hits" tint=s(&era, "color") height=84 href=chart_href("era", &s(&era, "id"))/>
                    }).collect_view()}
                </Shelf>

                <SectionHeader title="Browse by genre"/>
                <Shelf albums=false min=200 gap=14>
                    {move || data.get().map(|d| list(&d, "genres")).unwrap_or_default().into_iter().enumerate().map(|(index, genre)| {
                        let tint = ["#ff5f7e", "#7a5cff", "#3db8ff", "#3ddc97", "#f5c542", "#ff8a3d", "#d36cff"][index % 7];
                        view! { <TileCard title=s(&genre, "name") image=s(&genre, "picture") tint=tint href=chart_href("genre", &s(&genre, "id"))/> }
                    }).collect_view()}
                </Shelf>

                <SectionHeader title="Reddit radio" action="ALL STATIONS" href="/radio"/>
                <Shelf albums=false min=200 gap=14 rows=1>
                    {move || app.stations.with(|groups| groups.as_array().and_then(|g| g.first()).map(|g| list(g, "stations")).unwrap_or_default())
                        .into_iter().enumerate().map(|(index, station)| {
                            let tint = ["#ff4500", "#ff8a3d", "#ff5f7e", "#d36cff", "#7a5cff", "#3db8ff"][index % 6];
                            view! { <TileCard title=s(&station, "name") subtitle=s(&station, "about") tint=tint href=radio_href(&s(&station, "id"))/> }
                        }).collect_view()}
                </Shelf>
            </Show>
        </div>
    }
}
