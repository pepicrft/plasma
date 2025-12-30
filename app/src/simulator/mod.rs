use axum::{
    body::Body,
    extract::Query,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response, sse::{Event, KeepAlive, Sse}},
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::process::Command;
use tokio::sync::broadcast;
use tracing::{debug, error, info};
use std::convert::Infallible;
use futures::stream::Stream;
use once_cell::sync::Lazy;

#[derive(Deserialize)]
pub struct StreamQuery {
    pub udid: String,
    pub fps: Option<u32>,
    pub quality: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum StreamLogEvent {
    #[serde(rename = "info")]
    Info { message: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "debug")]
    Debug { message: String },
    #[serde(rename = "frame")]
    Frame { frame_number: u64 },
}

// Global broadcast channel for log events - allows multiple listeners
static STREAM_LOG_SENDER: Lazy<broadcast::Sender<StreamLogEvent>> = Lazy::new(|| {
    let (tx, _) = broadcast::channel(256);
    tx
});

pub async fn stream_simulator(Query(query): Query<StreamQuery>) -> Response {
    let log_tx = STREAM_LOG_SENDER.clone();

    // Try plasma-stream first (fast IOSurface-based), fallback to axe (screenshot-based)
    let streamer = find_plasma_stream_binary()
        .map(|p| ("plasma-stream", p))
        .or_else(|| find_axe_binary().map(|p| ("axe", p)));

    let Some((streamer_name, streamer_path)) = streamer else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "No streaming binary found (plasma-stream or axe)").into_response();
    };

    let fps = query.fps
        .or_else(|| {
            std::env::var("PLASMA_STREAM_FPS")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
        })
        .unwrap_or(30)
        .min(60);

    let quality = query.quality
        .or_else(|| {
            std::env::var("PLASMA_STREAM_QUALITY")
                .ok()
                .and_then(|value| value.parse::<f32>().ok())
        })
        .unwrap_or(0.6)
        .clamp(0.1, 1.0);

    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("Starting {} stream for simulator {}", streamer_name, query.udid),
    });
    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("Found {} at: {}", streamer_name, streamer_path.display()),
    });
    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("Using FPS: {}, Quality: {}", fps, quality),
    });

    // Build command based on which streamer we're using
    let mut cmd = Command::new(&streamer_path);

    if streamer_name == "plasma-stream" {
        // plasma-stream uses IOSurface for fast native streaming
        cmd.args([
            "--udid", &query.udid,
            "--fps", &fps.to_string(),
            "--quality", &quality.to_string(),
        ]);
    } else {
        // axe uses screenshot polling (slower fallback)
        let axe_quality = ((quality * 100.0) as u32).min(80); // Cap at 80 to avoid PNG output
        cmd.args([
            "stream-video",
            "--udid", &query.udid,
            "--format", "mjpeg",
            "--fps", &fps.to_string(),
            "--quality", &axe_quality.to_string(),
        ]);
    }

    cmd.stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());

    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("Spawning: {} with args for UDID {}", streamer_name, query.udid),
    });

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = log_tx.send(StreamLogEvent::Error {
                message: format!("Failed to spawn {}: {}", streamer_name, e),
            });
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to spawn {}: {}", streamer_name, e)).into_response();
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = log_tx.send(StreamLogEvent::Error {
                message: format!("Failed to capture {} stdout", streamer_name),
            });
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to capture {} stdout", streamer_name)).into_response();
        }
    };

    // Log stderr in background
    let streamer_name_for_stderr = streamer_name.to_string();
    if let Some(stderr) = child.stderr.take() {
        let log_tx_stderr = log_tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    let _ = log_tx_stderr.send(StreamLogEvent::Debug {
                        message: format!("{} stderr: {}", streamer_name_for_stderr, line),
                    });
                }
            }
        });
    }

    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("{} process spawned, proxying output...", streamer_name),
    });

    // Skip the HTTP headers from the streamer, then proxy the multipart body
    let mut reader = tokio::io::BufReader::new(stdout);
    let stream = async_stream::stream! {
        use tokio::io::AsyncBufReadExt;

        // Skip any intro text until we hit the HTTP response line
        let mut found_http = false;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim_end();
                    let _ = log_tx.send(StreamLogEvent::Debug {
                        message: format!("streamer output: {}", trimmed),
                    });
                    if trimmed.starts_with("HTTP/1.1") {
                        found_http = true;
                        // Skip the rest of the HTTP headers until empty line
                        loop {
                            let mut header_line = String::new();
                            match reader.read_line(&mut header_line).await {
                                Ok(0) => break, // EOF
                                Ok(_) => {
                                    let trimmed_header = header_line.trim_end();
                                    let _ = log_tx.send(StreamLogEvent::Debug {
                                        message: format!("streamer header: {}", trimmed_header),
                                    });
                                    if trimmed_header.is_empty() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        if !found_http {
            let _ = log_tx.send(StreamLogEvent::Error {
                message: "Never found HTTP response from streamer".to_string(),
            });
            let _ = child.kill().await;
            return;
        }

        let _ = log_tx.send(StreamLogEvent::Info {
            message: "HTTP headers skipped, streaming multipart data...".to_string(),
        });

        // Proxy the multipart body directly
        let mut buf = vec![0u8; 65536];
        loop {
            use tokio::io::AsyncReadExt;
            match reader.read(&mut buf).await {
                Ok(0) => {
                    let _ = log_tx.send(StreamLogEvent::Info {
                        message: "Stream EOF".to_string(),
                    });
                    break;
                }
                Ok(n) => {
                    yield Ok::<_, std::convert::Infallible>(axum::body::Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => {
                    let _ = log_tx.send(StreamLogEvent::Error {
                        message: format!("Read error: {}", e),
                    });
                    break;
                }
            }
        }

        let _ = child.kill().await;
    };

    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("multipart/x-mixed-replace; boundary=--mjpegstream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// SSE endpoint for streaming logs
pub async fn stream_logs() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = STREAM_LOG_SENDER.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    yield Ok(Event::default().data(json));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    let event = StreamLogEvent::Debug {
                        message: format!("Skipped {} log messages due to buffer overflow", n)
                    };
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    yield Ok(Event::default().data(json));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}


/// Find the plasma-stream binary (fast IOSurface-based streaming)
fn find_plasma_stream_binary() -> Option<PathBuf> {
    // 1. Environment variable override
    if let Ok(path) = std::env::var("PLASMA_STREAM") {
        let candidate = PathBuf::from(&path);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 2. Bundled binary in app/bin (for development)
    let dev_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin").join("plasma-stream");
    if dev_bin.exists() {
        return Some(dev_bin);
    }

    // 3. Bundled binary in app resources (for release builds)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(macos_dir) = exe_path.parent() {
            let resources_dir = macos_dir.parent().map(|p| p.join("Resources"));
            if let Some(resources) = resources_dir {
                let bundled = resources.join("binaries").join("plasma-stream");
                if bundled.exists() {
                    return Some(bundled);
                }
            }
        }
    }

    None
}

/// Find the axe binary (screenshot-based streaming fallback)
fn find_axe_binary() -> Option<PathBuf> {
    // 1. Environment variable override
    if let Ok(path) = std::env::var("PLASMA_AXE") {
        let candidate = PathBuf::from(&path);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 2. Bundled binary in app resources (for release builds)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(macos_dir) = exe_path.parent() {
            let resources_dir = macos_dir.parent().map(|p| p.join("Resources"));
            if let Some(resources) = resources_dir {
                let bundled_axe = resources.join("binaries").join("axe");
                if bundled_axe.exists() {
                    return Some(bundled_axe);
                }
            }
        }
    }

    // 3. Standard locations
    for candidate in ["/opt/homebrew/bin/axe", "/usr/local/bin/axe"] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    // 4. Search PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for entry in std::env::split_paths(&path_var) {
            let candidate = entry.join("axe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

// --- Simulator listing and launching ---

#[derive(Debug, Serialize, Clone)]
pub struct Simulator {
    pub udid: String,
    pub name: String,
    pub state: String,
    pub runtime: String,
}

#[derive(Debug, Serialize)]
pub struct SimulatorListResponse {
    pub simulators: Vec<Simulator>,
}

/// List all available iOS simulators using `xcrun simctl list devices`
pub async fn list_simulators() -> impl IntoResponse {
    match get_simulators().await {
        Ok(simulators) => Json(SimulatorListResponse { simulators }).into_response(),
        Err(e) => {
            error!("Failed to list simulators: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

async fn get_simulators() -> Result<Vec<Simulator>, String> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "-j"])
        .output()
        .await
        .map_err(|e| format!("Failed to run simctl: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "simctl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse simctl output: {}", e))?;

    let mut simulators = Vec::new();

    if let Some(devices) = json.get("devices").and_then(|d| d.as_object()) {
        for (runtime, device_list) in devices {
            if let Some(arr) = device_list.as_array() {
                for device in arr {
                    let udid = device.get("udid").and_then(|v| v.as_str()).unwrap_or("");
                    let name = device.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let state = device.get("state").and_then(|v| v.as_str()).unwrap_or("");

                    if !udid.is_empty() && state != "Unavailable" {
                        simulators.push(Simulator {
                            udid: udid.to_string(),
                            name: name.to_string(),
                            state: state.to_string(),
                            runtime: runtime.clone(),
                        });
                    }
                }
            }
        }
    }

    // Sort by state (Booted first) then by name
    simulators.sort_by(|a, b| {
        let a_booted = a.state == "Booted";
        let b_booted = b.state == "Booted";
        match (a_booted, b_booted) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    Ok(simulators)
}

#[derive(Debug, Deserialize)]
pub struct InstallAndLaunchRequest {
    pub udid: String,
    pub app_path: String,
    pub bundle_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallAndLaunchResponse {
    pub success: bool,
    pub message: String,
}

/// Boot simulator, install app, and launch it
pub async fn install_and_launch(
    Json(request): Json<InstallAndLaunchRequest>,
) -> impl IntoResponse {
    match do_install_and_launch(&request.udid, &request.app_path, request.bundle_id.as_deref()).await
    {
        Ok(msg) => Json(InstallAndLaunchResponse {
            success: true,
            message: msg,
        })
        .into_response(),
        Err(e) => {
            error!("Failed to install and launch: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(InstallAndLaunchResponse {
                    success: false,
                    message: e,
                }),
            )
                .into_response()
        }
    }
}

async fn do_install_and_launch(
    udid: &str,
    app_path: &str,
    bundle_id: Option<&str>,
) -> Result<String, String> {
    // Boot simulator if not already booted
    info!("Booting simulator {}...", udid);
    let boot_output = Command::new("xcrun")
        .args(["simctl", "boot", udid])
        .output()
        .await
        .map_err(|e| format!("Failed to boot simulator: {}", e))?;

    // Ignore error if already booted
    if !boot_output.status.success() {
        let stderr = String::from_utf8_lossy(&boot_output.stderr);
        if !stderr.contains("current state: Booted") {
            debug!("Boot warning (may already be booted): {}", stderr);
        }
    }

    // Install the app
    info!("Installing app at {}...", app_path);
    let install_output = Command::new("xcrun")
        .args(["simctl", "install", udid, app_path])
        .output()
        .await
        .map_err(|e| format!("Failed to install app: {}", e))?;

    if !install_output.status.success() {
        return Err(format!(
            "Install failed: {}",
            String::from_utf8_lossy(&install_output.stderr)
        ));
    }

    // Extract bundle ID from app if not provided
    let bundle_id = match bundle_id {
        Some(id) => id.to_string(),
        None => extract_bundle_id(app_path)?,
    };

    // Launch the app
    info!("Launching app with bundle ID {}...", bundle_id);
    let launch_output = Command::new("xcrun")
        .args(["simctl", "launch", udid, &bundle_id])
        .output()
        .await
        .map_err(|e| format!("Failed to launch app: {}", e))?;

    if !launch_output.status.success() {
        return Err(format!(
            "Launch failed: {}",
            String::from_utf8_lossy(&launch_output.stderr)
        ));
    }

    Ok(format!("App {} launched successfully", bundle_id))
}

fn extract_bundle_id(app_path: &str) -> Result<String, String> {
    let plist_path = PathBuf::from(app_path).join("Info.plist");

    // Use PlistBuddy to read the bundle identifier
    let output = std::process::Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier", plist_path.to_str().unwrap()])
        .output()
        .map_err(|e| format!("Failed to read bundle ID: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to read bundle ID: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
