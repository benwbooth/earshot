//! The building blocks every page uses, matching the Qt app's QML components
//! of the same names (Icon, IconButton, PillButton, Cover, AlbumCard,
//! TileCard, TrackRow, TrackList, SectionHeader, HorizontalShelf, StatusView).

use leptos::prelude::*;
use serde_json::Value;
use web_sys::MouseEvent;

use crate::state::{MenuRequest, album_href, app, artist_href, duration, s};

/// One of the Qt app's icons (qml/icons), coloured by the text colour.
#[component]
pub fn Icon(#[prop(into)] name: Signal<String>, #[prop(default = 20)] size: u32, #[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <span
            class=format!("icon {class}")
            aria-hidden="true"
            style=move || format!("--size:{size}px;--icon:url(/icons/{}.svg)", name.get())
        ></span>
    }
}

/// A round icon button with a tooltip; `active` marks it with an accent dot.
#[component]
pub fn IconButton(
    #[prop(into)] icon: Signal<String>,
    #[prop(default = 20)] size: u32,
    #[prop(optional, into)] tip: Signal<String>,
    #[prop(optional, into)] active: Signal<bool>,
    #[prop(optional)] filled: bool,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] class: String,
    #[prop(into)] on_click: Callback<MouseEvent>,
) -> impl IntoView {
    let width = if filled { size * 2 + 8 } else { size + 16 };
    view! {
        <button
            class=format!("icon-button {class}")
            class:filled=filled
            class:active=move || active.get()
            disabled=move || disabled.get()
            title=move || tip.get()
            aria-label=move || tip.get()
            style=format!("--box:{width}px")
            on:click=move |event: MouseEvent| {
                event.stop_propagation();
                on_click.run(event)
            }
        >
            <Icon name=icon size=size/>
        </button>
    }
}

/// A pink play button. While the music it started (`ours`) is playing, or on
/// its way, it shows pause and says "Pause", and a click pauses or carries
/// on; otherwise a click runs `on_play`.
#[component]
pub fn PlayButton(
    #[prop(into)] ours: Signal<bool>,
    #[prop(into)] tip: String,
    #[prop(default = 24)] size: u32,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] class: String,
    #[prop(into)] on_play: Callback<MouseEvent>,
) -> impl IntoView {
    let player = app().player;
    let pause = Memo::new(move |_| ours.get() && player.busy());
    view! {
        <IconButton size=size filled=true class=class disabled=disabled
            icon=Signal::derive(move || if pause.get() { "pause".to_owned() } else { "play".to_owned() })
            tip=Signal::derive(move || if pause.get() { "Pause".to_owned() } else { tip.clone() })
            on_click=move |event: MouseEvent| {
                // Inside a card's link: play rather than open it.
                event.prevent_default();
                if ours.get_untracked() { player.toggle() } else { on_play.run(event) }
            }/>
    }
}

#[component]
pub fn PillButton(
    #[prop(into)] text: Signal<String>,
    #[prop(optional, into)] icon: Signal<String>,
    #[prop(optional)] primary: bool,
    #[prop(optional, into)] checked: Signal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(into)] on_click: Callback<MouseEvent>,
) -> impl IntoView {
    let has_icon = move || !icon.with(String::is_empty);
    let icon_size = if primary { 18 } else { 16 };
    view! {
        <button
            class="pill"
            class:primary=primary
            class:checked=move || checked.get()
            disabled=move || disabled.get()
            on:click=move |event| on_click.run(event)
        >
            {move || has_icon().then(|| view! { <Icon name=icon size=icon_size/> })}
            <span>{move || text.get()}</span>
        </button>
    }
}

const SWATCHES: [(&str, &str); 6] = [
    ("#ff5f7e", "#7a5cff"),
    ("#3db8ff", "#3ddc97"),
    ("#ff8a3d", "#ff5f7e"),
    ("#7a5cff", "#3db8ff"),
    ("#f5c542", "#ff8a3d"),
    ("#3ddc97", "#7a5cff"),
];

/// The same gradient for the same seed as the Qt app's Cover.
pub fn swatch(seed: &str) -> (&'static str, &'static str) {
    let mut h: u32 = 0;
    for unit in seed.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(unit as u32);
    }
    SWATCHES[(h % SWATCHES.len() as u32) as usize]
}

/// Artwork with rounded corners and a soft gradient while it loads (or when there is none).
#[component]
pub fn Cover(
    #[prop(into)] src: Signal<String>,
    #[prop(optional, into)] seed: Signal<String>,
    #[prop(default = 8)] radius: u32,
    #[prop(optional)] round: bool,
    #[prop(default = "music")] placeholder: &'static str,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let failed = RwSignal::new(false);
    Effect::new(move |_| {
        src.track();
        failed.set(false);
    });
    let style = move || {
        let seed = seed.get();
        let source = src.get();
        let (a, b) = swatch(if seed.is_empty() { &source } else { &seed });
        format!("--radius:{}; --a:{a}; --b:{b}", if round { "50%".to_owned() } else { format!("{radius}px") })
    };
    view! {
        <div class=format!("cover {class}") style=style>
            <Icon name=placeholder.to_owned() size=0 class="placeholder"/>
            <Show when=move || !src.get().is_empty() && !failed.get()>
                <img src=move || src.get() alt="" loading="lazy" decoding="async" on:error=move |_| failed.set(true)/>
            </Show>
        </div>
    }
}

/// An album (or artist, or playlist) as a card: artwork, title and a line below.
/// In list view it's a row instead: small artwork with the text beside it.
#[component]
pub fn AlbumCard(
    #[prop(into)] title: String,
    #[prop(into)] subtitle: String,
    #[prop(into)] image: String,
    #[prop(optional, into)] seed: String,
    #[prop(into)] href: String,
    #[prop(optional)] round: bool,
    #[prop(default = true)] playable: bool,
    #[prop(default = "album")] placeholder: &'static str,
    #[prop(optional)] on_play: Option<Callback<()>>,
    /// The album the play button plays: while it's what's playing, the button
    /// shows pause and pauses (or carries on) instead of starting it again.
    #[prop(optional, into)]
    album_id: String,
) -> impl IntoView {
    let app = app();
    let player = app.player;
    let seed = if seed.is_empty() { title.clone() } else { seed };
    let playing_this = Memo::new(move |_| player.playing_album(&album_id));
    view! {
        <a class="album-card" class:list=move || app.album_list.get() class:round=round class:playing=move || playing_this.get() href=href>
            <div class="art">
                <Cover src=image seed=seed radius=10 round=round placeholder=placeholder/>
                {(playable && on_play.is_some()).then(|| {
                    let on_play = on_play.unwrap();
                    view! {
                        <PlayButton size=20 class="card-play" tip="Play" ours=playing_this on_play=move |_| on_play.run(())/>
                    }
                })}
            </div>
            <div class="text">
                <div class="title">{title}</div>
                <div class="subtitle">{subtitle}</div>
            </div>
        </a>
    }
}

/// A coloured tile for a genre, era or radio station.
#[component]
pub fn TileCard(
    #[prop(into)] title: String,
    #[prop(optional, into)] subtitle: String,
    #[prop(optional, into)] image: String,
    #[prop(into)] tint: String,
    #[prop(into)] href: String,
    #[prop(default = 96)] height: u32,
) -> impl IntoView {
    let has_image = !image.is_empty();
    let seed = title.clone();
    view! {
        <a class="tile" href=href style=format!("--tint:{tint};--height:{height}px") class:with-image=has_image>
            <div class="tile-text">
                <div class="title">{title}</div>
                {(!subtitle.is_empty()).then(|| view! { <div class="subtitle">{subtitle}</div> })}
            </div>
            {has_image.then(|| view! { <Cover src=image seed=seed class="tile-art"/> })}
        </a>
    }
}

#[component]
pub fn SectionHeader(
    #[prop(into)] title: Signal<String>,
    #[prop(optional, into)] action: String,
    #[prop(optional, into)] href: String,
) -> impl IntoView {
    view! {
        <div class="section-header">
            <h2>{move || title.get()}</h2>
            {(!action.is_empty()).then(|| view! { <a class="section-action" href=href>{action}</a> })}
        </div>
    }
}

/// Album covers across a phone (narrower than the desktop's 168 px).
pub const PHONE_CARD: f64 = 140.0;

/// A responsive grid: as many columns as fit (`min` px per card). Album
/// shelves follow the list view setting; `rows` limits it to that many rows
/// of however many cards fit across.
#[component]
pub fn Shelf(
    #[prop(default = 168)] min: u32,
    #[prop(default = 24)] gap: u32,
    #[prop(default = true)] albums: bool,
    #[prop(optional)] rows: Option<u32>,
    children: Children,
) -> impl IntoView {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let app = app();
    let id = format!("shelf-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let node = NodeRef::<leptos::html::Div>::new();
    let width = RwSignal::new(0.0);
    let measure = move || {
        if let Some(element) = node.get_untracked() {
            width.set(element.client_width() as f64);
        }
    };
    if rows.is_some() {
        // Measured once shown, and again when the window or the sidebar changes size.
        Effect::new(move |_| {
            node.track();
            app.sidebar_width.track();
            request_animation_frame(measure);
        });
        let resized = window_event_listener(leptos::ev::resize, move |_| measure());
        on_cleanup(move || resized.remove());
    }
    let selector = id.clone();
    let hide_extra = move || {
        let (Some(rows), w) = (rows, width.get()) else { return String::new() };
        if w <= 0.0 {
            return String::new();
        }
        let phone = crate::state::window().inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1200.0) <= 760.0;
        let card = if albums && app.album_list.get() { 380.0 } else if albums && phone { PHONE_CARD } else { min as f64 };
        let columns = ((w + gap as f64) / (card + gap as f64)).floor().max(1.0) as u32;
        format!("#{selector} > :nth-child(n+{}) {{ display: none; }}", columns * rows + 1)
    };
    view! {
        <style>{hide_extra}</style>
        <div
            id=id
            node_ref=node
            class="shelf"
            class:albums=albums
            class:list=move || albums && app.album_list.get()
            style=format!("--min:{min}px;--gap:{gap}px")
        >
            {children()}
        </div>
    }
}

/// Loading spinner, error with retry, or an empty-state message.
#[component]
pub fn StatusView(
    #[prop(into)] loading: Signal<bool>,
    #[prop(optional, into)] error: Signal<String>,
    #[prop(optional, into)] empty: Signal<String>,
    #[prop(default = "music")] empty_icon: &'static str,
    #[prop(optional)] on_retry: Option<Callback<()>>,
) -> impl IntoView {
    let shown = move || loading.get() || !error.get().is_empty() || !empty.get().is_empty();
    view! {
        <Show when=shown>
            <div class="status">
                <Show when=move || loading.get() fallback=move || view! {
                    <Icon name=Signal::derive(move || if error.get().is_empty() { empty_icon.to_owned() } else { "close".to_owned() }) size=40 class="faint"/>
                    <p>{move || if error.get().is_empty() { empty.get() } else { error.get() }}</p>
                    {move || (!error.get().is_empty() && on_retry.is_some()).then(|| {
                        let retry = on_retry.unwrap();
                        view! { <PillButton text="Try again" on_click=move |_| retry.run(())/> }
                    })}
                }>
                    <div class="spinner"></div>
                </Show>
            </div>
        </Show>
    }
}

/// Six bars that move with the music actually playing (see levels.rs), and
/// stay still when it can't be measured.
#[component]
pub fn Waveform(#[prop(default = false)] small: bool) -> impl IntoView {
    let player = app().player;
    let bars = (0..crate::levels::BANDS)
        .map(|band| view! { <i style=move || format!("height:{:.0}%", 15.0 + 85.0 * player.levels.with(|l| l[band]))></i> })
        .collect_view();
    view! {
        <span class="waveform" class:small=small title=move || player.levels_note.get()>
            {bars}
        </span>
    }
}

/// Text that glides sideways when it's too long for its space.
#[component]
pub fn Marquee(#[prop(into)] text: Signal<String>, #[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <div class=format!("marquee {class}") title=move || text.get()>
            <span>{move || text.get()}</span>
        </div>
    }
}

/// Where a song plays from (the site's name and logo), for every song row.
/// Catalogue songs (and Reddit's) play from YouTube.
pub fn source(track: &Value) -> (&'static str, &'static str) {
    match track["service"].as_str() {
        Some("bandcamp") => ("Bandcamp", "source-bandcamp"),
        Some("soundcloud") => ("SoundCloud", "source-soundcloud"),
        Some("mixcloud") => ("Mixcloud", "source-mixcloud"),
        Some("archive") => ("Internet Archive", "source-archive"),
        _ => ("YouTube", "source-youtube"),
    }
}

fn opens_menu(tracks: Vec<Value>, event: &MouseEvent) -> MenuRequest {
    MenuRequest::new(tracks, event.client_x() as f64, event.client_y() as f64)
}

/// One song in a list: number or cover, title, artist, album, length, like and more.
#[component]
pub fn TrackRow(
    track: Value,
    number: usize,
    #[prop(default = true)] show_cover: bool,
    #[prop(default = true)] show_album: bool,
    #[prop(default = true)] show_number: bool,
    #[prop(into)] on_play: Callback<()>,
    #[prop(into)] on_menu: Callback<MenuRequest>,
) -> impl IntoView {
    let app = app();
    let player = app.player;
    let key = s(&track, "key");
    let is_current = {
        let key = key.clone();
        Memo::new(move |_| player.current_key() == key)
    };
    let liked = {
        let key = key.clone();
        Signal::derive(move || app.is_liked(&key))
    };
    let toggle_or_play = move || if is_current.get_untracked() { player.toggle() } else { on_play.run(()) };
    let artist_id = s(&track, "artistId");
    let album_id = s(&track, "albumId");
    let mut by = s(&track, "artist");
    for extra in ["source", "tags"] {
        let text = s(&track, extra);
        if !text.is_empty() {
            by = format!("{by}  ·  {text}");
        }
    }
    let length = track["duration"].as_f64().filter(|d| *d > 0.0).map(duration).unwrap_or_default();
    let hover_track = track.clone();
    let like_track = track.clone();
    let menu_track = track.clone();
    let context_track = track.clone();
    view! {
        <div
            class="track-row"
            class:current=move || is_current.get()
            // The song that's on pauses or carries on; any other starts.
            on:dblclick=move |_| toggle_or_play()
            on:contextmenu=move |event: MouseEvent| {
                event.prevent_default();
                on_menu.run(opens_menu(vec![context_track.clone()], &event));
            }
            // Pointing at a song looks up its video, so a click starts it right away.
            on:mouseenter=move |_| {
                let track = hover_track.clone();
                gloo_timers::callback::Timeout::new(450, move || player.look_ahead(&track)).forget();
            }
        >
            {show_number.then(|| view! {
                <button class="number" on:click=move |_| toggle_or_play()>
                    <span class="n">{number}</span>
                    <Icon name=Signal::derive(move || if is_current.get() && player.playing() { "pause".to_owned() } else { "play".to_owned() }) size=16 class="play-icon"/>
                    <Show when=move || is_current.get() && player.playing()>
                        <Waveform small=true/>
                    </Show>
                </button>
            })}
            {show_cover.then(|| {
                let cover = s(&track, "cover");
                let seed = key.clone();
                view! {
                    <button class="row-cover" class:clickable=!show_number on:click=move |_| if !show_number { toggle_or_play() }>
                        <Cover src=cover seed=seed radius=6/>
                        {(!show_number).then(|| view! {
                            <span class="cover-play"><Icon name=Signal::derive(move || if is_current.get() && player.playing() { "pause".to_owned() } else { "play".to_owned() }) size=16/></span>
                        })}
                    </button>
                }
            })}
            <div class="row-text">
                <div class="row-title">{s(&track, "title")}</div>
                <div class="row-by">
                    {track["explicit"].as_bool().unwrap_or(false).then(|| view! { <span class="explicit">"E"</span> })}
                    {
                        let (name, logo) = source(&track);
                        // One colour, like the other small icons; the name on hover.
                        view! { <span class="source" role="img" aria-label=name title=format!("Plays from {name}")><Icon name=logo.to_owned() size=14/></span> }
                    }
                    {if artist_id.is_empty() {
                        view! { <span>{by}</span> }.into_any()
                    } else {
                        view! { <a href=artist_href(&artist_id) on:click=|e: MouseEvent| e.stop_propagation()>{by}</a> }.into_any()
                    }}
                </div>
            </div>
            {show_album.then(|| {
                let album = s(&track, "album");
                if album_id.is_empty() {
                    view! { <div class="row-album">{album}</div> }.into_any()
                } else {
                    view! { <a class="row-album" href=album_href(&album_id)>{album}</a> }.into_any()
                }
            })}
            <IconButton
                icon=Signal::derive(move || if liked.get() { "heart-filled".to_owned() } else { "heart".to_owned() })
                size=16
                class="row-like"
                active=Signal::derive(move || false)
                tip=Signal::derive(move || if liked.get() { "Remove from Liked songs".to_owned() } else { "Add to Liked songs".to_owned() })
                on_click=move |_| app.set_liked(&like_track, !liked.get_untracked())
            />
            <span class="row-time">{length}</span>
            <IconButton icon="more" size=16 class="row-more" tip="More"
                on_click=move |event: MouseEvent| on_menu.run(opens_menu(vec![menu_track.clone()], &event))/>
        </div>
    }
}

/// A list of songs. Double-click (or the play button) plays from that song on.
#[component]
pub fn TrackList(
    #[prop(into)] tracks: Signal<Vec<Value>>,
    #[prop(optional, into)] context: Signal<String>,
    #[prop(default = true)] show_cover: bool,
    #[prop(default = true)] show_album: bool,
    #[prop(default = true)] show_number: bool,
    #[prop(default = true)] show_header: bool,
    #[prop(optional, into)] playlist_id: String,
    #[prop(optional)] limit: Option<usize>,
    #[prop(optional)] queue_mode: bool,
    /// The page to come back to from the player bar (the current page by default).
    #[prop(optional, into)] route: Option<String>,
) -> impl IntoView {
    let app = app();
    let player = app.player;
    let playlist_id = StoredValue::new(playlist_id);
    let route = StoredValue::new(route);
    // The songs shown (explicit ones are left out while they're hidden).
    let shown = Memo::new(move |_| tracks.get().into_iter().filter(|t| player.allowed(t)).collect::<Vec<Value>>());
    let rows = move || {
        let all = tracks.get();
        let length = all.len();
        // Each song shown with where it sits in `tracks` (for playlist and queue changes).
        let visible: Vec<(usize, Value)> = all.into_iter().enumerate().filter(|(_, t)| player.allowed(t)).take(limit.unwrap_or(usize::MAX)).collect();
        visible
            .into_iter()
            .enumerate()
            .map(|(index, (at, track))| {
                let number = if !show_cover { track["trackNumber"].as_u64().map(|n| n as usize).unwrap_or(index + 1) } else { index + 1 };
                let on_play = Callback::new(move |_| {
                    if queue_mode {
                        player.jump(at as i64);
                    } else {
                        player.play_tracks(shown.get_untracked(), index, context.get_untracked(), route.get_value());
                    }
                });
                let on_menu = Callback::new(move |mut request: MenuRequest| {
                    let id = playlist_id.get_value();
                    if !id.is_empty() {
                        request.playlist_id = id;
                        request.playlist_index = at as i64;
                        request.playlist_length = length;
                    }
                    if queue_mode {
                        request.queue_index = at as i64;
                    }
                    app.menu.set(Some(request));
                });
                view! {
                    <TrackRow track=track number=number show_cover=show_cover show_album=show_album show_number=show_number on_play=on_play on_menu=on_menu/>
                }
            })
            .collect_view()
    };
    view! {
        <div class="track-list" class:no-album=!show_album>
            <Show when=move || show_header && !shown.with(Vec::is_empty)>
                <div class="track-header">
                    {show_number.then(|| view! { <span class="number">"#"</span> })}
                    {show_cover.then(|| view! { <span class="row-cover"></span> })}
                    <span class="row-text">"TITLE"</span>
                    {show_album.then(|| view! { <span class="row-album">"ALBUM"</span> })}
                    <span class="row-like"></span>
                    <span class="row-time">"TIME"</span>
                    <span class="row-more"></span>
                </div>
            </Show>
            {rows}
        </div>
    }
}
