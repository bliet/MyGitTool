//! Composition root: parses the command line, builds the adapters, injects
//! them into the use cases and launches the UI.

// Release builds on Windows are GUI apps without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Context;
use application::{
    AbortOperation, Checkout, ComputeFileDiff, ContinueOperation, CreateBranch, CreateTag,
    DeleteBranch, DeleteTag, Fetch, GetCommitDetails, LoadBranches, LoadHistory, LoadOngoing,
    LoadWorkTreeChanges, MergeIntoCurrent, OpenRepository, Pull, Push, PushTag, RebaseCurrentOnto,
    RecentRepositories, RememberLayout, UserSettings, WatchRepository,
};
use infra_config::{JsonLayoutStore, JsonRecentRepositoriesStore, JsonSettingsStore};
use infra_git::{
    GitRemoteOperations, GitRepoWatcher, GitRepositoryLocator, GitRepositoryReader,
    GitRepositoryWriter,
};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "Usage: mygittool [PATH]

Opens the Git repository at PATH. Without PATH, opens the current directory
if it is inside a Git repository, and otherwise shows a folder picker.";

fn main() -> anyhow::Result<ExitCode> {
    let requested = match parse_args(std::env::args_os().skip(1)) {
        Args::Help => {
            println!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
        Args::Version => {
            println!("mygittool {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        Args::Invalid(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return Ok(ExitCode::from(2));
        }
        Args::Open(path) => path,
    };

    let config_dir = infra_config::config_dir().context("cannot locate the config directory")?;
    let open_repository = OpenRepository::new(Arc::new(GitRepositoryLocator));
    let recent_repositories = RecentRepositories::new(Arc::new(
        JsonRecentRepositoriesStore::in_config_dir(&config_dir),
    ));

    let user_settings = UserSettings::new(Arc::new(JsonSettingsStore::in_config_dir(&config_dir)));
    let remember_layout =
        RememberLayout::new(Arc::new(JsonLayoutStore::in_config_dir(&config_dir)));
    let reader = Arc::new(GitRepositoryReader);
    let writer = Arc::new(GitRepositoryWriter);
    let remote = Arc::new(GitRemoteOperations);

    let current_dir = std::env::current_dir().ok();
    let startup = open_repository.resolve_startup(requested.as_deref(), current_dir.as_deref());

    ui::launch(
        ui::Services {
            open_repository,
            recent_repositories,
            load_branches: LoadBranches::new(reader.clone()),
            load_history: LoadHistory::new(reader.clone()),
            get_commit_details: GetCommitDetails::new(reader.clone()),
            load_work_tree_changes: LoadWorkTreeChanges::new(reader.clone()),
            compute_file_diff: ComputeFileDiff::new(reader.clone()),
            user_settings,
            remember_layout,
            watch_repository: WatchRepository::new(Arc::new(GitRepoWatcher::default())),
            fetch: Fetch::new(reader.clone(), remote.clone()),
            pull: Pull::new(reader.clone(), writer.clone(), remote.clone()),
            push: Push::new(reader.clone(), writer.clone(), remote.clone()),
            abort_operation: AbortOperation::new(writer.clone()),
            checkout: Checkout::new(reader.clone(), writer.clone()),
            create_branch: CreateBranch::new(reader.clone(), writer.clone()),
            delete_branch: DeleteBranch::new(reader.clone(), writer.clone(), remote.clone()),
            create_tag: CreateTag::new(reader.clone(), writer.clone()),
            delete_tag: DeleteTag::new(reader.clone(), writer.clone(), remote.clone()),
            push_tag: PushTag::new(reader.clone(), remote),
            merge_into_current: MergeIntoCurrent::new(reader.clone(), writer.clone()),
            rebase_current_onto: RebaseCurrentOnto::new(reader.clone(), writer.clone()),
            continue_operation: ContinueOperation::new(reader.clone(), writer),
            load_ongoing: LoadOngoing::new(reader.clone()),
        },
        startup,
    );
    Ok(ExitCode::SUCCESS)
}

enum Args {
    Open(Option<PathBuf>),
    Help,
    Version,
    Invalid(String),
}

fn parse_args(mut args: impl Iterator<Item = OsString>) -> Args {
    let Some(first) = args.next() else {
        return Args::Open(None);
    };
    if let Some(extra) = args.next() {
        return Args::Invalid(format!("unexpected argument: {}", extra.to_string_lossy()));
    }
    match first.to_str() {
        Some("-h" | "--help") => Args::Help,
        Some("-V" | "--version") => Args::Version,
        Some(flag) if flag.starts_with('-') && flag != "-" => {
            Args::Invalid(format!("unknown option: {flag}"))
        }
        _ => Args::Open(Some(PathBuf::from(first))),
    }
}
