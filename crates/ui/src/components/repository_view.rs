use dioxus::prelude::*;
use domain::RepositoryInfo;

/// The main three-column layout for an open repository.
#[component]
pub fn RepositoryView(
    repo: RepositoryInfo,
    on_pick: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let name = repo.name();
    let location = repo.root().display().to_string();
    rsx! {
        header { class: "toolbar",
            span { class: "repo-name", "{name}" }
            span { class: "repo-path", title: "{location}", "{location}" }
            div { class: "spacer" }
            button { onclick: move |_| on_pick.call(()), "Open…" }
            button { onclick: move |_| on_close.call(()), "Close" }
        }
        div { class: "columns",
            section { class: "column branches",
                h2 { "Branches" }
                p { class: "placeholder", "Local branches and remotes appear here." }
            }
            section { class: "column history",
                h2 { "History" }
                p { class: "placeholder", "The commit graph appears here." }
            }
            section { class: "column details",
                h2 { "Details" }
                p { class: "placeholder", "Select a commit to see its details." }
            }
        }
    }
}
