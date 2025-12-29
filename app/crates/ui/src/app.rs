//! Main App Component
//!
//! Root component that manages navigation between views.

use gpui::{div, rgb, IntoElement, ParentElement, Render, Styled, View, ViewContext, VisualContext};

use crate::{get_started::GetStartedEvent, main_layout::MainLayoutEvent, GetStartedView, MainLayoutView};

/// App view state
#[derive(Clone, Copy, PartialEq, Eq)]
enum AppViewState {
    /// Showing the get started view
    GetStarted,
    /// Showing the main layout
    MainLayout,
}

/// The main Plasma app component
pub struct PlasmaApp {
    /// Current app state
    state: AppViewState,
    /// Get started view
    get_started_view: Option<View<GetStartedView>>,
    /// Main layout view
    main_layout_view: Option<View<MainLayoutView>>,
    /// Current project path
    project_path: Option<String>,
}

impl PlasmaApp {
    /// Create a new PlasmaApp
    pub fn new(cx: &mut ViewContext<Self>) -> Self {
        // Create the get started view
        let get_started_view = cx.new_view(|cx| GetStartedView::new(cx));

        // Subscribe to get started events
        cx.subscribe(&get_started_view, Self::handle_get_started_event)
            .detach();

        Self {
            state: AppViewState::GetStarted,
            get_started_view: Some(get_started_view),
            main_layout_view: None,
            project_path: None,
        }
    }

    /// Handle events from the get started view
    fn handle_get_started_event(
        &mut self,
        _view: View<GetStartedView>,
        event: &GetStartedEvent,
        cx: &mut ViewContext<Self>,
    ) {
        match event {
            GetStartedEvent::ProjectSelected { path } => {
                self.project_path = Some(path.clone());
                self.transition_to_main_layout(cx);
            }
            GetStartedEvent::OpenProject => {
                // TODO: Show file picker
            }
        }
    }

    /// Handle events from the main layout view
    fn handle_main_layout_event(
        &mut self,
        _view: View<MainLayoutView>,
        event: &MainLayoutEvent,
        cx: &mut ViewContext<Self>,
    ) {
        match event {
            MainLayoutEvent::BackToProject => {
                self.transition_to_get_started(cx);
            }
            MainLayoutEvent::BuildAndRun => {
                // TODO: Start build process
            }
            MainLayoutEvent::StopSimulator => {
                // TODO: Stop simulator
            }
        }
    }

    /// Transition to the main layout view
    fn transition_to_main_layout(&mut self, cx: &mut ViewContext<Self>) {
        self.state = AppViewState::MainLayout;

        // Create main layout view with the selected project
        let project_path = self.project_path.clone().unwrap_or_default();
        let main_layout_view = cx.new_view(|cx| MainLayoutView::new(project_path, cx));

        // Subscribe to main layout events
        cx.subscribe(&main_layout_view, Self::handle_main_layout_event)
            .detach();

        self.main_layout_view = Some(main_layout_view);
        cx.notify();
    }

    /// Transition to the get started view
    fn transition_to_get_started(&mut self, cx: &mut ViewContext<Self>) {
        self.state = AppViewState::GetStarted;
        self.project_path = None;
        cx.notify();
    }
}

impl Render for PlasmaApp {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x0f0f14))
            .child(match self.state {
                AppViewState::GetStarted => {
                    if let Some(ref view) = self.get_started_view {
                        view.clone().into_any_element()
                    } else {
                        div().into_any_element()
                    }
                }
                AppViewState::MainLayout => {
                    if let Some(ref view) = self.main_layout_view {
                        view.clone().into_any_element()
                    } else {
                        div().into_any_element()
                    }
                }
            })
    }
}
