//! Plasma Xcode Integration
//!
//! This crate provides functionality for working with Xcode projects, simulators,
//! and building iOS apps.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

/// Errors that can occur when working with Xcode
#[derive(Error, Debug)]
pub enum XcodeError {
    #[error("Xcode not found at path: {0}")]
    XcodeNotFound(String),

    #[error("Project not found at path: {0}")]
    ProjectNotFound(PathBuf),

    #[error("Invalid Xcode project: {0}")]
    InvalidProject(String),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Simulator error: {0}")]
    SimulatorError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Information about an Xcode project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcodeProject {
    /// Path to the project
    pub path: PathBuf,
    /// Project name
    pub name: String,
    /// Type of project (workspace vs project)
    pub project_type: ProjectType,
    /// Available schemes
    pub schemes: Vec<String>,
    /// Available configurations
    pub configurations: Vec<String>,
}

/// Type of Xcode project container
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    /// .xcworkspace file
    Workspace,
    /// .xcodeproj file
    Project,
}

/// Result of building a scheme
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    /// Whether the build succeeded
    pub success: bool,
    /// Path to build directory
    pub build_dir: PathBuf,
    /// Built products (.app bundles)
    pub products: Vec<Product>,
}

/// A built product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    /// Product name
    pub name: String,
    /// Path to the .app bundle
    pub path: PathBuf,
}

/// Information about an iOS Simulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Simulator {
    /// Unique device identifier
    pub udid: String,
    /// Device name
    pub name: String,
    /// Runtime (e.g., "iOS 17.0")
    pub runtime: String,
    /// Current state
    pub state: String,
    /// Whether the simulator is available
    pub is_available: bool,
}

#[derive(Debug, Deserialize)]
struct XcodeBuildList {
    project: Option<XcodeBuildProjectInfo>,
    workspace: Option<XcodeBuildProjectInfo>,
}

#[derive(Debug, Deserialize)]
struct XcodeBuildProjectInfo {
    #[serde(default)]
    configurations: Vec<String>,
    #[serde(default)]
    schemes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SimctlListOutput {
    devices: HashMap<String, Vec<SimctlDevice>>,
}

#[derive(Debug, Deserialize)]
struct SimctlDevice {
    udid: String,
    name: String,
    state: String,
    #[serde(rename = "isAvailable")]
    is_available: bool,
}

/// Discover an Xcode project at the given path
pub fn discover_project(path: &Path) -> Result<XcodeProject, XcodeError> {
    if !path.exists() {
        return Err(XcodeError::ProjectNotFound(path.to_path_buf()));
    }

    // Check if it's a workspace or project
    let (project_path, project_type) = if path.extension().map_or(false, |e| e == "xcworkspace") {
        (path.to_path_buf(), ProjectType::Workspace)
    } else if path.extension().map_or(false, |e| e == "xcodeproj") {
        (path.to_path_buf(), ProjectType::Project)
    } else {
        // Look for .xcworkspace or .xcodeproj in the directory
        let workspace = find_file_with_extension(path, "xcworkspace");
        let project = find_file_with_extension(path, "xcodeproj");

        if let Some(ws) = workspace {
            (ws, ProjectType::Workspace)
        } else if let Some(proj) = project {
            (proj, ProjectType::Project)
        } else {
            return Err(XcodeError::InvalidProject("No Xcode project found".to_string()));
        }
    };

    // Run xcodebuild -list to get schemes
    let mut cmd = Command::new("xcodebuild");
    match project_type {
        ProjectType::Workspace => cmd.arg("-workspace"),
        ProjectType::Project => cmd.arg("-project"),
    };
    cmd.arg(&project_path).arg("-list").arg("-json");

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XcodeError::InvalidProject(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let build_list: XcodeBuildList = serde_json::from_str(&stdout)?;

    let info = match project_type {
        ProjectType::Workspace => build_list.workspace,
        ProjectType::Project => build_list.project,
    }
    .ok_or_else(|| XcodeError::InvalidProject("No project info found".to_string()))?;

    let name = project_path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    Ok(XcodeProject {
        path: project_path,
        name,
        project_type,
        schemes: info.schemes,
        configurations: info.configurations,
    })
}

fn find_file_with_extension(dir: &Path, ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == ext) {
            Some(path)
        } else {
            None
        }
    })
}

/// List all available iOS simulators
pub fn list_simulators() -> Result<Vec<Simulator>, XcodeError> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "available", "--json"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XcodeError::SimulatorError(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let simctl_output: SimctlListOutput = serde_json::from_str(&stdout)?;

    let mut simulators = Vec::new();

    for (runtime, devices) in simctl_output.devices {
        // Only include iOS simulators
        if !runtime.contains("iOS") {
            continue;
        }

        for device in devices {
            simulators.push(Simulator {
                udid: device.udid,
                name: device.name,
                runtime: runtime.clone(),
                state: device.state,
                is_available: device.is_available,
            });
        }
    }

    Ok(simulators)
}

/// Find the first available iOS simulator (prioritizes iPhone models)
pub fn find_default_simulator() -> Result<Simulator, XcodeError> {
    let simulators = list_simulators()?;

    // Try to find an iPhone simulator first
    for name_prefix in ["iPhone 16", "iPhone 15", "iPhone 14", "iPhone"] {
        if let Some(sim) = simulators.iter().find(|s| s.name.starts_with(name_prefix)) {
            return Ok(sim.clone());
        }
    }

    // Fall back to any available iOS simulator
    simulators
        .first()
        .cloned()
        .ok_or_else(|| XcodeError::SimulatorError("No iOS simulators found".to_string()))
}

/// Boot a simulator
pub fn boot_simulator(udid: &str) -> Result<(), XcodeError> {
    // Check if already booted
    let simulators = list_simulators()?;
    if let Some(sim) = simulators.iter().find(|s| s.udid == udid) {
        if sim.state == "Booted" {
            return Ok(());
        }
    }

    let output = Command::new("xcrun")
        .args(["simctl", "boot", udid])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "already booted" error
        if !stderr.contains("current state: Booted") {
            return Err(XcodeError::SimulatorError(stderr.to_string()));
        }
    }

    Ok(())
}

/// Install an app to a simulator
pub fn install_app(udid: &str, app_path: &Path) -> Result<(), XcodeError> {
    let output = Command::new("xcrun")
        .args(["simctl", "install", udid, app_path.to_str().unwrap_or("")])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XcodeError::SimulatorError(stderr.to_string()));
    }

    Ok(())
}

/// Launch an app on a simulator
pub fn launch_app(udid: &str, bundle_id: &str) -> Result<String, XcodeError> {
    let output = Command::new("xcrun")
        .args(["simctl", "launch", udid, bundle_id])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XcodeError::SimulatorError(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().to_string())
}

/// Build an Xcode scheme for iOS Simulator
pub fn build_scheme(project: &XcodeProject, scheme: &str) -> Result<BuildResult, XcodeError> {
    // First get build settings to find the build directory
    let mut settings_cmd = Command::new("xcodebuild");
    match project.project_type {
        ProjectType::Workspace => settings_cmd.arg("-workspace"),
        ProjectType::Project => settings_cmd.arg("-project"),
    };
    settings_cmd
        .arg(&project.path)
        .arg("-scheme")
        .arg(scheme)
        .arg("-configuration")
        .arg("Debug")
        .arg("-sdk")
        .arg("iphonesimulator")
        .arg("-destination")
        .arg("generic/platform=iOS Simulator")
        .arg("CODE_SIGN_IDENTITY=")
        .arg("CODE_SIGNING_REQUIRED=NO")
        .arg("CODE_SIGNING_ALLOWED=NO")
        .arg("-showBuildSettings");

    let settings_output = settings_cmd.output()?;
    let settings_stdout = String::from_utf8_lossy(&settings_output.stdout);

    let build_dir = extract_build_dir(&settings_stdout)
        .ok_or_else(|| XcodeError::BuildFailed("Could not determine build directory".to_string()))?;

    // Now run the actual build
    let mut build_cmd = Command::new("xcodebuild");
    match project.project_type {
        ProjectType::Workspace => build_cmd.arg("-workspace"),
        ProjectType::Project => build_cmd.arg("-project"),
    };
    build_cmd
        .arg(&project.path)
        .arg("-scheme")
        .arg(scheme)
        .arg("-configuration")
        .arg("Debug")
        .arg("-sdk")
        .arg("iphonesimulator")
        .arg("-destination")
        .arg("generic/platform=iOS Simulator")
        .arg("CODE_SIGN_IDENTITY=")
        .arg("CODE_SIGNING_REQUIRED=NO")
        .arg("CODE_SIGNING_ALLOWED=NO");

    let build_output = build_cmd.output()?;

    if !build_output.status.success() {
        let stderr = String::from_utf8_lossy(&build_output.stderr);
        return Err(XcodeError::BuildFailed(stderr.to_string()));
    }

    // Find build products
    let products = find_build_products(&build_dir)?;

    Ok(BuildResult {
        success: true,
        build_dir: PathBuf::from(&build_dir),
        products,
    })
}

fn extract_build_dir(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("CONFIGURATION_BUILD_DIR = ") {
            return Some(value.to_string());
        }
    }
    None
}

fn find_build_products(build_dir: &str) -> Result<Vec<Product>, XcodeError> {
    let path = PathBuf::from(build_dir);

    if !path.exists() {
        return Ok(vec![]);
    }

    let mut products = Vec::new();

    for entry in std::fs::read_dir(&path)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str.ends_with(".app") {
            products.push(Product {
                name: file_name_str.to_string(),
                path: entry.path(),
            });
        }
    }

    Ok(products)
}

/// Get the bundle ID from an .app bundle
pub fn get_bundle_id(app_path: &Path) -> Result<String, XcodeError> {
    let info_plist = app_path.join("Info.plist");

    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier", info_plist.to_str().unwrap_or("")])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XcodeError::InvalidProject(stderr.to_string()));
    }

    let bundle_id = String::from_utf8_lossy(&output.stdout);
    Ok(bundle_id.trim().to_string())
}
