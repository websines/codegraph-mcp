use crate::learning::patterns::Pattern;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Convention {
    pub patterns: Vec<String>, // Pattern IDs
    pub common_prefix: Option<String>,
    pub common_tags: Vec<String>,
    pub summary: String,
}

/// Cluster patterns into conventions
pub fn cluster_conventions(patterns: &[Pattern], min_cluster_size: usize) -> Vec<Convention> {
    let mut conventions = Vec::new();

    // Cluster by path prefix
    let by_prefix = cluster_by_path_prefix(patterns);
    for (prefix, pattern_ids) in by_prefix {
        if pattern_ids.len() >= min_cluster_size {
            let summary = generate_convention_summary(Some(&prefix), &pattern_ids, patterns);
            conventions.push(Convention {
                patterns: pattern_ids,
                common_prefix: Some(prefix),
                common_tags: vec![],
                summary,
            });
        }
    }

    // Cluster by tags
    let by_tags = cluster_by_tags(patterns);
    for (tags, pattern_ids) in by_tags {
        if pattern_ids.len() >= min_cluster_size {
            // Check if not already covered by path clustering
            if !conventions.iter().any(|c| {
                c.patterns.iter().all(|p| pattern_ids.contains(p))
            }) {
                let summary = generate_tag_summary(&tags, &pattern_ids, patterns);
                conventions.push(Convention {
                    patterns: pattern_ids,
                    common_prefix: None,
                    common_tags: tags,
                    summary,
                });
            }
        }
    }

    conventions
}

/// Cluster patterns by shared path prefix
fn cluster_by_path_prefix(patterns: &[Pattern]) -> HashMap<String, Vec<String>> {
    let mut clusters: HashMap<String, Vec<String>> = HashMap::new();

    for pattern in patterns {
        if pattern.scope.include_paths.is_empty() {
            continue;
        }

        for path in &pattern.scope.include_paths {
            // Extract directory prefix (before **)
            let prefix = path
                .split("**")
                .next()
                .unwrap_or(path)
                .trim_end_matches('/')
                .to_string();

            if prefix.len() > 3 {
                // Only meaningful prefixes
                clusters
                    .entry(prefix)
                    .or_default()
                    .push(pattern.id.clone());
            }
        }
    }

    clusters
}

/// Cluster patterns by shared tags
fn cluster_by_tags(patterns: &[Pattern]) -> HashMap<Vec<String>, Vec<String>> {
    let mut clusters: HashMap<Vec<String>, Vec<String>> = HashMap::new();

    for pattern in patterns {
        if pattern.scope.tags.is_empty() {
            continue;
        }

        let mut tags = pattern.scope.tags.clone();
        tags.sort();

        clusters
            .entry(tags)
            .or_default()
            .push(pattern.id.clone());
    }

    clusters
}

/// Generate a convention summary from patterns
fn generate_convention_summary(
    prefix: Option<&str>,
    pattern_ids: &[String],
    all_patterns: &[Pattern],
) -> String {
    let patterns: Vec<_> = all_patterns
        .iter()
        .filter(|p| pattern_ids.contains(&p.id))
        .collect();

    if patterns.is_empty() {
        return String::new();
    }

    if let Some(prefix) = prefix {
        format!(
            "Code in `{}`: {}",
            prefix,
            summarize_intents(&patterns)
        )
    } else {
        summarize_intents(&patterns)
    }
}

/// Generate a summary from tag-based cluster
fn generate_tag_summary(
    tags: &[String],
    pattern_ids: &[String],
    all_patterns: &[Pattern],
) -> String {
    let patterns: Vec<_> = all_patterns
        .iter()
        .filter(|p| pattern_ids.contains(&p.id))
        .collect();

    format!(
        "For {} code: {}",
        tags.join("/"),
        summarize_intents(&patterns)
    )
}

/// Summarize the intents of a group of patterns
fn summarize_intents(patterns: &[&Pattern]) -> String {
    if patterns.len() == 1 {
        return patterns[0].intent.clone();
    }

    // Find common themes
    let themes: Vec<_> = patterns.iter().take(3).map(|p| p.intent.as_str()).collect();

    if themes.len() <= 2 {
        themes.join("; ")
    } else {
        format!("{} and {} more patterns", themes[0], patterns.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::Scope;

    fn make_pattern(id: &str, intent: &str, paths: Vec<String>, tags: Vec<String>) -> Pattern {
        Pattern {
            id: id.to_string(),
            intent: intent.to_string(),
            mechanism: None,
            examples: vec![],
            scope: Scope {
                include_paths: paths,
                exclude_paths: vec![],
                symbols: vec![],
                tags,
            },
            confidence: 0.8,
            usage_count: 0,
            success_count: 0,
            last_validated: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn test_cluster_by_path() {
        let patterns = vec![
            make_pattern("1", "Use async", vec!["src/db/**".to_string()], vec![]),
            make_pattern("2", "Use prepared statements", vec!["src/db/**".to_string()], vec![]),
            make_pattern("3", "Use connection pooling", vec!["src/db/**".to_string()], vec![]),
        ];

        let conventions = cluster_conventions(&patterns, 3);
        assert_eq!(conventions.len(), 1);
        assert_eq!(conventions[0].patterns.len(), 3);
    }

    #[test]
    fn test_cluster_by_tags() {
        let patterns = vec![
            make_pattern("1", "Pattern A", vec![], vec!["async".to_string()]),
            make_pattern("2", "Pattern B", vec![], vec!["async".to_string()]),
            make_pattern("3", "Pattern C", vec![], vec!["async".to_string()]),
        ];

        let conventions = cluster_conventions(&patterns, 3);
        assert_eq!(conventions.len(), 1);
    }
}
