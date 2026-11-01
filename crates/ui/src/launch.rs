use crate::components::App;
use crate::state::AppState;
use crate::view::settings::theme_script;
use application::{
    AbortOperation, ApplyPatch, ApplyStash, Checkout, ComputeFileDiff, ContinueOperation,
    CreateBranch, CreateCommit, CreatePatch, CreateTag, DeleteBranch, DeleteTag, DropStash, Fetch,
    GetCommitDetails, LoadBranches, LoadHistory, LoadOngoing, LoadWorkTreeChanges,
    MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, MergeIntoCurrent, OpenRepository, Pull, Push, PushTag,
    RebaseCurrentOnto, RecentRepositories, RememberLayout, SaveStash, Settings, StageChanges,
    Startup, Theme, UserSettings, WatchRepository, WindowLayout,
};
use dioxus::desktop::icon_from_memory;
use dioxus::desktop::tao::window::{Icon, Theme as NativeTheme};
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use std::sync::LazyLock;

/// The stylesheet: the light colours and all rules, then the dark colours
/// for the dark theme and for the system theme on a dark system.
static STYLE: LazyLock<String> = LazyLock::new(|| {
    let base = include_str!("../assets/main.css");
    let dark = include_str!("../assets/dark.css");
    format!(
        "{base}
:root[data-theme=\"light\"] {{ color-scheme: light; }}
:root[data-theme=\"dark\"] {{ color-scheme: dark; {dark} }}
@media (prefers-color-scheme: dark) {{
  :root:not([data-theme=\"light\"]) {{ {dark} }}
}}"
    )
});

/// The app icon of every window, or `None` if it cannot be decoded (which a
/// test rules out for the embedded file).
pub fn window_icon() -> Option<Icon> {
    icon_from_memory(include_bytes!("../assets/icon.png")).ok()
}

/// The `<head>` additions of every window: the stylesheet, and the theme
/// applied before the first paint.
pub(crate) fn custom_head(theme: Theme) -> String {
    format!(
        "<style>{}</style><script>{}</script>",
        *STYLE,
        theme_script(theme)
    )
}

/// The theme of the window decorations; `None` follows the system.
pub(crate) fn native_theme(theme: Theme) -> Option<NativeTheme> {
    match theme {
        Theme::System => None,
        Theme::Light => Some(NativeTheme::Light),
        Theme::Dark => Some(NativeTheme::Dark),
    }
}

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
    pub remember_layout: RememberLayout,
    pub watch_repository: WatchRepository,
    pub fetch: Fetch,
    pub pull: Pull,
    pub push: Push,
    pub abort_operation: AbortOperation,
    pub checkout: Checkout,
    pub create_branch: CreateBranch,
    pub delete_branch: DeleteBranch,
    pub create_tag: CreateTag,
    pub delete_tag: DeleteTag,
    pub push_tag: PushTag,
    pub merge_into_current: MergeIntoCurrent,
    pub rebase_current_onto: RebaseCurrentOnto,
    pub continue_operation: ContinueOperation,
    pub load_ongoing: LoadOngoing,
    pub stage_changes: StageChanges,
    pub create_commit: CreateCommit,
    pub save_stash: SaveStash,
    pub apply_stash: ApplyStash,
    pub drop_stash: DropStash,
    pub create_patch: CreatePatch,
    pub apply_patch: ApplyPatch,
}

/// What the app starts with: the settings and the window layout, loaded
/// before the window opens so it appears with the right size and theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Initial {
    pub settings: Settings,
    pub layout: WindowLayout,
}

/// Opens the main window and runs the UI until it is closed.
pub fn launch(services: Services, startup: Startup) {
    let mut state = AppState::from_startup(startup);
    let settings = services.user_settings.load().unwrap_or_else(|err| {
        state.error = Some(format!("Settings: {err}"));
        Settings::default()
    });
    let layout = services.remember_layout.load().unwrap_or_else(|err| {
        state.error = Some(format!("Window layout: {err}"));
        WindowLayout::default()
    });
    let window = WindowBuilder::new()
        .with_title(state.window_title())
        .with_inner_size(LogicalSize::new(layout.width, layout.height))
        .with_min_inner_size(LogicalSize::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT))
        .with_maximized(layout.maximized)
        .with_theme(native_theme(settings.theme))
        .with_window_icon(window_icon());
    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_custom_head(custom_head(settings.theme));

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(services)
        .with_context(state)
        .with_context(Initial { settings, layout })
        .launch(App);
}
