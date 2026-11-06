//! One module per page, named after the Qt app's pages (HomePage.qml → home).

use leptos::prelude::*;

pub mod album;
pub mod artist;
pub mod bandcamp;
pub mod chart;
pub mod foryou;
pub mod home;
pub mod library;
pub mod playlist;
pub mod queue;
pub mod radio;
pub mod search;
pub mod service;

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="page">
            <crate::components::StatusView loading=false empty="There's nothing here." />
        </div>
    }
}
