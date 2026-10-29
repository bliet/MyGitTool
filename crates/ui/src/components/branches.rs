use super::session::Session;
use crate::view::branch_tree::{BranchItem, BranchRowKind, branch_rows};
use dioxus::prelude::*;

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

    rsx! {
        section { class: "column branches",
            h2 { "Branches" }
            if let Some(error) = error {
                p { class: "column-error", "{error}" }
            }
            ul { class: "tree",
                for row in rows {
                    match row.kind {
                        BranchRowKind::Section(title) => rsx! {
                            li { class: "tree-section", "{title}" }
                        },
                        BranchRowKind::Folder { key, name, collapsed: is_collapsed } => rsx! {
                            li {
                                class: "tree-row folder",
                                style: "padding-left: {folder_indent(row.depth)}px;",
                                onclick: move |_| {
                                    let mut set = collapsed.write();
                                    if !set.remove(&key) {
                                        set.insert(key.clone());
                                    }
                                },
                                span { class: "twisty", if is_collapsed { "▸" } else { "▾" } }
                                span { class: "label", "{name}" }
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
        "{}\nDouble-click to show only this branch's history",
        item.name
    );
    let indent = folder_indent(depth) + 12;
    let click_item = item.clone();
    let filter_item = item.clone();

    rsx! {
        li {
            class: "{class}",
            title: "{title}",
            style: "padding-left: {indent}px;",
            onclick: move |_| session.select_branch(&click_item),
            ondoubleclick: move |_| session.set_filter(Some(filter_item.clone())),
            span { class: "marker", if item.is_head { "●" } }
            span { class: "label", "{item.label}" }
            if let Some(tracking) = tracking {
                span { class: "tracking", "{tracking}" }
            }
        }
    }
}

fn folder_indent(depth: usize) -> usize {
    8 + depth * INDENT
}
