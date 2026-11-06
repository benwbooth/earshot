//! Bandcamp: genres and styles, any tag at all (qml/BandcampPage.qml), and a
//! tag's best-selling, new or random albums (qml/BandcampTagPage.qml).

use std::cell::RefCell;
use std::collections::HashSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::api::{self, Load};
use crate::components::{AlbumCard, Icon, PillButton, SectionHeader, Shelf, StatusView, TileCard};
use crate::state::{album_href, app, bandcamp_tag_href, encode, list, s, window};

const TINTS: [&str; 8] = ["#ff5f7e", "#7a5cff", "#3db8ff", "#3ddc97", "#ff8a3d", "#f5c542", "#e05cff", "#5c8aff"];
const WASH: &str = "--wash: rgba(29, 160, 195, 0.4)";

thread_local! {
    // Bandcamp's genres and styles, fetched once (the Qt app's Api.bandcampTags).
    static TAGS: RefCell<Option<Value>> = const { RefCell::new(None) };
}

/// A tag's name ("label", or "name").
fn label_of(tag: &Value) -> String {
    let label = s(tag, "label");
    if label.is_empty() { s(tag, "name") } else { label }
}

/// The tag page for another slice, keeping the name the link gave.
fn tag_href(tag: &str, slice: &str, label: Option<String>) -> String {
    let mut href = bandcamp_tag_href(tag);
    if let Some(label) = label.filter(|l| !l.is_empty()) {
        href.push_str(&format!("&label={}", encode(&label)));
    }
    if slice != "top" {
        href.push_str(&format!("&slice={}", encode(slice)));
    }
    href
}

/// Bandcamp's genres and styles (kept for the visit), and a way to ask again.
fn known_tags() -> (RwSignal<Load>, Callback<()>) {
    let cached = TAGS.with(|t| t.borrow().clone());
    let state = RwSignal::new(cached.clone().map(Load::Ready).unwrap_or(Load::Loading));
    let load = Callback::new(move |_: ()| {
        state.set(Load::Loading);
        spawn_local(async move {
            let result = api::call("bandcampTags", json!({})).await;
            if let Ok(tags) = &result {
                TAGS.with(|t| *t.borrow_mut() = Some(tags.clone()));
            }
            let _ = state.try_set(match result {
                Ok(tags) => Load::Ready(tags),
                Err(message) => Load::Failed(message),
            });
        });
    });
    if cached.is_none() {
        load.run(());
    }
    (state, load)
}

fn subgenres(tags: &Value, parent: &str) -> Vec<Value> {
    list(tags, "subgenres").into_iter().filter(|t| s(t, "parent") == parent).collect()
}

/// Tags as chips; each opens the tag's albums (qml/TagChips.qml).
#[component]
pub fn TagChips(#[prop(into)] tags: Signal<Vec<Value>>) -> impl IntoView {
    view! {
        <Show when=move || !tags.with(Vec::is_empty)>
            <div class="tag-chips">
                {move || tags.get().into_iter().map(|tag| view! {
                    <a class="pill" href=bandcamp_tag_href(&s(&tag, "slug"))>{label_of(&tag)}</a>
                }).collect_view()}
            </div>
        </Show>
    }
}

/// Every known tag matching the typed words, the words themselves first.
/// `seen`: tags on Bandcamp albums opened this visit.
fn matching(filter: &str, tags: Option<&Value>, seen: Vec<Value>) -> Vec<Value> {
    if filter.is_empty() {
        return Vec::new();
    }
    let mut known: Vec<Value> = tags.map(|t| [list(t, "genres"), list(t, "subgenres")].concat()).unwrap_or_default();
    known.extend(seen);
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let typed = filter.split(|c: char| !c.is_alphanumeric()).filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
    if !typed.is_empty() {
        found.push(json!({ "slug": typed, "label": filter }));
        seen.insert(typed);
    }
    let words: Vec<&str> = filter.split_whitespace().collect();
    let mut hits: Vec<Value> = known
        .into_iter()
        .filter(|t| {
            let (label, slug) = (label_of(t).to_lowercase(), s(t, "slug"));
            words.iter().all(|w| label.contains(w) || slug.contains(w))
        })
        .collect();
    hits.sort_by_key(|t| {
        let label = label_of(t).to_lowercase();
        (!label.starts_with(filter), label.chars().count())
    });
    for tag in hits {
        if seen.insert(s(&tag, "slug")) {
            found.push(tag);
        }
    }
    found.truncate(60);
    found
}

/// A rounded text box with a tag icon; Enter hands over its text.
#[component]
pub fn TagField(#[prop(into)] placeholder: Signal<String>, text: RwSignal<String>, #[prop(into)] on_enter: Callback<String>) -> impl IntoView {
    view! {
        <label class="tag-field">
            <Icon name="tag" size=18/>
            <input
                type="text"
                autocomplete="off"
                spellcheck="false"
                placeholder=move || placeholder.get()
                prop:value=move || text.get()
                on:input=move |event| text.set(event_target_value(&event))
                on:keydown=move |event: web_sys::KeyboardEvent| {
                    if event.key() == "Enter" {
                        on_enter.run(text.get_untracked())
                    }
                }
            />
        </label>
    }
}

#[component]
pub fn BandcampPage() -> impl IntoView {
    let query = use_query_map();
    let (tags, retry) = known_tags();
    let field = RwSignal::new(query.with_untracked(|q| q.get("id").unwrap_or_default()));
    let data = Memo::new(move |_| tags.with(|l| l.value().cloned()));
    let filter = Memo::new(move |_| field.get().trim().to_lowercase());
    let seen = app().seen_bandcamp_tags;
    let matches = Signal::derive(move || data.with(|tags| matching(&filter.get(), tags.as_ref(), seen.get())));
    let browsing = move || data.with(Option::is_some) && filter.with(String::is_empty);
    let open_tag = Callback::new(move |text: String| {
        let words = text.trim();
        if !words.is_empty() {
            app().go(bandcamp_tag_href(words));
        }
    });

    view! {
        <div class="page bandcamp">
            <div class="wash" style=WASH></div>
            <div class="eyebrow">"BANDCAMP"</div>
            <h1 class="hero">"Browse by tag"</h1>
            <p class="blurb">"Best-selling, new and random albums from Bandcamp's artists and labels for any genre or tag. Songs stream from Bandcamp itself."</p>
            // Any tag at all, not only the genres below.
            <TagField placeholder="Filter tags, or type any tag: chiptune, vaporwave…" text=field on_enter=open_tag/>
            <StatusView loading=Signal::derive(move || tags.with(Load::loading)) error=Signal::derive(move || tags.with(Load::error)) on_retry=retry/>

            // While filtering: the matching tags, the typed words first.
            <Show when=move || !filter.with(String::is_empty)>
                <SectionHeader title=Signal::derive(move || {
                    if matches.with(Vec::len) > 1 { format!("Tags matching \"{}\"", filter.get()) } else { "Tag".to_owned() }
                })/>
                <TagChips tags=matches/>
            </Show>

            <Show when=browsing>
                <SectionHeader title="Genres"/>
                <div class="genre-tiles">
                    {move || data.get().map(|tags| list(&tags, "genres").into_iter().enumerate().map(|(index, genre)| {
                        let slug = s(&genre, "slug");
                        let styles = subgenres(&tags, &slug).len();
                        let subtitle = if styles > 0 { format!("{styles} styles") } else { String::new() };
                        view! { <TileCard title=s(&genre, "label") subtitle=subtitle tint=TINTS[index % TINTS.len()] href=bandcamp_tag_href(&slug)/> }
                    }).collect_view())}
                </div>
                {move || data.get().map(|tags| list(&tags, "genres").into_iter().filter_map(|genre| {
                    let styles = subgenres(&tags, &s(&genre, "slug"));
                    (!styles.is_empty()).then(|| view! {
                        <div class="styles">
                            <SectionHeader title=s(&genre, "label")/>
                            <TagChips tags=styles/>
                        </div>
                    })
                }).collect_view())}
            </Show>
        </div>
    }
}

/// The page's scroller (the window's main column).
fn scroller() -> Option<web_sys::Element> {
    window().document()?.query_selector("main.content").ok().flatten()
}

fn near_end() -> bool {
    scroller().is_some_and(|el| el.scroll_top() + el.client_height() > el.scroll_height() - 1200)
}

#[component]
pub fn BandcampTagPage() -> impl IntoView {
    let app = app();
    let query = use_query_map();
    let tag = Memo::new(move |_| query.with(|q| q.get("tag").unwrap_or_default()));
    let slice = Memo::new(move |_| {
        query.with(|q| q.get("slice").filter(|s| ["top", "new", "rand"].contains(&s.as_str())).unwrap_or_else(|| "top".to_owned()))
    });
    let (tags, _) = known_tags();
    // The name the link gave, or Bandcamp's own name for a known tag.
    let label = Memo::new(move |_| {
        if let Some(label) = query.with(|q| q.get("label")).filter(|l| !l.is_empty()) {
            return label;
        }
        let tag = tag.get();
        let mut known = tags.with(|l| l.value().map(|t| [list(t, "genres"), list(t, "subgenres")].concat())).unwrap_or_default();
        known.extend(app.seen_bandcamp_tags.get());
        known.iter().find(|t| s(t, "slug") == tag).map(label_of).unwrap_or_else(|| tag.to_lowercase())
    });
    let related = Signal::derive(move || tags.with(|l| l.value().map(|t| subgenres(t, &tag.get())).unwrap_or_default()));

    let albums = RwSignal::new(Vec::<Value>::new());
    let next = RwSignal::new(String::new());
    let loading = RwSignal::new(true);
    let loading_more = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let reload = RwSignal::new(0u32);
    // Answers for an earlier tag or slice are dropped.
    let generation = StoredValue::new(0u32);

    let load_more = Callback::new(move |_: ()| {
        if next.get_untracked().is_empty() || loading.get_untracked() || loading_more.get_untracked() {
            return;
        }
        loading_more.set(true);
        let wanted = generation.get_value();
        let args = json!({ "tag": tag.get_untracked(), "slice": slice.get_untracked(), "next": next.get_untracked() });
        spawn_local(async move {
            let result = api::call("bandcampTag", args).await;
            if generation.try_get_value() != Some(wanted) {
                return;
            }
            match result {
                Ok(page) => {
                    albums.update(|albums| {
                        let seen: HashSet<String> = albums.iter().map(|a| s(a, "id")).collect();
                        albums.extend(list(&page, "albums").into_iter().filter(|a| !seen.contains(&s(a, "id"))));
                    });
                    next.set(s(&page, "next"));
                }
                Err(_) => next.set(String::new()),
            }
            loading_more.set(false);
        });
    });

    Effect::new(move |_| {
        reload.track();
        let args = json!({ "tag": tag.get(), "slice": slice.get(), "next": "" });
        generation.update_value(|g| *g += 1);
        let wanted = generation.get_value();
        albums.set(Vec::new());
        next.set(String::new());
        loading.set(true);
        loading_more.set(false);
        error.set(String::new());
        spawn_local(async move {
            let result = api::call("bandcampTag", args).await;
            if generation.try_get_value() != Some(wanted) {
                return;
            }
            match result {
                Ok(page) => {
                    albums.set(list(&page, "albums"));
                    next.set(s(&page, "next"));
                }
                Err(message) => error.set(message),
            }
            loading.set(false);
        });
    });
    // After each page arrives: more if the end is already in sight.
    Effect::new(move |_| {
        albums.track();
        if !loading.get() && !loading_more.get() {
            gloo_timers::callback::Timeout::new(60, move || {
                if near_end() {
                    load_more.run(());
                }
            })
            .forget();
        }
    });
    // And more while scrolling near the end.
    if let Some(element) = scroller() {
        let on_scroll = Closure::<dyn Fn(web_sys::Event)>::new(move |_| {
            if near_end() {
                load_more.run(());
            }
        });
        let _ = element.add_event_listener_with_callback("scroll", on_scroll.as_ref().unchecked_ref());
        let listener = StoredValue::new_local(Some(on_scroll));
        on_cleanup(move || {
            if let (Some(Some(on_scroll)), Some(element)) = (listener.try_update_value(Option::take), scroller()) {
                let _ = element.remove_event_listener_with_callback("scroll", on_scroll.as_ref().unchecked_ref());
            }
        });
    }

    let slices = [("top", "Best-selling"), ("new", "New arrivals"), ("rand", "Surprise me")];
    view! {
        <div class="page bandcamp-tag">
            <div class="wash" style=WASH></div>
            <div class="eyebrow">"BANDCAMP TAG"</div>
            <h1 class="hero">{move || label.get()}</h1>
            <div class="slices">
                {slices.into_iter().map(|(id, text)| view! {
                    <PillButton text=text checked=Signal::derive(move || slice.get() == id) on_click=move |_| {
                        if slice.get_untracked() != id {
                            app.go(tag_href(&tag.get_untracked(), id, query.with_untracked(|q| q.get("label"))));
                        } else if id == "rand" {
                            reload.update(|n| *n += 1);
                        }
                    }/>
                }).collect_view()}
            </div>
            <TagChips tags=related/>
            <StatusView
                loading=loading
                error=error
                empty=Signal::derive(move || {
                    if !loading.get() && error.with(String::is_empty) && albums.with(Vec::is_empty) {
                        format!("Bandcamp has no albums tagged \"{}\".", label.get())
                    } else {
                        String::new()
                    }
                })
                empty_icon="tag"
                on_retry=Callback::new(move |_| reload.update(|n| *n += 1))/>
            <Show when=move || !albums.with(Vec::is_empty)>
                <Shelf>
                    // Keyed, so the albums already shown stay put as more arrive.
                    <For each=move || albums.get() key=|album| s(album, "id") let:album>
                        {
                            let (id, title) = (s(&album, "id"), s(&album, "title"));
                            let location = s(&album, "location");
                            let subtitle = if location.is_empty() { s(&album, "artist") } else { format!("{} · {location}", s(&album, "artist")) };
                            let image = if s(&album, "coverBig").is_empty() { s(&album, "cover") } else { s(&album, "coverBig") };
                            let (play_id, play_title) = (id.clone(), title.clone());
                            view! {
                                <AlbumCard title=title subtitle=subtitle image=image seed=id.clone() href=album_href(&id)
                                    on_play=Callback::new(move |_| app.play_album(play_id.clone(), play_title.clone(), false))/>
                            }
                        }
                    </For>
                </Shelf>
            </Show>
            <Show when=move || loading_more.get()>
                <p class="more">"Loading more from Bandcamp…"</p>
            </Show>
        </div>
    }
}
