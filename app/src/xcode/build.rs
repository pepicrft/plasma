use crate::services::projects;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("No Xcode project found at path")]
    ProjectNotFound,

    #[error("Not an Xcode project: {0:?}")]
    NotXcodeProject(projects::ProjectType),

    #[error("Failed to execute xcodebuild: {0}")]
    XcodebuildExecution(#[from] std::io::Error),

    #[error("xcodebuild failed: {0}")]
    XcodebuildFailed(String),

    #[error("Failed to parse build output: {0}")]
    ParseError(String),

    #[error("Scheme not found: {0}")]
    SchemeNotFound(String),

    #[error("No build products found")]
    NoBuildProducts,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildResult {
    pub success: bool,
    pub build_dir: String,
    pub products: Vec<BuildProduct>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BuildProduct {
    pub name: String,
    pub path: String,
    pub product_type: ProductType,
    pub is_launchable: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProductType {
    Application,
    Framework,
    StaticLibrary,
    DynamicLibrary,
    Bundle,
    UnitTest,
    UITest,
    AppExtension,
    Unknown,
}

/// Build an Xcode scheme for iOS Simulator with code signing disabled
pub async fn build_scheme(project_path: &Path, scheme: &str) -> Result<BuildResult, BuildError> {
    let project = projects::detect_project(project_path).ok_or(BuildError::ProjectNotFound)?;

    if !matches!(project.project_type, projects::ProjectType::Xcode) {
        return Err(BuildError::NotXcodeProject(project.project_type));
    }

    let is_workspace = project.path.ends_with(".xcworkspace");

    let mut cmd = Command::new("xcodebuild");

    if is_workspace {
        cmd.arg("-workspace").arg(&project.path);
    } else {
        cmd.arg("-project").arg(&project.path);
    }

    cmd.arg("-scheme")
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

    let output = cmd.output().await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Ok(BuildResult {
            success: false,
            build_dir: String::new(),
            products: vec![],
            stdout,
            stderr,
        });
    }

    // Extract build directory from build settings
    let build_dir = extract_build_dir_from_settings(&stdout)
        .ok_or_else(|| BuildError::ParseError("Could not find build directory".to_string()))?;

    // Now run the actual build
    let mut build_cmd = Command::new("xcodebuild");

    if is_workspace {
        build_cmd.arg("-workspace").arg(&project.path);
    } else {
        build_cmd.arg("-project").arg(&project.path);
    }

    build_cmd
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

    let build_output = build_cmd.output().await?;
    let build_stdout = String::from_utf8_lossy(&build_output.stdout).to_string();
    let build_stderr = String::from_utf8_lossy(&build_output.stderr).to_string();

    if !build_output.status.success() {
        return Ok(BuildResult {
            success: false,
            build_dir: String::new(),
            products: vec![],
            stdout: build_stdout,
            stderr: build_stderr,
        });
    }

    let products = find_build_products(&build_dir).await?;

    Ok(BuildResult {
        success: true,
        build_dir,
        products,
        stdout: build_stdout,
        stderr: build_stderr,
    })
}

/// Extract the build directory from xcodebuild -showBuildSettings output
fn extract_build_dir_from_settings(output: &str) -> Option<String> {
    // Look for CONFIGURATION_BUILD_DIR or BUILD_DIR
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("CONFIGURATION_BUILD_DIR = ") {
            return Some(value.to_string());
        }
    }
    None
}

async fn find_build_products(build_dir: &str) -> Result<Vec<BuildProduct>, BuildError> {
    let path = PathBuf::from(build_dir);

    if !path.exists() {
        return Ok(vec![]);
    }

    let mut products = Vec::new();
    let mut entries = tokio::fs::read_dir(&path)
        .await
        .map_err(|e| BuildError::ParseError(e.to_string()))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| BuildError::ParseError(e.to_string()))?
    {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        let (product_type, is_launchable) = if file_name_str.ends_with(".app") {
            (ProductType::Application, true)
        } else if file_name_str.ends_with(".framework") {
            (ProductType::Framework, false)
        } else if file_name_str.ends_with(".a") {
            (ProductType::StaticLibrary, false)
        } else if file_name_str.ends_with(".dylib") {
            (ProductType::DynamicLibrary, false)
        } else if file_name_str.ends_with(".bundle") {
            (ProductType::Bundle, false)
        } else if file_name_str.ends_with(".xctest") {
            if file_name_str.to_lowercase().contains("uitest") {
                (ProductType::UITest, false)
            } else {
                (ProductType::UnitTest, false)
            }
        } else if file_name_str.ends_with(".appex") {
            (ProductType::AppExtension, false)
        } else {
            continue;
        };

        products.push(BuildProduct {
            name: file_name_str.to_string(),
            path: path_str,
            product_type,
            is_launchable,
        });
    }

    Ok(products)
}

/// Get launchable products from a list of build products
pub fn get_launchable_products(products: &[BuildProduct]) -> Vec<BuildProduct> {
    products
        .iter()
        .filter(|p| p.is_launchable)
        .cloned()
        .collect()
}

/// Get launchable products from a build directory
pub async fn get_launchable_products_from_dir(
    build_dir: &str,
) -> Result<Vec<BuildProduct>, BuildError> {
    let all_products = find_build_products(build_dir).await?;
    Ok(get_launchable_products(&all_products))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildEvent {
    Started {
        scheme: String,
        project_path: String,
    },
    Output {
        line: String,
    },
    Completed {
        success: bool,
        build_dir: String,
        products: Vec<BuildProduct>,
    },
    Error {
        message: String,
    },
}

/// Stream build output line by line for live updates
pub async fn build_scheme_stream(
    project_path: &Path,
    scheme: &str,
) -> Result<impl Stream<Item = Result<BuildEvent, BuildError>>, BuildError> {
    let project = projects::detect_project(project_path).ok_or(BuildError::ProjectNotFound)?;

    if !matches!(project.project_type, projects::ProjectType::Xcode) {
        return Err(BuildError::NotXcodeProject(project.project_type));
    }

    let is_workspace = project.path.ends_with(".xcworkspace");
    let scheme_owned = scheme.to_string();
    let project_path_owned = project_path.to_string_lossy().to_string();

    // First, get build settings to find the build directory
    let mut settings_cmd = Command::new("xcodebuild");

    if is_workspace {
        settings_cmd.arg("-workspace").arg(&project.path);
    } else {
        settings_cmd.arg("-project").arg(&project.path);
    }

    settings_cmd
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

    let settings_output = settings_cmd.output().await?;
    let settings_stdout = String::from_utf8_lossy(&settings_output.stdout).to_string();

    let build_dir = extract_build_dir_from_settings(&settings_stdout)
        .ok_or_else(|| BuildError::ParseError("Could not find build directory".to_string()))?;

    // Now start the actual build with streaming
    let mut cmd = Command::new("xcodebuild");

    if is_workspace {
        cmd.arg("-workspace").arg(&project.path);
    } else {
        cmd.arg("-project").arg(&project.path);
    }

    cmd.arg("-scheme")
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BuildError::ParseError("Failed to capture stdout".to_string()))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| BuildError::ParseError("Failed to capture stderr".to_string()))?;

    let stream = async_stream::stream! {
        yield Ok(BuildEvent::Started {
            scheme: scheme_owned.clone(),
            project_path: project_path_owned.clone(),
        });

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let mut stdout_lines = stdout_reader.lines();
        let mut stderr_lines = stderr_reader.lines();

        loop {
            tokio::select! {
                line = stdout_lines.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            yield Ok(BuildEvent::Output { line });
                        }
                        Ok(None) => break,
                        Err(e) => {
                            yield Err(BuildError::ParseError(e.to_string()));
                            break;
                        }
                    }
                }
                line = stderr_lines.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            yield Ok(BuildEvent::Output { line });
                        }
                        Ok(None) => {},
                        Err(e) => {
                            yield Err(BuildError::ParseError(e.to_string()));
                            break;
                        }
                    }
                }
            }
        }

        let status = child.wait().await;

        match status {
            Ok(exit_status) => {
                let success = exit_status.success();
                let products = if success {
                    find_build_products(&build_dir).await.unwrap_or_default()
                } else {
                    vec![]
                };

                yield Ok(BuildEvent::Completed {
                    success,
                    build_dir,
                    products,
                });
            }
            Err(e) => {
                yield Ok(BuildEvent::Error {
                    message: e.to_string(),
                });
            }
        }
    };

    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_launchable_products() {
        let products = vec![
            BuildProduct {
                name: "MyApp.app".to_string(),
                path: "/path/to/MyApp.app".to_string(),
                product_type: ProductType::Application,
                is_launchable: true,
            },
            BuildProduct {
                name: "MyFramework.framework".to_string(),
                path: "/path/to/MyFramework.framework".to_string(),
                product_type: ProductType::Framework,
                is_launchable: false,
            },
            BuildProduct {
                name: "MyTests.xctest".to_string(),
                path: "/path/to/MyTests.xctest".to_string(),
                product_type: ProductType::UnitTest,
                is_launchable: false,
            },
        ];

        let launchable = get_launchable_products(&products);
        assert_eq!(launchable.len(), 1);
        assert_eq!(launchable[0].name, "MyApp.app");
        assert_eq!(launchable[0].product_type, ProductType::Application);
    }

    #[test]
    fn test_get_launchable_products_empty() {
        let products = vec![BuildProduct {
            name: "MyFramework.framework".to_string(),
            path: "/path/to/MyFramework.framework".to_string(),
            product_type: ProductType::Framework,
            is_launchable: false,
        }];

        let launchable = get_launchable_products(&products);
        assert_eq!(launchable.len(), 0);
    }
}
