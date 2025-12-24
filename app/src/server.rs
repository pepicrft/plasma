use crate::routes;
use crate::Database;
use anyhow::Result;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
}

/// Handle to control the running server
pub struct ServerHandle {
    shutdown_tx: oneshot::Sender<()>,
    port: u16,
}

impl ServerHandle {
    /// Get the port the server is running on
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Shutdown the server gracefully
    pub fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Run the HTTP server
pub async fn run_server(
    port: u16,
    db: Database,
    frontend_dir: Option<&str>,
) -> Result<ServerHandle> {
    let state = Arc::new(AppState { db });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(routes::create_routes(frontend_dir))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    let actual_port = listener.local_addr()?.port();

    info!("Server listening on http://localhost:{}", actual_port);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
                info!("Shutting down server...");
            })
            .await
            .ok();
    });

    Ok(ServerHandle {
        shutdown_tx,
        port: actual_port,
    })
}
