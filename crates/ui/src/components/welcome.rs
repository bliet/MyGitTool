use crate::state::AppState;
use dioxus::prelude::*;
use std::path::PathBuf;

/// Shown when no repository is open: the folder picker and the recent list.
#[component]
pub fn Welcome(
    on_open: EventHandler<PathBuf>,
    on_pick: EventHandler<()>,
    on_forget: EventHandler<PathBuf>,
) -> Element {
    let state = use_context::<Signal<AppState>>();
    let recent = state.read().recent.clone();
    rsx! {
        main { class: "welcome",
            h1 { "MyGitTool" }
            p { class: "muted", "Open a Git repository to get started." }
            button { class: "primary", onclick: move |_| on_pick.call(()), "Open repository…" }
            if !recent.is_empty() {
                section { class: "recent",
                    h2 { "Recent" }
                    ul {
                        for entry in recent {
                            li { key: "{entry.location}",
                                button {
                                    class: "recent-entry",
                                    title: "{entry.location}",
                                    onclick: {
                                        let path = entry.path.clone();
                                        move |_| on_open.call(path.clone())
                                    },
                                    span { class: "name", "{entry.name}" }
                                    span { class: "location", "{entry.location}" }
                                }
                                button {
                                    class: "icon",
                                    title: "Remove from list",
                                    onclick: {
                                        let path = entry.path.clone();
                                        move |_| on_forget.call(path.clone())
                                    },
                                    "✕"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
