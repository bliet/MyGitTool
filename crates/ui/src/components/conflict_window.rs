//! The conflict resolver: a separate desktop window per conflicted file.

use super::blocking;
use crate::view::conflict::{
    Resolver, clean_preview, progress_text, resolution_label, shown, side_titles, take_label,
    whole_file_text,
};
use crate::view::diff::Direction;
use application::{LoadConflict, OperationError, ResolveConflict};
use dioxus::prelude::*;
use domain::{
    ConflictContent, ConflictHunk, ConflictSide, MergeChunk, OngoingOperation, RepositoryInfo,
    Resolution, SideKind,
};

/// Lines of a long clean part shown above and below the folded rest.
const CONTEXT: usize = 3;

/// The window's keys. Escape in the editor leaves the editor alone, so a
/// stray press never discards the work.
const WINDOW_SCRIPT: &str = r#"
    window.__myGitToolConflict?.abort();
    const controller = new AbortController();
    window.__myGitToolConflict = controller;
    window.addEventListener("keydown", (event) => {
        if (!["F5", "F6", "F7", "Escape"].includes(event.key)) return;
        if (event.key === "Escape" && event.target.closest?.("textarea")) return;
        event.preventDefault();
        dioxus.send(event.key);
    }, { signal: controller.signal });
    await new Promise(() => {});
"#;

/// The use cases the resolver window needs; its root context.
#[derive(Clone)]
pub struct ConflictServices {
    pub load: LoadConflict,
    pub resolve: ResolveConflict,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Loading,
    /// The decisions live in the `Resolver` signal.
    Text,
    WholeFile {
        ours: SideKind,
        theirs: SideKind,
    },
    Error(String),
}

/// A part of the file as the window lists it.
enum Item {
    Clean(String),
    Conflict {
        number: usize,
        hunk: ConflictHunk,
        resolution: Option<Resolution>,
    },
}

type Task = Box<dyn FnOnce() -> Result<(), OperationError> + Send>;

#[component]
pub(super) fn ConflictWindow(
    repo: RepositoryInfo,
    path: String,
    operation: Option<OngoingOperation>,
) -> Element {
    let services = use_context::<ConflictServices>();
    let mut state = use_signal(|| State::Loading);
    let mut resolver = use_signal(|| Resolver::new(Vec::new()));
    let mut current = use_signal(|| None::<usize>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let load = use_callback({
        let load = services.load.clone();
        let repo = repo.clone();
        let path = path.clone();
        move |()| {
            let (load, repo, path) = (load.clone(), repo.clone(), path.clone());
            spawn(async move {
                match blocking(move || load.load(&repo, &path)).await {
                    Ok(Ok(conflict)) => match conflict.content {
                        ConflictContent::Text(chunks) => {
                            resolver.set(Resolver::new(chunks));
                            current.set(None);
                            state.set(State::Text);
                        }
                        ConflictContent::WholeFile { ours, theirs } => {
                            state.set(State::WholeFile { ours, theirs });
                        }
                    },
                    Ok(Err(err)) => state.set(State::Error(err.to_string())),
                    Err(message) => state.set(State::Error(message)),
                }
            });
        }
    });

    // Runs a resolution and closes the window once the file is staged.
    let mut finish = move |task: Task| {
        if *busy.peek() {
            return;
        }
        busy.set(true);
        error.set(None);
        spawn(async move {
            match blocking(task).await {
                Ok(Ok(())) => dioxus::desktop::window().close(),
                Ok(Err(err)) => error.set(Some(err.to_string())),
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    };

    let mut navigate = move |direction: Direction| {
        let Some(next) = resolver.peek().step(*current.peek(), direction) else {
            return;
        };
        current.set(Some(next));
        document::eval(&format!(
            "document.getElementById('conflict-{next}')?.scrollIntoView({{ block: 'center', behavior: 'smooth' }});"
        ));
    };

    use_hook(move || {
        load.call(());
        spawn(async move {
            let mut keys = document::eval(WINDOW_SCRIPT);
            while let Ok(key) = keys.recv::<String>().await {
                match key.as_str() {
                    "F5" => load.call(()),
                    "F6" => navigate(Direction::Previous),
                    "F7" => navigate(Direction::Next),
                    "Escape" => dioxus::desktop::window().close(),
                    _ => {}
                }
            }
        });
    });

    let save = {
        let (resolve, repo, path) = (services.resolve.clone(), repo.clone(), path.clone());
        move |_| {
            let Some(text) = resolver.peek().merged() else {
                return;
            };
            let (resolve, repo, path) = (resolve.clone(), repo.clone(), path.clone());
            finish(Box::new(move || resolve.resolve_text(&repo, &path, &text)));
        }
    };
    let keep = {
        let (resolve, repo, path) = (services.resolve.clone(), repo.clone(), path.clone());
        move |_| {
            let (resolve, repo, path) = (resolve.clone(), repo.clone(), path.clone());
            finish(Box::new(move || resolve.keep_work_tree(&repo, &path)));
        }
    };
    let mut take = {
        let (resolve, repo, path) = (services.resolve.clone(), repo.clone(), path.clone());
        move |side: ConflictSide| {
            let (resolve, repo, path) = (resolve.clone(), repo.clone(), path.clone());
            finish(Box::new(move || resolve.take_side(&repo, &path, side)));
        }
    };

    let (ours_title, theirs_title) = side_titles(operation);
    let state = state.read().clone();
    let is_text = state == State::Text;
    let decisions = resolver.read();
    let total = decisions.conflict_count();
    let ready = decisions.merged().is_some();
    let progress = progress_text(decisions.resolved_count(), total);
    let mut number = 0;
    let items: Vec<Item> = decisions
        .chunks()
        .iter()
        .map(|chunk| match chunk {
            MergeChunk::Clean(text) => Item::Clean(text.clone()),
            MergeChunk::Conflict(hunk) => {
                number += 1;
                Item::Conflict {
                    number: number - 1,
                    hunk: hunk.clone(),
                    resolution: decisions.resolution(number - 1).cloned(),
                }
            }
        })
        .collect();
    drop(decisions);
    let focused = current();
    let working = busy();

    rsx! {
        div { class: "conflict-window",
            header { class: "conflict-header",
                span { class: "path selectable", title: "{path}", "{path}" }
                if is_text {
                    span { class: "muted", "{progress}" }
                }
            }
            match state {
                State::Loading => rsx! { p { class: "placeholder", "Loading…" } },
                State::Error(message) => rsx! { p { class: "column-error", "{message}" } },
                State::WholeFile { ours, theirs } => rsx! {
                    div { class: "conflict-body whole-file",
                        p { {whole_file_text(ours, theirs)} }
                        div { class: "button-row",
                            button {
                                disabled: working,
                                onclick: {
                                    let mut take = take.clone();
                                    move |_| take(ConflictSide::Ours)
                                },
                                {take_label(ConflictSide::Ours, ours)}
                            }
                            button {
                                disabled: working,
                                onclick: move |_| take(ConflictSide::Theirs),
                                {take_label(ConflictSide::Theirs, theirs)}
                            }
                        }
                    }
                },
                State::Text => rsx! {
                    div { class: "conflict-body",
                        if total == 0 {
                            p { class: "muted",
                                "The versions merge without conflicts now. Save to take the merged file."
                            }
                        }
                        for (index, item) in items.into_iter().enumerate() {
                            match item {
                                Item::Clean(text) => rsx! { CleanBlock { key: "{index}", text } },
                                Item::Conflict { number, hunk, resolution } => rsx! {
                                    ConflictCard {
                                        key: "{index}",
                                        number,
                                        hunk,
                                        resolution,
                                        current: focused == Some(number),
                                        ours_title,
                                        theirs_title,
                                        on_focus: move |()| current.set(Some(number)),
                                        on_resolve: move |resolution| {
                                            resolver.write().resolve(number, resolution);
                                            current.set(Some(number));
                                        },
                                    }
                                },
                            }
                        }
                    }
                },
            }
            footer { class: "status-bar",
                if let Some(message) = error() {
                    span { class: "field-error", role: "alert", "{message}" }
                } else {
                    span { class: "muted", "F7 next conflict · F6 previous · F5 reload · Esc close" }
                }
                div { class: "spacer" }
                button {
                    disabled: working,
                    title: "Stage the file as it is now in the work tree, e.g. after resolving it with another tool",
                    onclick: keep,
                    "Mark resolved as it is"
                }
                if is_text {
                    button {
                        class: "primary",
                        disabled: working || !ready,
                        title: if ready { "Write the merged file and stage it" } else { "Decide every conflict first" },
                        onclick: save,
                        "Save and mark resolved"
                    }
                }
            }
        }
    }
}

/// Lines both sides agree on; long runs are folded.
#[component]
fn CleanBlock(text: String) -> Element {
    let mut expanded = use_signal(|| false);
    let preview = clean_preview(&text, CONTEXT);
    if preview.hidden == 0 || expanded() {
        return rsx! {
            pre { class: "merge-text clean selectable", {shown(&text).to_owned()} }
        };
    }
    rsx! {
        pre { class: "merge-text clean selectable", {shown(&preview.head).to_owned()} }
        button {
            class: "merge-fold",
            title: "Show the unchanged lines",
            onclick: move |_| expanded.set(true),
            "⋯ {preview.hidden} unchanged lines"
        }
        pre { class: "merge-text clean selectable", {shown(&preview.tail).to_owned()} }
    }
}

/// One conflict: both sides with the choices, or the decision taken.
#[component]
fn ConflictCard(
    number: usize,
    hunk: ConflictHunk,
    resolution: Option<Resolution>,
    current: bool,
    ours_title: &'static str,
    theirs_title: &'static str,
    on_focus: EventHandler<()>,
    on_resolve: EventHandler<Option<Resolution>>,
) -> Element {
    let mut editing = use_signal(|| None::<String>);
    let mut show_base = use_signal(|| false);
    let class = if current {
        "conflict-card current"
    } else {
        "conflict-card"
    };
    let choices = [
        (Resolution::Ours, "Use ours"),
        (Resolution::Theirs, "Use theirs"),
        (Resolution::OursThenTheirs, "Ours, then theirs"),
        (Resolution::TheirsThenOurs, "Theirs, then ours"),
    ];
    // The editor starts from the decision taken, or from ours.
    let draft = resolution
        .as_ref()
        .map_or_else(|| hunk.ours.clone(), |r| r.apply(&hunk));
    let result = resolution.as_ref().map(|r| r.apply(&hunk));

    rsx! {
        div { id: "conflict-{number}", class, onclick: move |_| on_focus.call(()),
            div { class: "conflict-card-header",
                span { class: "conflict-title", "Conflict {number + 1}" }
                if let Some(resolution) = &resolution {
                    span { class: "resolved-badge", "✓ {resolution_label(resolution)}" }
                }
                div { class: "spacer" }
                if editing().is_none() {
                    if resolution.is_some() {
                        button { class: "small", onclick: move |_| on_resolve.call(None), "Change" }
                    } else {
                        for (choice, label) in choices {
                            button {
                                class: "small",
                                onclick: move |_| on_resolve.call(Some(choice.clone())),
                                "{label}"
                            }
                        }
                    }
                    button {
                        class: "small",
                        title: "Write the lines yourself",
                        onclick: move |_| editing.set(Some(draft.clone())),
                        "Edit…"
                    }
                }
            }
            if let Some(text) = editing() {
                textarea {
                    class: "merge-editor",
                    rows: "{text.lines().count().clamp(3, 20)}",
                    value: "{text}",
                    spellcheck: "false",
                    oninput: move |e| editing.set(Some(e.value())),
                    onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                }
                div { class: "button-row",
                    button { class: "small", onclick: move |_| editing.set(None), "Cancel" }
                    button {
                        class: "small primary",
                        onclick: move |_| {
                            if let Some(text) = editing.take() {
                                on_resolve.call(Some(Resolution::Custom(text)));
                            }
                        },
                        "Use this text"
                    }
                }
            } else if let Some(result) = result {
                {lines(&result, "merge-text result")}
            } else {
                div { class: "conflict-sides",
                    div { class: "conflict-side ours",
                        div { class: "side-title", "{ours_title}" }
                        {lines(&hunk.ours, "merge-text")}
                    }
                    div { class: "conflict-side theirs",
                        div { class: "side-title", "{theirs_title}" }
                        {lines(&hunk.theirs, "merge-text")}
                    }
                }
                button {
                    class: "link",
                    onclick: move |_| show_base.toggle(),
                    if show_base() { "Hide the base version" } else { "Show the base version" }
                }
                if show_base() {
                    div { class: "conflict-side base",
                        div { class: "side-title", "Base: before both changes" }
                        {lines(&hunk.base, "merge-text")}
                    }
                }
            }
        }
    }
}

/// A block of lines, or a note that there are none.
fn lines(text: &str, class: &'static str) -> Element {
    if text.is_empty() {
        return rsx! { p { class: "muted empty-side", "(no lines)" } };
    }
    rsx! { pre { class: "{class} selectable", {shown(text).to_owned()} } }
}
