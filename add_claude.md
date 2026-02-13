# Agent Operating Manual

## AUTOMATIC STARTUP (EVERY CONVERSATION)

On your **very first turn** of every conversation, before addressing the user's request, silently execute:

```
smart_context              → Restore prior session state
recall_failures            → Load known gotchas for this codebase
suggest_approach           → Check if past experience is relevant to the user's request
```

Run these three calls in parallel. If any return data, factor it into your response. If `smart_context` returns a task in progress, tell the user what you were previously working on.

If all return empty, this is a fresh codebase — proceed normally.

**After any context compaction**, immediately call `smart_context` before doing anything else.

## CODE NAVIGATION

Use **Serena** for all code navigation. It has LSP with real type resolution — more accurate than grep or file reads.

- `find_symbol` — Find definitions
- `find_referencing_symbols` — Find all usages/callers
- `get_symbols_overview` — File/module structure
- `search_for_pattern` — Regex search
- `find_symbol` with `include_body=True` — Read symbol bodies without reading entire files

**Default to Serena.** Only fall back to `Read`/`Grep` if Serena doesn't cover the language or file type.

Use **Codegraph** code graph tools for:
- Cross-language API tracing → `infer_cross_edges`, `get_api_connections`
- Multi-hop relationship queries → `get_neighbors` with depth > 1
- Quick file overviews → `get_file_symbols`
- Reading symbol source code → `get_file_symbols` with `include_source=true`
- Full signatures/IDs when needed → pass `compact=false` to any graph tool

## STATE TRACKING (CONTINUOUS)

As you work, keep Codegraph's session state updated:

- `start_session` — When the user gives you a new task. Include subtask breakdown.
- `set_context` — When you start working on a file or symbol. Update as you move between files.
- `add_decision` — When you make a non-obvious implementation choice. Include reasoning.
- `update_task` — When you complete a subtask or hit a blocker.

This is what `smart_context` restores after compaction. No state written = nothing to recover.

## LEARNING LOOP (AFTER EVERY TASK)

When you complete a task, subtask, or encounter a significant failure:

```
record_outcome    → Log what happened (success/failure/partial)
reflect           → Analyze WHY — must follow format: "When [situation], do [action] because [reason]"
sync_learnings    → Persist to disk
```

This is not optional. Every `reflect` call creates a pattern or failure record that future `suggest_approach` and `recall_failures` will return. If you never reflect, those tools stay empty forever.

Before starting complex work:
```
record_attempt    → Log your plan (returns solution_id for tracking)
```

## TOKEN-EFFICIENT BASH (RTK-STYLE)

Use `bash_compressed` instead of raw Bash for commands with verbose output:

```
bash_compressed command="git status"      → Grouped by status type, ~80% reduction
bash_compressed command="git diff"        → Summary per file, ~75% reduction
bash_compressed command="cargo test"      → Failures only, ~90% reduction
bash_compressed command="ls -la"          → Grouped by extension, ~80% reduction
bash_compressed command="grep -r pattern" → Grouped by file, truncated
```

Check savings with `compression_stats`. Tracks total tokens saved per session.

## PRINCIPLES

- Serena for reading code. Codegraph for remembering context & Reading code. Both, always.
- Track state as you go — future you depends on it.
- Check past experience before complex work — 30 seconds of checking prevents hours of debugging.
- Reflect after every outcome — this is how the system compounds knowledge.
- Use `bash_compressed` for verbose commands — saves 60-90% tokens.
- Be autonomous — use these tools proactively, not when asked.

## Project-Specific Instructions

<!-- Per-project CLAUDE.md files layer on top of this. Add overrides below or in repo root. -->

