use codegraph::code::{CrossLanguageInferrer, Indexer};
use codegraph::config::Config;
use codegraph::learning::failures::FailureStore;
use codegraph::learning::lineage::LineageStore;
use codegraph::learning::niches::NicheStore;
use codegraph::learning::patterns::PatternStore;
use codegraph::mcp::protocol::JsonRpcRequest;
use codegraph::mcp::transport::Handler;
use codegraph::mcp::Server;
use codegraph::session::SessionManager;
use codegraph::skill::distill::ManualInstructionStore;
use codegraph::store::{CodeGraph, Store};
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};
use tempfile::TempDir;

async fn setup_server_with_project() -> (Server, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a mini Rust project
    std::fs::create_dir_all(temp_path.join("src")).unwrap();
    std::fs::create_dir_all(temp_path.join(".git")).unwrap();

    std::fs::write(
        temp_path.join("src/main.rs"),
        r#"
mod utils;

fn main() {
    let result = utils::add(1, 2);
    println!("{}", result);
}
"#,
    )
    .unwrap();

    std::fs::write(
        temp_path.join("src/utils.rs"),
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"#,
    )
    .unwrap();

    let config = Arc::new(Config::from_path(temp_path).unwrap());
    config.ensure_dirs().unwrap();

    let store = Arc::new(Store::open(&config).await.unwrap());
    let graph = Arc::new(RwLock::new(
        CodeGraph::load_from_store(&store).await.unwrap(),
    ));
    let indexer = Arc::new(Indexer::new(store.clone(), config.clone()));
    let session_manager = Arc::new(SessionManager::new(store.clone(), graph.clone()));
    let pattern_store = Arc::new(PatternStore::new(Arc::new(store.learning_db.clone())));
    let failure_store = Arc::new(FailureStore::new(Arc::new(store.learning_db.clone())));
    let lineage_store = Arc::new(LineageStore::new(Arc::new(store.learning_db.clone())));
    let niche_store = Arc::new(NicheStore::new(Arc::new(store.learning_db.clone())));
    let manual_instruction_store =
        Arc::new(ManualInstructionStore::new(Arc::new(store.learning_db.clone())));
    let cross_language_inferrer = Arc::new(CrossLanguageInferrer::new(store.clone()));

    let server = Server::with_dependencies(
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
    (server, temp_dir)
}

fn make_request(id: i64, method: &str, params: Option<Value>) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(id)),
        method: method.to_string(),
        params,
    }
}

#[tokio::test]
async fn test_full_lifecycle_index_search_neighbors() {
    let (server, _temp) = setup_server_with_project().await;

    // 1. Initialize
    let resp = server
        .handle(make_request(
            1,
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0.0" }
            })),
        ))
        .await;
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());

    // 2. List tools
    let resp = server
        .handle(make_request(2, "tools/list", None))
        .await;
    let tools = resp.result.unwrap();
    let tool_list = tools["tools"].as_array().unwrap();
    assert!(tool_list.len() >= 26);

    // 3. Index project
    let resp = server
        .handle(make_request(
            3,
            "tools/call",
            Some(json!({ "name": "index_project", "arguments": { "full": true } })),
        ))
        .await;
    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    let content = result["content"][0]["text"].as_str().unwrap();
    assert!(content.contains("Indexed"));
    assert!(content.contains("symbols"));

    // 4. Search for the 'add' function
    let resp = server
        .handle(make_request(
            4,
            "tools/call",
            Some(json!({ "name": "search_symbols", "arguments": { "query": "add" } })),
        ))
        .await;
    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    let content = result["content"][0]["text"].as_str().unwrap();
    assert!(content.contains("add"));
    assert!(content.contains("function"));

    // 5. Get file symbols
    let resp = server
        .handle(make_request(
            5,
            "tools/call",
            Some(json!({ "name": "get_file_symbols", "arguments": { "path": "src/utils.rs" } })),
        ))
        .await;
    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    let content = result["content"][0]["text"].as_str().unwrap();
    assert!(content.contains("add"));
    assert!(content.contains("multiply"));
}

#[tokio::test]
async fn test_unknown_method_returns_error() {
    let (server, _temp) = setup_server_with_project().await;

    let resp = server
        .handle(make_request(1, "nonexistent/method", None))
        .await;
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32601); // METHOD_NOT_FOUND
}
