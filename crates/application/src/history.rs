use crate::graph::{GraphLayout, GraphRow};
use crate::ports::{HistoryCursor, HistoryScope, ReadError, RepositoryReader};
use crate::search::CommitQuery;
use domain::{Commit, RepositoryInfo};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub commit: Commit,
    pub graph: GraphRow,
}

/// Starts paged history walks.
#[derive(Clone)]
pub struct LoadHistory {
    reader: Arc<dyn RepositoryReader>,
}

impl LoadHistory {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self { reader }
    }

    pub fn open(
        &self,
        repo: &RepositoryInfo,
        scope: &HistoryScope,
    ) -> Result<HistoryPager, ReadError> {
        Ok(HistoryPager {
            cursor: self.reader.history(repo, scope)?,
            layout: GraphLayout::new(),
            query: None,
            exhausted: false,
        })
    }

    /// Like [`open`](Self::open), but hands out only the commits that match
    /// `query`. Their rows have no graph edges, as the commits in between
    /// are left out.
    pub fn search(
        &self,
        repo: &RepositoryInfo,
        scope: &HistoryScope,
        query: CommitQuery,
    ) -> Result<HistoryPager, ReadError> {
        let mut pager = self.open(repo, scope)?;
        pager.query = Some(query);
        Ok(pager)
    }
}

/// The most commits a search looks at per page, so a page with few matches
/// in a large history still comes back quickly. The rest is searched by the
/// following pages.
pub const SEARCH_BATCH: usize = 20_000;

/// Hands out history rows page by page and lays out the graph as it goes.
pub struct HistoryPager {
    cursor: Box<dyn HistoryCursor>,
    layout: GraphLayout,
    query: Option<CommitQuery>,
    exhausted: bool,
}

impl HistoryPager {
    /// Returns up to `max` further rows. Without a search, fewer than `max`
    /// means the end. A search returns what it found among the next
    /// [`SEARCH_BATCH`] commits, possibly nothing, so check
    /// [`is_exhausted`](Self::is_exhausted). After the end, calls return
    /// nothing.
    pub fn next_page(&mut self, max: usize) -> Result<Vec<HistoryRow>, ReadError> {
        if self.exhausted {
            return Ok(Vec::new());
        }
        if let Some(query) = &self.query {
            return self.search_page(&query.clone(), max);
        }
        let commits = self.cursor.next_page(max)?;
        if commits.len() < max {
            self.exhausted = true;
        }
        Ok(commits
            .into_iter()
            .map(|commit| {
                let graph = self.layout.push(commit.id, &commit.parents);
                HistoryRow { commit, graph }
            })
            .collect())
    }

    fn search_page(
        &mut self,
        query: &CommitQuery,
        max: usize,
    ) -> Result<Vec<HistoryRow>, ReadError> {
        let mut rows = Vec::new();
        let mut looked_at = 0;
        while rows.len() < max && looked_at < SEARCH_BATCH {
            let batch = (max - rows.len()).min(SEARCH_BATCH - looked_at);
            let commits = self.cursor.next_page(batch)?;
            looked_at += commits.len();
            if commits.len() < batch {
                self.exhausted = true;
            }
            rows.extend(
                commits
                    .into_iter()
                    .filter(|commit| query.matches(commit))
                    .map(|commit| HistoryRow {
                        commit,
                        graph: GraphRow {
                            node: 0,
                            node_color: 0,
                            width: 1,
                            edges: Vec::new(),
                        },
                    }),
            );
            if self.exhausted {
                break;
            }
        }
        Ok(rows)
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}
