use crate::learning::patterns::Pattern;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct NavigationHint {
    pub path: String,
    pub description: String,
}

/// Generate navigation hints from patterns
pub fn generate_navigation_hints(patterns: &[Pattern]) -> Vec<NavigationHint> {
    let mut path_groups: HashMap<String, Vec<String>> = HashMap::new();

    // Group patterns by path prefix
    for pattern in patterns {
        for path in &pattern.scope.include_paths {
            let prefix = extract_directory_prefix(path);
            if prefix.len() > 3 {
                path_groups
                    .entry(prefix)
                    .or_default()
                    .push(pattern.intent.clone());
            }
        }
    }

    // Generate hints
    let mut hints = Vec::new();
    for (path, intents) in path_groups {
        if intents.len() >= 2 {
            let description = generate_hint_description(&path, &intents);
            hints.push(NavigationHint { path, description });
        }
    }

    // Sort by path for consistent output
    hints.sort_by(|a, b| a.path.cmp(&b.path));

    hints
}

/// Extract directory prefix from a glob pattern
fn extract_directory_prefix(pattern: &str) -> String {
    // Remove glob patterns and trailing slashes
    pattern
        .split("**")
        .next()
        .unwrap_or(pattern)
        .split('*')
        .next()
        .unwrap_or(pattern)
        .trim_end_matches('/')
        .to_string()
}

/// Generate a description for a path based on pattern intents
fn generate_hint_description(path: &str, intents: &[String]) -> String {
    // Try to infer purpose from path name
    let path_lower = path.to_lowercase();

    let inferred = if path_lower.contains("test") {
        "tests"
    } else if path_lower.contains("util") || path_lower.contains("helper") {
        "utility code"
    } else if path_lower.contains("api") || path_lower.contains("routes") {
        "API endpoints"
    } else if path_lower.contains("model") || path_lower.contains("schema") {
        "data models"
    } else if path_lower.contains("component") {
        "UI components"
    } else if path_lower.contains("service") {
        "business logic"
    } else if path_lower.contains("store") || path_lower.contains("db") {
        "database layer"
    } else {
        ""
    };

    if !inferred.is_empty() {
        format!("{}", inferred)
    } else {
        // Fall back to pattern count
        if intents.len() == 1 {
            intents[0].clone()
        } else {
            format!("{} related patterns", intents.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_directory_prefix() {
        assert_eq!(extract_directory_prefix("src/db/**/*.rs"), "src/db");
        assert_eq!(extract_directory_prefix("src/api/"), "src/api");
        assert_eq!(extract_directory_prefix("tests/*.py"), "tests");
    }

    #[test]
    fn test_generate_hints() {
        use crate::learning::Scope;

        let patterns = vec![
            Pattern {
                id: "1".to_string(),
                intent: "Use connection pooling".to_string(),
                mechanism: None,
                examples: vec![],
                scope: Scope {
                    include_paths: vec!["src/db/**".to_string()],
                    exclude_paths: vec![],
                    symbols: vec![],
                    tags: vec![],
                },
                confidence: 0.8,
                usage_count: 0,
                success_count: 0,
                last_validated: None,
                created_at: 0,
                updated_at: 0,
            },
            Pattern {
                id: "2".to_string(),
                intent: "Use prepared statements".to_string(),
                mechanism: None,
                examples: vec![],
                scope: Scope {
                    include_paths: vec!["src/db/**".to_string()],
                    exclude_paths: vec![],
                    symbols: vec![],
                    tags: vec![],
                },
                confidence: 0.8,
                usage_count: 0,
                success_count: 0,
                last_validated: None,
                created_at: 0,
                updated_at: 0,
            },
        ];

        let hints = generate_navigation_hints(&patterns);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].path, "src/db");
        assert!(hints[0].description.contains("database"));
    }
}
