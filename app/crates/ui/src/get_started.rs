//! Get Started / Intro View
//!
//! The initial view shown to users where they can select an Xcode project.

use gpui::{
    div, px, rgb, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, ViewContext,
};

use crate::theme::{use_colors, Colors};

/// Events emitted by the GetStartedView
#[derive(Clone, Debug)]
pub enum GetStartedEvent {
    /// User selected a project
    ProjectSelected { path: String },
    /// User wants to open a project
    OpenProject,
}

/// The get started/intro view where users select their Xcode project
pub struct GetStartedView {
    /// Path to the selected project
    project_path: Option<String>,
    /// Whether we're currently loading/discovering
    #[allow(dead_code)]
    loading: bool,
}

impl EventEmitter<GetStartedEvent> for GetStartedView {}

impl GetStartedView {
    /// Create a new GetStartedView
    pub fn new(_cx: &mut ViewContext<Self>) -> Self {
        Self {
            project_path: None,
            loading: false,
        }
    }

    /// Handle opening a project
    fn open_project(&mut self, cx: &mut ViewContext<Self>) {
        // TODO: Implement native file picker
        cx.emit(GetStartedEvent::OpenProject);
    }

    /// Handle selecting the current project
    fn select_project(&mut self, cx: &mut ViewContext<Self>) {
        if let Some(path) = &self.project_path {
            cx.emit(GetStartedEvent::ProjectSelected {
                path: path.clone(),
            });
        }
    }
}

impl Render for GetStartedView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = use_colors(cx);

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .bg(colors.background)
            .child(render_header(colors))
            .child(render_project_selector(self, colors, cx))
            .child(render_footer(colors))
    }
}

fn render_header(colors: Colors) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(32.0))
        .mb(px(32.0))
        .child(
            div()
                .text_xl()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(colors.text)
                .child("Plasma"),
        )
        .child(
            div()
                .text_base()
                .text_color(colors.text_muted)
                .child("iOS Simulator Streaming"),
        )
}

fn render_project_selector(
    view: &GetStartedView,
    colors: Colors,
    cx: &mut ViewContext<GetStartedView>,
) -> impl IntoElement {
    let has_project = view.project_path.is_some();

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .w(px(600.0))
        .p(px(24.0))
        .bg(colors.surface)
        .rounded_lg()
        .border_1()
        .border_color(colors.border)
        .child(
            div()
                .text_base()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(colors.text)
                .mb(px(8.0))
                .child("Select Xcode Project"),
        )
        .child(
            div()
                .text_sm()
                .text_color(colors.text_muted)
                .mb(px(16.0))
                .child("Choose an Xcode project to build and run in the simulator"),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .h(px(40.0))
                        .px(px(12.0))
                        .bg(colors.background)
                        .border_1()
                        .border_color(colors.border)
                        .rounded_md()
                        .flex()
                        .items_center()
                        .text_sm()
                        .text_color(if view.project_path.is_some() {
                            colors.text
                        } else {
                            colors.text_placeholder
                        })
                        .child(
                            view.project_path
                                .as_ref()
                                .cloned()
                                .unwrap_or_else(|| "/path/to/your/project".to_string()),
                        ),
                )
                .child(
                    render_button("browse-btn", "Browse...", false, colors)
                        .on_click(cx.listener(|view, _, cx| view.open_project(cx))),
                ),
        )
        .child(
            render_button("continue-btn", "Continue", has_project, colors)
                .on_click(cx.listener(|view, _, cx| view.select_project(cx))),
        )
}

fn render_button(id: impl Into<gpui::ElementId>, label: &str, enabled: bool, colors: Colors) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .px(px(24.0))
        .h(px(40.0))
        .min_w(px(120.0))
        .bg(if enabled {
            colors.primary
        } else {
            colors.surface
        })
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(if enabled {
            rgb(0xffffff)
        } else {
            colors.text_muted
        })
        .cursor_pointer()
        .child(label.to_string())
}

fn render_footer(colors: Colors) -> impl IntoElement {
    div()
        .mt(px(32.0))
        .text_sm()
        .text_color(colors.text_placeholder)
        .child("Open an Xcode project or workspace to get started")
}
