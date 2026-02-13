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

## Building

```bash
# Prerequisites: Rust toolchain (1.70+)
cargo build --release
cargo test              # 87 tests
cargo bench             # criterion benchmarks
```

## Usage

Add to your MCP client config (e.g. `~/.claude.json`):

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "/path/to/target/release/codegraph"
    }
  }
}
```

On first run in a git repo, it creates `.codegraph/` with a default `config.toml`. Run `index_project` to build the code graph.

## What I Learned

This was a personal project to explore the design space of "AI agent memory." Some things that became clear along the way:

- **Structured code graphs are useful but not as differentiated as expected.** LSPs and IDE-native indexing solve the same navigation problem with better type resolution. The graph is most useful for cross-file dependency traversal, less so for single-file exploration.
- **Session memory is genuinely helpful** for long-running tasks that hit context limits, but its value shrinks as context windows grow.
- **The learning system is the most ambitious part** and the hardest to validate. AI self-reflection produces mixed-quality records. The hypothesis that accumulated patterns improve future agent performance is plausible but unproven at scale.
- **MCP as a protocol works well** for this kind of tool. Stdio transport is simple, the tool/resource model is clean, and lazy initialization (waiting for the client's `initialize` handshake to resolve the project root) was the right call.

## License

MIT
