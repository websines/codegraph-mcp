use once_cell::sync::Lazy;
use std::collections::HashMap;
use tree_sitter::Language;

#[derive(Debug, Clone)]
pub struct LanguageConfig {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub tree_sitter_language: Language,
    pub queries: LanguageQueries,
}

#[derive(Debug, Clone)]
pub struct LanguageQueries {
    pub symbols: &'static str,
    pub references: &'static str,
}

// Language registry singleton
pub static LANGUAGE_REGISTRY: Lazy<HashMap<String, LanguageConfig>> = Lazy::new(|| {
    let mut registry = HashMap::new();

    // Rust
    registry.insert(
        "rust".to_string(),
        LanguageConfig {
            name: "rust",
            extensions: &["rs"],
            tree_sitter_language: tree_sitter_rust::LANGUAGE.into(),
            queries: LanguageQueries {
                symbols: include_str!("queries/rust-symbols.scm"),
                references: include_str!("queries/rust-references.scm"),
            },
        },
    );

    // TypeScript
    registry.insert(
        "typescript".to_string(),
        LanguageConfig {
            name: "typescript",
            extensions: &["ts", "tsx"],
            tree_sitter_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            queries: LanguageQueries {
                symbols: include_str!("queries/typescript-symbols.scm"),
                references: include_str!("queries/typescript-references.scm"),
            },
        },
    );

    // JavaScript
    registry.insert(
        "javascript".to_string(),
        LanguageConfig {
            name: "javascript",
            extensions: &["js", "jsx", "mjs"],
            tree_sitter_language: tree_sitter_javascript::LANGUAGE.into(),
            queries: LanguageQueries {
                symbols: include_str!("queries/javascript-symbols.scm"),
                references: include_str!("queries/javascript-references.scm"),
            },
        },
    );

    // Python
    registry.insert(
        "python".to_string(),
        LanguageConfig {
            name: "python",
            extensions: &["py"],
            tree_sitter_language: tree_sitter_python::LANGUAGE.into(),
            queries: LanguageQueries {
                symbols: include_str!("queries/python-symbols.scm"),
                references: include_str!("queries/python-references.scm"),
            },
        },
    );

    // Go
    registry.insert(
        "go".to_string(),
        LanguageConfig {
            name: "go",
            extensions: &["go"],
            tree_sitter_language: tree_sitter_go::LANGUAGE.into(),
            queries: LanguageQueries {
                symbols: include_str!("queries/go-symbols.scm"),
                references: include_str!("queries/go-references.scm"),
            },
        },
    );

    registry
});

/// Detect language from file extension
pub fn detect_language(path: &str) -> Option<&'static LanguageConfig> {
    let extension = std::path::Path::new(path)
        .extension()?
        .to_str()?;

    LANGUAGE_REGISTRY
        .values()
        .find(|config| config.extensions.contains(&extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("test.rs").unwrap().name, "rust");
        assert_eq!(detect_language("test.ts").unwrap().name, "typescript");
        assert_eq!(detect_language("test.js").unwrap().name, "javascript");
        assert_eq!(detect_language("test.py").unwrap().name, "python");
        assert_eq!(detect_language("test.go").unwrap().name, "go");
        assert!(detect_language("test.txt").is_none());
    }

    #[test]
    fn test_language_registry() {
        assert_eq!(LANGUAGE_REGISTRY.len(), 5);
        assert!(LANGUAGE_REGISTRY.contains_key("rust"));
        assert!(LANGUAGE_REGISTRY.contains_key("typescript"));
        assert!(LANGUAGE_REGISTRY.contains_key("javascript"));
        assert!(LANGUAGE_REGISTRY.contains_key("python"));
        assert!(LANGUAGE_REGISTRY.contains_key("go"));
    }
}
