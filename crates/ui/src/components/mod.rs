mod branches;
mod details;
mod diff_window;
mod error_banner;
mod history;
mod operation_views;
mod operations;
mod repository_view;
mod session;
mod settings_dialog;
mod welcome;
mod window_layout;

use crate::launch::{Initial, Services, native_theme};
use crate::state::{AppState, Screen};
use crate::view::settings::theme_script;
use application::{Settings, StorageError};
use diff_window::DiffWindows;
use dioxus::prelude::*;
use error_banner::ErrorBanner;
use repository_view::RepositoryView;
use settings_dialog::{SettingsDialog, SettingsOpen};
use std::path::PathBuf;
use welcome::Welcome;
use window_layout::use_layout_keeper;

#[component]
pub fn App() -> Element {
    let services = use_context::<Services>();
    let initial_state = use_context::<AppState>();
    let initial = use_context::<Initial>();
    let mut state = use_context_provider(|| Signal::new(initial_state));
    let settings = use_context_provider(|| Signal::new(initial.settings));
    let diff_windows = use_context_provider(DiffWindows::default);
    use_context_provider(|| SettingsOpen(Signal::new(false)));
    use_layout_keeper(&services, initial.layout);
    use_drop({
        let diff_windows = diff_windows.clone();
        move || diff_windows.close_all()
    });

    // The window starts with the stored theme; this follows later changes.
    let theme = use_memo(move || settings.read().theme);
    use_effect(move || {
        let theme = theme();
        document::eval(&theme_script(theme));
        dioxus::desktop::window().set_theme(native_theme(theme));
        diff_windows.set_theme(theme);
    });

    // Load the recent list once. A repository opened at startup is recorded
    // first, which also returns the updated list.
    use_hook({
        let recent = services.recent_repositories.clone();
        move || {
            let startup_root = state
                .peek()
                .current_repository()
                .map(|r| r.root().to_path_buf());
            spawn(async move {
                let result = blocking(move || match startup_root {
                    Some(root) => recent.record(&root),
                    None => recent.list(),
                })
                .await;
                apply_recent(state, result);
            });
        }
    });

    use_effect(move || dioxus::desktop::window().set_title(&state.read().window_title()));

    let open = use_callback({
        let services = services.clone();
        move |path: PathBuf| {
            spawn(open_path(services.clone(), state, path));
        }
    });
    let pick = use_callback(move |()| {
        spawn(async move {
            let folder = rfd::AsyncFileDialog::new()
                .set_title("Open repository")
                .pick_folder()
                .await;
            if let Some(folder) = folder {
                open.call(folder.path().to_path_buf());
            }
        });
    });
    let forget = use_callback(move |path: PathBuf| {
        let recent = services.recent_repositories.clone();
        spawn(async move {
            let result = blocking(move || recent.forget(&path)).await;
            apply_recent(state, result);
        });
    });

    let screen = state.read().screen.clone();
    rsx! {
        div { class: "app",
            ErrorBanner {}
            match screen {
                Screen::Welcome => rsx! {
                    Welcome { on_open: open, on_pick: pick, on_forget: forget }
                },
                Screen::Repository(repo) => rsx! {
                    RepositoryView {
                        // A new key per repository gives every repository a fresh session.
                        key: "{repo.root().display()}",
                        repo,
                        on_pick: pick,
                        on_close: move |()| state.write().close_repository(),
                    }
                },
            }
            SettingsDialog {}
        }
    }
}

/// Applies a change to the settings and saves them in the background.
pub(crate) fn update_settings(
    services: &Services,
    mut settings: Signal<Settings>,
    mut state: Signal<AppState>,
    change: impl FnOnce(&mut Settings),
) {
    change(&mut settings.write());
    let saved = *settings.peek();
    let user_settings = services.user_settings.clone();
    spawn(async move {
        match blocking(move || user_settings.save(&saved)).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => state.write().error = Some(format!("Settings: {err}")),
            Err(message) => state.write().error = Some(message),
        }
    });
}

async fn open_path(services: Services, mut state: Signal<AppState>, path: PathBuf) {
    let open = services.open_repository;
    match blocking(move || open.open(&path)).await {
        Ok(Ok(repo)) => {
            let root = repo.root().to_path_buf();
            state.write().opened(repo);
            let recent = services.recent_repositories;
            let result = blocking(move || recent.record(&root)).await;
            apply_recent(state, result);
        }
        Ok(Err(err)) => state.write().open_failed(&err),
        Err(message) => state.write().error = Some(message),
    }
}

fn apply_recent(
    mut state: Signal<AppState>,
    result: Result<Result<Vec<PathBuf>, StorageError>, String>,
) {
    match result {
        Ok(Ok(paths)) => state.write().set_recent(&paths),
        Ok(Err(err)) => state.write().recent_failed(&err),
        Err(message) => state.write().error = Some(message),
    }
}

/// The current time in seconds since the epoch, for relative dates.
pub(crate) fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Runs a synchronous use case off the UI thread.
pub(crate) async fn blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|err| format!("Internal error: {err}"))
}
