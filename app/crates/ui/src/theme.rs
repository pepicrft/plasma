//! Theme system for Plasma
//!
//! Provides a modern dark theme with Plasma branding colors.

use gpui::{rgb, AppContext, Global, Rgba};

/// Plasma color palette
#[derive(Clone, Copy, Debug)]
pub struct Colors {
    // Primary (Plasma brand color)
    pub primary: Rgba,
    pub primary_hover: Rgba,
    pub primary_active: Rgba,

    // Background colors
    pub background: Rgba,
    pub surface: Rgba,
    pub surface_hover: Rgba,

    // Border colors
    pub border: Rgba,
    pub border_focus: Rgba,

    // Text colors
    pub text: Rgba,
    pub text_muted: Rgba,
    pub text_placeholder: Rgba,

    // Status colors
    pub success: Rgba,
    pub warning: Rgba,
    pub error: Rgba,
    pub info: Rgba,

    // Simulator specific
    pub simulator_frame: Rgba,
    pub simulator_background: Rgba,
}

impl Colors {
    /// Dark theme for Plasma
    pub fn dark() -> Self {
        Self {
            // Primary - Plasma purple/indigo
            primary: rgb(0x6366f1),       // Indigo 500
            primary_hover: rgb(0x818cf8),  // Indigo 400
            primary_active: rgb(0x4f46e5), // Indigo 600

            // Backgrounds
            background: rgb(0x0f0f14),      // Very dark
            surface: rgb(0x1c1c24),         // Slightly lighter
            surface_hover: rgb(0x272732),   // Hover state

            // Borders
            border: rgb(0x2a2a35),         // Subtle border
            border_focus: rgb(0x6366f1),    // Focus matches primary

            // Text
            text: rgb(0xf5f5f5),            // Nearly white
            text_muted: rgb(0x9ca3af),      // Gray 400
            text_placeholder: rgb(0x6b7280), // Gray 500

            // Status
            success: rgb(0x22c55e),         // Green 500
            warning: rgb(0xf59e0b),         // Amber 500
            error: rgb(0xef4444),           // Red 500
            info: rgb(0x3b82f6),            // Blue 500

            // Simulator
            simulator_frame: rgb(0x000000),
            simulator_background: rgb(0x1a1a1a),
        }
    }

    /// Light theme for Plasma
    pub fn light() -> Self {
        Self {
            // Primary
            primary: rgb(0x4f46e5),       // Indigo 600
            primary_hover: rgb(0x6366f1),  // Indigo 500
            primary_active: rgb(0x4338ca), // Indigo 700

            // Backgrounds
            background: rgb(0xffffff),      // White
            surface: rgb(0xf9fafb),        // Gray 50
            surface_hover: rgb(0xf3f4f6),  // Gray 100

            // Borders
            border: rgb(0xe5e7eb),         // Gray 200
            border_focus: rgb(0x4f46e5),    // Focus matches primary

            // Text
            text: rgb(0x111827),            // Gray 900
            text_muted: rgb(0x6b7280),      // Gray 500
            text_placeholder: rgb(0x9ca3af), // Gray 400

            // Status
            success: rgb(0x16a34a),         // Green 600
            warning: rgb(0xd97706),         // Amber 600
            error: rgb(0xdc2626),           // Red 600
            info: rgb(0x2563eb),            // Blue 600

            // Simulator
            simulator_frame: rgb(0xe5e5e5),
            simulator_background: rgb(0xf5f5f5),
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self::dark()
    }
}

/// Theme model shared across the app
pub struct Theme {
    pub colors: Colors,
}

impl Theme {
    pub fn new(colors: Colors) -> Self {
        Self { colors }
    }

    pub fn dark() -> Self {
        Self::new(Colors::dark())
    }

    pub fn light() -> Self {
        Self::new(Colors::light())
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Global for Theme {}

/// Install the theme globally
pub fn install_theme(cx: &mut AppContext) {
    cx.set_global(Theme::default());
}

/// Get the current theme from context
pub fn use_theme(cx: &AppContext) -> &Theme {
    cx.global::<Theme>()
}

/// Get colors from the theme
pub fn use_colors(cx: &AppContext) -> Colors {
    use_theme(cx).colors
}
