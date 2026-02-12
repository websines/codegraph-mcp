use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

use crate::store::{CodeGraph, Store};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub what: String,
    pub why: String,
    pub related_symbols: Vec<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub modified_files: Vec<String>,
    pub working_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub task: String,
    pub items: Vec<TaskItem>,
    pub decisions: Vec<Decision>,
    pub context: SessionContext,
    pub blockers: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartContextResult {
    pub task: String,
    pub current_item: Option<TaskItem>,
    pub progress: String,
    pub recent_decisions: Vec<Decision>,
    pub working_symbols: Vec<String>,
    pub related_symbols: Vec<String>,
    pub files_modified: Vec<String>,
    pub blockers: Vec<String>,
}

pub struct SessionManager {
    store: Arc<Store>,
    graph: Arc<std::sync::RwLock<CodeGraph>>,
}

impl SessionManager {
    pub fn new(store: Arc<Store>, graph: Arc<std::sync::RwLock<CodeGraph>>) -> Self {
        Self { store, graph }
    }

    /// Start a new session
    pub async fn start_session(&self, task: &str, items: &[String]) -> Result<Session> {
        debug!("Starting new session: {}", task);

        // Clear old session first
        self.clear_session().await?;

        // Create session task node
        let session_id = "session::current";
        self.store
            .upsert_node(
                session_id,
                "session",
                "task",
                &json!({
                    "description": task,
                }),
            )
            .await?;

        // Create task items
        let mut task_items = Vec::new();
        for (idx, item_desc) in items.iter().enumerate() {
            let item_id = format!("session::item::{}", idx);
            task_items.push(TaskItem {
                id: item_id.clone(),
                description: item_desc.clone(),
                status: TaskStatus::Pending,
                blockers: Vec::new(),
            });

            self.store
                .upsert_node(
                    &item_id,
                    "session",
                    "item",
                    &json!({
                        "description": item_desc,
                        "status": "pending",
                    }),
                )
                .await?;

            // Link to session
            self.store
                .upsert_edge(&session_id, &item_id, "has_item", "session", None)
                .await?;
        }

        // Create context node
        let context_id = "session::context";
        self.store
            .upsert_node(
                context_id,
                "session",
                "context",
                &json!({
                    "modified_files": [],
                    "working_symbols": [],
                }),
            )
            .await?;

        self.store
            .upsert_edge(session_id, context_id, "has_context", "session", None)
            .await?;

        Ok(Session {
            task: task.to_string(),
            items: task_items,
            decisions: Vec::new(),
            context: SessionContext {
                modified_files: Vec::new(),
                working_symbols: Vec::new(),
            },
            blockers: Vec::new(),
            notes: Vec::new(),
        })
    }

    /// Get current session
    pub async fn get_session(&self) -> Result<Option<Session>> {
        let session_id = "session::current";

        // Check if session exists
        let session_node = match self.store.get_node(session_id).await? {
            Some(node) => node,
            None => return Ok(None),
        };

        let task = session_node
            .data
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Get task items
        let item_edges = self.store.get_edges_from(session_id).await?;
        let mut items = Vec::new();

        for edge in &item_edges {
            if edge.kind == "has_item" {
                if let Some(item_node) = self.store.get_node(&edge.target).await? {
                    items.push(TaskItem {
                        id: item_node.id.clone(),
                        description: item_node
                            .data
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        status: parse_status(
                            item_node
                                .data
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("pending"),
                        ),
                        blockers: item_node
                            .data
                            .get("blockers")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    });
                }
            }
        }

        // Get decisions
        let mut decisions = Vec::new();
        for edge in &item_edges {
            if edge.kind == "has_decision" {
                if let Some(decision_node) = self.store.get_node(&edge.target).await? {
                    decisions.push(Decision {
                        id: decision_node.id.clone(),
                        what: decision_node
                            .data
                            .get("what")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        why: decision_node
                            .data
                            .get("why")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        related_symbols: decision_node
                            .data
                            .get("related_symbols")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        timestamp: decision_node
                            .data
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    });
                }
            }
        }

        // Get context
        let context_id = "session::context";
        let context = if let Some(context_node) = self.store.get_node(context_id).await? {
            SessionContext {
                modified_files: context_node
                    .data
                    .get("modified_files")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                working_symbols: context_node
                    .data
                    .get("working_symbols")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        } else {
            SessionContext {
                modified_files: Vec::new(),
                working_symbols: Vec::new(),
            }
        };

        Ok(Some(Session {
            task,
            items,
            decisions,
            context,
            blockers: Vec::new(),
            notes: Vec::new(),
        }))
    }

    /// Update task status or add/remove items
    pub async fn update_task(
        &self,
        item_id: Option<&str>,
        status: Option<TaskStatus>,
        add_item: Option<&str>,
        add_blocker: Option<&str>,
        remove_blocker: Option<&str>,
    ) -> Result<Session> {
        // Update existing item status
        if let (Some(id), Some(new_status)) = (item_id, status) {
            if let Some(mut node) = self.store.get_node(id).await? {
                node.data["status"] = json!(status_to_str(&new_status));

                if let Some(blocker) = add_blocker {
                    let mut blockers: Vec<String> = node
                        .data
                        .get("blockers")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    if !blockers.contains(&blocker.to_string()) {
                        blockers.push(blocker.to_string());
                    }
                    node.data["blockers"] = json!(blockers);
                }

                if let Some(blocker) = remove_blocker {
                    let mut blockers: Vec<String> = node
                        .data
                        .get("blockers")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    blockers.retain(|b| b != blocker);
                    node.data["blockers"] = json!(blockers);
                }

                self.store
                    .upsert_node(&node.id, &node.graph, &node.kind, &node.data)
                    .await?;
            }
        }

        // Add new item
        if let Some(desc) = add_item {
            let session_id = "session::current";
            let item_id = format!("session::item::{}", Uuid::new_v4());

            self.store
                .upsert_node(
                    &item_id,
                    "session",
                    "item",
                    &json!({
                        "description": desc,
                        "status": "pending",
                    }),
                )
                .await?;

            self.store
                .upsert_edge(&session_id, &item_id, "has_item", "session", None)
                .await?;
        }

        self.get_session()
            .await?
            .context("Session not found after update")
    }

    /// Add a decision to the session
    pub async fn add_decision(
        &self,
        what: &str,
        why: &str,
        related_symbols: &[String],
    ) -> Result<()> {
        let session_id = "session::current";
        let decision_id = format!("session::decision::{}", Uuid::new_v4());

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        self.store
            .upsert_node(
                &decision_id,
                "session",
                "decision",
                &json!({
                    "what": what,
                    "why": why,
                    "related_symbols": related_symbols,
                    "timestamp": timestamp,
                }),
            )
            .await?;

        self.store
            .upsert_edge(&session_id, &decision_id, "has_decision", "session", None)
            .await?;

        // Link to related symbols in code graph (resolve names to node IDs)
        for symbol in related_symbols {
            // Try the symbol name as-is first (might already be qualified like file::name)
            let resolved = if self.store.get_node(symbol).await?.is_some() {
                Some(symbol.clone())
            } else {
                // Search for a node whose ID ends with ::symbol_name
                self.store.find_node_by_suffix(symbol).await?
            };

            if let Some(node_id) = resolved {
                self.store
                    .upsert_edge(&decision_id, &node_id, "related_to", "cross", None)
                    .await?;
            }
        }

        Ok(())
    }

    /// Update session context (files/symbols being worked on)
    pub async fn set_context(
        &self,
        add_file: Option<&str>,
        remove_file: Option<&str>,
        add_symbol: Option<&str>,
        remove_symbol: Option<&str>,
        _add_note: Option<&str>,
    ) -> Result<()> {
        let context_id = "session::context";

        let mut context = if let Some(node) = self.store.get_node(context_id).await? {
            node
        } else {
            // Create context if it doesn't exist
            self.store
                .upsert_node(
                    context_id,
                    "session",
                    "context",
                    &json!({
                        "modified_files": [],
                        "working_symbols": [],
                    }),
                )
                .await?;
            self.store.get_node(context_id).await?.context("Context node not found after creation")?
        };

        let mut files: Vec<String> = context
            .data
            .get("modified_files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let mut symbols: Vec<String> = context
            .data
            .get("working_symbols")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(file) = add_file {
            if !files.contains(&file.to_string()) {
                files.push(file.to_string());
            }
        }

        if let Some(file) = remove_file {
            files.retain(|f| f != file);
        }

        if let Some(symbol) = add_symbol {
            if !symbols.contains(&symbol.to_string()) {
                symbols.push(symbol.to_string());
            }
        }

        if let Some(symbol) = remove_symbol {
            symbols.retain(|s| s != symbol);
        }

        context.data["modified_files"] = json!(files);
        context.data["working_symbols"] = json!(symbols);

        self.store
            .upsert_node(&context.id, &context.graph, &context.kind, &context.data)
            .await?;

        Ok(())
    }

    /// Get smart context summary
    pub async fn smart_context(&self) -> Result<SmartContextResult> {
        let session = self
            .get_session()
            .await?
            .context("No active session")?;

        // Find current in-progress item
        let current_item = session
            .items
            .iter()
            .find(|item| item.status == TaskStatus::InProgress)
            .or_else(|| {
                session
                    .items
                    .iter()
                    .find(|item| item.status == TaskStatus::Pending)
            })
            .cloned();

        // Calculate progress
        let completed = session
            .items
            .iter()
            .filter(|item| item.status == TaskStatus::Completed)
            .count();
        let total = session.items.len();
        let progress = format!("{}/{} tasks completed", completed, total);

        // Get recent decisions (last 3)
        let recent_decisions: Vec<Decision> = session
            .decisions
            .iter()
            .rev()
            .take(3)
            .cloned()
            .collect();

        // Get related symbols (1-hop neighbors of working symbols)
        let graph = self.graph.read().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;
        let mut related_symbols = Vec::new();

        for symbol in &session.context.working_symbols {
            let neighbors = graph.neighbors(
                symbol,
                1,
                crate::store::Direction::Both,
                Some(&["calls", "imports"]),
            );

            for neighbor in neighbors {
                if let Some(name) = neighbor.node.data.get("name").and_then(|v| v.as_str()) {
                    if !related_symbols.contains(&name.to_string()) {
                        related_symbols.push(name.to_string());
                    }
                }
            }
        }

        Ok(SmartContextResult {
            task: session.task,
            current_item,
            progress,
            recent_decisions,
            working_symbols: session.context.working_symbols,
            related_symbols,
            files_modified: session.context.modified_files,
            blockers: session.blockers,
        })
    }

    async fn clear_session(&self) -> Result<()> {
        // Delete all session and cross-graph nodes/edges
        self.store.delete_graph("cross").await?;
        self.store.delete_graph("session").await?;
        Ok(())
    }
}

fn parse_status(s: &str) -> TaskStatus {
    match s {
        "pending" => TaskStatus::Pending,
        "in_progress" => TaskStatus::InProgress,
        "completed" => TaskStatus::Completed,
        "blocked" => TaskStatus::Blocked,
        _ => TaskStatus::Pending,
    }
}

fn status_to_str(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    async fn setup_test_manager() -> (SessionManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let config = Config {
            project_root: temp_path.to_path_buf(),
            cache_dir: temp_path.join("cache"),
            codegraph_dir: temp_path.join(".codegraph"),
            store_db_path: temp_path.join("cache/store.db"),
            learning_db_path: temp_path.join(".codegraph/learning.db"),
            settings: crate::config::ConfigFile::default(),
        };

        let store = Arc::new(Store::open(&config).await.unwrap());
        let graph = Arc::new(std::sync::RwLock::new(
            CodeGraph::load_from_store(&store).await.unwrap(),
        ));

        let manager = SessionManager::new(store, graph);
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_start_session() {
        let (manager, _temp) = setup_test_manager().await;

        let session = manager
            .start_session("Build feature X", &["Design API".to_string()])
            .await
            .unwrap();

        assert_eq!(session.task, "Build feature X");
        assert_eq!(session.items.len(), 1);
        assert_eq!(session.items[0].description, "Design API");
    }

    #[tokio::test]
    async fn test_update_task() {
        let (manager, _temp) = setup_test_manager().await;

        let session = manager
            .start_session("Test task", &["Item 1".to_string()])
            .await
            .unwrap();

        let item_id = session.items[0].id.clone();

        let updated = manager
            .update_task(
                Some(&item_id),
                Some(TaskStatus::InProgress),
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(updated.items[0].status, TaskStatus::InProgress);
    }

    #[tokio::test]
    async fn test_add_decision() {
        let (manager, _temp) = setup_test_manager().await;

        manager
            .start_session("Test task", &[])
            .await
            .unwrap();

        manager
            .add_decision("Use REST API", "Simpler than GraphQL", &[])
            .await
            .unwrap();

        let session = manager.get_session().await.unwrap().unwrap();
        assert_eq!(session.decisions.len(), 1);
        assert_eq!(session.decisions[0].what, "Use REST API");
    }
}
