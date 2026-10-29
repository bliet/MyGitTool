use crate::ports::{ReadError, RepositoryReader};
use domain::{CommitDetails, CommitId, RepositoryInfo};
use std::sync::Arc;

#[derive(Clone)]
pub struct GetCommitDetails {
    reader: Arc<dyn RepositoryReader>,
}

impl GetCommitDetails {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self { reader }
    }

    pub fn get(&self, repo: &RepositoryInfo, id: CommitId) -> Result<CommitDetails, ReadError> {
        self.reader.commit_details(repo, id)
    }
}
