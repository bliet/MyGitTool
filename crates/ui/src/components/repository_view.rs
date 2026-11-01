use super::branches::BranchesColumn;
use super::details::DetailsColumn;
use super::history::HistoryColumn;
use super::operation_views::{ContextMenu, DialogHost, OngoingBanner, OperationStatus};
use super::operations::Operations;
use super::session::Session;
use super::settings_dialog::SettingsOpen;
use super::window_layout::LayoutKeeper;
use super::{blocking, update_settings};
use crate::launch::Services;
use crate::state::AppState;
use crate::view::columns::{ColumnLayout, Side};
use crate::view::operation::Action;
use application::{FetchTarget, RepoChanges, Settings, WatchGuard};
use dioxus::prelude::*;
use domain::RepositoryInfo;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

/// Keys handled app-wide. The listener calls `preventDefault`, so `F5` never
/// reloads the web view. The web view's own context menu is suppressed; the
/// app shows its own where it has one.
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
    window.addEventListener("contextmenu", (event) => {
        if (!event.target.closest?.("input, textarea, .selectable")) {
            event.preventDefault();
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
    let operations = use_context_provider(|| Operations::new(services.clone(), session, app));
    let busy = operations.is_running();
    let push_case = services.push.clone();
    let guides = settings.read().indent_guides;
    let SettingsOpen(mut settings_open) = use_context::<SettingsOpen>();
    let keeper = use_context::<LayoutKeeper>();
    let mut layout = keeper.columns;
    let total_width = keeper.window_width;
    let mut drag = use_signal(|| None::<Drag>);

    // Refresh on changes made outside the app. The guard lives as long as
    // this view; an operation in progress refreshes when it is done.
    let watch = use_hook(|| Rc::new(RefCell::new(None::<WatchGuard>)));
    use_hook({
        let watch_repository = services.watch_repository.clone();
        let repo = repo.clone();
        move || {
            spawn(async move {
                let (sender, mut changes) = mpsc::unbounded_channel::<RepoChanges>();
                let started = blocking(move || {
                    watch_repository.watch(&repo, move |change| {
                        let _ = sender.send(change);
                    })
                })
                .await;
                match started {
                    Ok(Ok(guard)) => *watch.borrow_mut() = Some(guard),
                    Ok(Err(err)) => {
                        let mut app = app;
                        app.write().error = Some(format!("Automatic refresh is off: {err}"));
                        return;
                    }
                    Err(message) => {
                        let mut app = app;
                        app.write().error = Some(message);
                        return;
                    }
                }
                while let Some(mut change) = changes.recv().await {
                    while let Ok(more) = changes.try_recv() {
                        change = change.merge(more);
                    }
                    if !operations.is_running() {
                        session.refresh_after(change);
                    }
                }
            });
        }
    });

    use_hook(move || {
        session.refresh();
        spawn(async move {
            let mut keys = document::eval(KEY_LISTENER);
            while let Ok(key) = keys.recv::<String>().await {
                match key.as_str() {
                    "F5" => session.refresh(),
                    // Escape closes the innermost thing first.
                    "Escape" => {
                        if *settings_open.peek() {
                            settings_open.set(false);
                        } else if operations.dialog.peek().is_some() {
                            operations.dismiss();
                        } else if operations.menu.peek().is_some() {
                            let mut menu = operations.menu;
                            menu.set(None);
                        } else if !session.search_input.peek().is_empty() {
                            session.clear_search();
                        } else {
                            session.set_filter(None);
                        }
                    }
                    _ => {}
                }
            }
        });
    });

    let name = repo.name();
    let location = repo.root().display().to_string();
    let shown = layout.read().fitted(*total_width.read());
    let template = shown.grid_template();
    let dragging = drag.read().is_some();
    let mut start_drag = move |side: Side, x: f64| {
        let start_width = layout.peek().fitted(*total_width.peek()).width(side);
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
            div { class: "button-group",
                button {
                    title: "Fetch all remotes",
                    disabled: busy,
                    onclick: move |_| operations.start(Action::Fetch(FetchTarget::All)),
                    "↓ Fetch"
                }
                button {
                    title: "Pull the current branch",
                    disabled: busy,
                    onclick: move |_| operations.start(Action::Pull),
                    "⇣ Pull"
                }
                button {
                    title: "Push the current branch",
                    disabled: busy,
                    onclick: move |_| {
                        let repo = session.repo.peek().clone();
                        let mut app = app;
                        match push_case.current_branch(&repo) {
                            Ok(branch) => operations.start(Action::Push { branch }),
                            Err(err) => app.write().error = Some(format!("Push failed: {err}")),
                        }
                    },
                    "⇡ Push"
                }
            }
            button {
                title: "Apply a patch file to the work tree",
                disabled: busy,
                onclick: move |_| operations.apply_patch(),
                "⎘ Apply patch…"
            }
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
            button { title: "Settings", onclick: move |_| settings_open.set(true), "⚙ Settings" }
            button { onclick: move |_| on_pick.call(()), "Open…" }
            button { onclick: move |_| on_close.call(()), "Close" }
        }
        OperationStatus {}
        OngoingBanner {}
        div {
            class: if dragging { "columns dragging" } else { "columns" },
            style: "grid-template-columns: {template};",
            onmousemove: move |event| {
                let Some(active) = *drag.peek() else { return };
                let x = event.client_coordinates().x;
                let width = ColumnLayout::dragged_width(active.side, active.start_width, active.start_x, x);
                let total = *total_width.peek();
                layout.write().resize(active.side, width, total);
            },
            onmouseup: move |_| {
                if drag.take().is_some() {
                    keeper.save();
                }
            },
            onmouseleave: move |_| {
                if drag.take().is_some() {
                    keeper.save();
                }
            },
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
        ContextMenu {}
        DialogHost {}
    }
}
