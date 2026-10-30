use crate::components::App;
use crate::state::AppState;
use application::{
    AbortOperation, Checkout, ComputeFileDiff, ContinueOperation, CreateBranch, DeleteBranch,
    Fetch, GetCommitDetails, LoadBranches, LoadHistory, LoadOngoing, LoadWorkTreeChanges,
    MergeIntoCurrent, OpenRepository, Pull, Push, RebaseCurrentOnto, RecentRepositories, Startup,
    UserSettings,
};
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

pub(crate) const STYLE: &str = include_str!("../assets/main.css");

/// The use cases the UI needs, built by the composition root.
#[derive(Clone)]
pub struct Services {
    pub open_repository: OpenRepository,
    pub recent_repositories: RecentRepositories,
    pub load_branches: LoadBranches,
    pub load_history: LoadHistory,
    pub get_commit_details: GetCommitDetails,
    pub load_work_tree_changes: LoadWorkTreeChanges,
    pub compute_file_diff: ComputeFileDiff,
    pub user_settings: UserSettings,
    pub fetch: Fetch,
    pub pull: Pull,
    pub push: Push,
    pub abort_operation: AbortOperation,
    pub checkout: Checkout,
    pub create_branch: CreateBranch,
    pub delete_branch: DeleteBranch,
    pub merge_into_current: MergeIntoCurrent,
    pub rebase_current_onto: RebaseCurrentOnto,
    pub continue_operation: ContinueOperation,
    pub load_ongoing: LoadOngoing,
}

/// Opens the main window and runs the UI until it is closed.
pub fn launch(services: Services, startup: Startup) {
    let state = AppState::from_startup(startup);
    let window = WindowBuilder::new()
        .with_title(state.window_title())
        .with_inner_size(LogicalSize::new(1400.0, 900.0))
        .with_min_inner_size(LogicalSize::new(800.0, 500.0));
    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_custom_head(format!("<style>{STYLE}</style>"));

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(services)
        .with_context(state)
        .launch(App);
}
