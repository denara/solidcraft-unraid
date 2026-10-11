//! Deployment adapter. CAD commands remain in the existing CLI and engine.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod config;
mod http;
mod paths;
mod sessions;

use std::process::ExitCode;
use std::sync::Arc;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("solvecraft-server: {e}");
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
    eprintln!("SolidCraft web: http://{addr}/ ; MCP: /mcp (Bearer authentication required)");
    axum::serve(listener, http::router(state))
        .with_graceful_shutdown(async {
            if let Err(e) = tokio::signal::ctrl_c().await {
                eprintln!("shutdown signal: {e}");
            }
        })
        .await
        .map_err(|e| e.to_string())
}
