use crate::state::AppState;
use dioxus::prelude::*;

#[component]
pub fn ErrorBanner() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let error = state.read().error.clone();
    rsx! {
        if let Some(message) = error {
            div { class: "error-banner", role: "alert",
                span { class: "message", "{message}" }
                button {
                    class: "icon",
                    title: "Dismiss",
                    onclick: move |_| state.write().dismiss_error(),
                    "✕"
                }
            }
        }
    }
}
