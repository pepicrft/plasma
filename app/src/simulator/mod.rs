use axum::{
    body::Body,
    extract::Query,
    http::{header, StatusCode},
    response::{IntoResponse, Response, sse::{Event, KeepAlive, Sse}},
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::process::{Command, Child};
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use std::convert::Infallible;
use futures::stream::Stream;
use once_cell::sync::Lazy;
use std::collections::HashMap;

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

// Global simulator session cache - one per UDID
type SessionCache = Mutex<HashMap<String, SimulatorSession>>;
static SESSION_CACHE: Lazy<SessionCache> = Lazy::new(|| Mutex::new(HashMap::new()));

// MARK: - SimulatorSession

/// Represents a persistent connection to a simulator via simulator-server
struct SimulatorSession {
    #[allow(dead_code)]
    udid: String,
    process: Child,
    stream_url: String,
}

impl SimulatorSession {
    /// Start a new simulator-server session
    async fn new(udid: String, fps: u32, quality: f32, log_tx: &broadcast::Sender<StreamLogEvent>) -> Result<Self, String> {
        let simulator_server_path = find_simulator_server_binary()
            .ok_or_else(|| "simulator-server binary not found".to_string())?;

        let _ = log_tx.send(StreamLogEvent::Info {
            message: format!("Spawning simulator-server for {}", udid),
        });

        let mut cmd = Command::new(&simulator_server_path);
        cmd.args([
            "--udid", &udid,
            "--fps", &fps.to_string(),
            "--quality", &quality.to_string(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| format!("Failed to spawn simulator-server: {}", e))?;

        // Read stdout to find "stream_ready <URL>"
        let stdout = child.stdout.take()
            .ok_or_else(|| "Failed to capture simulator-server stdout".to_string())?;

        let log_tx_clone = log_tx.clone();
        let stream_url = Self::read_stream_ready_async(stdout, &log_tx_clone)
            .await
            .map_err(|e| e.to_string())?;

        // Log stderr in background
        if let Some(stderr) = child.stderr.take() {
            let log_tx_stderr = log_tx.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.is_empty() {
                        let _ = log_tx_stderr.send(StreamLogEvent::Debug {
                            message: format!("simulator-server stderr: {}", line),
                        });
                    }
                }
            });
        }

        let _ = log_tx.send(StreamLogEvent::Info {
            message: format!("simulator-server ready at {}", stream_url),
        });

        Ok(SimulatorSession {
            udid,
            process: child,
            stream_url,
        })
    }

    async fn read_stream_ready_async(
        stdout: tokio::process::ChildStdout,
        log_tx: &broadcast::Sender<StreamLogEvent>,
    ) -> Result<String, String> {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        // Read until we find "stream_ready <URL>"
        loop {
            line.clear();
            reader.read_line(&mut line)
                .await
                .map_err(|e| format!("Failed to read from simulator-server: {}", e))?;

            if line.is_empty() {
                return Err("simulator-server closed without sending stream_ready".to_string());
            }

            let trimmed = line.trim();
            let _ = log_tx.send(StreamLogEvent::Debug {
                message: format!("simulator-server: {}", trimmed),
            });

            if trimmed.starts_with("stream_ready ") {
                let url = trimmed.strip_prefix("stream_ready ")
                    .ok_or_else(|| "Invalid stream_ready format".to_string())?
                    .to_string();
                return Ok(url);
            }
        }
    }
}

impl Drop for SimulatorSession {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

pub async fn stream_simulator(Query(query): Query<StreamQuery>) -> Response {
    let log_tx = STREAM_LOG_SENDER.clone();

    let fps = query.fps
        .or_else(|| {
            std::env::var("PLASMA_STREAM_FPS")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
        })
        .unwrap_or(60)
        .min(60);

    let quality = query.quality
        .or_else(|| {
            std::env::var("PLASMA_STREAM_QUALITY")
                .ok()
                .and_then(|value| value.parse::<f32>().ok())
        })
        .unwrap_or(0.7)
        .clamp(0.1, 1.0);

    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("Stream request for simulator {}", query.udid),
    });
    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("Using FPS: {}, Quality: {}", fps, quality),
    });

    // Get or create session
    let cache = SESSION_CACHE.lock().await;
    let stream_url = match cache.get(&query.udid) {
        Some(session) => {
            let _ = log_tx.send(StreamLogEvent::Info {
                message: format!("Reusing cached session for {}", query.udid),
            });
            session.stream_url.clone()
        }
        None => {
            drop(cache); // Release lock before spawning

            match SimulatorSession::new(query.udid.clone(), fps, quality, &log_tx).await {
                Ok(session) => {
                    let stream_url = session.stream_url.clone();
                    SESSION_CACHE.lock().await.insert(query.udid.clone(), session);
                    stream_url
                }
                Err(e) => {
                    let _ = log_tx.send(StreamLogEvent::Error {
                        message: format!("Failed to start session: {}", e),
                    });
                    return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start session: {}", e)).into_response();
                }
            }
        }
    };

    // Proxy the stream from simulator-server through the backend
    let _ = log_tx.send(StreamLogEvent::Info {
        message: format!("Proxying stream from: {}", stream_url),
    });

    // Stream the MJPEG from simulator-server
    let stream = async_stream::stream! {
        // Use reqwest to fetch the stream from simulator-server
        match reqwest::Client::new().get(&stream_url).send().await {
            Ok(response) => {
                let mut bytes_stream = response.bytes_stream();
                while let Some(chunk_result) = futures::stream::StreamExt::next(&mut bytes_stream).await {
                    match chunk_result {
                        Ok(chunk) => {
                            yield Ok::<_, Infallible>(chunk);
                        }
                        Err(e) => {
                            let _ = log_tx.send(StreamLogEvent::Error {
                                message: format!("Stream chunk error: {}", e),
                            });
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                let _ = log_tx.send(StreamLogEvent::Error {
                    message: format!("Failed to fetch stream from simulator-server: {}", e),
                });
            }
        }
    };

    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        "multipart/x-mixed-replace; boundary=--mjpegstream".parse().unwrap(),
    );
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
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


/// Find the simulator-server binary (persistent connection with persistent callbacks)
fn find_simulator_server_binary() -> Option<PathBuf> {
    // 1. Environment variable override
    if let Ok(path) = std::env::var("SIMULATOR_SERVER") {
        let candidate = PathBuf::from(&path);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 2. Bundled binary in app/bin (for development)
    let dev_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| {
            let tools_server = p.join("tools").join("simulator-server").join(".build").join("debug").join("simulator-server");
            if tools_server.exists() {
                return Some(tools_server);
            }
            
            let tools_server_release = p.join("tools").join("simulator-server").join(".build").join("release").join("simulator-server");
            if tools_server_release.exists() {
                return Some(tools_server_release);
            }
            
            None
        });
    if dev_bin.is_some() {
        return dev_bin;
    }

    // 3. Bundled binary in app resources (for release builds)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(macos_dir) = exe_path.parent() {
            let resources_dir = macos_dir.parent().map(|p| p.join("Resources"));
            if let Some(resources) = resources_dir {
                let bundled = resources.join("binaries").join("simulator-server");
                if bundled.exists() {
                    return Some(bundled);
                }
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
