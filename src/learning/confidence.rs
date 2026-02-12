use super::patterns::Pattern;
use crate::store::CodeGraph;

/// Calculate effective confidence for a pattern
///
/// Formula:
/// - Base success rate
/// - Time decay (90-day half-life)
/// - Validation recency bonus
/// - Drift penalty (if referenced symbols changed)
/// - Usage momentum (capped at +30%)
pub fn effective_confidence(
    pattern: &Pattern,
    graph: Option<&CodeGraph>,
    now: i64,
    half_life_days: i64,
) -> f32 {
    // Base success rate
    let success_rate = if pattern.usage_count > 0 {
        pattern.success_count as f32 / pattern.usage_count as f32
    } else {
        pattern.confidence // Use original confidence if never used
    };

    // Time decay: exponential decay with configurable half-life
    let age_days = (now - pattern.created_at) / 86400;
    let decay_factor = 0.5_f32.powf(age_days as f32 / half_life_days as f32);

    // Validation recency: boost if recently validated
    let validation_boost = if let Some(last_val) = pattern.last_validated {
        let days_since_validation = (now - last_val) / 86400;
        if days_since_validation < 7 {
            0.1 // +10% if validated in last week
        } else if days_since_validation < 30 {
            0.05 // +5% if validated in last month
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Drift detection: check if symbols in examples still exist and are unchanged
    let drift_penalty = if let Some(g) = graph {
        detect_drift(pattern, g)
    } else {
        0.0 // No penalty if graph not available
    };

    // Usage momentum: log-scale boost for frequently used patterns
    let momentum = if pattern.usage_count > 0 {
        let log_usage = (pattern.usage_count as f32).ln();
        (log_usage * 0.05).min(0.3) // Cap at +30%
    } else {
        0.0
    };

    // Combine all factors
    let effective = success_rate * decay_factor + validation_boost - drift_penalty + momentum;

    // Clamp to [0.0, 1.0]
    effective.max(0.0).min(1.0)
}

/// Detect if a pattern's referenced symbols have changed
fn detect_drift(pattern: &Pattern, graph: &CodeGraph) -> f32 {
    // Extract symbol names from scope
    let symbols = &pattern.scope.symbols;
    if symbols.is_empty() {
        return 0.0; // No symbols to check
    }

    let mut missing_count = 0;
    for symbol_pattern in symbols {
        // Check if any symbol matching the pattern exists
        let found = graph
            .graph
            .node_weights()
            .any(|node| node.id.contains(symbol_pattern) || node.data.get("name")
                .and_then(|v| v.as_str())
                .map(|name| name.contains(symbol_pattern))
                .unwrap_or(false));

        if !found {
            missing_count += 1;
        }
    }

    // Penalty proportional to missing symbols
    let missing_ratio = missing_count as f32 / symbols.len() as f32;
    missing_ratio * 0.3 // Up to -30% penalty for drift
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::Scope;
    use crate::store::graph::{CodeGraph, NodeData};
    use chrono::Utc;
    use serde_json::json;

    #[test]
    fn test_confidence_no_usage() {
        let pattern = Pattern {
            id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.8,
            usage_count: 0,
            success_count: 0,
            last_validated: None,
            created_at: Utc::now().timestamp() - 86400, // 1 day ago
            updated_at: Utc::now().timestamp(),
        };

        let now = Utc::now().timestamp();
        let eff = effective_confidence(&pattern, None, now, 90);

        // Should be close to base confidence with slight decay
        assert!(eff > 0.7 && eff < 0.85);
    }

    #[test]
    fn test_confidence_with_success() {
        let pattern = Pattern {
            id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.8,
            usage_count: 10,
            success_count: 9,
            last_validated: None,
            created_at: Utc::now().timestamp() - 86400, // 1 day ago
            updated_at: Utc::now().timestamp(),
        };

        let now = Utc::now().timestamp();
        let eff = effective_confidence(&pattern, None, now, 90);

        // Should have high confidence due to good success rate
        assert!(eff > 0.8);
    }

    #[test]
    fn test_confidence_time_decay() {
        let old_pattern = Pattern {
            id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.9,
            usage_count: 10,
            success_count: 10,
            last_validated: None,
            created_at: Utc::now().timestamp() - 180 * 86400, // 180 days ago
            updated_at: Utc::now().timestamp(),
        };

        let now = Utc::now().timestamp();
        let eff = effective_confidence(&old_pattern, None, now, 90);

        // Should have significant decay after 180 days (2 half-lives)
        assert!(eff < 0.5);
    }

    #[test]
    fn test_confidence_validation_boost() {
        let pattern = Pattern {
            id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.7,
            usage_count: 5,
            success_count: 5,
            last_validated: Some(Utc::now().timestamp() - 3 * 86400), // 3 days ago
            created_at: Utc::now().timestamp() - 30 * 86400,
            updated_at: Utc::now().timestamp(),
        };

        let now = Utc::now().timestamp();
        let eff = effective_confidence(&pattern, None, now, 90);

        // Should have validation boost
        assert!(eff > 0.7);
    }

    #[test]
    fn test_confidence_momentum() {
        let high_usage = Pattern {
            id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags: vec![],
            },
            confidence: 0.6,
            usage_count: 100,
            success_count: 100,
            last_validated: None,
            created_at: Utc::now().timestamp() - 10 * 86400,
            updated_at: Utc::now().timestamp(),
        };

        let now = Utc::now().timestamp();
        let eff = effective_confidence(&high_usage, None, now, 90);

        // High usage should provide momentum boost
        assert!(eff > 0.9);
    }

    #[test]
    fn test_drift_detection() {
        let mut graph = CodeGraph::new();
        graph.add_node(
            "src/test.rs::MyStruct".to_string(),
            "struct".to_string(),
            json!({"name": "MyStruct"}),
        );

        let pattern_with_existing_symbol = Pattern {
            id: "test".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec!["MyStruct".to_string()],
                tags: vec![],
            },
            confidence: 0.8,
            usage_count: 5,
            success_count: 5,
            last_validated: None,
            created_at: Utc::now().timestamp() - 10 * 86400,
            updated_at: Utc::now().timestamp(),
        };

        let now = Utc::now().timestamp();
        let eff = effective_confidence(&pattern_with_existing_symbol, Some(&graph), now, 90);

        // Should not have drift penalty since symbol exists
        assert!(eff > 0.7);

        // Pattern with missing symbol
        let pattern_with_missing_symbol = Pattern {
            id: "test2".to_string(),
            intent: "Test".to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec!["NonExistent".to_string()],
                tags: vec![],
            },
            confidence: 0.8,
            usage_count: 5,
            success_count: 5,
            last_validated: None,
            created_at: Utc::now().timestamp() - 10 * 86400,
            updated_at: Utc::now().timestamp(),
        };

        let eff2 = effective_confidence(&pattern_with_missing_symbol, Some(&graph), now, 90);

        // Should have drift penalty
        assert!(eff2 < eff);
    }
}
