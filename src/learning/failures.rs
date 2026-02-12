use anyhow::Result;
use libsql::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::{QueryContext, Scope};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Major,
    Minor,
}

impl Severity {
    fn to_str(&self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::Major => "major",
            Severity::Minor => "minor",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "critical" => Severity::Critical,
            "major" => Severity::Major,
            _ => Severity::Minor,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub id: String,
    pub cause: String,
    pub avoidance_rule: String,
    pub severity: Severity,
    pub scope: Scope,
    pub times_prevented: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewFailure {
    pub cause: String,
    pub avoidance_rule: String,
    pub severity: Severity,
    pub scope: Scope,
}

pub struct FailureStore {
    db: Arc<Connection>,
}

impl FailureStore {
    pub fn new(db: Arc<Connection>) -> Self {
        Self { db }
    }

    /// Create a new failure
    pub async fn create(&self, failure: &NewFailure) -> Result<Failure> {
        let id = Uuid::new_v4().to_string();
        let scope_json = serde_json::to_string(&failure.scope)?;
        let now = chrono::Utc::now().timestamp();

        self.db
            .execute(
                "INSERT INTO failures (id, cause, avoidance_rule, severity, scope, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                libsql::params![
                    id.as_str(),
                    failure.cause.as_str(),
                    failure.avoidance_rule.as_str(),
                    failure.severity.to_str(),
                    scope_json.as_str(),
                    now,
                    now
                ],
            )
            .await?;

        Ok(Failure {
            id,
            cause: failure.cause.clone(),
            avoidance_rule: failure.avoidance_rule.clone(),
            severity: failure.severity.clone(),
            scope: failure.scope.clone(),
            times_prevented: 0,
            created_at: now,
            updated_at: now,
        })
    }

    /// Get failure by ID
    pub async fn get(&self, id: &str) -> Result<Option<Failure>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, cause, avoidance_rule, severity, scope, times_prevented, created_at, updated_at
                 FROM failures WHERE id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let scope_json: String = row.get(4)?;
            let severity_str: String = row.get(3)?;

            Ok(Some(Failure {
                id: row.get(0)?,
                cause: row.get(1)?,
                avoidance_rule: row.get(2)?,
                severity: Severity::from_str(&severity_str),
                scope: serde_json::from_str(&scope_json)?,
                times_prevented: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Increment times prevented
    pub async fn increment_prevented(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        self.db
            .execute(
                "UPDATE failures SET times_prevented = times_prevented + 1, updated_at = ?1 WHERE id = ?2",
                libsql::params![now, id],
            )
            .await?;

        Ok(())
    }

    /// Query failures matching the given context
    pub async fn query(
        &self,
        context: &QueryContext,
        include_all_critical: bool,
    ) -> Result<Vec<Failure>> {
        // Get all failures
        let mut rows = self
            .db
            .query(
                "SELECT id, cause, avoidance_rule, severity, scope, times_prevented, created_at, updated_at
                 FROM failures
                 ORDER BY
                     CASE severity
                         WHEN 'critical' THEN 0
                         WHEN 'major' THEN 1
                         ELSE 2
                     END,
                     created_at DESC",
                (),
            )
            .await?;

        let mut failures = Vec::new();

        while let Some(row) = rows.next().await? {
            let scope_json: String = row.get(4)?;
            let severity_str: String = row.get(3)?;

            let failure = Failure {
                id: row.get(0)?,
                cause: row.get(1)?,
                avoidance_rule: row.get(2)?,
                severity: Severity::from_str(&severity_str),
                scope: serde_json::from_str(&scope_json)?,
                times_prevented: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            };

            // Include critical failures regardless of scope
            if include_all_critical && failure.severity == Severity::Critical {
                failures.push(failure);
                continue;
            }

            // Check if scope matches context
            if failure.scope.matches(
                context.current_file.as_deref(),
                &context.relevant_symbols,
                &context.tags,
            ) {
                failures.push(failure);
            }
        }

        Ok(failures)
    }

    /// List all failures
    pub async fn list_all(&self) -> Result<Vec<Failure>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, cause, avoidance_rule, severity, scope, times_prevented, created_at, updated_at
                 FROM failures
                 ORDER BY created_at DESC",
                (),
            )
            .await?;

        let mut failures = Vec::new();

        while let Some(row) = rows.next().await? {
            let scope_json: String = row.get(4)?;
            let severity_str: String = row.get(3)?;

            failures.push(Failure {
                id: row.get(0)?,
                cause: row.get(1)?,
                avoidance_rule: row.get(2)?,
                severity: Severity::from_str(&severity_str),
                scope: serde_json::from_str(&scope_json)?,
                times_prevented: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            });
        }

        Ok(failures)
    }

    /// Delete a failure
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM failures WHERE id = ?1", [id])
            .await?;
        Ok(())
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
    async fn test_failure_crud() {
        let (store, _temp) = setup_test_store().await;
        let failure_store = FailureStore::new(Arc::new(store.learning_db));

        // Create
        let new_failure = NewFailure {
            cause: "Forgot to check null".to_string(),
            avoidance_rule: "Always validate input".to_string(),
            severity: Severity::Major,
            scope: Scope {
                include_paths: vec!["src/**/*.rs".to_string()],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
        };

        let failure = failure_store.create(&new_failure).await.unwrap();
        assert_eq!(failure.cause, "Forgot to check null");
        assert_eq!(failure.severity, Severity::Major);

        // Read
        let retrieved = failure_store.get(&failure.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().cause, "Forgot to check null");

        // Increment prevented
        failure_store
            .increment_prevented(&failure.id)
            .await
            .unwrap();
        let updated = failure_store.get(&failure.id).await.unwrap().unwrap();
        assert_eq!(updated.times_prevented, 1);

        // Delete
        failure_store.delete(&failure.id).await.unwrap();
        let deleted = failure_store.get(&failure.id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_failure_query_critical() {
        let (store, _temp) = setup_test_store().await;
        let failure_store = FailureStore::new(Arc::new(store.learning_db));

        // Create critical failure with narrow scope
        let critical = NewFailure {
            cause: "Critical error".to_string(),
            avoidance_rule: "Never do this".to_string(),
            severity: Severity::Critical,
            scope: Scope {
                include_paths: vec!["src/store/**".to_string()],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
        };

        failure_store.create(&critical).await.unwrap();

        // Query from different file - critical should still be included
        let context = QueryContext {
            description: "Working elsewhere".to_string(),
            current_file: Some("src/code/parser.rs".to_string()),
            relevant_symbols: vec![],
            tags: vec![],
        };

        let results = failure_store.query(&context, true).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].cause, "Critical error");
    }
}
