//! TODO: port qml/LibraryAlbumsPage.qml; LibraryPage is the phone's Library tab.

use leptos::prelude::*;

#[component]
pub fn AlbumsPage() -> impl IntoView {
    view! { <div class="page"><h1 class="hero">"Albums"</h1></div> }
}

#[component]
pub fn LibraryPage() -> impl IntoView {
    view! { <div class="page"><h1 class="hero">"Library"</h1></div> }
}
