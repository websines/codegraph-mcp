use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::store::Store;

#[derive(Debug, Clone)]
pub struct CrossLanguageRule {
    pub rule_type: String,
    pub client_glob: String,
    pub server_glob: String,
    pub client_pattern: Regex,
    pub server_pattern: Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConnection {
    pub client_file: String,
    pub server_file: String,
    pub api_path: String,
    pub method: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStats {
    pub client_calls_found: usize,
    pub server_routes_found: usize,
    pub connections_made: usize,
    pub duration_ms: u64,
}

pub struct CrossLanguageInferrer {
    store: Arc<Store>,
    rules: Vec<CrossLanguageRule>,
}

impl CrossLanguageInferrer {
    pub fn new(store: Arc<Store>) -> Self {
        let rules = Self::default_rules();
        Self { store, rules }
    }

    /// Default inference rules for common patterns
    fn default_rules() -> Vec<CrossLanguageRule> {
        vec![
            // REST API: fetch/axios calls
            CrossLanguageRule {
                rule_type: "rest_fetch".to_string(),
                client_glob: "**/*.{js,ts,jsx,tsx}".to_string(),
                server_glob: "**/*.{py,rs,js,ts,go}".to_string(),
                client_pattern: Regex::new(
                    r#"(?:fetch|axios\.(?:get|post|put|delete|patch))\s*\(\s*['"`]([/\w\-{}:]+)['"`]"#,
                )
                .unwrap(),
                server_pattern: Regex::new(
                    r#"(?:@app\.route|@router\.|router\.(?:get|post|put|delete|patch)|app\.(?:get|post|put|delete|patch))\s*\(\s*['"`]([/\w\-{}:]+)['"`]"#,
                )
                .unwrap(),
            },
            // GraphQL
            CrossLanguageRule {
                rule_type: "graphql".to_string(),
                client_glob: "**/*.{js,ts,jsx,tsx,gql,graphql}".to_string(),
                server_glob: "**/*.{py,rs,js,ts,go}".to_string(),
                client_pattern: Regex::new(r#"(?:query|mutation)\s+(\w+)"#).unwrap(),
                server_pattern: Regex::new(r#"def\s+(?:resolve_)?(\w+)"#).unwrap(),
            },
        ]
    }

    /// Run cross-language inference
    pub async fn infer(&self, force_rebuild: bool) -> Result<InferenceStats> {
        let start = std::time::Instant::now();

        // Clear existing edges if force rebuild
        if force_rebuild {
            self.clear_cross_language_edges().await?;
        }

        let mut stats = InferenceStats {
            client_calls_found: 0,
            server_routes_found: 0,
            connections_made: 0,
            duration_ms: 0,
        };

        // Get all files from the database
        let files = self.store.list_indexed_files().await?;

        for rule in &self.rules {
            // Find client calls
            let mut client_calls: HashMap<String, Vec<String>> = HashMap::new();
            for file in &files {
                if matches_glob(file, &rule.client_glob) {
                    if let Ok(content) = std::fs::read_to_string(file) {
                        for cap in rule.client_pattern.captures_iter(&content) {
                            if let Some(path) = cap.get(1) {
                                let normalized = normalize_path(path.as_str());
                                client_calls
                                    .entry(normalized)
                                    .or_default()
                                    .push(file.clone());
                            }
                        }
                    }
                }
            }

            stats.client_calls_found += client_calls.len();

            // Find server routes
            let mut server_routes: HashMap<String, Vec<String>> = HashMap::new();
            for file in &files {
                if matches_glob(file, &rule.server_glob) {
                    if let Ok(content) = std::fs::read_to_string(file) {
                        for cap in rule.server_pattern.captures_iter(&content) {
                            if let Some(path) = cap.get(1) {
                                let normalized = normalize_path(path.as_str());
                                server_routes
                                    .entry(normalized)
                                    .or_default()
                                    .push(file.clone());
                            }
                        }
                    }
                }
            }

            stats.server_routes_found += server_routes.len();

            // Match client calls to server routes
            for (api_path, client_files) in &client_calls {
                if let Some(server_files) = server_routes.get(api_path) {
                    for client_file in client_files {
                        for server_file in server_files {
                            self.record_connection(
                                client_file,
                                server_file,
                                api_path,
                                None,
                                0.8, // Base confidence
                            )
                            .await?;
                            stats.connections_made += 1;
                        }
                    }
                }
            }
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;

        Ok(stats)
    }

    /// Get API connections for a given path
    pub async fn get_api_connections(&self, path: &str) -> Result<Vec<ApiConnection>> {
        let mut rows = self
            .store
            .code_db
            .query(
                "SELECT client_file, server_file, api_path, method, confidence
                 FROM cross_language_edges
                 WHERE client_file = ?1 OR server_file = ?1 OR api_path LIKE ?2
                 ORDER BY confidence DESC",
                libsql::params![path, format!("%{}%", path)],
            )
            .await?;

        let mut connections = Vec::new();

        while let Some(row) = rows.next().await? {
            let method: Option<String> = row.get(3).ok();
            connections.push(ApiConnection {
                client_file: row.get(0)?,
                server_file: row.get(1)?,
                api_path: row.get(2)?,
                method,
                confidence: row.get::<f64>(4)? as f32,
            });
        }

        Ok(connections)
    }

    async fn record_connection(
        &self,
        client_file: &str,
        server_file: &str,
        api_path: &str,
        method: Option<&str>,
        confidence: f32,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        self.store
            .code_db
            .execute(
                "INSERT INTO cross_language_edges (client_file, server_file, api_path, method, confidence, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(client_file, server_file, api_path) DO UPDATE SET
                     method = ?4,
                     confidence = ?5",
                libsql::params![
                    client_file,
                    server_file,
                    api_path,
                    method.unwrap_or(""),
                    confidence as f64,
                    now
                ],
            )
            .await?;

        Ok(())
    }

    async fn clear_cross_language_edges(&self) -> Result<()> {
        self.store
            .code_db
            .execute("DELETE FROM cross_language_edges", ())
            .await?;
        Ok(())
    }
}

/// Normalize an API path for matching
fn normalize_path(path: &str) -> String {
    // Replace param placeholders first (before lowercasing)
    path.trim_start_matches('/')
        .replace("${", "{")
        .replace(":id", "{id}")
        .replace(":userId", "{userId}")
        .to_lowercase()
        // Add more normalizations as needed
}

/// Simple glob matching
fn matches_glob(path: &str, pattern: &str) -> bool {
    // Handle common patterns
    if pattern.contains("**/*.") {
        // Match file extension with brace expansion: **/*.{js,ts}
        if let (Some(start), Some(end)) = (pattern.find('{'), pattern.rfind('}')) {
            let exts_str = &pattern[start + 1..end];
            let exts: Vec<&str> = exts_str.split(',').collect();
            return exts
                .iter()
                .any(|e| path.ends_with(&format!(".{}", e.trim())));
        }
        // Simple extension: **/*.rs
        let ext = pattern.split('.').last().unwrap_or("");
        path.ends_with(&format!(".{}", ext))
    } else if pattern.contains('*') {
        // Basic wildcard matching
        path.contains(pattern.trim_matches('*'))
    } else {
        path.contains(pattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/api/users/{id}"), "api/users/{id}");
        assert_eq!(normalize_path("/users/:id"), "users/{id}");
        assert_eq!(normalize_path("/Users/${userId}"), "users/{userid}");
    }

    #[test]
    fn test_matches_glob() {
        assert!(matches_glob("src/api/users.ts", "**/*.{js,ts}"));
        assert!(matches_glob("src/main.js", "**/*.{js,ts}"));
        assert!(!matches_glob("src/main.rs", "**/*.{js,ts}"));
    }

    #[test]
    fn test_client_pattern() {
        let rule = CrossLanguageInferrer::default_rules()[0].clone();

        let content = r#"
            fetch('/api/users')
            axios.get('/api/posts')
            axios.post("/api/comments")
        "#;

        let mut paths = Vec::new();
        for cap in rule.client_pattern.captures_iter(content) {
            if let Some(path) = cap.get(1) {
                paths.push(path.as_str());
            }
        }

        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&"/api/users"));
        assert!(paths.contains(&"/api/posts"));
        assert!(paths.contains(&"/api/comments"));
    }
}
