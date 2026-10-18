//! Deployment adapter. CAD commands remain in the existing CLI and engine.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod audit;
mod config;
mod http;
mod paths;
mod sessions;
mod workspace;
mod workspace_files;
mod workspace_live;
mod workspace_model;
mod workspace_view;

use std::process::ExitCode;
use std::sync::Arc;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[error] ❌ solvecraft-server: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let config = config::Config::load()?;
    let addr = config.listen.clone();
    let state = Arc::new(sessions::State::new(config));
    let cleaner = Arc::clone(&state);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            tick.tick().await;
            cleaner.expire().await;
        }
    });
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| format!("listen {addr}: {e}"))?;
    eprintln!("SolidCraft web: http://{addr}/ ; workspace: /workspace ; MCP: /mcp (Bearer authentication required)");
    state.audit.startup(&state.config);
    let stopping = Arc::clone(&state);
    axum::serve(listener, http::router(state)).with_graceful_shutdown(shutdown(stopping)).await.map_err(|e| e.to_string())
}

async fn shutdown(state: Arc<sessions::State>) {
    #[cfg(unix)]
    if let Ok(mut terminate) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        tokio::select! { _ = terminate.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
        state.audit.emit(audit::Event::Stopping, serde_json::json!({}));
        state.browser.live.stop();
        return;
    }
    let _ = tokio::signal::ctrl_c().await;
    state.audit.emit(audit::Event::Stopping, serde_json::json!({}));
    state.browser.live.stop();
}
