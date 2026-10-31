//! State and actions of an open repository, shared by the three columns.

use super::blocking;
use crate::launch::Services;
use crate::view::branch_tree::{BranchItem, TAGS_KEY};
use crate::view::history::{HistoryList, PAGE_SIZE, ROW_HEIGHT, Selection, scroll_to_reveal};
use crate::view::refresh::Reload;
use application::{
    CommitQuery, HistoryPager, HistoryScope, OngoingState, RefIndex, RepoChanges, RepositoryRefs,
};
use dioxus::prelude::*;
use domain::{CommitDetails, CommitId, FileChange, RepositoryInfo};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

pub const HISTORY_SCROLL_ID: &str = "history-scroll";

/// Serializes page loads, so pages are appended in order. It is held across
/// the whole fetch-and-append of a page.
type SharedPager = Arc<Mutex<Option<HistoryPager>>>;

#[derive(Debug, Clone, PartialEq)]
pub enum DetailsState {
    Empty,
    Loading,
    Commit(Box<CommitDetails>),
    Uncommitted,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub scroll_top: f64,
    pub height: f64,
}

#[derive(Clone, Copy)]
pub struct Session {
    services: Signal<Services>,
    pager: Signal<SharedPager>,
    pub repo: Signal<RepositoryInfo>,
    pub refs: Signal<Option<RepositoryRefs>>,
    pub ref_index: Signal<RefIndex>,
    pub refs_error: Signal<Option<String>>,
    pub history: Signal<HistoryList>,
    pub history_error: Signal<Option<String>>,
    pub history_loading: Signal<bool>,
    /// The branch the history is filtered to; `None` shows all refs.
    pub filter: Signal<Option<BranchItem>>,
    /// The text in the search field, as typed.
    pub search_input: Signal<String>,
    /// The search the history shows, applied shortly after typing stops.
    pub search: Signal<String>,
    pub work_tree: Signal<Vec<FileChange>>,
    /// A merge, rebase or similar in progress, e.g. after conflicts.
    pub ongoing: Signal<Option<OngoingState>>,
    pub selection: Signal<Option<Selection>>,
    pub selected_branch: Signal<Option<String>>,
    pub details: Signal<DetailsState>,
    pub collapsed: Signal<HashSet<String>>,
    /// Whether the body of the commit message is hidden in the details
    /// column. It applies to every commit, so it sticks while browsing.
    pub message_collapsed: Signal<bool>,
    /// Collapsed directories of the changed-files tree, by full path. They
    /// stay collapsed in other commits that change the same directory.
    pub collapsed_dirs: Signal<HashSet<String>>,
    pub viewport: Signal<Viewport>,
}

impl Session {
    pub fn new(services: Services, repo: RepositoryInfo) -> Self {
        Self {
            services: Signal::new(services),
            pager: Signal::new(Arc::new(Mutex::new(None))),
            repo: Signal::new(repo),
            refs: Signal::new(None),
            ref_index: Signal::new(RefIndex::default()),
            refs_error: Signal::new(None),
            history: Signal::new(HistoryList::default()),
            history_error: Signal::new(None),
            history_loading: Signal::new(false),
            filter: Signal::new(None),
            search_input: Signal::new(String::new()),
            search: Signal::new(String::new()),
            work_tree: Signal::new(Vec::new()),
            ongoing: Signal::new(None),
            selection: Signal::new(None),
            selected_branch: Signal::new(None),
            details: Signal::new(DetailsState::Empty),
            collapsed: Signal::new(HashSet::from([TAGS_KEY.to_owned()])),
            message_collapsed: Signal::new(false),
            collapsed_dirs: Signal::new(HashSet::new()),
            viewport: Signal::new(Viewport {
                scroll_top: 0.0,
                height: 800.0,
            }),
        }
    }

    /// Whether the "Uncommitted changes" row is shown above the commits.
    /// A search hides it.
    pub fn has_uncommitted_row(&self) -> bool {
        !self.work_tree.read().is_empty() && self.search.read().trim().is_empty()
    }

    /// Reloads everything: refs, work tree, history and the details.
    pub fn refresh(self) {
        spawn(async move {
            self.load_refs().await;
            self.load_work_tree().await;
            self.load_ongoing().await;
            self.reset_history().await;
            if let Some(selection) = *self.selection.peek() {
                self.load_details(selection).await;
            }
        });
    }

    /// Reloads what `changes` (made outside the app) can have affected. A
    /// large history is only walked again if a ref moved.
    pub fn refresh_after(self, changes: RepoChanges) {
        let reload = Reload::after(changes);
        spawn(async move {
            let refs_changed = reload.refs && self.load_refs().await;
            if reload.work_tree {
                self.load_work_tree().await;
            }
            if reload.ongoing {
                self.load_ongoing().await;
            }
            if refs_changed {
                self.reset_history().await;
            }
        });
    }

    /// Returns whether the refs differ from the ones shown before.
    async fn load_refs(mut self) -> bool {
        let load = self.services.peek().load_branches.clone();
        let repo = self.repo.peek().clone();
        match blocking(move || load.load(&repo)).await {
            Ok(Ok(refs)) => {
                if self.refs_error.peek().is_some() {
                    self.refs_error.set(None);
                }
                if self.refs.peek().as_ref() == Some(&refs) {
                    return false;
                }
                self.ref_index.set(RefIndex::new(&refs));
                self.refs.set(Some(refs));
                true
            }
            Ok(Err(err)) => {
                self.refs_error.set(Some(err.to_string()));
                false
            }
            Err(message) => {
                self.refs_error.set(Some(message));
                false
            }
        }
    }

    async fn load_work_tree(mut self) {
        let load = self.services.peek().load_work_tree_changes.clone();
        let repo = self.repo.peek().clone();
        // The history still works without the uncommitted row.
        let changes = match blocking(move || load.load(&repo)).await {
            Ok(Ok(changes)) => changes,
            Ok(Err(_)) | Err(_) => Vec::new(),
        };
        if *self.work_tree.peek() != changes {
            self.work_tree.set(changes);
        }
        if *self.selection.peek() == Some(Selection::Uncommitted)
            && self.work_tree.peek().is_empty()
        {
            self.selection.set(None);
            self.details.set(DetailsState::Empty);
        }
    }

    async fn load_ongoing(mut self) {
        let load = self.services.peek().load_ongoing.clone();
        let repo = self.repo.peek().clone();
        // The banner is only a hint; the operations check again.
        let state = match blocking(move || load.load(&repo)).await {
            Ok(Ok(state)) => state,
            Ok(Err(_)) | Err(_) => None,
        };
        if *self.ongoing.peek() != state {
            self.ongoing.set(state);
        }
    }

    /// Starts a new history walk for the current filter and loads its first page.
    async fn reset_history(mut self) {
        let pager = self.pager.peek().clone();
        let mut guard = pager.lock_owned().await;
        let load = self.services.peek().load_history.clone();
        let repo = self.repo.peek().clone();
        let scope = match &*self.filter.peek() {
            Some(branch) => HistoryScope::Ref(branch.refname.clone()),
            None => HistoryScope::All,
        };
        let query = CommitQuery::new(&self.search.peek());
        let opened = blocking(move || match query {
            Some(query) => load.search(&repo, &scope, query),
            None => load.open(&repo, &scope),
        });
        match opened.await {
            Ok(Ok(new_pager)) => {
                *guard = Some(new_pager);
                self.history.set(HistoryList::default());
                self.history_error.set(None);
            }
            Ok(Err(err)) => self.fail_history(&mut guard, err.to_string()),
            Err(message) => self.fail_history(&mut guard, message),
        }
        drop(guard);
        self.fetch_page().await;
    }

    fn fail_history(&mut self, pager: &mut Option<HistoryPager>, message: String) {
        *pager = None;
        let mut empty = HistoryList::default();
        empty.append(Vec::new(), true);
        self.history.set(empty);
        self.history_error.set(Some(message));
    }

    /// Loads the next page unless one is already on its way.
    pub fn load_more(mut self) {
        if *self.history_loading.peek() || self.history.peek().is_exhausted() {
            return;
        }
        self.history_loading.set(true);
        spawn(async move {
            self.fetch_page().await;
            self.history_loading.set(false);
        });
    }

    /// Fetches and appends one page. Returns `false` once nothing is left.
    async fn fetch_page(mut self) -> bool {
        let pager = self.pager.peek().clone();
        let guard = pager.lock_owned().await;
        let result = blocking(move || {
            let mut guard = guard;
            let page = guard
                .as_mut()
                .map(|p| p.next_page(PAGE_SIZE).map(|rows| (rows, p.is_exhausted())));
            (guard, page)
        })
        .await;
        match result {
            Ok((_guard, Some(Ok((rows, exhausted))))) => {
                // A search page can be empty with more history left.
                let more = !rows.is_empty() || !exhausted;
                self.history.write().append(rows, exhausted);
                more
            }
            Ok((_guard, Some(Err(err)))) => {
                self.history_error.set(Some(err.to_string()));
                self.history.write().append(Vec::new(), true);
                false
            }
            Ok((_guard, None)) => false,
            Err(message) => {
                self.history_error.set(Some(message));
                false
            }
        }
    }

    pub fn select(mut self, selection: Selection) {
        if *self.selection.peek() == Some(selection) {
            return;
        }
        self.selection.set(Some(selection));
        spawn(async move { self.load_details(selection).await });
    }

    async fn load_details(mut self, selection: Selection) {
        let id = match selection {
            Selection::Uncommitted => {
                self.details.set(DetailsState::Uncommitted);
                return;
            }
            Selection::Commit(id) => id,
        };
        self.details.set(DetailsState::Loading);
        let get = self.services.peek().get_commit_details.clone();
        let repo = self.repo.peek().clone();
        let result = blocking(move || get.get(&repo, id)).await;
        // Ignore the result if the user moved on in the meantime.
        if *self.selection.peek() != Some(selection) {
            return;
        }
        self.details.set(match result {
            Ok(Ok(details)) => DetailsState::Commit(Box::new(details)),
            Ok(Err(err)) => DetailsState::Error(err.to_string()),
            Err(message) => DetailsState::Error(message),
        });
    }

    /// Selects a commit and scrolls the history to it, loading pages until
    /// it is found.
    pub fn select_and_reveal(self, id: CommitId) {
        self.select(Selection::Commit(id));
        spawn(async move {
            loop {
                let position = self.history.peek().position(&id);
                if let Some(position) = position {
                    let offset = usize::from(self.has_uncommitted_row());
                    self.scroll_to_row(position + offset);
                    return;
                }
                if self.history.peek().is_exhausted() || !self.fetch_page().await {
                    return;
                }
            }
        });
    }

    pub fn select_branch(mut self, branch: &BranchItem) {
        self.selected_branch.set(Some(branch.refname.clone()));
        self.select_and_reveal(branch.target);
    }

    /// Filters the history to `branch`, or shows all refs for `None`.
    pub fn set_filter(mut self, branch: Option<BranchItem>) {
        if *self.filter.peek() == branch {
            return;
        }
        self.filter.set(branch);
        self.scroll_to_top();
        spawn(async move { self.reset_history().await });
    }

    /// Shows only the commits matching `text`; an empty text ends the
    /// search.
    pub fn set_search(mut self, text: String) {
        if *self.search.peek() == text {
            return;
        }
        self.search.set(text);
        self.scroll_to_top();
        spawn(async move { self.reset_history().await });
    }

    /// Empties the search field and ends the search.
    pub fn clear_search(mut self) {
        self.search_input.set(String::new());
        self.set_search(String::new());
    }

    fn scroll_to_row(self, row: usize) {
        let viewport = *self.viewport.peek();
        if let Some(top) = scroll_to_reveal(row, viewport.scroll_top, viewport.height) {
            set_scroll_top(top);
        }
    }

    fn scroll_to_top(mut self) {
        self.viewport.write().scroll_top = 0.0;
        set_scroll_top(0.0);
    }
}

fn set_scroll_top(top: f64) {
    // The top is a finite pixel value computed from row indexes.
    let top = (top / ROW_HEIGHT).round() * ROW_HEIGHT;
    document::eval(&format!(
        "const el = document.getElementById('{HISTORY_SCROLL_ID}'); if (el) el.scrollTop = {top};"
    ));
}
