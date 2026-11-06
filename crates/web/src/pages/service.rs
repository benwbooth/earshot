//! YouTube, SoundCloud, Mixcloud or the Internet Archive, by genre or any tag
//! (qml/ServicePage.qml).

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use serde_json::json;

use crate::api::{Load, fetch};
use crate::components::{AlbumCard, IconButton, PillButton, Shelf, StatusView, TrackList};
use crate::pages::bandcamp::TagField;
use crate::state::{album_href, app, list, s, service_href};

struct Service {
    name: &'static str,
    tint: &'static str,
    blurb: &'static str,
    hint: &'static str,
    /// (tag to ask for, name shown)
    genres: &'static [(&'static str, &'static str)],
}

const YOUTUBE: Service = Service {
    name: "YouTube",
    tint: "#ff0033",
    blurb: "Playlists, full albums and soundtracks (split into songs at their timestamps), and songs, straight from YouTube's own search.",
    hint: "Any genre, game, film or mood: city pop, zelda, rainy day…",
    genres: &[
        ("Lo-fi", "Lo-fi"), ("Video game", "Video game"), ("Soundtrack", "Soundtrack"), ("Anime", "Anime"), ("City pop", "City pop"),
        ("Synthwave", "Synthwave"), ("Jazz", "Jazz"), ("Classical", "Classical"), ("Ambient", "Ambient"), ("Hip-hop", "Hip-hop"),
        ("Rock", "Rock"), ("Metal", "Metal"), ("Electronic", "Electronic"), ("K-pop", "K-pop"), ("Chiptune", "Chiptune"),
        ("Live concert", "Live concert"), ("Folk", "Folk"), ("Blues", "Blues"), ("Reggae", "Reggae"), ("Latin", "Latin"),
    ],
};

const SOUNDCLOUD: Service = Service {
    name: "SoundCloud",
    tint: "#ff5500",
    blurb: "Playlists and tracks from SoundCloud's artists, DJs and producers. They play in SoundCloud's own player.",
    hint: "Any genre or tag: lo-fi, vaporwave, chiptune…",
    genres: &[
        ("Electronic", "Electronic"), ("Hip-hop", "Hip-hop"), ("Lo-fi", "Lo-fi"), ("House", "House"), ("Techno", "Techno"),
        ("Ambient", "Ambient"), ("Indie", "Indie"), ("Pop", "Pop"), ("R&B", "R&B"), ("Rock", "Rock"),
        ("Jazz", "Jazz"), ("Classical", "Classical"), ("Soundtrack", "Soundtrack"), ("Chiptune", "Chiptune"), ("Drum & bass", "Drum & bass"),
        ("Dubstep", "Dubstep"), ("Metal", "Metal"), ("Folk", "Folk"), ("Reggae", "Reggae"), ("Latin", "Latin"),
    ],
};

const MIXCLOUD: Service = Service {
    name: "Mixcloud",
    tint: "#5000ff",
    blurb: "DJ mixes and radio shows, most popular first. They play in Mixcloud's own player.",
    hint: "Any genre or tag: deep house, disco, jungle…",
    genres: &[
        ("House", "House"), ("Deep house", "Deep house"), ("Techno", "Techno"), ("Drum & bass", "Drum & bass"), ("Ambient", "Ambient"),
        ("Hip hop", "Hip hop"), ("Jazz", "Jazz"), ("Soul", "Soul"), ("Funk", "Funk"), ("Disco", "Disco"),
        ("Electronica", "Electronica"), ("Dubstep", "Dubstep"), ("Reggae", "Reggae"), ("Rock", "Rock"), ("Chillout", "Chillout"),
        ("Breakbeat", "Breakbeat"), ("Trance", "Trance"), ("Jungle", "Jungle"), ("World", "World"), ("Talk", "Talk"),
    ],
};

const ARCHIVE: Service = Service {
    name: "Internet Archive",
    tint: "#4a8fd4",
    blurb: "Live concert recordings, netlabel releases, 78 rpm records and other freely shared music from archive.org, most downloaded first.",
    hint: "Any subject: blues, grateful dead, bluegrass…",
    genres: &[
        ("collection:etree", "Live Music Archive"), ("collection:78rpm", "78 rpm records"), ("collection:netlabels", "Netlabels"),
        ("collection:georgeblood", "Great 78 Project"), ("collection:opensource_audio", "Community audio"),
        ("Jazz", "Jazz"), ("Blues", "Blues"), ("Classical", "Classical"), ("Folk", "Folk"), ("Rock", "Rock"),
        ("Electronic", "Electronic"), ("Ambient", "Ambient"), ("Hip hop", "Hip hop"), ("Country", "Country"), ("Bluegrass", "Bluegrass"),
        ("Experimental", "Experimental"),
    ],
};

fn service(id: &str) -> (&'static str, &'static Service) {
    match id {
        "youtube" => ("youtube", &YOUTUBE),
        "mixcloud" => ("mixcloud", &MIXCLOUD),
        "archive" => ("archive", &ARCHIVE),
        _ => ("soundcloud", &SOUNDCLOUD),
    }
}

/// "#ff5500" at the given opacity.
fn alpha(hex: &str, opacity: f64) -> String {
    let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2).unwrap_or("00"), 16).unwrap_or(0);
    format!("rgba({}, {}, {}, {opacity})", channel(1), channel(3), channel(5))
}

#[component]
pub fn ServicePage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let query = use_query_map();
    let id = Memo::new(move |_| service(&query.with(|q| q.get("id").unwrap_or_default())).0);
    let info = move || service(id.get()).1;
    // The chosen tag and its name, for the service it was chosen on
    // (another service starts again at its first genre).
    let chosen = RwSignal::new(None::<(&'static str, String, String)>);
    let tag = Memo::new(move |_| {
        let id = id.get();
        chosen.get().filter(|(on, _, _)| *on == id).map(|(_, tag, label)| (tag, label)).unwrap_or_else(|| {
            let (tag, label) = service(id).1.genres[0];
            (tag.to_owned(), label.to_owned())
        })
    });
    let field = RwSignal::new(String::new());
    Effect::new(move |previous: Option<&'static str>| {
        let id = id.get();
        if previous.is_some_and(|p| p != id) {
            field.set(String::new());
        }
        id
    });
    let retry = RwSignal::new(0u32);
    let load = fetch(move || {
        retry.track();
        Some(("browse", json!({ "service": id.get(), "tag": tag.get().0 })))
    });
    let found = Memo::new(move |_| load.with(|l| l.value().cloned()));
    let albums = Signal::derive(move || found.with(|f| f.as_ref().map(|f| list(f, "albums")).unwrap_or_default()));
    let tracks = Signal::derive(move || found.with(|f| f.as_ref().map(|f| list(f, "tracks")).unwrap_or_default()));
    let context = Signal::derive(move || format!("{}: {}", info().name, tag.get().1));
    let show = move |tag: String, label: String| chosen.set(Some((id.get_untracked(), tag, label)));

    view! {
        <div class="page service-page">
            <div class="wash" style=move || format!("--wash: {}", alpha(info().tint, 0.4))></div>
            <div class="eyebrow">{move || info().name.to_uppercase()}</div>
            <h1 class="hero">{move || tag.get().1}</h1>
            <p class="blurb">{move || info().blurb}</p>
            <TagField placeholder=Signal::derive(move || info().hint.to_owned()) text=field on_enter=Callback::new(move |text: String| {
                let text = text.trim().to_owned();
                if !text.is_empty() {
                    show(text.clone(), text);
                }
            })/>
            <div class="genre-pills">
                {move || info().genres.iter().map(|(genre, label)| view! {
                    <PillButton text=*label checked=Signal::derive(move || tag.with(|(tag, _)| tag == genre))
                        on_click=move |_| show(genre.to_string(), label.to_string())/>
                }).collect_view()}
            </div>
            <StatusView
                loading=Signal::derive(move || load.with(Load::loading))
                error=Signal::derive(move || load.with(Load::error))
                empty=Signal::derive(move || {
                    if found.with(Option::is_some) && albums.with(Vec::is_empty) && tracks.with(Vec::is_empty) {
                        format!("Nothing on {} for \"{}\".", info().name, tag.get().1)
                    } else {
                        String::new()
                    }
                })
                empty_icon="tag"
                on_retry=Callback::new(move |_| retry.update(|n| *n += 1))/>
            <Show when=move || !albums.with(Vec::is_empty)>
                <Shelf>
                    {move || albums.get().into_iter().map(|album| {
                        let (album_id, title) = (s(&album, "id"), s(&album, "title"));
                        let year = s(&album, "year");
                        let subtitle = if year.is_empty() { s(&album, "artist") } else { format!("{} · {year}", s(&album, "artist")) };
                        let image = if s(&album, "coverBig").is_empty() { s(&album, "cover") } else { s(&album, "coverBig") };
                        let (play_id, play_title) = (album_id.clone(), title.clone());
                        view! {
                            <AlbumCard title=title subtitle=subtitle image=image seed=album_id.clone() href=album_href(&album_id)
                                on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
                        }
                    }).collect_view()}
                </Shelf>
            </Show>
            <Show when=move || !tracks.with(Vec::is_empty)>
                <div class="actions">
                    <IconButton icon="play" size=22 filled=true tip="Play all"
                        on_click=move |_| player.play_tracks(tracks.get_untracked(), 0, context.get_untracked(), Some(service_href(id.get_untracked())))/>
                    <PillButton text="Shuffle" icon="shuffle"
                        on_click=move |_| player.shuffle_tracks(tracks.get_untracked(), context.get_untracked(), Some(service_href(id.get_untracked())))/>
                </div>
                <TrackList tracks=tracks show_album=false context=context/>
            </Show>
        </div>
    }
}
