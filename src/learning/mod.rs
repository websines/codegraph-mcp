pub mod confidence;
pub mod conflicts;
pub mod failures;
pub mod lineage;
pub mod niches;
pub mod patterns;
pub mod reflection;
pub mod sync;

use serde::{Deserialize, Serialize};

/// Scope defines where a pattern or failure applies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    /// Include path patterns (glob)
    #[serde(default)]
    pub include_paths: Vec<String>,
    /// Exclude path patterns (glob)
    #[serde(default)]
    pub exclude_paths: Vec<String>,
    /// Symbol name substrings
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Scope {
    /// Check if this scope matches the given context
    pub fn matches(&self, file_path: Option<&str>, symbols: &[String], tags: &[String]) -> bool {
        // If scope is completely empty, it matches everything
        if self.include_paths.is_empty()
            && self.exclude_paths.is_empty()
            && self.symbols.is_empty()
            && self.tags.is_empty()
        {
            return true;
        }

        // Check path patterns
        if let Some(path) = file_path {
            // If there are include patterns, path must match at least one
            if !self.include_paths.is_empty() {
                let matches_include = self
                    .include_paths
                    .iter()
                    .any(|pattern| glob_match(pattern, path));
                if !matches_include {
                    return false;
                }
            }

            // Path must not match any exclude pattern
            if self
                .exclude_paths
                .iter()
                .any(|pattern| glob_match(pattern, path))
            {
                return false;
            }
        }

        // Check symbol matching (only when BOTH scope and query have symbols)
        if !self.symbols.is_empty() && !symbols.is_empty() {
            let matches_symbol = self.symbols.iter().any(|pattern| {
                symbols
                    .iter()
                    .any(|sym| sym.to_lowercase().contains(&pattern.to_lowercase()))
            });
            if !matches_symbol {
                return false;
            }
        }

        // Check tag intersection (only when BOTH scope and query have tags)
        if !self.tags.is_empty() && !tags.is_empty() {
            let has_common_tag = self.tags.iter().any(|t| tags.contains(t));
            if !has_common_tag {
                return false;
            }
        }

        true
    }
}

/// Simple glob matcher (supports * and **)
fn glob_match(pattern: &str, path: &str) -> bool {
    // Convert glob pattern to regex
    let mut regex_str = String::new();
    regex_str.push('^');

    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                    regex_str.push_str(".*"); // ** matches anything including /
                } else {
                    regex_str.push_str("[^/]*"); // * matches anything except /
                }
            }
            '?' => regex_str.push('.'),
            '.' => regex_str.push_str("\\."),
            c if c.is_alphanumeric() || c == '/' || c == '_' || c == '-' => regex_str.push(c),
            _ => {
                regex_str.push('\\');
                regex_str.push(c);
            }
        }
    }

    regex_str.push('$');

    // Use simple string matching if it's not a pattern
    if !pattern.contains('*') && !pattern.contains('?') {
        return path.contains(pattern);
    }

    // Otherwise try regex (fallback to substring match on error)
    regex::Regex::new(&regex_str)
        .map(|re| re.is_match(path))
        .unwrap_or_else(|_| path.contains(pattern))
}

/// Query context for pattern/failure lookup
#[derive(Debug, Clone)]
pub struct QueryContext {
    pub description: String,
    pub current_file: Option<String>,
    pub relevant_symbols: Vec<String>,
    pub tags: Vec<String>,
}

impl QueryContext {
    pub fn from_task(task: &str, current_file: Option<&str>) -> Self {
        Self {
            description: task.to_string(),
            current_file: current_file.map(String::from),
            relevant_symbols: Vec::new(),
            tags: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_empty_matches_all() {
        let scope = Scope {
            include_paths: vec![],
            exclude_paths: vec![],
            symbols: vec![],
            tags: vec![],
        };

        assert!(scope.matches(Some("src/main.rs"), &[], &[]));
        assert!(scope.matches(None, &[], &[]));
    }

    #[test]
    fn test_scope_include_paths() {
        let scope = Scope {
            include_paths: vec!["src/**/*.rs".to_string()],
            exclude_paths: vec![],
            symbols: vec![],
            tags: vec![],
        };

        assert!(scope.matches(Some("src/store/db.rs"), &[], &[]));
        assert!(!scope.matches(Some("tests/integration.rs"), &[], &[]));
    }

    #[test]
    fn test_scope_exclude_paths() {
        let scope = Scope {
            include_paths: vec!["**/*.rs".to_string()],
            exclude_paths: vec!["**/tests/**".to_string()],
            symbols: vec![],
            tags: vec![],
        };

        assert!(scope.matches(Some("src/main.rs"), &[], &[]));
        assert!(!scope.matches(Some("src/tests/utils.rs"), &[], &[]));
    }

    #[test]
    fn test_scope_symbols() {
        let scope = Scope {
            include_paths: vec![],
            exclude_paths: vec![],
            symbols: vec!["Store".to_string()],
            tags: vec![],
        };

        assert!(scope.matches(None, &["Store::upsert_node".to_string()], &[]));
        assert!(!scope.matches(None, &["Parser::parse".to_string()], &[]));
    }

    #[test]
    fn test_scope_tags() {
        let scope = Scope {
            include_paths: vec![],
            exclude_paths: vec![],
            symbols: vec![],
            tags: vec!["async".to_string()],
        };

        assert!(scope.matches(None, &[], &["async".to_string(), "db".to_string()]));
        assert!(!scope.matches(None, &[], &["sync".to_string()]));
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("src/**/*.rs", "src/store/db.rs"));
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "src/main.rs"));
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(!glob_match("src/*.rs", "src/store/db.rs"));
    }
}
