# Agent Operating Manual

## MANDATORY: Codegraph Memory System

You have a persistent memory system via Codegraph MCP. **You MUST use it.** It is not optional.

### COMPACTION RECOVERY (NON-NEGOTIABLE)

When context is compacted or you are resuming a session, your **VERY FIRST action** before doing ANYTHING else:

```
smart_context
```

This restores your task, decisions, working files, and notes. Do NOT attempt to re-read files or ask the user what you were doing. The state is persisted. Call `smart_context` and read it.

**If you skip this, you lose all prior session state. There is no excuse to skip it.**

### SESSION STARTUP (EVERY SESSION)

Before responding to the user's first message, execute these in order:

1. `smart_context` — Restore prior state. If empty, this is a fresh session.
2. `start_session` with the user's task — Register what you're working on.
3. `recall_failures` — Check what has gone wrong before in this codebase. **Read the results.**
4. `suggest_approach` — Check if past patterns/lineage suggest an approach. **Read the results.**

If `recall_failures` or `suggest_approach` return results, **you must acknowledge them** in your response. Do not silently ignore past experience.

### DURING WORK (TRACK STATE)

As you work, maintain session state:

- `set_context` — Update working files and symbols whenever you start editing a new file
- `add_decision` — Record every non-obvious architectural or implementation choice with reasoning
- `update_task` — Mark subtasks as in_progress/completed as you go

This state is what `smart_context` restores after compaction. **If you don't write it, you can't recover it.**

### TASK COMPLETION (EVERY TASK)

When you finish a task or a significant subtask, you MUST run the learning loop:

1. `record_outcome` — Log success, failure, or partial with a description
2. `reflect` — Analyze WHY it worked or failed. This creates a pattern or failure record.
3. `sync_learnings` — Persist to disk

**Reflection format:** "When [situation], do [action] because [reason]"

**Bad reflection:** "It worked." / "There was an error."
**Good reflection:** "When adding edges to a graph with FK constraints, create stub target nodes first because the DB enforces referential integrity on INSERT."

Do NOT skip the learning loop. Every `reflect` call makes future sessions better. If you skip it, past mistakes get repeated.

### BEFORE COMPLEX CHANGES

Before starting any non-trivial implementation:

1. `suggest_approach` — Check accumulated knowledge
2. `recall_failures` — Check what to avoid
3. `record_attempt` — Log your plan BEFORE executing (returns solution_id for tracking)

---

## Code Navigation

### Serena — Primary (LSP-powered)
Use Serena for all code understanding. It has real type resolution via language servers.

- `find_symbol` — Find definitions by name
- `find_referencing_symbols` — Find all usages/callers
- `get_symbols_overview` — File/module structure
- `rename_symbol` — Safe renames across codebase
- `search_for_pattern` — Regex search with context
- `find_symbol` with `include_body=True` — Read specific symbol bodies

**Serena is more accurate than grep or file reads for code navigation. Prefer it.**

### Codegraph — Supplementary (Graph queries)
Use codegraph's code graph tools when you need:
- Cross-language API tracing → `infer_cross_edges`, `get_api_connections`
- Bulk relationship queries → `get_neighbors` with depth > 1
- Token-efficient overviews → `get_file_symbols`

---

## Operating Principles

- **Serena navigates code. Codegraph remembers context.** Use both, for what they're good at.
- **State is cheap, amnesia is expensive.** Always track what you're doing via set_context and add_decision.
- **Past experience is data.** Always check suggest_approach and recall_failures before complex work.
- **Reflect or repeat.** The learning loop is how you get smarter. Skipping it guarantees repeating mistakes.
- **Be autonomous.** You have the tools to understand, track, and learn. Don't wait to be told.

---

## Project-Specific Instructions

<!-- Add per-project overrides below, or use per-repo CLAUDE.md files which layer on top of this. -->

