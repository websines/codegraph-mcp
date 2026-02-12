use codegraph::config::Config;
use codegraph::learning::failures::{FailureStore, NewFailure, Severity};
use codegraph::learning::lineage::{LineageStore, Outcome};
use codegraph::learning::patterns::{NewPattern, PatternStore};
use codegraph::learning::{QueryContext, Scope};
use codegraph::store::Store;
use std::sync::Arc;
use tempfile::TempDir;

async fn setup_learning_stores() -> (Arc<PatternStore>, Arc<FailureStore>, Arc<LineageStore>, TempDir)
{
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let config = Config {
        project_root: temp_path.to_path_buf(),
        cache_dir: temp_path.join("cache"),
        codegraph_dir: temp_path.join(".codegraph"),
        store_db_path: temp_path.join("cache/store.db"),
        learning_db_path: temp_path.join(".codegraph/learning.db"),
        settings: codegraph::config::ConfigFile::default(),
    };
    config.ensure_dirs().unwrap();

    let store = Store::open(&config).await.unwrap();

    let pattern_store = Arc::new(PatternStore::new(Arc::new(store.learning_db.clone())));
    let failure_store = Arc::new(FailureStore::new(Arc::new(store.learning_db.clone())));
    let lineage_store = Arc::new(LineageStore::new(Arc::new(store.learning_db.clone())));

    (pattern_store, failure_store, lineage_store, temp_dir)
}

#[tokio::test]
async fn test_full_learning_lifecycle() {
    let (pattern_store, failure_store, lineage_store, _temp) = setup_learning_stores().await;

    // 1. Record an attempt
    let solution_id = lineage_store
        .record_attempt(
            "Fix database connection pooling",
            "Use connection pool with max 10 connections",
            Some("pool-approach"),
            None,
        )
        .await
        .unwrap();
    assert!(!solution_id.is_empty());

    // 2. Record outcome
    lineage_store
        .record_outcome(
            &solution_id,
            Outcome::Success,
            None,
            &["src/db.rs".to_string()],
            &["ConnectionPool".to_string()],
        )
        .await
        .unwrap();

    // 3. Extract a pattern from the success
    let pattern = pattern_store
        .create(&NewPattern {
            intent: "Use connection pooling for database access".to_string(),
            mechanism: Some("Create a pool with bounded connections".to_string()),
            examples: vec!["let pool = Pool::new(10);".to_string()],
            scope: Scope {
                include_paths: vec!["src/db/**".to_string()],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec!["database".to_string()],
            },
            confidence: 0.85,
        })
        .await
        .unwrap();

    // 4. Record a failure
    let failure = failure_store
        .create(&NewFailure {
            cause: "Direct database connections without pooling cause exhaustion".to_string(),
            avoidance_rule: "Always use connection pooling for database access".to_string(),
            severity: Severity::Critical,
            scope: Scope {
                include_paths: vec!["src/db/**".to_string()],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec!["database".to_string()],
            },
        })
        .await
        .unwrap();

    // 5. Query patterns
    let context = QueryContext {
        description: "database connection pooling".to_string(),
        current_file: Some("src/db/pool.rs".to_string()),
        relevant_symbols: vec![],
        tags: vec!["database".to_string()],
    };

    let patterns = pattern_store.query(&context, 5).await.unwrap();
    assert!(!patterns.is_empty());
    assert!(patterns.iter().any(|p| p.id == pattern.id));

    // 6. Query failures
    let failures = failure_store.query(&context, true).await.unwrap();
    assert!(!failures.is_empty());
    assert!(failures.iter().any(|f| f.id == failure.id));

    // 7. Query lineage
    let solutions = lineage_store
        .query("database connection", true, 10)
        .await
        .unwrap();
    assert!(!solutions.is_empty());
    assert!(solutions.iter().any(|s| s.id == solution_id));
}

#[tokio::test]
async fn test_pattern_usage_tracking() {
    let (pattern_store, _failure_store, _lineage_store, _temp) = setup_learning_stores().await;

    let pattern = pattern_store
        .create(&NewPattern {
            intent: "Use async/await for I/O operations".to_string(),
            mechanism: None,
            examples: vec!["async fn read_file() { ... }".to_string()],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.7,
        })
        .await
        .unwrap();

    assert_eq!(pattern.usage_count, 0);
    assert_eq!(pattern.success_count, 0);

    // Record usage
    pattern_store.update_usage(&pattern.id, true).await.unwrap();
    pattern_store.update_usage(&pattern.id, true).await.unwrap();
    pattern_store.update_usage(&pattern.id, false).await.unwrap();

    // Verify counts
    let context = QueryContext {
        description: "async I/O".to_string(),
        current_file: None,
        relevant_symbols: vec![],
        tags: vec![],
    };
    let patterns = pattern_store.query(&context, 10).await.unwrap();
    let found = patterns.iter().find(|p| p.id == pattern.id).unwrap();
    assert_eq!(found.usage_count, 3);
    assert_eq!(found.success_count, 2);
}
