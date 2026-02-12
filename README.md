# Codegraph MCP

A Rust MCP server that gives Claude Code a persistent code graph, session memory, and learning system — so it navigates codebases via structure instead of grep/read, and remembers context across compaction.

## Why

Claude Code spends **~92% of navigation tokens** on grep + file reads. On a 183k LOC project, a typical session burns ~150,000 tokens just figuring out what calls what and where things are defined. Codegraph replaces that with structured graph queries that return the same information in ~600 tokens.

It also solves context compaction amnesia: when Claude's context window fills up and gets compressed, all working state (task progress, architectural decisions, which files you're editing) is lost. Codegraph persists all of this in SQLite and restores it in ~125 tokens via `smart_context`.

## Features

### Code Graph (Phases 0-2)
- **Multi-language parsing** via tree-sitter: Rust, TypeScript, JavaScript, Python, Go
- **Incremental indexing** with mtime + xxh3 content hash (only re-parses changed files)
- **Symbol extraction**: functions, classes, structs, enums, traits, interfaces, methods, variables
- **Relationship tracking**: calls, imports, inherits, implements
- **Cross-file resolution**: post-index pass resolves `unresolved::X` stubs to real `file.py::X` nodes (47% resolution rate on 183k LOC project)
- **In-memory graph** (petgraph) for fast BFS neighbor traversal
- **Persistent storage** in libSQL (SQLite)

### Session Memory (Phase 3)
- **Task tracking** with subtasks, status, and blockers
- **Decision log** with reasoning and symbol links
- **Working context** tracking (files, symbols, notes)
- **Smart context** — one-call full state restoration after compaction (~125 tokens)

### Learning System (Phases 4-5)
- **Pattern memory** — record successful patterns with examples, scoped by file/tag
- **Failure memory** — track gotchas with avoidance rules and severity
- **Confidence scoring** — time decay (90-day half-life), drift detection, usage momentum
- **Solution lineage** — track attempts with parent-child retry chains
- **Reflection engine** — convert solution outcomes into patterns/failures
- **Suggestion system** — combines patterns + failures + lineage for recommendations

### Behavioral Niches (Phase 6)
- **Niche clustering** — solutions assigned to niches based on feature vectors (performance, readability, maintainability)
- **Best solution tracking** — each niche tracks its highest-scoring solution

### Skill Distillation (Phase 7)
- **SKILL.md generation** — distill patterns, failures, and conventions into a project skill file
- **Manual instructions** — add project-specific guidance by category (architecture, testing, style, etc.)
- **Convention clustering** — automatically group related patterns into navigational conventions

### Cross-Language Inference (Phase 8)
- **API matching** — detect REST/GraphQL calls in frontend code and match to backend routes
- **Confidence scoring** — connections scored by match quality
- **Path normalization** — handles `:id`, `${id}`, `{id}` style parameter formats

### Sync & Persistence (Phase 9)
- **JSON export** — sync patterns and failures to `.codegraph/` JSON files
- **Confidence-filtered** — only exports high-confidence patterns

### Config & Polish (Phase 10)
- **config.toml** — customizable exclude patterns, file size limits, learning decay, cross-language toggle
- **.codegraph/ auto-init** — creates directory with config.toml + .gitignore on first run
- **Error handling** — proper error propagation (no unwrap panics in production paths)
- **Integration tests** — full lifecycle tests for MCP, indexing, learning, and session systems
- **Benchmarks** — criterion benchmarks for graph search, neighbors, and smart_context

## Installation

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (1.70+)

### 1. Clone and Build

```bash
git clone https://github.com/anthropics/codegraph-mcp.git
cd codegraph-mcp
cargo build --release
```

The binary will be at `target/release/codegraph`.

### 2. Install as MCP Server

Add to your Claude Code MCP config at `~/.claude.json` (or `~/.claude/config.json`):

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "/absolute/path/to/codegraph-mcp/target/release/codegraph"
    }
  }
}
```

Replace `/absolute/path/to/codegraph-mcp` with the actual path where you cloned the repo.

For project-specific config, add to `.claude/config.json` in your project root instead:

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "/absolute/path/to/codegraph-mcp/target/release/codegraph"
    }
  }
}
```

### 3. Install the Skill (Optional but Recommended)

The skill file teaches Claude how to use Codegraph effectively. Copy it to your global Claude skills:

```bash
mkdir -p ~/.claude/skills
cp resources/skill-global.md ~/.claude/skills/codegraph.md
```

This makes Claude automatically use the code graph for navigation, maintain session state across compaction, and build up project knowledge through the learning loop.

### 4. Verify Installation

Start Claude Code in any git project and you should see Codegraph's 26 tools available. Test with:

```
search_symbols(query: "main")
```

If you see "No symbols found", run `index_project(full: true)` first.

### How It Detects Your Project

The server auto-detects the project root by walking up from the current directory looking for:
1. `.codegraph/` directory (highest priority)
2. `.git/` directory

### First Run

On first run, Codegraph creates `.codegraph/` with:
- `config.toml` — default settings (customize exclude patterns, file size limits, etc.)
- `.gitignore` — excludes SQLite databases, allows config.toml for team sharing

After starting Claude Code, run:
```
index_project(full: true)
```

Subsequent sessions only need `index_project()` (incremental) or nothing if files haven't changed.

### Configuration

Edit `.codegraph/config.toml` to customize behavior:

```toml
[indexing]
exclude = ["node_modules", "target", ".git", "dist", "build", "__pycache__"]
max_file_size = 1048576  # 1 MiB

[learning]
decay_half_life = 90  # days

[cross_language]
enabled = true
```

### Uninstall

Remove the `codegraph` entry from your MCP config and optionally delete the binary:

```bash
rm -rf /path/to/codegraph-mcp  # Remove the repo
rm -rf ~/.cache/codegraph       # Remove cached databases
# Remove .codegraph/ from individual projects if desired
```

## MCP Tools (26)

### Code Graph

| Tool | Description |
|---|---|
| `index_project` | Rebuild code graph. `full: true` forces complete rebuild. |
| `search_symbols` | Find symbols by name (partial match), filter by kind/file. |
| `get_file_symbols` | List all symbols in a file with signatures and line numbers. |
| `get_neighbors` | Get callers/callees/imports connected to a symbol. Supports direction and depth. |

### Session

| Tool | Description |
|---|---|
| `start_session` | Start a task with optional subtask breakdown. |
| `get_session` | Load current session state. |
| `update_task` | Update subtask status, add items, manage blockers. |
| `add_decision` | Record a decision with reasoning and related symbols. |
| `set_context` | Track working files, symbols, and notes. |
| `smart_context` | One-shot context restoration. Call on startup and after compaction. |

### Learning

| Tool | Description |
|---|---|
| `recall_patterns` | Query relevant patterns for current task. |
| `recall_failures` | Get failures to avoid (always includes critical). |
| `extract_pattern` | Record a successful pattern with examples. |
| `record_failure` | Record a gotcha with avoidance rule. |
| `record_attempt` | Start tracking a solution attempt. |
| `record_outcome` | Record success/failure/partial outcome. |
| `reflect` | Convert a solution into pattern or failure record. |
| `query_lineage` | Find past solution attempts for a task. |
| `suggest_approach` | Get recommendations from patterns + failures + lineage. |

### Niches, Skills, Cross-Language, Sync

| Tool | Description |
|---|---|
| `list_niches` | List behavioral niches with their best solutions. |
| `distill_project_skill` | Generate SKILL.md from patterns, failures, and conventions. |
| `add_instruction` | Add a manual project instruction by category. |
| `get_project_instructions` | List all manual project instructions. |
| `infer_cross_edges` | Infer frontend→backend API connections. |
| `get_api_connections` | Get API connections for a specific file. |
| `sync_learnings` | Export patterns/failures to .codegraph/ JSON files. |

## Benchmarks

Real-world benchmarks on brain-test (183k LOC, 379 files, Python/Rust/TypeScript):

### Indexing
- **Index time**: 31.5s (full), incremental is near-instant for unchanged files
- **Symbols**: 16,553 extracted
- **Edges**: 39,497 relationships
- **Cross-file resolution**: 793/1,673 stubs resolved (47%)
- **DB size**: 19 MB

### Graph Operations (criterion, 10k node graph)

| Operation | Time |
|---|---|
| `search_symbols` | ~598 µs |
| `search_by_kind` | ~381 µs |
| `search_by_file` | ~282 µs |
| `neighbors_depth1` | ~7.3 µs |
| `neighbors_depth2` | ~13.6 µs |
| `neighbors_outgoing` | ~2.6 µs |
| `file_symbols` | ~63.9 µs |
| `smart_context` | ~10.8 ms |

### Token Savings

| Query | grep/read | codegraph | Savings |
|---|---|---|---|
| "Who uses Diagnosis?" | ~8,000 tokens | ~600 tokens | **92%** |
| "What does bootstrap() call?" | ~8,400 tokens | ~500 tokens | **94%** |
| "What's in diagnosis.py?" | ~14,500 tokens | ~875 tokens | **94%** |
| "Resume after compaction" | ~20,000 tokens | ~125 tokens | **99%** |

**Session-level**: ~138,000 tokens saved per session (92% reduction on navigation/search).

See [BENCHMARK.md](BENCHMARK.md) for full methodology and cost projections.

## Architecture

```
src/
├── main.rs                  # Entry point, async runtime
├── lib.rs                   # Library crate (for integration tests)
├── config.rs                # Project root detection, config.toml parsing
├── mcp/                     # MCP protocol layer
│   ├── protocol.rs          # JSON-RPC 2.0 + MCP types
│   ├── transport.rs         # Stdio transport
│   ├── server.rs            # Request dispatch
│   └── tools.rs             # Tool registry (26 tools)
├── store/                   # Persistence
│   ├── db.rs                # libSQL CRUD (nodes, edges, files)
│   ├── graph.rs             # petgraph in-memory graph
│   └── migrations.rs        # Schema versioning
├── code/                    # Code analysis
│   ├── languages.rs         # Language configs + tree-sitter grammars
│   ├── parser.rs            # Symbol/reference extraction
│   ├── indexer.rs           # Project indexer + cross-file resolution
│   ├── cross_language.rs    # Cross-language API inference
│   └── queries/*.scm        # Tree-sitter queries per language
├── session/                 # Session memory
│   └── state.rs             # Task, decision, context management
├── learning/                # Learning system
│   ├── patterns.rs          # Pattern CRUD + scoped query
│   ├── failures.rs          # Failure CRUD + severity
│   ├── confidence.rs        # Time decay + drift detection
│   ├── lineage.rs           # Solution tracking + retry chains
│   ├── reflection.rs        # Solution → pattern/failure conversion
│   ├── niches.rs            # Behavioral niche clustering
│   ├── conflicts.rs         # Pattern conflict detection
│   └── sync.rs              # JSON file export
└── skill/                   # Skill distillation
    ├── distill.rs           # Pattern → instruction conversion
    ├── categories.rs        # Instruction categorization
    ├── conventions.rs       # Convention clustering
    ├── navigation.rs        # Navigation hint generation
    └── render.rs            # SKILL.md markdown rendering
```

## Development

```bash
cargo build            # Build
cargo test             # Run tests (87 tests)
cargo bench            # Run benchmarks
cargo build --release  # Optimized build

# Debug logging
RUST_LOG=codegraph=debug cargo run
```

## License

MIT
# codegod-mcp
