use clap::Parser;

/// Proxy that converts GitHub Copilot WebSocket protocol to OpenAI-compatible HTTP API
#[derive(Parser, Clone, Debug)]
#[command(name = "copilot-openai-proxy")]
pub struct Config {
    /// Listen host address
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Listen port
    #[arg(long, default_value = "8080")]
    pub port: String,

    /// API key; if set, requests must include Authorization: Bearer <key>
    #[arg(long, default_value = "", alias = "api-key")]
    pub api_key: String,

    /// Request timeout in seconds
    #[arg(long, default_value_t = 120)]
    pub timeout: u64,

    /// WebSocket connection timeout in seconds
    #[arg(long, default_value_t = 20, alias = "conn-timeout")]
    pub conn_timeout: u64,

    /// Session TTL in seconds, sessions are auto-cleaned after expiry
    #[arg(long, default_value_t = 1800, alias = "session-ttl")]
    pub session_ttl: u64,

    /// Session cleanup interval in seconds
    #[arg(long, default_value_t = 300, alias = "cleanup-interval")]
    pub cleanup_interval: u64,

    /// Maximum in-memory sessions, oldest evicted when exceeded
    #[arg(long, default_value_t = 1000, alias = "max-sessions")]
    pub max_sessions: usize,

    /// Print raw protocol logs
    #[arg(long, default_value_t = false)]
    pub debug: bool,

    /// Disable colored log output
    #[arg(long, default_value_t = false, alias = "no-color")]
    pub no_color: bool,
}
