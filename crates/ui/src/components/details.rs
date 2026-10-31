use super::diff_window::DiffWindows;
use super::session::{DetailsState, Session};
use super::{now_seconds, update_settings};
use crate::launch::Services;
use crate::state::AppState;
use crate::view::file_tree::{FileViewMode, file_rows};
use crate::view::format::details_date;
use application::{DateFormat, RefLabelKind, Settings};
use dioxus::prelude::*;
use domain::{CommitDetails, DiffSource, FileChange, Signature};
use std::cell::RefCell;
use std::rc::Rc;

/// Indentation per tree level in pixels.
const INDENT: usize = 14;

/// Keeps the clipboard alive: on Linux, copied text is served by the
/// `Clipboard` instance and disappears when it is dropped.
#[derive(Clone, Default)]
struct ClipboardHolder(Rc<RefCell<Option<arboard::Clipboard>>>);

impl ClipboardHolder {
    fn copy(&self, text: &str) -> Result<(), String> {
        let mut slot = self.0.borrow_mut();
        if slot.is_none() {
            *slot = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        match slot.as_mut() {
            Some(clipboard) => clipboard.set_text(text).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }
}

#[component]
pub fn DetailsColumn() -> Element {
    let session = use_context::<Session>();
    use_context_provider(ClipboardHolder::default);
    let details = session.details.read().clone();
    let work_tree = session.work_tree.read().clone();

    rsx! {
        section { class: "column details",
            h2 { "Details" }
            match details {
                DetailsState::Empty => rsx! {
                    p { class: "placeholder", "Select a commit to see its details." }
                },
                DetailsState::Loading => rsx! {
                    p { class: "placeholder", "Loading…" }
                },
                DetailsState::Error(message) => rsx! {
                    p { class: "column-error", "{message}" }
                },
                DetailsState::Uncommitted => rsx! {
                    div { class: "details-body",
                        h3 { class: "subject", "Uncommitted changes" }
                        ChangedFiles { changes: work_tree, source: DiffSource::WorkTree }
                    }
                },
                DetailsState::Commit(details) => rsx! {
                    CommitView { details: *details }
                },
            }
        }
    }
}

#[component]
fn CommitView(details: CommitDetails) -> Element {
    let session = use_context::<Session>();
    let date_format = use_context::<Signal<Settings>>().read().date_format;
    let now = now_seconds();
    let commit = &details.commit;
    let id = commit.id;
    let short = id.short();
    let labels = session.ref_index.read().labels(&id).to_vec();
    let same_person = details.committer.name == commit.author.name
        && details.committer.email == commit.author.email
        && details.committer.time == commit.author.time;
    let body = details
        .message
        .split_once('\n')
        .map(|(_, rest)| rest.trim().to_owned())
        .unwrap_or_default();

    rsx! {
        div { class: "details-body",
            h3 { class: "subject selectable", "{commit.summary}" }
            if !body.is_empty() {
                pre { class: "message selectable", "{body}" }
            }
            dl { class: "fields",
                dt { "Commit" }
                dd {
                    code { class: "selectable", "{short}" }
                    CopyButton { text: id.short(), label: "Copy short hash" }
                    code { class: "selectable full-hash", "{id}" }
                    CopyButton { text: id.to_hex(), label: "Copy full hash" }
                }
                dt { if commit.parents.len() > 1 { "Parents" } else { "Parent" } }
                dd {
                    if commit.parents.is_empty() {
                        span { class: "muted", "none (root commit)" }
                    }
                    for (parent, parent_short) in commit.parents.iter().map(|p| (*p, p.short())) {
                        button {
                            class: "link",
                            title: "Go to {parent}",
                            onclick: move |_| session.select_and_reveal(parent),
                            "{parent_short}"
                        }
                    }
                }
                dt { "Author" }
                dd { {person(&commit.author, date_format, now)} }
                dt { "Committer" }
                dd {
                    if same_person {
                        span { class: "muted", "same as author" }
                    } else {
                        {person(&details.committer, date_format, now)}
                    }
                }
                if !labels.is_empty() {
                    dt { "Refs" }
                    dd {
                        for label in labels {
                            span {
                                class: match label.kind {
                                    RefLabelKind::DetachedHead | RefLabelKind::Local { is_head: true } => "badge local head",
                                    RefLabelKind::Local { .. } => "badge local",
                                    RefLabelKind::Remote => "badge remote",
                                    RefLabelKind::Tag => "badge tag",
                                },
                                "{label.name}"
                            }
                        }
                    }
                }
            }
            ChangedFiles {
                changes: details.changes.clone(),
                source: DiffSource::Commit {
                    id,
                    parent: commit.parents.first().copied(),
                },
            }
        }
    }
}

fn person(signature: &Signature, format: DateFormat, now: i64) -> Element {
    let who = format!("{} <{}>", signature.name, signature.email);
    let when = details_date(signature.time, format, now);
    rsx! {
        div { class: "selectable", "{who}" }
        div { class: "muted selectable", "{when}" }
    }
}

#[component]
fn CopyButton(text: String, label: &'static str) -> Element {
    let clipboard = use_context::<ClipboardHolder>();
    let mut app = use_context::<Signal<AppState>>();
    let mut copied = use_signal(|| false);
    rsx! {
        button {
            class: "icon copy",
            title: "{label}",
            onclick: move |_| match clipboard.copy(&text) {
                Ok(()) => copied.set(true),
                Err(err) => app.write().error = Some(format!("Cannot copy to the clipboard: {err}")),
            },
            onmouseleave: move |_| copied.set(false),
            if copied() { "✓" } else { "⧉" }
        }
    }
}

#[component]
fn ChangedFiles(changes: Vec<FileChange>, source: DiffSource) -> Element {
    let session = use_context::<Session>();
    let services = use_context::<Services>();
    let windows = use_context::<DiffWindows>();
    let open_diff = move |change: FileChange| {
        windows.open(
            services.compute_file_diff.clone(),
            session.repo.peek().clone(),
            source,
            change,
        );
    };
    let app = use_context::<Signal<AppState>>();
    let settings = use_context::<Signal<Settings>>();
    let Settings {
        file_view: current,
        indent_guides: guides,
        ..
    } = *settings.read();
    // The toggle is the file view setting, so it sticks and the settings
    // dialog shows the same choice.
    let set_mode = use_callback({
        let services = use_context::<Services>();
        move |mode: FileViewMode| {
            update_settings(&services, settings, app, |s| s.file_view = mode);
        }
    });
    let rows = file_rows(&changes, current);
    let additions: usize = changes.iter().map(|c| c.additions).sum();
    let deletions: usize = changes.iter().map(|c| c.deletions).sum();
    let count_label = match changes.len() {
        1 => "1 changed file".to_owned(),
        n => format!("{n} changed files"),
    };

    rsx! {
        div { class: "files-header",
            span { class: "files-title", "{count_label}" }
            span { class: "additions", "+{additions}" }
            span { class: "deletions", "−{deletions}" }
            div { class: "spacer" }
            div { class: "toggle",
                button {
                    class: if current == FileViewMode::Tree { "active" } else { "" },
                    onclick: move |_| set_mode.call(FileViewMode::Tree),
                    "Tree"
                }
                button {
                    class: if current == FileViewMode::Flat { "active" } else { "" },
                    onclick: move |_| set_mode.call(FileViewMode::Flat),
                    "Flat"
                }
            }
        }
        ul { class: if guides { "files guides" } else { "files" },
            for (row, indent) in rows.into_iter().map(|r| { let indent = 8 + r.depth * INDENT; (r, indent) }) {
                match row.change.map(|i| &changes[i]) {
                    None => rsx! {
                        li { class: "file dir", style: "padding-left: {indent}px; --depth: {row.depth};",
                            span { class: "status" }
                            span { class: "path", "{row.name}/" }
                        }
                    },
                    Some(change) => rsx! {
                        li {
                            class: "file",
                            style: "padding-left: {indent}px; --depth: {row.depth};",
                            title: "{change.path} — double-click to show the diff",
                            ondoubleclick: {
                                let change = change.clone();
                                let open_diff = open_diff.clone();
                                move |_| open_diff(change.clone())
                            },
                            span { class: "status s{change.kind.letter()}", {change.kind.letter().to_string()} }
                            span { class: "path",
                                if let Some(old) = &change.old_path {
                                    span { class: "muted", "{old} → " }
                                }
                                "{row.name}"
                            }
                            if change.binary {
                                span { class: "muted", "binary" }
                            } else {
                                span { class: "additions", "+{change.additions}" }
                                span { class: "deletions", "−{change.deletions}" }
                            }
                        }
                    },
                }
            }
        }
    }
}
