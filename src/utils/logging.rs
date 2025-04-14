// src/utils/logging.rs
use tracing_subscriber::{fmt, EnvFilter};

/// Sets up the logging framework using tracing_subscriber.
/// Reads log level filters from the `RUST_LOG` environment variable.
/// Defaults to "info" if `RUST_LOG` is not set.
pub fn setup_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info")); // Default to INFO level

    fmt()
        .with_env_filter(filter)
        // .with_max_level(tracing::Level::DEBUG) // Or set a hardcoded max level
        .init();

    tracing::debug!("Logging setup complete.");
}