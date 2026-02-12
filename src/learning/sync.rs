use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::confidence::effective_confidence;
use super::failures::{Failure, FailureStore, Severity};
use super::patterns::{Pattern, PatternStore};
use crate::config::Config;
use crate::store::CodeGraph;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStats {
    pub patterns_synced: usize,
    pub failures_synced: usize,
    pub files_written: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternsFile {
    pub version: u32,
    pub synced_at: String,
    pub patterns: Vec<DistilledPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistilledPattern {
    pub id: String,
    pub intent: String,
    pub mechanism: Option<String>,
    pub examples: Vec<String>,
    pub scope: serde_json::Value,
    pub confidence: f32,
    pub effective_confidence: f32,
    pub usage_count: i64,
    pub success_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailuresFile {
    pub version: u32,
    pub synced_at: String,
    pub failures: Vec<DistilledFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistilledFailure {
    pub id: String,
    pub cause: String,
    pub avoidance_rule: String,
    pub severity: String,
    pub scope: serde_json::Value,
    pub times_prevented: i64,
}

/// Sync learnings to JSON files
pub async fn sync_learnings(
    patterns: &PatternStore,
    failures: &FailureStore,
    config: &Config,
    graph: Option<&CodeGraph>,
    threshold: f32,
    include_all_critical: bool,
) -> Result<SyncStats> {
    let start = std::time::Instant::now();
    let mut stats = SyncStats {
        patterns_synced: 0,
        failures_synced: 0,
        files_written: Vec::new(),
        duration_ms: 0,
    };

    let now = chrono::Utc::now();
    let now_timestamp = now.timestamp();

    // Get all patterns
    let all_patterns = patterns.list_all().await?;

    // Filter patterns by effective confidence
    let mut distilled_patterns = Vec::new();
    for pattern in &all_patterns {
        let eff_conf = effective_confidence(&pattern, graph, now_timestamp, 90);
        if eff_conf >= threshold {
            distilled_patterns.push(DistilledPattern {
                id: pattern.id.clone(),
                intent: pattern.intent.clone(),
                mechanism: pattern.mechanism.clone(),
                examples: pattern.examples.clone(),
                scope: serde_json::to_value(&pattern.scope)?,
                confidence: pattern.confidence,
                effective_confidence: eff_conf,
                usage_count: pattern.usage_count,
                success_count: pattern.success_count,
            });
        }
    }

    // Sort by ID for deterministic output
    distilled_patterns.sort_by(|a, b| a.id.cmp(&b.id));

    stats.patterns_synced = distilled_patterns.len();

    // Write patterns file
    let patterns_file = PatternsFile {
        version: 1,
        synced_at: now.to_rfc3339(),
        patterns: distilled_patterns,
    };

    let patterns_path = config.codegraph_dir.join("patterns.json");
    let patterns_json = serde_json::to_string_pretty(&patterns_file)?;
    std::fs::write(&patterns_path, patterns_json)?;
    stats.files_written.push(
        patterns_path
            .to_string_lossy()
            .to_string(),
    );

    // Get all failures
    let all_failures = failures.list_all().await?;

    // Filter failures (critical + above threshold)
    let mut distilled_failures = Vec::new();
    for failure in &all_failures {
        let should_include = if include_all_critical {
            failure.severity == Severity::Critical
        } else {
            false
        };

        if should_include {
            distilled_failures.push(distill_failure(failure)?);
        }
    }

    // Sort by ID for deterministic output
    distilled_failures.sort_by(|a, b| a.id.cmp(&b.id));

    stats.failures_synced = distilled_failures.len();

    // Write failures file
    let failures_file = FailuresFile {
        version: 1,
        synced_at: now.to_rfc3339(),
        failures: distilled_failures,
    };

    let failures_path = config.codegraph_dir.join("failures.json");
    let failures_json = serde_json::to_string_pretty(&failures_file)?;
    std::fs::write(&failures_path, failures_json)?;
    stats
        .files_written
        .push(failures_path.to_string_lossy().to_string());

    // Regenerate SKILL.md (if we have the skill module)
    // This would be done by calling distill_project_skill separately

    stats.duration_ms = start.elapsed().as_millis() as u64;

    Ok(stats)
}

fn distill_failure(failure: &Failure) -> Result<DistilledFailure> {
    Ok(DistilledFailure {
        id: failure.id.clone(),
        cause: failure.cause.clone(),
        avoidance_rule: failure.avoidance_rule.clone(),
        severity: format!("{:?}", failure.severity).to_lowercase(),
        scope: serde_json::to_value(&failure.scope)?,
        times_prevented: failure.times_prevented,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::learning::failures::NewFailure;
    use crate::learning::patterns::NewPattern;
    use crate::learning::Scope;
    use crate::store::Store;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn setup_test() -> (PatternStore, FailureStore, Config, TempDir) {
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

        config.ensure_dirs().unwrap();

        let store = Store::open(&config).await.unwrap();
        let pattern_store = PatternStore::new(Arc::new(store.learning_db.clone()));
        let failure_store = FailureStore::new(Arc::new(store.learning_db));

        (pattern_store, failure_store, config, temp_dir)
    }

    #[tokio::test]
    async fn test_sync_learnings() {
        let (pattern_store, failure_store, config, _temp) = setup_test().await;

        // Create a high-confidence pattern
        let pattern = NewPattern {
            intent: "Use async for DB calls".to_string(),
            mechanism: None,
            examples: vec!["async fn query()".to_string()],
            scope: Scope {
                include_paths: vec!["src/db/**".to_string()],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec!["database".to_string()],
            },
            confidence: 0.9,
        };
        pattern_store.create(&pattern).await.unwrap();

        // Create a critical failure
        let failure = NewFailure {
            cause: "SQL injection risk".to_string(),
            avoidance_rule: "Always use parameterized queries".to_string(),
            severity: Severity::Critical,
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
        };
        failure_store.create(&failure).await.unwrap();

        // Sync
        let stats = sync_learnings(
            &pattern_store,
            &failure_store,
            &config,
            None,
            0.7,
            true,
        )
        .await
        .unwrap();

        assert_eq!(stats.patterns_synced, 1);
        assert_eq!(stats.failures_synced, 1);
        assert_eq!(stats.files_written.len(), 2);

        // Verify files exist
        assert!(config.codegraph_dir.join("patterns.json").exists());
        assert!(config.codegraph_dir.join("failures.json").exists());

        // Verify content
        let patterns_content = std::fs::read_to_string(config.codegraph_dir.join("patterns.json")).unwrap();
        assert!(patterns_content.contains("Use async for DB calls"));

        let failures_content = std::fs::read_to_string(config.codegraph_dir.join("failures.json")).unwrap();
        assert!(failures_content.contains("SQL injection"));
    }
}
