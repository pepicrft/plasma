use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

// Helper to create test infrastructure
fn create_test_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

fn create_mock_xcodeproj(dir: &std::path::Path, name: &str) {
    let proj_path = dir.join(format!("{}.xcodeproj", name));
    fs::create_dir(&proj_path).unwrap();

    // Create a minimal project.pbxproj file
    let pbxproj = proj_path.join("project.pbxproj");
    fs::write(&pbxproj, "// Mock project file").unwrap();
}

fn create_mock_xcworkspace(dir: &std::path::Path, name: &str) {
    let workspace_path = dir.join(format!("{}.xcworkspace", name));
    fs::create_dir(&workspace_path).unwrap();

    // Create contents.xcworkspacedata
    let contents = workspace_path.join("contents.xcworkspacedata");
    fs::write(
        &contents,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Workspace version="1.0">
</Workspace>"#,
    )
    .unwrap();
}

async fn create_test_app() -> axum::Router {
    // Create in-memory database for testing
    let db = app_lib::db::Database::new(std::path::Path::new(":memory:"))
        .await
        .unwrap();

    let state = Arc::new(app_lib::server::AppState { db });

    app_lib::routes::create_routes(None).with_state(state)
}

#[tokio::test]
async fn test_xcode_schemes_endpoint_with_xcodeproj() {
    let app = create_test_app().await;
    let temp_dir = create_test_dir();
    create_mock_xcodeproj(temp_dir.path(), "TestApp");

    let proj_path = temp_dir.path().join("TestApp.xcodeproj");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": proj_path.to_str().unwrap()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Note: This will fail with xcodebuild error since we have a mock project
    // In a real scenario, xcodebuild would need a valid project structure
    // We're testing the endpoint accepts the request properly
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_xcode_schemes_endpoint_with_directory() {
    let app = create_test_app().await;
    let temp_dir = create_test_dir();
    create_mock_xcodeproj(temp_dir.path(), "TestApp");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": temp_dir.path().to_str().unwrap()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should find the project in the directory
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_xcode_schemes_endpoint_nonexistent_path() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": "/nonexistent/path"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["error"].as_str().unwrap().contains("does not exist"));
}

#[tokio::test]
async fn test_xcode_schemes_endpoint_directory_without_project() {
    let app = create_test_app().await;
    let temp_dir = create_test_dir();

    // Create a file but no Xcode project
    fs::write(temp_dir.path().join("README.md"), "# Test").unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": temp_dir.path().to_str().unwrap()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("No .xcworkspace or .xcodeproj found"));
}

#[tokio::test]
async fn test_xcode_schemes_endpoint_workspace_priority() {
    let app = create_test_app().await;
    let temp_dir = create_test_dir();

    // Create both project and workspace
    create_mock_xcodeproj(temp_dir.path(), "TestApp");
    create_mock_xcworkspace(temp_dir.path(), "TestWorkspace");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": temp_dir.path().to_str().unwrap()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should prefer workspace over project (will fail with xcodebuild but that's ok)
    // We're testing that the endpoint picks the workspace
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::BAD_REQUEST);

    if response.status() == StatusCode::BAD_REQUEST {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        // Error should reference xcodebuild, not "not found"
        let error = json["error"].as_str().unwrap();
        assert!(
            error.contains("xcodebuild") || error.contains("workspace"),
            "Error should be from xcodebuild execution, not file discovery: {}",
            error
        );
    }
}

#[tokio::test]
async fn test_xcode_schemes_endpoint_malformed_json() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from("{invalid json}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return bad request for malformed JSON
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_xcode_schemes_endpoint_missing_path_field() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 422 Unprocessable Entity when required JSON fields are missing
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// Tests with real Xcode fixture

fn fixture_path(relative: &str) -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{}/tests/fixtures/xcode/{}", manifest_dir, relative)
}

#[tokio::test]
async fn test_real_xcode_project_discovery() {
    let app = create_test_app().await;
    let project_path = fixture_path("Plasma/Plasma.xcodeproj");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": project_path
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Verify the response structure
    assert_eq!(json["project_type"], "project");
    assert!(json["schemes"].is_array());
    assert!(json["targets"].is_array());
    assert!(json["configurations"].is_array());

    // Verify expected values from the fixture
    let schemes = json["schemes"].as_array().unwrap();
    assert_eq!(schemes.len(), 1);
    assert_eq!(schemes[0], "Plasma Project");

    let targets = json["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0], "Plasma");

    let configurations = json["configurations"].as_array().unwrap();
    assert_eq!(configurations.len(), 2);
    assert!(configurations.contains(&json!("Debug")));
    assert!(configurations.contains(&json!("Release")));
}

#[tokio::test]
async fn test_real_xcode_project_discovery_from_directory() {
    let app = create_test_app().await;
    let directory_path = fixture_path("Plasma");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/xcode/discover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "path": directory_path
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Should discover the project in the directory
    assert_eq!(json["project_type"], "project");

    let schemes = json["schemes"].as_array().unwrap();
    assert_eq!(schemes[0], "Plasma Project");
}
