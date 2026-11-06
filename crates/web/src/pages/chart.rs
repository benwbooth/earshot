//! Popularity list for a genre or an era (qml/ChartPage.qml).

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use serde_json::json;
use web_sys::MouseEvent;

use crate::api::{Load, fetch};
use crate::components::{AlbumCard, Cover, IconButton, PillButton, SectionHeader, Shelf, StatusView, TrackList};
use crate::state::{MenuRequest, album_href, app, list, s};

#[component]
pub fn ChartPage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let query = use_query_map();
    let kind = Memo::new(move |_| query.with(|q| q.get("kind").unwrap_or_default()));
    let id = Memo::new(move |_| query.with(|q| q.get("id").unwrap_or_default()));
    let era = move || kind.get() == "era";
    let retry = RwSignal::new(0u32);
    let load = fetch(move || {
        retry.track();
        Some((if era() { "era" } else { "genre" }, json!({ "id": id.get() })))
    });
    let data = Memo::new(move |_| load.with(|l| l.value().cloned()));

    let ready = move || data.with(Option::is_some);
    let title = Signal::derive(move || data.with(|d| d.as_ref().map(|d| s(d, "title")).unwrap_or_default()));
    let tracks = Signal::derive(move || data.with(|d| d.as_ref().map(|d| list(d, "tracks")).unwrap_or_default()));
    let albums = Signal::derive(move || data.with(|d| d.as_ref().map(|d| list(d, "albums")).unwrap_or_default()));
    let context = Signal::derive(move || {
        let title = title.get();
        if title.is_empty() { "Chart".to_owned() } else { title }
    });
    let wash = move || {
        let color = data.with(|d| d.as_ref().map(|d| s(d, "color")).unwrap_or_default());
        let color = if color.is_empty() { "var(--accent2)".to_owned() } else { color };
        format!("--wash: color-mix(in srgb, {color} 45%, transparent)")
    };
    let picture = Signal::derive(move || {
        data.with(|d| {
            let Some(d) = d else { return String::new() };
            let picture = s(d, "picture");
            if !picture.is_empty() { picture } else { d["tracks"].get(0).map(|t| s(t, "coverBig")).unwrap_or_default() }
        })
    });
    let heading = move || {
        data.with(|d| match d {
            None => "\u{a0}".to_owned(),
            Some(d) if era() => s(d, "title"),
            Some(d) if s(d, "title") == "All" => "Top songs".to_owned(),
            Some(d) => format!("Top {}", s(d, "title")),
        })
    };
    let subtitle = move || data.with(|d| d.as_ref().map(|d| format!("{} · {} songs", s(d, "subtitle"), list(d, "tracks").len())));

    view! {
        <div class="page chart-page">
            <div class="wash chart-wash" style=wash></div>

            <div class="chart-head">
                <Cover src=picture seed=Signal::derive(move || id.get()) radius=10 class="chart-art"/>
                <div class="chart-info">
                    <div class="eyebrow">{move || if era() { "ERA" } else { "CHART" }}</div>
                    <h1 class="chart-title">{heading}</h1>
                    {move || subtitle().map(|text| view! { <div class="chart-subtitle">{text}</div> })}
                </div>
            </div>

            <Show when=ready>
                <div class="chart-actions">
                    <IconButton filled=true size=24 tip="Play"
                        icon=Signal::derive(move || tracks.with(|list| player.play_icon(&context.get(), list)))
                        on_click=move |_| {
                            let (list, name) = (tracks.get_untracked(), context.get_untracked());
                            player.play_or_toggle(&name, &list, || player.play_tracks(list.clone(), 0, name.clone(), None))
                        }/>
                    <PillButton text="Shuffle" icon="shuffle"
                        on_click=move |_| player.shuffle_tracks(tracks.get_untracked(), title.get_untracked(), None)/>
                    <IconButton icon="more" tip="More"
                        on_click=move |event: MouseEvent| app.menu.set(Some(MenuRequest::new(
                            tracks.get_untracked(), event.client_x() as f64, event.client_y() as f64)))/>
                </div>
            </Show>

            <StatusView loading=Signal::derive(move || load.with(Load::loading)) error=Signal::derive(move || load.with(Load::error))
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>

            <Show when=ready>
                <TrackList tracks=tracks context=title/>
            </Show>

            <Show when=move || !albums.with(Vec::is_empty)>
                <SectionHeader title="Popular albums"/>
                <Shelf>
                    {move || albums.get().into_iter().map(|album| {
                        let (album_id, album_title) = (s(&album, "id"), s(&album, "title"));
                        let image = if s(&album, "coverBig").is_empty() { s(&album, "cover") } else { s(&album, "coverBig") };
                        let (play_id, play_title) = (album_id.clone(), album_title.clone());
                        view! {
                            <AlbumCard title=album_title subtitle=s(&album, "artist") image=image seed=album_id.clone() href=album_href(&album_id)
                                on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
                        }
                    }).collect_view()}
                </Shelf>
            </Show>
        </div>
    }
}
