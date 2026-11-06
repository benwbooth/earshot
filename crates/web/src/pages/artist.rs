//! An artist: a big photo, their popular songs and their albums, singles and
//! EPs (qml/ArtistPage.qml).

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use serde_json::{Value, json};

use crate::api::{Load, fetch};
use crate::components::{AlbumCard, Cover, IconButton, PillButton, SectionHeader, Shelf, StatusView, TrackList};
use crate::state::{album_href, app, list, num, s};

/// 5232611 as "5,232,611".
fn thousands(n: f64) -> String {
    let digits = (n.max(0.0).round() as u64).to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "2013", or "2013 · Single", "2013 · EP".
fn release_line(album: &Value) -> String {
    let kind = s(album, "kind");
    let label = match kind.as_str() {
        "" | "album" => String::new(),
        "ep" => " · EP".to_owned(),
        _ => {
            let mut letters = kind.chars();
            let first = letters.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
            format!(" · {first}{}", letters.as_str())
        }
    };
    format!("{}{label}", s(album, "year"))
}

#[component]
pub fn ArtistPage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let query = use_query_map();
    let id = Memo::new(move |_| query.with(|q| q.get("id").unwrap_or_default()));
    let retry = RwSignal::new(0u32);
    let load = fetch(move || {
        retry.track();
        Some(("artist", json!({ "id": id.get() })))
    });
    let artist = Memo::new(move |_| load.with(|l| l.value().cloned()));
    let show_all_top = RwSignal::new(false);
    let singles = RwSignal::new(false);
    // A different artist starts with the short list of their albums.
    Effect::new(move |_| {
        id.track();
        show_all_top.set(false);
        singles.set(false);
    });

    let ready = move || artist.with(Option::is_some);
    let name = Signal::derive(move || artist.with(|a| a.as_ref().map(|a| s(a, "name")).unwrap_or_default()));
    let top = Signal::derive(move || artist.with(|a| a.as_ref().map(|a| list(a, "topTracks")).unwrap_or_default()));
    let releases = Memo::new(move |_| {
        let key = if singles.get() { "singles" } else { "albums" };
        artist.with(|a| a.as_ref().map(|a| list(a, key)).unwrap_or_default())
    });
    let picture = Signal::derive(move || artist.with(|a| a.as_ref().map(|a| s(a, "pictureBig")).unwrap_or_default()));
    let about = move || {
        artist.with(|a| a.as_ref().map(|a| format!("{} fans · {} releases", thousands(num(a, "fans")), num(a, "albumCount") as u64)))
    };

    view! {
        <div class="page artist-page">
            // Big artist photo header
            <div class="artist-hero">
                <Cover src=picture seed=Signal::derive(move || id.get()) radius=0 placeholder="user" class="artist-photo"/>
                <div class="artist-shade"></div>
                <div class="artist-heading">
                    <h1 class="artist-name">{move || { let n = name.get(); if n.is_empty() { "\u{a0}".to_owned() } else { n } }}</h1>
                    {move || about().map(|text| view! { <div class="artist-about">{text}</div> })}
                </div>
            </div>

            <StatusView loading=Signal::derive(move || load.with(Load::loading)) error=Signal::derive(move || load.with(Load::error))
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>

            <Show when=ready>
                <div class="artist-actions">
                    <IconButton filled=true size=24 tip="Play top songs"
                        icon=Signal::derive(move || top.with(|list| player.play_icon(&name.get(), list)))
                        on_click=move |_| {
                            let (list, artist) = (top.get_untracked(), name.get_untracked());
                            player.play_or_toggle(&artist, &list, || player.play_tracks(list.clone(), 0, artist.clone(), None))
                        }/>
                    <PillButton text="Shuffle" icon="shuffle"
                        on_click=move |_| player.shuffle_tracks(top.get_untracked(), name.get_untracked(), None)/>
                </div>

                <SectionHeader title="Popular"/>
                {move || {
                    let limit = if show_all_top.get() { usize::MAX } else { 5 };
                    view! { <TrackList tracks=top limit=limit show_header=false context=name/> }
                }}
                <Show when=move || top.with(|t| t.len() > 5)>
                    <button class="see-more" on:click=move |_| show_all_top.update(|all| *all = !*all)>
                        {move || if show_all_top.get() { "Show less" } else { "See more" }}
                    </button>
                </Show>

                <SectionHeader title="Discography"/>
                <div class="artist-tabs">
                    <PillButton text="Albums" checked=Signal::derive(move || !singles.get()) on_click=move |_| singles.set(false)/>
                    <PillButton text="Singles and EPs" checked=Signal::derive(move || singles.get()) on_click=move |_| singles.set(true)/>
                </div>
                <Shelf>
                    {move || releases.get().into_iter().map(|album| {
                        let (album_id, title) = (s(&album, "id"), s(&album, "title"));
                        let image = if s(&album, "coverBig").is_empty() { s(&album, "cover") } else { s(&album, "coverBig") };
                        let (play_id, play_title) = (album_id.clone(), title.clone());
                        view! {
                            <AlbumCard title=title subtitle=release_line(&album) image=image seed=album_id.clone() href=album_href(&album_id)
                                on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
                        }
                    }).collect_view()}
                </Shelf>
                <Show when=move || releases.with(Vec::is_empty)>
                    <p class="muted">"Nothing here."</p>
                </Show>
            </Show>
        </div>
    }
}
