# Codegraph MCP

A self-use Rust MCP server that gives AI coding agents persistent code understanding and memory across sessions. Built as a personal tool to explore what happens when you give an LLM structured access to a codebase's symbol graph, session state, and accumulated project knowledge.

## What It Does

Codegraph runs as an [MCP server](https://modelcontextprotocol.io/) (stdio transport) and exposes 26 tools that an AI agent can call:

- **Code Graph** — Parses source code with tree-sitter (Rust, TypeScript, JavaScript, Python, Go), extracts symbols and their relationships (calls, imports, inherits), and stores them as a directed graph. The agent can search symbols, traverse dependencies, and understand file structure without reading entire files.

- **Session Memory** — Tracks the agent's current task, subtasks, decisions, and working context. Survives context window compaction so the agent can resume where it left off.

- **Learning System** — Records patterns (things that worked), failures (gotchas to avoid), and solution lineage (attempt chains with outcomes). A reflection engine converts outcomes into reusable knowledge. A suggestion system combines all three to recommend approaches for new tasks.

- **Skill Distillation** — Generates a `SKILL.md` from accumulated patterns, failures, and conventions — a machine-readable summary of project-specific knowledge.

- **Cross-Language Inference** — Detects REST/GraphQL calls in frontend code and matches them to backend route definitions.

- **Bash Compression** — Compresses verbose command output (git status, test results, directory listings) to reduce token usage.

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (async, tokio) |
| Protocol | MCP over stdio (JSON-RPC 2.0) |
| Parsing | tree-sitter (5 language grammars) |
| Graph | petgraph (directed graph with BFS traversal) |
| Storage | libSQL / SQLite (two databases: code graph + learning) |
| Hashing | xxh3 (content-based change detection) |
| Config | TOML |

## Architecture

```
src/
├── main.rs              # Entry point
├── config.rs            # Project root detection, config.toml
├── mcp/                 # MCP protocol layer
│   ├── protocol.rs      # JSON-RPC 2.0 + MCP types
│   ├── transport.rs     # Stdio transport
│   ├── server.rs        # Request dispatch, lazy init
│   └── tools.rs         # Tool registry (26 tools)
├── store/               # Persistence
│   ├── db.rs            # SQLite CRUD
│   ├── graph.rs         # In-memory petgraph
│   └── migrations.rs    # Schema versioning
├── code/                # Code analysis
│   ├── parser.rs        # tree-sitter symbol extraction
│   ├── indexer.rs       # Incremental indexing + cross-file resolution
│   ├── languages.rs     # Language configs + grammars
│   └── cross_language.rs
├── session/             # Session state machine
│   └── state.rs         # Task, decisions, context tracking
├── learning/            # Learning system
│   ├── patterns.rs      # Pattern storage + scoped queries
│   ├── failures.rs      # Failure records + severity
│   ├── confidence.rs    # Time decay + drift detection
│   ├── lineage.rs       # Solution attempt tracking
│   ├── reflection.rs    # Outcome → pattern/failure conversion
│   ├── niches.rs        # Behavioral clustering
│   └── sync.rs          # JSON export
├── skill/               # Skill distillation
│   ├── distill.rs       # Pattern → SKILL.md generation
│   ├── conventions.rs   # Convention clustering
│   └── render.rs        # Markdown rendering
└── compress/            # Token-saving output compression
    ├── bash.rs          # Command dispatch
    ├── git.rs           # Git output compression
    ├── test_output.rs   # Test result compression
    └── analytics.rs     # Savings tracking
```

## Setup

### 1. Build

```bash
git clone https://github.com/subhankar-chowdhury/codegraph-mcp.git
cd codegraph-mcp
cargo build --release
```

The binary will be at `target/release/codegraph`.

### 2. Add to your MCP client

The server communicates over **stdio** (newline-delimited JSON-RPC 2.0) — your MCP client must connect via stdio transport, not HTTP/SSE. Add it to whichever MCP client you use:

**Claude Code** (`~/.claude.json`):

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "/absolute/path/to/codegraph-mcp/target/release/codegraph",
      "type": "stdio"
    }
  }
}
```

**Cursor** (`.cursor/mcp.json` in your project root):

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "/absolute/path/to/codegraph-mcp/target/release/codegraph",
      "type": "stdio"
    }
  }
}
```

Replace `/absolute/path/to/codegraph-mcp` with wherever you cloned the repo.

### 3. First run

When you start a session in any git repo, Codegraph will:
1. Auto-detect the project root (walks up looking for `.git/`)
2. Create a `.codegraph/` directory with a default `config.toml`
3. Wait for you to index — run `index_project(full: true)` to build the code graph

After the initial index, subsequent sessions only need `index_project()` (incremental — skips unchanged files) or nothing at all if you haven't changed code.

### 4. Configuration (optional)

Edit `.codegraph/config.toml` to customize:

```toml
[indexing]
exclude = ["node_modules", "target", ".git", "dist", "build", "__pycache__"]
max_file_size = 1048576  # 1 MiB

[learning]
decay_half_life = 90  # days

[cross_language]
enabled = true
```

### Running tests

```bash
cargo test              # 87 tests
cargo bench             # criterion benchmarks
```

## Benchmarks

Independently tested on a 395K-line, 974-file codebase (Python + Rust + TypeScript) across 4 configurations: Vanilla (grep/read), Serena (LSP), Codegraph, and Serena+Codegraph combined.

### Per-Query Token Savings

| Query | Grep/Read | Codegraph | Savings |
|-------|-----------|-----------|---------|
| "Who uses Diagnosis?" | 42,478 tokens | 151 tokens | **99.6% (281x)** |
| "What does bootstrap() call?" | 9,431 tokens | 366 tokens | **96% (26x)** |
| "What's in diagnosis.py?" | 14,503 tokens | 1,382 tokens | **90% (10x)** |
| "Resume after compaction" | ~20,000 tokens | 95 tokens | **99.5% (210x)** |

Codegraph's cost scales with answer size, not file size — the gap widens on larger codebases:

| Codebase Size | Grep/Read Cost | Codegraph Cost | Gap |
|---------------|---------------|----------------|-----|
| 1K lines | ~250 tokens | ~150 tokens | 1.7x |
| 100K lines | ~15,000 tokens | ~500 tokens | 30x |
| 395K lines | ~42,000 tokens | ~600 tokens | 70x |

### Four-Config Comparison (3 navigation tasks)

| Config | Total Tokens | Accuracy | Tokens per Correct Answer |
|--------|-------------|----------|--------------------------|
| Vanilla (grep/read) | 82,739 | 2.5/3 | 33,096 |
| Serena (LSP) | 76,921 | 2.5/3 | 30,768 |
| **Codegraph** | **89,368** | **3/3** | **29,789** |
| Serena + Codegraph | 105,760 | 3/3 | 35,253 |

Codegraph has the best tokens-per-correct-answer ratio. Vanilla grep wins on raw token cost for simple text search, but misses semantic accuracy on cross-file tracing.

### Learning System (10-Session Evolution Test)

10 sequential sessions with overlapping themes. Sessions 9-10 deliberately overlapped with earlier sessions to test knowledge compounding.

- **Approach generation**: Working. `suggest_approach` returned increasingly specific strategies across sessions. By sessions 9-10, it explicitly synthesized learnings from earlier sessions ("Leverage Session 1 + 2 + 4. Search for redis.RedisError catches.").
- **Pattern retrieval**: Working. `recall_patterns` surfaces relevant patterns scoped by file paths and tags, with confidence scoring and time decay.
- **Failure recall**: Working. `recall_failures` always includes critical-severity failures and filters others by scope relevance.

See [BENCHMARK.md](BENCHMARK.md) for full methodology, per-task breakdowns, and accuracy analysis.

## What I Learned

This was a self-use project to explore the design space of "AI agent memory." Some takeaways:

- **The code graph's value is situational.** On small repos, grep is fine. On large codebases, the per-query savings (26-281x) are real and compound across a session.
- **Session memory is genuinely helpful** for long-running tasks that hit context limits, though its value shrinks as context windows grow.
- **The learning system compounds.** Approach generation noticeably improves over sessions as patterns accumulate. The "When X, do Y because Z" format produces actionable, reusable knowledge.
- **MCP as a protocol works well** for this kind of tool. Stdio transport is simple, the tool/resource model is clean, and lazy initialization (waiting for the client's `initialize` handshake to resolve the project root) was the right call.

## License

MIT
