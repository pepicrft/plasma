mod health;
mod projects;
mod xcode;

use crate::server::AppState;
use crate::simulator;
use axum::{
    response::Html,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

/// Check if we're in development mode (Vite dev server running)
fn is_dev_mode() -> bool {
    // Check if VITE_DEV environment variable is set, or if we're in debug build
    std::env::var("VITE_DEV").is_ok() || cfg!(debug_assertions)
}

/// Generate development HTML that loads from Vite dev server
fn dev_index_html() -> Html<&'static str> {
    Html(r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/png" href="http://localhost:5173/plasma-icon.png" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Plasma</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module">
      import RefreshRuntime from 'http://localhost:5173/@react-refresh'
      RefreshRuntime.injectIntoGlobalHook(window)
      window.$RefreshReg$ = () => {}
      window.$RefreshSig$ = () => (type) => type
      window.__vite_plugin_react_preamble_installed__ = true
    </script>
    <script type="module" src="http://localhost:5173/@vite/client"></script>
    <script type="module" src="http://localhost:5173/src/main.tsx"></script>
  </body>
</html>"#)
}

/// Create all routes for the application
pub fn create_routes(frontend_dir: Option<&str>) -> Router<Arc<AppState>> {
    let api_routes = Router::new()
        .route("/health", get(health::health))
        .route("/about", get(health::about))
        .route("/projects/validate", post(projects::validate_project))
        .route("/projects/recent", get(projects::get_recent_projects))
        .route("/xcode/discover", post(xcode::discover_project))
        .route("/xcode/build", post(xcode::build_scheme))
        .route("/xcode/build/stream", post(xcode::build_scheme_stream))
        .route(
            "/xcode/launchable-products",
            post(xcode::get_launchable_products),
        )
        .route("/simulator/list", get(simulator::list_simulators))
        .route("/simulator/launch", post(simulator::install_and_launch))
        .route("/simulator/stream", get(simulator::stream_simulator))
        .route("/simulator/stream/logs", get(simulator::stream_logs))
        .route("/simulator/touch", post(simulator::send_touch))
        .route("/simulator/tap", post(simulator::send_tap))
        .route("/simulator/swipe", post(simulator::send_swipe));

    let router = Router::new().nest("/api", api_routes);

    // In development, serve HTML that points to Vite dev server
    if is_dev_mode() {
        router.fallback(|| async { dev_index_html() })
    } else if let Some(dir) = frontend_dir {
        // In production, serve static files
        let serve_dir = ServeDir::new(dir).fallback(ServeFile::new(format!("{}/index.html", dir)));
        router.fallback_service(serve_dir)
    } else {
        router
    }
}
