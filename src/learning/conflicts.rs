use super::patterns::Pattern;
use super::Scope;

#[derive(Debug, Clone)]
pub struct Conflict {
    pub pattern_a: String,
    pub pattern_b: String,
    pub reason: String,
    pub resolution: ConflictResolution,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConflictResolution {
    PreferA,
    PreferB,
    RequiresHumanReview,
}

/// Detect contradictory patterns
pub fn detect_conflicts(patterns: &[Pattern]) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    for i in 0..patterns.len() {
        for j in (i + 1)..patterns.len() {
            let pattern_a = &patterns[i];
            let pattern_b = &patterns[j];

            // Check if scopes overlap
            if !scopes_overlap(&pattern_a.scope, &pattern_b.scope) {
                continue;
            }

            // Check for contradictory content
            if let Some(reason) = detect_contradiction(pattern_a, pattern_b) {
                let resolution = resolve_conflict(pattern_a, pattern_b);

                conflicts.push(Conflict {
                    pattern_a: pattern_a.id.clone(),
                    pattern_b: pattern_b.id.clone(),
                    reason,
                    resolution,
                });
            }
        }
    }

    conflicts
}

/// Check if two scopes overlap
fn scopes_overlap(scope_a: &Scope, scope_b: &Scope) -> bool {
    // If both scopes are empty, they overlap everywhere
    if scope_a.include_paths.is_empty()
        && scope_a.exclude_paths.is_empty()
        && scope_b.include_paths.is_empty()
        && scope_b.exclude_paths.is_empty()
    {
        return true;
    }

    // Check path overlap
    if !scope_a.include_paths.is_empty() && !scope_b.include_paths.is_empty() {
        // Check if there's any path that matches both include patterns
        let has_overlap = scope_a.include_paths.iter().any(|path_a| {
            scope_b.include_paths.iter().any(|path_b| {
                patterns_overlap(path_a, path_b)
            })
        });
        if !has_overlap {
            return false;
        }
    }

    // Check tag overlap
    if !scope_a.tags.is_empty() && !scope_b.tags.is_empty() {
        let has_common_tag = scope_a.tags.iter().any(|tag| scope_b.tags.contains(tag));
        if !has_common_tag {
            return false;
        }
    }

    true
}

/// Check if two glob patterns overlap
fn patterns_overlap(pattern_a: &str, pattern_b: &str) -> bool {
    // Simple heuristic: check if patterns share common path segments
    let segments_a: Vec<&str> = pattern_a.split('/').collect();
    let segments_b: Vec<&str> = pattern_b.split('/').collect();

    // Check for common non-wildcard segments
    for seg_a in &segments_a {
        if seg_a.contains('*') {
            continue;
        }
        for seg_b in &segments_b {
            if seg_b.contains('*') {
                continue;
            }
            if seg_a == seg_b {
                return true;
            }
        }
    }

    // If both contain wildcards, assume they might overlap
    if pattern_a.contains('*') && pattern_b.contains('*') {
        return true;
    }

    false
}

/// Detect if two patterns contradict each other
fn detect_contradiction(pattern_a: &Pattern, pattern_b: &Pattern) -> Option<String> {
    // Tokenize descriptions
    let tokens_a = tokenize(&pattern_a.intent);
    let tokens_b = tokenize(&pattern_b.intent);

    // Compute Jaccard similarity
    let similarity = jaccard_similarity(&tokens_a, &tokens_b);

    // If high similarity (>0.6), check for opposing sentiment
    if similarity > 0.6 {
        let sentiment_a = detect_sentiment(&pattern_a.intent);
        let sentiment_b = detect_sentiment(&pattern_b.intent);

        if sentiment_a != sentiment_b && sentiment_a != 0 && sentiment_b != 0 {
            return Some(format!(
                "Similar topics with opposing advice: '{}' vs '{}'",
                pattern_a.intent, pattern_b.intent
            ));
        }
    }

    None
}

/// Tokenize a string into words
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2) // Filter short words
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Compute Jaccard similarity between two token sets
fn jaccard_similarity(tokens_a: &[String], tokens_b: &[String]) -> f64 {
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    if tokens_a.is_empty() || tokens_b.is_empty() {
        return 0.0;
    }

    let set_a: std::collections::HashSet<_> = tokens_a.iter().collect();
    let set_b: std::collections::HashSet<_> = tokens_b.iter().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Detect sentiment (positive, negative, or neutral)
/// Returns: 1 for positive, -1 for negative, 0 for neutral
fn detect_sentiment(text: &str) -> i32 {
    let text_lower = text.to_lowercase();

    // Check for negation phrases first — these override individual word counting
    let negation_phrases = ["don't use", "never use", "avoid using", "do not use", "stop using"];
    let affirmation_phrases = ["always use", "prefer using", "should use", "ensure using"];

    let has_negation = negation_phrases.iter().any(|p| text_lower.contains(p));
    let has_affirmation = affirmation_phrases.iter().any(|p| text_lower.contains(p));

    if has_negation && !has_affirmation {
        return -1;
    }
    if has_affirmation && !has_negation {
        return 1;
    }

    // Fall back to individual word counting
    let negative_words = ["don't", "never", "avoid", "not", "no", "prevent", "stop"];
    let positive_words = ["always", "prefer", "do", "yes", "ensure", "should"];

    let negative_count = negative_words.iter().filter(|w| text_lower.contains(*w)).count();
    let positive_count = positive_words.iter().filter(|w| text_lower.contains(*w)).count();

    if negative_count > positive_count {
        -1
    } else if positive_count > negative_count {
        1
    } else {
        0
    }
}

/// Resolve conflict between two patterns
fn resolve_conflict(pattern_a: &Pattern, pattern_b: &Pattern) -> ConflictResolution {
    let confidence_diff = (pattern_a.confidence - pattern_b.confidence).abs();

    // If confidence gap > 0.2, prefer the higher confidence one
    if confidence_diff > 0.2 {
        if pattern_a.confidence > pattern_b.confidence {
            ConflictResolution::PreferA
        } else {
            ConflictResolution::PreferB
        }
    } else {
        // Otherwise, requires human review
        ConflictResolution::RequiresHumanReview
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pattern(id: &str, intent: &str, confidence: f32, tags: Vec<String>) -> Pattern {
        Pattern {
            id: id.to_string(),
            intent: intent.to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: vec![],
                exclude_paths: vec![],
                symbols: vec![],
                tags,
            },
            confidence,
            usage_count: 0,
            success_count: 0,
            last_validated: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn test_detect_conflicts() {
        let patterns = vec![
            make_pattern(
                "1",
                "Always use async for database queries",
                0.9,
                vec!["database".to_string()],
            ),
            make_pattern(
                "2",
                "Never use async for database queries",
                0.5,
                vec!["database".to_string()],
            ),
        ];

        let conflicts = detect_conflicts(&patterns);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].resolution, ConflictResolution::PreferA);
    }

    #[test]
    fn test_no_conflict_different_scopes() {
        let patterns = vec![
            make_pattern(
                "1",
                "Use async for queries",
                0.9,
                vec!["database".to_string()],
            ),
            make_pattern(
                "2",
                "Use sync for queries",
                0.8,
                vec!["cache".to_string()],
            ),
        ];

        let conflicts = detect_conflicts(&patterns);
        assert_eq!(conflicts.len(), 0);
    }

    #[test]
    fn test_jaccard_similarity() {
        let tokens_a = vec!["use".to_string(), "async".to_string(), "database".to_string()];
        let tokens_b = vec!["use".to_string(), "sync".to_string(), "database".to_string()];

        let similarity = jaccard_similarity(&tokens_a, &tokens_b);
        assert!(similarity >= 0.5);
        assert!(similarity < 1.0);
    }

    #[test]
    fn test_sentiment_detection() {
        assert_eq!(detect_sentiment("Always use async"), 1);
        assert_eq!(detect_sentiment("Never use sync"), -1);
        assert_eq!(detect_sentiment("Consider using async"), 0);
    }
}
