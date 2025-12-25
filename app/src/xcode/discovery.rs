use crate::services::projects;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct XcodeProject {
    pub path: String,
    pub project_type: ProjectType,
    pub schemes: Vec<String>,
    pub targets: Vec<String>,
    pub configurations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// Discover Xcode project details including schemes, targets, and configurations
pub async fn discover_project(path: &Path) -> Result<XcodeProject, String> {
    // Use the services layer to detect the project
    let project = projects::detect_project(path)
        .ok_or_else(|| "No Xcode project found".to_string())?;

    // Ensure it's an Xcode project
    if !matches!(project.project_type, projects::ProjectType::Xcode) {
        return Err(format!("Not an Xcode project: {:?}", project.project_type));
    }

    // Determine if it's a workspace or project based on the path extension
    let project_type = if project.path.ends_with(".xcworkspace") {
        ProjectType::Workspace
    } else {
        ProjectType::Project
    };

    // Run xcodebuild to get project details
    let output = match project_type {
        ProjectType::Workspace => Command::new("xcodebuild")
            .arg("-workspace")
            .arg(&project.path)
            .arg("-list")
            .arg("-json")
            .output()
            .await
            .map_err(|e| format!("Failed to execute xcodebuild: {}", e))?,
        ProjectType::Project => Command::new("xcodebuild")
            .arg("-project")
            .arg(&project.path)
            .arg("-list")
            .arg("-json")
            .output()
            .await
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
        path: project.path,
        project_type,
        schemes: info.schemes,
        targets: info.targets,
        configurations: info.configurations,
    })
}
