//! The queue: what's playing and what plays next (qml/QueuePage.qml).

use leptos::prelude::*;
use serde_json::Value;

use crate::components::{PillButton, SectionHeader, StatusView, TrackRow};
use crate::state::{MenuRequest, Prompt, app};

#[component]
pub fn QueuePage() -> impl IntoView {
    let app = app();
    let player = app.player;
    let current = Memo::new(move |_| player.current());
    let upcoming = Memo::new(move |_| {
        let start = (player.index.get() + 1).max(0) as usize;
        player.queue.with(|q| q.iter().skip(start).cloned().collect::<Vec<Value>>())
    });

    view! {
        <div class="page queue">
            <div class="queue-head">
                <h1 class="hero">"Queue"</h1>
                <Show when=move || !player.queue.with(Vec::is_empty)>
                    <PillButton text="Save as playlist" icon="plus"
                        on_click=move |_| app.prompt.set(Some(Prompt::NewPlaylist(player.queue.get_untracked())))/>
                </Show>
                <Show when=move || !upcoming.with(Vec::is_empty)>
                    <PillButton text="Clear upcoming" on_click=move |_| player.clear_upcoming()/>
                </Show>
            </div>

            <StatusView loading=false empty_icon="queue"
                empty=Signal::derive(move || if player.queue.with(Vec::is_empty) {
                    "Nothing queued. Play an album, a chart or a radio station and its songs line up here.".to_owned()
                } else {
                    String::new()
                })/>

            {move || current.get().map(|track| {
                view! {
                    <SectionHeader title=Signal::derive(move || {
                        let context = player.context.get();
                        if context.is_empty() { "Now playing".to_owned() } else { format!("Now playing · from {context}") }
                    })/>
                    <div class="track-list">
                        <TrackRow track=track number=0 show_number=false
                            on_play=Callback::new(move |_| player.toggle())
                            on_menu=Callback::new(move |mut request: MenuRequest| {
                                request.queue_index = -1;
                                app.menu.set(Some(request));
                            })/>
                    </div>
                }
            })}

            // Up next: rows numbered from 1, standing for their place in the whole queue.
            <Show when=move || !upcoming.with(Vec::is_empty)>
                <SectionHeader title="Up next"/>
                <div class="track-list">
                    {move || {
                        let first = player.index.get_untracked() + 1;
                        upcoming.get().into_iter().enumerate().map(|(i, track)| {
                            let at = first + i as i64;
                            view! {
                                <TrackRow track=track number=i + 1
                                    on_play=Callback::new(move |_| player.jump(at))
                                    on_menu=Callback::new(move |mut request: MenuRequest| {
                                        request.queue_index = at;
                                        app.menu.set(Some(request));
                                    })/>
                            }
                        }).collect_view()
                    }}
                </div>
            </Show>
        </div>
    }
}
