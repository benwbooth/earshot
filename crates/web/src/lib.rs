//! Earshot's web version: the Qt app's interface in Leptos, talking to the
//! same Rust core through Earshot's server. Works on desktops and phones.

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_navigate;
use leptos_router::path;
use wasm_bindgen::prelude::*;

mod api;
mod components;
mod engine;
mod levels;
mod pages;
mod player;
mod report;
mod shell;
mod state;

use shell::{ArtViewer, PlayerBar, PromptDialog, Sidebar, TabBar, Toast, TopBar, TrackMenu};
use state::App;

#[component]
fn Shell() -> impl IntoView {
    let app = state::app();
    let navigate = use_navigate();
    app.set_navigate(Callback::new(move |path: String| navigate(&path, Default::default())));
    shell::integrate();
    view! {
        <div class="window" style=move || format!("--sidebar:{}px", app.sidebar_width.get())>
            <TopBar/>
            <Sidebar/>
            <main class="content">
                <Routes fallback=|| view! { <pages::NotFound/> }>
                    <Route path=path!("/") view=pages::foryou::ForYouPage/>
                    <Route path=path!("/trending") view=pages::home::TrendingPage/>
                    <Route path=path!("/search") view=pages::search::SearchPage/>
                    <Route path=path!("/album") view=pages::album::AlbumPage/>
                    <Route path=path!("/artist") view=pages::artist::ArtistPage/>
                    <Route path=path!("/chart") view=pages::chart::ChartPage/>
                    <Route path=path!("/radio") view=pages::radio::RadioPage/>
                    <Route path=path!("/playlist") view=pages::playlist::PlaylistPage/>
                    <Route path=path!("/albums") view=pages::library::AlbumsPage/>
                    <Route path=path!("/library") view=pages::library::LibraryPage/>
                    <Route path=path!("/queue") view=pages::queue::QueuePage/>
                    <Route path=path!("/bandcamp") view=pages::bandcamp::BandcampPage/>
                    <Route path=path!("/bandcamp/tag") view=pages::bandcamp::BandcampTagPage/>
                    <Route path=path!("/service") view=pages::service::ServicePage/>
                    // Made for you moved to the home page; old links still work.
                    <Route path=path!("/foryou") view=pages::foryou::ForYouPage/>
                </Routes>
            </main>
            <PlayerBar/>
            <TabBar/>
            <TrackMenu/>
            <PromptDialog/>
            <Toast/>
            <ArtViewer/>
        </div>
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    mount_to_body(|| {
        provide_context(App::new());
        view! { <Router><Shell/></Router> }
    });
}
