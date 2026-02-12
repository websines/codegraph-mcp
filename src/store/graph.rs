use anyhow::Result;
use petgraph::graph::{DiGraph, NodeIndex};
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;

use super::db::Store;

#[derive(Debug, Clone)]
pub struct NodeData {
    pub id: String,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct EdgeData {
    pub kind: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct NeighborResult {
    pub node: NodeData,
    pub path: Vec<String>, // Edge kinds in the path
    pub distance: u32,
}

pub struct CodeGraph {
    pub graph: DiGraph<NodeData, EdgeData>,
    id_to_index: HashMap<String, NodeIndex>,
    index_to_id: HashMap<NodeIndex, String>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            id_to_index: HashMap::new(),
            index_to_id: HashMap::new(),
        }
    }

    /// Load graph from store (code graph only)
    pub async fn load_from_store(store: &Store) -> Result<Self> {
        debug!("Loading code graph from store");

        let mut code_graph = Self::new();

        // Load all code nodes
        let nodes_query = "SELECT id, kind, data FROM nodes WHERE graph = 'code'";
        let mut rows = store.code_db.query(nodes_query, ()).await?;

        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let data_str: String = row.get(2)?;
            let data: Value = serde_json::from_str(&data_str)?;

            code_graph.add_node(id, kind, data);
        }

        // Load all code edges
        let edges_query = "SELECT source, target, kind, data FROM edges WHERE graph = 'code'";
        let mut rows = store.code_db.query(edges_query, ()).await?;

        while let Some(row) = rows.next().await? {
            let source: String = row.get(0)?;
            let target: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let data_str: Option<String> = row.get(3).ok();
            let data = data_str.and_then(|s| serde_json::from_str(&s).ok());

            code_graph.add_edge(&source, &target, kind, data);
        }

        debug!(
            "Loaded code graph: {} nodes, {} edges",
            code_graph.graph.node_count(),
            code_graph.graph.edge_count()
        );

        Ok(code_graph)
    }

    pub fn add_node(&mut self, id: String, kind: String, data: Value) {
        let node_data = NodeData {
            id: id.clone(),
            kind,
            data,
        };

        let index = self.graph.add_node(node_data);
        self.id_to_index.insert(id.clone(), index);
        self.index_to_id.insert(index, id);
    }

    pub fn add_edge(&mut self, source: &str, target: &str, kind: String, data: Option<Value>) {
        if let (Some(&source_idx), Some(&target_idx)) = (
            self.id_to_index.get(source),
            self.id_to_index.get(target),
        ) {
            self.graph
                .add_edge(source_idx, target_idx, EdgeData { kind, data });
        }
    }

    /// Search for symbols by name, kind, or file pattern
    pub fn search(
        &self,
        query: &str,
        kind: Option<&str>,
        file_pattern: Option<&str>,
        limit: usize,
    ) -> Vec<&NodeData> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<(&NodeData, i32)> = Vec::new();

        for node in self.graph.node_weights() {
            // Filter by kind if specified
            if let Some(k) = kind {
                if node.kind != k {
                    continue;
                }
            }

            // Filter by file pattern if specified
            if let Some(pattern) = file_pattern {
                if let Some(file) = node.data.get("file").and_then(|v| v.as_str()) {
                    if !file.contains(pattern) {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Get symbol name from data
            let name = match node.data.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => continue,
            };

            let name_lower = name.to_lowercase();

            // Calculate relevance score
            let score = if name_lower == query_lower {
                100 // Exact match
            } else if name_lower.starts_with(&query_lower) {
                50 // Prefix match
            } else if name_lower.contains(&query_lower) {
                25 // Contains match
            } else {
                continue; // No match
            };

            results.push((node, score));
        }

        // Sort by score (descending) and take limit
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results
            .into_iter()
            .take(limit)
            .map(|(node, _)| node)
            .collect()
    }

    /// Get symbols in a specific file
    pub fn file_symbols(&self, path: &str) -> Vec<&NodeData> {
        self.graph
            .node_weights()
            .filter(|node| {
                node.data
                    .get("file")
                    .and_then(|v| v.as_str())
                    .map(|f| f == path)
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Get neighbors of a node up to a certain depth
    pub fn neighbors(
        &self,
        id: &str,
        depth: u32,
        direction: Direction,
        edge_filter: Option<&[&str]>,
    ) -> Vec<NeighborResult> {
        let start_idx = match self.id_to_index.get(id) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        let mut results = Vec::new();
        let mut visited = HashMap::new();
        let mut queue = vec![(start_idx, Vec::new(), 0)];

        while let Some((idx, path, dist)) = queue.pop() {
            if dist > depth {
                continue;
            }

            if visited.contains_key(&idx) {
                continue;
            }

            visited.insert(idx, true);

            // Add to results (except the start node)
            if idx != start_idx {
                if let Some(_node_id) = self.index_to_id.get(&idx) {
                    if let Some(node) = self.graph.node_weight(idx) {
                        results.push(NeighborResult {
                            node: node.clone(),
                            path: path.clone(),
                            distance: dist,
                        });
                    }
                }
            }

            // Get neighbors based on direction
            let neighbors: Vec<_> = match direction {
                Direction::Outgoing => self.graph.neighbors(idx).collect(),
                Direction::Incoming => self.graph.neighbors_directed(
                    idx,
                    petgraph::Direction::Incoming,
                )
                .collect(),
                Direction::Both => {
                    let mut n = self.graph.neighbors(idx).collect::<Vec<_>>();
                    n.extend(
                        self.graph
                            .neighbors_directed(idx, petgraph::Direction::Incoming),
                    );
                    n
                }
            };

            for neighbor_idx in neighbors {
                // Get edge data (try both directions since we may be traversing incoming edges)
                let edge_idx = self.graph.find_edge(idx, neighbor_idx)
                    .or_else(|| self.graph.find_edge(neighbor_idx, idx));
                if let Some(edge_idx) = edge_idx {
                    if let Some(edge) = self.graph.edge_weight(edge_idx) {
                        // Apply edge filter
                        if let Some(filter) = edge_filter {
                            if !filter.contains(&edge.kind.as_str()) {
                                continue;
                            }
                        }

                        let mut new_path = path.clone();
                        new_path.push(edge.kind.clone());
                        queue.push((neighbor_idx, new_path, dist + 1));
                    }
                }
            }
        }

        results
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Option<&NodeData> {
        self.id_to_index
            .get(id)
            .and_then(|&idx| self.graph.node_weight(idx))
    }

    /// Rebuild graph from store
    pub async fn rebuild_from_store(&mut self, store: &Store) -> Result<()> {
        *self = Self::load_from_store(store).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_node() {
        let mut graph = CodeGraph::new();
        graph.add_node(
            "test::func".to_string(),
            "function".to_string(),
            serde_json::json!({"name": "func"}),
        );

        assert_eq!(graph.graph.node_count(), 1);
        assert!(graph.get_node("test::func").is_some());
    }

    #[test]
    fn test_add_edge() {
        let mut graph = CodeGraph::new();
        graph.add_node(
            "a".to_string(),
            "function".to_string(),
            serde_json::json!({"name": "a"}),
        );
        graph.add_node(
            "b".to_string(),
            "function".to_string(),
            serde_json::json!({"name": "b"}),
        );
        graph.add_edge("a", "b", "calls".to_string(), None);

        assert_eq!(graph.graph.edge_count(), 1);
    }

    #[test]
    fn test_search() {
        let mut graph = CodeGraph::new();
        graph.add_node(
            "test::hello_world".to_string(),
            "function".to_string(),
            serde_json::json!({"name": "hello_world"}),
        );
        graph.add_node(
            "test::goodbye".to_string(),
            "function".to_string(),
            serde_json::json!({"name": "goodbye"}),
        );

        let results = graph.search("hello", None, None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "test::hello_world");
    }
}
