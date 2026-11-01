use super::operations::Operations;
use super::session::Session;
use crate::view::branch_tree::{
    BranchItem, BranchRowKind, TagItem, branch_rows, remote_of_folder, stash_summary,
};
use crate::view::history::Selection;
use crate::view::operation::{Action, branch_menu, remote_menu, stash_menu, tag_menu};
use application::Settings;
use dioxus::prelude::*;
use domain::{Stash, WebLink};

/// Indentation per tree level in pixels.
const INDENT: usize = 14;

#[component]
pub fn BranchesColumn() -> Element {
    let session = use_context::<Session>();
    let mut collapsed = session.collapsed;
    let error = session.refs_error.read().clone();
    let rows = match &*session.refs.read() {
        Some(refs) => branch_rows(refs, &collapsed.read()),
        None => Vec::new(),
    };
    let selected = session.selected_branch.read().clone();
    let selection = *session.selection.read();
    let remotes: Vec<String> = session
        .refs
        .read()
        .iter()
        .flat_map(|refs| refs.remotes.iter().map(|r| r.remote.name.clone()))
        .collect();
    let guides = use_context::<Signal<Settings>>().read().indent_guides;

    rsx! {
        section { class: "column branches",
            h2 { "Branches" }
            if let Some(error) = error {
                p { class: "column-error", "{error}" }
            }
            ul { class: if guides { "tree guides" } else { "tree" },
                for row in rows {
                    match row.kind {
                        BranchRowKind::Section(title) => rsx! {
                            li { class: "tree-section", "{title}" }
                        },
                        BranchRowKind::Folder { key, name, collapsed: is_collapsed, web } => rsx! {
                            li {
                                class: "tree-row folder",
                                title: if remote_of_folder(&key).is_some() { "Right-click to fetch" } else { "" },
                                oncontextmenu: {
                                    let remote = remote_of_folder(&key).map(str::to_owned);
                                    let web = web.clone();
                                    move |event: MouseEvent| {
                                        if let Some(remote) = &remote {
                                            consume_context::<Operations>().open_menu(&event, remote_menu(remote, web.as_ref()));
                                        }
                                    }
                                },
                                style: "padding-left: {folder_indent(row.depth)}px; --depth: {row.depth};",
                                onclick: move |_| {
                                    let mut set = collapsed.write();
                                    if !set.remove(&key) {
                                        set.insert(key.clone());
                                    }
                                },
                                span { class: "twisty", if is_collapsed { "▸" } else { "▾" } }
                                span { class: "label", "{name}" }
                                if let Some(web) = web.clone() {
                                    OpenInBrowserButton { web }
                                }
                            }
                        },
                        BranchRowKind::Tag(tag) => rsx! {
                            TagRow {
                                key: "{tag.refname()}",
                                selected: selected.as_deref() == Some(tag.refname().as_str()),
                                remotes: remotes.clone(),
                                tag,
                            }
                        },
                        BranchRowKind::Stash(stash) => rsx! {
                            StashRow {
                                key: "{stash.id}",
                                selected: selection == Some(Selection::Stash(stash.id)),
                                stash,
                            }
                        },
                        BranchRowKind::Branch(item) => rsx! {
                            BranchRow {
                                key: "{item.refname}",
                                selected: selected.as_deref() == Some(item.refname.as_str()),
                                depth: row.depth,
                                item,
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn BranchRow(item: BranchItem, depth: usize, selected: bool) -> Element {
    let session = use_context::<Session>();
    let mut class = String::from("tree-row branch");
    if item.is_head {
        class.push_str(" head");
    }
    if selected {
        class.push_str(" selected");
    }
    let tracking = match (item.ahead, item.behind) {
        (Some(ahead), Some(behind)) if ahead > 0 || behind > 0 => {
            let mut parts = Vec::new();
            if ahead > 0 {
                parts.push(format!("↑{ahead}"));
            }
            if behind > 0 {
                parts.push(format!("↓{behind}"));
            }
            Some(parts.join(" "))
        }
        _ if item.upstream_gone => Some("gone".to_owned()),
        _ => None,
    };
    let title = format!(
        "{}\nDouble-click to show only this branch's history\nRight-click to fetch, pull or push",
        item.name
    );
    let indent = folder_indent(depth) + 12;
    let click_item = item.clone();
    let filter_item = item.clone();
    let menu_item = item.clone();

    rsx! {
        li {
            class: "{class}",
            title: "{title}",
            style: "padding-left: {indent}px; --depth: {depth};",
            onclick: move |_| session.select_branch(&click_item),
            ondoubleclick: move |_| session.set_filter(Some(filter_item.clone())),
            oncontextmenu: move |event| {
                session.select_branch(&menu_item);
                consume_context::<Operations>().open_menu(&event, branch_menu(&menu_item));
            },
            span { class: "marker", if item.is_head { "●" } }
            span { class: "label", "{item.label}" }
            if let Some(tracking) = tracking {
                span { class: "tracking", "{tracking}" }
            }
            if let Some(web) = item.web.clone() {
                OpenInBrowserButton { web }
            }
        }
    }
}

#[component]
fn TagRow(tag: TagItem, selected: bool, remotes: Vec<String>) -> Element {
    let mut session = use_context::<Session>();
    let class = if selected {
        "tree-row tag selected"
    } else {
        "tree-row tag"
    };
    let indent = folder_indent(1) + 12;
    let refname = tag.refname();
    let title = format!(
        "{}\nClick to show the tagged commit\nRight-click to push or delete",
        tag.name
    );
    let target = tag.target;
    let menu = tag_menu(&tag, &remotes);
    rsx! {
        li {
            class,
            title: "{title}",
            style: "padding-left: {indent}px; --depth: 1;",
            onclick: {
                let refname = refname.clone();
                move |_| {
                    session.selected_branch.set(Some(refname.clone()));
                    session.select_and_reveal(target);
                }
            },
            oncontextmenu: move |event| {
                session.selected_branch.set(Some(refname.clone()));
                session.select_and_reveal(target);
                consume_context::<Operations>().open_menu(&event, menu.clone());
            },
            span { class: "marker tag-marker", "◆" }
            span { class: "label", "{tag.name}" }
        }
    }
}

#[component]
fn StashRow(stash: Stash, selected: bool) -> Element {
    let session = use_context::<Session>();
    let class = if selected {
        "tree-row stash selected"
    } else {
        "tree-row stash"
    };
    let (text, branch) = stash_summary(&stash.message);
    let title = format!(
        "{}: {}\nClick to show its changes\nRight-click to apply, pop or drop",
        stash.name(),
        stash.message
    );
    let id = stash.id;
    let menu = stash_menu(&stash);
    rsx! {
        li {
            class,
            title: "{title}",
            style: "padding-left: {folder_indent(0) + 12}px; --depth: 0;",
            onclick: move |_| session.select_stash(id),
            oncontextmenu: move |event| {
                session.select_stash(id);
                consume_context::<Operations>().open_menu(&event, menu.clone());
            },
            span { class: "marker stash-marker", "≡" }
            span { class: "label", "{text}" }
            if let Some(branch) = branch {
                span { class: "tracking", "{branch}" }
            }
        }
    }
}

/// Opens a page on the remote's hoster; shown while the row is hovered.
#[component]
fn OpenInBrowserButton(web: WebLink) -> Element {
    let title = format!("Open on {}\n{}", web.hoster, web.url);
    rsx! {
        button {
            class: "icon open-web",
            title: "{title}",
            onclick: move |event| {
                // The row would select or collapse on the click.
                event.stop_propagation();
                consume_context::<Operations>().start(Action::OpenInBrowser(web.clone()));
            },
            ondoubleclick: move |event| event.stop_propagation(),
            "↗"
        }
    }
}

fn folder_indent(depth: usize) -> usize {
    8 + depth * INDENT
}
