use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

/// Health check endpoint
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// About endpoint with app info
pub async fn about() -> impl IntoResponse {
    Json(json!({
        "name": "plasma",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
