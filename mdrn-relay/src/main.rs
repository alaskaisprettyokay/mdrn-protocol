//! MDRN Relay - Standalone relay node daemon

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    tracing::info!("MDRN Relay starting...");
    tracing::info!("Protocol: {}", mdrn_core::PROTOCOL_ID);

    // TODO: Parse config from CLI/env
    // TODO: Initialize transport layer
    // TODO: Start relay service
    // TODO: Handle graceful shutdown

    tracing::warn!("Relay daemon not yet implemented");

    Ok(())
}
