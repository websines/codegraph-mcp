use serde_json::json;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

use super::protocol::*;
use super::tools::{ToolContext, ToolRegistry};
use super::transport::Handler;
use crate::code::{CrossLanguageInferrer, Indexer};
use crate::config::Config;
use crate::learning::failures::FailureStore;
use crate::learning::lineage::LineageStore;
use crate::learning::niches::NicheStore;
use crate::learning::patterns::PatternStore;
use crate::session::SessionManager;
use crate::skill::distill::ManualInstructionStore;
use crate::store::{CodeGraph, Store};

const MCP_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "codegraph";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Server {
    tools: ToolRegistry,
    initialized: std::sync::atomic::AtomicBool,
}

impl Server {
    pub fn with_dependencies(
        store: Arc<Store>,
        config: Arc<Config>,
        indexer: Arc<Indexer>,
        graph: Arc<RwLock<CodeGraph>>,
        session_manager: Arc<SessionManager>,
        pattern_store: Arc<PatternStore>,
        failure_store: Arc<FailureStore>,
        lineage_store: Arc<LineageStore>,
        niche_store: Arc<NicheStore>,
        manual_instruction_store: Arc<ManualInstructionStore>,
        cross_language_inferrer: Arc<CrossLanguageInferrer>,
    ) -> Self {
        let ctx = Arc::new(ToolContext {
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
        });

        Self {
            tools: ToolRegistry::new(ctx),
            initialized: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn handle_initialize(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: InitializeParams = match request.params {
            Some(p) => match serde_json::from_value(p) {
                Ok(params) => params,
                Err(e) => {
                    return JsonRpcResponse::error(
                        request.id,
                        INVALID_PARAMS,
                        format!("Invalid initialize params: {}", e),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(
                    request.id,
                    INVALID_PARAMS,
                    "Missing initialize params".to_string(),
                );
            }
        };

        debug!(
            "Client initialized: {} v{}",
            params.client_info.name, params.client_info.version
        );

        let result = InitializeResult {
            protocol_version: MCP_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(json!({})),
                experimental: None,
            },
            server_info: ServerInfo {
                name: SERVER_NAME.to_string(),
                version: SERVER_VERSION.to_string(),
            },
        };

        self.initialized
            .store(true, std::sync::atomic::Ordering::Relaxed);

        match serde_json::to_value(result) {
            Ok(v) => JsonRpcResponse::success(request.id, v),
            Err(e) => JsonRpcResponse::error(request.id, INTERNAL_ERROR, format!("Serialization failed: {}", e)),
        }
    }

    fn handle_tools_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let tools = self.tools.list();

        #[derive(serde::Serialize)]
        struct ToolsListResult {
            tools: Vec<Tool>,
        }

        let result = ToolsListResult { tools };

        match serde_json::to_value(result) {
            Ok(v) => JsonRpcResponse::success(request.id, v),
            Err(e) => JsonRpcResponse::error(request.id, INTERNAL_ERROR, format!("Serialization failed: {}", e)),
        }
    }

    async fn handle_tool_call(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let tool_call: ToolCall = match request.params {
            Some(p) => match serde_json::from_value(p) {
                Ok(call) => call,
                Err(e) => {
                    return JsonRpcResponse::error(
                        request.id,
                        INVALID_PARAMS,
                        format!("Invalid tool call params: {}", e),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(
                    request.id,
                    INVALID_PARAMS,
                    "Missing tool call params".to_string(),
                );
            }
        };

        debug!("Tool call: {}", tool_call.name);

        let result = match self.tools.execute(&tool_call.name, tool_call.arguments).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Tool execution failed: {}", e);
                ToolResult::error(format!("Tool execution failed: {}", e))
            }
        };

        match serde_json::to_value(result) {
            Ok(v) => JsonRpcResponse::success(request.id, v),
            Err(e) => JsonRpcResponse::error(request.id, INTERNAL_ERROR, format!("Serialization failed: {}", e)),
        }
    }

    fn handle_initialized(&self, _request: JsonRpcRequest) -> JsonRpcResponse {
        debug!("Received initialized notification");
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: None,
            error: None,
        }
    }
}

impl Handler for Server {
    async fn handle(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request),
            "notifications/initialized" => self.handle_initialized(request),
            "tools/list" => self.handle_tools_list(request),
            "tools/call" => self.handle_tool_call(request).await,
            _ => {
                warn!("Unknown method: {}", request.method);
                JsonRpcResponse::error(
                    request.id,
                    METHOD_NOT_FOUND,
                    format!("Method not found: {}", request.method),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Indexer;
    use crate::config::Config;
    use crate::session::SessionManager;
    use crate::store::CodeGraph;
    use serde_json::Value;
    use tempfile::TempDir;

    async fn setup_test_server() -> (Server, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let config = Arc::new(Config {
            project_root: temp_path.to_path_buf(),
            cache_dir: temp_path.join("cache"),
            codegraph_dir: temp_path.join(".codegraph"),
            store_db_path: temp_path.join("cache/store.db"),
            learning_db_path: temp_path.join(".codegraph/learning.db"),
            settings: crate::config::ConfigFile::default(),
        });

        config.ensure_dirs().unwrap();

        let store = Arc::new(Store::open(&config).await.unwrap());
        let graph = Arc::new(RwLock::new(CodeGraph::load_from_store(&store).await.unwrap()));
        let indexer = Arc::new(Indexer::new(store.clone(), config.clone()));
        let session_manager = Arc::new(SessionManager::new(store.clone(), graph.clone()));

        let pattern_store = Arc::new(PatternStore::new(Arc::new(store.learning_db.clone())));
        let failure_store = Arc::new(FailureStore::new(Arc::new(store.learning_db.clone())));
        let lineage_store = Arc::new(LineageStore::new(Arc::new(store.learning_db.clone())));
        let niche_store = Arc::new(NicheStore::new(Arc::new(store.learning_db.clone())));
        let manual_instruction_store = Arc::new(ManualInstructionStore::new(Arc::new(store.learning_db.clone())));
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

    #[tokio::test]
    async fn test_initialize() {
        let (server, _temp) = setup_test_server().await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(1)),
            method: "initialize".to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                }
            })),
        };

        let response = server.handle(request).await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_tools_list() {
        let (server, _temp) = setup_test_server().await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(2)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = server.handle(request).await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());

        // Verify tools are present
        let result = response.result.unwrap();
        let tools = result.get("tools").unwrap().as_array().unwrap();
        assert!(tools.len() >= 10);
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let (server, _temp) = setup_test_server().await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(3)),
            method: "unknown/method".to_string(),
            params: None,
        };

        let response = server.handle(request).await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_tool_call_search() {
        let (server, _temp) = setup_test_server().await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(4)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "search_symbols",
                "arguments": {"query": "test"}
            })),
        };

        let response = server.handle(request).await;
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }
}
