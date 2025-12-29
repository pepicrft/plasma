//! Main Layout View
//!
//! The main view shown after project selection, containing the simulator stream
//! and controls.

use gpui::{
    div, px, rgb, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, ViewContext,
};

use crate::theme::{use_colors, Colors};

/// Events emitted by the MainLayoutView
#[derive(Clone, Debug)]
pub enum MainLayoutEvent {
    /// User wants to go back to project selection
    BackToProject,
    /// User wants to build and run
    BuildAndRun,
    /// User wants to stop the simulator
    StopSimulator,
}

/// View state for the main layout
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum MainViewState {
    /// Not actively running anything
    Idle,
    /// Building the project
    Building,
    /// Running in simulator
    Running,
}

/// The main layout view with simulator stream and controls
pub struct MainLayoutView {
    /// Current project path
    project_path: String,
    /// Current view state
    state: MainViewState,
    /// Currently selected simulator
    #[allow(dead_code)]
    selected_simulator: Option<String>,
    /// List of available simulators
    simulators: Vec<String>,
}

impl EventEmitter<MainLayoutEvent> for MainLayoutView {}

impl MainLayoutView {
    /// Create a new MainLayoutView
    pub fn new(project_path: String, _cx: &mut ViewContext<Self>) -> Self {
        Self {
            project_path,
            state: MainViewState::Idle,
            selected_simulator: None,
            simulators: vec![],
        }
    }

    /// Handle back button click
    fn back(&mut self, cx: &mut ViewContext<Self>) {
        cx.emit(MainLayoutEvent::BackToProject);
    }

    /// Handle build and run button click
    fn build_and_run(&mut self, cx: &mut ViewContext<Self>) {
        self.state = MainViewState::Building;
        cx.notify();
        // TODO: Trigger actual build
    }

    /// Handle stop button click
    fn stop(&mut self, cx: &mut ViewContext<Self>) {
        self.state = MainViewState::Idle;
        cx.notify();
        // TODO: Stop simulator
    }
}

impl Render for MainLayoutView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = use_colors(cx);

        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(colors.background)
            .child(render_sidebar(self, colors, cx))
            .child(render_main_content(self, colors, cx))
    }
}

fn render_sidebar(
    view: &MainLayoutView,
    colors: Colors,
    cx: &mut ViewContext<MainLayoutView>,
) -> impl IntoElement {
    div()
        .w(px(280.0))
        .h_full()
        .bg(colors.surface)
        .border_r_1()
        .border_color(colors.border)
        .flex()
        .flex_col()
        .child(render_sidebar_header(colors, cx))
        .child(render_project_info(view, colors))
        .child(render_simulator_list(view, colors))
}

fn render_sidebar_header(
    colors: Colors,
    cx: &mut ViewContext<MainLayoutView>,
) -> impl IntoElement {
    div()
        .h(px(60.0))
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(colors.border)
        .child(
            div()
                .text_base()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(colors.text)
                .child("Plasma"),
        )
        .child(
            div()
                .id("back-btn")
                .text_xs()
                .text_color(colors.text_muted)
                .cursor_pointer()
                .child("Back")
                .on_click(cx.listener(|view, _, cx| view.back(cx))),
        )
}

fn render_project_info(view: &MainLayoutView, colors: Colors) -> impl IntoElement {
    div()
        .p(px(16.0))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(colors.text_muted)
                .mb(px(4.0))
                .child("PROJECT"),
        )
        .child(
            div()
                .text_sm()
                .text_color(colors.text)
                .child(view.project_path.clone()),
        )
}

fn render_simulator_list(view: &MainLayoutView, colors: Colors) -> impl IntoElement {
    div()
        .flex_1()
        .p(px(16.0))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(colors.text_muted)
                .mb(px(8.0))
                .child("SIMULATORS"),
        )
        .child(render_simulator_items(view, colors))
}

fn render_simulator_items(view: &MainLayoutView, colors: Colors) -> impl IntoElement {
    if view.simulators.is_empty() {
        div()
            .text_sm()
            .text_color(colors.text_placeholder)
            .child("No simulators available")
    } else {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(
                view.simulators
                    .iter()
                    .map(|sim| render_simulator_item(sim, colors)),
            )
    }
}

fn render_simulator_item(name: &str, colors: Colors) -> impl IntoElement {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded_md()
        .text_sm()
        .text_color(colors.text)
        .child(name.to_string())
}

fn render_main_content(
    view: &MainLayoutView,
    colors: Colors,
    cx: &mut ViewContext<MainLayoutView>,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .child(render_toolbar(view, colors, cx))
        .child(render_content_area(view, colors))
}

fn render_toolbar(
    view: &MainLayoutView,
    colors: Colors,
    cx: &mut ViewContext<MainLayoutView>,
) -> impl IntoElement {
    div()
        .h(px(60.0))
        .px(px(24.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.surface)
        .child(
            div()
                .text_base()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(colors.text)
                .child(match view.state {
                    MainViewState::Idle => "Ready",
                    MainViewState::Building => "Building...",
                    MainViewState::Running => "Running",
                }),
        )
        .child(render_toolbar_actions(view, colors, cx))
}

fn render_toolbar_actions(
    view: &MainLayoutView,
    colors: Colors,
    cx: &mut ViewContext<MainLayoutView>,
) -> impl IntoElement {
    match view.state {
        MainViewState::Idle | MainViewState::Building => {
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    render_action_button("build-run-btn", "Build & Run", colors)
                        .on_click(cx.listener(|view, _, cx| view.build_and_run(cx))),
                )
        }
        MainViewState::Running => {
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    render_action_button("stop-btn", "Stop", colors)
                        .on_click(cx.listener(|view, _, cx| view.stop(cx))),
                )
        }
    }
}

fn render_action_button(id: impl Into<gpui::ElementId>, label: &str, colors: Colors) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .px(px(16.0))
        .h(px(36.0))
        .bg(colors.primary)
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(0xffffff))
        .cursor_pointer()
        .child(label.to_string())
}

fn render_content_area(view: &MainLayoutView, colors: Colors) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .bg(colors.simulator_background)
        .child(match view.state {
            MainViewState::Idle => render_idle_state(colors).into_any_element(),
            MainViewState::Building => render_building_state(colors).into_any_element(),
            MainViewState::Running => render_simulator_frame(colors).into_any_element(),
        })
}

fn render_idle_state(colors: Colors) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(16.0))
        .text_color(colors.text_muted)
        .child(
            div()
                .w(px(64.0))
                .h(px(64.0))
                .bg(colors.surface)
                .rounded_xl()
                .border_2()
                .border_color(colors.border),
        )
        .child(
            div()
                .text_base()
                .child("Select a simulator and build your app"),
        )
}

fn render_building_state(colors: Colors) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(16.0))
        .text_color(colors.text_muted)
        .child(
            // Simple spinner placeholder
            div()
                .w(px(48.0))
                .h(px(48.0))
                .rounded_full()
                .border_4()
                .border_color(colors.border),
        )
        .child(
            div()
                .text_base()
                .child("Building your project..."),
        )
        .child(
            div()
                .text_sm()
                .child("This may take a moment"),
        )
}

fn render_simulator_frame(colors: Colors) -> impl IntoElement {
    // TODO: Render actual simulator frames
    div()
        .w(px(390.0)) // iPhone 14 width
        .h(px(844.0)) // iPhone 14 height
        .bg(colors.simulator_frame)
        .rounded_3xl()
        .border_8()
        .border_color(colors.surface)
        .flex()
        .items_center()
        .justify_center()
        .text_color(colors.text_placeholder)
        .child("Simulator Stream")
}
