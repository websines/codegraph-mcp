use anyhow::Result;
use libsql::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::{QueryContext, Scope};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub intent: String,
    pub mechanism: Option<String>,
    pub examples: Vec<String>,
    pub scope: Scope,
    pub confidence: f32,
    pub usage_count: i64,
    pub success_count: i64,
    pub last_validated: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewPattern {
    pub intent: String,
    pub mechanism: Option<String>,
    pub examples: Vec<String>,
    pub scope: Scope,
    pub confidence: f32,
}

pub struct PatternStore {
    db: Arc<Connection>,
}

impl PatternStore {
    pub fn new(db: Arc<Connection>) -> Self {
        Self { db }
    }

    /// Create a new pattern
    pub async fn create(&self, pattern: &NewPattern) -> Result<Pattern> {
        let id = Uuid::new_v4().to_string();
        let examples_json = serde_json::to_string(&pattern.examples)?;
        let scope_json = serde_json::to_string(&pattern.scope)?;
        let now = chrono::Utc::now().timestamp();

        self.db
            .execute(
                "INSERT INTO patterns (id, intent, mechanism, examples, scope, confidence, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                libsql::params![
                    id.as_str(),
                    pattern.intent.as_str(),
                    pattern.mechanism.as_deref().unwrap_or(""),
                    examples_json.as_str(),
                    scope_json.as_str(),
                    pattern.confidence as f64,
                    now,
                    now
                ],
            )
            .await?;

        Ok(Pattern {
            id,
            intent: pattern.intent.clone(),
            mechanism: pattern.mechanism.clone(),
            examples: pattern.examples.clone(),
            scope: pattern.scope.clone(),
            confidence: pattern.confidence,
            usage_count: 0,
            success_count: 0,
            last_validated: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Get pattern by ID
    pub async fn get(&self, id: &str) -> Result<Option<Pattern>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, intent, mechanism, examples, scope, confidence, usage_count, success_count, last_validated, created_at, updated_at
                 FROM patterns WHERE id = ?1",
                [id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let examples_json: String = row.get(3)?;
            let scope_json: String = row.get(4)?;

            Ok(Some(Pattern {
                id: row.get(0)?,
                intent: row.get(1)?,
                mechanism: {
                    let mech: String = row.get(2)?;
                    if mech.is_empty() {
                        None
                    } else {
                        Some(mech)
                    }
                },
                examples: serde_json::from_str(&examples_json)?,
                scope: serde_json::from_str(&scope_json)?,
                confidence: row.get::<f64>(5)? as f32,
                usage_count: row.get(6)?,
                success_count: row.get(7)?,
                last_validated: row.get(8).ok(),
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Update usage statistics
    pub async fn update_usage(&self, id: &str, succeeded: bool) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        if succeeded {
            self.db
                .execute(
                    "UPDATE patterns SET usage_count = usage_count + 1, success_count = success_count + 1, last_validated = ?1, updated_at = ?1 WHERE id = ?2",
                    libsql::params![now, id],
                )
                .await?;
        } else {
            self.db
                .execute(
                    "UPDATE patterns SET usage_count = usage_count + 1, updated_at = ?1 WHERE id = ?2",
                    libsql::params![now, id],
                )
                .await?;
        }

        Ok(())
    }

    /// Query patterns matching the given context
    pub async fn query(&self, context: &QueryContext, limit: usize) -> Result<Vec<Pattern>> {
        // Get all patterns and filter by scope
        let mut rows = self
            .db
            .query(
                "SELECT id, intent, mechanism, examples, scope, confidence, usage_count, success_count, last_validated, created_at, updated_at
                 FROM patterns
                 ORDER BY confidence DESC, success_count DESC",
                (),
            )
            .await?;

        let mut patterns = Vec::new();

        while let Some(row) = rows.next().await? {
            let examples_json: String = row.get(3)?;
            let scope_json: String = row.get(4)?;

            let pattern = Pattern {
                id: row.get(0)?,
                intent: row.get(1)?,
                mechanism: {
                    let mech: String = row.get(2)?;
                    if mech.is_empty() {
                        None
                    } else {
                        Some(mech)
                    }
                },
                examples: serde_json::from_str(&examples_json)?,
                scope: serde_json::from_str(&scope_json)?,
                confidence: row.get::<f64>(5)? as f32,
                usage_count: row.get(6)?,
                success_count: row.get(7)?,
                last_validated: row.get(8).ok(),
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            };

            // Check if scope matches context
            if pattern.scope.matches(
                context.current_file.as_deref(),
                &context.relevant_symbols,
                &context.tags,
            ) {
                patterns.push(pattern);
            }
        }

        // Take limit
        patterns.truncate(limit);

        Ok(patterns)
    }

    /// List all patterns
    pub async fn list_all(&self) -> Result<Vec<Pattern>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, intent, mechanism, examples, scope, confidence, usage_count, success_count, last_validated, created_at, updated_at
                 FROM patterns
                 ORDER BY created_at DESC",
                (),
            )
            .await?;

        let mut patterns = Vec::new();

        while let Some(row) = rows.next().await? {
            let examples_json: String = row.get(3)?;
            let scope_json: String = row.get(4)?;

            patterns.push(Pattern {
                id: row.get(0)?,
                intent: row.get(1)?,
                mechanism: {
                    let mech: String = row.get(2)?;
                    if mech.is_empty() {
                        None
                    } else {
                        Some(mech)
                    }
                },
                examples: serde_json::from_str(&examples_json)?,
                scope: serde_json::from_str(&scope_json)?,
                confidence: row.get::<f64>(5)? as f32,
                usage_count: row.get(6)?,
                success_count: row.get(7)?,
                last_validated: row.get(8).ok(),
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            });
        }

        Ok(patterns)
    }

    /// Delete a pattern
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM patterns WHERE id = ?1", [id])
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
    async fn test_pattern_crud() {
        let (store, _temp) = setup_test_store().await;
        let pattern_store = PatternStore::new(Arc::new(store.learning_db));

        // Create
        let new_pattern = NewPattern {
            intent: "Test pattern".to_string(),
            mechanism: Some("Test mechanism".to_string()),
            examples: vec!["example1".to_string(), "example2".to_string()],
            scope: Scope {
                include_paths: vec!["src/**/*.rs".to_string()],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec!["test".to_string()],
            },
            confidence: 0.8,
        };

        let pattern = pattern_store.create(&new_pattern).await.unwrap();
        assert_eq!(pattern.intent, "Test pattern");
        assert_eq!(pattern.confidence, 0.8);

        // Read
        let retrieved = pattern_store.get(&pattern.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().intent, "Test pattern");

        // Update usage
        pattern_store
            .update_usage(&pattern.id, true)
            .await
            .unwrap();
        let updated = pattern_store.get(&pattern.id).await.unwrap().unwrap();
        assert_eq!(updated.usage_count, 1);
        assert_eq!(updated.success_count, 1);

        // Delete
        pattern_store.delete(&pattern.id).await.unwrap();
        let deleted = pattern_store.get(&pattern.id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_pattern_query() {
        let (store, _temp) = setup_test_store().await;
        let pattern_store = PatternStore::new(Arc::new(store.learning_db));

        // Create pattern with specific scope
        let new_pattern = NewPattern {
            intent: "Database pattern".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec!["src/store/**".to_string()],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.9,
        };

        pattern_store.create(&new_pattern).await.unwrap();

        // Query with matching context
        let context = QueryContext {
            description: "Working with database".to_string(),
            current_file: Some("src/store/db.rs".to_string()),
            relevant_symbols: vec![],
            tags: vec![],
        };

        let results = pattern_store.query(&context, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].intent, "Database pattern");

        // Query with non-matching context
        let context2 = QueryContext {
            description: "Working with parser".to_string(),
            current_file: Some("src/code/parser.rs".to_string()),
            relevant_symbols: vec![],
            tags: vec![],
        };

        let results2 = pattern_store.query(&context2, 10).await.unwrap();
        assert_eq!(results2.len(), 0);
    }
}
