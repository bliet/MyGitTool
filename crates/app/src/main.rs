//! Composition root: parses the command line, builds the adapters, injects
//! them into the use cases and launches the UI.

use anyhow::Context;
use application::{
    ComputeFileDiff, GetCommitDetails, LoadBranches, LoadHistory, LoadWorkTreeChanges,
    OpenRepository, RecentRepositories, UserSettings,
};
use infra_config::{JsonRecentRepositoriesStore, JsonSettingsStore};
use infra_git::{GitRepositoryLocator, GitRepositoryReader};
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
    let reader = Arc::new(GitRepositoryReader);

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
            compute_file_diff: ComputeFileDiff::new(reader),
            user_settings,
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
