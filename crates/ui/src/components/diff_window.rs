//! The diff window: a separate desktop window per changed file.

use super::blocking;
use crate::launch::{custom_head, native_theme, window_icon};
use crate::view::diff::{
    DIFF_ROW_HEIGHT, DiffKey, Direction, Jump, Step, at_end_hint, content_width, format_size, jump,
    rows_on_screen, scroll_top_for, segments, side_class, side_labels, status, window_title,
};
use crate::view::history::visible_rows;
use crate::view::settings::theme_script;
use application::{ComputeFileDiff, MAX_DIFF_FILE_SIZE, Theme};
use dioxus::desktop::{Config, LogicalSize, WeakDesktopContext, WindowBuilder};
use dioxus::prelude::*;
use domain::{DiffBody, DiffLine, DiffSource, FileChange, FileDiff, RepositoryInfo};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const OLD_PANE_ID: &str = "diff-old";

/// Keys of the diff window, and scroll syncing: scrolling either pane
/// scrolls the other one to the same position. Both panes have the same
/// size, so the positions always match.
const WINDOW_SCRIPT: &str = r#"
    window.__myGitToolDiff?.abort();
    const controller = new AbortController();
    window.__myGitToolDiff = controller;
    const signal = controller.signal;
    window.addEventListener("keydown", (event) => {
        if (["F5", "F6", "F7", "Escape"].includes(event.key)) {
            event.preventDefault();
            dioxus.send(event.key);
        }
    }, { signal });
    document.addEventListener("scroll", (event) => {
        const source = event.target;
        if (!(source instanceof Element) || !source.classList.contains("diff-pane")) return;
        for (const pane of document.querySelectorAll(".diff-pane")) {
            if (pane === source) continue;
            if (pane.scrollTop !== source.scrollTop) pane.scrollTop = source.scrollTop;
            if (pane.scrollLeft !== source.scrollLeft) pane.scrollLeft = source.scrollLeft;
        }
    }, { capture: true, passive: true, signal });
    await new Promise(() => {});
"#;

/// The open diff windows of the app, and the theme they use. A `None` entry
/// is a window that is still being created.
#[derive(Clone, Default)]
pub struct DiffWindows {
    windows: Rc<RefCell<HashMap<DiffKey, Option<WeakDesktopContext>>>>,
    theme: Rc<Cell<Theme>>,
}

impl DiffWindows {
    /// Applies `theme` to the open windows and the ones opened later.
    pub fn set_theme(&self, theme: Theme) {
        if self.theme.replace(theme) == theme {
            return;
        }
        let script = theme_script(theme);
        for window in self.windows.borrow().values().flatten() {
            if let Some(window) = window.upgrade() {
                // A window that cannot run the script keeps its old theme.
                let _ = window.webview.evaluate_script(&script);
                window.window.set_theme(native_theme(theme));
            }
        }
    }

    /// Opens the diff of `change`, or focuses its window if it is open.
    pub fn open(
        &self,
        compute: ComputeFileDiff,
        repo: RepositoryInfo,
        source: DiffSource,
        change: FileChange,
    ) {
        let key = DiffKey {
            repo: repo.root().to_path_buf(),
            source,
            path: change.path.clone(),
        };
        match self.windows.borrow().get(&key) {
            Some(None) => return,
            Some(Some(weak)) => {
                if let Some(window) = weak.upgrade() {
                    window.window.set_focus();
                    return;
                }
            }
            None => {}
        }
        self.windows.borrow_mut().insert(key.clone(), None);

        let title = window_title(&change.path, source);
        let dom = VirtualDom::new_with_props(
            DiffWindow,
            DiffWindowProps {
                repo,
                source,
                change,
            },
        )
        .with_root_context(compute);
        let builder = WindowBuilder::new()
            .with_title(title)
            .with_inner_size(LogicalSize::new(1300.0, 850.0))
            .with_min_inner_size(LogicalSize::new(500.0, 300.0))
            .with_theme(native_theme(self.theme.get()))
            .with_window_icon(window_icon());
        let config = Config::new()
            .with_window(builder)
            .with_menu(None)
            .with_custom_head(custom_head(self.theme.get()));
        let pending = dioxus::desktop::window().new_window(dom, config);
        let windows = self.windows.clone();
        spawn(async move {
            match pending.try_resolve().await {
                Ok(window) => {
                    windows
                        .borrow_mut()
                        .insert(key, Some(Rc::downgrade(&window)));
                }
                Err(_) => {
                    windows.borrow_mut().remove(&key);
                }
            }
        });
    }

    /// Closes every diff window, e.g. when the main window closes.
    pub fn close_all(&self) {
        for weak in self.windows.borrow_mut().drain().filter_map(|(_, w)| w) {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum DiffState {
    Loading,
    Ready(Loaded),
    Error(String),
}

/// A computed diff. Copies share it and compare by identity, so passing it
/// as a prop never compares whole diffs.
#[derive(Debug, Clone)]
struct Loaded(Rc<LoadedDiff>);

#[derive(Debug)]
struct LoadedDiff {
    diff: FileDiff,
    /// The widest line in character cells.
    width: usize,
}

impl PartialEq for Loaded {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Navigation {
    current: Option<usize>,
    last: Option<(Step, Direction)>,
    /// Set after a press found no further change in this direction; the
    /// next press in the same direction wraps around.
    at_end: Option<Direction>,
}

#[component]
fn DiffWindow(repo: RepositoryInfo, source: DiffSource, change: FileChange) -> Element {
    let compute = use_context::<ComputeFileDiff>();
    let mut state = use_signal(|| DiffState::Loading);
    let mut scroll_top = use_signal(|| 0.0);
    let mut height = use_signal(|| 600.0);
    let mut navigation = use_signal(|| Navigation {
        current: None,
        last: None,
        at_end: None,
    });
    // The scroll position to apply once the rows are rendered.
    let mut pending_scroll = use_signal(|| None::<f64>);

    let load = use_callback(move |first: bool| {
        let compute = compute.clone();
        let repo = repo.clone();
        let change = change.clone();
        spawn(async move {
            let result = blocking(move || compute.compute(&repo, source, &change)).await;
            let loaded = match result {
                Ok(Ok(diff)) => diff,
                Ok(Err(err)) => return state.set(DiffState::Error(err.to_string())),
                Err(message) => return state.set(DiffState::Error(message)),
            };
            let width = match &loaded.body {
                DiffBody::Text(text) => content_width(text),
                _ => 0,
            };
            navigation.write().at_end = None;
            // Start at the first change.
            if first
                && let DiffBody::Text(text) = &loaded.body
                && let Some(hunk) = text.hunks.first()
            {
                navigation.set(Navigation {
                    current: Some(0),
                    last: None,
                    at_end: None,
                });
                pending_scroll.set(Some(scroll_top_for(hunk)));
            }
            state.set(DiffState::Ready(Loaded(Rc::new(LoadedDiff {
                diff: loaded,
                width,
            }))));
        });
    });

    let mut navigate = move |direction: Direction| {
        let DiffState::Ready(loaded) = &*state.peek() else {
            return;
        };
        let DiffBody::Text(text) = &loaded.0.diff.body else {
            return;
        };
        let on_screen = rows_on_screen(*scroll_top.peek(), *height.peek(), text.rows.len());
        let Navigation {
            current, at_end, ..
        } = *navigation.peek();
        match jump(&text.hunks, current, on_screen, direction, at_end) {
            Some(Jump::To(next)) => {
                navigation.set(Navigation {
                    current: Some(next.index),
                    last: Some((next, direction)),
                    at_end: None,
                });
                pending_scroll.set(Some(scroll_top_for(&text.hunks[next.index])));
            }
            Some(Jump::AtEnd(direction)) => navigation.write().at_end = Some(direction),
            None => {}
        }
    };

    use_hook(move || {
        load.call(true);
        spawn(async move {
            let mut keys = document::eval(WINDOW_SCRIPT);
            while let Ok(key) = keys.recv::<String>().await {
                match key.as_str() {
                    "F5" => load.call(false),
                    "F6" => navigate(Direction::Previous),
                    "F7" => navigate(Direction::Next),
                    "Escape" => dioxus::desktop::window().close(),
                    _ => {}
                }
            }
        });
    });

    // Scrolls once the rows are in the DOM; the sync script moves the other pane.
    use_effect(move || {
        let ready = matches!(*state.read(), DiffState::Ready(_));
        let pending = *pending_scroll.read();
        if let (true, Some(top)) = (ready, pending) {
            pending_scroll.set(None);
            document::eval(&format!(
                "requestAnimationFrame(() => {{ const el = document.getElementById('{OLD_PANE_ID}'); if (el) el.scrollTop = {top}; }});"
            ));
        }
    });

    let (old_label, new_label) = side_labels(source);
    let state = state.read().clone();
    let nav = *navigation.read();
    let (old_path, new_path) = match &state {
        DiffState::Ready(loaded) => (
            loaded.0.diff.old_path.clone(),
            loaded.0.diff.new_path.clone(),
        ),
        _ => (None, None),
    };
    let old_path = old_path.unwrap_or_else(|| "(none)".to_owned());
    let new_path = new_path.unwrap_or_else(|| "(none)".to_owned());
    let status_text = match &state {
        DiffState::Ready(loaded) => match &loaded.0.diff.body {
            DiffBody::Text(text) => match nav.last {
                Some((step, direction)) => status(text.hunks.len(), Some(step), direction),
                None => status(text.hunks.len(), None, Direction::Next),
            },
            _ => String::new(),
        },
        DiffState::Loading => "Loading…".to_owned(),
        DiffState::Error(_) => String::new(),
    };

    rsx! {
        div { class: "diff-window",
            header { class: "diff-header",
                div { class: "diff-side-title",
                    span { class: "revision", "{old_label}" }
                    span { class: "path selectable", title: "{old_path}", "{old_path}" }
                }
                div { class: "diff-side-title",
                    span { class: "revision", "{new_label}" }
                    span { class: "path selectable", title: "{new_path}", "{new_path}" }
                }
            }
            match state {
                DiffState::Loading => rsx! { p { class: "placeholder", "Loading…" } },
                DiffState::Error(message) => rsx! { p { class: "column-error", "{message}" } },
                DiffState::Ready(loaded) => match &loaded.0.diff.body {
                    DiffBody::Binary => rsx! { p { class: "diff-notice", "Binary file changed" } },
                    DiffBody::TooLarge { size } => {
                        let size = format_size(*size);
                        let limit = format_size(MAX_DIFF_FILE_SIZE);
                        rsx! { p { class: "diff-notice", "File too large to diff ({size}; the limit is {limit})" } }
                    }
                    DiffBody::Text(text) if text.rows.is_empty() => rsx! {
                        p { class: "diff-notice", "The file is empty." }
                    },
                    DiffBody::Text(_) => rsx! {
                        DiffPanes {
                            diff: loaded.clone(),
                            current: nav.current,
                            scroll_top: scroll_top(),
                            height: height(),
                            on_scroll: move |top| scroll_top.set(top),
                            on_resize: move |h| height.set(h),
                        }
                    },
                },
            }
            if let Some(direction) = nav.at_end {
                div { class: "diff-hint", role: "status", {at_end_hint(direction)} }
            }
            footer { class: "status-bar",
                span { "{status_text}" }
                div { class: "spacer" }
                span { class: "muted", "F7 next change · F6 previous · F5 reload · Esc close" }
            }
        }
    }
}

#[component]
fn DiffPanes(
    diff: Loaded,
    current: Option<usize>,
    scroll_top: f64,
    height: f64,
    on_scroll: EventHandler<f64>,
    on_resize: EventHandler<f64>,
) -> Element {
    let LoadedDiff { diff, width } = &*diff.0;
    let DiffBody::Text(text) = &diff.body else {
        return rsx! {};
    };
    let range = visible_rows(DIFF_ROW_HEIGHT, scroll_top, height, text.rows.len());
    let current = current
        .and_then(|i| text.hunks.get(i))
        .map(|h| h.rows.clone());
    let content_height = text.rows.len() as f64 * DIFF_ROW_HEIGHT;
    let style = format!("height: {content_height}px; --text-width: {width}ch;");

    rsx! {
        div { class: "diff-panes",
            for old in [true, false] {
                div {
                    key: "{old}",
                    id: if old { OLD_PANE_ID } else { "diff-new" },
                    class: "diff-pane",
                    onscroll: move |event| on_scroll.call(event.data().scroll_top()),
                    onresize: move |event| {
                        if let Ok(size) = event.data().get_content_box_size() {
                            on_resize.call(size.height);
                        }
                    },
                    div { class: "diff-content", style: "{style}",
                        for index in range.clone() {
                            {
                                let row = &text.rows[index];
                                let line = if old { &row.old } else { &row.new };
                                let marked = current.as_ref().is_some_and(|r| r.contains(&index));
                                let class = format!(
                                    "diff-row {}{}",
                                    side_class(row.kind, old),
                                    if marked { " current" } else { "" }
                                );
                                let top = index as f64 * DIFF_ROW_HEIGHT;
                                rsx! {
                                    div { key: "{index}", class, style: "top: {top}px;",
                                        {line_cells(line.as_ref())}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn line_cells(line: Option<&DiffLine>) -> Element {
    let Some(line) = line else {
        return rsx! {
            span { class: "ln" }
            span { class: "code" }
        };
    };
    let parts: Vec<(String, bool)> = segments(&line.text, &line.highlights)
        .into_iter()
        .map(|(text, marked)| (text.to_owned(), marked))
        .collect();
    rsx! {
        span { class: "ln", "{line.number}" }
        span { class: "code selectable",
            for (text, marked) in parts {
                if marked {
                    span { class: "emph", "{text}" }
                } else {
                    "{text}"
                }
            }
        }
    }
}
