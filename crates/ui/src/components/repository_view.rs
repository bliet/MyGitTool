use super::branches::BranchesColumn;
use super::details::DetailsColumn;
use super::history::HistoryColumn;
use super::session::Session;
use super::update_settings;
use crate::launch::Services;
use crate::state::AppState;
use crate::view::columns::{ColumnLayout, Side};
use application::Settings;
use dioxus::prelude::*;
use domain::RepositoryInfo;

/// Keys handled app-wide. The listener calls `preventDefault`, so `F5` never
/// reloads the web view.
const KEY_LISTENER: &str = r#"
    window.__myGitToolKeys?.abort();
    const controller = new AbortController();
    window.__myGitToolKeys = controller;
    window.addEventListener("keydown", (event) => {
        if (event.key === "F5" || event.key === "Escape") {
            event.preventDefault();
            dioxus.send(event.key);
        }
    }, { signal: controller.signal });
    await new Promise(() => {});
"#;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    side: Side,
    start_x: f64,
    start_width: f64,
}

/// The main three-column layout for an open repository.
#[component]
pub fn RepositoryView(
    repo: RepositoryInfo,
    on_pick: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let services = use_context::<Services>();
    let session = use_context_provider(|| Session::new(services.clone(), repo.clone()));
    let settings = use_context::<Signal<Settings>>();
    let app = use_context::<Signal<AppState>>();
    let guides = settings.read().indent_guides;
    let mut layout = use_signal(ColumnLayout::default);
    let mut total_width = use_signal(|| 1400.0);
    let mut drag = use_signal(|| None::<Drag>);

    use_hook(move || {
        session.refresh();
        spawn(async move {
            let mut keys = document::eval(KEY_LISTENER);
            while let Ok(key) = keys.recv::<String>().await {
                match key.as_str() {
                    "F5" => session.refresh(),
                    "Escape" => session.set_filter(None),
                    _ => {}
                }
            }
        });
    });

    let name = repo.name();
    let location = repo.root().display().to_string();
    let template = layout.read().grid_template();
    let dragging = drag.read().is_some();
    let mut start_drag = move |side: Side, x: f64| {
        let start_width = layout.peek().width(side);
        drag.set(Some(Drag {
            side,
            start_x: x,
            start_width,
        }));
    };

    rsx! {
        header { class: "toolbar",
            span { class: "repo-name", "{name}" }
            span { class: "repo-path", title: "{location}", "{location}" }
            div { class: "spacer" }
            button {
                class: if guides { "active" } else { "" },
                title: if guides { "Hide indent guides in trees" } else { "Show indent guides in trees" },
                aria_pressed: "{guides}",
                onclick: move |_| {
                    update_settings(&services, settings, app, |s| s.indent_guides = !s.indent_guides)
                },
                "┊ Guides"
            }
            button { title: "Refresh (F5)", onclick: move |_| session.refresh(), "⟳ Refresh" }
            button { onclick: move |_| on_pick.call(()), "Open…" }
            button { onclick: move |_| on_close.call(()), "Close" }
        }
        div {
            class: if dragging { "columns dragging" } else { "columns" },
            style: "grid-template-columns: {template};",
            onresize: move |event| {
                if let Ok(size) = event.data().get_border_box_size() {
                    total_width.set(size.width);
                }
            },
            onmousemove: move |event| {
                let Some(active) = *drag.peek() else { return };
                let x = event.client_coordinates().x;
                let width = ColumnLayout::dragged_width(active.side, active.start_width, active.start_x, x);
                let total = *total_width.peek();
                layout.write().resize(active.side, width, total);
            },
            onmouseup: move |_| drag.set(None),
            onmouseleave: move |_| drag.set(None),
            BranchesColumn {}
            div {
                class: "splitter",
                onmousedown: move |event| start_drag(Side::Left, event.client_coordinates().x),
            }
            HistoryColumn {}
            div {
                class: "splitter",
                onmousedown: move |event| start_drag(Side::Right, event.client_coordinates().x),
            }
            DetailsColumn {}
        }
    }
}
