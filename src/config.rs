use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;
use xxhash_rust::xxh3::xxh3_64;

/// Runtime-resolved paths and settings
#[derive(Debug, Clone)]
pub struct Config {
    /// Detected project root directory
    pub project_root: PathBuf,
    /// Cache directory: ~/.cache/codegraph/{project_hash}/
    pub cache_dir: PathBuf,
    /// Project-local directory: {project_root}/.codegraph/
    pub codegraph_dir: PathBuf,
    /// Code graph database: {cache_dir}/store.db
    pub store_db_path: PathBuf,
    /// Learning database: {codegraph_dir}/learning.db
    pub learning_db_path: PathBuf,
    /// Parsed config file settings
    pub settings: ConfigFile,
}

/// Parsed from .codegraph/config.toml (all fields have defaults)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub indexing: IndexingConfig,
    pub learning: LearningConfig,
    pub cross_language: CrossLanguageConfig,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            indexing: IndexingConfig::default(),
            learning: LearningConfig::default(),
            cross_language: CrossLanguageConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexingConfig {
    pub exclude: Vec<String>,
    pub max_file_size: usize,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            exclude: vec![
                "node_modules".into(),
                "target".into(),
                ".git".into(),
                "dist".into(),
                "build".into(),
                "__pycache__".into(),
                ".cache".into(),
                ".pytest_cache".into(),
                "coverage".into(),
                ".codegraph".into(),
                ".venv".into(),
                "venv".into(),
                ".tox".into(),
            ],
            max_file_size: 1_048_576, // 1 MiB
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LearningConfig {
    pub decay_half_life: u32,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            decay_half_life: 90,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CrossLanguageConfig {
    pub enabled: bool,
}

impl Default for CrossLanguageConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

const DEFAULT_CONFIG_TOML: &str = r#"# Codegraph configuration
# See https://github.com/anthropics/codegraph-mcp for documentation

[indexing]
# Directories to exclude from indexing (matched as exact path components)
exclude = [
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    "__pycache__",
    ".cache",
    ".pytest_cache",
    "coverage",
    ".codegraph",
    ".venv",
    "venv",
    ".tox",
]

# Maximum file size in bytes (files larger than this are skipped)
max_file_size = 1048576  # 1 MiB

[learning]
# Half-life for confidence decay in days
decay_half_life = 90

[cross_language]
# Enable cross-language API inference
enabled = true
"#;

const CODEGRAPH_GITIGNORE: &str = r#"# Codegraph - SQLite databases (user-local, not shared)
learning.db
learning.db-*
"#;

impl Config {
    /// Detect project configuration from current directory or provided path
    pub fn detect() -> Result<Self> {
        Self::from_path(&std::env::current_dir()?)
    }

    /// Create configuration from a specific path
    pub fn from_path(start_path: &Path) -> Result<Self> {
        let project_root = Self::find_project_root(start_path)?;
        let project_hash = Self::hash_project_path(&project_root);

        // Cache directory in user's cache dir
        let cache_base = dirs::cache_dir()
            .context("Failed to determine cache directory")?
            .join("codegraph");
        let cache_dir = cache_base.join(&project_hash);

        // Project-local directory
        let codegraph_dir = project_root.join(".codegraph");

        // Database paths
        let store_db_path = cache_dir.join("store.db");
        let learning_db_path = codegraph_dir.join("learning.db");

        // Load config file
        let settings = Self::load_config_file(&codegraph_dir);

        Ok(Self {
            project_root,
            cache_dir,
            codegraph_dir,
            store_db_path,
            learning_db_path,
            settings,
        })
    }

    /// Load config.toml from .codegraph/ directory, falling back to defaults
    fn load_config_file(codegraph_dir: &Path) -> ConfigFile {
        let config_path = codegraph_dir.join("config.toml");
        if config_path.is_file() {
            match std::fs::read_to_string(&config_path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("Warning: Failed to parse config.toml: {}", e);
                    }
                },
                Err(e) => {
                    eprintln!("Warning: Failed to read config.toml: {}", e);
                }
            }
        }
        ConfigFile::default()
    }

    /// Find project root by walking up from start path
    ///
    /// Looks for:
    /// 1. .codegraph directory
    /// 2. .git directory
    /// 3. Falls back to start path if nothing found
    fn find_project_root(start_path: &Path) -> Result<PathBuf> {
        let mut current = start_path.canonicalize()?;

        loop {
            // Check for .codegraph marker
            if current.join(".codegraph").is_dir() {
                return Ok(current);
            }

            // Check for .git marker
            if current.join(".git").is_dir() {
                return Ok(current);
            }

            // Move up one directory
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => {
                    // Reached filesystem root, use start path
                    return Ok(start_path.canonicalize()?);
                }
            }
        }
    }

    /// Hash project path to create a unique cache key
    fn hash_project_path(path: &Path) -> String {
        let canonical = path.to_string_lossy();
        let hash = xxh3_64(canonical.as_bytes());
        format!("{:016x}", hash)
    }

    /// Ensure all necessary directories exist; initialize .codegraph/ on first run
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.cache_dir)
            .context("Failed to create cache directory")?;

        let is_first_run = !self.codegraph_dir.exists();

        std::fs::create_dir_all(&self.codegraph_dir)
            .context("Failed to create .codegraph directory")?;

        if is_first_run {
            self.init_codegraph_dir()?;
        }

        Ok(())
    }

    /// Initialize .codegraph/ directory with default files
    fn init_codegraph_dir(&self) -> Result<()> {
        // Write default config.toml
        let config_path = self.codegraph_dir.join("config.toml");
        if !config_path.exists() {
            std::fs::write(&config_path, DEFAULT_CONFIG_TOML)
                .context("Failed to write default config.toml")?;
            info!("Created .codegraph/config.toml with defaults");
        }

        // Write .gitignore
        let gitignore_path = self.codegraph_dir.join(".gitignore");
        if !gitignore_path.exists() {
            std::fs::write(&gitignore_path, CODEGRAPH_GITIGNORE)
                .context("Failed to write .codegraph/.gitignore")?;
            info!("Created .codegraph/.gitignore");
        }

        eprintln!(
            "Initialized .codegraph/ directory. Consider adding config.toml to version control."
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_hash_deterministic() {
        let path1 = PathBuf::from("/some/test/path");
        let path2 = PathBuf::from("/some/test/path");

        let hash1 = Config::hash_project_path(&path1);
        let hash2 = Config::hash_project_path(&path2);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_different_paths() {
        let path1 = PathBuf::from("/path/one");
        let path2 = PathBuf::from("/path/two");

        let hash1 = Config::hash_project_path(&path1);
        let hash2 = Config::hash_project_path(&path2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_find_project_root_with_git() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();
        let sub_dir = project_root.join("src/deep/nested");

        fs::create_dir_all(&sub_dir).unwrap();
        fs::create_dir_all(project_root.join(".git")).unwrap();

        let found = Config::find_project_root(&sub_dir).unwrap();

        assert_eq!(found, project_root.canonicalize().unwrap());
    }

    #[test]
    fn test_find_project_root_with_codegraph() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();
        let sub_dir = project_root.join("src");

        fs::create_dir_all(&sub_dir).unwrap();
        fs::create_dir_all(project_root.join(".codegraph")).unwrap();

        let found = Config::find_project_root(&sub_dir).unwrap();

        assert_eq!(found, project_root.canonicalize().unwrap());
    }

    #[test]
    fn test_config_file_defaults() {
        let config = ConfigFile::default();
        assert!(config.indexing.exclude.contains(&"node_modules".to_string()));
        assert_eq!(config.indexing.max_file_size, 1_048_576);
        assert_eq!(config.learning.decay_half_life, 90);
        assert!(config.cross_language.enabled);
    }

    #[test]
    fn test_config_file_parse() {
        let toml_str = r#"
[indexing]
exclude = ["vendor", "build"]
max_file_size = 500000

[learning]
decay_half_life = 30
"#;
        let config: ConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(config.indexing.exclude, vec!["vendor", "build"]);
        assert_eq!(config.indexing.max_file_size, 500_000);
        assert_eq!(config.learning.decay_half_life, 30);
        // cross_language should use default
        assert!(config.cross_language.enabled);
    }

    #[test]
    fn test_init_codegraph_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        fs::create_dir_all(project_root.join(".git")).unwrap();

        let config = Config::from_path(project_root).unwrap();
        config.ensure_dirs().unwrap();

        // Verify files created
        assert!(config.codegraph_dir.join("config.toml").exists());
        assert!(config.codegraph_dir.join(".gitignore").exists());

        // Verify contents
        let gitignore = fs::read_to_string(config.codegraph_dir.join(".gitignore")).unwrap();
        assert!(gitignore.contains("learning.db"));
    }
}
