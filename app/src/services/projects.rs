use crate::db::entity::projects;
use sea_orm::{entity::*, query::*, DatabaseConnection};
use serde::Serialize;
use std::path::Path;

// Re-export ProjectType so other modules can access it
pub use crate::db::entity::projects::ProjectType;

#[derive(Debug, Serialize)]
pub struct Project {
    pub path: String,
    pub name: String,
    #[serde(rename = "type")]
    pub project_type: ProjectType,
    pub valid: bool,
}

/// Detect project from a path
pub fn detect_project(path: &Path) -> Option<Project> {
    // If the path itself is a project file/bundle, use it directly
    if is_project_path(path) {
        detect_from_project_path(path)
    } else if path.is_dir() {
        // Search directory for project files
        detect_from_directory(path)
    } else {
        None
    }
}

/// Check if a path points directly to a project file/bundle
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
fn detect_from_project_path(path: &Path) -> Option<Project> {
    let file_name = path.file_name()?.to_string_lossy();

    // Xcode workspace
    if file_name.ends_with(".xcworkspace") {
        let name = file_name.trim_end_matches(".xcworkspace").to_string();
        return Some(Project {
            project_type: ProjectType::Xcode,
            name,
            path: path.to_string_lossy().to_string(),
            valid: path.exists(),
        });
    }

    // Xcode project
    if file_name.ends_with(".xcodeproj") {
        let name = file_name.trim_end_matches(".xcodeproj").to_string();
        return Some(Project {
            project_type: ProjectType::Xcode,
            name,
            path: path.to_string_lossy().to_string(),
            valid: path.exists(),
        });
    }

    // Android Gradle build file
    if file_name == "build.gradle" || file_name == "build.gradle.kts" {
        let name = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        return Some(Project {
            project_type: ProjectType::Android,
            name,
            path: path.to_string_lossy().to_string(),
            valid: path.exists(),
        });
    }

    None
}

/// Detect project by searching a directory
fn detect_from_directory(path: &Path) -> Option<Project> {
    let entries = std::fs::read_dir(path).ok()?;

    // First pass: look for workspace (takes priority)
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str.ends_with(".xcworkspace") {
            let name = file_name_str.trim_end_matches(".xcworkspace").to_string();
            let project_path = entry.path();
            return Some(Project {
                project_type: ProjectType::Xcode,
                name,
                path: project_path.to_string_lossy().to_string(),
                valid: project_path.exists(),
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
            let project_path = entry.path();
            return Some(Project {
                project_type: ProjectType::Xcode,
                name,
                path: project_path.to_string_lossy().to_string(),
                valid: project_path.exists(),
            });
        }

        if file_name_str == "build.gradle" || file_name_str == "build.gradle.kts" {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let project_path = entry.path();
            return Some(Project {
                project_type: ProjectType::Android,
                name,
                path: project_path.to_string_lossy().to_string(),
                valid: project_path.exists(),
            });
        }
    }

    None
}

/// Save or update a project in the database
pub async fn save_project(
    db: &DatabaseConnection,
    path: &str,
    name: &str,
) -> Result<(), sea_orm::DbErr> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Try to find existing project
    let existing = projects::Entity::find()
        .filter(projects::Column::Path.eq(path))
        .one(db)
        .await?;

    match existing {
        Some(existing_project) => {
            // Update existing project
            let mut active: projects::ActiveModel = existing_project.into();
            active.name = Set(name.to_string());
            active.last_opened_at = Set(Some(now));
            active.update(db).await?;
        }
        None => {
            // Insert new project
            let new_project = projects::ActiveModel {
                id: NotSet,
                path: Set(path.to_string()),
                name: Set(name.to_string()),
                last_opened_at: Set(Some(now.clone())),
                created_at: Set(Some(now)),
            };
            projects::Entity::insert(new_project).exec(db).await?;
        }
    }

    Ok(())
}

/// Get recent projects from the database
pub async fn get_recent_projects(
    db: &DatabaseConnection,
    query: Option<&str>,
    limit: u64,
) -> Result<Vec<Project>, sea_orm::DbErr> {
    let mut select = projects::Entity::find()
        .order_by_desc(projects::Column::LastOpenedAt)
        .limit(limit);

    if let Some(search) = query {
        select = select.filter(projects::Column::Path.contains(search));
    }

    let projects = select.all(db).await?;

    let validated: Vec<Project> = projects
        .into_iter()
        .filter_map(|p| {
            let project_type = p.project_type()?;
            let path = Path::new(&p.path);
            let valid = path.exists();
            Some(Project {
                path: p.path,
                name: p.name,
                project_type,
                valid,
            })
        })
        .collect();

    Ok(validated)
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

        let result = detect_project(dir.path());
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

        let result = detect_project(dir.path());
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Xcode);
        assert_eq!(project.name, "MyWorkspace");
        assert!(project.path.ends_with("MyWorkspace.xcworkspace"));
    }

    #[test]
    fn test_workspace_takes_priority_over_project() {
        let dir = create_test_dir();
        std::fs::create_dir(dir.path().join("MyApp.xcodeproj")).unwrap();
        std::fs::create_dir(dir.path().join("MyApp.xcworkspace")).unwrap();

        let result = detect_project(dir.path());
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.name, "MyApp");
        assert!(project.path.ends_with("MyApp.xcworkspace"));
    }

    #[test]
    fn test_detect_direct_xcworkspace_path() {
        let dir = create_test_dir();
        let workspace_path = dir.path().join("MyApp.xcworkspace");
        std::fs::create_dir(&workspace_path).unwrap();

        let result = detect_project(&workspace_path);
        assert!(result.is_some());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Xcode);
        assert_eq!(project.name, "MyApp");
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
}
