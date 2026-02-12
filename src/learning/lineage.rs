use anyhow::Result;
use libsql::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Success,
    Failure,
    Partial,
}

impl Outcome {
    fn to_str(&self) -> &'static str {
        match self {
            Outcome::Success => "success",
            Outcome::Failure => "failure",
            Outcome::Partial => "partial",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "success" => Outcome::Success,
            "failure" => Outcome::Failure,
            "partial" => Outcome::Partial,
            _ => Outcome::Failure,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub tokens_saved: Option<i64>,
    pub time_saved_ms: Option<i64>,
    pub errors_avoided: Option<i64>,
    pub custom: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub id: String,
    pub task: String,
    pub plan: String,
    pub approach: Option<String>,
    pub outcome: Outcome,
    pub metrics: Option<Metrics>,
    pub files_modified: Vec<String>,
    pub symbols_modified: Vec<String>,
    pub parent_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageTree {
    pub root: Solution,
    pub children: Vec<LineageTree>,
}

pub struct LineageStore {
    db: Arc<Connection>,
}

impl LineageStore {
    pub fn new(db: Arc<Connection>) -> Self {
        Self { db }
    }

    /// Record an attempt (creates a solution entry)
    pub async fn record_attempt(
        &self,
        task: &str,
        plan: &str,
        approach: Option<&str>,
        parent_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let approach_str = approach.unwrap_or("");
        let parent_str = parent_id.unwrap_or("");

        self.db
            .execute(
                "INSERT INTO solutions (id, task, plan, approach, outcome, parent_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'partial', ?5, ?6)",
                libsql::params![
                    id.as_str(),
                    task,
                    plan,
                    if approach_str.is_empty() { None::<&str> } else { Some(approach_str) },
                    if parent_str.is_empty() { None::<&str> } else { Some(parent_str) },
                    now
                ],
            )
            .await?;

        Ok(id)
    }

    /// Record outcome for a solution
    pub async fn record_outcome(
        &self,
        id: &str,
        outcome: Outcome,
        metrics: Option<&Metrics>,
        files: &[String],
        symbols: &[String],
    ) -> Result<()> {
        let metrics_json = metrics
            .map(|m| serde_json::to_string(m))
            .transpose()?;
        let files_json = serde_json::to_string(files)?;
        let symbols_json = serde_json::to_string(symbols)?;

        self.db
            .execute(
                "UPDATE solutions SET outcome = ?1, metrics = ?2, files_modified = ?3, symbols_modified = ?4 WHERE id = ?5",
                libsql::params![
                    outcome.to_str(),
                    metrics_json.as_deref().unwrap_or(""),
                    files_json.as_str(),
                    symbols_json.as_str(),
                    id
                ],
            )
            .await?;

        Ok(())
    }

    /// Get solution by ID
    pub async fn get(&self, id: &str) -> Result<Option<Solution>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, task, plan, approach, outcome, metrics, files_modified, symbols_modified, parent_id, created_at
                 FROM solutions WHERE id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_solution(row)?))
        } else {
            Ok(None)
        }
    }

    /// Query solutions by task description
    pub async fn query(
        &self,
        task: &str,
        include_failures: bool,
        limit: usize,
    ) -> Result<Vec<Solution>> {
        let task_lower = task.to_lowercase();

        let query = if include_failures {
            "SELECT id, task, plan, approach, outcome, metrics, files_modified, symbols_modified, parent_id, created_at
             FROM solutions
             WHERE LOWER(task) LIKE ?1
             ORDER BY created_at DESC
             LIMIT ?2"
        } else {
            "SELECT id, task, plan, approach, outcome, metrics, files_modified, symbols_modified, parent_id, created_at
             FROM solutions
             WHERE LOWER(task) LIKE ?1 AND outcome = 'success'
             ORDER BY created_at DESC
             LIMIT ?2"
        };

        let pattern = format!("%{}%", task_lower);
        let mut rows = self
            .db
            .query(query, libsql::params![pattern.as_str(), limit as i64])
            .await?;

        let mut solutions = Vec::new();
        while let Some(row) = rows.next().await? {
            solutions.push(self.row_to_solution(row)?);
        }

        Ok(solutions)
    }

    /// Get children of a solution
    pub async fn get_children(&self, id: &str) -> Result<Vec<Solution>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, task, plan, approach, outcome, metrics, files_modified, symbols_modified, parent_id, created_at
                 FROM solutions WHERE parent_id = ?1
                 ORDER BY created_at ASC",
                [id],
            )
            .await?;

        let mut solutions = Vec::new();
        while let Some(row) = rows.next().await? {
            solutions.push(self.row_to_solution(row)?);
        }

        Ok(solutions)
    }

    /// Get full lineage tree for a solution
    pub async fn get_lineage_tree(&self, id: &str) -> Result<LineageTree> {
        let solution = self
            .get(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Solution not found"))?;

        let children = self.build_tree_recursive(&solution.id).await?;

        Ok(LineageTree {
            root: solution,
            children,
        })
    }

    fn build_tree_recursive<'a>(
        &'a self,
        id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<LineageTree>>> + 'a>> {
        Box::pin(async move {
            let children = self.get_children(id).await?;

            let mut trees = Vec::new();
            for child in children {
                let child_id = child.id.clone();
                let subtree = self.build_tree_recursive(&child_id).await?;
                trees.push(LineageTree {
                    root: child,
                    children: subtree,
                });
            }

            Ok(trees)
        })
    }

    fn row_to_solution(&self, row: libsql::Row) -> Result<Solution> {
        let approach: Option<String> = row.get(3).ok();
        let outcome_str: String = row.get(4)?;
        let metrics_str: Option<String> = row.get(5).ok();
        let files_str: Option<String> = row.get(6).ok();
        let symbols_str: Option<String> = row.get(7).ok();
        let parent: Option<String> = row.get(8).ok();

        Ok(Solution {
            id: row.get(0)?,
            task: row.get(1)?,
            plan: row.get(2)?,
            approach,
            outcome: Outcome::from_str(&outcome_str),
            metrics: metrics_str.and_then(|s| serde_json::from_str(&s).ok()),
            files_modified: files_str
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            symbols_modified: symbols_str
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            parent_id: parent,
            created_at: row.get(9)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::store::Store;
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
    async fn test_solution_lifecycle() {
        let (store, _temp) = setup_test_store().await;
        let lineage = LineageStore::new(Arc::new(store.learning_db));

        // Record attempt
        let id = lineage
            .record_attempt(
                "Implement search feature",
                "Use tree-sitter to parse and index",
                Some("tree-sitter"),
                None,
            )
            .await
            .unwrap();

        // Get solution
        let solution = lineage.get(&id).await.unwrap().unwrap();
        assert_eq!(solution.task, "Implement search feature");
        assert_eq!(solution.outcome, Outcome::Partial);

        // Record outcome
        let metrics = Metrics {
            tokens_saved: Some(500),
            time_saved_ms: Some(2000),
            errors_avoided: Some(2),
            custom: None,
        };

        lineage
            .record_outcome(
                &id,
                Outcome::Success,
                Some(&metrics),
                &["src/search.rs".to_string()],
                &["search_symbols".to_string()],
            )
            .await
            .unwrap();

        // Verify outcome updated
        let updated = lineage.get(&id).await.unwrap().unwrap();
        assert_eq!(updated.outcome, Outcome::Success);
        assert_eq!(updated.files_modified, vec!["src/search.rs"]);
        assert!(updated.metrics.is_some());
    }

    #[tokio::test]
    async fn test_query_solutions() {
        let (store, _temp) = setup_test_store().await;
        let lineage = LineageStore::new(Arc::new(store.learning_db));

        // Create successful solution
        let id1 = lineage
            .record_attempt("Add auth system", "Use JWT tokens", None, None)
            .await
            .unwrap();
        lineage
            .record_outcome(&id1, Outcome::Success, None, &[], &[])
            .await
            .unwrap();

        // Create failed solution
        let id2 = lineage
            .record_attempt("Add auth system", "Use sessions", None, None)
            .await
            .unwrap();
        lineage
            .record_outcome(&id2, Outcome::Failure, None, &[], &[])
            .await
            .unwrap();

        // Query only successes
        let results = lineage.query("auth", false, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, Outcome::Success);

        // Query including failures
        let all_results = lineage.query("auth", true, 10).await.unwrap();
        assert_eq!(all_results.len(), 2);
    }

    #[tokio::test]
    async fn test_lineage_tree() {
        let (store, _temp) = setup_test_store().await;
        let lineage = LineageStore::new(Arc::new(store.learning_db));

        // Create parent solution
        let parent_id = lineage
            .record_attempt("Fix bug", "Try approach A", None, None)
            .await
            .unwrap();
        lineage
            .record_outcome(&parent_id, Outcome::Failure, None, &[], &[])
            .await
            .unwrap();

        // Create child (retry)
        let child_id = lineage
            .record_attempt("Fix bug", "Try approach B", None, Some(&parent_id))
            .await
            .unwrap();
        lineage
            .record_outcome(&child_id, Outcome::Success, None, &[], &[])
            .await
            .unwrap();

        // Get tree
        let tree = lineage.get_lineage_tree(&parent_id).await.unwrap();
        assert_eq!(tree.root.id, parent_id);
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].root.id, child_id);
    }
}
