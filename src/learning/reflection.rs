use anyhow::{Context as AnyhowContext, Result};

use super::failures::{FailureStore, NewFailure, Severity};
use super::lineage::{LineageStore, Outcome};
use super::patterns::{NewPattern, Pattern, PatternStore};
use super::{failures::Failure, Scope};

pub struct ReflectionInput {
    pub attempt_id: String,
    pub intent: String,
    pub mechanism: Option<String>,
    pub root_cause: String,
    pub lesson: String,
    pub confidence: Option<f32>,
    pub scope_paths: Vec<String>,
    pub scope_tags: Vec<String>,
}

pub enum ReflectionResult {
    PatternCreated(Pattern),
    FailureRecorded(Failure),
    Both { pattern: Pattern, failure: Failure },
}

/// Reflect on a solution and create pattern or failure
pub async fn reflect(
    input: &ReflectionInput,
    lineage: &LineageStore,
    patterns: &PatternStore,
    failures: &FailureStore,
    validate: bool,
) -> Result<ReflectionResult> {
    // Get the solution
    let solution = lineage
        .get(&input.attempt_id)
        .await?
        .context("Solution not found")?;

    // Validate reflection if enabled
    if validate {
        validate_reflection(input)?;
    }

    // Build scope from input
    let scope = Scope {
        include_paths: input.scope_paths.clone(),
        exclude_paths: vec![],
        symbols: vec![],
        tags: input.scope_tags.clone(),
    };

    match solution.outcome {
        Outcome::Success => {
            // Create pattern from successful solution
            let new_pattern = NewPattern {
                intent: input.intent.clone(),
                mechanism: input.mechanism.clone(),
                examples: vec![input.lesson.clone()],
                scope: scope.clone(),
                confidence: input.confidence.unwrap_or(0.7),
            };

            let pattern = patterns.create(&new_pattern).await?;
            Ok(ReflectionResult::PatternCreated(pattern))
        }
        Outcome::Failure => {
            // Create failure from failed solution
            let new_failure = NewFailure {
                cause: input.root_cause.clone(),
                avoidance_rule: input.lesson.clone(),
                severity: infer_severity(&input.root_cause),
                scope: scope.clone(),
            };

            let failure = failures.create(&new_failure).await?;
            Ok(ReflectionResult::FailureRecorded(failure))
        }
        Outcome::Partial => {
            // Create both pattern (for what worked) and failure (for what didn't)
            let new_pattern = NewPattern {
                intent: input.intent.clone(),
                mechanism: input.mechanism.clone(),
                examples: vec![input.lesson.clone()],
                scope: scope.clone(),
                confidence: input.confidence.unwrap_or(0.5),
            };

            let new_failure = NewFailure {
                cause: input.root_cause.clone(),
                avoidance_rule: format!("Partial success. {}", input.lesson),
                severity: Severity::Minor,
                scope: scope.clone(),
            };

            let pattern = patterns.create(&new_pattern).await?;
            let failure = failures.create(&new_failure).await?;

            Ok(ReflectionResult::Both { pattern, failure })
        }
    }
}

/// Validate reflection input
fn validate_reflection(input: &ReflectionInput) -> Result<()> {
    // Check that root_cause is not generic
    let generic_causes = [
        "it failed",
        "syntax error",
        "error occurred",
        "didn't work",
        "broke",
    ];

    let root_cause_lower = input.root_cause.to_lowercase();
    for generic in &generic_causes {
        if root_cause_lower.contains(generic) && root_cause_lower.len() < 30 {
            return Err(anyhow::anyhow!(
                "Root cause is too generic. Please provide specific details about what went wrong."
            ));
        }
    }

    // Check that lesson follows "When X, do Y because Z" format
    let lesson_lower = input.lesson.to_lowercase();
    let has_when = lesson_lower.contains("when");
    let has_action = lesson_lower.contains("do")
        || lesson_lower.contains("use")
        || lesson_lower.contains("avoid")
        || lesson_lower.contains("never")
        || lesson_lower.contains("always");
    let _has_reason =
        lesson_lower.contains("because") || lesson_lower.contains("since") || lesson_lower.contains("to");

    if !has_when || !has_action {
        eprintln!(
            "Warning: Lesson should follow 'When X, do Y because Z' format for better reusability"
        );
    }

    // Check confidence is reasonable
    if let Some(conf) = input.confidence {
        if conf < 0.0 || conf > 1.0 {
            return Err(anyhow::anyhow!("Confidence must be between 0.0 and 1.0"));
        }
    }

    Ok(())
}

/// Infer severity from root cause description
fn infer_severity(root_cause: &str) -> Severity {
    let cause_lower = root_cause.to_lowercase();

    // Critical indicators
    if cause_lower.contains("security")
        || cause_lower.contains("vulnerability")
        || cause_lower.contains("data loss")
        || cause_lower.contains("corruption")
    {
        return Severity::Critical;
    }

    // Major indicators
    if cause_lower.contains("crash")
        || cause_lower.contains("panic")
        || cause_lower.contains("deadlock")
        || cause_lower.contains("race condition")
    {
        return Severity::Major;
    }

    // Default to minor
    Severity::Minor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::learning::lineage::Metrics;
    use crate::store::Store;
    use std::sync::Arc;
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
    async fn test_reflect_success() {
        let (store, _temp) = setup_test_store().await;
        let db = Arc::new(store.learning_db);

        let lineage = LineageStore::new(db.clone());
        let patterns = PatternStore::new(db.clone());
        let failures = FailureStore::new(db.clone());

        // Create successful solution
        let id = lineage
            .record_attempt("Add feature", "Use approach A", None, None)
            .await
            .unwrap();
        lineage
            .record_outcome(&id, Outcome::Success, None, &[], &[])
            .await
            .unwrap();

        // Reflect
        let input = ReflectionInput {
            attempt_id: id,
            intent: "Add caching layer".to_string(),
            mechanism: Some("Use in-memory cache".to_string()),
            root_cause: "N/A".to_string(),
            lesson: "When adding caching, use in-memory cache because it's faster".to_string(),
            confidence: Some(0.8),
            scope_paths: vec!["src/cache/**".to_string()],
            scope_tags: vec!["performance".to_string()],
        };

        let result = reflect(&input, &lineage, &patterns, &failures, false)
            .await
            .unwrap();

        match result {
            ReflectionResult::PatternCreated(p) => {
                assert_eq!(p.intent, "Add caching layer");
                assert_eq!(p.confidence, 0.8);
            }
            _ => panic!("Expected PatternCreated"),
        }
    }

    #[tokio::test]
    async fn test_reflect_failure() {
        let (store, _temp) = setup_test_store().await;
        let db = Arc::new(store.learning_db);

        let lineage = LineageStore::new(db.clone());
        let patterns = PatternStore::new(db.clone());
        let failures = FailureStore::new(db.clone());

        // Create failed solution
        let id = lineage
            .record_attempt("Fix bug", "Try approach A", None, None)
            .await
            .unwrap();
        lineage
            .record_outcome(&id, Outcome::Failure, None, &[], &[])
            .await
            .unwrap();

        // Reflect
        let input = ReflectionInput {
            attempt_id: id,
            intent: "Fix deadlock".to_string(),
            mechanism: None,
            root_cause: "Incorrect lock ordering caused deadlock".to_string(),
            lesson: "When using multiple locks, always acquire in consistent order".to_string(),
            confidence: None,
            scope_paths: vec![],
            scope_tags: vec!["concurrency".to_string()],
        };

        let result = reflect(&input, &lineage, &patterns, &failures, false)
            .await
            .unwrap();

        match result {
            ReflectionResult::FailureRecorded(f) => {
                assert!(f.cause.contains("deadlock"));
                assert_eq!(f.severity, Severity::Major);
            }
            _ => panic!("Expected FailureRecorded"),
        }
    }

    #[tokio::test]
    async fn test_severity_inference() {
        assert_eq!(
            infer_severity("SQL injection vulnerability found"),
            Severity::Critical
        );
        assert_eq!(infer_severity("Application crashed"), Severity::Major);
        assert_eq!(infer_severity("Minor display issue"), Severity::Minor);
    }

    #[test]
    fn test_validation() {
        let good_input = ReflectionInput {
            attempt_id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            root_cause: "The function didn't handle null pointers correctly, leading to segfault".to_string(),
            lesson: "When handling user input, always validate for null because it prevents crashes".to_string(),
            confidence: Some(0.8),
            scope_paths: vec![],
            scope_tags: vec![],
        };

        assert!(validate_reflection(&good_input).is_ok());

        let bad_input = ReflectionInput {
            attempt_id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            root_cause: "it failed".to_string(),
            lesson: "fix it".to_string(),
            confidence: Some(1.5),
            scope_paths: vec![],
            scope_tags: vec![],
        };

        assert!(validate_reflection(&bad_input).is_err());
    }
}
