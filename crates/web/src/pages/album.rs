//! An album, playlist or long video: artwork, play and save, its tags and
//! its songs (qml/AlbumPage.qml, qml/TagChips.qml).

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use serde_json::{Value, json};
use web_sys::MouseEvent;

use crate::api::{Load, fetch};
use crate::components::{Cover, IconButton, PillButton, StatusView, TrackList};
use crate::state::{
    MenuRequest, album_href, album_page, app, artist_href, bandcamp_tag_href, encode, list, long_duration, num, open_web, s, site_name,
};

/// "ALBUM", "PLAYLIST ON SOUNDCLOUD", "ALBUM ON BANDCAMP"…
fn kind_label(album: &Value) -> String {
    let kind = match s(album, "kind").as_str() {
        "single" => "SINGLE",
        "ep" => "EP",
        "compile" => "COMPILATION",
        "playlist" => "PLAYLIST",
        "mix" => "SHOW",
        _ => "ALBUM",
    };
    let place = match s(album, "source").as_str() {
        "youtube" => " ON YOUTUBE",
        "soundcloud" => " ON SOUNDCLOUD",
        "bandcamp" => " ON BANDCAMP",
        "archive" => " ON THE INTERNET ARCHIVE",
        "mixcloud" => " ON MIXCLOUD",
        _ => "",
    };
    format!("{kind}{place}")
}

/// "2001-03-07" as "March 7, 2001"; other dates as they come.
fn release_date(text: &str) -> String {
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let parts: Vec<&str> = text.split('-').collect();
    if let [year, month, day] = parts[..]
        && let (Ok(_), Ok(month @ 1..=12), Ok(day)) = (year.parse::<u32>(), month.parse::<usize>(), day.parse::<u32>())
    {
        return format!("{} {day}, {year}", MONTHS[month - 1]);
    }
    text.to_owned()
}

/// Bandcamp tags as chips; each opens the tag's albums.
#[component]
pub fn TagChips(#[prop(into)] tags: Signal<Vec<Value>>, #[prop(default = "#")] prefix: &'static str) -> impl IntoView {
    view! {
        <Show when=move || !tags.with(Vec::is_empty)>
            <div class="tag-chips">
                {move || tags.get().into_iter().map(|tag| {
                    let label = s(&tag, "label");
                    let href = format!("{}&label={}", bandcamp_tag_href(&s(&tag, "slug")), encode(&label));
                    view! { <a class="pill" href=href>{format!("{prefix}{label}")}</a> }
                }).collect_view()}
            </div>
        </Show>
    }
}

#[component]
pub fn AlbumPage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let query = use_query_map();
    let id = Memo::new(move |_| query.with(|q| q.get("id").unwrap_or_default()));
    let retry = RwSignal::new(0u32);
    let load = fetch(move || {
        retry.track();
        Some(("album", json!({ "id": id.get() })))
    });
    let album = Memo::new(move |_| load.with(|l| l.value().cloned()));
    // Look up the first song's video now, so Play starts right away.
    Effect::new(move |_| {
        if let Some(first) = album.with(|a| a.as_ref().and_then(|a| a["tracks"].get(0).cloned())) {
            player.look_ahead(&first);
        }
        // Bandcamp's tags join the Bandcamp page's tag filter.
        if let Some(tags) = album.with(|a| a.as_ref().map(|a| list(a, "tags"))) {
            app.remember_bandcamp_tags(&tags);
        }
    });

    let field = move |key: &'static str| Signal::derive(move || album.with(|a| a.as_ref().map(|a| s(a, key)).unwrap_or_default()));
    let title = field("title");
    let seed = Signal::derive(move || id.get());
    let tracks = Signal::derive(move || album.with(|a| a.as_ref().map(|a| list(a, "tracks")).unwrap_or_default()));
    let tags = Signal::derive(move || album.with(|a| a.as_ref().map(|a| list(a, "tags")).unwrap_or_default()));
    let web = Signal::derive(move || album.with(|a| a.as_ref().map(album_page).unwrap_or_default()));
    let saved = Signal::derive(move || app.is_album_saved(&id.get()));
    let playing_this = Memo::new(move |_| {
        let Some(current) = player.current() else { return false };
        let context = player.context.get();
        album.with(|a| a.as_ref().is_some_and(|a| s(&current, "albumId") == s(a, "id") && context == s(a, "title")))
    });
    let route = move || Some(album_href(&id.get_untracked()));

    let cover = Signal::derive(move || {
        album.with(|a| a.as_ref().map(|a| if s(a, "coverBig").is_empty() { s(a, "cover") } else { s(a, "coverBig") }).unwrap_or_default())
    });
    let meta = move || {
        album.with(|a| {
            let Some(a) = a else { return String::new() };
            let count = num(a, "trackCount") as u64;
            let songs = format!("{count} {}", if count == 1 { "song" } else { "songs" });
            let length = if num(a, "duration") > 0.0 { long_duration(num(a, "duration")) } else { String::new() };
            [s(a, "year"), songs, length].into_iter().filter(|x| !x.is_empty()).map(|x| format!(" · {x}")).collect::<String>()
        })
    };
    let artist = move || {
        album.with(|a| {
            let a = a.as_ref()?;
            let (name, artist_id) = (s(a, "artist"), s(a, "artistId"));
            Some(if artist_id.is_empty() {
                view! { <span class="album-artist">{name}</span> }.into_any()
            } else {
                view! { <a class="album-artist link" href=artist_href(&artist_id)>{name}</a> }.into_any()
            })
        })
    };
    let footer = move || {
        album.with(|a| {
            let a = a.as_ref()?;
            let date = s(a, "releaseDate");
            let genres = list(a, "genres").iter().filter_map(|g| g.as_str().map(str::to_owned)).collect::<Vec<_>>().join(", ");
            let about = [s(a, "label"), genres].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
            Some(view! {
                <div class="album-foot">
                    {(!date.is_empty()).then(|| view! { <div class="date">{release_date(&date)}</div> })}
                    {(!about.is_empty()).then(|| view! { <div class="about">{about}</div> })}
                </div>
            })
        })
    };

    let play = move |_| {
        if playing_this.get_untracked() {
            player.toggle();
        } else {
            player.play_tracks(tracks.get_untracked(), 0, title.get_untracked(), route());
        }
    };
    let shuffle = move |_| player.shuffle_tracks(tracks.get_untracked(), title.get_untracked(), route());
    let toggle_saved = move |_| {
        let Some(album) = album.get_untracked() else { return };
        let now = !saved.get_untracked();
        app.change("setAlbumSaved", json!({ "album": album, "saved": now }));
        app.notify(if now { "Saved to your albums" } else { "Removed from your albums" });
    };
    let more = move |event: MouseEvent| {
        app.menu.set(Some(MenuRequest::new(tracks.get_untracked(), event.client_x() as f64, event.client_y() as f64)));
    };

    view! {
        <div class="page album-page">
            // Blurred artwork wash behind the header
            <div class="album-wash">
                <Cover src=field("cover") seed=seed radius=0/>
            </div>

            <div class="album-head">
                <Cover src=cover seed=seed radius=10 placeholder="album" class="album-art"/>
                <div class="album-info">
                    <div class="album-kind">{move || album.with(|a| a.as_ref().map(kind_label).unwrap_or_default())}</div>
                    <h1 class="album-title" class:long=move || title.with(|t| t.chars().count() > 22)>
                        {move || { let t = title.get(); if t.is_empty() { "\u{a0}".to_owned() } else { t } }}
                    </h1>
                    <div class="album-meta">{artist}<span class="muted">{meta}</span></div>
                </div>
            </div>

            <Show when=move || album.with(Option::is_some)>
                <div class="album-actions">
                    <IconButton filled=true size=24 tip="Play album"
                        icon=Signal::derive(move || if playing_this.get() && player.playing() { "pause".to_owned() } else { "play".to_owned() })
                        on_click=play/>
                    <PillButton text="Shuffle" icon="shuffle" on_click=shuffle/>
                    <span class="save" class:saved=move || saved.get()>
                        <IconButton size=22
                            icon=Signal::derive(move || if saved.get() { "heart-filled".to_owned() } else { "heart".to_owned() })
                            tip=Signal::derive(move || if saved.get() { "Remove from your albums".to_owned() } else { "Save to your albums".to_owned() })
                            on_click=toggle_saved/>
                    </span>
                    <IconButton icon="more" size=20 tip="Add the whole album to the queue or a playlist" on_click=more/>
                    <Show when=move || !web.with(String::is_empty)>
                        <IconButton icon="external" size=20
                            tip=Signal::derive(move || format!("Open on {}", site_name(&web.get())))
                            on_click=move |_| open_web(&web.get_untracked())/>
                    </Show>
                </div>
            </Show>

            <StatusView loading=Signal::derive(move || load.with(Load::loading)) error=Signal::derive(move || load.with(Load::error))
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>

            // A Bandcamp release's tags: each opens more albums like it.
            <TagChips tags=tags/>

            <Show when=move || album.with(Option::is_some)>
                <TrackList tracks=tracks context=title show_cover=false show_album=false/>
            </Show>

            {footer}
        </div>
    }
}
