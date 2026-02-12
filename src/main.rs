mod code;
mod config;
mod learning;
mod mcp;
mod session;
mod skill;
mod store;

use anyhow::Result;
use std::sync::{Arc, RwLock};
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

    // Detect project configuration
    let config = Arc::new(config::Config::detect()?);
    tracing::info!("Project root: {:?}", config.project_root);

    // Open/create databases
    let store = Arc::new(store::Store::open(&config).await?);
    tracing::info!("Databases ready");

    // Load code graph
    let graph = Arc::new(RwLock::new(store::CodeGraph::load_from_store(&store).await?));
    tracing::info!("Code graph loaded: {} nodes", graph.read().map(|g| g.graph.node_count()).unwrap_or(0));

    // Create indexer
    let indexer = Arc::new(code::Indexer::new(store.clone(), config.clone()));

    // Create session manager
    let session_manager = Arc::new(session::SessionManager::new(store.clone(), graph.clone()));

    // Create learning stores
    let pattern_store = Arc::new(learning::patterns::PatternStore::new(Arc::new(store.learning_db.clone())));
    let failure_store = Arc::new(learning::failures::FailureStore::new(Arc::new(store.learning_db.clone())));
    let lineage_store = Arc::new(learning::lineage::LineageStore::new(Arc::new(store.learning_db.clone())));
    let niche_store = Arc::new(learning::niches::NicheStore::new(Arc::new(store.learning_db.clone())));
    let manual_instruction_store = Arc::new(skill::distill::ManualInstructionStore::new(Arc::new(store.learning_db.clone())));
    let cross_language_inferrer = Arc::new(code::CrossLanguageInferrer::new(store.clone()));

    // Create server with dependencies
    let server = mcp::Server::with_dependencies(
        store,
        config,
        indexer,
        graph,
        session_manager,
        pattern_store,
        failure_store,
        lineage_store,
        niche_store,
        manual_instruction_store,
        cross_language_inferrer,
    );

    // Run stdio transport
    mcp::run_stdio(server).await?;

    Ok(())
}
