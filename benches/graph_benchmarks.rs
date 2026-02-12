use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::sync::{Arc, RwLock};

use codegraph::config::Config;
use codegraph::session::SessionManager;
use codegraph::store::{CodeGraph, Direction, Store};

fn build_large_graph(node_count: usize) -> CodeGraph {
    let mut graph = CodeGraph::new();

    // Create file nodes
    for file_idx in 0..10 {
        let file_id = format!("file_{}.rs", file_idx);
        graph.add_node(
            file_id.clone(),
            "module".to_string(),
            json!({
                "name": format!("file_{}", file_idx),
                "file": file_id,
                "line_start": 1,
                "line_end": 100,
                "signature": "",
            }),
        );
    }

    // Create function nodes
    for i in 0..node_count {
        let file_idx = i % 10;
        let node_id = format!("file_{}.rs::func_{}", file_idx, i);
        graph.add_node(
            node_id,
            "function".to_string(),
            json!({
                "name": format!("func_{}", i),
                "file": format!("file_{}.rs", file_idx),
                "line_start": (i % 100) * 10,
                "line_end": (i % 100) * 10 + 9,
                "signature": format!("fn func_{}()", i),
            }),
        );
    }

    // Create edges (calls between functions)
    for i in 0..node_count {
        let file_idx = i % 10;
        let source = format!("file_{}.rs::func_{}", file_idx, i);

        // Each function calls 3 others
        for j in 1..=3 {
            let target_idx = (i + j * 7) % node_count;
            let target_file = target_idx % 10;
            let target = format!("file_{}.rs::func_{}", target_file, target_idx);

            graph.add_edge(&source, &target, "calls".to_string(), None);
        }
    }

    graph
}

fn bench_search_symbols(c: &mut Criterion) {
    let graph = build_large_graph(10_000);

    c.bench_function("search_symbols_10k", |b| {
        b.iter(|| {
            let results = graph.search("func_42", None, None, 10);
            assert!(!results.is_empty());
        })
    });

    c.bench_function("search_symbols_by_kind_10k", |b| {
        b.iter(|| {
            let results = graph.search("func", Some("function"), None, 10);
            assert!(!results.is_empty());
        })
    });

    c.bench_function("search_symbols_by_file_10k", |b| {
        b.iter(|| {
            let results = graph.search("func", None, Some("file_3"), 10);
            assert!(!results.is_empty());
        })
    });
}

fn bench_get_neighbors(c: &mut Criterion) {
    let graph = build_large_graph(10_000);

    c.bench_function("neighbors_depth1_10k", |b| {
        b.iter(|| {
            let neighbors = graph.neighbors("file_0.rs::func_0", 1, Direction::Both, None);
            assert!(!neighbors.is_empty());
        })
    });

    c.bench_function("neighbors_depth2_10k", |b| {
        b.iter(|| {
            let neighbors = graph.neighbors("file_0.rs::func_0", 2, Direction::Both, None);
            assert!(!neighbors.is_empty());
        })
    });

    c.bench_function("neighbors_outgoing_10k", |b| {
        b.iter(|| {
            let neighbors =
                graph.neighbors("file_0.rs::func_0", 1, Direction::Outgoing, None);
            assert!(!neighbors.is_empty());
        })
    });
}

fn bench_file_symbols(c: &mut Criterion) {
    let graph = build_large_graph(10_000);

    c.bench_function("file_symbols_10k", |b| {
        b.iter(|| {
            let symbols = graph.file_symbols("file_0.rs");
            assert!(!symbols.is_empty());
        })
    });
}

fn bench_smart_context(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("smart_context_with_graph", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_dir = tempfile::TempDir::new().unwrap();
                let temp_path = temp_dir.path();

                let config = Config {
                    project_root: temp_path.to_path_buf(),
                    cache_dir: temp_path.join("cache"),
                    codegraph_dir: temp_path.join(".codegraph"),
                    store_db_path: temp_path.join("cache/store.db"),
                    learning_db_path: temp_path.join(".codegraph/learning.db"),
                    settings: codegraph::config::ConfigFile::default(),
                };
                config.ensure_dirs().unwrap();

                let store = Arc::new(Store::open(&config).await.unwrap());
                let graph = Arc::new(RwLock::new(build_large_graph(1000)));
                let manager = SessionManager::new(store, graph);

                manager
                    .start_session("Test task", &["Item 1".to_string()])
                    .await
                    .unwrap();

                let ctx = manager.smart_context().await.unwrap();
                assert_eq!(ctx.task, "Test task");
            })
        })
    });
}

criterion_group!(
    benches,
    bench_search_symbols,
    bench_get_neighbors,
    bench_file_symbols,
    bench_smart_context,
);
criterion_main!(benches);
