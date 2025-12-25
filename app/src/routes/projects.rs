use crate::server::AppState;
use crate::services::projects;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ValidateProjectRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ValidateProjectResponse {
    Valid(projects::Project),
    Invalid { error: String },
}

/// Validate that a path contains a valid project (Xcode or Android)
pub async fn validate_project(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ValidateProjectRequest>,
) -> impl IntoResponse {
    let path = Path::new(&request.path);

    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ValidateProjectResponse::Invalid {
                error: "Path does not exist".to_string(),
            }),
        );
    }

    match projects::detect_project(path) {
        Some(project) => {
            // Save to database in the background
            if let Err(e) =
                projects::save_project(state.db.conn(), &project.path, &project.name).await
            {
                tracing::warn!("Failed to save project to database: {}", e);
            }

            (
                StatusCode::OK,
                Json(ValidateProjectResponse::Valid(project)),
            )
        }
        None => (
            StatusCode::BAD_REQUEST,
            Json(ValidateProjectResponse::Invalid {
                error: "No Xcode or Android project found".to_string(),
            }),
        ),
    }
}

#[derive(Debug, Deserialize)]
pub struct RecentProjectsQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_limit() -> u64 {
    10
}

/// Get recent projects, validating each one still exists
pub async fn get_recent_projects(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RecentProjectsQuery>,
) -> impl IntoResponse {
    match projects::get_recent_projects(state.db.conn(), params.query.as_deref(), params.limit)
        .await
    {
        Ok(projects) => (StatusCode::OK, Json(json!({ "projects": projects }))),
        Err(e) => {
            tracing::error!("Failed to fetch projects: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to fetch projects" })),
            )
        }
    }
}
