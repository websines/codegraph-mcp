mod code;
mod compress;
mod config;
mod learning;
mod mcp;
mod session;
mod skill;
mod store;

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing - logs to stderr
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "codegraph=debug,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting Codegraph MCP server v{}", env!("CARGO_PKG_VERSION"));

    // Create server — deps will be initialized lazily on MCP initialize handshake,
    // using the project root from the client's roots parameter
    let server = mcp::Server::new();

    // Run stdio transport
    mcp::run_stdio(server).await?;

    Ok(())
}
