use crate::db::entity::projects::{self, ProjectType};
use crate::server::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use sea_orm::{entity::*, query::*};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
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
        /// Project type inferred from path
        #[serde(rename = "type")]
        project_type: ProjectType,
        name: String,
        /// Canonical path to the project file (.xcworkspace, .xcodeproj, or build.gradle)
        path: String,
    },
    #[serde(rename = "false")]
    Invalid { error: String },
}

/// Detected project information
struct DetectedProject {
    project_type: ProjectType,
    name: String,
    /// Full path to the project file
    path: PathBuf,
}

/// Validate that a path contains a valid project (Xcode or Android)
async fn validate_project(
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

    // Check if this is a direct project file/bundle path, or a directory to search
    let detected = if is_project_path(path) {
        detect_project_from_path(path)
    } else if path.is_dir() {
        detect_project_in_directory(path)
    } else {
        None
    };

    match detected {
        Some(project) => {
            let path_str = project.path.to_string_lossy().to_string();

            // Try to find existing project by path
            let existing = projects::Entity::find()
                .filter(projects::Column::Path.eq(&path_str))
                .one(state.db.conn())
                .await;

            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

            match existing {
                Ok(Some(existing_project)) => {
                    // Update existing project
                    let mut active: projects::ActiveModel = existing_project.into();
                    active.name = Set(project.name.clone());
                    active.last_opened_at = Set(Some(now));
                    let _ = active.update(state.db.conn()).await;
                }
                Ok(None) => {
                    // Insert new project
                    let new_project = projects::ActiveModel {
                        id: NotSet,
                        path: Set(path_str.clone()),
                        name: Set(project.name.clone()),
                        last_opened_at: Set(Some(now.clone())),
                        created_at: Set(Some(now)),
                    };
                    let _ = projects::Entity::insert(new_project)
                        .exec(state.db.conn())
                        .await;
                }
                Err(_) => {}
            }

            (
                StatusCode::OK,
                Json(ValidateProjectResponse::Valid {
                    project_type: project.project_type,
                    name: project.name,
                    path: path_str,
                }),
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

/// Check if a path is a project file/bundle (not just a regular directory)
fn is_project_path(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy();

    name.ends_with(".xcworkspace")
        || name.ends_with(".xcodeproj")
        || name == "build.gradle"
        || name == "build.gradle.kts"
}

/// Detect project from a direct project file/bundle path
fn detect_project_from_path(path: &Path) -> Option<DetectedProject> {
    let file_name = path.file_name()?.to_string_lossy();

    // Xcode workspace
    if file_name.ends_with(".xcworkspace") {
        let name = file_name.trim_end_matches(".xcworkspace").to_string();
        return Some(DetectedProject {
            project_type: ProjectType::Xcode,
            name,
            path: path.to_path_buf(),
        });
    }

    // Xcode project
    if file_name.ends_with(".xcodeproj") {
        let name = file_name.trim_end_matches(".xcodeproj").to_string();
        return Some(DetectedProject {
            project_type: ProjectType::Xcode,
            name,
            path: path.to_path_buf(),
        });
    }

    // Android Gradle build file
    if file_name == "build.gradle" || file_name == "build.gradle.kts" {
        let name = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        return Some(DetectedProject {
            project_type: ProjectType::Android,
            name,
            path: path.to_path_buf(),
        });
    }

    None
}

/// Detect project by searching a directory for project files
fn detect_project_in_directory(path: &Path) -> Option<DetectedProject> {
    let entries = std::fs::read_dir(path).ok()?;

    // First pass: look for workspace (takes priority)
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str.ends_with(".xcworkspace") {
            let name = file_name_str.trim_end_matches(".xcworkspace").to_string();
            return Some(DetectedProject {
                project_type: ProjectType::Xcode,
                name,
                path: entry.path(),
            });
        }
    }

    // Second pass: look for project or gradle
    let entries = std::fs::read_dir(path).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str.ends_with(".xcodeproj") {
            let name = file_name_str.trim_end_matches(".xcodeproj").to_string();
            return Some(DetectedProject {
                project_type: ProjectType::Xcode,
                name,
                path: entry.path(),
            });
        }

        if file_name_str == "build.gradle" || file_name_str == "build.gradle.kts" {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            return Some(DetectedProject {
                project_type: ProjectType::Android,
                name,
                path: entry.path(),
            });
        }
    }

    None
}

#[derive(Debug, Deserialize)]
struct RecentProjectsQuery {
    #[serde(default)]
    query: Option<String>,
    #[serde(default = "default_limit")]
    limit: u64,
}

fn default_limit() -> u64 {
    10
}

#[derive(Debug, Serialize)]
struct RecentProject {
    /// Path to the project file
    path: String,
    name: String,
    /// Project type inferred from path
    #[serde(rename = "type")]
    project_type: ProjectType,
    /// Whether the project file still exists
    valid: bool,
}

/// Get recent projects, validating each one still exists
async fn get_recent_projects(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RecentProjectsQuery>,
) -> impl IntoResponse {
    let query = projects::Entity::find()
        .order_by_desc(projects::Column::LastOpenedAt)
        .limit(params.limit);

    let query = if let Some(ref search) = params.query {
        query.filter(projects::Column::Path.contains(search))
    } else {
        query
    };

    match query.all(state.db.conn()).await {
        Ok(projects) => {
            // Validate each project file still exists and has valid type
            let validated: Vec<RecentProject> = projects
                .into_iter()
                .filter_map(|p| {
                    let project_type = p.project_type()?;
                    let path = Path::new(&p.path);
                    let valid = path.exists();
                    Some(RecentProject {
                        path: p.path,
                        name: p.name,
                        project_type,
                        valid,
                    })
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
    fn test_detect_xcode_project_in_directory() {
        let dir = create_test_dir();
        std::fs::create_dir(dir.path().join("MyApp.xcodeproj")).unwrap();

        let result = detect_project_in_directory(dir.path());
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Xcode);
        assert_eq!(project.name, "MyApp");
        assert!(project.path.ends_with("MyApp.xcodeproj"));
    }

    #[test]
    fn test_detect_xcode_workspace_in_directory() {
        let dir = create_test_dir();
        std::fs::create_dir(dir.path().join("MyWorkspace.xcworkspace")).unwrap();

        let result = detect_project_in_directory(dir.path());
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Xcode);
        assert_eq!(project.name, "MyWorkspace");
        assert!(project.path.ends_with("MyWorkspace.xcworkspace"));
    }

    #[test]
    fn test_detect_android_project_groovy() {
        let dir = create_test_dir();
        std::fs::write(dir.path().join("build.gradle"), "// gradle build").unwrap();

        let result = detect_project_in_directory(dir.path());
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Android);
        assert!(project.path.ends_with("build.gradle"));
    }

    #[test]
    fn test_detect_android_project_kotlin() {
        let dir = create_test_dir();
        std::fs::write(
            dir.path().join("build.gradle.kts"),
            "// kotlin gradle build",
        )
        .unwrap();

        let result = detect_project_in_directory(dir.path());
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Android);
        assert!(project.path.ends_with("build.gradle.kts"));
    }

    #[test]
    fn test_detect_no_project() {
        let dir = create_test_dir();
        std::fs::write(dir.path().join("README.md"), "# Hello").unwrap();

        let result = detect_project_in_directory(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_empty_directory() {
        let dir = create_test_dir();

        let result = detect_project_in_directory(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_workspace_takes_priority_over_project() {
        let dir = create_test_dir();
        std::fs::create_dir(dir.path().join("MyApp.xcodeproj")).unwrap();
        std::fs::create_dir(dir.path().join("MyApp.xcworkspace")).unwrap();

        let result = detect_project_in_directory(dir.path());
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Xcode);
        assert_eq!(project.name, "MyApp");
        // Workspace should take priority
        assert!(project.path.ends_with("MyApp.xcworkspace"));
    }

    #[test]
    fn test_detect_direct_xcworkspace_path() {
        let dir = create_test_dir();
        let workspace_path = dir.path().join("MyApp.xcworkspace");
        std::fs::create_dir(&workspace_path).unwrap();

        // Direct path to workspace
        let result = detect_project_from_path(&workspace_path);
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Xcode);
        assert_eq!(project.name, "MyApp");
    }

    #[test]
    fn test_detect_direct_xcodeproj_path() {
        let dir = create_test_dir();
        let proj_path = dir.path().join("MyApp.xcodeproj");
        std::fs::create_dir(&proj_path).unwrap();

        let result = detect_project_from_path(&proj_path);
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Xcode);
        assert_eq!(project.name, "MyApp");
    }

    #[test]
    fn test_detect_direct_gradle_path() {
        let dir = create_test_dir();
        let gradle_path = dir.path().join("build.gradle");
        std::fs::write(&gradle_path, "// gradle").unwrap();

        let result = detect_project_from_path(&gradle_path);
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Android);
    }

    #[test]
    fn test_is_project_path() {
        assert!(is_project_path(Path::new("/path/to/MyApp.xcworkspace")));
        assert!(is_project_path(Path::new("/path/to/MyApp.xcodeproj")));
        assert!(is_project_path(Path::new("/path/to/build.gradle")));
        assert!(is_project_path(Path::new("/path/to/build.gradle.kts")));
        assert!(!is_project_path(Path::new("/path/to/some/directory")));
        assert!(!is_project_path(Path::new("/path/to/file.txt")));
    }

    #[test]
    fn test_project_type_from_path() {
        assert_eq!(
            ProjectType::from_path(Path::new("/path/MyApp.xcworkspace")),
            Some(ProjectType::Xcode)
        );
        assert_eq!(
            ProjectType::from_path(Path::new("/path/MyApp.xcodeproj")),
            Some(ProjectType::Xcode)
        );
        assert_eq!(
            ProjectType::from_path(Path::new("/path/build.gradle")),
            Some(ProjectType::Android)
        );
        assert_eq!(
            ProjectType::from_path(Path::new("/path/build.gradle.kts")),
            Some(ProjectType::Android)
        );
        assert_eq!(ProjectType::from_path(Path::new("/path/other")), None);
    }
}
