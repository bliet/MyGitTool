//! Dioxus desktop UI of MyGitTool.
//!
//! Components call use cases only. Logic lives in view-state structs (see
//! [`AppState`]) so it can be tested without rendering.

mod components;
mod launch;
mod state;
pub mod view;

pub use launch::{Services, launch, window_icon};
pub use state::{AppState, RecentEntry, Screen};
