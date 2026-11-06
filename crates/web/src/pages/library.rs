//! Saved albums (qml/LibraryAlbumsPage.qml), and the phone's Library tab: the
//! sidebar's library entries, since phones don't show the sidebar.

use std::cmp::Ordering;

use leptos::prelude::*;
use serde_json::Value;

use crate::components::{AlbumCard, Icon, IconButton, PillButton, Shelf, StatusView};
use crate::state::{Prompt, album_href, app, playlist_href, s};

fn by_text(a: &Value, b: &Value, key: &str) -> Ordering {
    s(a, key).to_lowercase().cmp(&s(b, key).to_lowercase())
}

fn sorted(mut albums: Vec<Value>, sort: &str) -> Vec<Value> {
    match sort {
        "title" => albums.sort_by(|a, b| by_text(a, b, "title")),
        "artist" => albums.sort_by(|a, b| by_text(a, b, "artist").then_with(|| s(a, "year").cmp(&s(b, "year")))),
        "year" => albums.sort_by(|a, b| s(b, "year").cmp(&s(a, "year"))),
        _ => {}
    }
    albums
}

#[component]
pub fn AlbumsPage() -> impl IntoView {
    let app = app();
    let sort = RwSignal::new("recent");
    let albums = Memo::new(move |_| app.saved_albums());

    view! {
        <div class="page albums">
            <h1 class="hero">"Your albums"</h1>
            <Show when=move || !albums.with(Vec::is_empty)>
                <div class="albums-sort">
                    <span class="muted albums-count">{move || {
                        let n = albums.with(Vec::len);
                        if n == 1 { "1 album".to_owned() } else { format!("{n} albums") }
                    }}</span>
                    {[("recent", "Recently saved"), ("title", "Title"), ("artist", "Artist"), ("year", "Year")].into_iter().map(|(key, label)| view! {
                        <PillButton text=label checked=Signal::derive(move || sort.get() == key) on_click=move |_| sort.set(key)/>
                    }).collect_view()}
                </div>
            </Show>
            <StatusView loading=false empty_icon="album"
                empty=Signal::derive(move || if albums.with(Vec::is_empty) {
                    "Albums you save appear here. Open any album and press the heart to keep it.".to_owned()
                } else {
                    String::new()
                })/>
            {move || {
                let list = sorted(albums.get(), sort.get());
                view! {
                    <Shelf>
                        {list.into_iter().map(|album| {
                            let (id, title) = (s(&album, "id"), s(&album, "title"));
                            let year = s(&album, "year");
                            let subtitle = if year.is_empty() { s(&album, "artist") } else { format!("{year} · {}", s(&album, "artist")) };
                            let image = if s(&album, "coverBig").is_empty() { s(&album, "cover") } else { s(&album, "coverBig") };
                            let (play_id, play_title) = (id.clone(), title.clone());
                            view! {
                                <AlbumCard title=title subtitle=subtitle image=image seed=id.clone() href=album_href(&id)
                                    on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
                            }
                        }).collect_view()}
                    </Shelf>
                }
            }}
        </div>
    }
}

/// One entry, like the sidebar's.
#[component]
fn Entry(#[prop(into)] icon: String, #[prop(into)] label: String, #[prop(into)] href: String, #[prop(into)] detail: Signal<String>) -> impl IntoView {
    view! {
        <a class="nav-item" href=href>
            <Icon name=icon size=18/>
            <span class="label">{label}</span>
            <span class="detail">{move || detail.get()}</span>
        </a>
    }
}

#[component]
pub fn LibraryPage() -> impl IntoView {
    let app = app();
    let count = |n: u64| if n > 0 { n.to_string() } else { String::new() };
    let liked = Signal::derive(move || count(app.playlists().iter().find(|p| s(p, "id") == "liked").and_then(|p| p["count"].as_u64()).unwrap_or(0)));
    let albums = Signal::derive(move || count(app.saved_albums().len() as u64));

    view! {
        <div class="page library">
            <h1 class="hero">"Your library"</h1>
            <div class="library-list">
                <Entry icon="album" label="Albums" href="/albums" detail=albums/>
                <Entry icon="heart" label="Liked songs" href=playlist_href("liked") detail=liked/>
                <div class="nav-heading with-buttons">
                    <span>"PLAYLISTS"</span>
                    <IconButton icon="link" size=15 tip="Import a playlist from YouTube, SoundCloud or Bandcamp" on_click=move |_| app.prompt.set(Some(Prompt::Import))/>
                    <IconButton icon="plus" size=16 tip="New playlist" on_click=move |_| app.prompt.set(Some(Prompt::NewPlaylist(Vec::new())))/>
                </div>
                {move || {
                    let mine: Vec<Value> = app.playlists().into_iter().filter(|p| !p["special"].as_bool().unwrap_or(false)).collect();
                    if mine.is_empty() {
                        view! { <p class="sidebar-hint">"Make a playlist with +, or add songs from any list with the ··· button."</p> }.into_any()
                    } else {
                        mine.into_iter().map(|p| {
                            let n = count(p["count"].as_u64().unwrap_or(0));
                            view! { <Entry icon="playlist" label=s(&p, "name") href=playlist_href(&s(&p, "id")) detail=n/> }
                        }).collect_view().into_any()
                    }
                }}
            </div>
        </div>
    }
}
