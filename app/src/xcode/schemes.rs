use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct XcodeProject {
    pub path: PathBuf,
    pub project_type: ProjectType,
    pub schemes: Vec<String>,
    pub targets: Vec<String>,
    pub configurations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ProjectType {
    Project,
    Workspace,
}

#[derive(Debug, Deserialize)]
struct XcodeBuildList {
    project: Option<ProjectInfo>,
    workspace: Option<ProjectInfo>,
}

#[derive(Debug, Deserialize)]
struct ProjectInfo {
    #[serde(default)]
    configurations: Vec<String>,
    #[serde(default)]
    schemes: Vec<String>,
    #[serde(default)]
    targets: Vec<String>,
}

pub fn discover_project(path: &Path) -> Result<XcodeProject, String> {
    let (project_path, project_type) = find_project_or_workspace(path)?;

    let output = match project_type {
        ProjectType::Workspace => Command::new("xcodebuild")
            .arg("-workspace")
            .arg(&project_path)
            .arg("-list")
            .arg("-json")
            .output()
            .map_err(|e| format!("Failed to execute xcodebuild: {}", e))?,
        ProjectType::Project => Command::new("xcodebuild")
            .arg("-project")
            .arg(&project_path)
            .arg("-list")
            .arg("-json")
            .output()
            .map_err(|e| format!("Failed to execute xcodebuild: {}", e))?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("xcodebuild failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let build_list: XcodeBuildList = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse xcodebuild output: {}", e))?;

    let info = match project_type {
        ProjectType::Workspace => build_list.workspace,
        ProjectType::Project => build_list.project,
    }
    .ok_or_else(|| "No project/workspace info in xcodebuild output".to_string())?;

    Ok(XcodeProject {
        path: project_path,
        project_type,
        schemes: info.schemes,
        targets: info.targets,
        configurations: info.configurations,
    })
}

fn find_project_or_workspace(path: &Path) -> Result<(PathBuf, ProjectType), String> {
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }

    // If the path itself is a .xcworkspace or .xcodeproj, use it directly
    if let Some(ext) = path.extension() {
        match ext.to_str() {
            Some("xcworkspace") => return Ok((path.to_path_buf(), ProjectType::Workspace)),
            Some("xcodeproj") => return Ok((path.to_path_buf(), ProjectType::Project)),
            _ => {}
        }
    }

    // Otherwise, search in the directory
    let search_dir = if path.is_dir() {
        path
    } else {
        path.parent()
            .ok_or_else(|| format!("Path has no parent directory: {}", path.display()))?
    };

    // Read directory entries once
    let entries: Vec<PathBuf> = std::fs::read_dir(search_dir)
        .map_err(|e| format!("Failed to read directory: {}", e))?
        .map(|entry_res| {
            entry_res
                .map(|entry| entry.path())
                .map_err(|e| format!("Failed to read entry: {}", e))
        })
        .collect::<Result<_, _>>()?;

    // Prefer workspace over project
    for entry_path in &entries {
        if let Some(ext) = entry_path.extension() {
            if ext == "xcworkspace" {
                return Ok((entry_path.clone(), ProjectType::Workspace));
            }
        }
    }

    // Fall back to project if no workspace found
    for entry_path in &entries {
        if let Some(ext) = entry_path.extension() {
            if ext == "xcodeproj" {
                return Ok((entry_path.clone(), ProjectType::Project));
            }
        }
    }

    Err(format!(
        "No .xcworkspace or .xcodeproj found in {}",
        search_dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    fn create_mock_xcodeproj(dir: &Path, name: &str) {
        let proj_path = dir.join(format!("{}.xcodeproj", name));
        fs::create_dir(&proj_path).unwrap();

        // Create a minimal project.pbxproj file so xcodebuild doesn't fail
        let pbxproj = proj_path.join("project.pbxproj");
        fs::write(&pbxproj, "// Mock project file").unwrap();
    }

    fn create_mock_xcworkspace(dir: &Path, name: &str) {
        let workspace_path = dir.join(format!("{}.xcworkspace", name));
        fs::create_dir(&workspace_path).unwrap();

        // Create contents.xcworkspacedata
        let contents_dir = workspace_path.join("contents.xcworkspacedata");
        fs::write(
            &contents_dir,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Workspace version=\"1.0\"></Workspace>",
        )
        .unwrap();
    }

    #[test]
    fn test_find_xcodeproj_direct_path() {
        let dir = create_test_dir();
        create_mock_xcodeproj(dir.path(), "TestApp");

        let proj_path = dir.path().join("TestApp.xcodeproj");
        let result = find_project_or_workspace(&proj_path);

        assert!(result.is_ok());
        let (path, project_type) = result.unwrap();
        assert_eq!(path, proj_path);
        assert!(matches!(project_type, ProjectType::Project));
    }

    #[test]
    fn test_find_xcworkspace_direct_path() {
        let dir = create_test_dir();
        create_mock_xcworkspace(dir.path(), "TestWorkspace");

        let workspace_path = dir.path().join("TestWorkspace.xcworkspace");
        let result = find_project_or_workspace(&workspace_path);

        assert!(result.is_ok());
        let (path, project_type) = result.unwrap();
        assert_eq!(path, workspace_path);
        assert!(matches!(project_type, ProjectType::Workspace));
    }

    #[test]
    fn test_find_xcodeproj_in_directory() {
        let dir = create_test_dir();
        create_mock_xcodeproj(dir.path(), "TestApp");

        let result = find_project_or_workspace(dir.path());

        assert!(result.is_ok());
        let (path, project_type) = result.unwrap();
        assert!(path.ends_with("TestApp.xcodeproj"));
        assert!(matches!(project_type, ProjectType::Project));
    }

    #[test]
    fn test_find_xcworkspace_in_directory() {
        let dir = create_test_dir();
        create_mock_xcworkspace(dir.path(), "TestWorkspace");

        let result = find_project_or_workspace(dir.path());

        assert!(result.is_ok());
        let (path, project_type) = result.unwrap();
        assert!(path.ends_with("TestWorkspace.xcworkspace"));
        assert!(matches!(project_type, ProjectType::Workspace));
    }

    #[test]
    fn test_workspace_takes_priority_over_project() {
        let dir = create_test_dir();
        create_mock_xcodeproj(dir.path(), "TestApp");
        create_mock_xcworkspace(dir.path(), "TestWorkspace");

        let result = find_project_or_workspace(dir.path());

        assert!(result.is_ok());
        let (path, project_type) = result.unwrap();
        assert!(path.ends_with("TestWorkspace.xcworkspace"));
        assert!(matches!(project_type, ProjectType::Workspace));
    }

    #[test]
    fn test_nonexistent_path() {
        let result = find_project_or_workspace(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_directory_without_xcode_project() {
        let dir = create_test_dir();
        fs::write(dir.path().join("README.md"), "# Test").unwrap();

        let result = find_project_or_workspace(dir.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("No .xcworkspace or .xcodeproj found"));
    }

    #[test]
    fn test_empty_directory() {
        let dir = create_test_dir();

        let result = find_project_or_workspace(dir.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("No .xcworkspace or .xcodeproj found"));
    }
}
