//! The app's secondary windows: diff windows and conflict resolvers.

use super::conflict_window::{ConflictServices, ConflictWindow, ConflictWindowProps};
use super::diff_window::{DiffWindow, DiffWindowProps};
use crate::launch::{custom_head, native_theme, window_icon};
use crate::view::conflict::window_title as conflict_title;
use crate::view::diff::{DiffKey, window_title as diff_title};
use crate::view::settings::theme_script;
use application::{ComputeFileDiff, Theme};
use dioxus::desktop::{Config, LogicalSize, WeakDesktopContext, WindowBuilder};
use dioxus::prelude::*;
use domain::{DiffSource, FileChange, OngoingOperation, RepositoryInfo};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Identifies a window, so each file opens at most one of each kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum WindowKey {
    Diff(DiffKey),
    Conflict { repo: PathBuf, path: String },
}

/// The open secondary windows of the app, and the theme they use. A
/// `None` entry is a window that is still being created.
#[derive(Clone, Default)]
pub struct ChildWindows {
    windows: Rc<RefCell<HashMap<WindowKey, Option<WeakDesktopContext>>>>,
    theme: Rc<Cell<Theme>>,
}

impl ChildWindows {
    /// Applies `theme` to the open windows and the ones opened later.
    pub fn set_theme(&self, theme: Theme) {
        if self.theme.replace(theme) == theme {
            return;
        }
        let script = theme_script(theme);
        for window in self.windows.borrow().values().flatten() {
            if let Some(window) = window.upgrade() {
                // A window that cannot run the script keeps its old theme.
                let _ = window.webview.evaluate_script(&script);
                window.window.set_theme(native_theme(theme));
            }
        }
    }

    /// Opens the diff of `change`, or focuses its window if it is open.
    pub fn open_diff(
        &self,
        compute: ComputeFileDiff,
        repo: RepositoryInfo,
        source: DiffSource,
        change: FileChange,
    ) {
        let key = WindowKey::Diff(DiffKey {
            repo: repo.root().to_path_buf(),
            source,
            path: change.path.clone(),
        });
        let title = diff_title(&change.path, source);
        self.open(key, title, LogicalSize::new(1300.0, 850.0), move || {
            VirtualDom::new_with_props(
                DiffWindow,
                DiffWindowProps {
                    repo,
                    source,
                    change,
                },
            )
            .with_root_context(compute)
        });
    }

    /// Opens the resolver for the conflicted file at `path`, or focuses
    /// its window if it is open. `operation` names the sides.
    pub fn open_conflict(
        &self,
        services: ConflictServices,
        repo: RepositoryInfo,
        path: String,
        operation: Option<OngoingOperation>,
    ) {
        let key = WindowKey::Conflict {
            repo: repo.root().to_path_buf(),
            path: path.clone(),
        };
        let title = conflict_title(&path);
        self.open(key, title, LogicalSize::new(1200.0, 850.0), move || {
            VirtualDom::new_with_props(
                ConflictWindow,
                ConflictWindowProps {
                    repo,
                    path,
                    operation,
                },
            )
            .with_root_context(services)
        });
    }

    fn open(
        &self,
        key: WindowKey,
        title: String,
        size: LogicalSize<f64>,
        dom: impl FnOnce() -> VirtualDom,
    ) {
        match self.windows.borrow().get(&key) {
            Some(None) => return,
            Some(Some(weak)) => {
                if let Some(window) = weak.upgrade() {
                    window.window.set_focus();
                    return;
                }
            }
            None => {}
        }
        self.windows.borrow_mut().insert(key.clone(), None);

        let builder = WindowBuilder::new()
            .with_title(title)
            .with_inner_size(size)
            .with_min_inner_size(LogicalSize::new(500.0, 300.0))
            .with_theme(native_theme(self.theme.get()))
            .with_window_icon(window_icon());
        let config = Config::new()
            .with_window(builder)
            .with_menu(None)
            .with_custom_head(custom_head(self.theme.get()));
        let pending = dioxus::desktop::window().new_window(dom(), config);
        let windows = self.windows.clone();
        spawn(async move {
            match pending.try_resolve().await {
                Ok(window) => {
                    windows
                        .borrow_mut()
                        .insert(key, Some(Rc::downgrade(&window)));
                }
                Err(_) => {
                    windows.borrow_mut().remove(&key);
                }
            }
        });
    }

    /// Closes every secondary window, e.g. when the main window closes.
    pub fn close_all(&self) {
        for weak in self.windows.borrow_mut().drain().filter_map(|(_, w)| w) {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        }
    }
}
