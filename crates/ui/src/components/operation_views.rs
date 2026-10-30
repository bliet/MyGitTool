//! The status strip, dialogs and context menu of the remote operations.

use super::operations::{Dialog, Operations, dialog_title};
use crate::view::operation::{progress_fraction, progress_text};
use dioxus::prelude::*;
use domain::{Credential, CredentialRequest};

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
                    Dialog::DirtyWorkTree { files, .. } => rsx! {
                        p { "Commit or stash the changes in these files first:" }
                        FileList { files }
                        div { class: "dialog-buttons",
                            button { class: "primary", onclick: move |_| operations.dismiss(), "OK" }
                        }
                    },
                    Dialog::Conflicts { operation, files } => rsx! {
                        p { "These files have conflicts:" }
                        FileList { files }
                        p { class: "muted",
                            "Abort the {operation} to return to the state before the pull, or leave it "
                            "and resolve the conflicts with another tool."
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
