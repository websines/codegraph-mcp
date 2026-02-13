use anyhow::Result;
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

use super::protocol::{Tool, ToolResult};
use crate::code::{CrossLanguageInferrer, Indexer};
use crate::compress::{CompressionAnalytics, CompressConfig};
use crate::config::Config;
use crate::learning::failures::FailureStore;
use crate::learning::lineage::LineageStore;
use crate::learning::niches::NicheStore;
use crate::learning::patterns::PatternStore;
use crate::session::SessionManager;
use crate::skill::distill::ManualInstructionStore;
use crate::store::{CodeGraph, Store};

/// Shared dependencies available to all tool handlers
pub struct ToolContext {
    pub store: Arc<Store>,
    pub config: Arc<Config>,
    pub indexer: Arc<Indexer>,
    pub graph: Arc<RwLock<CodeGraph>>,
    pub session_manager: Arc<SessionManager>,
    pub pattern_store: Arc<PatternStore>,
    pub failure_store: Arc<FailureStore>,
    pub lineage_store: Arc<LineageStore>,
    pub niche_store: Arc<NicheStore>,
    pub manual_instruction_store: Arc<ManualInstructionStore>,
    pub cross_language_inferrer: Arc<CrossLanguageInferrer>,
    pub compression_analytics: Mutex<CompressionAnalytics>,
}

pub struct ToolRegistry {
    ctx: Arc<ToolContext>,
}

impl ToolRegistry {
    pub fn new(ctx: Arc<ToolContext>) -> Self {
        Self { ctx }
    }

    pub fn list(&self) -> Vec<Tool> {
        vec![
            // Code Graph tools
            Tool {
                name: "index_project".into(),
                description: "Rebuild the code graph. Run if files changed outside this session or after major refactoring.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "full": { "type": "boolean", "default": false, "description": "Force full rebuild vs incremental" }
                    }
                }),
            },
            Tool {
                name: "search_symbols".into(),
                description: "Find symbols by name. Returns signatures and locations. Start here before reading files.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Symbol name (partial match)" },
                        "kind": { "type": "string", "enum": ["function", "method", "class", "struct", "interface", "trait", "type", "variable", "const", "static", "module", "enum", "impl"] },
                        "file_pattern": { "type": "string", "description": "Filter by file path substring" },
                        "limit": { "type": "integer", "default": 10, "maximum": 50 }
                    },
                    "required": ["query"]
                }),
            },
            Tool {
                name: "get_file_symbols".into(),
                description: "List all symbols defined in a file with signatures. Use before reading full file to understand structure.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path relative to project root" }
                    },
                    "required": ["path"]
                }),
            },
            Tool {
                name: "get_neighbors".into(),
                description: "Get symbols connected to a given symbol (callers, callees, imports, type usage). Use to understand dependencies and impact.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Symbol ID from search_symbols" },
                        "depth": { "type": "integer", "default": 1, "minimum": 1, "maximum": 3 },
                        "direction": { "type": "string", "enum": ["outgoing", "incoming", "both"], "default": "both" },
                        "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter by edge type: calls, imports, inherits, etc." }
                    },
                    "required": ["id"]
                }),
            },
            // Session tools
            Tool {
                name: "start_session".into(),
                description: "Start a new session with a task description.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string", "description": "What you're trying to accomplish" },
                        "items": { "type": "array", "items": { "type": "string" }, "description": "Breakdown of subtasks" }
                    },
                    "required": ["task"]
                }),
            },
            Tool {
                name: "get_session".into(),
                description: "Load current session state: task, items, decisions, context.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "update_task".into(),
                description: "Update task items: mark complete, add new items, change status.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "item_id": { "type": "string" },
                        "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "blocked"] },
                        "add_item": { "type": "string", "description": "New item text to add" },
                        "add_blocker": { "type": "string" },
                        "remove_blocker": { "type": "string" }
                    }
                }),
            },
            Tool {
                name: "add_decision".into(),
                description: "Record a decision with reasoning. Persists across compaction.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "what": { "type": "string", "description": "The decision made" },
                        "why": { "type": "string", "description": "Reasoning behind it" },
                        "related_symbols": { "type": "array", "items": { "type": "string" }, "description": "Symbol names this relates to" }
                    },
                    "required": ["what", "why"]
                }),
            },
            Tool {
                name: "set_context".into(),
                description: "Update working context: files modified, symbols being worked on.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "add_file": { "type": "string" },
                        "remove_file": { "type": "string" },
                        "add_symbol": { "type": "string" },
                        "remove_symbol": { "type": "string" },
                        "add_note": { "type": "string" }
                    }
                }),
            },
            Tool {
                name: "smart_context".into(),
                description: "One-shot full context restoration. Returns task, decisions, working symbols with signatures, related symbols from graph. Call on startup and after compaction.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            // Learning System - Phase 4
            Tool {
                name: "recall_patterns".into(),
                description: "Query relevant patterns matching your current task/context. Returns high-confidence patterns with examples.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "description": { "type": "string", "description": "What you're trying to accomplish" },
                        "current_file": { "type": "string", "description": "Current file path" },
                        "symbols": { "type": "array", "items": { "type": "string" }, "description": "Relevant symbol names" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags (e.g., 'async', 'db')" },
                        "limit": { "type": "integer", "default": 5, "maximum": 20 }
                    },
                    "required": ["description"]
                }),
            },
            Tool {
                name: "recall_failures".into(),
                description: "Query relevant failures to avoid. Always includes critical failures, filters others by scope.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "description": { "type": "string", "description": "What you're trying to accomplish" },
                        "current_file": { "type": "string", "description": "Current file path" },
                        "symbols": { "type": "array", "items": { "type": "string" }, "description": "Relevant symbol names" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags" }
                    },
                    "required": ["description"]
                }),
            },
            Tool {
                name: "extract_pattern".into(),
                description: "Record a successful pattern for future reuse.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "intent": { "type": "string", "description": "What this pattern accomplishes" },
                        "mechanism": { "type": "string", "description": "How it works (optional)" },
                        "examples": { "type": "array", "items": { "type": "string" }, "description": "Code examples or descriptions" },
                        "scope_paths": { "type": "array", "items": { "type": "string" }, "description": "Include path patterns (globs)" },
                        "scope_tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for categorization" },
                        "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": 0.7 }
                    },
                    "required": ["intent", "examples"]
                }),
            },
            Tool {
                name: "record_failure".into(),
                description: "Record a failure/gotcha to prevent future mistakes.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "cause": { "type": "string", "description": "What went wrong" },
                        "avoidance_rule": { "type": "string", "description": "How to avoid it" },
                        "severity": { "type": "string", "enum": ["critical", "major", "minor"], "default": "minor" },
                        "scope_paths": { "type": "array", "items": { "type": "string" }, "description": "Include path patterns" },
                        "scope_tags": { "type": "array", "items": { "type": "string" }, "description": "Tags" }
                    },
                    "required": ["cause", "avoidance_rule"]
                }),
            },
            // Learning System - Phase 5
            Tool {
                name: "record_attempt".into(),
                description: "Start tracking a solution attempt. Returns solution ID for later outcome recording.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string", "description": "Task description" },
                        "plan": { "type": "string", "description": "Approach being taken" },
                        "approach": { "type": "string", "description": "Named approach (optional)" },
                        "parent_id": { "type": "string", "description": "Parent solution ID if this is a retry" }
                    },
                    "required": ["task", "plan"]
                }),
            },
            Tool {
                name: "record_outcome".into(),
                description: "Record the outcome of a solution attempt.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Solution ID from record_attempt" },
                        "outcome": { "type": "string", "enum": ["success", "failure", "partial"] },
                        "files": { "type": "array", "items": { "type": "string" }, "description": "Files modified" },
                        "symbols": { "type": "array", "items": { "type": "string" }, "description": "Symbols modified" }
                    },
                    "required": ["id", "outcome"]
                }),
            },
            Tool {
                name: "reflect".into(),
                description: "Reflect on a solution to create pattern or failure record.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "attempt_id": { "type": "string", "description": "Solution ID from record_attempt" },
                        "intent": { "type": "string", "description": "What you were trying to accomplish" },
                        "mechanism": { "type": "string", "description": "How it works (optional)" },
                        "root_cause": { "type": "string", "description": "Root cause if it failed" },
                        "lesson": { "type": "string", "description": "Lesson learned (use 'When X, do Y because Z' format)" },
                        "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "scope_paths": { "type": "array", "items": { "type": "string" } },
                        "scope_tags": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["attempt_id", "intent", "root_cause", "lesson"]
                }),
            },
            Tool {
                name: "query_lineage".into(),
                description: "Query past solution attempts for a task.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string", "description": "Task description to search for" },
                        "include_failures": { "type": "boolean", "default": false },
                        "limit": { "type": "integer", "default": 10, "maximum": 50 }
                    },
                    "required": ["task"]
                }),
            },
            Tool {
                name: "suggest_approach".into(),
                description: "Get suggestions based on patterns, failures, and past solutions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string", "description": "What you're trying to do" },
                        "current_file": { "type": "string" },
                        "constraints": { "type": "array", "items": { "type": "string" }, "description": "Constraints or requirements" }
                    },
                    "required": ["task"]
                }),
            },
            // Phase 6: Behavioral Niches
            Tool {
                name: "list_niches".into(),
                description: "List all behavioral niches with their best solutions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task_type": { "type": "string", "description": "Filter by task type" }
                    }
                }),
            },
            // Phase 7: Skill Distillation
            Tool {
                name: "distill_project_skill".into(),
                description: "Generate SKILL.md from patterns, failures, and conventions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "confidence_threshold": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": 0.7, "description": "Min confidence for patterns" },
                        "write_file": { "type": "boolean", "default": true, "description": "Write to .codegraph/SKILL.md" }
                    }
                }),
            },
            Tool {
                name: "add_instruction".into(),
                description: "Add a manual instruction to the project skill file.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "instruction": { "type": "string", "description": "The instruction text" },
                        "category": { "type": "string", "enum": ["architecture", "testing", "style", "navigation", "workflow", "tooling", "gotchas"], "description": "Category" },
                        "reason": { "type": "string", "description": "Why this instruction is needed" }
                    },
                    "required": ["instruction", "category"]
                }),
            },
            Tool {
                name: "get_project_instructions".into(),
                description: "List all manual project instructions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            // Phase 8: Cross-Language Inference
            Tool {
                name: "infer_cross_edges".into(),
                description: "Infer cross-language edges (e.g., frontend API calls to backend routes).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "force_rebuild": { "type": "boolean", "default": false, "description": "Force full rebuild vs incremental" }
                    }
                }),
            },
            Tool {
                name: "get_api_connections".into(),
                description: "Get API connections for a specific file path.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path" }
                    },
                    "required": ["path"]
                }),
            },
            // Phase 9: Sync + Persistence
            Tool {
                name: "sync_learnings".into(),
                description: "Sync patterns and failures to JSON files in .codegraph/.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "confidence_threshold": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": 0.7, "description": "Min effective confidence for patterns" },
                        "include_all_critical": { "type": "boolean", "default": true, "description": "Include all critical failures" }
                    }
                }),
            },
            // RTK-style compression tools
            Tool {
                name: "bash_compressed".into(),
                description: "Execute a bash command with RTK-style output compression. Saves 60-90% tokens on git, ls, grep, test output.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The bash command to execute" },
                        "max_lines": { "type": "integer", "default": 50, "description": "Max lines before truncating" },
                        "max_items_per_group": { "type": "integer", "default": 10, "description": "Max items per category" }
                    },
                    "required": ["command"]
                }),
            },
            Tool {
                name: "compression_stats".into(),
                description: "Get token compression statistics. Shows total savings, by-category breakdown.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "reset": { "type": "boolean", "default": false, "description": "Reset statistics after returning" }
                    }
                }),
            },
        ]
    }

    pub async fn execute(&self, name: &str, args: Value) -> Result<ToolResult> {
        match name {
            "index_project" => self.handle_index_project(args).await,
            "search_symbols" => self.handle_search_symbols(args).await,
            "get_file_symbols" => self.handle_get_file_symbols(args).await,
            "get_neighbors" => self.handle_get_neighbors(args).await,
            "start_session" => self.handle_start_session(args).await,
            "get_session" => self.handle_get_session(args).await,
            "update_task" => self.handle_update_task(args).await,
            "add_decision" => self.handle_add_decision(args).await,
            "set_context" => self.handle_set_context(args).await,
            "smart_context" => self.handle_smart_context(args).await,
            "recall_patterns" => self.handle_recall_patterns(args).await,
            "recall_failures" => self.handle_recall_failures(args).await,
            "extract_pattern" => self.handle_extract_pattern(args).await,
            "record_failure" => self.handle_record_failure(args).await,
            "record_attempt" => self.handle_record_attempt(args).await,
            "record_outcome" => self.handle_record_outcome(args).await,
            "reflect" => self.handle_reflect(args).await,
            "query_lineage" => self.handle_query_lineage(args).await,
            "suggest_approach" => self.handle_suggest_approach(args).await,
            "list_niches" => self.handle_list_niches(args).await,
            "distill_project_skill" => self.handle_distill_project_skill(args).await,
            "add_instruction" => self.handle_add_instruction(args).await,
            "get_project_instructions" => self.handle_get_project_instructions(args).await,
            "infer_cross_edges" => self.handle_infer_cross_edges(args).await,
            "get_api_connections" => self.handle_get_api_connections(args).await,
            "sync_learnings" => self.handle_sync_learnings(args).await,
            "bash_compressed" => self.handle_bash_compressed(args).await,
            "compression_stats" => self.handle_compression_stats(args).await,
            _ => Ok(ToolResult::error(format!("Tool not found: {}", name))),
        }
    }

    // === Code Graph Tools ===

    async fn handle_index_project(&self, args: Value) -> Result<ToolResult> {
        let full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);

        let stats = if full {
            self.ctx.indexer.index_full().await?
        } else {
            self.ctx.indexer.index_incremental().await?
        };

        // Rebuild in-memory graph after indexing
        {
            let mut graph = self.ctx.graph.write().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;
            graph.rebuild_from_store(&self.ctx.store).await?;
        }

        let mut output = format!(
            "Indexed {} files ({} new/changed, {} skipped, {} removed)\n{} symbols, {} edges",
            stats.files_scanned,
            stats.files_indexed,
            stats.files_skipped,
            stats.files_removed,
            stats.symbols_found,
            stats.edges_found,
        );

        if stats.unresolved_before > 0 {
            output.push_str(&format!(
                "\nCross-file resolution: {}/{} resolved ({} remaining)",
                stats.resolved, stats.unresolved_before, stats.unresolved_after
            ));
        }

        output.push_str(&format!("\n({}ms)", stats.duration_ms));

        Ok(ToolResult::text(output))
    }

    async fn handle_search_symbols(&self, args: Value) -> Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if query.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: query"));
        }

        let kind = args.get("kind").and_then(|v| v.as_str());
        let file_pattern = args.get("file_pattern").and_then(|v| v.as_str());
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let graph = self.ctx.graph.read().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;
        let results = graph.search(query, kind, file_pattern, limit);

        if results.is_empty() {
            return Ok(ToolResult::text(format!("No symbols found matching '{}'", query)));
        }

        let mut output = String::new();
        for node in &results {
            let name = node.data.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let file = node.data.get("file").and_then(|v| v.as_str()).unwrap_or("?");
            let line = node.data.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0);
            let sig = node.data.get("signature").and_then(|v| v.as_str()).unwrap_or("");
            let kind_str = &node.kind;

            output.push_str(&format!(
                "[{}] {} ({}:{})\n  {}\n  id: {}\n\n",
                kind_str, name, file, line, sig, node.id
            ));
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    async fn handle_get_file_symbols(&self, args: Value) -> Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if path.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: path"));
        }

        let graph = self.ctx.graph.read().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;
        let symbols = graph.file_symbols(path);

        if symbols.is_empty() {
            return Ok(ToolResult::text(format!(
                "No symbols found in '{}'. Run index_project if the file was recently added.",
                path
            )));
        }

        let mut output = format!("## {}\n\n", path);
        for node in &symbols {
            let name = node.data.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let line_start = node.data.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0);
            let line_end = node.data.get("line_end").and_then(|v| v.as_u64()).unwrap_or(0);
            let sig = node.data.get("signature").and_then(|v| v.as_str()).unwrap_or("");

            output.push_str(&format!(
                "L{}-{} [{}] {}\n  {}\n",
                line_start, line_end, node.kind, name, sig
            ));
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    async fn handle_get_neighbors(&self, args: Value) -> Result<ToolResult> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if id.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: id"));
        }

        let depth = args
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;

        let direction = match args.get("direction").and_then(|v| v.as_str()) {
            Some("outgoing") => crate::store::Direction::Outgoing,
            Some("incoming") => crate::store::Direction::Incoming,
            _ => crate::store::Direction::Both,
        };

        let edge_filter: Option<Vec<String>> = args
            .get("edge_types")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });

        let edge_refs: Option<Vec<&str>> = edge_filter
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str()).collect());

        let graph = self.ctx.graph.read().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;
        let neighbors = graph.neighbors(id, depth, direction, edge_refs.as_deref());

        if neighbors.is_empty() {
            return Ok(ToolResult::text(format!(
                "No neighbors found for '{}' at depth {}",
                id, depth
            )));
        }

        let mut output = format!("## Neighbors of {}\n\n", id);
        for neighbor in &neighbors {
            let name = neighbor.node.data.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let file = neighbor.node.data.get("file").and_then(|v| v.as_str()).unwrap_or("?");
            let path_str = neighbor.path.join(" → ");

            output.push_str(&format!(
                "[{}] {} ({})\n  via: {}\n  distance: {}\n  id: {}\n\n",
                neighbor.node.kind, name, file, path_str, neighbor.distance, neighbor.node.id
            ));
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    // === Session Tools ===

    async fn handle_start_session(&self, args: Value) -> Result<ToolResult> {
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if task.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: task"));
        }

        let items: Vec<String> = args
            .get("items")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let session = self.ctx.session_manager.start_session(task, &items).await?;

        let mut output = format!("Session started: {}\n", session.task);
        if !session.items.is_empty() {
            output.push_str("\nItems:\n");
            for item in &session.items {
                output.push_str(&format!("  - [{}] {}\n", "pending", item.description));
            }
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    async fn handle_get_session(&self, _args: Value) -> Result<ToolResult> {
        match self.ctx.session_manager.get_session().await? {
            Some(session) => Ok(ToolResult::text(
                serde_json::to_string_pretty(&session)?,
            )),
            None => Ok(ToolResult::text("No active session. Use start_session to begin.")),
        }
    }

    async fn handle_update_task(&self, args: Value) -> Result<ToolResult> {
        let item_id = args.get("item_id").and_then(|v| v.as_str());
        let status = args.get("status").and_then(|v| v.as_str()).map(parse_task_status);
        let add_item = args.get("add_item").and_then(|v| v.as_str());
        let add_blocker = args.get("add_blocker").and_then(|v| v.as_str());
        let remove_blocker = args.get("remove_blocker").and_then(|v| v.as_str());

        let session = self
            .ctx
            .session_manager
            .update_task(item_id, status, add_item, add_blocker, remove_blocker)
            .await?;

        Ok(ToolResult::text(serde_json::to_string_pretty(&session)?))
    }

    async fn handle_add_decision(&self, args: Value) -> Result<ToolResult> {
        let what = args.get("what").and_then(|v| v.as_str()).unwrap_or("");
        let why = args.get("why").and_then(|v| v.as_str()).unwrap_or("");

        if what.is_empty() || why.is_empty() {
            return Ok(ToolResult::error(
                "Missing required parameters: what, why",
            ));
        }

        let related_symbols: Vec<String> = args
            .get("related_symbols")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        self.ctx
            .session_manager
            .add_decision(what, why, &related_symbols)
            .await?;

        Ok(ToolResult::text(format!(
            "Decision recorded: {} — {}",
            what, why
        )))
    }

    async fn handle_set_context(&self, args: Value) -> Result<ToolResult> {
        let add_file = args.get("add_file").and_then(|v| v.as_str());
        let remove_file = args.get("remove_file").and_then(|v| v.as_str());
        let add_symbol = args.get("add_symbol").and_then(|v| v.as_str());
        let remove_symbol = args.get("remove_symbol").and_then(|v| v.as_str());
        let add_note = args.get("add_note").and_then(|v| v.as_str());

        self.ctx
            .session_manager
            .set_context(add_file, remove_file, add_symbol, remove_symbol, add_note)
            .await?;

        Ok(ToolResult::text("Context updated"))
    }

    async fn handle_smart_context(&self, _args: Value) -> Result<ToolResult> {
        let result = self.ctx.session_manager.smart_context().await?;
        Ok(ToolResult::text(serde_json::to_string_pretty(&result)?))
    }

    // === Learning Tools - Phase 4 ===

    async fn handle_recall_patterns(&self, args: Value) -> Result<ToolResult> {
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if description.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: description"));
        }

        let current_file = args.get("current_file").and_then(|v| v.as_str()).map(String::from);
        let symbols: Vec<String> = args
            .get("symbols")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

        let context = crate::learning::QueryContext {
            description: description.to_string(),
            current_file,
            relevant_symbols: symbols,
            tags,
        };

        let mut patterns = self.ctx.pattern_store.query(&context, limit).await?;

        // Calculate effective confidence for each pattern
        let now = chrono::Utc::now().timestamp();
        let graph = self.ctx.graph.read().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;

        patterns.sort_by(|a, b| {
            let eff_a = crate::learning::confidence::effective_confidence(a, Some(&graph), now, 90);
            let eff_b = crate::learning::confidence::effective_confidence(b, Some(&graph), now, 90);
            eff_b.partial_cmp(&eff_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        if patterns.is_empty() {
            return Ok(ToolResult::text("No matching patterns found. Use extract_pattern to record new patterns."));
        }

        let mut output = String::from("# Relevant Patterns\n\n");
        for pattern in &patterns {
            let eff_conf = crate::learning::confidence::effective_confidence(&pattern, Some(&graph), now, 90);
            output.push_str(&format!(
                "## {} (confidence: {:.1}%)\n",
                pattern.intent,
                eff_conf * 100.0
            ));
            if let Some(mechanism) = &pattern.mechanism {
                output.push_str(&format!("**How:** {}\n\n", mechanism));
            }
            output.push_str("**Examples:**\n");
            for example in &pattern.examples {
                output.push_str(&format!("- {}\n", example));
            }
            output.push_str(&format!(
                "\n**Usage:** {} times ({} successful)\n\n",
                pattern.usage_count, pattern.success_count
            ));
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    async fn handle_recall_failures(&self, args: Value) -> Result<ToolResult> {
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if description.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: description"));
        }

        let current_file = args.get("current_file").and_then(|v| v.as_str()).map(String::from);
        let symbols: Vec<String> = args
            .get("symbols")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let context = crate::learning::QueryContext {
            description: description.to_string(),
            current_file,
            relevant_symbols: symbols,
            tags,
        };

        let failures = self.ctx.failure_store.query(&context, true).await?;

        if failures.is_empty() {
            return Ok(ToolResult::text("No matching failures found."));
        }

        let mut output = String::from("# Failures to Avoid\n\n");
        for failure in &failures {
            let severity_emoji = match failure.severity {
                crate::learning::failures::Severity::Critical => "🔴",
                crate::learning::failures::Severity::Major => "🟠",
                crate::learning::failures::Severity::Minor => "🟡",
            };
            output.push_str(&format!(
                "## {} {:?}: {}\n",
                severity_emoji, failure.severity, failure.cause
            ));
            output.push_str(&format!("**Avoidance:** {}\n\n", failure.avoidance_rule));
            if failure.times_prevented > 0 {
                output.push_str(&format!("*Prevented {} times*\n\n", failure.times_prevented));
            }
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    async fn handle_extract_pattern(&self, args: Value) -> Result<ToolResult> {
        let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
        if intent.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: intent"));
        }

        let mechanism = args.get("mechanism").and_then(|v| v.as_str()).map(String::from);
        let examples: Vec<String> = args
            .get("examples")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        if examples.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: examples"));
        }

        let scope_paths: Vec<String> = args
            .get("scope_paths")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let scope_tags: Vec<String> = args
            .get("scope_tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let confidence = args.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;

        let new_pattern = crate::learning::patterns::NewPattern {
            intent: intent.to_string(),
            mechanism,
            examples,
            scope: crate::learning::Scope {
                include_paths: scope_paths,
                exclude_paths: vec![],
                symbols: vec![],
                tags: scope_tags,
            },
            confidence,
        };

        let pattern = self.ctx.pattern_store.create(&new_pattern).await?;

        Ok(ToolResult::text(format!(
            "Pattern recorded:\n  Intent: {}\n  ID: {}\n  Confidence: {:.1}%",
            pattern.intent,
            pattern.id,
            pattern.confidence * 100.0
        )))
    }

    async fn handle_record_failure(&self, args: Value) -> Result<ToolResult> {
        let cause = args.get("cause").and_then(|v| v.as_str()).unwrap_or("");
        let avoidance_rule = args.get("avoidance_rule").and_then(|v| v.as_str()).unwrap_or("");

        if cause.is_empty() || avoidance_rule.is_empty() {
            return Ok(ToolResult::error(
                "Missing required parameters: cause, avoidance_rule",
            ));
        }

        let severity = match args.get("severity").and_then(|v| v.as_str()) {
            Some("critical") => crate::learning::failures::Severity::Critical,
            Some("major") => crate::learning::failures::Severity::Major,
            _ => crate::learning::failures::Severity::Minor,
        };

        let scope_paths: Vec<String> = args
            .get("scope_paths")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let scope_tags: Vec<String> = args
            .get("scope_tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let new_failure = crate::learning::failures::NewFailure {
            cause: cause.to_string(),
            avoidance_rule: avoidance_rule.to_string(),
            severity,
            scope: crate::learning::Scope {
                include_paths: scope_paths,
                exclude_paths: vec![],
                symbols: vec![],
                tags: scope_tags,
            },
        };

        let failure = self.ctx.failure_store.create(&new_failure).await?;

        Ok(ToolResult::text(format!(
            "Failure recorded:\n  Cause: {}\n  Severity: {:?}\n  ID: {}",
            failure.cause, failure.severity, failure.id
        )))
    }

    // === Learning Tools - Phase 5 ===

    async fn handle_record_attempt(&self, args: Value) -> Result<ToolResult> {
        let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
        let plan = args.get("plan").and_then(|v| v.as_str()).unwrap_or("");

        if task.is_empty() || plan.is_empty() {
            return Ok(ToolResult::error("Missing required parameters: task, plan"));
        }

        let approach = args.get("approach").and_then(|v| v.as_str());
        let parent_id = args.get("parent_id").and_then(|v| v.as_str());

        let solution_id = self
            .ctx
            .lineage_store
            .record_attempt(task, plan, approach, parent_id)
            .await?;

        Ok(ToolResult::text(format!(
            "Solution attempt recorded\nID: {}\nTask: {}\nPlan: {}",
            solution_id, task, plan
        )))
    }

    async fn handle_record_outcome(&self, args: Value) -> Result<ToolResult> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: id"));
        }

        let outcome = match args.get("outcome").and_then(|v| v.as_str()) {
            Some("success") => crate::learning::lineage::Outcome::Success,
            Some("failure") => crate::learning::lineage::Outcome::Failure,
            Some("partial") => crate::learning::lineage::Outcome::Partial,
            _ => return Ok(ToolResult::error("Invalid outcome (must be: success, failure, partial)")),
        };

        let files: Vec<String> = args
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let symbols: Vec<String> = args
            .get("symbols")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        self.ctx
            .lineage_store
            .record_outcome(id, outcome.clone(), None, &files, &symbols)
            .await?;

        Ok(ToolResult::text(format!(
            "Outcome recorded: {:?}\nFiles: {:?}\nSymbols: {:?}",
            outcome, files, symbols
        )))
    }

    async fn handle_reflect(&self, args: Value) -> Result<ToolResult> {
        let attempt_id = args.get("attempt_id").and_then(|v| v.as_str()).unwrap_or("");
        let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
        let root_cause = args.get("root_cause").and_then(|v| v.as_str()).unwrap_or("");
        let lesson = args.get("lesson").and_then(|v| v.as_str()).unwrap_or("");

        if attempt_id.is_empty() || intent.is_empty() || root_cause.is_empty() || lesson.is_empty() {
            return Ok(ToolResult::error(
                "Missing required parameters: attempt_id, intent, root_cause, lesson",
            ));
        }

        let mechanism = args.get("mechanism").and_then(|v| v.as_str()).map(String::from);
        let confidence = args.get("confidence").and_then(|v| v.as_f64()).map(|f| f as f32);
        let scope_paths: Vec<String> = args
            .get("scope_paths")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let scope_tags: Vec<String> = args
            .get("scope_tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let input = crate::learning::reflection::ReflectionInput {
            attempt_id: attempt_id.to_string(),
            intent: intent.to_string(),
            mechanism,
            root_cause: root_cause.to_string(),
            lesson: lesson.to_string(),
            confidence,
            scope_paths,
            scope_tags,
        };

        let result = crate::learning::reflection::reflect(
            &input,
            &self.ctx.lineage_store,
            &self.ctx.pattern_store,
            &self.ctx.failure_store,
            true, // Enable validation
        )
        .await?;

        let output = match result {
            crate::learning::reflection::ReflectionResult::PatternCreated(p) => {
                format!("Pattern created:\n  Intent: {}\n  ID: {}", p.intent, p.id)
            }
            crate::learning::reflection::ReflectionResult::FailureRecorded(f) => {
                format!("Failure recorded:\n  Cause: {}\n  ID: {}", f.cause, f.id)
            }
            crate::learning::reflection::ReflectionResult::Both { pattern, failure } => {
                format!(
                    "Pattern and failure recorded:\n  Pattern ID: {}\n  Failure ID: {}",
                    pattern.id, failure.id
                )
            }
        };

        Ok(ToolResult::text(output))
    }

    async fn handle_query_lineage(&self, args: Value) -> Result<ToolResult> {
        let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
        if task.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: task"));
        }

        let include_failures = args
            .get("include_failures")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        let solutions = self
            .ctx
            .lineage_store
            .query(task, include_failures, limit)
            .await?;

        if solutions.is_empty() {
            return Ok(ToolResult::text(format!("No solutions found for '{}'", task)));
        }

        let mut output = format!("# Solutions for: {}\n\n", task);
        for solution in &solutions {
            output.push_str(&format!(
                "## {:?} - {}\n",
                solution.outcome,
                chrono::DateTime::<chrono::Utc>::from_timestamp(solution.created_at, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ));
            output.push_str(&format!("**Plan:** {}\n", solution.plan));
            if let Some(approach) = &solution.approach {
                output.push_str(&format!("**Approach:** {}\n", approach));
            }
            if !solution.files_modified.is_empty() {
                output.push_str(&format!("**Files:** {:?}\n", solution.files_modified));
            }
            output.push_str(&format!("**ID:** {}\n\n", solution.id));
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    async fn handle_suggest_approach(&self, args: Value) -> Result<ToolResult> {
        let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
        if task.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: task"));
        }

        let current_file = args.get("current_file").and_then(|v| v.as_str()).map(String::from);

        let context = crate::learning::QueryContext {
            description: task.to_string(),
            current_file: current_file.clone(),
            relevant_symbols: vec![],
            tags: vec![],
        };

        // Query patterns, failures, and lineage
        let patterns = self.ctx.pattern_store.query(&context, 5).await?;
        let failures = self.ctx.failure_store.query(&context, true).await?;
        let solutions = self.ctx.lineage_store.query(task, true, 10).await?;

        let mut output = format!("# Suggestions for: {}\n\n", task);

        // Show relevant patterns
        if !patterns.is_empty() {
            output.push_str("## Relevant Patterns\n\n");
            let graph = self.ctx.graph.read().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;
            let now = chrono::Utc::now().timestamp();
            for pattern in patterns.iter().take(3) {
                let eff_conf = crate::learning::confidence::effective_confidence(pattern, Some(&graph), now, 90);
                output.push_str(&format!(
                    "- {} (confidence: {:.1}%)\n",
                    pattern.intent,
                    eff_conf * 100.0
                ));
            }
            output.push_str("\n");
        }

        // Show failures to avoid
        if !failures.is_empty() {
            output.push_str("## Failures to Avoid\n\n");
            for failure in failures.iter().take(3) {
                output.push_str(&format!("- {:?}: {}\n", failure.severity, failure.cause));
            }
            output.push_str("\n");
        }

        // Show past successful approaches
        let successful: Vec<_> = solutions
            .iter()
            .filter(|s| s.outcome == crate::learning::lineage::Outcome::Success)
            .take(2)
            .collect();

        if !successful.is_empty() {
            output.push_str("## Past Successful Approaches\n\n");
            for solution in successful {
                output.push_str(&format!("- {}\n", solution.plan));
                if let Some(approach) = &solution.approach {
                    output.push_str(&format!("  Approach: {}\n", approach));
                }
            }
            output.push_str("\n");
        }

        // Show failed approaches to avoid
        let failed: Vec<_> = solutions
            .iter()
            .filter(|s| s.outcome == crate::learning::lineage::Outcome::Failure)
            .take(2)
            .collect();

        if !failed.is_empty() {
            output.push_str("## Approaches That Failed\n\n");
            for solution in failed {
                output.push_str(&format!("- {}\n", solution.plan));
            }
            output.push_str("\n");
        }

        if patterns.is_empty() && failures.is_empty() && solutions.is_empty() {
            output.push_str("No relevant patterns, failures, or past solutions found.\n");
            output.push_str("This appears to be a novel task. Use record_attempt to track your approach.\n");
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    // === Phase 6: Behavioral Niches ===

    async fn handle_list_niches(&self, args: Value) -> Result<ToolResult> {
        let task_type = args.get("task_type").and_then(|v| v.as_str());

        let niches = self.ctx.niche_store.list_niches(task_type).await?;

        if niches.is_empty() {
            return Ok(ToolResult::text("No niches found. Niches are created by assigning solutions with feature vectors."));
        }

        let mut output = String::from("# Behavioral Niches\n\n");
        for niche_with_best in &niches {
            output.push_str(&format!("## Niche: {}\n", niche_with_best.niche.id));
            output.push_str(&format!("**Task Type:** {}\n", niche_with_best.niche.task_type));
            output.push_str(&format!("**Feature:** {}\n", niche_with_best.niche.feature_description));
            if let Some(best) = &niche_with_best.best_solution {
                output.push_str(&format!(
                    "**Best Solution:** {} (score: {:.2})\n",
                    best.solution_id, best.score
                ));
            }
            output.push_str("\n");
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    // === Phase 7: Skill Distillation ===

    async fn handle_distill_project_skill(&self, args: Value) -> Result<ToolResult> {
        let confidence_threshold = args
            .get("confidence_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;
        let write_file = args
            .get("write_file")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let result = crate::skill::distill::distill_project_skill(
            &self.ctx.pattern_store,
            &self.ctx.failure_store,
            &self.ctx.manual_instruction_store,
            confidence_threshold,
        )
        .await?;

        let markdown = crate::skill::render::generate_project_skill_md(&result);

        if write_file {
            let skill_path = self.ctx.config.codegraph_dir.join("SKILL.md");
            std::fs::write(&skill_path, &markdown)?;

            Ok(ToolResult::text(format!(
                "Project skill distilled successfully!\n\nInstructions: {} (patterns: {}, failures: {}, conventions: {}, manual: {})\nNavigation hints: {}\nWritten to: {}\n\nPreview:\n{}",
                result.instructions.len(),
                result.instructions.iter().filter(|i| matches!(i.source, crate::skill::categories::InstructionSource::Pattern { .. })).count(),
                result.instructions.iter().filter(|i| matches!(i.source, crate::skill::categories::InstructionSource::Failure { .. })).count(),
                result.instructions.iter().filter(|i| matches!(i.source, crate::skill::categories::InstructionSource::Convention { .. })).count(),
                result.instructions.iter().filter(|i| matches!(i.source, crate::skill::categories::InstructionSource::Manual { .. })).count(),
                result.navigation_hints.len(),
                skill_path.display(),
                markdown.lines().take(20).collect::<Vec<_>>().join("\n")
            )))
        } else {
            Ok(ToolResult::text(format!(
                "Project skill distilled:\n\nInstructions: {}\nNavigation hints: {}\n\n{}",
                result.instructions.len(),
                result.navigation_hints.len(),
                markdown.lines().take(30).collect::<Vec<_>>().join("\n")
            )))
        }
    }

    async fn handle_add_instruction(&self, args: Value) -> Result<ToolResult> {
        let instruction = args
            .get("instruction")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if instruction.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: instruction"));
        }

        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .map(crate::skill::categories::InstructionCategory::from_str)
            .unwrap_or(crate::skill::categories::InstructionCategory::Gotchas);

        let reason = args.get("reason").and_then(|v| v.as_str());

        let id = self
            .ctx
            .manual_instruction_store
            .add(instruction, category.clone(), reason)
            .await?;

        Ok(ToolResult::text(format!(
            "Instruction added:\n  ID: {}\n  Category: {:?}\n  Instruction: {}",
            id, category, instruction
        )))
    }

    async fn handle_get_project_instructions(&self, _args: Value) -> Result<ToolResult> {
        let instructions = self.ctx.manual_instruction_store.list_all().await?;

        if instructions.is_empty() {
            return Ok(ToolResult::text(
                "No manual instructions. Use add_instruction to add project-specific guidance.",
            ));
        }

        let mut output = String::from("# Manual Project Instructions\n\n");
        for inst in &instructions {
            output.push_str(&format!("## {:?}: {}\n", inst.category, inst.instruction));
            if let Some(scope) = &inst.scope {
                output.push_str(&format!("**Scope:** {}\n", scope));
            }
            if let crate::skill::categories::InstructionSource::Manual { reason } = &inst.source {
                if let Some(reason) = reason {
                    output.push_str(&format!("**Reason:** {}\n", reason));
                }
            }
            output.push_str(&format!("**ID:** {}\n\n", inst.id));
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    // === Phase 8: Cross-Language Inference ===

    async fn handle_infer_cross_edges(&self, args: Value) -> Result<ToolResult> {
        let force_rebuild = args
            .get("force_rebuild")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let stats = self.ctx.cross_language_inferrer.infer(force_rebuild).await?;

        Ok(ToolResult::text(format!(
            "Cross-language edges inferred:\n  Client calls found: {}\n  Server routes found: {}\n  Connections made: {}\n  Duration: {}ms",
            stats.client_calls_found,
            stats.server_routes_found,
            stats.connections_made,
            stats.duration_ms
        )))
    }

    async fn handle_get_api_connections(&self, args: Value) -> Result<ToolResult> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: path"));
        }

        let connections = self.ctx.cross_language_inferrer.get_api_connections(path).await?;

        if connections.is_empty() {
            return Ok(ToolResult::text(format!(
                "No API connections found for '{}'",
                path
            )));
        }

        let mut output = format!("# API Connections for {}\n\n", path);
        for conn in &connections {
            output.push_str(&format!(
                "## {} → {}\n",
                conn.client_file, conn.server_file
            ));
            output.push_str(&format!("**Path:** {}\n", conn.api_path));
            if let Some(method) = &conn.method {
                output.push_str(&format!("**Method:** {}\n", method));
            }
            output.push_str(&format!("**Confidence:** {:.1}%\n\n", conn.confidence * 100.0));
        }

        Ok(ToolResult::text(output.trim_end()))
    }

    // === Phase 9: Sync + Persistence ===

    async fn handle_sync_learnings(&self, args: Value) -> Result<ToolResult> {
        let confidence_threshold = args
            .get("confidence_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;
        let include_all_critical = args
            .get("include_all_critical")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let graph = self.ctx.graph.read().map_err(|e| anyhow::anyhow!("Graph lock poisoned: {}", e))?;
        let stats = crate::learning::sync::sync_learnings(
            &self.ctx.pattern_store,
            &self.ctx.failure_store,
            &self.ctx.config,
            Some(&graph),
            confidence_threshold,
            include_all_critical,
        )
        .await?;

        let mut output = format!(
            "Learnings synced successfully!\n\nPatterns: {}\nFailures: {}\n\nFiles written:\n",
            stats.patterns_synced, stats.failures_synced
        );
        for file in &stats.files_written {
            output.push_str(&format!("  - {}\n", file));
        }
        output.push_str(&format!("\nDuration: {}ms", stats.duration_ms));

        Ok(ToolResult::text(output.trim_end()))
    }

    // === RTK-style Compression Tools ===

    async fn handle_bash_compressed(&self, args: Value) -> Result<ToolResult> {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if command.is_empty() {
            return Ok(ToolResult::error("Missing required parameter: command"));
        }

        let max_lines = args.get("max_lines").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
        let max_items = args.get("max_items_per_group").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        let config = CompressConfig {
            max_lines,
            max_items_per_group: max_items,
            ..Default::default()
        };

        let result = crate::compress::exec_compressed(command, &config);

        match result {
            Ok(compressed) => {
                // Track analytics
                let category = crate::compress::categorize_command(command);
                let original_tokens = compressed.original_size / 4;
                let compressed_tokens = compressed.compressed_size / 4;

                {
                    let mut analytics = self.ctx.compression_analytics.lock().await;
                    analytics.record(category, original_tokens, compressed_tokens);
                }

                let reduction = compressed.reduction_percent();
                let header = if reduction > 10.0 {
                    format!("📦 Compressed ({:.0}% reduction, ~{} tokens saved)\n\n", reduction, compressed.estimated_token_savings)
                } else {
                    String::new()
                };

                Ok(ToolResult::text(format!("{}{}", header, compressed.output)))
            }
            Err(e) => Ok(ToolResult::error(e)),
        }
    }

    async fn handle_compression_stats(&self, args: Value) -> Result<ToolResult> {
        let reset = args.get("reset").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut analytics = self.ctx.compression_analytics.lock().await;
        let report = analytics.format_report();
        let stats_json = analytics.to_json();

        if reset {
            analytics.reset();
        }

        Ok(ToolResult::text(format!("{}\n\nJSON:\n{}", report, serde_json::to_string_pretty(&stats_json)?)))
    }
}

fn parse_task_status(s: &str) -> crate::session::TaskStatus {
    match s {
        "pending" => crate::session::TaskStatus::Pending,
        "in_progress" => crate::session::TaskStatus::InProgress,
        "completed" => crate::session::TaskStatus::Completed,
        "blocked" => crate::session::TaskStatus::Blocked,
        _ => crate::session::TaskStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Indexer;
    use crate::config::Config;
    use crate::session::SessionManager;
    use crate::store::{CodeGraph, Store};
    use tempfile::TempDir;

    async fn setup_ctx() -> (Arc<ToolContext>, TempDir) {
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
        let graph = Arc::new(RwLock::new(
            CodeGraph::load_from_store(&store).await.unwrap(),
        ));
        let indexer = Arc::new(Indexer::new(store.clone(), config.clone()));
        let session_manager = Arc::new(SessionManager::new(store.clone(), graph.clone()));

        let pattern_store = Arc::new(crate::learning::patterns::PatternStore::new(Arc::new(
            store.learning_db.clone(),
        )));
        let failure_store = Arc::new(crate::learning::failures::FailureStore::new(Arc::new(
            store.learning_db.clone(),
        )));
        let lineage_store = Arc::new(crate::learning::lineage::LineageStore::new(Arc::new(
            store.learning_db.clone(),
        )));
        let niche_store = Arc::new(crate::learning::niches::NicheStore::new(Arc::new(
            store.learning_db.clone(),
        )));
        let manual_instruction_store = Arc::new(crate::skill::distill::ManualInstructionStore::new(Arc::new(
            store.learning_db.clone(),
        )));
        let cross_language_inferrer = Arc::new(crate::code::CrossLanguageInferrer::new(store.clone()));

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
            compression_analytics: tokio::sync::Mutex::new(crate::compress::CompressionAnalytics::new()),
        });

        (ctx, temp_dir)
    }

    #[tokio::test]
    async fn test_list_tools() {
        let (ctx, _temp) = setup_ctx().await;
        let registry = ToolRegistry::new(ctx);
        let tools = registry.list();

        assert!(tools.len() >= 26); // 10 original + 9 phase 4-5 + 7 phase 6-9
        assert!(tools.iter().any(|t| t.name == "search_symbols"));
        assert!(tools.iter().any(|t| t.name == "smart_context"));
        assert!(tools.iter().any(|t| t.name == "index_project"));
        // Learning tools
        assert!(tools.iter().any(|t| t.name == "recall_patterns"));
        assert!(tools.iter().any(|t| t.name == "record_attempt"));
        assert!(tools.iter().any(|t| t.name == "reflect"));
        // Phase 6-9 tools
        assert!(tools.iter().any(|t| t.name == "list_niches"));
        assert!(tools.iter().any(|t| t.name == "distill_project_skill"));
        assert!(tools.iter().any(|t| t.name == "sync_learnings"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let (ctx, _temp) = setup_ctx().await;
        let registry = ToolRegistry::new(ctx);
        let result = registry.execute("unknown", json!({})).await.unwrap();

        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_search_empty_graph() {
        let (ctx, _temp) = setup_ctx().await;
        let registry = ToolRegistry::new(ctx);
        let result = registry
            .execute("search_symbols", json!({"query": "foo"}))
            .await
            .unwrap();

        // Should return "no symbols found", not an error
        assert!(result.is_error.is_none());
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (ctx, _temp) = setup_ctx().await;
        let registry = ToolRegistry::new(ctx);

        // Start session
        let result = registry
            .execute(
                "start_session",
                json!({"task": "Build feature", "items": ["Step 1", "Step 2"]}),
            )
            .await
            .unwrap();
        assert!(result.is_error.is_none());

        // Get session
        let result = registry
            .execute("get_session", json!({}))
            .await
            .unwrap();
        assert!(result.is_error.is_none());

        // Smart context
        let result = registry
            .execute("smart_context", json!({}))
            .await
            .unwrap();
        assert!(result.is_error.is_none());
    }
}
