//! Plasma UI Components
//!
//! This crate contains all UI components and views for the Plasma app.

use gpui::{AppContext, SharedString};

pub mod app;
pub mod get_started;
pub mod main_layout;
pub mod theme;

/// Initialize the Plasma UI library
pub fn init(cx: &mut AppContext) {
    theme::install_theme(cx);
}

/// Shared application state
pub struct AppState {
    /// Currently selected Xcode project path
    pub project_path: Option<SharedString>,
    /// Whether we're currently in a simulator session
    pub in_simulator_session: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            project_path: None,
            in_simulator_session: false,
        }
    }

    pub fn with_project(path: String) -> Self {
        Self {
            project_path: Some(path.into()),
            in_simulator_session: false,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export commonly used items
pub use app::PlasmaApp;
pub use get_started::GetStartedView;
pub use main_layout::MainLayoutView;
