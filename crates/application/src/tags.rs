use crate::ports::{
    CredentialProvider, ProgressReporter, RemoteOperations, RepositoryReader, RepositoryWriter,
};
use crate::sync::OperationError;
use domain::{CommitId, RepositoryInfo, validate_tag_name};
use std::sync::Arc;

fn tag_ref(name: &str) -> String {
    format!("refs/tags/{name}")
}

/// Creates tags, annotated or lightweight.
#[derive(Clone)]
pub struct CreateTag {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl CreateTag {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Checks a proposed name before anything is created.
    pub fn check_name(&self, repo: &RepositoryInfo, name: &str) -> Result<(), OperationError> {
        validate_tag_name(name).map_err(OperationError::InvalidTagName)?;
        if self.reader.resolve(repo, &tag_ref(name))?.is_some() {
            return Err(OperationError::TagExists(name.to_owned()));
        }
        Ok(())
    }

    /// Creates `name` at `target`. A message that is not blank makes it an
    /// annotated tag; otherwise it is lightweight.
    pub fn create(
        &self,
        repo: &RepositoryInfo,
        name: &str,
        target: CommitId,
        message: &str,
    ) -> Result<(), OperationError> {
        self.check_name(repo, name)?;
        let message = message.trim();
        let message = (!message.is_empty()).then(|| format!("{message}\n"));
        Ok(self
            .writer
            .create_tag(repo, name, target, message.as_deref())?)
    }
}

/// Deletes tags, here or on a remote.
#[derive(Clone)]
pub struct DeleteTag {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
    remote: Arc<dyn RemoteOperations>,
}

impl DeleteTag {
    pub fn new(
        reader: Arc<dyn RepositoryReader>,
        writer: Arc<dyn RepositoryWriter>,
        remote: Arc<dyn RemoteOperations>,
    ) -> Self {
        Self {
            reader,
            writer,
            remote,
        }
    }

    /// Deletes the local tag `name`. Remotes keep their copy.
    pub fn delete_local(&self, repo: &RepositoryInfo, name: &str) -> Result<(), OperationError> {
        if self.reader.resolve(repo, &tag_ref(name))?.is_none() {
            return Err(OperationError::TagNotFound(name.to_owned()));
        }
        Ok(self.writer.delete_tag(repo, name)?)
    }

    /// Deletes the tag `name` on `remote`. The local tag stays.
    pub fn delete_remote(
        &self,
        repo: &RepositoryInfo,
        remote: &str,
        name: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), OperationError> {
        Ok(self
            .remote
            .delete_branch(repo, remote, &tag_ref(name), progress, credentials)?)
    }
}

/// Pushes a tag to a remote.
#[derive(Clone)]
pub struct PushTag {
    reader: Arc<dyn RepositoryReader>,
    remote: Arc<dyn RemoteOperations>,
}

impl PushTag {
    pub fn new(reader: Arc<dyn RepositoryReader>, remote: Arc<dyn RemoteOperations>) -> Self {
        Self { reader, remote }
    }

    pub fn push(
        &self,
        repo: &RepositoryInfo,
        name: &str,
        remote: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), OperationError> {
        if self.reader.resolve(repo, &tag_ref(name))?.is_none() {
            return Err(OperationError::TagNotFound(name.to_owned()));
        }
        Ok(self
            .remote
            .push_tag(repo, name, remote, progress, credentials)?)
    }
}
