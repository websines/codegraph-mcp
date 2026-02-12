-- Learning Database V1: Patterns, Failures, Solutions, Niches
-- Stores the learning system data

-- Patterns table: successful solutions
CREATE TABLE IF NOT EXISTS patterns (
    id TEXT PRIMARY KEY,               -- UUID
    intent TEXT NOT NULL,              -- What the pattern accomplishes
    mechanism TEXT,                    -- How it works
    examples TEXT NOT NULL,            -- JSON array of code examples
    scope TEXT NOT NULL,               -- JSON Scope object
    confidence REAL NOT NULL,          -- Base confidence (0.0-1.0)
    usage_count INTEGER DEFAULT 0,    -- How many times applied
    success_count INTEGER DEFAULT 0,   -- How many times succeeded
    last_validated INTEGER,            -- Unix timestamp of last validation
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Failures table: things that went wrong
CREATE TABLE IF NOT EXISTS failures (
    id TEXT PRIMARY KEY,               -- UUID
    cause TEXT NOT NULL,               -- What went wrong
    avoidance_rule TEXT NOT NULL,      -- How to avoid it
    severity TEXT NOT NULL,            -- "critical", "major", "minor"
    scope TEXT NOT NULL,               -- JSON Scope object
    times_prevented INTEGER DEFAULT 0, -- How many times prevented
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Solutions table: solution lineage tracking
CREATE TABLE IF NOT EXISTS solutions (
    id TEXT PRIMARY KEY,               -- UUID
    task TEXT NOT NULL,                -- Task description
    plan TEXT NOT NULL,                -- Approach taken
    approach TEXT,                     -- Named approach (optional)
    outcome TEXT NOT NULL,             -- "success", "failure", "partial"
    metrics TEXT,                      -- JSON Metrics object
    files_modified TEXT,               -- JSON array of file paths
    symbols_modified TEXT,             -- JSON array of symbol IDs
    parent_id TEXT,                    -- Parent solution ID (if retry/iteration)
    created_at INTEGER DEFAULT (strftime('%s', 'now')),

    FOREIGN KEY (parent_id) REFERENCES solutions(id) ON DELETE SET NULL
);

-- Niches table: behavioral niches for diverse solutions
CREATE TABLE IF NOT EXISTS niches (
    id TEXT PRIMARY KEY,               -- Niche ID
    task_type TEXT NOT NULL,           -- Task category
    feature_description TEXT NOT NULL, -- Human-readable feature description
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Niche solutions: best solution per niche
CREATE TABLE IF NOT EXISTS niche_solutions (
    niche_id TEXT NOT NULL,
    solution_id TEXT NOT NULL,
    score REAL NOT NULL,               -- Solution score
    feature_vector TEXT NOT NULL,      -- JSON feature vector
    updated_at INTEGER DEFAULT (strftime('%s', 'now')),

    PRIMARY KEY (niche_id, solution_id),
    FOREIGN KEY (niche_id) REFERENCES niches(id) ON DELETE CASCADE,
    FOREIGN KEY (solution_id) REFERENCES solutions(id) ON DELETE CASCADE
);

-- Cross-language edges: inferred API connections
CREATE TABLE IF NOT EXISTS cross_language_edges (
    client_file TEXT NOT NULL,         -- Client file path
    server_file TEXT NOT NULL,         -- Server file path
    api_path TEXT NOT NULL,            -- API endpoint path
    method TEXT,                       -- HTTP method (GET, POST, etc.)
    confidence REAL NOT NULL,          -- Confidence score (0.0-1.0)
    created_at INTEGER DEFAULT (strftime('%s', 'now')),

    PRIMARY KEY (client_file, server_file, api_path)
);

-- Manual instructions (for skill distillation)
CREATE TABLE IF NOT EXISTS instructions (
    id TEXT PRIMARY KEY,               -- UUID
    instruction TEXT NOT NULL,         -- The instruction text
    category TEXT NOT NULL,            -- Category (Architecture, Testing, etc.)
    reason TEXT,                       -- Why this was added
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Indexes for fast queries
CREATE INDEX IF NOT EXISTS idx_patterns_scope ON patterns(scope);
CREATE INDEX IF NOT EXISTS idx_failures_severity ON failures(severity);
CREATE INDEX IF NOT EXISTS idx_failures_scope ON failures(scope);
CREATE INDEX IF NOT EXISTS idx_solutions_task ON solutions(task);
CREATE INDEX IF NOT EXISTS idx_solutions_outcome ON solutions(outcome);
CREATE INDEX IF NOT EXISTS idx_solutions_parent ON solutions(parent_id);
CREATE INDEX IF NOT EXISTS idx_niches_task_type ON niches(task_type);
CREATE INDEX IF NOT EXISTS idx_cross_language_api_path ON cross_language_edges(api_path);
CREATE INDEX IF NOT EXISTS idx_instructions_category ON instructions(category);
