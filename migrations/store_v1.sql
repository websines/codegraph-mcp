-- Store Database V1: Code Graph Schema
-- Stores nodes (symbols), edges (relationships), and file metadata

-- Nodes table: symbols, tasks, decisions, etc.
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,               -- Unique node ID (e.g., "file.rs::Class::method")
    graph TEXT NOT NULL,               -- Graph name ("code", "session", "cross")
    kind TEXT NOT NULL,                -- Node kind (function, class, task, decision, etc.)
    data TEXT NOT NULL,                -- JSON data (metadata, signatures, etc.)
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Edges table: relationships between nodes
CREATE TABLE IF NOT EXISTS edges (
    source TEXT NOT NULL,              -- Source node ID
    target TEXT NOT NULL,              -- Target node ID
    kind TEXT NOT NULL,                -- Edge kind (calls, imports, has_item, etc.)
    graph TEXT NOT NULL,               -- Graph name
    data TEXT,                         -- Optional JSON metadata
    created_at INTEGER DEFAULT (strftime('%s', 'now')),

    PRIMARY KEY (source, target, kind, graph),
    FOREIGN KEY (source) REFERENCES nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (target) REFERENCES nodes(id) ON DELETE CASCADE
);

-- File metadata table: tracks indexed files for incremental updates
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,             -- Relative file path
    mtime INTEGER NOT NULL,            -- Last modification time (unix timestamp)
    hash TEXT NOT NULL,                -- Content hash (xxh3)
    indexed_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Indexes for fast lookups
CREATE INDEX IF NOT EXISTS idx_nodes_graph_kind ON nodes(graph, kind);
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
CREATE INDEX IF NOT EXISTS idx_edges_graph_kind ON edges(graph, kind);
CREATE INDEX IF NOT EXISTS idx_files_mtime ON files(mtime);
