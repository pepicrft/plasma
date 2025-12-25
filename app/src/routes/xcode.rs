use crate::xcode;
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct DiscoverProjectRequest {
    pub path: String,
}

/// Discover Xcode project information (schemes, targets, configurations)
pub async fn discover_project(Json(request): Json<DiscoverProjectRequest>) -> impl IntoResponse {
    let path = Path::new(&request.path);

    match xcode::discover_project(path).await {
        Ok(project) => (
            StatusCode::OK,
            Json(Value::from(serde_json::to_value(project).unwrap())),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}
