use anyhow::Result;
use libsql::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Niche {
    pub id: String,
    pub task_type: String,
    pub feature_description: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NicheWithBest {
    pub niche: Niche,
    pub best_solution: Option<BestSolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestSolution {
    pub solution_id: String,
    pub score: f32,
    pub feature_vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    pub performance: f32,      // 0.0 (slow) to 1.0 (fast)
    pub readability: f32,      // 0.0 (complex) to 1.0 (simple)
    pub maintainability: f32,  // 0.0 (brittle) to 1.0 (robust)
}

impl FeatureVector {
    pub fn to_vec(&self) -> Vec<f32> {
        vec![self.performance, self.readability, self.maintainability]
    }

    pub fn from_vec(vec: &[f32]) -> Self {
        Self {
            performance: vec.get(0).copied().unwrap_or(0.5),
            readability: vec.get(1).copied().unwrap_or(0.5),
            maintainability: vec.get(2).copied().unwrap_or(0.5),
        }
    }
}

pub struct NicheStore {
    db: Arc<Connection>,
}

impl NicheStore {
    pub fn new(db: Arc<Connection>) -> Self {
        Self { db }
    }

    /// List all niches, optionally filtered by task type
    pub async fn list_niches(&self, task_type: Option<&str>) -> Result<Vec<NicheWithBest>> {
        let query = if let Some(tt) = task_type {
            "SELECT id, task_type, feature_description, created_at FROM niches WHERE task_type = ?1"
        } else {
            "SELECT id, task_type, feature_description, created_at FROM niches"
        };

        let mut rows = if let Some(tt) = task_type {
            self.db.query(query, [tt]).await?
        } else {
            self.db.query(query, ()).await?
        };

        let mut niches_with_best = Vec::new();

        while let Some(row) = rows.next().await? {
            let niche = Niche {
                id: row.get(0)?,
                task_type: row.get(1)?,
                feature_description: row.get(2)?,
                created_at: row.get(3)?,
            };

            // Get best solution for this niche
            let best_solution = self.get_best_solution(&niche.id).await?;

            niches_with_best.push(NicheWithBest {
                niche,
                best_solution,
            });
        }

        Ok(niches_with_best)
    }

    /// Get the best solution for a niche
    async fn get_best_solution(&self, niche_id: &str) -> Result<Option<BestSolution>> {
        let mut rows = self
            .db
            .query(
                "SELECT solution_id, score, feature_vector FROM niche_solutions WHERE niche_id = ?1 ORDER BY score DESC LIMIT 1",
                [niche_id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let feature_vec_json: String = row.get(2)?;
            let feature_vec: Vec<f32> = serde_json::from_str(&feature_vec_json)?;

            Ok(Some(BestSolution {
                solution_id: row.get(0)?,
                score: row.get::<f64>(1)? as f32,
                feature_vector: feature_vec,
            }))
        } else {
            Ok(None)
        }
    }

    /// Assign a solution to a niche based on feature vector
    pub async fn assign_to_niche(
        &self,
        solution_id: &str,
        feature_vector: &FeatureVector,
        score: f32,
    ) -> Result<String> {
        // Find the closest niche based on feature vector
        let niche_id = self.find_closest_niche(feature_vector).await?;

        let feature_vec_json = serde_json::to_string(&feature_vector.to_vec())?;
        let now = chrono::Utc::now().timestamp();

        // Insert or update the solution in this niche
        self.db
            .execute(
                "INSERT INTO niche_solutions (niche_id, solution_id, score, feature_vector, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(niche_id, solution_id) DO UPDATE SET
                     score = ?3,
                     feature_vector = ?4,
                     updated_at = ?5",
                libsql::params![
                    niche_id.as_str(),
                    solution_id,
                    score as f64,
                    feature_vec_json.as_str(),
                    now
                ],
            )
            .await?;

        Ok(niche_id)
    }

    /// Find the closest niche to a feature vector
    async fn find_closest_niche(&self, feature_vector: &FeatureVector) -> Result<String> {
        // For now, use a simple heuristic based on the dominant feature
        let vec = feature_vector.to_vec();
        let max_idx = vec
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        // Map to predefined niches
        let niche_id = match max_idx {
            0 => "high-performance",
            1 => "high-readability",
            2 => "high-maintainability",
            _ => "balanced",
        };

        // Ensure niche exists
        self.ensure_niche_exists(niche_id).await?;

        Ok(niche_id.to_string())
    }

    /// Ensure a niche exists in the database
    async fn ensure_niche_exists(&self, niche_id: &str) -> Result<()> {
        let (task_type, description) = match niche_id {
            "high-performance" => ("general", "Optimized for speed and efficiency"),
            "high-readability" => ("general", "Optimized for clarity and simplicity"),
            "high-maintainability" => ("general", "Optimized for robustness and maintainability"),
            _ => ("general", "Balanced approach"),
        };

        let now = chrono::Utc::now().timestamp();

        self.db
            .execute(
                "INSERT OR IGNORE INTO niches (id, task_type, feature_description, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                libsql::params![niche_id, task_type, description, now],
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::store::Store;
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

    /// Insert a dummy solution so FK constraints are satisfied
    async fn insert_dummy_solution(store: &Store, id: &str) {
        let now = chrono::Utc::now().timestamp();
        store
            .learning_db
            .execute(
                "INSERT INTO solutions (id, task, plan, outcome, created_at)
                 VALUES (?1, 'test task', 'test plan', 'success', ?2)",
                libsql::params![id, now],
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_assign_to_niche() {
        let (store, _temp) = setup_test_store().await;
        insert_dummy_solution(&store, "solution1").await;
        let niche_store = NicheStore::new(Arc::new(store.learning_db));

        let feature_vec = FeatureVector {
            performance: 0.9,
            readability: 0.6,
            maintainability: 0.7,
        };

        let niche_id = niche_store
            .assign_to_niche("solution1", &feature_vec, 0.85)
            .await
            .unwrap();

        assert_eq!(niche_id, "high-performance");
    }

    #[tokio::test]
    async fn test_list_niches() {
        let (store, _temp) = setup_test_store().await;
        insert_dummy_solution(&store, "solution1").await;
        let niche_store = NicheStore::new(Arc::new(store.learning_db));

        // Assign a solution to create a niche
        let feature_vec = FeatureVector {
            performance: 0.5,
            readability: 0.9,
            maintainability: 0.6,
        };

        niche_store
            .assign_to_niche("solution1", &feature_vec, 0.8)
            .await
            .unwrap();

        let niches = niche_store.list_niches(None).await.unwrap();
        assert!(!niches.is_empty());
        assert!(niches.iter().any(|n| n.niche.id == "high-readability"));
    }
}
