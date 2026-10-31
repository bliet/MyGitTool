use super::operations::Operations;
use super::session::Session;
use crate::view::branch_tree::{BranchItem, BranchRowKind, branch_rows, remote_of_folder};
use crate::view::operation::{Action, branch_menu, remote_menu};
use application::Settings;
use dioxus::prelude::*;
use domain::WebLink;

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
