# Codegraph MCP — Benchmark Results

Real-world benchmark comparing standard Claude Code (grep/read) vs Claude Code + Codegraph MCP on two codebases.

## Test Codebases

| Codebase | Files | Lines of Code | Language Mix |
|---|---|---|---|
| video-analyzer-mcp | 1 | 590 | TypeScript |
| brain-test (memoria) | 379 | 183,436 | Python, Rust, TypeScript |

## Indexing Performance

| Metric | video-analyzer-mcp | brain-test |
|---|---|---|
| Index time | 143ms | 35.8s |
| Symbols extracted | 77 | 16,553 |
| Edges (relationships) | 156 | 39,497 |
| Unresolved stubs (before) | 58 | 1,673 |
| Cross-file resolved | — | 793 (47%) |
| Unresolved remaining | 58 | 880 |
| DB size on disk | 96 KB | 19 MB |

### Edge Breakdown (brain-test)

| Relationship Type | Count |
|---|---|
| calls | 23,578 |
| imports | 593 |
| inherits | 78 |
| implements | 70 |

### Symbol Breakdown (brain-test)

| Kind | Count |
|---|---|
| function | 5,359 |
| variable | 5,181 |
| struct | 828 |
| class | 442 |
| module | 329 |
| enum | 111 |
| type | 105 |
| const | 65 |
| trait | 54 |

### Cross-File Resolution (brain-test)

After indexing, a post-processing pass resolves `unresolved::X` stubs to real `file.py::X` nodes by matching symbol names against the full project symbol table.

| Metric | Count |
|---|---|
| Unresolved stubs before | 1,673 |
| Resolved (unambiguous match) | 793 (47%) |
| Remaining (ambiguous or external) | 880 |

The remaining 880 stubs are:
- **External symbols**: stdlib (`print`, `len`, `isinstance`), third-party libraries
- **Ambiguous names**: symbols defined in multiple files (e.g., `Diagnosis` exists in both `diagnosis.py` and `orchestrator.py`)

This means `get_neighbors` now works across file boundaries for 47% of previously broken cross-file references.

## Token Savings

Measured on brain-test (183k LOC). Token estimates use ~4 chars/token.

### Query: "Who uses the Diagnosis class?"

Standard Claude Code needs to grep across the project, then read matching files to understand context.

| Approach | Steps | Tokens |
|---|---|---|
| **grep/read** | grep → 83 matches across 14 files → read files for context (511K chars) | ~129,302 |
| **codegraph** | `get_neighbors("Diagnosis", incoming)` → 4 precise callers | ~600 |
| **Savings** | | **99.5%** |

Realistically Claude reads 3-5 files partially rather than all 14 fully:

| Approach | Realistic Tokens | Savings |
|---|---|---|
| grep/read (realistic) | ~8,000 | — |
| codegraph | ~600 | **92%** |

### Query: "What does bootstrap() call?"

| Approach | Steps | Tokens |
|---|---|---|
| **grep/read** | Read bootstrap.py (908 lines, 33K chars) + grep for called functions | ~8,424 |
| **codegraph** | `get_neighbors("bootstrap", outgoing)` → 7 callees with names and files | ~500 |
| **Savings** | | **94%** |

### Query: "What's in diagnosis.py?"

| Approach | Steps | Tokens |
|---|---|---|
| **grep/read** | Read entire file (1,622 lines, 58K chars) | ~14,502 |
| **codegraph** | `get_file_symbols("diagnosis.py")` → all classes, functions, signatures | ~875 |
| **Savings** | | **94%** |

### Query: "Resume after context compaction"

| Approach | Steps | Tokens |
|---|---|---|
| **grep/read** | Re-read all working files from scratch (5-10 files) | ~20,000 |
| **codegraph** | `smart_context()` → task, progress, decisions, working symbols | ~125 |
| **Savings** | | **99%** |

## Session-Level Impact

A typical multi-step coding session on a 183k LOC project involves 15-25 search/read queries for navigation and understanding.

| Metric | Without Codegraph | With Codegraph |
|---|---|---|
| Tokens on navigation/search | ~150,000 | ~12,000 |
| Net savings per session | — | **~138,000 tokens (92%)** |
| Context compaction frequency | Higher (fills window faster) | Lower (structured responses are compact) |
| Recovery after compaction | Full re-read (~20K tokens) | `smart_context()` (~125 tokens) |
| Decision history after compaction | Lost | Persisted in DB |
| Task progress after compaction | Lost | Persisted in DB |

## What Codegraph Returns vs Grep

### get_neighbors — "Who calls getVideoDuration?"

```
get_neighbors("src/index.ts::getVideoDuration", direction="incoming")

→ [function] extractKeyframes (src/index.ts)
    via: calls, distance: 1
```

vs grep returning every string match including comments, variable names, and imports — requiring Claude to read each file and manually determine which are actual callers.

### get_file_symbols — "What's in this file?"

```
get_file_symbols("memoria/diagnosis.py")

→ L51-86   [class]    FailureType
  L89-113  [class]    RecoveryStrategy
  L176-260 [class]    Diagnosis
  L264-284 [class]    DiagnosisConfig
  L568-649 [function] diagnose
  L651-684 [function] _diagnose_rule_based
  L686-754 [function] _diagnose_memory_based
  L756-807 [function] _diagnose_llm_based
  ... (47 symbols total with signatures)
```

vs reading the entire 1,622-line file to understand its structure.

### smart_context — "Where was I?"

```
smart_context()

→ Task: "Refactor extractKeyframes"
  Progress: 0/2 items completed
  Current item: "Understand flow" (pending)
  Decisions:
    - "Use worker_threads" — "ffmpeg spawns subprocesses anyway"
      related: extractKeyframes, extractAtInterval
```

vs nothing — context compaction wipes all working state.

## Cost Projection

At Anthropic's API pricing (~$3/M input tokens for Sonnet):

| Scenario | Without Codegraph | With Codegraph | Savings |
|---|---|---|---|
| 1 session (25 queries) | $0.45 | $0.04 | $0.41 |
| 10 sessions/day | $4.50 | $0.36 | $4.14 |
| Monthly (200 sessions) | $90.00 | $7.20 | **$82.80** |

*Navigation/search tokens only. Code generation tokens are unchanged.*

## Methodology

- All measurements taken on real codebases, not synthetic benchmarks
- Token estimates use 4 chars/token approximation
- "Realistic" grep/read estimates assume Claude reads 3-5 files partially per query rather than all matching files fully
- Codegraph token counts measured from actual MCP JSON-RPC responses
- Session-level estimates assume 15-25 navigation queries per multi-step coding task
- Cost projections use Claude Sonnet input pricing as of 2025
