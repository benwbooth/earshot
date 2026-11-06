//! Talking to Earshot's server: the same requests the Qt app makes, by name
//! with JSON arguments, answered by the shared core.

use std::cell::Cell;

use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

/// Asks the server; the answer, or a message to show.
pub async fn call(method: &str, args: Value) -> Result<Value, String> {
    let response = Request::post(&format!("/api/{method}"))
        .json(&args)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|_| "Earshot's server can't be reached.".to_owned())?;
    let ok = response.ok();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if ok {
        Ok(value)
    } else {
        Err(value["error"].as_str().unwrap_or("Earshot's server couldn't answer.").to_owned())
    }
}

/// Fires a request whose answer doesn't matter (library changes).
pub fn send(method: &'static str, args: Value) {
    spawn_local(async move {
        let _ = call(method, args).await;
    });
}

/// What a page is showing: still loading, the answer, or what went wrong.
#[derive(Clone, Debug, PartialEq)]
pub enum Load {
    Loading,
    Ready(Value),
    Failed(String),
}

impl Load {
    pub fn value(&self) -> Option<&Value> {
        match self {
            Load::Ready(value) => Some(value),
            _ => None,
        }
    }
    pub fn error(&self) -> String {
        match self {
            Load::Failed(message) => message.clone(),
            _ => String::new(),
        }
    }
    pub fn loading(&self) -> bool {
        matches!(self, Load::Loading)
    }
}

thread_local! {
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Loads `request()` (method and arguments) now and again whenever the
/// signals it reads change; answers to outdated requests are dropped.
pub fn fetch(request: impl Fn() -> Option<(&'static str, Value)> + 'static) -> RwSignal<Load> {
    let state = RwSignal::new(Load::Loading);
    let latest = StoredValue::new(0u64);
    Effect::new(move |_| {
        let Some((method, args)) = request() else { return };
        let generation = GENERATION.with(|g| {
            g.set(g.get() + 1);
            g.get()
        });
        latest.set_value(generation);
        state.set(Load::Loading);
        spawn_local(async move {
            let result = call(method, args).await;
            if latest.get_value() == generation {
                state.set(match result {
                    Ok(value) => Load::Ready(value),
                    Err(message) => Load::Failed(message),
                });
            }
        });
    });
    state
}
