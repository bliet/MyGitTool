use crate::credentials::{AllowedCredentials, Attempt, CredentialChain};
use application::{CredentialProvider, ProgressReporter, RemoteError, RemoteOperations};
use domain::{Progress, RepositoryInfo};
use git2::{
    Cred, CredentialType, Direction, ErrorClass, ErrorCode, FetchOptions, Progress as Stats,
    PushOptions, Remote, RemoteCallbacks, Repository,
};
use std::cell::{Cell, RefCell};

/// Fetches and pushes with git2, authenticating through a
/// [`CredentialChain`] per operation.
#[derive(Debug, Default, Clone, Copy)]
pub struct GitRemoteOperations;

/// Why the credential callback gave up.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AuthFailure {
    Cancelled,
    Exhausted(String),
}

impl RemoteOperations for GitRemoteOperations {
    fn fetch(
        &self,
        repo: &RepositoryInfo,
        remote: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), RemoteError> {
        let git = open(repo)?;
        let mut handle = find_remote(&git, remote)?;
        let failure = Cell::new(None);
        let chain = RefCell::new(CredentialChain::for_current_user());
        let mut callbacks = callbacks(&git, &chain, &failure, progress, credentials);
        callbacks.transfer_progress(|stats| {
            progress.report(transfer(&stats));
            !progress.is_cancelled()
        });
        let mut options = FetchOptions::new();
        options.remote_callbacks(callbacks);
        let no_refspecs: &[&str] = &[];
        handle
            .fetch(no_refspecs, Some(&mut options), None)
            .map_err(|err| remote_error(err, failure.take(), progress))
    }

    fn push(
        &self,
        repo: &RepositoryInfo,
        branch: &str,
        remote: &str,
        remote_ref: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), RemoteError> {
        // No leading `+`: the remote only accepts a fast-forward.
        let refspec = format!("refs/heads/{branch}:{remote_ref}");
        push_refspec(repo, remote, &refspec, remote_ref, progress, credentials)
    }

    fn delete_branch(
        &self,
        repo: &RepositoryInfo,
        remote: &str,
        remote_ref: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), RemoteError> {
        push_refspec(
            repo,
            remote,
            &format!(":{remote_ref}"),
            remote_ref,
            progress,
            credentials,
        )?;
        // Pushing a deletion usually prunes the remote-tracking branch, but
        // not with every transport; make sure it is gone.
        let git = open(repo)?;
        let handle = find_remote(&git, remote)?;
        for spec in handle.refspecs() {
            if spec.direction() != Direction::Fetch || !spec.src_matches(remote_ref) {
                continue;
            }
            let Ok(local) = spec.transform(remote_ref) else {
                continue;
            };
            if let Ok(name) = local.as_str()
                && let Ok(mut reference) = git.find_reference(name)
            {
                reference
                    .delete()
                    .map_err(|err| RemoteError::Backend(err.message().to_owned()))?;
            }
        }
        Ok(())
    }
}

/// Pushes one refspec and turns a rejection into [`RemoteError::Rejected`].
fn push_refspec(
    repo: &RepositoryInfo,
    remote: &str,
    refspec: &str,
    remote_ref: &str,
    progress: &dyn ProgressReporter,
    credentials: &dyn CredentialProvider,
) -> Result<(), RemoteError> {
    {
        let git = open(repo)?;
        let mut handle = find_remote(&git, remote)?;
        let failure = Cell::new(None);
        let rejected = RefCell::new(None);
        let chain = RefCell::new(CredentialChain::for_current_user());
        let mut callbacks = callbacks(&git, &chain, &failure, progress, credentials);
        callbacks.push_transfer_progress(|objects, total, bytes| {
            progress.report(Progress::Sending {
                objects,
                total,
                bytes,
            });
        });
        callbacks.push_update_reference(|reference, status| {
            if let Some(message) = status {
                *rejected.borrow_mut() = Some(RemoteError::Rejected {
                    reference: reference.to_owned(),
                    message: message.to_owned(),
                });
            }
            Ok(())
        });
        let mut options = PushOptions::new();
        options.remote_callbacks(callbacks);
        match handle.push(&[refspec], Some(&mut options)) {
            Ok(()) => rejected.borrow_mut().take().map_or(Ok(()), Err),
            Err(err) if err.code() == ErrorCode::NotFastForward => Err(RemoteError::Rejected {
                reference: remote_ref.to_owned(),
                message: "the remote branch has commits that are not here; pull first".to_owned(),
            }),
            Err(err) => Err(remote_error(err, failure.take(), progress)),
        }
    }
}

fn open(repo: &RepositoryInfo) -> Result<Repository, RemoteError> {
    Repository::open(repo.root()).map_err(|err| RemoteError::Backend(err.message().to_owned()))
}

fn find_remote<'r>(git: &'r Repository, name: &str) -> Result<Remote<'r>, RemoteError> {
    git.find_remote(name).map_err(|err| match err.code() {
        ErrorCode::NotFound | ErrorCode::InvalidSpec => RemoteError::UnknownRemote(name.to_owned()),
        _ => RemoteError::Backend(err.message().to_owned()),
    })
}

/// The callbacks both operations share: credentials and server messages.
fn callbacks<'a>(
    git: &'a Repository,
    chain: &'a RefCell<CredentialChain>,
    failure: &'a Cell<Option<AuthFailure>>,
    progress: &'a dyn ProgressReporter,
    provider: &'a dyn CredentialProvider,
) -> RemoteCallbacks<'a> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |url, username, allowed| {
        let allowed = AllowedCredentials {
            ssh_key: allowed.contains(CredentialType::SSH_KEY),
            user_pass: allowed.contains(CredentialType::USER_PASS_PLAINTEXT),
            username: allowed.contains(CredentialType::USERNAME),
        };
        loop {
            let attempt = chain.borrow_mut().next(url, username, allowed, provider);
            match attempt {
                Attempt::SshAgent { username } => return Cred::ssh_key_from_agent(&username),
                Attempt::SshKey {
                    username,
                    key,
                    passphrase,
                } => return Cred::ssh_key(&username, None, &key, passphrase.as_deref()),
                Attempt::CredentialHelper => {
                    let helper = git
                        .config()
                        .and_then(|config| Cred::credential_helper(&config, url, username));
                    if let Ok(cred) = helper {
                        return Ok(cred);
                    }
                }
                Attempt::UserPass { username, password } => {
                    return Cred::userpass_plaintext(&username, &password);
                }
                Attempt::Username(username) => return Cred::username(&username),
                Attempt::Cancelled => {
                    failure.set(Some(AuthFailure::Cancelled));
                    return Err(git2::Error::from_str("cancelled"));
                }
                Attempt::Exhausted => {
                    failure.set(Some(AuthFailure::Exhausted(url.to_owned())));
                    return Err(git2::Error::from_str("no more credentials to try"));
                }
            }
        }
    });
    callbacks.sideband_progress(move |data| {
        let text = String::from_utf8_lossy(data);
        let line = text
            .split(['\r', '\n'])
            .map(str::trim)
            .rfind(|line| !line.is_empty());
        if let Some(line) = line {
            progress.report(Progress::Message(line.to_owned()));
        }
        !progress.is_cancelled()
    });
    callbacks
}

fn transfer(stats: &Stats<'_>) -> Progress {
    if stats.received_objects() < stats.total_objects() {
        Progress::Receiving {
            objects: stats.received_objects(),
            total: stats.total_objects(),
            bytes: stats.received_bytes(),
        }
    } else {
        Progress::Resolving {
            deltas: stats.indexed_deltas(),
            total: stats.total_deltas(),
        }
    }
}

fn remote_error(
    err: git2::Error,
    failure: Option<AuthFailure>,
    progress: &dyn ProgressReporter,
) -> RemoteError {
    match failure {
        Some(AuthFailure::Cancelled) => return RemoteError::Cancelled,
        Some(AuthFailure::Exhausted(url)) => return RemoteError::AuthenticationFailed(url),
        None => {}
    }
    if progress.is_cancelled() || err.code() == ErrorCode::User {
        return RemoteError::Cancelled;
    }
    if err.code() == ErrorCode::Auth
        || (err.class() == ErrorClass::Ssh && err.message().contains("authenticat"))
    {
        return RemoteError::AuthenticationFailed(err.message().to_owned());
    }
    RemoteError::Backend(err.message().to_owned())
}
