mod health;
mod projects;
mod xcode;

use crate::server::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

/// Create all routes for the application
pub fn create_routes(frontend_dir: Option<&str>) -> Router<Arc<AppState>> {
    let api_routes = Router::new()
        .route("/health", get(health::health))
        .route("/about", get(health::about))
        .route("/projects/validate", post(projects::validate_project))
        .route("/projects/recent", get(projects::get_recent_projects))
        .route("/xcode/discover", post(xcode::discover_project));

    let router = Router::new().nest("/api", api_routes);

    // Serve frontend if directory is provided
    if let Some(dir) = frontend_dir {
        let serve_dir = ServeDir::new(dir).fallback(ServeFile::new(format!("{}/index.html", dir)));
        router.fallback_service(serve_dir)
    } else {
        router
    }
}
