use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::OnceCell;
use tracing::{debug, info, warn};

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
    tools: OnceCell<ToolRegistry>,
    initialized: std::sync::atomic::AtomicBool,
}

impl Server {
    pub fn new() -> Self {
        Self {
            tools: OnceCell::new(),
            initialized: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Extract project root from MCP initialize roots, falling back to current_dir
    fn resolve_project_root(params: &InitializeParams) -> PathBuf {
        if let Some(roots) = &params.roots {
            if let Some(root) = roots.first() {
                // MCP roots use file:// URIs
                let path_str = root.uri.strip_prefix("file://").unwrap_or(&root.uri);
                let path = PathBuf::from(path_str);
                if path.is_dir() {
                    info!("Using project root from MCP client roots: {:?}", path);
                    return path;
                }
                warn!("MCP root path is not a directory: {:?}, falling back to cwd", path);
            }
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        info!("No MCP roots provided, using current_dir: {:?}", cwd);
        cwd
    }

    /// Initialize all dependencies from the resolved project root
    async fn init_dependencies(project_root: &std::path::Path) -> Result<ToolRegistry, String> {
        let config = Arc::new(
            Config::from_path(project_root)
                .map_err(|e| format!("Failed to detect config: {}", e))?,
        );

        info!("Project root: {:?}", config.project_root);

        config
            .ensure_dirs()
            .map_err(|e| format!("Failed to ensure directories: {}", e))?;

        let store = Arc::new(
            Store::open(&config)
                .await
                .map_err(|e| format!("Failed to open databases: {}", e))?,
        );

        let graph = Arc::new(RwLock::new(
            CodeGraph::load_from_store(&store)
                .await
                .map_err(|e| format!("Failed to load code graph: {}", e))?,
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
            compression_analytics: tokio::sync::Mutex::new(
                crate::compress::CompressionAnalytics::new(),
            ),
        });

        Ok(ToolRegistry::new(ctx))
    }

    async fn handle_initialize(&self, request: JsonRpcRequest) -> JsonRpcResponse {
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

        // Resolve project root from MCP roots and init all deps
        let project_root = Self::resolve_project_root(&params);

        match Self::init_dependencies(&project_root).await {
            Ok(registry) => {
                let _ = self.tools.set(registry);
                info!("Dependencies initialized for project: {:?}", project_root);
            }
            Err(e) => {
                return JsonRpcResponse::error(
                    request.id,
                    INTERNAL_ERROR,
                    format!("Failed to initialize: {}", e),
                );
            }
        }

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
            Err(e) => JsonRpcResponse::error(
                request.id,
                INTERNAL_ERROR,
                format!("Serialization failed: {}", e),
            ),
        }
    }

    fn handle_tools_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let Some(tools_registry) = self.tools.get() else {
            return JsonRpcResponse::error(
                request.id,
                INTERNAL_ERROR,
                "Server not initialized yet".to_string(),
            );
        };

        let tools = tools_registry.list();

        #[derive(serde::Serialize)]
        struct ToolsListResult {
            tools: Vec<Tool>,
        }

        let result = ToolsListResult { tools };

        match serde_json::to_value(result) {
            Ok(v) => JsonRpcResponse::success(request.id, v),
            Err(e) => JsonRpcResponse::error(
                request.id,
                INTERNAL_ERROR,
                format!("Serialization failed: {}", e),
            ),
        }
    }

    async fn handle_tool_call(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let Some(tools_registry) = self.tools.get() else {
            return JsonRpcResponse::error(
                request.id,
                INTERNAL_ERROR,
                "Server not initialized yet".to_string(),
            );
        };

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

        let result = match tools_registry
            .execute(&tool_call.name, tool_call.arguments)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                warn!("Tool execution failed: {}", e);
                ToolResult::error(format!("Tool execution failed: {}", e))
            }
        };

        match serde_json::to_value(result) {
            Ok(v) => JsonRpcResponse::success(request.id, v),
            Err(e) => JsonRpcResponse::error(
                request.id,
                INTERNAL_ERROR,
                format!("Serialization failed: {}", e),
            ),
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
            "initialize" => self.handle_initialize(request).await,
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
    use serde_json::Value;
    use tempfile::TempDir;

    async fn setup_test_server() -> (Server, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create a .git dir so find_project_root works
        std::fs::create_dir_all(temp_path.join(".git")).unwrap();

        let server = Server::new();

        // Simulate initialize with roots pointing to temp dir
        let init_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(0)),
            method: "initialize".to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                },
                "roots": [{
                    "uri": format!("file://{}", temp_path.display()),
                    "name": "test-project"
                }]
            })),
        };

        let response = server.handle(init_request).await;
        assert!(response.error.is_none(), "Initialize failed: {:?}", response.error);

        (server, temp_dir)
    }

    #[tokio::test]
    async fn test_initialize() {
        let (server, _temp) = setup_test_server().await;

        // Verify server is initialized
        assert!(server.tools.get().is_some());
    }

    #[tokio::test]
    async fn test_initialize_with_roots() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        std::fs::create_dir_all(temp_path.join(".git")).unwrap();

        let server = Server::new();

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
                },
                "roots": [{
                    "uri": format!("file://{}", temp_path.display()),
                    "name": "test-project"
                }]
            })),
        };

        let response = server.handle(request).await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());

        // Verify .codegraph was created in the right place
        assert!(temp_path.join(".codegraph").exists());
        assert!(temp_path.join(".codegraph/config.toml").exists());
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
    async fn test_tools_list_before_init() {
        let server = Server::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(2)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = server.handle(request).await;

        // Should get error since not initialized
        assert!(response.error.is_some());
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
