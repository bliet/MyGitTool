//! Fetch, pull and push in the background: the running operation with its
//! progress, the dialogs they need, and the branch context menu.

use super::blocking;
use super::session::Session;
use crate::launch::Services;
use crate::state::AppState;
use crate::view::operation::{
    MenuEntry, RemoteAction, conflict_title, pull_summary, running_label,
};
use application::{
    CredentialProvider, ProgressReporter, PullOutcome, PushDestination, PushPlan, SyncError,
};
use dioxus::dioxus_core::{Runtime, ScopeId, current_scope_id};
use dioxus::prelude::*;
use domain::{Credential, CredentialRequest, OngoingOperation, Progress};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// How long a success message stays in the status strip.
const NOTICE_DURATION: Duration = Duration::from_secs(6);

/// The operation in progress.
#[derive(Debug, Clone)]
pub struct Running {
    pub label: String,
    pub progress: Option<Progress>,
    cancel: Arc<AtomicBool>,
}

/// Answers a credential prompt; the worker thread waits for it.
pub type Reply = Rc<RefCell<Option<oneshot::Sender<Option<Credential>>>>>;

/// A modal dialog over the main window.
#[derive(Debug, Clone)]
pub enum Dialog {
    Credentials {
        request: CredentialRequest,
        reply: Reply,
    },
    /// The branch has no upstream; push it to one of `remotes` and make
    /// that the upstream.
    SetUpstream {
        branch: String,
        remotes: Vec<String>,
    },
    /// The work tree has uncommitted changes in `files`.
    DirtyWorkTree { title: String, files: Vec<String> },
    Conflicts {
        operation: OngoingOperation,
        files: Vec<String>,
    },
}

/// A context menu at window coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub entries: Vec<MenuEntry>,
}

#[derive(Clone, Copy)]
pub struct Operations {
    /// The repository view's scope. Tasks run there, so they outlive the
    /// dialog or menu that started them.
    scope: ScopeId,
    services: Signal<Services>,
    session: Session,
    app: Signal<AppState>,
    pub running: Signal<Option<Running>>,
    pub notice: Signal<Option<String>>,
    pub dialog: Signal<Option<Dialog>>,
    pub menu: Signal<Option<Menu>>,
}

impl Operations {
    pub fn new(services: Services, session: Session, app: Signal<AppState>) -> Self {
        Self {
            scope: current_scope_id(),
            services: Signal::new(services),
            session,
            app,
            running: Signal::new(None),
            notice: Signal::new(None),
            dialog: Signal::new(None),
            menu: Signal::new(None),
        }
    }

    fn spawn(&self, task: impl Future<Output = ()> + 'static) {
        Runtime::current().spawn(self.scope, task);
    }

    pub fn is_running(&self) -> bool {
        self.running.read().is_some()
    }

    /// Starts `action` unless another operation is running.
    pub fn start(mut self, action: RemoteAction) {
        self.menu.set(None);
        if self.running.peek().is_some() {
            return;
        }
        let services = self.services.peek().clone();
        let repo = self.session.repo.peek().clone();
        let label = running_label(&action);
        match action {
            RemoteAction::Fetch(target) => self.spawn(async move {
                let fetch = services.fetch;
                let result = self
                    .remote(label, move |progress, credentials| {
                        fetch.fetch(&repo, &target, progress, credentials)
                    })
                    .await;
                self.finish("Fetch", result, |remotes| {
                    format!("Fetched {}", remotes.join(", "))
                });
            }),
            RemoteAction::Pull => self.spawn(async move {
                let pull = services.pull;
                let result = self
                    .remote(label, move |progress, credentials| {
                        pull.pull(&repo, progress, credentials)
                    })
                    .await;
                if let Some(Ok(PullOutcome::Conflicts { operation, files })) = &result {
                    self.dialog.set(Some(Dialog::Conflicts {
                        operation: *operation,
                        files: files.clone(),
                    }));
                    self.session.refresh();
                    return;
                }
                self.finish("Pull", result, |outcome| pull_summary(&outcome));
            }),
            RemoteAction::Push { branch } => self.spawn(async move {
                let push = services.push.clone();
                let plan = {
                    let branch = branch.clone();
                    blocking(move || push.plan(&repo, &branch)).await
                };
                match plan {
                    Ok(Ok(PushPlan::Upstream(destination))) => {
                        self.push(branch, destination).await;
                    }
                    Ok(Ok(PushPlan::NoUpstream { branch, remotes })) => {
                        self.dialog
                            .set(Some(Dialog::SetUpstream { branch, remotes }));
                    }
                    Ok(Err(err)) => self.fail("Push", &err),
                    Err(message) => self.app.write().error = Some(message),
                }
            }),
        };
    }

    /// Pushes `branch` to `remote` and makes it the upstream.
    pub fn push_with_new_upstream(mut self, branch: String, remote: String) {
        self.dialog.set(None);
        let destination = application::Push::new_upstream(&branch, &remote);
        self.spawn(async move { self.push(branch, destination).await });
    }

    async fn push(self, branch: String, destination: PushDestination) {
        let push = self.services.peek().push.clone();
        let repo = self.session.repo.peek().clone();
        let label = running_label(&RemoteAction::Push {
            branch: branch.clone(),
        });
        let remote = destination.remote.clone();
        let result = self
            .remote(label, move |progress, credentials| {
                push.push(&repo, &branch, &destination, progress, credentials)
                    .map(|()| branch)
            })
            .await;
        self.finish("Push", result, |branch| {
            format!("Pushed {branch} to {remote}")
        });
    }

    /// Aborts the merge or rebase a pull left behind.
    pub fn abort(mut self) {
        self.dialog.set(None);
        let abort = self.services.peek().abort_operation.clone();
        let repo = self.session.repo.peek().clone();
        self.spawn(async move {
            let result = blocking(move || abort.abort(&repo)).await;
            self.finish("Abort", result.ok(), |()| "Aborted the pull".to_owned());
        });
    }

    /// Leaves the conflicts for the user to resolve outside the app.
    pub fn leave_conflicts(mut self) {
        self.dialog.set(None);
        self.show_notice("Conflicts left for resolution outside the app".to_owned());
    }

    /// Asks the running operation to stop, answering an open prompt with
    /// "no".
    pub fn cancel(self) {
        if let Some(running) = &*self.running.peek() {
            running.cancel.store(true, Ordering::Relaxed);
        }
        if self.is_prompting() {
            self.answer(None);
        }
    }

    /// Answers the credential prompt; `None` cancels.
    pub fn answer(mut self, credential: Option<Credential>) {
        if let Some(Dialog::Credentials { reply, .. }) = &*self.dialog.peek()
            && let Some(sender) = reply.borrow_mut().take()
        {
            // The worker may have stopped waiting already; that is fine.
            let _ = sender.send(credential);
        }
        self.dialog.set(None);
    }

    fn is_prompting(&self) -> bool {
        matches!(&*self.dialog.peek(), Some(Dialog::Credentials { .. }))
    }

    /// Closes a dialog that only informs, or cancels one that asks.
    pub fn dismiss(self) {
        self.answer(None);
    }

    /// Runs `task` on a worker thread, showing its progress and answering
    /// its credential prompts with a dialog. `None` if the worker panicked.
    async fn remote<T, F>(mut self, label: String, task: F) -> Option<T>
    where
        T: Send + 'static,
        F: FnOnce(&dyn ProgressReporter, &dyn CredentialProvider) -> T + Send + 'static,
    {
        let cancel = Arc::new(AtomicBool::new(false));
        self.notice.set(None);
        self.running.set(Some(Running {
            label,
            progress: None,
            cancel: cancel.clone(),
        }));
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
        let (prompt_tx, mut prompt_rx) = mpsc::unbounded_channel();
        let bridge = Bridge {
            progress: progress_tx,
            prompts: prompt_tx,
            cancel,
        };
        let mut worker = tokio::task::spawn_blocking(move || task(&bridge, &bridge));
        let result = loop {
            tokio::select! {
                result = &mut worker => break result,
                Some(progress) = progress_rx.recv() => {
                    // Only the latest state matters; skip what piled up.
                    let mut latest = progress;
                    while let Ok(next) = progress_rx.try_recv() {
                        latest = next;
                    }
                    if let Some(running) = self.running.write().as_mut() {
                        running.progress = Some(latest);
                    }
                }
                Some((request, reply)) = prompt_rx.recv() => {
                    self.dialog.set(Some(Dialog::Credentials {
                        request,
                        reply: Rc::new(RefCell::new(Some(reply))),
                    }));
                }
            }
        };
        self.running.set(None);
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.app.write().error = Some(format!("Internal error: {err}"));
                None
            }
        }
    }

    /// Shows the outcome and reloads the repository view.
    fn finish<T>(
        mut self,
        what: &str,
        result: Option<Result<T, SyncError>>,
        summary: impl FnOnce(T) -> String,
    ) {
        match result {
            Some(Ok(value)) => self.show_notice(summary(value)),
            Some(Err(SyncError::DirtyWorkTree(dirty))) => {
                self.dialog.set(Some(Dialog::DirtyWorkTree {
                    title: format!("{what} needs a clean work tree"),
                    files: dirty.files,
                }));
            }
            Some(Err(SyncError::Remote(application::RemoteError::Cancelled))) => {
                self.show_notice(format!("{what} cancelled"));
            }
            Some(Err(err)) => self.fail(what, &err),
            None => {}
        }
        self.session.refresh();
    }

    fn fail(mut self, what: &str, err: &SyncError) {
        self.app.write().error = Some(format!("{what} failed: {err}"));
    }

    fn show_notice(mut self, text: String) {
        self.notice.set(Some(text.clone()));
        let mut notice = self.notice;
        self.spawn(async move {
            tokio::time::sleep(NOTICE_DURATION).await;
            if notice.peek().as_deref() == Some(text.as_str()) {
                notice.set(None);
            }
        });
    }
}

/// Connects the worker thread to the UI.
struct Bridge {
    progress: mpsc::UnboundedSender<Progress>,
    prompts: mpsc::UnboundedSender<(CredentialRequest, oneshot::Sender<Option<Credential>>)>,
    cancel: Arc<AtomicBool>,
}

impl ProgressReporter for Bridge {
    fn report(&self, progress: Progress) {
        let _ = self.progress.send(progress);
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl CredentialProvider for Bridge {
    fn credentials(&self, request: &CredentialRequest) -> Option<Credential> {
        if self.is_cancelled() {
            return None;
        }
        let (reply, answer) = oneshot::channel();
        self.prompts.send((request.clone(), reply)).ok()?;
        // Runs on a worker thread, so blocking until the user answers is fine.
        answer.blocking_recv().ok().flatten()
    }
}

/// The title of a dialog.
pub fn dialog_title(dialog: &Dialog) -> String {
    match dialog {
        Dialog::Credentials {
            request: CredentialRequest::UserPass { .. },
            ..
        } => "Sign in".to_owned(),
        Dialog::Credentials {
            request: CredentialRequest::SshPassphrase { .. },
            ..
        } => "SSH key passphrase".to_owned(),
        Dialog::SetUpstream { branch, .. } => format!("{branch} has no upstream"),
        Dialog::DirtyWorkTree { title, .. } => title.clone(),
        Dialog::Conflicts { operation, .. } => conflict_title(*operation),
    }
}
