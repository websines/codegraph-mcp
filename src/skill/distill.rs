use anyhow::Result;
use libsql::Connection;
use std::sync::Arc;

use crate::learning::failures::{Failure, FailureStore, Severity};
use crate::learning::patterns::{Pattern, PatternStore};

use super::categories::{InstructionCategory, InstructionSource, ProjectInstruction};
use super::conventions::{cluster_conventions, Convention};
use super::navigation::{generate_navigation_hints, NavigationHint};

pub struct DistillResult {
    pub instructions: Vec<ProjectInstruction>,
    pub conventions: Vec<Convention>,
    pub navigation_hints: Vec<NavigationHint>,
}

/// Distill project skill from patterns and failures
pub async fn distill_project_skill(
    pattern_store: &PatternStore,
    failure_store: &FailureStore,
    manual_store: &ManualInstructionStore,
    confidence_threshold: f32,
) -> Result<DistillResult> {
    let mut instructions = Vec::new();

    // Get high-confidence patterns
    let patterns = pattern_store.list_all().await?;
    let high_conf_patterns: Vec<_> = patterns
        .iter()
        .filter(|p| p.confidence >= confidence_threshold)
        .cloned()
        .collect();

    // Convert patterns to "Do" instructions
    for pattern in &high_conf_patterns {
        let category = infer_category_from_pattern(pattern);
        instructions.push(ProjectInstruction {
            id: pattern.id.clone(),
            instruction: format_do_instruction(pattern),
            category,
            source: InstructionSource::Pattern {
                id: pattern.id.clone(),
            },
            confidence: Some(pattern.confidence),
            scope: format_scope(&pattern.scope.include_paths),
        });
    }

    // Get critical and major failures
    let failures = failure_store.list_all().await?;
    let important_failures: Vec<_> = failures
        .iter()
        .filter(|f| matches!(f.severity, Severity::Critical | Severity::Major))
        .cloned()
        .collect();

    // Convert failures to "Don't" instructions
    for failure in &important_failures {
        instructions.push(ProjectInstruction {
            id: failure.id.clone(),
            instruction: format_dont_instruction(failure),
            category: InstructionCategory::Gotchas,
            source: InstructionSource::Failure {
                id: failure.id.clone(),
            },
            confidence: None,
            scope: format_scope(&failure.scope.include_paths),
        });
    }

    // Cluster patterns into conventions
    let conventions = cluster_conventions(&high_conf_patterns, 3);

    // Add convention instructions
    for convention in &conventions {
        instructions.push(ProjectInstruction {
            id: format!("conv_{}", instructions.len()),
            instruction: convention.summary.clone(),
            category: InstructionCategory::Architecture,
            source: InstructionSource::Convention {
                pattern_ids: convention.patterns.clone(),
            },
            confidence: None,
            scope: convention.common_prefix.clone(),
        });
    }

    // Generate navigation hints
    let navigation_hints = generate_navigation_hints(&high_conf_patterns);

    // Add navigation instructions
    for hint in &navigation_hints {
        instructions.push(ProjectInstruction {
            id: format!("nav_{}", instructions.len()),
            instruction: format!("`{}` contains {}", hint.path, hint.description),
            category: InstructionCategory::Navigation,
            source: InstructionSource::Manual { reason: None },
            confidence: None,
            scope: Some(hint.path.clone()),
        });
    }

    // Add manual instructions
    let manual = manual_store.list_all().await?;
    instructions.extend(manual);

    Ok(DistillResult {
        instructions,
        conventions,
        navigation_hints,
    })
}

/// Infer instruction category from pattern
fn infer_category_from_pattern(pattern: &Pattern) -> InstructionCategory {
    let intent_lower = pattern.intent.to_lowercase();

    if intent_lower.contains("test") {
        InstructionCategory::Testing
    } else if intent_lower.contains("format")
        || intent_lower.contains("style")
        || intent_lower.contains("naming")
    {
        InstructionCategory::Style
    } else if intent_lower.contains("build")
        || intent_lower.contains("deploy")
        || intent_lower.contains("workflow")
    {
        InstructionCategory::Workflow
    } else if intent_lower.contains("tool") || intent_lower.contains("cli") {
        InstructionCategory::Tooling
    } else {
        InstructionCategory::Architecture
    }
}

/// Format a pattern as a "Do" instruction
fn format_do_instruction(pattern: &Pattern) -> String {
    if let Some(mechanism) = &pattern.mechanism {
        format!("{} — {}", pattern.intent, mechanism)
    } else {
        pattern.intent.clone()
    }
}

/// Format a failure as a "Don't" instruction
fn format_dont_instruction(failure: &Failure) -> String {
    format!("❌ {} — {}", failure.cause, failure.avoidance_rule)
}

/// Format scope paths for display
fn format_scope(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        None
    } else if paths.len() == 1 {
        Some(paths[0].clone())
    } else {
        Some(paths.join(", "))
    }
}

/// Store for manual instructions
pub struct ManualInstructionStore {
    db: Arc<Connection>,
}

impl ManualInstructionStore {
    pub fn new(db: Arc<Connection>) -> Self {
        Self { db }
    }

    pub async fn add(
        &self,
        instruction: &str,
        category: InstructionCategory,
        reason: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        self.db
            .execute(
                "INSERT INTO instructions (id, instruction, category, reason, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                libsql::params![
                    id.as_str(),
                    instruction,
                    category.to_str(),
                    reason.unwrap_or(""),
                    now
                ],
            )
            .await?;

        Ok(id)
    }

    pub async fn list_all(&self) -> Result<Vec<ProjectInstruction>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, instruction, category, reason FROM instructions ORDER BY created_at",
                (),
            )
            .await?;

        let mut instructions = Vec::new();

        while let Some(row) = rows.next().await? {
            let reason: String = row.get(3)?;
            instructions.push(ProjectInstruction {
                id: row.get(0)?,
                instruction: row.get(1)?,
                category: InstructionCategory::from_str(&row.get::<String>(2)?),
                source: InstructionSource::Manual {
                    reason: if reason.is_empty() {
                        None
                    } else {
                        Some(reason)
                    },
                },
                confidence: None,
                scope: None,
            });
        }

        Ok(instructions)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM instructions WHERE id = ?1", [id])
            .await?;
        Ok(())
    }
}

use crate::learning::Scope;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_category() {
        let pattern = Pattern {
            id: "1".to_string(),
            intent: "Always write unit tests".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.9,
            usage_count: 0,
            success_count: 0,
            last_validated: None,
            created_at: 0,
            updated_at: 0,
        };

        assert_eq!(
            infer_category_from_pattern(&pattern),
            InstructionCategory::Testing
        );
    }
}
