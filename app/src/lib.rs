mod db;
mod routes;
mod server;

use clap::{Parser, Subcommand};
use db::Database;
use server::ServerHandle;
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, RunEvent,
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

/// Initialize logging with the given debug level
fn setup_logging(debug: bool) {
    let filter = if debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Initialize the database at the default path
async fn init_database() -> anyhow::Result<Database> {
    let db_path = db::default_path()?;
    info!("Database path: {}", db_path.display());
    Database::new(&db_path).await
}

#[derive(Parser)]
#[command(name = "plasma")]
#[command(about = "AI-powered app development")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run headless server without GUI
    Serve {
        /// Port to run the server on
        #[arg(short, long, default_value = "4000")]
        port: u16,

        /// Path to frontend directory
        #[arg(short, long)]
        frontend: Option<String>,
    },
}

struct AppState {
    server_handle: Mutex<Option<ServerHandle>>,
    port: Mutex<u16>,
    frontend_dir: Mutex<Option<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { port, frontend }) => {
            run_headless(port, frontend, cli.debug);
        }
        None => {
            run_desktop(cli.debug);
        }
    }
}

/// Run in headless mode (server only, no GUI)
fn run_headless(port: u16, frontend_dir: Option<String>, debug: bool) {
    setup_logging(debug);

    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
    rt.block_on(async move {
        if let Err(e) = run_server_headless(port, frontend_dir).await {
            error!("Server error: {}", e);
            std::process::exit(1);
        }
    });
}

async fn run_server_headless(port: u16, frontend_dir: Option<String>) -> anyhow::Result<()> {
    info!("Starting Plasma server in headless mode...");

    let db = init_database().await?;
    let handle = server::run_server(port, db, frontend_dir.as_deref()).await?;

    info!("Server running on http://localhost:{}", handle.port());
    info!("Press Ctrl+C to stop");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    info!("Shutting down...");
    handle.shutdown();

    Ok(())
}

/// Run in desktop mode (with system tray)
fn run_desktop(debug: bool) {
    setup_logging(debug);

    // Set macOS activation policy to hide from dock
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_cli::init())
            .setup(|app| {
                app.set_activation_policy(ActivationPolicy::Accessory);
                setup_app(app)
            })
            .build(tauri::generate_context!())
            .expect("error while building tauri application")
            .run(|_app_handle, event| {
                if let RunEvent::ExitRequested { .. } = event {
                    info!("Application exit requested");
                }
            });
        return;
    }

    #[cfg(not(target_os = "macos"))]
    {
        tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_cli::init())
            .setup(setup_app)
            .build(tauri::generate_context!())
            .expect("error while building tauri application")
            .run(|_app_handle, event| {
                if let RunEvent::ExitRequested { .. } = event {
                    info!("Application exit requested");
                }
            });
    }
}

fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
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
    let resource_path = app.path().resource_dir().ok()?.join("frontend");

    if resource_path.exists() {
        return Some(resource_path.to_string_lossy().to_string());
    }

    None
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let open_i = MenuItem::with_id(app, "open", "Open in Browser", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit Plasma", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_i, &separator, &quit_i])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(true)
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
        .build(app)?;

    Ok(())
}

async fn start_server(app: &AppHandle) -> anyhow::Result<()> {
    info!("Starting Plasma server...");

    let db = init_database().await?;

    // Get frontend directory from state
    let state = app.state::<AppState>();
    let frontend_dir = state.frontend_dir.lock().unwrap().clone();
    let port = *state.port.lock().unwrap();

    let handle = server::run_server(port, db, frontend_dir.as_deref()).await?;

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
