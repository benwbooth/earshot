//! Around every page: the top bar (search, settings), the sidebar, the
//! player bar, the song menu, the text dialog and notices, as in the Qt app.
//! On phones the sidebar becomes a row of tabs and the player bar shrinks.

use leptos::ev;
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlInputElement, KeyboardEvent, MouseEvent};

use crate::components::{Cover, Icon, IconButton, Marquee, Waveform};
use crate::state::{Prompt, album_href, app, duration, open_web, playlist_href, s, search_href, service_href, site_name, song_page, window};

#[component]
pub fn TopBar() -> impl IntoView {
    let app = app();
    let input = NodeRef::<leptos::html::Input>::new();
    let focused = RwSignal::new(false);
    let settings_open = RwSignal::new(false);
    let remember = StoredValue::new_local(None::<gloo_timers::callback::Timeout>);
    let matching = move || {
        let typed = app.query.get().trim().to_lowercase();
        app.recent_searches
            .get()
            .into_iter()
            .filter(|item| typed.is_empty() || (item.to_lowercase().contains(&typed) && item.to_lowercase() != typed))
            .take(12)
            .collect::<Vec<_>>()
    };
    let search = move |query: String| {
        app.query.set(query.clone());
        app.go(if query.trim().is_empty() { "/search".to_owned() } else { search_href(&query) });
    };
    let pick = move |query: String| {
        search(query.clone());
        app.add_search(&query);
        focused.set(false);
        if let Some(input) = input.get_untracked() {
            let _ = input.blur();
        }
    };
    let location = use_location();
    view! {
        <header class="topbar">
            <a class="brand" href="/">
                <img src="/icons/earshot-app.svg" alt=""/>
                <span>"Earshot"</span>
            </a>
            <IconButton icon="back" size=18 tip="Back (Alt+Left)" class="desktop-only"
                on_click=move |_| { let _ = window().history().map(|h| h.back()); }/>
            <IconButton icon="home" size=18 tip="Home" active=Signal::derive(move || location.pathname.get() == "/")
                class="desktop-only" on_click=move |_| app.go("/")/>
            <div class="search-box" class:focused=move || focused.get()>
                <Icon name="search" size=17 class="search-icon"/>
                <input
                    node_ref=input
                    type="search"
                    placeholder="What do you want to hear?"
                    prop:value=move || app.query.get()
                    on:focus=move |_| focused.set(true)
                    on:blur=move |_| {
                        // Let a click on a recent search land first.
                        gloo_timers::callback::Timeout::new(150, move || focused.set(false)).forget();
                    }
                    on:input=move |event| {
                        let text = event_target_value(&event);
                        search(text.clone());
                        // A search typed and left alone for a moment counts as one worth keeping.
                        remember.set_value(Some(gloo_timers::callback::Timeout::new(2000, move || app.add_search(&text))));
                    }
                    on:keydown=move |event: KeyboardEvent| match event.key().as_str() {
                        "Enter" => {
                            let text = app.query.get_untracked();
                            search(text.clone());
                            app.add_search(&text);
                        }
                        "Escape" => {
                            app.query.set(String::new());
                            if let Some(input) = input.get_untracked() { let _ = input.blur(); }
                        }
                        _ => {}
                    }
                />
                <Show when=move || !app.query.get().is_empty()>
                    <IconButton icon="close" size=14 class="search-clear" on_click=move |_| search(String::new())/>
                </Show>
                <Show when=move || focused.get() && !matching().is_empty()>
                    <div class="history-popup">
                        <div class="history-head">
                            <span>"RECENT SEARCHES"</span>
                            <button on:mousedown=move |e: MouseEvent| { e.prevent_default(); app.recent_searches.set(Vec::new()); }>"Clear all"</button>
                        </div>
                        {move || matching().into_iter().map(|item| {
                            let picked = item.clone();
                            let removed = item.clone();
                            view! {
                                <div class="history-item" on:mousedown=move |e: MouseEvent| { e.prevent_default(); pick(picked.clone()); }>
                                    <Icon name="history" size=15/>
                                    <span>{item}</span>
                                    <IconButton icon="close" size=13 tip="Remove from history"
                                        on_click=move |_| app.recent_searches.update(|items| items.retain(|i| *i != removed))/>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </Show>
            </div>
            <div class="spacer"></div>
            <IconButton
                icon=Signal::derive(move || if app.album_list.get() { "grid".to_owned() } else { "list".to_owned() })
                size=18
                tip=Signal::derive(move || if app.album_list.get() { "Show albums as a grid".to_owned() } else { "Show albums as a list".to_owned() })
                on_click=move |_| app.album_list.update(|on| *on = !*on)
            />
            <div class="settings-anchor">
                <IconButton icon="settings" size=18 tip="Settings (Ctrl+,)" on_click=move |_| settings_open.update(|open| *open = !*open)/>
                <Show when=move || settings_open.get()>
                    <div class="menu-backdrop" on:click=move |_| settings_open.set(false)></div>
                    <div class="menu settings-menu">
                        <div class="menu-item disabled"><span class="check"></span>"Now-playing card when a song starts:"</div>
                        {[("always", "Always"), ("background", "Only when Earshot isn't in front"), ("never", "Never")].into_iter().map(|(value, label)| view! {
                            <button class="menu-item" on:click=move |_| {
                                app.now_playing_card.set(value.to_owned());
                                if value != "never" { ask_notification_permission(); }
                                settings_open.set(false);
                            }>
                                <Icon name="check" size=14 class=if app.now_playing_card.get_untracked() == value { "check on" } else { "check" }/>
                                {label}
                            </button>
                        }).collect_view()}
                    </div>
                </Show>
            </div>
        </header>
    }
}

fn ask_notification_permission() {
    if web_sys::Notification::permission() == web_sys::NotificationPermission::Default {
        let _ = web_sys::Notification::request_permission();
    }
}

#[component]
fn NavItem(
    #[prop(into)] icon: String,
    #[prop(into)] label: String,
    #[prop(into)] href: String,
    #[prop(optional, into)] detail: Signal<String>,
    #[prop(optional)] tab: bool,
) -> impl IntoView {
    let location = use_location();
    let target = href.clone();
    let selected = move || {
        // The router's search part comes without its "?".
        let search = location.search.get();
        let here = if search.is_empty() || search.starts_with('?') {
            format!("{}{search}", location.pathname.get())
        } else {
            format!("{}?{search}", location.pathname.get())
        };
        if target == "/" { here == "/" } else if target.contains('?') { here == target } else { here == target || here.starts_with(&format!("{target}?")) || here.starts_with(&format!("{target}/")) }
    };
    view! {
        <a class="nav-item" class:selected=selected class:tab-item=tab href=href>
            <Icon name=icon size=18/>
            <span class="label">{label}</span>
            <span class="detail">{move || detail.get()}</span>
        </a>
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let app = app();
    let player = app.player;
    let upcoming = Signal::derive(move || {
        let left = player.queue.with(Vec::len) as i64 - player.index.get() - 1;
        if player.queue.with(Vec::is_empty) || left <= 0 { String::new() } else { left.to_string() }
    });
    let count = |n: usize| if n > 0 { n.to_string() } else { String::new() };
    let liked = Signal::derive(move || count(app.playlists().iter().find(|p| s(p, "id") == "liked").and_then(|p| p["count"].as_u64()).unwrap_or(0) as usize));
    let albums = Signal::derive(move || count(app.saved_albums().len()));
    let resizing = RwSignal::new(false);
    view! {
        <nav class="sidebar" style=move || format!("--sidebar:{}px", app.sidebar_width.get())>
            <NavItem icon="home" label="Home" href="/"/>
            <NavItem icon="search" label="Search" href="/search"/>
            <NavItem icon="sparkle" label="Made for you" href="/foryou"/>
            <NavItem icon="youtube" label="YouTube" href=service_href("youtube")/>
            <NavItem icon="radio" label="Reddit radio" href="/radio"/>
            <NavItem icon="tag" label="Bandcamp" href="/bandcamp"/>
            <NavItem icon="cloud" label="SoundCloud" href=service_href("soundcloud")/>
            <NavItem icon="mix" label="Mixcloud" href=service_href("mixcloud")/>
            <NavItem icon="archive" label="Internet Archive" href=service_href("archive")/>
            <NavItem icon="queue" label="Queue" href="/queue" detail=upcoming/>
            <div class="nav-heading">"YOUR LIBRARY"</div>
            <NavItem icon="album" label="Albums" href="/albums" detail=albums/>
            <NavItem icon="heart" label="Liked songs" href=playlist_href("liked") detail=liked/>
            <div class="nav-heading with-buttons">
                <span>"PLAYLISTS"</span>
                <IconButton icon="link" size=15 tip="Import a playlist from YouTube, SoundCloud or Bandcamp" on_click=move |_| app.prompt.set(Some(Prompt::Import))/>
                <IconButton icon="plus" size=16 tip="New playlist" on_click=move |_| app.prompt.set(Some(Prompt::NewPlaylist(Vec::new())))/>
            </div>
            <div class="playlists">
                {move || {
                    let mine: Vec<Value> = app.playlists().into_iter().filter(|p| !p["special"].as_bool().unwrap_or(false)).collect();
                    if mine.is_empty() {
                        view! { <p class="sidebar-hint">"Make a playlist with +, or add songs from any list with the ··· button."</p> }.into_any()
                    } else {
                        mine.into_iter().map(|p| {
                            let n = p["count"].as_u64().unwrap_or(0) as usize;
                            view! { <NavItem icon="playlist" label=s(&p, "name") href=playlist_href(&s(&p, "id")) detail=count(n)/> }
                        }).collect_view().into_any()
                    }
                }}
            </div>
            // Drag the edge to make the sidebar wider or narrower.
            <div class="sidebar-handle" class:active=move || resizing.get()
                on:pointerdown=move |event: web_sys::PointerEvent| {
                    resizing.set(true);
                    if let Some(target) = event.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                        let _ = target.set_pointer_capture(event.pointer_id());
                    }
                }
                on:pointermove=move |event: web_sys::PointerEvent| {
                    if resizing.get_untracked() {
                        app.sidebar_width.set((event.client_x() as f64).clamp(200.0, 420.0));
                    }
                }
                on:pointerup=move |_| resizing.set(false)
            ></div>
        </nav>
    }
}

/// The tabs along the bottom on phones.
#[component]
pub fn TabBar() -> impl IntoView {
    view! {
        <nav class="tabbar">
            <NavItem icon="home" label="Home" href="/" tab=true/>
            <NavItem icon="search" label="Search" href="/search" tab=true/>
            <NavItem icon="sparkle" label="For you" href="/foryou" tab=true/>
            <NavItem icon="radio" label="Radio" href="/radio" tab=true/>
            <NavItem icon="library" label="Library" href="/library" tab=true/>
        </nav>
    }
}

/// The seek bar and volume slider.
#[component]
fn Slider(
    #[prop(into)] value: Signal<f64>,
    #[prop(into)] max: Signal<f64>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] dragging: Option<RwSignal<Option<f64>>>,
    #[prop(into)] on_change: Callback<f64>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let local = dragging.unwrap_or_else(|| RwSignal::new(None));
    let shown = move || local.get().unwrap_or_else(|| value.get());
    view! {
        <input
            type="range"
            class=format!("slider {class}")
            min="0"
            step="any"
            prop:max=move || max.get().max(0.0001)
            prop:value=move || shown()
            disabled=move || disabled.get()
            style=move || format!("--fill:{}%", (shown() / max.get().max(0.0001) * 100.0).clamp(0.0, 100.0))
            on:input=move |event| {
                let v: f64 = event_target_value(&event).parse().unwrap_or(0.0);
                local.set(Some(v));
                if dragging.is_none() { on_change.run(v); }
            }
            on:change=move |event| {
                let v: f64 = event_target_value(&event).parse().unwrap_or(0.0);
                local.set(None);
                on_change.run(v);
            }
        />
    }
}

#[component]
pub fn PlayerBar() -> impl IntoView {
    let app = app();
    let player = app.player;
    let track = Memo::new(move |_| player.current());
    let has = move || track.with(Option::is_some);
    let liked = move || track.with(|t| t.as_ref().is_some_and(|t| app.is_liked(&s(t, "key"))));
    let page = Memo::new(move |_| {
        track.with(|t| t.as_ref().map(|t| {
            let found = player.found.with(|f| f.get(&s(t, "key")).and_then(|v| v.first().cloned()));
            song_page(t, found.as_ref())
        }))
        .unwrap_or_default()
    });
    let seeking = RwSignal::new(None);
    let location = use_location();
    let on_queue = move || location.pathname.get() == "/queue";
    view! {
        <footer class="playerbar">
            <div class="song">
                <Show when=has>
                    <button class="bar-cover" on:click=move |_| player.open_source() title="Go to what's playing">
                        <Cover src=Signal::derive(move || track.with(|t| t.as_ref().map(|t| s(t, "cover")).unwrap_or_default()))
                            seed=Signal::derive(move || track.with(|t| t.as_ref().map(|t| s(t, "key")).unwrap_or_default())) radius=8/>
                    </button>
                </Show>
                <div class="song-text">
                    <div class="song-title-row">
                        <Show when=has><Waveform/></Show>
                        <button class="song-title" on:click=move |_| if has() { player.open_source() }>
                            <Marquee text=Signal::derive(move || track.with(|t| t.as_ref().map(|t| s(t, "title")).unwrap_or_else(|| "Nothing playing".to_owned())))/>
                        </button>
                    </div>
                    {move || {
                        let artist = track.with(|t| t.as_ref().map(|t| (s(t, "artist"), s(t, "artistId"))));
                        match artist {
                            Some((name, id)) if !id.is_empty() => view! { <a class="song-artist" href=crate::state::artist_href(&id)><Marquee text=name/></a> }.into_any(),
                            Some((name, _)) => view! { <div class="song-artist"><Marquee text=name/></div> }.into_any(),
                            None => view! { <div class="song-artist"><Marquee text="Pick a song, album or radio station"/></div> }.into_any(),
                        }
                    }}
                    <Show when=move || has() && !player.context.get().is_empty()>
                        <div class="song-context"><Marquee text=Signal::derive(move || format!("From {}", player.context.get()))/></div>
                    </Show>
                </div>
                <Show when=has>
                    <IconButton
                        icon=Signal::derive(move || if liked() { "heart-filled".to_owned() } else { "heart".to_owned() })
                        size=18
                        class=if true { "like" } else { "" }
                        tip=Signal::derive(move || if liked() { "Remove from Liked songs".to_owned() } else { "Add to Liked songs".to_owned() })
                        on_click=move |_| if let Some(t) = track.get_untracked() { app.set_liked(&t, !liked()) }
                    />
                    <Show when=move || !page.get().is_empty()>
                        <IconButton icon="external" size=17 class="desktop-only"
                            tip=Signal::derive(move || format!("Open on {}", site_name(&page.get())))
                            on_click=move |_| open_web(&page.get_untracked())/>
                    </Show>
                </Show>
            </div>

            <div class="controls">
                <div class="buttons">
                    <IconButton icon="shuffle" size=18 class="desktop-only" active=Signal::derive(move || player.shuffle.get())
                        tip=Signal::derive(move || if player.shuffle.get() { "Shuffle on".to_owned() } else { "Shuffle off".to_owned() })
                        on_click=move |_| player.set_shuffle(!player.shuffle.get_untracked())/>
                    <IconButton icon="previous" size=20 class="bright" tip="Previous" disabled=Signal::derive(move || !has()) on_click=move |_| player.previous()/>
                    <div class="play-holder">
                        <Show when=move || !player.loading.get() fallback=|| view! { <div class="spinner small"></div> }>
                            <button class="play-button" disabled=move || !has()
                                title=move || if player.playing() { "Pause" } else { "Play" }
                                on:click=move |_| player.toggle()>
                                <Icon name=Signal::derive(move || if player.playing() { "pause".to_owned() } else { "play".to_owned() }) size=20/>
                            </button>
                        </Show>
                    </div>
                    <IconButton icon="next" size=20 class="bright" tip="Next" disabled=Signal::derive(move || !player.has_next()) on_click=move |_| player.next(false)/>
                    <IconButton
                        icon=Signal::derive(move || if player.repeat.get() == 2 { "repeat-one".to_owned() } else { "repeat".to_owned() })
                        size=18 class="desktop-only"
                        active=Signal::derive(move || player.repeat.get() > 0)
                        tip=Signal::derive(move || ["Repeat off", "Repeat all", "Repeat this song"][player.repeat.get() as usize % 3].to_owned())
                        on_click=move |_| player.cycle_repeat()/>
                    <IconButton icon="dice" size=18 class="desktop-only" active=Signal::derive(move || player.radio.get())
                        tip=Signal::derive(move || if player.radio.get() {
                            "Random radio is on: songs like these keep playing when the queue ends".to_owned()
                        } else {
                            "Random radio: keep playing songs like these when the queue ends".to_owned()
                        })
                        on_click=move |_| player.radio.update(|on| *on = !*on)/>
                </div>
                <div class="seek">
                    <span class="time">{move || duration(seeking.get().unwrap_or_else(|| player.position()))}</span>
                    <Slider
                        value=Signal::derive(move || player.position())
                        max=Signal::derive(move || player.length().max(1.0))
                        disabled=Signal::derive(move || !has() || player.length() <= 0.0)
                        dragging=seeking
                        on_change=move |v| player.seek(v)
                    />
                    <span class="time end">{move || if player.length() > 0.0 { duration(player.length()) } else { "-:--".to_owned() }}</span>
                </div>
                <Show when=move || !player.error.get().is_empty() && !player.loading.get()>
                    <div class="player-error">{move || player.error.get()}</div>
                </Show>
            </div>

            <div class="volume">
                <IconButton icon="queue" size=18 tip="Queue" active=Signal::derive(on_queue)
                    on_click=move |_| if on_queue() { let _ = window().history().map(|h| h.back()); } else { app.go("/queue") }/>
                <IconButton
                    icon=Signal::derive(move || if player.muted.get() || player.volume.get() == 0.0 { "volume-mute".to_owned() } else { "volume".to_owned() })
                    size=18 class="desktop-only"
                    tip=Signal::derive(move || if player.muted.get() { "Unmute".to_owned() } else { "Mute".to_owned() })
                    on_click=move |_| player.muted.update(|m| *m = !*m)/>
                <Slider class="volume-slider desktop-only" value=Signal::derive(move || player.volume.get()) max=1.0
                    on_change=move |v| { player.volume.set(v); player.muted.set(false); }/>
            </div>
            <div class="mobile-progress" style=move || format!("--fill:{}%", (player.position() / player.length().max(1.0) * 100.0).clamp(0.0, 100.0))></div>
        </footer>
    }
}

/// The song menu: Play next, Add to queue, Add to playlist, Like, Go to album…
#[component]
pub fn TrackMenu() -> impl IntoView {
    let app = app();
    let player = app.player;
    let submenu = RwSignal::new(false);
    let close = move || {
        submenu.set(false);
        app.menu.set(None);
    };
    view! {
        {move || app.menu.get().map(|request| {
            let tracks = request.tracks.clone();
            let one = if tracks.len() == 1 { Some(tracks[0].clone()) } else { None };
            let liked = one.as_ref().is_some_and(|t| app.is_liked(&s(t, "key")));
            let found = one.as_ref().and_then(|t| player.found.with_untracked(|f| f.get(&s(t, "key")).and_then(|v| v.first().cloned())));
            let web = one.as_ref().map(|t| song_page(t, found.as_ref())).unwrap_or_default();
            let width = window().inner_width().ok().and_then(|w| w.as_f64()).unwrap_or(1200.0);
            let height = window().inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(800.0);
            let x = (request.x - 240.0).clamp(8.0, width - 248.0);
            let y = request.y.min(height - 380.0).max(8.0);
            let (t1, t2, t3, t4, t5, t6) = (tracks.clone(), tracks.clone(), tracks.clone(), one.clone(), one.clone(), one.clone());
            let (playlist_id, index, length, queue_index) = (request.playlist_id.clone(), request.playlist_index, request.playlist_length, request.queue_index);
            let (pid1, pid2, pid3) = (playlist_id.clone(), playlist_id.clone(), playlist_id.clone());
            view! {
                <div class="menu-backdrop" on:click=move |_| close() on:contextmenu=move |e: MouseEvent| { e.prevent_default(); close(); }></div>
                <div class="menu track-menu" style=format!("left:{x}px;top:{y}px")>
                    <button class="menu-item" on:click=move |_| { player.play_next(t1.clone()); close(); }>
                        {if tracks.len() > 1 { "Play all next" } else { "Play next" }}
                    </button>
                    <button class="menu-item" on:click=move |_| { player.enqueue(t2.clone()); close(); }>"Add to queue"</button>
                    {(!web.is_empty()).then(|| {
                        let url = web.clone();
                        view! { <button class="menu-item" on:click=move |_| { open_web(&url); close(); }>{format!("Open on {}", site_name(&web))}</button> }
                    })}
                    <div class="menu-item has-sub" on:click=move |_| submenu.update(|o| *o = !*o)>
                        "Add to playlist" <Icon name="forward" size=14/>
                        <Show when=move || submenu.get()>
                            <div class="menu submenu">
                                {
                                    let t = t3.clone();
                                    view! { <button class="menu-item" on:click=move |_| { app.prompt.set(Some(Prompt::NewPlaylist(t.clone()))); close(); }>"New playlist…"</button> }
                                }
                                <div class="menu-separator"></div>
                                {
                                    let t = t3.clone();
                                    move || app.playlists().into_iter().filter(|p| s(p, "id") != "liked").map(|p| {
                                        let (id, name, songs) = (s(&p, "id"), s(&p, "name"), t.clone());
                                        let label = name.clone();
                                        view! { <button class="menu-item" on:click=move |_| { app.add_to_playlist(id.clone(), name.clone(), songs.clone()); close(); }>{label}</button> }
                                    }).collect_view()
                                }
                            </div>
                        </Show>
                    </div>
                    {t4.map(|t| view! {
                        <button class="menu-item" on:click=move |_| { app.set_liked(&t, !liked); close(); }>
                            {if liked { "Remove from Liked songs" } else { "Add to Liked songs" }}
                        </button>
                    })}
                    <div class="menu-separator"></div>
                    {t5.filter(|t| !s(t, "albumId").is_empty()).map(|t| {
                        let href = album_href(&s(&t, "albumId"));
                        view! { <button class="menu-item" on:click=move |_| { app.go(href.clone()); close(); }>"Go to album"</button> }
                    })}
                    {t6.clone().filter(|t| !s(t, "artistId").is_empty()).map(|t| {
                        let href = crate::state::artist_href(&s(&t, "artistId"));
                        view! { <button class="menu-item" on:click=move |_| { app.go(href.clone()); close(); }>"Go to artist"</button> }
                    })}
                    {t6.filter(|t| !s(t, "link").is_empty()).map(|t| {
                        let link = s(&t, "link");
                        view! { <button class="menu-item" on:click=move |_| { open_web(&link); close(); }>"Open on Reddit"</button> }
                    })}
                    {(!playlist_id.is_empty() && index > 0).then(|| view! {
                        <button class="menu-item" on:click=move |_| { app.change("moveInPlaylist", json!({ "id": pid1, "from": index, "to": index - 1 })); close(); }>"Move up"</button>
                    })}
                    {(!playlist_id.is_empty() && index >= 0 && (index as usize) + 1 < length).then(|| view! {
                        <button class="menu-item" on:click=move |_| { app.change("moveInPlaylist", json!({ "id": pid2, "from": index, "to": index + 1 })); close(); }>"Move down"</button>
                    })}
                    {(!playlist_id.is_empty() && index >= 0).then(|| view! {
                        <button class="menu-item" on:click=move |_| { app.change("removeFromPlaylist", json!({ "id": pid3, "index": index })); close(); }>"Remove from this playlist"</button>
                    })}
                    {(queue_index >= 0 && queue_index != player.index.get_untracked()).then(|| view! {
                        <button class="menu-item" on:click=move |_| { player.remove_at(queue_index); close(); }>"Remove from queue"</button>
                    })}
                </div>
            }
        })}
    }
}

/// A question with a text field: a playlist's name, or a link to import.
#[component]
pub fn PromptDialog() -> impl IntoView {
    let app = app();
    let text = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        let prompt = app.prompt.get();
        text.set(match &prompt {
            Some(Prompt::Rename(_, name)) => name.clone(),
            _ => String::new(),
        });
        if prompt.is_some() {
            gloo_timers::callback::Timeout::new(30, move || {
                if let Some(input) = input.get_untracked() {
                    let _ = input.focus();
                    input.select();
                }
            })
            .forget();
        }
    });
    let accept = move || {
        let value = text.get_untracked().trim().to_owned();
        let Some(prompt) = app.prompt.get_untracked() else { return };
        if value.is_empty() {
            return;
        }
        match prompt {
            Prompt::NewPlaylist(tracks) => {
                let count = tracks.len();
                leptos::task::spawn_local(async move {
                    if let Ok(id) = crate::api::call("createPlaylist", json!({ "name": value, "tracks": tracks })).await {
                        app.revision.update(|r| *r += 1);
                        if count > 0 {
                            app.notify(format!("Added {} to {value}", if count == 1 { "1 song".to_owned() } else { format!("{count} songs") }));
                        } else if let Some(id) = id.as_str() {
                            app.go(playlist_href(id));
                        }
                    }
                });
                app.prompt.set(None);
            }
            Prompt::Rename(id, _) => {
                app.change("renamePlaylist", json!({ "id": id, "name": value }));
                app.prompt.set(None);
            }
            Prompt::Import => {
                busy.set(true);
                app.notify("Importing…");
                leptos::task::spawn_local(async move {
                    match crate::api::call("importPlaylist", json!({ "url": value })).await {
                        Ok(found) => {
                            let tracks = found["tracks"].clone();
                            let name = s(&found, "title");
                            let count = tracks.as_array().map(Vec::len).unwrap_or(0);
                            if let Ok(id) = crate::api::call("createPlaylist", json!({ "name": if name.is_empty() { "Imported playlist".to_owned() } else { name.clone() }, "tracks": tracks })).await {
                                app.revision.update(|r| *r += 1);
                                app.notify(format!("Imported {count} songs"));
                                if let Some(id) = id.as_str() {
                                    app.go(playlist_href(id));
                                }
                            }
                            app.prompt.set(None);
                        }
                        Err(message) => app.notify(message),
                    }
                    busy.set(false);
                });
            }
        }
    };
    let title = move || match app.prompt.get() {
        Some(Prompt::NewPlaylist(_)) => "New playlist",
        Some(Prompt::Rename(..)) => "Rename playlist",
        Some(Prompt::Import) => "Import a playlist",
        None => "",
    };
    let hint = move || match app.prompt.get() {
        Some(Prompt::Import) => "A link to a YouTube or YouTube Music playlist, a SoundCloud or Mixcloud playlist, a Bandcamp album or an Internet Archive item.",
        _ => "",
    };
    view! {
        <Show when=move || app.prompt.get().is_some()>
            <div class="dialog-backdrop" on:click=move |_| app.prompt.set(None)>
                <div class="dialog" on:click=|e: MouseEvent| e.stop_propagation()>
                    <h3>{title}</h3>
                    <Show when=move || !hint().is_empty()><p>{hint}</p></Show>
                    <input node_ref=input type="text"
                        placeholder=move || if matches!(app.prompt.get(), Some(Prompt::Import)) { "https://…" } else { "Name" }
                        prop:value=move || text.get()
                        on:input=move |e| text.set(event_target_value(&e))
                        on:keydown=move |e: KeyboardEvent| match e.key().as_str() {
                            "Enter" => accept(),
                            "Escape" => app.prompt.set(None),
                            _ => {}
                        }/>
                    <div class="dialog-buttons">
                        <button class="pill" on:click=move |_| app.prompt.set(None)>"Cancel"</button>
                        <button class="pill primary" disabled=move || busy.get() || text.get().trim().is_empty() on:click=move |_| accept()>
                            {move || match app.prompt.get() { Some(Prompt::Import) => if busy.get() { "Importing…" } else { "Import" }, Some(Prompt::Rename(..)) => "Rename", _ => "Create" }}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
pub fn Toast() -> impl IntoView {
    let app = app();
    view! {
        {move || app.toast.get().map(|(_, message)| view! { <div class="toast">{message}</div> })}
    }
}

/// The browser's own notification when a song starts (as the Qt app's
/// now-playing card), plus the phone's lock-screen controls and keyboard shortcuts.
pub fn integrate() {
    let app = app();
    let player = app.player;
    crate::levels::run(player);
    // Notices and the now-playing card are system notifications; browsers
    // only let a page ask for them on a click.
    let ask = Closure::<dyn Fn()>::new(ask_notification_permission);
    let once = web_sys::AddEventListenerOptions::new();
    once.set_once(true);
    let _ = window().add_event_listener_with_callback_and_add_event_listener_options("pointerdown", ask.as_ref().unchecked_ref(), &once);
    ask.forget();
    Effect::new(move |previous: Option<u32>| {
        let count = player.started_count.get();
        if previous.is_some_and(|p| p != count) {
            if let Some(track) = player.current_untracked() {
                announce(&track, &app.now_playing_card.get_untracked());
                media_session(&track);
            }
        }
        count
    });
    Effect::new(move |_| {
        let title = player.current().map(|t| format!("{} · {} — Earshot", s(&t, "title"), s(&t, "artist"))).unwrap_or_else(|| "Earshot".to_owned());
        if let Some(document) = window().document() {
            document.set_title(&title);
        }
    });
    Effect::new(move |_| {
        let state = if player.playing() { "playing" } else { "paused" };
        if let Some(session) = media_session_object() {
            let _ = js_sys::Reflect::set(&session, &"playbackState".into(), &state.into());
        }
    });
    if let Some(session) = media_session_object() {
        let set_handler = js_sys::Reflect::get(&session, &"setActionHandler".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok());
        for (action, run) in [
            ("play", Box::new(move || player.toggle()) as Box<dyn Fn()>),
            ("pause", Box::new(move || player.toggle())),
            ("nexttrack", Box::new(move || player.next(false))),
            ("previoustrack", Box::new(move || player.previous())),
        ] {
            let handler = Closure::<dyn Fn()>::new(run);
            if let Some(set_handler) = &set_handler {
                let _ = set_handler.call2(&session, &action.into(), handler.as_ref());
            }
            handler.forget();
        }
    }
    let _ = window_event_listener(ev::keydown, move |event: KeyboardEvent| {
        let typing = event.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()).is_some();
        let ctrl = event.ctrl_key() || event.meta_key();
        match event.key().as_str() {
            " " if !typing => {
                event.prevent_default();
                player.toggle();
            }
            "ArrowRight" if ctrl => player.next(false),
            "ArrowLeft" if ctrl => player.previous(),
            "ArrowLeft" if event.alt_key() => {
                let _ = window().history().map(|h| h.back());
            }
            "f" | "F" if ctrl => {
                event.prevent_default();
                if let Some(input) = window().document().and_then(|d| d.query_selector(".search-box input").ok().flatten()) {
                    let _ = input.unchecked_into::<web_sys::HtmlElement>().focus();
                }
            }
            "n" | "N" if ctrl && !typing => {
                event.prevent_default();
                app.prompt.set(Some(Prompt::NewPlaylist(Vec::new())));
            }
            "s" | "S" if ctrl && !typing => {
                event.prevent_default();
                player.set_shuffle(!player.shuffle.get_untracked());
            }
            "r" | "R" if ctrl && !typing => {
                event.prevent_default();
                player.cycle_repeat();
            }
            "," if ctrl => {
                event.prevent_default();
                if let Some(button) = window().document().and_then(|d| d.query_selector(".settings-anchor button").ok().flatten()) {
                    let _ = button.unchecked_into::<web_sys::HtmlElement>().click();
                }
            }
            _ => {}
        }
    });
}

fn announce(track: &Value, when: &str) {
    let hidden = window().document().map(|d| d.hidden()).unwrap_or(false);
    if when == "never" || (when == "background" && !hidden) {
        return;
    }
    if web_sys::Notification::permission() != web_sys::NotificationPermission::Granted {
        return;
    }
    let options = web_sys::NotificationOptions::new();
    options.set_body(&format!("{}{}", s(track, "artist"), if s(track, "album").is_empty() { String::new() } else { format!(" · {}", s(track, "album")) }));
    options.set_icon(&s(track, "cover"));
    options.set_tag("earshot-now-playing");
    options.set_silent(Some(true));
    let _ = web_sys::Notification::new_with_options(&s(track, "title"), &options);
}

/// navigator.mediaSession, through the browser's own objects (the Rust
/// bindings still call it experimental).
fn media_session_object() -> Option<JsValue> {
    let session = js_sys::Reflect::get(&window().navigator(), &"mediaSession".into()).ok()?;
    (!session.is_undefined() && !session.is_null()).then_some(session)
}

/// The phone's lock screen and the desktop's media controls show the song.
fn media_session(track: &Value) {
    let Some(session) = media_session_object() else { return };
    let Some(constructor) = js_sys::Reflect::get(&window(), &"MediaMetadata".into()).ok().and_then(|c| c.dyn_into::<js_sys::Function>().ok()) else { return };
    let cover = s(track, "coverBig");
    let cover = if cover.is_empty() { s(track, "cover") } else { cover };
    let init = serde_json::json!({
        "title": s(track, "title"),
        "artist": s(track, "artist"),
        "album": s(track, "album"),
        "artwork": if cover.is_empty() { serde_json::json!([]) } else { serde_json::json!([{ "src": cover }]) },
    });
    let Ok(init) = js_sys::JSON::parse(&init.to_string()) else { return };
    if let Ok(metadata) = js_sys::Reflect::construct(&constructor, &js_sys::Array::of1(&init)) {
        let _ = js_sys::Reflect::set(&session, &"metadata".into(), &metadata);
    }
}
