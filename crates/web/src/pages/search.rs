//! Search: the catalogue's top result, songs, albums and artists, YouTube's
//! playlists and videos (loading more as you scroll, like youtube.com), and
//! Bandcamp, SoundCloud, Mixcloud and the Internet Archive
//! (qml/SearchPage.qml). The query and tab are in the address:
//! /search?q=…&tab=youtube.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::{use_navigate, use_query_map};
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::api;
use crate::components::{AlbumCard, Cover, PillButton, PlayButton, SectionHeader, Shelf, StatusView, TrackList};
use crate::state::{album_href, app, artist_href, bandcamp_tag_href, encode, list, long_duration, num, s, search_href, window};

const TABS: [(&str, &str); 9] = [
    ("all", "All"),
    ("songs", "Songs"),
    ("albums", "Albums"),
    ("artists", "Artists"),
    ("youtube", "YouTube"),
    ("bandcamp", "Bandcamp"),
    ("soundcloud", "SoundCloud"),
    ("mixcloud", "Mixcloud"),
    ("archive", "Internet Archive"),
];

/// YouTube pages fetched at most for one search.
const YT_MAX_PAGES: u32 = 15;

/// Same as the shelves' grid (168 px cards, 24 px apart).
const CARD: i32 = 168;
const GAP: i32 = 24;

fn tab_href(query: &str, tab: &str) -> String {
    if tab == "all" { search_href(query) } else { format!("{}&tab={tab}", search_href(query)) }
}

fn image(value: &Value, big: &str, small: &str) -> String {
    let big = s(value, big);
    if big.is_empty() { s(value, small) } else { big }
}

fn yt_album_subtitle(album: &Value) -> String {
    let artist = s(album, "artist");
    let by = if artist.is_empty() { String::new() } else { format!("{artist} · ") };
    let count = num(album, "trackCount") as u64;
    if s(album, "kind") == "playlist" {
        return format!("{by}Playlist{}", if count > 0 { format!(" · {count} videos") } else { String::new() });
    }
    format!("{by}{}{}", if count > 1 { format!("{count} tracks · ") } else { String::new() }, long_duration(num(album, "duration")))
}

fn album_card(album: &Value, subtitle: String) -> impl IntoView + use<> {
    let app = app();
    let (id, title) = (s(album, "id"), s(album, "title"));
    let (play_id, play_title) = (id.clone(), title.clone());
    view! {
        <AlbumCard title=title subtitle=subtitle image=image(album, "coverBig", "cover") seed=id.clone() href=album_href(&id) album_id=id.clone()
            on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
    }
}

/// The page's scroller (the shell's main area).
fn scroller() -> Option<web_sys::Element> {
    window().document()?.query_selector("main.content").ok().flatten()
}

thread_local! {
    // The scroll listener of the search page on screen, removed when it goes.
    static SCROLL: RefCell<Option<(u32, Closure<dyn Fn(web_sys::Event)>)>> = const { RefCell::new(None) };
    static SCROLL_ID: Cell<u32> = const { Cell::new(0) };
}

fn stop_listening(id: Option<u32>) {
    SCROLL.with(|current| {
        let mut current = current.borrow_mut();
        if current.as_ref().is_some_and(|(mine, _)| id.is_none_or(|id| id == *mine)) {
            if let Some((_, closure)) = current.take() {
                let _ = window().remove_event_listener_with_callback_and_bool("scroll", closure.as_ref().unchecked_ref(), true);
            }
        }
    });
}

/// Calls `on_scroll` whenever the page scrolls. The main area's scroll
/// events don't bubble, so the window catches them on the way down.
fn listen_for_scroll(on_scroll: impl Fn() + 'static) {
    stop_listening(None);
    let id = SCROLL_ID.with(|n| {
        n.set(n.get() + 1);
        n.get()
    });
    let closure = Closure::<dyn Fn(web_sys::Event)>::new(move |_| on_scroll());
    let _ = window().add_event_listener_with_callback_and_bool("scroll", closure.as_ref().unchecked_ref(), true);
    SCROLL.with(|current| *current.borrow_mut() = Some((id, closure)));
    on_cleanup(move || stop_listening(Some(id)));
}

/// One of the other services: a shelf of albums or playlists and a list of songs.
#[derive(Clone, Copy)]
struct Service {
    key: &'static str,
    name: &'static str,
    method: &'static str,
    /// { albums, tracks (, tags) }, or None while searching.
    found: RwSignal<Option<Value>>,
    failure: RwSignal<String>,
}

impl Service {
    fn new(key: &'static str, name: &'static str, method: &'static str) -> Self {
        Self { key, name, method, found: RwSignal::new(None), failure: RwSignal::new(String::new()) }
    }
}

/// Everything the page shows, as signals, so callbacks can share it.
#[derive(Clone, Copy)]
struct Search {
    /// The search being shown (trimmed; settles 450 ms after typing stops).
    query: RwSignal<String>,
    tab: Memo<String>,
    /// The catalogue's { tracks, albums, artists }.
    results: RwSignal<Option<Value>>,
    loading: RwSignal<bool>,
    error: RwSignal<String>,
    /// YouTube's videos and its playlists and full-length videos, a page at a time.
    youtube: RwSignal<Option<Vec<Value>>>,
    youtube_error: RwSignal<String>,
    yt_albums: RwSignal<Option<Vec<Value>>>,
    yt_albums_loading: RwSignal<bool>,
    yt_next: RwSignal<String>,
    yt_more_loading: RwSignal<bool>,
    yt_pages: RwSignal<u32>,
    services: [Service; 4],
}

impl Search {
    /// Still the search `q`? (Answers to older searches are dropped.)
    fn current(self, q: &str) -> bool {
        self.query.try_get_untracked().is_some_and(|query| query == q)
    }

    fn run(self) {
        let q = self.query.get_untracked();
        self.youtube.set(None);
        self.youtube_error.set(String::new());
        self.error.set(String::new());
        if q.is_empty() {
            self.results.set(None);
            self.loading.set(false);
        } else {
            // The last results stay on screen until the new ones come.
            self.loading.set(true);
            let query = q.clone();
            spawn_local(async move {
                let result = api::call("search", json!({ "query": query })).await;
                if !self.current(&query) {
                    return;
                }
                match result {
                    Ok(found) => self.results.set(Some(found)),
                    Err(message) => self.error.set(message),
                }
                self.loading.set(false);
            });
        }
        self.run_youtube_albums(&q);
        self.run_services(&q);
    }

    // Playlists and full-length videos (soundtracks, live sets) that work as albums.
    fn run_youtube_albums(self, q: &str) {
        self.yt_albums.set(None);
        self.yt_next.set(String::new());
        self.yt_pages.set(0);
        self.yt_more_loading.set(false);
        self.yt_albums_loading.set(false);
        if q.is_empty() {
            return;
        }
        self.yt_albums_loading.set(true);
        let q = q.to_owned();
        spawn_local(async move {
            let result = api::call("searchYoutubeAlbums", json!({ "query": q, "next": "", "preferMusic": app().prefer_music.get_untracked() })).await;
            if !self.current(&q) {
                return;
            }
            match result {
                Ok(found) => {
                    self.yt_albums.set(Some(list(&found, "albums")));
                    self.youtube.set(Some(list(&found, "tracks")));
                    self.yt_next.set(s(&found, "next"));
                    self.yt_pages.set(1);
                    self.yt_albums_loading.set(false);
                    // YouTube's first page is short; fetch the second straight away.
                    self.load_more_youtube();
                }
                Err(message) => {
                    self.yt_albums.set(Some(Vec::new()));
                    self.youtube.set(Some(Vec::new()));
                    self.youtube_error.set(message);
                    self.yt_albums_loading.set(false);
                }
            }
        });
    }

    /// The next page of YouTube results, added after what's shown (repeats skipped).
    fn load_more_youtube(self) {
        let next = self.yt_next.get_untracked();
        if next.is_empty() || self.yt_more_loading.get_untracked() || self.yt_albums_loading.get_untracked() || self.yt_pages.get_untracked() >= YT_MAX_PAGES {
            return;
        }
        self.yt_more_loading.set(true);
        let q = self.query.get_untracked();
        spawn_local(async move {
            let result = api::call("searchYoutubeAlbums", json!({ "query": q, "next": next, "preferMusic": app().prefer_music.get_untracked() })).await;
            if !self.current(&q) {
                return;
            }
            match result {
                Ok(found) => {
                    let mut seen: HashSet<String> = HashSet::new();
                    self.yt_albums.with_untracked(|albums| seen.extend(albums.iter().flatten().map(|a| s(a, "id"))));
                    self.youtube.with_untracked(|tracks| seen.extend(tracks.iter().flatten().map(|t| s(t, "key"))));
                    let albums: Vec<Value> = list(&found, "albums").into_iter().filter(|a| !seen.contains(&s(a, "id"))).collect();
                    let tracks: Vec<Value> = list(&found, "tracks").into_iter().filter(|t| !seen.contains(&s(t, "key"))).collect();
                    self.yt_albums.update(|all| all.get_or_insert_with(Vec::new).extend(albums));
                    self.youtube.update(|all| all.get_or_insert_with(Vec::new).extend(tracks));
                    self.yt_next.set(s(&found, "next"));
                    self.yt_pages.update(|n| *n += 1);
                    self.yt_more_loading.set(false);
                    self.load_more_soon();
                }
                Err(_) => {
                    self.yt_more_loading.set(false);
                    self.yt_next.set(String::new());
                }
            }
        });
    }

    /// The YouTube and Albums tabs keep going, like youtube.com, as you near the end.
    fn load_more_if_near_end(self) {
        let Some(tab) = self.tab.try_get_untracked() else { return };
        if tab != "youtube" && tab != "albums" {
            return;
        }
        if let Some(view) = scroller() {
            if view.scroll_top() + view.client_height() > view.scroll_height() - 1200 {
                self.load_more_youtube();
            }
        }
    }

    /// The same, once the page has been laid out with what just arrived.
    fn load_more_soon(self) {
        Timeout::new(60, move || self.load_more_if_near_end()).forget();
    }

    fn run_services(self, q: &str) {
        for service in self.services {
            service.found.set(None);
            service.failure.set(String::new());
            if q.is_empty() {
                continue;
            }
            let q = q.to_owned();
            spawn_local(async move {
                let result = api::call(service.method, json!({ "query": q })).await;
                if !self.current(&q) {
                    return;
                }
                match result {
                    Ok(found) => service.found.set(Some(found)),
                    Err(message) => {
                        service.found.set(Some(json!({ "albums": [], "tracks": [], "tags": [] })));
                        service.failure.set(message);
                    }
                }
            });
        }
    }
}

#[component]
pub fn SearchPage() -> impl IntoView {
    let app = app();
    let params = use_query_map();
    let url_query = Memo::new(move |_| params.with(|p| p.get("q")));
    let tab = Memo::new(move |_| {
        let tab = params.with(|p| p.get("tab")).unwrap_or_default();
        if TABS.iter().any(|(key, _)| *key == tab) { tab } else { "all".to_owned() }
    });
    let search = Search {
        query: RwSignal::new(String::new()),
        tab,
        results: RwSignal::new(None),
        loading: RwSignal::new(false),
        error: RwSignal::new(String::new()),
        youtube: RwSignal::new(None),
        youtube_error: RwSignal::new(String::new()),
        yt_albums: RwSignal::new(None),
        yt_albums_loading: RwSignal::new(false),
        yt_next: RwSignal::new(String::new()),
        yt_more_loading: RwSignal::new(false),
        yt_pages: RwSignal::new(0),
        services: [
            Service::new("bandcamp", "Bandcamp", "searchBandcamp"),
            Service::new("soundcloud", "SoundCloud", "searchSoundcloud"),
            Service::new("mixcloud", "Mixcloud", "searchMixcloud"),
            Service::new("archive", "the Internet Archive", "searchArchive"),
        ],
    };
    let query = search.query;

    // The address's query, searched 450 ms after typing stops (at once when the page opens).
    let navigate = use_navigate();
    let started = StoredValue::new(false);
    let debounce = StoredValue::new_local(None::<Timeout>);
    Effect::new(move |_| {
        let text = match url_query.get() {
            Some(text) => text,
            None => {
                // Opened from the sidebar: show the last search, like the Qt app.
                let last = app.query.get_untracked();
                if !started.get_value() && !last.trim().is_empty() {
                    navigate(&search_href(&last), NavigateOptions { replace: true, ..Default::default() });
                    return;
                }
                String::new()
            }
        };
        // The top bar shows what's searched, also when the page is opened by its address.
        if app.query.get_untracked() != text {
            app.query.set(text.clone());
        }
        let text = text.trim().to_owned();
        if !started.get_value() || text.is_empty() {
            started.set_value(true);
            debounce.set_value(None);
            query.set(text);
        } else {
            debounce.set_value(Some(Timeout::new(450, move || {
                let _ = query.try_set(text);
            })));
        }
    });
    let retry = RwSignal::new(0u32);
    Effect::new(move |_| {
        query.track();
        retry.track();
        search.run();
    });

    // A new tab starts at the top; the YouTube and Albums tabs may need more at once.
    Effect::new(move |previous: Option<()>| {
        tab.track();
        if previous.is_some() {
            if let Some(view) = scroller() {
                view.set_scroll_top(0);
            }
        }
        search.load_more_soon();
    });
    listen_for_scroll(move || search.load_more_if_near_end());

    // The shelves' width, to fit two rows of YouTube albums like the Qt app.
    let measure = NodeRef::<leptos::html::Div>::new();
    let width = RwSignal::new(0);
    let remeasure = move || {
        if let Some(element) = measure.get_untracked() {
            width.set(element.client_width());
        }
    };
    Effect::new(move |_| {
        measure.track();
        app.sidebar_width.track();
        remeasure();
    });
    let resize = window_event_listener(leptos::ev::resize, move |_| remeasure());
    on_cleanup(move || resize.remove());
    // How many cards fit across, so one-row shelves show that many (the Qt app's `take`).
    let columns = Memo::new(move |_| ((width.get() + GAP) / (CARD + GAP)).max(1) as usize);
    let row_limit = move |one_row: bool| if one_row { columns.get() } else { usize::MAX };

    let has_query = move || !query.with(String::is_empty);
    let is_tab = move |key: &'static str| tab.with(|t| t == key);
    let catalog_tab = move || tab.with(|t| ["all", "songs", "albums", "artists"].contains(&t.as_str()));
    let youtube_tab = move || tab.with(|t| ["all", "albums", "youtube"].contains(&t.as_str()));
    let results_list = move |key: &'static str| search.results.with(|r| r.as_ref().map(|r| list(r, key)).unwrap_or_default());
    let has_results = move || search.results.with(Option::is_some);

    let status_empty = Signal::derive(move || {
        let q = query.get();
        if q.is_empty() {
            return "Search for any song, album or artist. Earshot finds the music on YouTube, Bandcamp and SoundCloud.".to_owned();
        }
        let nothing = search.results.with(|r| r.as_ref().is_some_and(|r| list(r, "tracks").is_empty() && list(r, "albums").is_empty()));
        if catalog_tab() && !is_tab("all") && nothing { format!("Nothing found for \"{q}\"") } else { String::new() }
    });
    let status_error = Signal::derive(move || {
        if is_tab("youtube") {
            search.youtube_error.get()
        } else if catalog_tab() {
            search.error.get()
        } else {
            String::new()
        }
    });
    let status_loading = Signal::derive(move || catalog_tab() && search.loading.get() && !has_results() && search.yt_albums.with(Option::is_none));

    let youtube_tracks = Signal::derive(move || search.youtube.get().unwrap_or_default());
    let catalog_tracks = Signal::derive(move || results_list("tracks"));
    let yt_shown = move || {
        let all = search.yt_albums.get().unwrap_or_default();
        // Two rows on the All tab; everything on the others.
        let limit = if is_tab("all") { columns.get() * 2 } else { usize::MAX };
        all.into_iter().take(limit).enumerate().collect::<Vec<_>>()
    };

    view! {
        <div class="page search">
            <div class="search-measure" node_ref=measure></div>

            <Show when=has_query>
                <div class="search-tabs">
                    {TABS.into_iter().map(|(key, label)| view! {
                        <PillButton text=label checked=Signal::derive(move || is_tab(key))
                            on_click=move |_| app.go(tab_href(&query.get_untracked(), key))/>
                    }).collect_view()}
                </div>
            </Show>

            <StatusView loading=status_loading error=status_error empty=status_empty empty_icon="search"
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>
            <StatusView loading=Signal::derive(move || is_tab("youtube") && search.yt_albums_loading.get())/>

            // Real YouTube results come first, in YouTube's own order; the
            // music catalogue (for album details and artist pages) follows.
            <Show when=move || has_query() && youtube_tab() && (search.yt_albums_loading.get() || search.yt_albums.with(|a| a.as_ref().is_some_and(|a| !a.is_empty())))>
                {move || view! {
                    <SectionHeader title="On YouTube" action=if is_tab("all") { "SEE ALL" } else { "" } href=tab_href(&query.get(), "youtube")/>
                }}
            </Show>
            <Show when=move || youtube_tab() && search.yt_albums_loading.get()>
                <p class="search-note">"Looking for playlists and full-length videos…"</p>
            </Show>
            <Show when=move || youtube_tab() && search.yt_albums.with(Option::is_some)>
                <Shelf>
                    <For each=yt_shown key=|(index, album)| format!("{index}:{}", s(album, "id")) let((_, album))>
                        {album_card(&album, yt_album_subtitle(&album))}
                    </For>
                </Shelf>
            </Show>
            <Show when=move || is_tab("all") && !youtube_tracks.with(Vec::is_empty)>
                <TrackList tracks=youtube_tracks limit=8 show_header=false show_album=false show_number=false
                    context=Signal::derive(move || format!("YouTube: {}", query.get()))/>
            </Show>

            // Top result and songs
            <Show when=move || is_tab("all") && !catalog_tracks.with(Vec::is_empty)>
                <div class="top-and-songs">
                    <div class="top-column">
                        <SectionHeader title="Top result"/>
                        {move || top_result(search)}
                    </div>
                    <div class="songs-column">
                        {move || view! { <SectionHeader title="Songs" action="SEE ALL" href=tab_href(&query.get(), "songs")/> }}
                        <TrackList tracks=catalog_tracks limit=5 show_header=false show_number=false show_album=false
                            context=Signal::derive(move || format!("Search: {}", query.get()))/>
                    </div>
                </div>
            </Show>

            <Show when=move || is_tab("songs") && has_results()>
                <TrackList tracks=catalog_tracks context=Signal::derive(move || format!("Search: {}", query.get()))/>
            </Show>

            <Show when=move || (is_tab("all") || is_tab("albums")) && !results_list("albums").is_empty()>
                {move || view! {
                    <SectionHeader title=if is_tab("albums") { "Albums in the catalogue" } else { "Albums" }
                        action=if is_tab("all") { "SEE ALL" } else { "" } href=tab_href(&query.get(), "albums")/>
                }}
            </Show>
            <Show when=move || (is_tab("all") || is_tab("albums")) && has_results()>
                {move || {
                    let albums = results_list("albums");
                    let cards = albums.iter().take(row_limit(is_tab("all"))).map(|album| {
                        let year = s(album, "year");
                        let by = s(album, "artist");
                        album_card(album, if year.is_empty() || year == "0" { by } else { format!("{year} · {by}") })
                    }).collect_view();
                    view! { <Shelf>{cards}</Shelf> }
                }}
            </Show>

            <Show when=move || is_tab("all") && !results_list("artists").is_empty()>
                {move || view! { <SectionHeader title="Artists" action="SEE ALL" href=tab_href(&query.get(), "artists")/> }}
            </Show>
            <Show when=move || (is_tab("all") || is_tab("artists")) && has_results()>
                {move || {
                    let artists = results_list("artists");
                    let cards = artists.iter().take(row_limit(is_tab("all"))).map(|artist| {
                        let id = s(artist, "id");
                        view! {
                            <AlbumCard title=s(artist, "name") subtitle="Artist" image=image(artist, "pictureBig", "picture") seed=id.clone()
                                href=artist_href(&id) round=true playable=false placeholder="user"/>
                        }
                    }).collect_view();
                    view! { <Shelf>{cards}</Shelf> }
                }}
            </Show>

            <Show when=move || is_tab("youtube") && search.youtube.with(Option::is_some)>
                <p class="search-note">"Straight from YouTube: videos, live sets, covers and remixes that aren't in the catalogue."</p>
                <TrackList tracks=youtube_tracks show_album=false context=Signal::derive(move || format!("YouTube: {}", query.get()))/>
            </Show>

            <Show when=move || (is_tab("youtube") || is_tab("albums")) && search.yt_more_loading.get()>
                <p class="search-note">"Loading more from YouTube…"</p>
            </Show>

            {search.services.into_iter().map(|service| service_section(search, service, columns)).collect_view()}
        </div>
    }
}

/// The catalogue's best match: the artist when the search is their name, else the first album.
fn top_result(search: Search) -> impl IntoView {
    let app = app();
    let results = search.results.get().unwrap_or(Value::Null);
    let query = search.query.get();
    let album = list(&results, "albums").into_iter().next();
    let artist = list(&results, "artists").into_iter().next();
    let show_artist = artist.as_ref().is_some_and(|a| s(a, "name").to_lowercase() == query.to_lowercase());
    let (href, picture, name, kind) = match (&artist, &album) {
        (Some(artist), _) if show_artist => (Some(artist_href(&s(artist, "id"))), s(artist, "picture"), s(artist, "name"), "Artist".to_owned()),
        (_, Some(album)) => (Some(album_href(&s(album, "id"))), s(album, "cover"), s(album, "title"), format!("Album · {}", s(album, "artist"))),
        _ => (None, String::new(), String::new(), String::new()),
    };
    let tracks = list(&results, "tracks");
    // The artist's songs among the results, or the album; pause while they play.
    let player = app.player;
    let theirs: Vec<Value> = match &artist {
        Some(artist) if show_artist => tracks.iter().filter(|t| s(t, "artistId") == s(artist, "id")).cloned().collect(),
        _ => Vec::new(),
    };
    let artist_name = artist.as_ref().map(|a| s(a, "name")).unwrap_or_default();
    let album_id = album.as_ref().map(|a| s(a, "id")).unwrap_or_default();
    let playing_this = {
        let (theirs, artist_name, album_id) = (theirs.clone(), artist_name.clone(), album_id.clone());
        Memo::new(move |_| if show_artist { player.playing_from(&artist_name, &theirs) } else { player.playing_album(&album_id) })
    };
    let play = move |_| {
        match &album {
            _ if show_artist => player.play_tracks(theirs.clone(), 0, artist_name.clone(), None),
            Some(album) => app.play_album(album_id.clone(), s(album, "title"), false),
            None => {}
        }
    };
    view! {
        <a class="top-result" class:playing=move || playing_this.get() href=href>
            <Cover src=picture seed=query round=show_artist radius=8/>
            <div class="grow"></div>
            <div class="top-name">{name}</div>
            <div class="top-kind">{kind}</div>
            <PlayButton size=22 tip="Play" ours=playing_this on_play=play/>
        </a>
    }
}

/// "On Bandcamp" and the others: tags, a shelf of albums, a few songs.
fn service_section(search: Search, service: Service, columns: Memo<usize>) -> impl IntoView {
    let full = move || search.tab.with(|t| t == service.key);
    let shown = move || !search.query.with(String::is_empty) && (full() || search.tab.with(|t| t == "all"));
    let count = move |key: &str| service.found.with(|f| f.as_ref().map(|f| list(f, key).len()).unwrap_or(0));
    let visible = move || {
        shown() && (service.found.with(Option::is_none) || !service.failure.with(String::is_empty) || count("albums") + count("tracks") + count("tags") > 0)
    };
    let tracks = Signal::derive(move || service.found.with(|f| f.as_ref().map(|f| list(f, "tracks")).unwrap_or_default()));
    view! {
        <Show when=visible>
            <section class="search-service">
                {move || view! {
                    <SectionHeader title=format!("On {}", service.name) action=if full() { "" } else { "SEE ALL" }
                        href=tab_href(&search.query.get(), service.key)/>
                }}
                <Show when=move || service.found.with(Option::is_none)>
                    <p class="search-note">{format!("Searching {}…", service.name)}</p>
                </Show>
                <Show when=move || !service.failure.with(String::is_empty)>
                    <p class="search-note">{move || service.failure.get()}</p>
                </Show>
                <Show when=move || count("tags") != 0>
                    <div class="tag-chips">
                        {move || {
                            let tags = service.found.with(|f| f.as_ref().map(|f| list(f, "tags")).unwrap_or_default());
                            tags.into_iter().take(if full() { 12 } else { 6 }).map(|tag| {
                                let (slug, label) = (s(&tag, "slug"), s(&tag, "label"));
                                // The label rides along for the tag page's title.
                                let href = format!("{}&label={}", bandcamp_tag_href(&slug), encode(&label));
                                view! { <a class="pill" href=href>{format!("#{label}")}</a> }
                            }).collect_view()
                        }}
                    </div>
                </Show>
                <Show when=move || count("albums") != 0>
                    {move || {
                        let albums = service.found.with(|f| f.as_ref().map(|f| list(f, "albums")).unwrap_or_default());
                        let limit = if full() { usize::MAX } else { columns.get() };
                        let cards = albums.iter().take(limit).map(|album| {
                            let playlist = if s(album, "kind") == "playlist" { " · Playlist" } else { "" };
                            album_card(album, format!("{}{playlist}", s(album, "artist")))
                        }).collect_view();
                        view! { <Shelf>{cards}</Shelf> }
                    }}
                </Show>
                <Show when=move || count("tracks") != 0>
                    {move || {
                        let full = full();
                        view! {
                            <TrackList tracks=tracks limit=if full { usize::MAX } else { 5 } show_header=full show_number=false
                                context=Signal::derive(move || format!("{}: {}", service.name, search.query.get()))/>
                        }
                    }}
                </Show>
            </section>
        </Show>
    }
}
