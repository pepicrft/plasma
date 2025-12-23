use appwave_core::{Config, Database, ServerHandle};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent,
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

struct AppState {
    server_handle: Mutex<Option<ServerHandle>>,
    port: Mutex<u16>,
    frontend_dir: Mutex<Option<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Setup logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Determine frontend directory
            let frontend_dir = get_frontend_dir(app.handle());
            info!("Frontend directory: {:?}", frontend_dir);

            // Initialize app state
            app.manage(AppState {
                server_handle: Mutex::new(None),
                port: Mutex::new(4000),
                frontend_dir: Mutex::new(frontend_dir),
            });

            // Setup system tray
            setup_tray(app.handle())?;

            // Start the server
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = start_server(&handle).await {
                    error!("Failed to start server: {}", e);
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            info!("Application exit requested");
        }
    });
}

fn get_frontend_dir(app: &AppHandle) -> Option<String> {
    // In development, check relative to the executable (target/debug/app)
    let dev_path = std::env::current_exe()
        .ok()?
        .parent()? // target/debug
        .parent()? // target
        .parent()? // project root
        .join("frontend")
        .join("dist");

    if dev_path.exists() {
        return Some(dev_path.to_string_lossy().to_string());
    }

    // In production, use the resource directory
    let resource_path = app
        .path()
        .resource_dir()
        .ok()?
        .join("frontend");

    if resource_path.exists() {
        return Some(resource_path.to_string_lossy().to_string());
    }

    None
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let open_i = MenuItem::with_id(app, "open", "Open in Browser", true, None::<&str>)?;
    let separator = MenuItem::with_id(app, "sep", "-", false, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit Appwave", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_i, &separator, &quit_i])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                open_in_browser(app);
            }
            "quit" => {
                info!("Quit requested from tray menu");
                shutdown_server(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                open_in_browser(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

async fn start_server(app: &AppHandle) -> anyhow::Result<()> {
    info!("Starting Appwave server...");

    let config = Config::load().unwrap_or_default();
    let db_path = config.get_database_path()?;

    info!("Database path: {}", db_path.display());

    let db = Database::new(&db_path).await?;

    // Get frontend directory from state
    let state = app.state::<AppState>();
    let frontend_dir = state.frontend_dir.lock().unwrap().clone();

    let handle = appwave_core::run_server(config, db, frontend_dir.as_deref()).await?;

    let port = handle.port();
    info!("Server started on port {}", port);

    // Store the handle and port
    *state.port.lock().unwrap() = port;
    *state.server_handle.lock().unwrap() = Some(handle);

    Ok(())
}

fn shutdown_server(app: &AppHandle) {
    let state = app.state::<AppState>();
    let handle = state.server_handle.lock().unwrap().take();
    if let Some(h) = handle {
        info!("Shutting down server...");
        h.shutdown();
    }
}

fn open_in_browser(app: &AppHandle) {
    let state = app.state::<AppState>();
    let port = *state.port.lock().unwrap();
    let url = format!("http://localhost:{}", port);

    info!("Opening {} in browser", url);

    if let Err(e) = open::that(&url) {
        error!("Failed to open browser: {}", e);
    }
}
