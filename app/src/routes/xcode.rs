use crate::xcode;
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct XcodeSchemesRequest {
    pub path: String,
}

/// Get Xcode schemes for a project or workspace
pub async fn get_xcode_schemes(Json(request): Json<XcodeSchemesRequest>) -> impl IntoResponse {
    let path = request.path.clone();

    // Run blocking I/O in a separate thread pool to avoid blocking the async runtime
    let result = tokio::task::spawn_blocking(move || {
        let path = Path::new(&path);
        xcode::discover_project(path)
    })
    .await;

    match result {
        Ok(Ok(project)) => (
            StatusCode::OK,
            Json(json!({
                "path": project.path,
                "project_type": match project.project_type {
                    xcode::ProjectType::Project => "project",
                    xcode::ProjectType::Workspace => "workspace",
                },
                "schemes": project.schemes,
                "targets": project.targets,
                "configurations": project.configurations,
            })),
        ),
        Ok(Err(error)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Task execution failed" })),
        ),
    }
}
