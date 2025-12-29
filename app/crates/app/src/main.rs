//! Plasma - Native iOS Simulator Streaming App
//!
//! A GPU-accelerated native app for streaming iOS simulators using gpui.

use cocoa::appkit::NSApp;
use cocoa::base::{id, nil};
use cocoa::foundation::NSData;
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_graphics::geometry::CGRect;
use core_graphics::image::CGImage;
use core_graphics::window::{
    copy_window_info, create_image, kCGNullWindowID, kCGWindowBounds,
    kCGWindowImageBoundsIgnoreFraming, kCGWindowImageNominalResolution,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionIncludingWindow,
    kCGWindowListOptionOnScreenOnly, kCGWindowLayer, kCGWindowName, kCGWindowNumber,
    kCGWindowOwnerName, CGWindowID,
};
use gpui::prelude::FluentBuilder;
use gpui::*;
use image::Frame;
use objc::{class, msg_send, sel, sel_impl};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::io::{BufRead, Read, Write};
use std::{env, io};

/// Set the dock icon from embedded icns data
#[allow(deprecated)]
fn set_dock_icon() {
    let icon_data = include_bytes!("../resources/Plasma.icns");

    unsafe {
        let data = NSData::dataWithBytes_length_(
            nil,
            icon_data.as_ptr() as *const std::ffi::c_void,
            icon_data.len() as u64,
        );
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithData: data];
        if image != nil {
            let app = NSApp();
            let _: () = msg_send![app, setApplicationIconImage: image];
        }
    }
}

#[derive(Clone, Debug)]
enum AppMessage {
    Log(String),
    Status(String),
    BuildDone { simulator_udid: String },
    Error(String),
    Frame(Arc<Vec<u8>>),
}

#[derive(Clone, PartialEq)]
enum AppState {
    Idle,
    Building,
    Streaming { simulator_udid: String },
}

struct PlasmaApp {
    status: SharedString,
    build_log: Vec<String>,
    state: AppState,
    current_frame: Option<Arc<RenderImage>>,
    simulator_udid: Option<String>,
    capture_mode: SharedString,
    capture_mode_flag: Option<Arc<AtomicU8>>,
    _task: Option<Task<()>>,
    _stream_task: Option<Task<()>>,
}

impl PlasmaApp {
    fn build_and_run_fixture(&mut self, cx: &mut ViewContext<Self>) {
        if self.state != AppState::Idle {
            return;
        }

        self.status = "Starting build...".into();
        self.build_log.clear();
        self.state = AppState::Building;
        self.current_frame = None;
        cx.notify();

        let (tx, rx) = mpsc::channel::<AppMessage>();

        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join("app/tests/fixtures/xcode/Plasma"))
            .unwrap_or_default();

        thread::spawn(move || {
            run_build(fixture_path, tx);
        });

        self._task = Some(cx.spawn(|view, mut cx| async move {
            loop {
                Timer::after(Duration::from_millis(50)).await;

                let messages: Vec<AppMessage> = rx.try_iter().collect();

                if messages.is_empty() {
                    continue;
                }

                let mut should_stop = false;
                let mut simulator_udid_to_stream: Option<String> = None;

                for msg in messages {
                    match &msg {
                        AppMessage::BuildDone { simulator_udid } => {
                            simulator_udid_to_stream = Some(simulator_udid.clone());
                            should_stop = true;
                        }
                        AppMessage::Error(_) => {
                            should_stop = true;
                        }
                        _ => {}
                    }

                    let msg_clone = msg.clone();
                    let _ = cx.update(|cx| {
                        let _ = view.update(cx, |view, cx| {
                            match msg_clone {
                                AppMessage::Log(line) => {
                                    view.build_log.push(line);
                                }
                                AppMessage::Status(status) => {
                                    view.status = status.into();
                                }
                                AppMessage::BuildDone { simulator_udid } => {
                                    view.state = AppState::Streaming {
                                        simulator_udid: simulator_udid.clone(),
                                    };
                                    view.simulator_udid = Some(simulator_udid.clone());
                                    view.status = format!("Streaming from simulator...").into();
                                }
                                AppMessage::Error(err) => {
                                    view.status = format!("Error: {}", err).into();
                                    view.state = AppState::Idle;
                                }
                                AppMessage::Frame(_) => {}
                            }
                            cx.notify();
                        });
                    });
                }

                if should_stop {
                    // Start streaming if we have a simulator
                    if let Some(udid) = simulator_udid_to_stream {
                        let _ = cx.update(|cx| {
                            let _ = view.update(cx, |view, cx| {
                                view.start_streaming(udid, cx);
                            });
                        });
                    }
                    break;
                }
            }
        }));
    }

    fn press_home(&self) {
        if let Some(udid) = &self.simulator_udid {
            let udid = udid.clone();
            thread::spawn(move || {
                // Use AXe to press the home button (installed via: brew install cameroncooke/axe/axe)
                let _ = Command::new("axe")
                    .args(["button", "home", "--udid", &udid])
                    .output();
            });
        }
    }

    fn start_streaming(&mut self, simulator_udid: String, cx: &mut ViewContext<Self>) {
        // Channel for decoded RenderImage from capture thread
        let (decoded_tx, decoded_rx) = mpsc::sync_channel::<Arc<RenderImage>>(1);
        let (log_tx, log_rx) = mpsc::channel::<String>();
        let capture_mode_flag = Arc::new(AtomicU8::new(0));
        self.capture_mode_flag = Some(capture_mode_flag.clone());

        // Spawn background thread for capturing frames
        thread::spawn(move || {
            capture_frames(simulator_udid, decoded_tx, capture_mode_flag, log_tx);
        });

        // Spawn async task to receive decoded frames and update UI (fast - just pointer copy)
        self._stream_task = Some(cx.spawn(|view, mut cx| async move {
            let mut last_mode = 0u8;
            loop {
                Timer::after(Duration::from_millis(16)).await; // ~60 FPS polling

                // Get the latest decoded frame (skip older ones for responsiveness)
                let mut latest_frame: Option<Arc<RenderImage>> = None;
                for frame in decoded_rx.try_iter() {
                    latest_frame = Some(frame);
                }

                for line in log_rx.try_iter() {
                    eprintln!("[Capture] {}", line);
                }

                if let Some(render_image) = latest_frame {
                    let _ = cx.update(|cx| {
                        let _ = view.update(cx, |view, cx| {
                            // Just assign the pre-decoded image - no decoding on UI thread!
                            view.current_frame = Some(render_image);
                            if let Some(flag) = &view.capture_mode_flag {
                                let mode = flag.load(Ordering::Relaxed);
                                if mode != last_mode {
                                    view.capture_mode = match mode {
                                        1 => "Streaming: sim-server".into(),
                                        2 => "Streaming: window capture".into(),
                                        3 => "Streaming: simctl fallback".into(),
                                        4 => "Streaming: fbsimctl".into(),
                                        _ => "Streaming: starting...".into(),
                                    };
                                    last_mode = mode;
                                }
                            }
                            cx.notify();
                        });
                    });
                }
            }
        }));
    }
}

struct WindowCaptureTarget {
    window_id: CGWindowID,
    bounds: CGRect,
}

#[derive(Clone, Copy, Debug)]
struct FbsimctlStreamAttributes {
    width: usize,
    height: usize,
    row_size: usize,
    frame_size: usize,
}

fn find_simulator_window_target() -> Option<WindowCaptureTarget> {
    let windows = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let mut best: Option<WindowCaptureTarget> = None;
    let mut best_area = 0.0;

    for entry in windows.iter() {
        let dict_ref = *entry as CFDictionaryRef;
        if dict_ref.is_null() {
            continue;
        }
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(dict_ref) };
        let owner_name = dict
            .find(unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerName) })
            .and_then(|value| value.downcast::<CFString>())
            .map(|name| name.to_string())
            .unwrap_or_default();

        if owner_name != "Simulator" {
            continue;
        }

        let layer = dict
            .find(unsafe { CFString::wrap_under_get_rule(kCGWindowLayer) })
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|number| number.to_i32())
            .unwrap_or(0);

        if layer != 0 {
            continue;
        }

        let window_name = dict
            .find(unsafe { CFString::wrap_under_get_rule(kCGWindowName) })
            .and_then(|value| value.downcast::<CFString>())
            .map(|name| name.to_string())
            .unwrap_or_default();

        let window_id = dict
            .find(unsafe { CFString::wrap_under_get_rule(kCGWindowNumber) })
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|number| number.to_i64())
            .map(|id| id as CGWindowID);

        let bounds = dict
            .find(unsafe { CFString::wrap_under_get_rule(kCGWindowBounds) })
            .and_then(|value| value.downcast::<CFDictionary>())
            .and_then(|dict| CGRect::from_dict_representation(&dict));

        if let (Some(window_id), Some(bounds)) = (window_id, bounds) {
            if !bounds.is_empty() {
                let area = bounds.size.width * bounds.size.height;
                if window_name.is_empty() && area < 100.0 {
                    continue;
                }
                if area > best_area {
                    best_area = area;
                    best = Some(WindowCaptureTarget { window_id, bounds });
                }
            }
        }
    }

    best
}

fn cgimage_to_rgba(image: &CGImage) -> Option<image::RgbaImage> {
    let width = image.width() as u32;
    let height = image.height() as u32;
    let bytes_per_row = image.bytes_per_row() as usize;

    if width == 0 || height == 0 {
        return None;
    }

    if image.bits_per_pixel() != 32 || image.bits_per_component() != 8 {
        return None;
    }

    let data = image.data();
    let expected_len = bytes_per_row.saturating_mul(height as usize);
    let data_len = data.len() as usize;
    if data_len < expected_len {
        return None;
    }

    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        let src_row = y as usize * bytes_per_row;
        let dst_row = y as usize * width as usize * 4;
        for x in 0..width {
            let src = src_row + x as usize * 4;
            let dst = dst_row + x as usize * 4;
            let b = data[src];
            let g = data[src + 1];
            let r = data[src + 2];
            let a = data[src + 3];
            rgba[dst] = r;
            rgba[dst + 1] = g;
            rgba[dst + 2] = b;
            rgba[dst + 3] = a;
        }
    }

    image::RgbaImage::from_raw(width, height, rgba)
}

fn capture_window_frame(target: &WindowCaptureTarget) -> Option<RenderImage> {
    let image = create_image(
        target.bounds,
        kCGWindowListOptionIncludingWindow,
        target.window_id,
        kCGWindowImageBoundsIgnoreFraming | kCGWindowImageNominalResolution,
    )?;

    let rgba = cgimage_to_rgba(&image)?;
    let frame = Frame::new(rgba);
    Some(RenderImage::new(vec![frame]))
}

fn find_simulator_server_binary() -> Option<PathBuf> {
    if let Ok(path) = env::var("PLASMA_SIM_SERVER") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(home) = env::var("HOME") {
        let extensions_dir = PathBuf::from(home).join(".vscode/extensions");
        if let Ok(entries) = std::fs::read_dir(&extensions_dir) {
            let mut best: Option<PathBuf> = None;
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                if !name.starts_with("swmansion.react-native-ide-") {
                    continue;
                }
                let candidate = entry
                    .path()
                    .join("dist")
                    .join("simulator-server-macos");
                if candidate.exists() {
                    best = Some(candidate);
                }
            }
            if best.is_some() {
                return best;
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("bin/simulator-server-macos"));
    if let Some(path) = candidate {
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn find_fbsimctl_binary() -> Option<PathBuf> {
    if let Ok(path) = env::var("PLASMA_FBSIMCTL") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("bin/fbsimctl"));
    if let Some(path) = candidate {
        if path.exists() {
            return Some(path);
        }
    }

    for candidate in ["/opt/homebrew/bin/fbsimctl", "/usr/local/bin/fbsimctl"] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(path_var) = env::var("PATH") {
        for entry in env::split_paths(&path_var) {
            let candidate = entry.join("fbsimctl");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

fn parse_fbsimctl_attributes_line(line: &str) -> Option<FbsimctlStreamAttributes> {
    let marker = "Mounting Surface with Attributes:";
    let start = line.find(marker)?;
    let attrs = line[start + marker.len()..].trim();
    let attrs = attrs.trim_start_matches('{').trim_end_matches('}');

    let mut width = None;
    let mut height = None;
    let mut row_size = None;
    let mut frame_size = None;

    for part in attrs.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut iter = part.splitn(2, '=');
        let key = iter.next()?.trim().trim_matches('"');
        let value = iter.next()?.trim();
        let parsed = value.parse::<usize>().ok();
        match key {
            "width" => width = parsed,
            "height" => height = parsed,
            "row_size" => row_size = parsed,
            "frame_size" => frame_size = parsed,
            _ => {}
        }
    }

    let width = width?;
    let height = height?;
    let row_size = row_size.unwrap_or(width * 4);
    let frame_size = frame_size.unwrap_or(row_size.saturating_mul(height));

    Some(FbsimctlStreamAttributes {
        width,
        height,
        row_size,
        frame_size,
    })
}

fn read_fbsimctl_stream(
    mut stdout: impl Read,
    attrs: FbsimctlStreamAttributes,
    tx: mpsc::SyncSender<Arc<RenderImage>>,
) -> bool {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut stash: Vec<u8> = Vec::new();
    let mut offset = 0usize;

    loop {
        let read = match stdout.read(&mut buffer) {
            Ok(0) => return false,
            Ok(n) => n,
            Err(_) => return false,
        };

        stash.extend_from_slice(&buffer[..read]);
        while stash.len().saturating_sub(offset) >= attrs.frame_size {
            let frame_bytes = &stash[offset..offset + attrs.frame_size];
            // RenderImage expects BGRA ordering, so keep the bytes as-is and just strip row padding.
            let mut bgra = Vec::with_capacity(attrs.width * attrs.height * 4);
            for y in 0..attrs.height {
                let src_row = y * attrs.row_size;
                let row = &frame_bytes[src_row..src_row + attrs.width * 4];
                bgra.extend_from_slice(row);
            }

            if let Some(image) =
                image::RgbaImage::from_raw(attrs.width as u32, attrs.height as u32, bgra)
            {
                let frame = Frame::new(image);
                let render_image = RenderImage::new(vec![frame]);
                match tx.try_send(Arc::new(render_image)) {
                    Ok(_) => {}
                    Err(mpsc::TrySendError::Full(_)) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => return false,
                }
            }

            offset += attrs.frame_size;
        }

        if offset > 0 {
            stash.drain(0..offset);
            offset = 0;
        }
    }
}

fn capture_frames_with_fbsimctl(
    simulator_udid: String,
    tx: mpsc::SyncSender<Arc<RenderImage>>,
    capture_mode_flag: Arc<AtomicU8>,
    log_tx: mpsc::Sender<String>,
) -> bool {
    let Some(fbsimctl_path) = find_fbsimctl_binary() else {
        let _ = log_tx.send("fbsimctl: binary not found".to_string());
        return false;
    };

    let fps = env::var("PLASMA_FBSIMCTL_FPS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(30);

    let mut args = Vec::new();
    if env::var("PLASMA_FBSIMCTL_DEBUG").ok().as_deref() == Some("1") {
        args.push("--debug-logging".to_string());
    }
    args.extend([
        simulator_udid,
        "stream".to_string(),
        "--bgra".to_string(),
        "--fps".to_string(),
        fps.to_string(),
        "-".to_string(),
    ]);

    let mut child = match Command::new(fbsimctl_path)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let _ = log_tx.send(format!("fbsimctl: spawn failed ({})", err));
            return false;
        }
    };

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = log_tx.send("fbsimctl: missing stdout".to_string());
            return false;
        }
    };

    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = log_tx.send("fbsimctl: missing stderr".to_string());
            return false;
        }
    };

    let (attr_tx, attr_rx) = mpsc::channel::<FbsimctlStreamAttributes>();
    let log_tx_stderr = log_tx.clone();
    thread::spawn(move || {
        let mut reader = io::BufReader::new(stderr);
        let mut line = String::new();
        let mut attrs_block: Option<String> = None;
        loop {
            line.clear();
            let bytes = match reader.read_line(&mut line) {
                Ok(bytes) => bytes,
                Err(_) => return,
            };
            if bytes == 0 {
                return;
            }
            let trimmed = line.trim_end();
            if let Some(block) = attrs_block.as_mut() {
                if !trimmed.is_empty() {
                    block.push(' ');
                    block.push_str(trimmed);
                }
                if trimmed.contains('}') {
                    if let Some(attrs) = parse_fbsimctl_attributes_line(block) {
                        let _ = attr_tx.send(attrs);
                        let _ = log_tx_stderr.send(format!(
                            "fbsimctl: stream attributes {}x{} row={} frame={}",
                            attrs.width, attrs.height, attrs.row_size, attrs.frame_size
                        ));
                        attrs_block = None;
                        continue;
                    }
                    attrs_block = None;
                }
            } else if trimmed.contains("Mounting Surface with Attributes:") {
                let mut block = String::new();
                block.push_str(trimmed);
                if trimmed.contains('}') {
                    if let Some(attrs) = parse_fbsimctl_attributes_line(&block) {
                        let _ = attr_tx.send(attrs);
                        let _ = log_tx_stderr.send(format!(
                            "fbsimctl: stream attributes {}x{} row={} frame={}",
                            attrs.width, attrs.height, attrs.row_size, attrs.frame_size
                        ));
                        continue;
                    }
                } else {
                    attrs_block = Some(block);
                    continue;
                }
            }
            if !trimmed.is_empty() {
                let _ = log_tx_stderr.send(format!("fbsimctl: {}", trimmed));
            }
        }
    });

    let attrs = match attr_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(attrs) => attrs,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = log_tx.send("fbsimctl: timed out waiting for stream attributes".to_string());
            return false;
        }
    };

    if attrs.row_size < attrs.width * 4 {
        let _ = log_tx.send(format!(
            "fbsimctl: invalid row_size {} for width {}",
            attrs.row_size, attrs.width
        ));
        let _ = child.kill();
        let _ = child.wait();
        return false;
    }
    if attrs.row_size.saturating_mul(attrs.height) > attrs.frame_size {
        let _ = log_tx.send(format!(
            "fbsimctl: frame_size {} smaller than row_size * height {}",
            attrs.frame_size,
            attrs.row_size.saturating_mul(attrs.height)
        ));
        let _ = child.kill();
        let _ = child.wait();
        return false;
    }

    capture_mode_flag.store(4, Ordering::Relaxed);
    let result = read_fbsimctl_stream(stdout, attrs, tx);
    if let Ok(status) = child.try_wait() {
        if let Some(status) = status {
            let _ = log_tx.send(format!("fbsimctl: exited with {}", status));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn stream_mjpeg_reader(
    reader: impl Read,
    tx: mpsc::SyncSender<Arc<RenderImage>>,
) -> Option<()> {
    let mut reader = io::BufReader::new(reader);
    let mut line = String::new();
    let mut content_length: Option<usize> = None;

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).ok()?;
        if bytes == 0 {
            return None;
        }

        let trimmed = line.trim_end();
        if trimmed.starts_with("--") {
            content_length = None;
            continue;
        }

        if trimmed.is_empty() {
            if let Some(len) = content_length.take() {
                let mut buffer = vec![0u8; len];
                if reader.read_exact(&mut buffer).is_err() {
                    return None;
                }

                let mut crlf = [0u8; 2];
                let _ = reader.read_exact(&mut crlf);

                if let Ok(img) = image::load_from_memory(&buffer) {
                    let rgba = img.to_rgba8();
                    let frame = Frame::new(rgba);
                    let render_image = RenderImage::new(vec![frame]);
                    match tx.try_send(Arc::new(render_image)) {
                        Ok(_) => {}
                        Err(mpsc::TrySendError::Full(_)) => {}
                        Err(mpsc::TrySendError::Disconnected(_)) => return None,
                    }
                }
            }
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            if let Ok(len) = rest.trim().parse::<usize>() {
                content_length = Some(len);
            }
        }
    }
}

fn stream_mjpeg_with_retries(
    url: &str,
    tx: mpsc::SyncSender<Arc<RenderImage>>,
    log_tx: mpsc::Sender<String>,
) -> bool {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let response = match ureq::get(url).call() {
            Ok(response) => response,
            Err(err) => {
                let _ = log_tx.send(format!("mjpeg: connect failed ({})", err));
                if attempt >= 10 {
                    return false;
                }
                thread::sleep(Duration::from_millis(500));
                continue;
            }
        };

        let _ = log_tx.send("mjpeg: connected".to_string());
        let result = stream_mjpeg_reader(response.into_reader(), tx.clone()).is_some();
        let _ = log_tx.send("mjpeg: stream ended, retrying".to_string());

        if result {
            return true;
        }

        if attempt >= 10 {
            return false;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn capture_frames_with_sim_server(
    simulator_udid: String,
    tx: mpsc::SyncSender<Arc<RenderImage>>,
    capture_mode_flag: Arc<AtomicU8>,
    log_tx: mpsc::Sender<String>,
) -> bool {
    let Some(server_path) = find_simulator_server_binary() else {
        let _ = log_tx.send("sim-server: binary not found".to_string());
        return false;
    };

    let mut args = vec!["ios".to_string(), "--id".to_string(), simulator_udid];
    let mut license_token: Option<String> = None;
    if let Ok(device_set) = env::var("PLASMA_SIM_DEVICE_SET") {
        if !device_set.is_empty() {
            args.push("--device-set".to_string());
            args.push(device_set);
        }
    }
    if let Ok(token) = env::var("PLASMA_SIM_SERVER_TOKEN") {
        if !token.is_empty() {
            license_token = Some(token);
        }
    } else {
        let _ = log_tx.send(
            "sim-server: no license token set (PLASMA_SIM_SERVER_TOKEN)".to_string(),
        );
    }

    let mut child = match Command::new(server_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let _ = log_tx.send(format!("sim-server: spawn failed ({})", err));
            return false;
        }
    };

    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            let _ = log_tx.send("sim-server: missing stdin".to_string());
            return false;
        }
    };

    if let Some(token) = license_token.as_deref() {
        if let Err(err) = stdin.write_all(format!("token {}\n", token).as_bytes()) {
            let _ = log_tx.send(format!("sim-server: failed to send token ({})", err));
        } else {
            let _ = log_tx.send("sim-server: token sent via stdin".to_string());
        }
    }

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = log_tx.send("sim-server: missing stdout".to_string());
            return false;
        }
    };

    let log_tx_clone = log_tx.clone();
    if let Some(stderr) = child.stderr.take() {
        let log_tx_clone = log_tx_clone.clone();
        thread::spawn(move || {
            let mut reader = io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                let bytes = match reader.read_line(&mut line) {
                    Ok(bytes) => bytes,
                    Err(_) => return,
                };
                if bytes == 0 {
                    return;
                }
                let _ = log_tx_clone.send(format!("sim-server: {}", line.trim_end()));
            }
        });
    }

    let (url_tx, url_rx) = mpsc::channel::<String>();
    let log_tx_stdout = log_tx.clone();
    thread::spawn(move || {
        let mut reader = io::BufReader::new(stdout);
        let mut line = String::new();
        let mut sent = false;

        loop {
            line.clear();
            let bytes = match reader.read_line(&mut line) {
                Ok(bytes) => bytes,
                Err(_) => return,
            };
            if bytes == 0 {
                return;
            }

            let trimmed = line.trim_end();
            if !sent && trimmed.contains("stream_ready") {
                if let Some(start) = line.find("http://") {
                    let rest = &line[start..];
                    if let Some(url) = rest.split_whitespace().next() {
                        let _ = url_tx.send(url.to_string());
                        sent = true;
                    }
                }
            } else if !trimmed.is_empty() {
                let _ = log_tx_stdout.send(format!("sim-server: {}", trimmed));
            }
        }
    });

    let stream_url = match url_rx.recv_timeout(Duration::from_secs(15)) {
        Ok(url) => url,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = log_tx.send("sim-server: timed out waiting for stream_ready".to_string());
            return false;
        }
    };

    let _ = log_tx.send(format!("sim-server: stream_ready {}", stream_url));
    capture_mode_flag.store(1, Ordering::Relaxed);
    let result = stream_mjpeg_with_retries(&stream_url, tx, log_tx.clone());
    if let Ok(status) = child.try_wait() {
        if let Some(status) = status {
            let _ = log_tx.send(format!("sim-server: exited with {}", status));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn capture_frames(
    simulator_udid: String,
    tx: mpsc::SyncSender<Arc<RenderImage>>,
    capture_mode_flag: Arc<AtomicU8>,
    log_tx: mpsc::Sender<String>,
) {
    use std::process::Stdio;

    let _ = log_tx.send("fbsimctl: starting".to_string());
    if capture_frames_with_fbsimctl(
        simulator_udid.clone(),
        tx.clone(),
        capture_mode_flag.clone(),
        log_tx.clone(),
    ) {
        return;
    }

    let _ = log_tx.send("sim-server: starting".to_string());
    if capture_frames_with_sim_server(
        simulator_udid.clone(),
        tx.clone(),
        capture_mode_flag.clone(),
        log_tx.clone(),
    ) {
        return;
    }

    let mut target: Option<WindowCaptureTarget> = None;
    let mut last_refresh = std::time::Instant::now() - Duration::from_secs(5);

    loop {
        if target.is_none() || last_refresh.elapsed() > Duration::from_secs(1) {
            target = find_simulator_window_target();
            last_refresh = std::time::Instant::now();
        }

        if let Some(current) = &target {
            if let Some(render_image) = capture_window_frame(current) {
                capture_mode_flag.store(2, Ordering::Relaxed);
                match tx.try_send(Arc::new(render_image)) {
                    Ok(_) => {}
                    Err(mpsc::TrySendError::Full(_)) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                }
            } else {
                target = None;
            }

            thread::sleep(Duration::from_millis(33));
            continue;
        }

        // Fall back to simctl screenshot if we can't locate the Simulator window.
        let _ = log_tx.send("simctl: using screenshot fallback".to_string());
        let output = Command::new("xcrun")
            .args([
                "simctl",
                "io",
                &simulator_udid,
                "screenshot",
                "--type=jpeg",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        if let Ok(out) = output {
            if out.status.success() && !out.stdout.is_empty() {
                if let Ok(img) = image::load_from_memory(&out.stdout) {
                    let rgba = img.to_rgba8();
                    let frame = Frame::new(rgba);
                    let render_image = RenderImage::new(vec![frame]);
                    capture_mode_flag.store(3, Ordering::Relaxed);
                    match tx.try_send(Arc::new(render_image)) {
                        Ok(_) => {}
                        Err(mpsc::TrySendError::Full(_)) => {}
                        Err(mpsc::TrySendError::Disconnected(_)) => return,
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(200));
    }
}

fn run_build(fixture_path: PathBuf, tx: mpsc::Sender<AppMessage>) {
    let send = |msg: AppMessage| {
        eprintln!("[Build] {:?}", msg);
        let _ = tx.send(msg);
    };

    send(AppMessage::Log(format!(
        "Fixture path: {:?}",
        fixture_path
    )));
    send(AppMessage::Status("Discovering project...".to_string()));

    let project = match plasma_xcode::discover_project(&fixture_path) {
        Ok(p) => p,
        Err(e) => {
            send(AppMessage::Error(format!(
                "Failed to discover project: {}",
                e
            )));
            return;
        }
    };

    send(AppMessage::Log(format!("Project: {}", project.name)));
    send(AppMessage::Log(format!("Schemes: {:?}", project.schemes)));

    let Some(scheme) = project.schemes.first() else {
        send(AppMessage::Error("No schemes found".to_string()));
        return;
    };

    send(AppMessage::Log(format!("Building scheme: {}", scheme)));
    send(AppMessage::Status(format!("Building {}...", scheme)));

    let build_result = match plasma_xcode::build_scheme(&project, scheme) {
        Ok(r) => r,
        Err(e) => {
            send(AppMessage::Error(format!("Build failed: {}", e)));
            return;
        }
    };

    send(AppMessage::Log(format!(
        "Build dir: {:?}",
        build_result.build_dir
    )));
    send(AppMessage::Log(format!(
        "Products: {:?}",
        build_result
            .products
            .iter()
            .map(|p| &p.name)
            .collect::<Vec<_>>()
    )));

    send(AppMessage::Status("Finding simulator...".to_string()));
    let simulator = match plasma_xcode::find_default_simulator() {
        Ok(s) => s,
        Err(e) => {
            send(AppMessage::Error(format!("Simulator error: {}", e)));
            return;
        }
    };

    send(AppMessage::Log(format!(
        "Using simulator: {} ({})",
        simulator.name, simulator.udid
    )));

    send(AppMessage::Status(format!("Booting {}...", simulator.name)));
    if let Err(e) = plasma_xcode::boot_simulator(&simulator.udid) {
        send(AppMessage::Error(format!(
            "Failed to boot simulator: {}",
            e
        )));
        return;
    }

    send(AppMessage::Log("Simulator booted".to_string()));

    if let Some(product) = build_result.products.first() {
        send(AppMessage::Status(format!(
            "Installing {}...",
            product.name
        )));
        send(AppMessage::Log(format!("Installing: {}", product.name)));

        if let Err(e) = plasma_xcode::install_app(&simulator.udid, &product.path) {
            send(AppMessage::Error(format!("Install failed: {}", e)));
            return;
        }

        let bundle_id = match plasma_xcode::get_bundle_id(&product.path) {
            Ok(id) => id,
            Err(e) => {
                send(AppMessage::Error(format!(
                    "Failed to get bundle ID: {}",
                    e
                )));
                return;
            }
        };

        send(AppMessage::Status(format!("Launching {}...", bundle_id)));
        send(AppMessage::Log(format!("Launching: {}", bundle_id)));

        if let Err(e) = plasma_xcode::launch_app(&simulator.udid, &bundle_id) {
            send(AppMessage::Error(format!("Launch failed: {}", e)));
            return;
        }

        send(AppMessage::Log("App launched successfully!".to_string()));
        send(AppMessage::BuildDone {
            simulator_udid: simulator.udid,
        });
    } else {
        send(AppMessage::Error(
            "No products found after build".to_string(),
        ));
    }
}

impl Render for PlasmaApp {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let is_busy = self.state != AppState::Idle;
        let is_streaming = matches!(self.state, AppState::Streaming { .. });

        let button_text = match &self.state {
            AppState::Idle => "Build & Run Fixture",
            AppState::Building => "Building...",
            AppState::Streaming { .. } => "Streaming...",
        };

        let button_bg = if is_busy {
            rgb(0x4b5563)
        } else {
            rgb(0x6366f1)
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1c1c24))
            .p_4()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child("Plasma"),
                    )
                    .child(
                        div()
                            .id("build-btn")
                            .px_4()
                            .py_2()
                            .bg(button_bg)
                            .rounded_md()
                            .text_color(rgb(0xffffff))
                            .cursor_pointer()
                            .child(button_text)
                            .on_click(cx.listener(|this, _, cx| {
                                this.build_and_run_fixture(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_base()
                            .text_color(rgb(0x9ca3af))
            .child(self.status.clone()),
                    )
                    .when(is_streaming, |this| {
                        this.child(
                            div()
                                .id("home-btn")
                                .px_3()
                                .py_1()
                                .bg(rgb(0x374151))
                                .rounded_md()
                                .text_color(rgb(0xffffff))
                                .text_sm()
                                .cursor_pointer()
                                .child("Home")
                                .on_click(cx.listener(|this, _, _cx| {
                                    this.press_home();
                                })),
                        )
                    }),
            )
            .when(is_streaming, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x6b7280))
                        .child(self.capture_mode.clone()),
                )
            })
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .bg(rgb(0x0f0f14))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if is_streaming {
                        // Show simulator frame
                        if let Some(frame) = &self.current_frame {
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size_full()
                                .child(
                                    img(ImageSource::Render(frame.clone()))
                                        .max_h_full()
                                        .max_w_full()
                                        .object_fit(ObjectFit::Contain),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .text_color(rgb(0x6b7280))
                                .child("Loading simulator...")
                                .into_any_element()
                        }
                    } else {
                        // Show build log
                        div()
                            .flex()
                            .flex_col()
                            .p_4()
                            .size_full()
                            .overflow_hidden()
                            .children(self.build_log.iter().map(|line| {
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x6b7280))
                                    .child(line.clone())
                            }))
                            .into_any_element()
                    }),
            )
    }
}

fn main() {
    let app = App::new();
    app.on_reopen(|cx| {
        if let Some(window) = cx.active_window() {
            window
                .update(cx, |_, cx| {
                    cx.activate_window();
                })
                .ok();
        } else {
            cx.activate(true);
        }
    });
    app.run(|cx: &mut AppContext| {
        set_dock_icon();

        let bounds = Bounds::centered(None, size(px(400.), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Plasma".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                ..Default::default()
            },
            |cx| {
                cx.new_view(|_cx| PlasmaApp {
                    status: "Click 'Build & Run Fixture' to build the Plasma test app".into(),
                    build_log: vec![],
                    state: AppState::Idle,
                    current_frame: None,
                    simulator_udid: None,
                    capture_mode: "Streaming: starting...".into(),
                    capture_mode_flag: None,
                    _task: None,
                    _stream_task: None,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
