//! The status strip, dialogs and context menu of the remote operations.

use super::operations::{Dialog, Operations, dialog_title};
use super::session::Session;
use crate::view::operation::{can_continue, ongoing_text, progress_fraction, progress_text};
use dioxus::prelude::*;
use domain::{CommitId, Credential, CredentialRequest, validate_branch_name, validate_tag_name};

/// Shows the running operation with its progress, or the last result.
#[component]
pub fn OperationStatus() -> Element {
    let operations = use_context::<Operations>();
    let mut notice = operations.notice;
    let running = operations.running.read().clone();
    let notice_text = notice.read().clone();

    if let Some(running) = running {
        let detail = running.progress.as_ref().map(progress_text);
        let percent = running
            .progress
            .as_ref()
            .and_then(progress_fraction)
            .map(|f| f * 100.0);
        return rsx! {
            div { class: "op-status running", role: "status",
                span { class: "spinner" }
                span { class: "op-label", "{running.label}" }
                if let Some(detail) = detail {
                    span { class: "op-detail", "{detail}" }
                }
                div { class: "spacer" }
                if let Some(percent) = percent {
                    div { class: "op-bar",
                        div { class: "op-bar-fill", style: "width: {percent:.0}%;" }
                    }
                }
                button { onclick: move |_| operations.cancel(), "Cancel" }
            }
        };
    }
    rsx! {
        if let Some(text) = notice_text {
            div { class: "op-status done", role: "status",
                span { class: "op-check", "✓" }
                span { class: "op-label", "{text}" }
                div { class: "spacer" }
                button { class: "icon", title: "Dismiss", onclick: move |_| notice.set(None), "✕" }
            }
        }
    }
}

/// The open dialog, if any, over a backdrop that blocks the window.
#[component]
pub fn DialogHost() -> Element {
    let operations = use_context::<Operations>();
    let Some(dialog) = operations.dialog.read().clone() else {
        return rsx! {};
    };
    let title = dialog_title(&dialog);
    rsx! {
        div { class: "dialog-backdrop",
            div { class: "dialog", role: "dialog", aria_modal: "true", aria_label: "{title}",
                h3 { "{title}" }
                match dialog {
                    Dialog::Credentials { request, .. } => rsx! {
                        CredentialForm { request }
                    },
                    Dialog::SetUpstream { branch, remotes } => rsx! {
                        UpstreamForm { branch, remotes }
                    },
                    Dialog::DirtyWorkTree { intro, files, .. } => rsx! {
                        p { "{intro}" }
                        FileList { files }
                        div { class: "dialog-buttons",
                            button { class: "primary", onclick: move |_| operations.dismiss(), "OK" }
                        }
                    },
                    Dialog::CreateBranch { start, error, .. } => rsx! {
                        CreateBranchForm { start, error }
                    },
                    Dialog::ConfirmDeleteUnmerged { branch, commits } => rsx! {
                        p {
                            if commits == 1 {
                                "{branch} has 1 commit that is not in the current branch. "
                            } else {
                                "{branch} has {commits} commits that are not in the current branch. "
                            }
                            "Deleting the branch may lose them."
                        }
                        div { class: "dialog-buttons",
                            button { onclick: move |_| operations.dismiss(), "Cancel" }
                            button {
                                class: "primary danger",
                                onclick: move |_| operations.delete_local(branch.clone(), true),
                                "Delete anyway"
                            }
                        }
                    },
                    Dialog::CreateTag { target, error, .. } => rsx! {
                        CreateTagForm { target, error }
                    },
                    Dialog::ConfirmDeleteRemoteTag { remote, tag } => rsx! {
                        p { "This deletes the tag on the server for everyone who uses {remote}. The tag here stays." }
                        div { class: "dialog-buttons",
                            button { onclick: move |_| operations.dismiss(), "Cancel" }
                            button {
                                class: "primary danger",
                                onclick: move |_| operations.delete_remote_tag(remote.clone(), tag.clone()),
                                "Delete from {remote}"
                            }
                        }
                    },
                    Dialog::ConfirmDeleteRemote { remote, branch } => rsx! {
                        p { "This deletes the branch on the server for everyone who uses {remote}." }
                        div { class: "dialog-buttons",
                            button { onclick: move |_| operations.dismiss(), "Cancel" }
                            button {
                                class: "primary danger",
                                onclick: move |_| operations.delete_remote(remote.clone(), branch.clone()),
                                "Delete from {remote}"
                            }
                        }
                    },
                    Dialog::Conflicts { operation, files, .. } => rsx! {
                        p { "These files have conflicts:" }
                        FileList { files }
                        p { class: "muted",
                            "Abort the {operation} to return to the state before it, or leave it, "
                            "resolve the conflicts with another tool, and then continue it here."
                        }
                        div { class: "dialog-buttons",
                            button { onclick: move |_| operations.leave_conflicts(), "Leave for external resolution" }
                            button { class: "primary", onclick: move |_| operations.abort(), "Abort {operation}" }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn CreateBranchForm(start: CommitId, error: Option<String>) -> Element {
    let operations = use_context::<Operations>();
    let mut name = use_signal(String::new);
    let mut checkout = use_signal(|| true);
    let typed = name.read().trim().to_owned();
    // The format is checked as the user types; whether the name is taken is
    // checked when creating.
    let invalid = if typed.is_empty() {
        None
    } else {
        validate_branch_name(&typed).err().map(|e| e.to_string())
    };
    let message = invalid.clone().or(error);
    let has_message = message.is_some();
    let ready = !typed.is_empty() && invalid.is_none();
    let submit = move || {
        let typed = name.peek().trim().to_owned();
        if !typed.is_empty() && validate_branch_name(&typed).is_ok() {
            operations.create_branch(typed, start, *checkout.peek());
        }
    };

    rsx! {
        div { class: "dialog-fields",
            label { "Branch name"
                input {
                    r#type: "text",
                    value: "{name}",
                    placeholder: "feature/my-change",
                    spellcheck: "false",
                    aria_invalid: "{has_message}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            submit();
                        }
                    },
                    onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                }
            }
            if let Some(message) = message {
                p { class: "field-error", role: "alert", "{message}" }
            }
            label { class: "checkbox",
                input {
                    r#type: "checkbox",
                    checked: checkout(),
                    onchange: move |e| checkout.set(e.checked()),
                }
                "Check out the new branch"
            }
        }
        div { class: "dialog-buttons",
            button { onclick: move |_| operations.dismiss(), "Cancel" }
            button { class: "primary", disabled: !ready, onclick: move |_| submit(), "Create branch" }
        }
    }
}

#[component]
fn CreateTagForm(target: CommitId, error: Option<String>) -> Element {
    let operations = use_context::<Operations>();
    let mut name = use_signal(String::new);
    let mut message = use_signal(String::new);
    let typed = name.read().trim().to_owned();
    let invalid = if typed.is_empty() {
        None
    } else {
        validate_tag_name(&typed).err().map(|e| e.to_string())
    };
    let shown = invalid.clone().or(error);
    let has_message = shown.is_some();
    let ready = !typed.is_empty() && invalid.is_none();
    let submit = move || {
        let typed = name.peek().trim().to_owned();
        if !typed.is_empty() && validate_tag_name(&typed).is_ok() {
            operations.create_tag(typed, target, message.peek().clone());
        }
    };

    rsx! {
        div { class: "dialog-fields",
            label { "Tag name"
                input {
                    r#type: "text",
                    value: "{name}",
                    placeholder: "v1.0.0",
                    spellcheck: "false",
                    aria_invalid: "{has_message}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            submit();
                        }
                    },
                    onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                }
            }
            if let Some(shown) = shown {
                p { class: "field-error", role: "alert", "{shown}" }
            }
            label { "Message (optional)"
                textarea {
                    rows: "3",
                    value: "{message}",
                    placeholder: "With a message, the tag is annotated",
                    oninput: move |e| message.set(e.value()),
                }
            }
        }
        div { class: "dialog-buttons",
            button { onclick: move |_| operations.dismiss(), "Cancel" }
            button { class: "primary", disabled: !ready, onclick: move |_| submit(), "Create tag" }
        }
    }
}

/// Shown while a merge, rebase or similar is in progress, e.g. after
/// conflicts or when started outside the app.
#[component]
pub fn OngoingBanner() -> Element {
    let session = use_context::<Session>();
    let operations = use_context::<Operations>();
    let Some(state) = session.ongoing.read().clone() else {
        return rsx! {};
    };
    let text = ongoing_text(&state);
    let operation = state.operation;
    let busy = operations.is_running();
    rsx! {
        div { class: "ongoing-banner", role: "status",
            span { class: "ongoing-icon", "⚠" }
            span { class: "message selectable", "{text}" }
            if can_continue(operation) {
                button {
                    disabled: busy || !state.conflicts.is_empty(),
                    title: if state.conflicts.is_empty() { "Commit the result and go on" } else { "Resolve and stage the conflicts first" },
                    onclick: move |_| operations.resume(),
                    "Continue {operation}"
                }
                button { disabled: busy, onclick: move |_| operations.abort(), "Abort {operation}" }
            }
        }
    }
}

#[component]
fn FileList(files: Vec<String>) -> Element {
    rsx! {
        ul { class: "dialog-files selectable",
            for file in files {
                li { key: "{file}", "{file}" }
            }
        }
    }
}

#[component]
fn CredentialForm(request: CredentialRequest) -> Element {
    let operations = use_context::<Operations>();
    let (prompt, username, needs_user) = match &request {
        CredentialRequest::UserPass { url, username } => (
            format!("{url} needs a user name and password (or token)."),
            username.clone().unwrap_or_default(),
            true,
        ),
        CredentialRequest::SshPassphrase { key, .. } => (
            format!("The key {} is protected by a passphrase.", key.display()),
            String::new(),
            false,
        ),
    };
    let mut user = use_signal(|| username);
    let mut secret = use_signal(String::new);
    let submit = move || {
        let credential = if needs_user {
            Credential::UserPass {
                username: user.peek().clone(),
                password: secret.peek().clone(),
            }
        } else {
            Credential::Passphrase(secret.peek().clone())
        };
        operations.answer(Some(credential));
    };
    let on_enter = move |event: KeyboardEvent| {
        if event.key() == Key::Enter {
            submit();
        }
    };

    rsx! {
        p { class: "selectable", "{prompt}" }
        p { class: "muted", "The answer is used for this operation only and is not stored." }
        div { class: "dialog-fields",
            if needs_user {
                label { "User name"
                    input {
                        r#type: "text",
                        value: "{user}",
                        autocomplete: "username",
                        oninput: move |e| user.set(e.value()),
                        onkeydown: on_enter,
                        onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                    }
                }
            }
            label { if needs_user { "Password or token" } else { "Passphrase" }
                input {
                    r#type: "password",
                    value: "{secret}",
                    oninput: move |e| secret.set(e.value()),
                    onkeydown: on_enter,
                    onmounted: move |e| async move {
                        if !needs_user {
                            let _ = e.set_focus(true).await;
                        }
                    },
                }
            }
        }
        div { class: "dialog-buttons",
            button { onclick: move |_| operations.answer(None), "Cancel" }
            button { class: "primary", onclick: move |_| submit(), "OK" }
        }
    }
}

#[component]
fn UpstreamForm(branch: String, remotes: Vec<String>) -> Element {
    let operations = use_context::<Operations>();
    let mut remote = use_signal(|| remotes.first().cloned().unwrap_or_default());
    let target = format!("{}/{branch}", remote.read());
    rsx! {
        p { "Push it and make the branch of the same name on the remote its upstream?" }
        if remotes.len() > 1 {
            div { class: "dialog-fields",
                label { "Remote"
                    select {
                        onchange: move |e| remote.set(e.value()),
                        for name in remotes.iter().cloned() {
                            option { key: "{name}", value: "{name}", selected: *remote.read() == name, "{name}" }
                        }
                    }
                }
            }
        }
        div { class: "dialog-buttons",
            button { onclick: move |_| operations.dismiss(), "Cancel" }
            button {
                class: "primary",
                onclick: move |_| operations.push_with_new_upstream(branch.clone(), remote.peek().clone()),
                "Push to {target}"
            }
        }
    }
}

/// The open context menu, closed by any click outside it.
#[component]
pub fn ContextMenu() -> Element {
    let operations = use_context::<Operations>();
    let mut menu_signal = operations.menu;
    let Some(menu) = menu_signal.read().clone() else {
        return rsx! {};
    };
    let busy = operations.is_running();
    rsx! {
        div {
            class: "menu-backdrop",
            onclick: move |_| menu_signal.set(None),
            oncontextmenu: move |_| menu_signal.set(None),
        }
        ul { class: "context-menu", role: "menu", style: "left: {menu.x}px; top: {menu.y}px;",
            for entry in menu.entries {
                if entry.separated {
                    li { class: "separator", role: "separator" }
                }
                li {
                    role: "menuitem",
                    class: if busy { "disabled" } else { "" },
                    onclick: move |_| {
                        if !busy {
                            operations.start(entry.action.clone());
                        }
                    },
                    "{entry.label}"
                }
            }
        }
    }
}
