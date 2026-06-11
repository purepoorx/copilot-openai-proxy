mod config;
mod copilot;
mod error;
mod openai;
mod server;
mod session;
mod util;

use std::io::IsTerminal;
use std::sync::Arc;

use clap::Parser;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::copilot::client::CopilotClient;
use crate::server::{AppState, build_router};
use crate::session::manager::SessionManager;

/// Try to enable ANSI virtual terminal processing on Windows console.
/// Returns true if ANSI is supported (either natively or after enabling).
fn try_enable_ansi() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let stderr = std::io::stderr();
        let handle = stderr.as_raw_handle();
        // Try to enable virtual terminal processing
        unsafe {
            const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
            let mut mode: u32 = 0;
            if GetConsoleMode(handle as *mut _, &mut mode as *mut _) != 0 {
                if SetConsoleMode(
                    handle as *mut _,
                    mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                ) != 0
                {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(windows))]
    {
        // On Unix, most terminals support ANSI natively
        std::io::stderr().is_terminal()
    }
}

#[cfg(windows)]
unsafe extern "system" {
    fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
    fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI arguments
    let config = Config::parse();

    // Initialize logging
    let filter = if config.debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    // Determine if ANSI colors should be enabled:
    // 1. If --no-color flag is set, disable
    // 2. If not a terminal, disable
    // 3. On Windows, try to enable virtual terminal processing
    let use_ansi = if config.no_color {
        false
    } else if !std::io::stderr().is_terminal() {
        false
    } else {
        try_enable_ansi()
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(use_ansi)
        .init();

    info!("starting copilot openai proxy");
    info!("config: host={}, port={}, debug={}", config.host, config.port, config.debug);

    // Create core components
    let config = Arc::new(config);
    let copilot_client = Arc::new(CopilotClient::new(Arc::clone(&config))?);
    let session_manager = Arc::new(SessionManager::new(
        Arc::clone(&config),
        Arc::clone(&copilot_client),
    ));

    // Start background session cleanup
    session_manager.clone().spawn_cleanup_task();

    // Build app state and router
    let state = AppState {
        config: Arc::clone(&config),
        session_manager,
        copilot_client,
    };
    let app = build_router(state);

    // Bind and serve
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("listening on {addr}");

    // Graceful shutdown on Ctrl+C
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("server shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    info!("received shutdown signal");
}
