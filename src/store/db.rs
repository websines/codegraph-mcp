use anyhow::{Context, Result};
use libsql::{Builder, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use tracing::debug;

use super::migrations::{apply_learning_migrations, apply_store_migrations};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub graph: String,
    pub kind: String,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub graph: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub path: String,
    pub mtime: i64,
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_at: Option<i64>,
}

pub struct Store {
    pub code_db: Connection,
    pub learning_db: Connection,
}

impl Store {
    /// Open or create databases and apply migrations
    pub async fn open(config: &Config) -> Result<Self> {
        // Ensure directories exist
        config.ensure_dirs()?;

        debug!("Opening store database: {:?}", config.store_db_path);
        let code_db = Self::open_database(&config.store_db_path).await?;
        apply_store_migrations(&code_db).await?;

        debug!("Opening learning database: {:?}", config.learning_db_path);
        let learning_db = Self::open_database(&config.learning_db_path).await?;
        apply_learning_migrations(&learning_db).await?;

        Ok(Self {
            code_db,
            learning_db,
        })
    }

    async fn open_database(path: &Path) -> Result<Connection> {
        let db = Builder::new_local(path)
            .build()
            .await
            .with_context(|| format!("Failed to open database: {:?}", path))?;

        let conn = db.connect()?;
        Ok(conn)
    }

    // ===== Node CRUD =====

    pub async fn upsert_node(
        &self,
        id: &str,
        graph: &str,
        kind: &str,
        data: &Value,
    ) -> Result<()> {
        let data_str = serde_json::to_string(data)?;

        self.code_db
            .execute(
                "INSERT INTO nodes (id, graph, kind, data)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                     graph = ?2,
                     kind = ?3,
                     data = ?4,
                     updated_at = strftime('%s', 'now')",
                [id, graph, kind, &data_str],
            )
            .await?;

        Ok(())
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let mut rows = self
            .code_db
            .query(
                "SELECT id, graph, kind, data, created_at, updated_at
                 FROM nodes WHERE id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let data_str: String = row.get(3)?;
            let node = Node {
                id: row.get(0)?,
                graph: row.get(1)?,
                kind: row.get(2)?,
                data: serde_json::from_str(&data_str)?,
                created_at: row.get(4).ok(),
                updated_at: row.get(5).ok(),
            };
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    /// Find a node whose ID ends with `::suffix` (for resolving unqualified symbol names)
    pub async fn find_node_by_suffix(&self, suffix: &str) -> Result<Option<String>> {
        let pattern = format!("%::{}", suffix);
        let mut rows = self
            .code_db
            .query(
                "SELECT id FROM nodes WHERE id LIKE ?1 AND graph = 'code' LIMIT 1",
                [pattern.as_str()],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_node(&self, id: &str) -> Result<()> {
        self.code_db
            .execute("DELETE FROM nodes WHERE id = ?1", [id])
            .await?;
        Ok(())
    }

    pub async fn query_nodes(&self, graph: &str, kind: &str) -> Result<Vec<Node>> {
        let mut rows = self
            .code_db
            .query(
                "SELECT id, graph, kind, data, created_at, updated_at
                 FROM nodes WHERE graph = ?1 AND kind = ?2",
                [graph, kind],
            )
            .await?;

        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            let data_str: String = row.get(3)?;
            nodes.push(Node {
                id: row.get(0)?,
                graph: row.get(1)?,
                kind: row.get(2)?,
                data: serde_json::from_str(&data_str)?,
                created_at: row.get(4).ok(),
                updated_at: row.get(5).ok(),
            });
        }

        Ok(nodes)
    }

    // ===== Edge CRUD =====

    pub async fn upsert_edge(
        &self,
        source: &str,
        target: &str,
        kind: &str,
        graph: &str,
        data: Option<&Value>,
    ) -> Result<()> {
        let data_str = data.map(|d| serde_json::to_string(d)).transpose()?;

        self.code_db
            .execute(
                "INSERT INTO edges (source, target, kind, graph, data)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(source, target, kind, graph) DO UPDATE SET
                     data = ?5",
                libsql::params![source, target, kind, graph, data_str],
            )
            .await?;

        Ok(())
    }

    pub async fn get_edges_from(&self, source: &str) -> Result<Vec<Edge>> {
        let mut rows = self
            .code_db
            .query(
                "SELECT source, target, kind, graph, data, created_at
                 FROM edges WHERE source = ?1",
                [source],
            )
            .await?;

        let mut edges = Vec::new();
        while let Some(row) = rows.next().await? {
            let data_str: Option<String> = row.get(4).ok();
            edges.push(Edge {
                source: row.get(0)?,
                target: row.get(1)?,
                kind: row.get(2)?,
                graph: row.get(3)?,
                data: data_str.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: row.get(5).ok(),
            });
        }

        Ok(edges)
    }

    pub async fn get_edges_to(&self, target: &str) -> Result<Vec<Edge>> {
        let mut rows = self
            .code_db
            .query(
                "SELECT source, target, kind, graph, data, created_at
                 FROM edges WHERE target = ?1",
                [target],
            )
            .await?;

        let mut edges = Vec::new();
        while let Some(row) = rows.next().await? {
            let data_str: Option<String> = row.get(4).ok();
            edges.push(Edge {
                source: row.get(0)?,
                target: row.get(1)?,
                kind: row.get(2)?,
                graph: row.get(3)?,
                data: data_str.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: row.get(5).ok(),
            });
        }

        Ok(edges)
    }

    pub async fn delete_edges_for(&self, node_id: &str) -> Result<()> {
        self.code_db
            .execute(
                "DELETE FROM edges WHERE source = ?1 OR target = ?1",
                [node_id],
            )
            .await?;
        Ok(())
    }

    // ===== File Metadata =====

    pub async fn get_file_meta(&self, path: &str) -> Result<Option<FileMeta>> {
        let mut rows = self
            .code_db
            .query(
                "SELECT path, mtime, hash, indexed_at FROM files WHERE path = ?1",
                [path],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(FileMeta {
                path: row.get(0)?,
                mtime: row.get(1)?,
                hash: row.get(2)?,
                indexed_at: row.get(3).ok(),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn upsert_file_meta(&self, path: &str, mtime: i64, hash: &str) -> Result<()> {
        self.code_db
            .execute(
                "INSERT INTO files (path, mtime, hash)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(path) DO UPDATE SET
                     mtime = ?2,
                     hash = ?3,
                     indexed_at = strftime('%s', 'now')",
                [path, &mtime.to_string(), hash],
            )
            .await?;
        Ok(())
    }

    pub async fn list_indexed_files(&self) -> Result<Vec<String>> {
        let mut rows = self.code_db.query("SELECT path FROM files", ()).await?;

        let mut paths = Vec::new();
        while let Some(row) = rows.next().await? {
            paths.push(row.get(0)?);
        }

        Ok(paths)
    }

    pub async fn remove_file_meta(&self, path: &str) -> Result<()> {
        self.code_db
            .execute("DELETE FROM files WHERE path = ?1", [path])
            .await?;
        Ok(())
    }

    /// Delete all nodes whose ID starts with a given prefix
    pub async fn delete_nodes_by_prefix(&self, prefix: &str) -> Result<u64> {
        let pattern = format!("{}%", prefix);
        let result = self
            .code_db
            .execute("DELETE FROM nodes WHERE id LIKE ?1", [pattern.as_str()])
            .await?;
        Ok(result)
    }

    /// Delete all edges where source or target starts with a given prefix
    pub async fn delete_edges_by_node_prefix(&self, prefix: &str) -> Result<u64> {
        let pattern = format!("{}%", prefix);
        let result = self
            .code_db
            .execute(
                "DELETE FROM edges WHERE source LIKE ?1 OR target LIKE ?1",
                [pattern.as_str()],
            )
            .await?;
        Ok(result)
    }

    /// Find all node IDs ending with `::suffix` (for cross-file resolution with ambiguity detection)
    pub async fn find_all_nodes_by_suffix(&self, suffix: &str) -> Result<Vec<String>> {
        let pattern = format!("%::{}", suffix);
        let mut rows = self
            .code_db
            .query(
                "SELECT id FROM nodes WHERE id LIKE ?1 AND graph = 'code' AND kind != 'unresolved'",
                [pattern.as_str()],
            )
            .await?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    /// Delete a specific edge
    pub async fn delete_edge(
        &self,
        source: &str,
        target: &str,
        kind: &str,
        graph: &str,
    ) -> Result<()> {
        self.code_db
            .execute(
                "DELETE FROM edges WHERE source = ?1 AND target = ?2 AND kind = ?3 AND graph = ?4",
                [source, target, kind, graph],
            )
            .await?;
        Ok(())
    }

    /// Rewrite all edges pointing to `old_target` to point to `new_target` instead.
    /// Handles duplicates by deleting edges that would conflict with existing ones.
    pub async fn retarget_edges(&self, old_target: &str, new_target: &str) -> Result<u64> {
        // First, delete any edges from old_target that would conflict
        // (where an edge with the same source+kind+graph already points to new_target)
        self.code_db
            .execute(
                "DELETE FROM edges WHERE target = ?1 AND EXISTS (
                    SELECT 1 FROM edges e2
                    WHERE e2.source = edges.source
                    AND e2.target = ?2
                    AND e2.kind = edges.kind
                    AND e2.graph = edges.graph
                )",
                [old_target, new_target],
            )
            .await?;

        // Now safely retarget the remaining edges
        let result = self
            .code_db
            .execute(
                "UPDATE edges SET target = ?2 WHERE target = ?1",
                [old_target, new_target],
            )
            .await?;
        Ok(result)
    }

    /// Get all unresolved stub nodes
    pub async fn get_unresolved_nodes(&self) -> Result<Vec<(String, String)>> {
        let mut rows = self
            .code_db
            .query(
                "SELECT id, data FROM nodes WHERE graph = 'code' AND kind = 'unresolved'",
                (),
            )
            .await?;

        let mut stubs = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let data_str: String = row.get(1)?;
            let data: Value = serde_json::from_str(&data_str).unwrap_or_default();
            let name = data
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            stubs.push((id, name));
        }
        Ok(stubs)
    }

    /// Delete all nodes and edges for a given graph type (e.g., "session")
    pub async fn delete_graph(&self, graph: &str) -> Result<()> {
        self.code_db
            .execute("DELETE FROM edges WHERE graph = ?1", [graph])
            .await?;
        self.code_db
            .execute("DELETE FROM nodes WHERE graph = ?1", [graph])
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    async fn setup_test_store() -> (Store, TempDir) {
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

        let store = Store::open(&config).await.unwrap();
        (store, temp_dir)
    }

    #[tokio::test]
    async fn test_node_crud() {
        let (store, _temp) = setup_test_store().await;

        // Create
        store
            .upsert_node("test::func", "code", "function", &json!({"name": "test"}))
            .await
            .unwrap();

        // Read
        let node = store.get_node("test::func").await.unwrap();
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind, "function");

        // Update
        store
            .upsert_node("test::func", "code", "function", &json!({"name": "updated"}))
            .await
            .unwrap();

        let node = store.get_node("test::func").await.unwrap().unwrap();
        assert_eq!(node.data["name"], "updated");

        // Delete
        store.delete_node("test::func").await.unwrap();
        let node = store.get_node("test::func").await.unwrap();
        assert!(node.is_none());
    }

    #[tokio::test]
    async fn test_edge_crud() {
        let (store, _temp) = setup_test_store().await;

        // Create nodes first
        store
            .upsert_node("node1", "code", "function", &json!({}))
            .await
            .unwrap();
        store
            .upsert_node("node2", "code", "function", &json!({}))
            .await
            .unwrap();

        // Create edge
        store
            .upsert_edge("node1", "node2", "calls", "code", None)
            .await
            .unwrap();

        // Read outgoing edges
        let edges = store.get_edges_from("node1").await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "node2");

        // Read incoming edges
        let edges = store.get_edges_to("node2").await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "node1");

        // Delete edges
        store.delete_edges_for("node1").await.unwrap();
        let edges = store.get_edges_from("node1").await.unwrap();
        assert_eq!(edges.len(), 0);
    }

    #[tokio::test]
    async fn test_file_meta() {
        let (store, _temp) = setup_test_store().await;

        // Create
        store
            .upsert_file_meta("src/main.rs", 1234567890, "abc123")
            .await
            .unwrap();

        // Read
        let meta = store.get_file_meta("src/main.rs").await.unwrap();
        assert!(meta.is_some());
        assert_eq!(meta.unwrap().hash, "abc123");

        // Update
        store
            .upsert_file_meta("src/main.rs", 1234567899, "def456")
            .await
            .unwrap();

        let meta = store.get_file_meta("src/main.rs").await.unwrap().unwrap();
        assert_eq!(meta.hash, "def456");

        // List
        let files = store.list_indexed_files().await.unwrap();
        assert_eq!(files.len(), 1);

        // Delete
        store.remove_file_meta("src/main.rs").await.unwrap();
        let meta = store.get_file_meta("src/main.rs").await.unwrap();
        assert!(meta.is_none());
    }
}
