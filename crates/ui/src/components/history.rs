use super::operations::Operations;
use super::session::{HISTORY_SCROLL_ID, Session, Viewport};
use crate::view::format::short_date;
use crate::view::history::{ROW_HEIGHT, Selection, visible_range};
use crate::view::operation::commit_menu;
use application::{EdgeHalf, GraphRow, RefLabel, RefLabelKind};
use dioxus::prelude::*;

/// Width of one graph lane in pixels.
const LANE_WIDTH: f64 = 14.0;
const NODE_RADIUS: f64 = 4.0;
const HALF_LANE: f64 = LANE_WIDTH / 2.0;
const HALF_ROW: f64 = ROW_HEIGHT / 2.0;
/// The number of lane colours defined in the stylesheet.
const COLORS: usize = 8;

#[component]
pub fn HistoryColumn() -> Element {
    let mut session = use_context::<Session>();
    let operations = use_context::<Operations>();
    let history = session.history.read();
    let uncommitted = session.work_tree.read().len();
    let offset = usize::from(uncommitted > 0);
    let files_label = if uncommitted == 1 {
        "1 file".to_owned()
    } else {
        format!("{uncommitted} files")
    };
    let total = history.len() + offset;
    let Viewport { scroll_top, height } = *session.viewport.read();
    let range = visible_range(scroll_top, height, total);
    let selection = *session.selection.read();
    let filter = session.filter.read().clone();
    let error = session.history_error.read().clone();
    let ref_index = session.ref_index.read();

    // Load the next page when the rendered rows approach the end.
    let commits_end = range.end.saturating_sub(offset);
    if history.needs_more(commits_end) && !*session.history_loading.peek() {
        session.load_more();
    }

    let commit_rows = range.start.saturating_sub(offset)..commits_end;
    let lanes = history.rows()[commit_rows.clone()]
        .iter()
        .map(|row| row.graph.width)
        .max()
        .unwrap_or(1)
        .max(1);
    let graph_width = lanes as f64 * LANE_WIDTH;
    let content_height = total as f64 * ROW_HEIGHT;
    let loading = !history.is_exhausted();
    let empty = history.is_empty() && !loading && uncommitted == 0;

    rsx! {
        section { class: "column history",
            div { class: "column-header",
                h2 { "History" }
                if let Some(branch) = filter {
                    span { class: "filter-chip", title: "Esc clears the filter",
                        "Branch: {branch.name}"
                        button {
                            class: "icon",
                            title: "Show all branches (Esc)",
                            onclick: move |_| session.set_filter(None),
                            "✕"
                        }
                    }
                }
            }
            if let Some(error) = error {
                p { class: "column-error", "{error}" }
            }
            if empty {
                p { class: "placeholder", "No commits yet." }
            }
            div {
                id: HISTORY_SCROLL_ID,
                class: "history-scroll",
                style: "--graph-width: {graph_width}px;",
                onscroll: move |event| {
                    session.viewport.write().scroll_top = event.data().scroll_top();
                },
                onresize: move |event| {
                    if let Ok(size) = event.data().get_content_box_size() {
                        session.viewport.write().height = size.height;
                    }
                },
                div { class: "history-content", style: "height: {content_height}px;",
                    if offset == 1 && range.start == 0 {
                        div {
                            class: if selection == Some(Selection::Uncommitted) { "history-row uncommitted selected" } else { "history-row uncommitted" },
                            style: "top: 0px;",
                            onclick: move |_| session.select(Selection::Uncommitted),
                            div { class: "graph",
                                svg { width: "{graph_width}", height: "{ROW_HEIGHT}",
                                    circle {
                                        class: "node-open",
                                        cx: "{HALF_LANE}",
                                        cy: "{HALF_ROW}",
                                        r: "{NODE_RADIUS}",
                                    }
                                }
                            }
                            span { class: "summary", "Uncommitted changes" }
                            span { class: "author", "{files_label}" }
                            span { class: "date" }
                            span { class: "hash" }
                        }
                    }
                    for index in commit_rows {
                        {
                            let row = &history.rows()[index];
                            let id = row.commit.id;
                            let top = (index + offset) as f64 * ROW_HEIGHT;
                            let selected = selection == Some(Selection::Commit(id));
                            let labels = ref_index.labels(&id).to_vec();
                            let date = short_date(row.commit.author.time);
                            let short = id.short();
                            rsx! {
                                div {
                                    key: "{id}",
                                    class: if selected { "history-row selected" } else { "history-row" },
                                    style: "top: {top}px;",
                                    title: "Right-click to create a branch here",
                                    onclick: move |_| session.select(Selection::Commit(id)),
                                    oncontextmenu: move |event| {
                                        session.select(Selection::Commit(id));
                                        operations.open_menu(&event, commit_menu(id));
                                    },
                                    div { class: "graph", {graph(&row.graph, graph_width)} }
                                    span { class: "summary",
                                        for label in labels {
                                            {badge(&label)}
                                        }
                                        "{row.commit.summary}"
                                    }
                                    span { class: "author", title: "{row.commit.author.email}", "{row.commit.author.name}" }
                                    span { class: "date", "{date}" }
                                    span { class: "hash", "{short}" }
                                }
                            }
                        }
                    }
                    if loading {
                        div { class: "history-loading", style: "top: {content_height}px;", "Loading…" }
                    }
                }
            }
        }
    }
}

fn badge(label: &RefLabel) -> Element {
    let class = match label.kind {
        RefLabelKind::DetachedHead => "badge head",
        RefLabelKind::Local { is_head: true } => "badge local head",
        RefLabelKind::Local { is_head: false } => "badge local",
        RefLabelKind::Remote => "badge remote",
        RefLabelKind::Tag => "badge tag",
    };
    rsx! { span { class, title: "{label.name}", "{label.name}" } }
}

fn lane_x(lane: usize) -> f64 {
    lane as f64 * LANE_WIDTH + LANE_WIDTH / 2.0
}

/// The SVG path of one edge: straight within a lane, an S-curve between lanes.
fn edge_path(from: usize, to: usize, half: EdgeHalf) -> String {
    let (y1, y2) = match half {
        EdgeHalf::Top => (0.0, ROW_HEIGHT / 2.0),
        EdgeHalf::Bottom => (ROW_HEIGHT / 2.0, ROW_HEIGHT),
    };
    let (x1, x2) = (lane_x(from), lane_x(to));
    if from == to {
        format!("M{x1} {y1}L{x2} {y2}")
    } else {
        let ym = (y1 + y2) / 2.0;
        format!("M{x1} {y1}C{x1} {ym} {x2} {ym} {x2} {y2}")
    }
}

fn graph(row: &GraphRow, width: f64) -> Element {
    let edges: Vec<(String, String)> = row
        .edges
        .iter()
        .map(|e| {
            (
                format!("edge c{}", e.color % COLORS),
                edge_path(e.from, e.to, e.half),
            )
        })
        .collect();
    let node_class = format!("node c{}", row.node_color % COLORS);
    let cx = lane_x(row.node);
    let cy = ROW_HEIGHT / 2.0;
    rsx! {
        svg { width: "{width}", height: "{ROW_HEIGHT}",
            for (class, d) in edges {
                path { class, d }
            }
            circle {
                class: node_class,
                cx: "{cx}",
                cy: "{cy}",
                r: "{NODE_RADIUS}",
            }
        }
    }
}
