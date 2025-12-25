use crate::services::projects;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("No Xcode project found at path")]
    ProjectNotFound,

    #[error("Not an Xcode project: {0:?}")]
    NotXcodeProject(projects::ProjectType),

    #[error("Failed to execute xcodebuild: {0}")]
    XcodebuildExecution(#[from] std::io::Error),

    #[error("xcodebuild failed: {0}")]
    XcodebuildFailed(String),

    #[error("Failed to parse xcodebuild output: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("No project/workspace info in xcodebuild output")]
    MissingProjectInfo,
}

impl DiscoveryError {
    /// Convert error to user-friendly string for HTTP responses
    pub fn to_user_message(&self) -> String {
        self.to_string()
    }
}

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
pub async fn discover_project(path: &Path) -> Result<XcodeProject, DiscoveryError> {
    // Use the services layer to detect the project
    let project = projects::detect_project(path).ok_or(DiscoveryError::ProjectNotFound)?;

    // Ensure it's an Xcode project
    if !matches!(project.project_type, projects::ProjectType::Xcode) {
        return Err(DiscoveryError::NotXcodeProject(project.project_type));
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
            .await?,
        ProjectType::Project => Command::new("xcodebuild")
            .arg("-project")
            .arg(&project.path)
            .arg("-list")
            .arg("-json")
            .output()
            .await?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DiscoveryError::XcodebuildFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let build_list: XcodeBuildList = serde_json::from_str(&stdout)?;

    let info = match project_type {
        ProjectType::Workspace => build_list.workspace,
        ProjectType::Project => build_list.project,
    }
    .ok_or(DiscoveryError::MissingProjectInfo)?;

    Ok(XcodeProject {
        path: project.path,
        project_type,
        schemes: info.schemes,
        targets: info.targets,
        configurations: info.configurations,
    })
}
