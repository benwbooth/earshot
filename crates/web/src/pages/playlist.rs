//! A playlist, or Liked songs: artwork, name, play and shuffle, rename and
//! delete, a filter, and the songs (qml/PlaylistPage.qml).

use leptos::portal::Portal;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use serde_json::{Value, json};

use crate::api;
use crate::components::{Cover, Icon, IconButton, PillButton, PlayButton, StatusView, TrackList};
use crate::state::{Prompt, app, list, long_duration, num, playlist_href, s};

#[component]
pub fn PlaylistPage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let query = use_query_map();
    let id = Memo::new(move |_| query.with(|q| q.get("id")).unwrap_or_default());

    // The playlist with the id it was loaded for, again whenever the library changes.
    let loaded: RwSignal<Option<(String, Value)>> = RwSignal::new(None);
    let error = RwSignal::new(String::new());
    let retry = RwSignal::new(0u32);
    Effect::new(move |_| {
        app.revision.track();
        retry.track();
        let id = id.get();
        spawn_local(async move {
            let result = api::call("playlist", json!({ "id": id })).await;
            if loaded.is_disposed() || id != query.with_untracked(|q| q.get("id")).unwrap_or_default() {
                return;
            }
            match result {
                Ok(value) => {
                    error.set(String::new());
                    loaded.set(Some((id, value)));
                }
                Err(message) => error.set(message),
            }
        });
    });
    let filter = RwSignal::new(String::new());
    Effect::new(move |_| {
        id.track();
        filter.set(String::new());
    });

    let playlist = Memo::new(move |_| loaded.with(|l| l.as_ref().filter(|(for_id, value)| *for_id == id.get() && value.is_object()).map(|(_, value)| value.clone())));
    let ready = move || loaded.with(|l| l.as_ref().is_some_and(|(for_id, _)| *for_id == id.get()));
    let exists = move || playlist.with(Option::is_some);
    let field = move |key: &'static str| playlist.with(|p| p.as_ref().map(|p| s(p, key)).unwrap_or_default());
    let special = Signal::derive(move || playlist.with(|p| p.as_ref().is_some_and(|p| p["special"].as_bool().unwrap_or(false))));
    let count = Signal::derive(move || playlist.with(|p| p.as_ref().map_or(0, |p| num(p, "count") as usize)));
    let tracks = Signal::derive(move || playlist.with(|p| p.as_ref().map(|p| list(p, "tracks")).unwrap_or_default()));
    let shown = Signal::derive(move || {
        let wanted = filter.with(|f| f.trim().to_lowercase());
        let all = tracks.get();
        if wanted.is_empty() {
            return all;
        }
        all.into_iter()
            .filter(|t| format!("{} {} {}", s(t, "title"), s(t, "artist"), s(t, "album")).to_lowercase().contains(&wanted))
            .collect()
    });
    let filtering = Memo::new(move |_| !filter.with(|f| f.trim().is_empty()));
    // Started from this playlist's page (its address, so a rename or another
    // playlist of the same name doesn't matter).
    let playing_this = Signal::derive(move || exists() && player.playing_route(&playlist_href(&id.get())));
    let route = move || Some(playlist_href(&id.get_untracked()));
    let rename = move || {
        if !special.get_untracked() {
            app.prompt.set(Some(Prompt::Rename(id.get_untracked(), field("name"))));
        }
    };
    let confirm_delete = RwSignal::new(false);
    let delete = move || {
        let name = field("name");
        confirm_delete.set(false);
        app.change("deletePlaylist", json!({ "id": id.get_untracked() }));
        app.notify(format!("Deleted {name}"));
        app.go("/");
    };

    view! {
        <div class="page playlist">
            <div class="wash" style=move || if special.get() { "--wash: rgba(255, 95, 127, 0.40)" } else { "--wash: rgba(61, 184, 255, 0.30)" }></div>
            <StatusView loading=Signal::derive(move || !ready() && error.get().is_empty()) error=error
                empty=Signal::derive(move || if ready() && !exists() { "This playlist doesn't exist any more.".to_owned() } else { String::new() })
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>
            <Show when=exists>
                <div class="playlist-head">
                    // Artwork: a heart for liked songs, otherwise a mosaic of the first covers
                    <div class="playlist-art" class:special=move || special.get()>
                        {move || {
                            let covers: Vec<String> = playlist.with(|p| p.as_ref().map(|p| list(p, "covers")).unwrap_or_default())
                                .iter().filter_map(|c| c.as_str().map(str::to_owned)).collect();
                            if special.get() || covers.is_empty() {
                                let icon = if special.get() { "heart-filled" } else { "playlist" };
                                view! { <Icon name=icon.to_owned() size=72/> }.into_any()
                            } else {
                                let mosaic = covers.len() >= 4;
                                let shown: Vec<String> = covers.into_iter().take(if mosaic { 4 } else { 1 }).collect();
                                view! {
                                    <div class="mosaic" class:four=mosaic>
                                        {shown.into_iter().map(|cover| view! { <Cover src=cover.clone() seed=cover radius=0/> }).collect_view()}
                                    </div>
                                }.into_any()
                            }
                        }}
                    </div>
                    <div class="playlist-text">
                        <div class="playlist-kind">"PLAYLIST"</div>
                        <h1 class="playlist-name" class:long=move || { field("name").chars().count() > 24 } class:renamable=move || !special.get()
                            on:click=move |_| rename()>{move || field("name")}</h1>
                        <div class="playlist-meta">{move || {
                            let n = count.get();
                            let length = playlist.with(|p| p.as_ref().map_or(0.0, |p| num(p, "duration")));
                            let songs = if n == 1 { "1 song".to_owned() } else { format!("{n} songs") };
                            if length > 0.0 { format!("{songs} · {}", long_duration(length)) } else { songs }
                        }}</div>
                    </div>
                </div>

                <div class="playlist-actions">
                    <PlayButton tip="Play" ours=playing_this disabled=Signal::derive(move || count.get() == 0)
                        on_play=move |_| player.play_tracks(tracks.get_untracked(), 0, field("name"), route())/>
                    <PillButton text="Shuffle" icon="shuffle" disabled=Signal::derive(move || count.get() == 0)
                        on_click=move |_| player.shuffle_tracks(tracks.get_untracked(), field("name"), route())/>
                    <Show when=move || !special.get()>
                        <IconButton icon="edit" tip="Rename" on_click=move |_| rename()/>
                        <IconButton icon="trash" tip="Delete playlist" on_click=move |_| confirm_delete.set(true)/>
                    </Show>
                    <span class="spacer"></span>
                    <Show when=move || { count.get() > 8 }>
                        <label class="playlist-filter">
                            <Icon name="search".to_owned() size=15/>
                            <input type="search" placeholder="Find in playlist" prop:value=move || filter.get()
                                on:input=move |event| filter.set(event_target_value(&event))/>
                        </label>
                    </Show>
                </div>

                <Show when=move || count.get() == 0>
                    {move || if special.get() {
                        view! { <StatusView loading=false empty="Songs you like show up here. Press the heart next to any song." empty_icon="heart"/> }.into_any()
                    } else {
                        view! { <StatusView loading=false empty="This playlist is empty. Use the ··· button next to any song or album to add it here." empty_icon="playlist"/> }.into_any()
                    }}
                </Show>

                <Show when=move || { count.get() > 0 }>
                    // Moving and removing songs only makes sense in the unfiltered list.
                    {move || {
                        let playlist_id = if filtering.get() { String::new() } else { id.get() };
                        view! { <TrackList tracks=shown playlist_id=playlist_id context=Signal::derive(move || field("name"))/> }
                    }}
                </Show>
            </Show>

            // Placed on the document's body, so it covers the whole window.
            <Show when=move || confirm_delete.get()>
                <Portal><div class="dialog-backdrop" on:click=move |_| confirm_delete.set(false)>
                    <div class="dialog delete-dialog" on:click=|event| event.stop_propagation()>
                        <h3>"Delete playlist?"</h3>
                        <p>{move || format!("\"{}\" and its {} songs will be removed from your library.", field("name"), count.get())}</p>
                        <div class="dialog-buttons">
                            <PillButton text="Cancel" on_click=move |_| confirm_delete.set(false)/>
                            <PillButton text="Delete" primary=true on_click=move |_| delete()/>
                        </div>
                    </div>
                </div></Portal>
            </Show>
        </div>
    }
}
