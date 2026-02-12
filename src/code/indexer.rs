use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use walkdir::WalkDir;
use xxhash_rust::xxh3::xxh3_64;

use std::collections::HashMap;

use super::languages::detect_language;
use super::parser::{parse_file, ReferenceKind, SymbolKind};
use crate::config::Config;
use crate::store::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub files_scanned: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub files_removed: usize,
    pub symbols_found: usize,
    pub edges_found: usize,
    pub unresolved_before: usize,
    pub resolved: usize,
    pub unresolved_after: usize,
    pub duration_ms: u64,
}

pub struct Indexer {
    store: Arc<Store>,
    config: Arc<Config>,
}

impl Indexer {
    pub fn new(store: Arc<Store>, config: Arc<Config>) -> Self {
        Self { store, config }
    }

    /// Full project index
    pub async fn index_full(&self) -> Result<IndexStats> {
        info!("Starting full project index");
        self.index_internal(true).await
    }

    /// Incremental index: only changed files
    pub async fn index_incremental(&self) -> Result<IndexStats> {
        info!("Starting incremental index");
        self.index_internal(false).await
    }

    /// Index specific paths
    pub async fn index_paths(&self, paths: &[PathBuf]) -> Result<IndexStats> {
        info!("Indexing {} specific paths", paths.len());
        let start = SystemTime::now();

        let mut stats = IndexStats {
            files_scanned: 0,
            files_indexed: 0,
            files_skipped: 0,
            files_removed: 0,
            symbols_found: 0,
            edges_found: 0,
            unresolved_before: 0,
            resolved: 0,
            unresolved_after: 0,
            duration_ms: 0,
        };

        for path in paths {
            if path.is_file() {
                stats.files_scanned += 1;
                if let Err(e) = self.index_file(path, &mut stats).await {
                    warn!("Failed to index {:?}: {}", path, e);
                }
            }
        }

        stats.duration_ms = start.elapsed()?.as_millis() as u64;
        Ok(stats)
    }

    async fn index_internal(&self, force_full: bool) -> Result<IndexStats> {
        let start = SystemTime::now();

        let mut stats = IndexStats {
            files_scanned: 0,
            files_indexed: 0,
            files_skipped: 0,
            files_removed: 0,
            symbols_found: 0,
            edges_found: 0,
            unresolved_before: 0,
            resolved: 0,
            unresolved_after: 0,
            duration_ms: 0,
        };

        // Get list of previously indexed files
        let indexed_files: HashSet<String> = self
            .store
            .list_indexed_files()
            .await?
            .into_iter()
            .collect();

        let mut found_files = HashSet::new();

        // Walk project directory
        for entry in WalkDir::new(&self.config.project_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_excluded(e.path(), &self.config.settings.indexing.exclude))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Walk error: {}", e);
                    continue;
                }
            };

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            // Check if this is a supported language
            if detect_language(path.to_str().unwrap_or("")).is_none() {
                continue;
            }

            stats.files_scanned += 1;

            // Get relative path
            let rel_path = path
                .strip_prefix(&self.config.project_root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            found_files.insert(rel_path.clone());

            // Check if we need to reindex
            let needs_reindex = if force_full {
                true
            } else {
                self.needs_reindex(path, &rel_path).await?
            };

            if needs_reindex {
                if let Err(e) = self.index_file(path, &mut stats).await {
                    warn!("Failed to index {:?}: {}", path, e);
                }
            } else {
                stats.files_skipped += 1;
            }
        }

        // Remove files that no longer exist
        for indexed_path in indexed_files {
            if !found_files.contains(&indexed_path) {
                debug!("Removing deleted file: {}", indexed_path);
                self.remove_file_nodes(&indexed_path).await?;
                self.store.remove_file_meta(&indexed_path).await?;
                stats.files_removed += 1;
            }
        }

        // Resolve cross-file references
        self.resolve_cross_file_references(&mut stats).await?;

        stats.duration_ms = start.elapsed()?.as_millis() as u64;

        info!(
            "Indexing complete: {} files scanned, {} indexed, {} skipped, {} removed ({} symbols, {} edges) in {}ms",
            stats.files_scanned,
            stats.files_indexed,
            stats.files_skipped,
            stats.files_removed,
            stats.symbols_found,
            stats.edges_found,
            stats.duration_ms
        );

        Ok(stats)
    }

    async fn needs_reindex(&self, path: &Path, rel_path: &str) -> Result<bool> {
        // Get file metadata
        let metadata = std::fs::metadata(path)?;
        let mtime = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

        // Check stored metadata
        if let Some(stored) = self.store.get_file_meta(rel_path).await? {
            // Compare mtime and hash
            if stored.mtime == mtime {
                return Ok(false);
            }

            // Mtime changed, check hash
            let content = std::fs::read(path)?;
            let hash = format!("{:016x}", xxh3_64(&content));

            if stored.hash == hash {
                // Content unchanged, update mtime
                self.store.upsert_file_meta(rel_path, mtime, &hash).await?;
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn index_file(&self, path: &Path, stats: &mut IndexStats) -> Result<()> {
        debug!("Indexing file: {:?}", path);

        let rel_path = path
            .strip_prefix(&self.config.project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        // Detect language
        let lang_config = detect_language(path.to_str().unwrap_or(""))
            .context("Unsupported file type")?;

        // Read file
        let content = std::fs::read(path).context("Failed to read file")?;

        // Compute hash
        let hash = format!("{:016x}", xxh3_64(&content));
        let mtime = std::fs::metadata(path)?
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

        // Remove old nodes for this file
        self.remove_file_nodes(&rel_path).await?;

        // Parse file
        let parse_result = parse_file(path, &content, lang_config)?;

        stats.symbols_found += parse_result.symbols.len();
        stats.edges_found += parse_result.references.len();

        // Insert symbols as nodes
        for symbol in &parse_result.symbols {
            let node_id = format!("{}::{}", rel_path, symbol.name);

            let data = json!({
                "name": symbol.name,
                "kind": symbol.kind,
                "file": rel_path,
                "line_start": symbol.line_start,
                "line_end": symbol.line_end,
                "signature": symbol.signature,
                "docstring": symbol.docstring,
            });

            self.store
                .upsert_node(&node_id, "code", &symbol_kind_to_str(&symbol.kind), &data)
                .await?;
        }

        // Build a lookup of symbol names to node IDs for this file
        let local_symbols: HashMap<String, String> = parse_result
            .symbols
            .iter()
            .map(|s| (s.name.clone(), format!("{}::{}", rel_path, s.name)))
            .collect();

        // Insert references as edges
        for reference in &parse_result.references {
            // Determine the source: the enclosing symbol, or a file-level node
            let source_id = match &reference.from_symbol {
                Some(name) => local_symbols
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| format!("{}::{}", rel_path, name)),
                None => format!("file::{}", rel_path),
            };

            // Resolve target: try same-file first, then store as unresolved
            // (will be resolved at graph-load time via name matching)
            let target_id = local_symbols
                .get(&reference.to_name)
                .cloned()
                .unwrap_or_else(|| format!("unresolved::{}", reference.to_name));

            let data = json!({
                "file": rel_path,
                "line": reference.line,
                "target_name": reference.to_name,
            });

            // Ensure the source node exists (create file-level node if needed)
            if source_id.starts_with("file::") {
                self.store
                    .upsert_node(
                        &source_id,
                        "code",
                        "file",
                        &json!({"path": rel_path, "name": rel_path}),
                    )
                    .await?;
            }

            // Ensure the target node exists (create unresolved stub if needed)
            if target_id.starts_with("unresolved::") {
                self.store
                    .upsert_node(
                        &target_id,
                        "code",
                        "unresolved",
                        &json!({"name": reference.to_name}),
                    )
                    .await?;
            }

            self.store
                .upsert_edge(
                    &source_id,
                    &target_id,
                    &reference_kind_to_str(&reference.kind),
                    "code",
                    Some(&data),
                )
                .await?;
        }

        // Update file metadata
        self.store.upsert_file_meta(&rel_path, mtime, &hash).await?;

        stats.files_indexed += 1;

        Ok(())
    }

    /// Post-index pass: resolve `unresolved::X` stubs to real `file.py::X` nodes
    async fn resolve_cross_file_references(&self, stats: &mut IndexStats) -> Result<()> {
        let stubs = self.store.get_unresolved_nodes().await?;
        stats.unresolved_before = stubs.len();

        let mut resolved_count = 0;

        for (stub_id, name) in &stubs {
            if name.is_empty() {
                continue;
            }

            // Find all real nodes matching this name
            let candidates = self.store.find_all_nodes_by_suffix(name).await?;

            if candidates.len() == 1 {
                // Unambiguous: rewrite edges and delete the stub
                let real_id = &candidates[0];

                let retargeted = self.store.retarget_edges(stub_id, real_id).await?;
                if retargeted > 0 {
                    debug!("Resolved {} → {} ({} edges)", stub_id, real_id, retargeted);
                }

                // Delete the stub node (CASCADE will clean up any remaining edges)
                self.store.delete_node(stub_id).await?;
                resolved_count += 1;
            }
            // Multiple matches = ambiguous, skip
            // Zero matches = truly external symbol, keep stub
        }

        stats.resolved = resolved_count;
        stats.unresolved_after = stats.unresolved_before - resolved_count;

        info!(
            "Cross-file resolution: {}/{} resolved, {} remaining",
            resolved_count, stats.unresolved_before, stats.unresolved_after
        );

        Ok(())
    }

    async fn remove_file_nodes(&self, rel_path: &str) -> Result<()> {
        let prefix = format!("{}::", rel_path);
        // Delete edges first (they reference nodes), then nodes
        self.store.delete_edges_by_node_prefix(&prefix).await?;
        self.store.delete_nodes_by_prefix(&prefix).await?;
        // Also clean up file-level node
        let file_node_id = format!("file::{}", rel_path);
        self.store.delete_edges_for(&file_node_id).await?;
        self.store.delete_node(&file_node_id).await?;
        Ok(())
    }
}

fn is_excluded(path: &Path, exclude_list: &[String]) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            if let Some(name_str) = name.to_str() {
                if exclude_list.iter().any(|e| e == name_str) {
                    return true;
                }
            }
        }
    }

    false
}

fn symbol_kind_to_str(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Class => "class",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Interface => "interface",
        SymbolKind::Trait => "trait",
        SymbolKind::Type => "type",
        SymbolKind::Const => "const",
        SymbolKind::Static => "static",
        SymbolKind::Variable => "variable",
        SymbolKind::Module => "module",
        SymbolKind::Impl => "impl",
    }
}

fn reference_kind_to_str(kind: &ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Call => "calls",
        ReferenceKind::Import => "imports",
        ReferenceKind::Inherits => "inherits",
        ReferenceKind::Implements => "implements",
        ReferenceKind::UsesType => "uses_type",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_excluded() {
        let defaults = crate::config::IndexingConfig::default().exclude;
        assert!(is_excluded(Path::new("node_modules/foo"), &defaults));
        assert!(is_excluded(Path::new("target/debug"), &defaults));
        assert!(is_excluded(Path::new(".git/objects"), &defaults));
        assert!(is_excluded(Path::new("src/.codegraph/store.db"), &defaults));
        assert!(!is_excluded(Path::new("src/main.rs"), &defaults));
        // Should NOT match partial names
        assert!(!is_excluded(Path::new("src/target_utils/lib.rs"), &defaults));
        assert!(!is_excluded(Path::new("src/rebuild/mod.rs"), &defaults));
    }

    #[test]
    fn test_symbol_kind_conversion() {
        assert_eq!(symbol_kind_to_str(&SymbolKind::Function), "function");
        assert_eq!(symbol_kind_to_str(&SymbolKind::Class), "class");
    }

    #[test]
    fn test_reference_kind_conversion() {
        assert_eq!(reference_kind_to_str(&ReferenceKind::Call), "calls");
        assert_eq!(reference_kind_to_str(&ReferenceKind::Import), "imports");
    }
}
