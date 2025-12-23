use crate::server::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

/// Create all routes for the application
pub fn create_routes(frontend_dir: Option<&str>) -> Router<Arc<AppState>> {
    let api_routes = Router::new()
        .route("/health", get(health))
        .route("/about", get(about))
        .route("/projects/validate", post(validate_project))
        .route("/projects/recent", get(get_recent_projects));

    let router = Router::new().nest("/api", api_routes);

    // Serve frontend if directory is provided
    if let Some(dir) = frontend_dir {
        let serve_dir = ServeDir::new(dir).fallback(ServeFile::new(format!("{}/index.html", dir)));
        router.fallback_service(serve_dir)
    } else {
        router
    }
}

/// Health check endpoint
async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// About endpoint with app info
async fn about() -> impl IntoResponse {
    Json(json!({
        "name": "plasma",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[derive(Debug, Deserialize)]
struct ValidateProjectRequest {
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "valid")]
enum ValidateProjectResponse {
    #[serde(rename = "true")]
    Valid {
        #[serde(rename = "type")]
        project_type: ProjectType,
        name: String,
    },
    #[serde(rename = "false")]
    Invalid { error: String },
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ProjectType {
    Xcode,
    Android,
}

/// Validate that a directory contains a valid project (Xcode or Android)
async fn validate_project(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ValidateProjectRequest>,
) -> impl IntoResponse {
    let path = Path::new(&request.path);

    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ValidateProjectResponse::Invalid {
                error: "Directory does not exist".to_string(),
            }),
        );
    }

    if !path.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ValidateProjectResponse::Invalid {
                error: "Path is not a directory".to_string(),
            }),
        );
    }

    match detect_project_type(path) {
        Some((project_type, name)) => {
            // Save to recent projects
            let type_str = match project_type {
                ProjectType::Xcode => "xcode",
                ProjectType::Android => "android",
            };
            let _ = state
                .db
                .projects()
                .upsert(&request.path, &name, type_str)
                .await;

            (
                StatusCode::OK,
                Json(ValidateProjectResponse::Valid { project_type, name }),
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

/// Detect the project type by looking for specific files/directories
fn detect_project_type(path: &Path) -> Option<(ProjectType, String)> {
    let entries = std::fs::read_dir(path).ok()?;

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        // Check for Xcode workspace first (takes priority over .xcodeproj)
        if file_name_str.ends_with(".xcworkspace") {
            let name = file_name_str.trim_end_matches(".xcworkspace").to_string();
            return Some((ProjectType::Xcode, name));
        }

        // Check for Xcode project
        if file_name_str.ends_with(".xcodeproj") {
            let name = file_name_str.trim_end_matches(".xcodeproj").to_string();
            return Some((ProjectType::Xcode, name));
        }

        // Check for Android project (Gradle build files)
        if file_name_str == "build.gradle" || file_name_str == "build.gradle.kts" {
            // Use the directory name as project name
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            return Some((ProjectType::Android, name));
        }
    }

    None
}

#[derive(Debug, Deserialize)]
struct RecentProjectsQuery {
    #[serde(default)]
    query: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    10
}

#[derive(Debug, Serialize)]
struct RecentProject {
    path: String,
    name: String,
    #[serde(rename = "type")]
    project_type: String,
    valid: bool,
}

/// Get recent projects, validating each one still exists
async fn get_recent_projects(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RecentProjectsQuery>,
) -> impl IntoResponse {
    let projects = if let Some(query) = params.query {
        state.db.projects().search(&query, params.limit).await
    } else {
        state.db.projects().get_recent(params.limit).await
    };

    match projects {
        Ok(projects) => {
            // Validate each project still exists
            let validated: Vec<RecentProject> = projects
                .into_iter()
                .map(|p| {
                    let path = Path::new(&p.path);
                    let valid = path.exists() && detect_project_type(path).is_some();
                    RecentProject {
                        path: p.path,
                        name: p.name,
                        project_type: p.project_type,
                        valid,
                    }
                })
                .collect();

            (StatusCode::OK, Json(json!({ "projects": validated })))
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to fetch projects" })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    #[test]
    fn test_detect_xcode_project() {
        let dir = create_test_dir();
        std::fs::create_dir(dir.path().join("MyApp.xcodeproj")).unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (project_type, name) = result.unwrap();
        assert_eq!(project_type, ProjectType::Xcode);
        assert_eq!(name, "MyApp");
    }

    #[test]
    fn test_detect_xcode_workspace() {
        let dir = create_test_dir();
        std::fs::create_dir(dir.path().join("MyWorkspace.xcworkspace")).unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (project_type, name) = result.unwrap();
        assert_eq!(project_type, ProjectType::Xcode);
        assert_eq!(name, "MyWorkspace");
    }

    #[test]
    fn test_detect_android_project_groovy() {
        let dir = create_test_dir();
        std::fs::write(dir.path().join("build.gradle"), "// gradle build").unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (project_type, _name) = result.unwrap();
        assert_eq!(project_type, ProjectType::Android);
    }

    #[test]
    fn test_detect_android_project_kotlin() {
        let dir = create_test_dir();
        std::fs::write(dir.path().join("build.gradle.kts"), "// kotlin gradle build").unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (project_type, _name) = result.unwrap();
        assert_eq!(project_type, ProjectType::Android);
    }

    #[test]
    fn test_detect_no_project() {
        let dir = create_test_dir();
        std::fs::write(dir.path().join("README.md"), "# Hello").unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_empty_directory() {
        let dir = create_test_dir();

        let result = detect_project_type(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_workspace_takes_priority_over_project() {
        let dir = create_test_dir();
        std::fs::create_dir(dir.path().join("MyApp.xcodeproj")).unwrap();
        std::fs::create_dir(dir.path().join("MyApp.xcworkspace")).unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (project_type, name) = result.unwrap();
        assert_eq!(project_type, ProjectType::Xcode);
        assert_eq!(name, "MyApp");
    }
}
