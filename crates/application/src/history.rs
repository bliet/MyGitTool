use crate::graph::{GraphLayout, GraphRow};
use crate::ports::{HistoryCursor, HistoryScope, ReadError, RepositoryReader};
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
            exhausted: false,
        })
    }
}

/// Hands out history rows page by page and lays out the graph as it goes.
pub struct HistoryPager {
    cursor: Box<dyn HistoryCursor>,
    layout: GraphLayout,
    exhausted: bool,
}

impl HistoryPager {
    /// Returns up to `max` further rows. Once fewer than `max` come back,
    /// [`is_exhausted`](Self::is_exhausted) is true and later calls return nothing.
    pub fn next_page(&mut self, max: usize) -> Result<Vec<HistoryRow>, ReadError> {
        if self.exhausted {
            return Ok(Vec::new());
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

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}
