//! Branch and remote operations in the background: the running operation
//! with its progress, the dialogs they need, and the context menu.

use super::blocking;
use super::session::Session;
use crate::launch::Services;
use crate::state::AppState;
use crate::view::operation::{
    Action, MenuEntry, conflict_title, dirty_texts, integration_summary, running_label,
};
use application::{
    CredentialProvider, IntegrationOutcome, OperationError, ProgressReporter, PushDestination,
    PushPlan, RemoteError,
};
use dioxus::dioxus_core::{Runtime, ScopeId, current_scope_id};
use dioxus::prelude::*;
use domain::{CommitId, Credential, CredentialRequest, OngoingOperation, Progress};
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
    DirtyWorkTree {
        title: String,
        intro: String,
        files: Vec<String>,
    },
    /// `what` (e.g. "Merge") stopped with conflicts.
    Conflicts {
        what: String,
        operation: OngoingOperation,
        files: Vec<String>,
    },
    /// Asks for the name of a new branch at `start`.
    CreateBranch {
        start: CommitId,
        from: String,
        /// Why the last attempt failed, e.g. the name exists.
        error: Option<String>,
    },
    /// The branch has commits the current branch lacks; delete anyway?
    ConfirmDeleteUnmerged {
        branch: String,
        commits: usize,
    },
    ConfirmDeleteRemote {
        remote: String,
        branch: String,
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

    /// Opens a context menu at the mouse position of `event`.
    pub fn open_menu(mut self, event: &MouseEvent, entries: Vec<MenuEntry>) {
        let point = event.client_coordinates();
        self.menu.set(Some(Menu {
            x: point.x,
            y: point.y,
            entries,
        }));
    }

    pub fn is_running(&self) -> bool {
        self.running.read().is_some()
    }

    /// Starts `action` unless another operation is running. Actions that
    /// need more input open a dialog first.
    pub fn start(mut self, action: Action) {
        self.menu.set(None);
        // Opening a page is no repository operation, so it never waits.
        let action = match action {
            Action::OpenInBrowser(web) => {
                if let Err(err) = open::that_detached(&web.url) {
                    self.app.write().error = Some(format!("Cannot open {}: {err}", web.url));
                }
                return;
            }
            action => action,
        };
        if self.running.peek().is_some() {
            return;
        }
        let services = self.services.peek().clone();
        let repo = self.session.repo.peek().clone();
        let label = running_label(&action);
        match action {
            // Handled above.
            Action::OpenInBrowser(_) => {}
            Action::Fetch(target) => self.spawn(async move {
                let fetch = services.fetch;
                let result = self
                    .run(label, move |progress, credentials| {
                        fetch.fetch(&repo, &target, progress, credentials)
                    })
                    .await;
                self.finish("Fetch", result, |remotes| {
                    format!("Fetched {}", remotes.join(", "))
                });
            }),
            Action::Pull => self.spawn(async move {
                let pull = services.pull;
                let result = self
                    .run(label, move |progress, credentials| {
                        pull.pull(&repo, progress, credentials)
                    })
                    .await;
                self.integrated("Pull", result);
            }),
            Action::Push { branch } => self.spawn(async move {
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
            Action::Checkout(target) => self.spawn(async move {
                let checkout = services.checkout;
                let result = self
                    .run(label, move |_, _| checkout.checkout(&repo, &target))
                    .await;
                self.finish("Checkout", result, |branch| format!("Switched to {branch}"));
            }),
            Action::CreateBranch { start, from } => self.dialog.set(Some(Dialog::CreateBranch {
                start,
                from,
                error: None,
            })),
            Action::DeleteLocal { branch } => self.delete_local(branch, false),
            Action::DeleteRemote { remote, branch } => {
                self.dialog
                    .set(Some(Dialog::ConfirmDeleteRemote { remote, branch }));
            }
            Action::Merge { refname, .. } => self.spawn(async move {
                let merge = services.merge_into_current;
                let result = self
                    .run(label, move |_, _| merge.merge(&repo, &refname))
                    .await;
                self.integrated("Merge", result);
            }),
            Action::Rebase { refname, .. } => self.spawn(async move {
                let rebase = services.rebase_current_onto;
                let result = self
                    .run(label, move |_, _| rebase.rebase(&repo, &refname))
                    .await;
                self.integrated("Rebase", result);
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
        let label = running_label(&Action::Push {
            branch: branch.clone(),
        });
        let remote = destination.remote.clone();
        let result = self
            .run(label, move |progress, credentials| {
                push.push(&repo, &branch, &destination, progress, credentials)
                    .map(|()| branch)
            })
            .await;
        self.finish("Push", result, |branch| {
            format!("Pushed {branch} to {remote}")
        });
    }

    /// Creates the branch from the create dialog. A bad or taken name keeps
    /// the dialog open with the reason.
    pub fn create_branch(mut self, name: String, start: CommitId, checkout: bool) {
        let create = self.services.peek().create_branch.clone();
        let repo = self.session.repo.peek().clone();
        self.spawn(async move {
            let attempt = {
                let name = name.clone();
                self.run("Creating the branch…".to_owned(), move |_, _| {
                    create.create(&repo, &name, start, checkout)
                })
                .await
            };
            if let Some(Err(
                err @ (OperationError::BranchExists(_) | OperationError::InvalidBranchName(_)),
            )) = &attempt
            {
                if let Some(Dialog::CreateBranch { error, .. }) = self.dialog.write().as_mut() {
                    *error = Some(err.to_string());
                }
                return;
            }
            self.dialog.set(None);
            self.finish("Create branch", attempt, |()| {
                if checkout {
                    format!("Created and switched to {name}")
                } else {
                    format!("Created {name}")
                }
            });
        });
    }

    /// Deletes a local branch. Without `force`, an unmerged branch asks for
    /// confirmation first.
    pub fn delete_local(mut self, branch: String, force: bool) {
        self.dialog.set(None);
        let delete = self.services.peek().delete_branch.clone();
        let repo = self.session.repo.peek().clone();
        let label = running_label(&Action::DeleteLocal {
            branch: branch.clone(),
        });
        self.spawn(async move {
            let result = {
                let branch = branch.clone();
                self.run(label, move |_, _| {
                    delete.delete_local(&repo, &branch, force)
                })
                .await
            };
            if let Some(Err(OperationError::Unmerged { branch, commits })) = result {
                self.dialog
                    .set(Some(Dialog::ConfirmDeleteUnmerged { branch, commits }));
                return;
            }
            self.finish("Delete", result, |()| format!("Deleted {branch}"));
        });
    }

    /// Deletes a branch on the server, after the confirm dialog.
    pub fn delete_remote(mut self, remote: String, branch: String) {
        self.dialog.set(None);
        let delete = self.services.peek().delete_branch.clone();
        let repo = self.session.repo.peek().clone();
        let label = running_label(&Action::DeleteRemote {
            remote: remote.clone(),
            branch: branch.clone(),
        });
        self.spawn(async move {
            let summary = format!("Deleted {branch} from {remote}");
            let result = self
                .run(label, move |progress, credentials| {
                    delete.delete_remote(&repo, &remote, &branch, progress, credentials)
                })
                .await;
            self.finish("Delete", result, |()| summary);
        });
    }

    /// Continues the merge or rebase in progress.
    pub fn resume(mut self) {
        self.dialog.set(None);
        let resume = self.services.peek().continue_operation.clone();
        let repo = self.session.repo.peek().clone();
        self.spawn(async move {
            let result = self
                .run("Continuing…".to_owned(), move |_, _| resume.resume(&repo))
                .await;
            self.integrated("Continue", result);
        });
    }

    /// Aborts the merge or rebase in progress.
    pub fn abort(mut self) {
        self.dialog.set(None);
        let abort = self.services.peek().abort_operation.clone();
        let repo = self.session.repo.peek().clone();
        self.spawn(async move {
            let result = self
                .run("Aborting…".to_owned(), move |_, _| abort.abort(&repo))
                .await;
            self.finish("Abort", result, |()| "Aborted".to_owned());
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

    /// Runs `task` on a worker thread, showing it in the status strip with
    /// its progress, and answering its credential prompts with a dialog.
    /// `None` if the worker panicked.
    async fn run<T, F>(mut self, label: String, task: F) -> Option<T>
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

    /// Shows the outcome of a pull, merge, rebase or continue: the conflict
    /// dialog if it stopped, otherwise a notice.
    fn integrated(
        mut self,
        what: &str,
        result: Option<Result<IntegrationOutcome, OperationError>>,
    ) {
        if let Some(Ok(IntegrationOutcome::Conflicts { operation, files })) = &result {
            self.dialog.set(Some(Dialog::Conflicts {
                what: what.to_owned(),
                operation: *operation,
                files: files.clone(),
            }));
            self.session.refresh();
            return;
        }
        self.finish(what, result, |outcome| integration_summary(&outcome));
    }

    /// Shows the outcome and reloads the repository view.
    fn finish<T>(
        mut self,
        what: &str,
        result: Option<Result<T, OperationError>>,
        summary: impl FnOnce(T) -> String,
    ) {
        match result {
            Some(Ok(value)) => self.show_notice(summary(value)),
            Some(Err(OperationError::DirtyWorkTree(dirty))) => {
                let (title, intro) = dirty_texts(what);
                self.dialog.set(Some(Dialog::DirtyWorkTree {
                    title,
                    intro,
                    files: dirty.files,
                }));
            }
            Some(Err(OperationError::Remote(RemoteError::Cancelled))) => {
                self.show_notice(format!("{what} cancelled"));
            }
            Some(Err(err)) => self.fail(what, &err),
            None => {}
        }
        self.session.refresh();
    }

    fn fail(mut self, what: &str, err: &OperationError) {
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
        Dialog::Conflicts {
            what, operation, ..
        } => conflict_title(what, *operation),
        Dialog::CreateBranch { from, .. } => format!("New branch from {from}"),
        Dialog::ConfirmDeleteUnmerged { branch, .. } => format!("Delete {branch}?"),
        Dialog::ConfirmDeleteRemote { remote, branch } => {
            format!("Delete {branch} from {remote}?")
        }
    }
}
