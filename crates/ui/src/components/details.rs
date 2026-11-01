use super::child_windows::ChildWindows;
use super::conflict_window::ConflictServices;
use super::operations::Operations;
use super::session::{DetailsState, Session};
use super::{now_seconds, update_settings};
use crate::launch::Services;
use crate::state::AppState;
use crate::view::branch_tree::stash_summary;
use crate::view::commit::{
    Staging, changes_under, commit_blocker, commit_label, without_conflicts,
};
use crate::view::file_tree::{FileViewMode, file_rows};
use crate::view::format::details_date;
use crate::view::operation::{Action, stash_menu};
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
                    UncommittedView {}
                },
                DetailsState::Commit(details) => rsx! {
                    CommitView { details: *details }
                },
                DetailsState::Stash(details) => rsx! {
                    StashView { details: *details }
                },
            }
        }
    }
}

/// The staged and unstaged changes, and the box to commit the staged ones.
#[component]
fn UncommittedView() -> Element {
    let session = use_context::<Session>();
    let operations = use_context::<Operations>();
    let changes = session.work_tree.read().clone();
    let ongoing = session.ongoing.read().as_ref().map(|state| state.operation);
    let mut draft = session.commit_message;
    let message = draft.read().clone();
    let staged = changes.staged.len();
    let blocker = commit_blocker(staged, &message, ongoing, changes.conflicts.len());
    let busy = operations.is_running();
    let ready = blocker.is_none() && !busy;

    rsx! {
        div { class: "details-body",
            div { class: "subject-row",
                h3 { class: "subject", "Uncommitted changes" }
                button {
                    class: "small",
                    title: "Stash the local changes and clean the work tree",
                    disabled: busy,
                    onclick: move |_| operations.start(Action::SaveStash),
                    "Stash…"
                }
                button {
                    class: "small",
                    title: "Save the uncommitted changes, untracked files included, as a patch file",
                    disabled: busy,
                    onclick: move |_| operations.create_patch(),
                    "Patch…"
                }
            }
            div { class: "commit-box",
                textarea {
                    rows: "4",
                    value: "{message}",
                    placeholder: "Commit message",
                    aria_label: "Commit message",
                    spellcheck: "true",
                    oninput: move |e| draft.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        let modifiers = e.modifiers();
                        if e.key() == Key::Enter && (modifiers.ctrl() || modifiers.meta()) {
                            e.prevent_default();
                            if ready {
                                operations.commit();
                            }
                        }
                    },
                }
                div { class: "commit-row",
                    span { class: "hint", {blocker.clone().unwrap_or_default()} }
                    button {
                        class: "primary",
                        disabled: !ready,
                        title: blocker.unwrap_or_else(|| "Commit the staged changes (Ctrl+Enter)".to_owned()),
                        onclick: move |_| operations.commit(),
                        {commit_label(staged)}
                    }
                }
            }
            ChangedFiles {
                changes: changes.staged,
                source: DiffSource::Staged,
                staging: Staging::Unstage,
            }
            ChangedFiles {
                changes: changes.unstaged,
                source: DiffSource::Unstaged,
                staging: Staging::Stage,
                conflicts: changes.conflicts,
            }
        }
    }
}

/// A stash: what it holds, and buttons to apply, pop or drop it.
#[component]
fn StashView(details: CommitDetails) -> Element {
    let session = use_context::<Session>();
    let operations = use_context::<Operations>();
    let date_format = use_context::<Signal<Settings>>().read().date_format;
    let busy = operations.is_running();
    let commit = &details.commit;
    let id = commit.id;
    let stash = session
        .refs
        .read()
        .as_ref()
        .and_then(|refs| refs.stashes.iter().find(|s| s.id == id).cloned());
    let (text, branch) = stash_summary(&details.message);
    let text = text.trim().to_owned();
    let base = commit.parents.first().copied();
    let created = details_date(details.committer.time, date_format, now_seconds());

    rsx! {
        div { class: "details-body",
            h3 { class: "subject selectable", "{text}" }
            if let Some(stash) = stash {
                div { class: "button-row",
                    for entry in stash_menu(&stash) {
                        button {
                            class: if matches!(entry.action, Action::DropStash { .. }) { "danger-text" } else { "" },
                            disabled: busy,
                            onclick: move |_| operations.start(entry.action.clone()),
                            "{entry.label}"
                        }
                    }
                }
            }
            dl { class: "fields",
                dt { "Stash" }
                dd {
                    code { class: "selectable",
                        {stash_name(&session, id)}
                    }
                }
                if let Some(branch) = branch {
                    dt { "Branch" }
                    dd { span { class: "selectable", "{branch}" } }
                }
                dt { "Created" }
                dd { span { class: "selectable", "{created}" } }
                if let Some(base) = base {
                    dt { "Based on" }
                    dd {
                        button {
                            class: "link",
                            title: "Go to {base}",
                            onclick: move |_| session.select_and_reveal(base),
                            {base.short()}
                        }
                    }
                }
            }
            ChangedFiles {
                changes: details.changes.clone(),
                source: DiffSource::Commit { id, parent: base },
            }
        }
    }
}

/// The current name of the stash with the commit `id`, e.g. `stash@{0}`.
fn stash_name(session: &Session, id: domain::CommitId) -> String {
    session
        .refs
        .read()
        .as_ref()
        .and_then(|refs| refs.stashes.iter().find(|s| s.id == id))
        .map_or_else(|| id.short(), |stash| stash.name())
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
    let mut message_collapsed = session.message_collapsed;
    let collapsed = message_collapsed();
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
            if body.is_empty() {
                h3 { class: "subject selectable", "{commit.summary}" }
            } else {
                div { class: "subject-row",
                    button {
                        class: "icon",
                        title: if collapsed { "Show the whole message" } else { "Hide the message body" },
                        aria_expanded: "{!collapsed}",
                        onclick: move |_| message_collapsed.toggle(),
                        if collapsed { "▸" } else { "▾" }
                    }
                    h3 { class: "subject selectable", "{commit.summary}" }
                }
                if !collapsed {
                    pre { class: "message selectable", "{body}" }
                }
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

/// A list of changed files. With `staging`, every file and directory gets
/// a button that stages or unstages it. Files in `conflicts` (sorted) get
/// a button that opens the resolver instead.
#[component]
fn ChangedFiles(
    changes: Vec<FileChange>,
    source: DiffSource,
    staging: Option<Staging>,
    #[props(default)] conflicts: Vec<String>,
) -> Element {
    let session = use_context::<Session>();
    let operations = use_context::<Operations>();
    let busy = operations.is_running();
    let services = use_context::<Services>();
    let windows = use_context::<ChildWindows>();
    let open_diff = {
        let windows = windows.clone();
        let compute = services.compute_file_diff.clone();
        move |change: FileChange| {
            windows.open_diff(compute.clone(), session.repo.peek().clone(), source, change);
        }
    };
    let open_resolver = {
        let conflict_services = ConflictServices {
            load: services.load_conflict.clone(),
            resolve: services.resolve_conflict.clone(),
        };
        move |path: String| {
            let operation = session.ongoing.peek().as_ref().map(|state| state.operation);
            windows.open_conflict(
                conflict_services.clone(),
                session.repo.peek().clone(),
                path,
                operation,
            );
        }
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
    let mut collapsed_dirs = session.collapsed_dirs;
    let rows = file_rows(&changes, current, &collapsed_dirs.read());
    let additions: usize = changes.iter().map(|c| c.additions).sum();
    let deletions: usize = changes.iter().map(|c| c.deletions).sum();
    let count_label = match (staging, changes.len()) {
        (Some(staging), n) => staging.title(n),
        (None, 1) => "1 changed file".to_owned(),
        (None, n) => format!("{n} changed files"),
    };
    // What "Stage all" or "Unstage all" moves.
    let movable = without_conflicts(changes.clone(), &conflicts);
    // A button that moves `changes` to the other list.
    let move_button = move |changes: Vec<FileChange>, name: &str| {
        let staging = staging?;
        Some(rsx! {
            button {
                class: "icon stage",
                title: staging.tooltip(name),
                disabled: busy,
                onclick: move |e| {
                    e.stop_propagation();
                    operations.move_changes(staging, changes.clone());
                },
                // Two quick clicks must not also open the diff.
                ondoubleclick: |e| e.stop_propagation(),
                {staging.symbol()}
            }
        })
    };

    rsx! {
        div { class: "files-header",
            span { class: "files-title", "{count_label}" }
            if !changes.is_empty() {
                span { class: "additions", "+{additions}" }
                span { class: "deletions", "−{deletions}" }
            }
            div { class: "spacer" }
            if let Some(staging) = staging {
                button {
                    class: "small",
                    disabled: busy || movable.is_empty(),
                    title: if movable.len() < changes.len() { "Files with conflicts are left out; resolve them first" } else { "" },
                    onclick: {
                        let movable = movable.clone();
                        move |_| operations.move_changes(staging, movable.clone())
                    },
                    {staging.all_label()}
                }
            }
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
            if let (Some(staging), true) = (staging, changes.is_empty()) {
                li { class: "file empty muted", {staging.empty_text()} }
            }
            for (row, indent) in rows.into_iter().map(|r| { let indent = 8 + r.depth * INDENT; (r, indent) }) {
                match row.change.map(|i| &changes[i]) {
                    None => rsx! {
                        li {
                            class: "file dir",
                            style: "padding-left: {indent}px; --depth: {row.depth};",
                            title: if row.collapsed { "Click to expand" } else { "Click to collapse" },
                            aria_expanded: "{!row.collapsed}",
                            onclick: {
                                let dir = row.dir.clone();
                                move |_| {
                                    let mut set = collapsed_dirs.write();
                                    if !set.remove(&dir) {
                                        set.insert(dir.clone());
                                    }
                                }
                            },
                            span { class: "status twisty", if row.collapsed { "▸" } else { "▾" } }
                            span { class: "path", "{row.name}/" }
                            {move_button(without_conflicts(changes_under(&changes, &row.dir), &conflicts), &format!("{}/", row.dir))}
                        }
                    },
                    Some(change) if conflicts.binary_search(&change.path).is_ok() => rsx! {
                        li {
                            class: "file conflicted",
                            style: "padding-left: {indent}px; --depth: {row.depth};",
                            title: "{change.path} has conflicts — double-click to resolve",
                            ondoubleclick: {
                                let path = change.path.clone();
                                let open_resolver = open_resolver.clone();
                                move |_| open_resolver(path.clone())
                            },
                            span { class: "status sC", "!" }
                            span { class: "path", "{row.name}" }
                            button {
                                class: "small resolve",
                                onclick: {
                                    let path = change.path.clone();
                                    let open_resolver = open_resolver.clone();
                                    move |e| {
                                        e.stop_propagation();
                                        open_resolver(path.clone());
                                    }
                                },
                                ondoubleclick: |e| e.stop_propagation(),
                                "Resolve…"
                            }
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
                            {move_button(vec![change.clone()], &change.path)}
                        }
                    },
                }
            }
        }
    }
}
