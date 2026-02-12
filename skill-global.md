---
name: codegraph
description: Self-improving code intelligence. Persistent code graph + learning memory. Use for navigation, context restoration, and accumulating codebase knowledge. Gets smarter over time.
---

# Codegraph

Persistent code graph + learning memory that makes you smarter at each codebase over time.

## What You Get

1. **Code Graph** — Symbol index with relationships (calls, imports, inherits). Search by name, traverse by relationship, list by file.
2. **Session Memory** — Task state, decisions, and working context that survives compaction.
3. **Learning System** — Patterns (what works), Failures (what doesn't), and Solution lineage (what was tried).
4. **Project Skill** — Auto-generated SKILL.md with accumulated project-specific instructions.
5. **Cross-Language** — Inferred API connections between frontend and backend code.

## Session Workflow

### On Every Session Start

```
1. smart_context              → Restore task, decisions, working state (~300 tokens)
2. Read .codegraph/SKILL.md   → If it exists, follow accumulated project instructions
3. index_project              → Only if files changed outside this session
```

### Before Working on a Task

```
1. suggest_approach            → Get recommendation from patterns + failures + lineage
2. recall_failures             → Check what NOT to do (critical failures always shown)
3. record_attempt              → Log your plan BEFORE starting (returns solution ID)
4. start_session / update_task → Track what you're doing
```

### During Work

```
- search_symbols    → Find symbols by name (NEVER grep for definitions)
- get_neighbors     → Find callers/callees/imports (NEVER grep for usages)
- get_file_symbols  → See file structure before reading full file
- set_context       → Track files and symbols you're modifying
- add_decision      → Record important choices with reasoning
```

### After Completing Work

```
1. record_outcome              → Log success/failure/partial
2. reflect                     → Extract WHY it worked/failed → creates pattern or failure
3. distill_project_skill       → Optionally regenerate SKILL.md with new learnings
4. sync_learnings              → Export patterns/failures to JSON for persistence
```

## Tools Reference

### Navigation (use instead of grep/read)

| Tool | Use When |
|------|----------|
| `search_symbols` | Finding a function/class/struct by name |
| `get_neighbors` | Finding what calls/uses/imports a symbol |
| `get_file_symbols` | Understanding file structure before reading |
| `get_api_connections` | Tracing frontend calls to backend routes |
| `index_project` | After external file changes or major refactoring |
| `infer_cross_edges` | Detecting frontend-backend API connections |

### Session (survives compaction)

| Tool | Use When |
|------|----------|
| `smart_context` | Session start, after compaction, or anytime you need full state |
| `start_session` | Beginning a new task (clears old session) |
| `update_task` | Changing item status, adding items, managing blockers |
| `add_decision` | Recording an important choice with reasoning |
| `set_context` | Tracking which files/symbols you're working on |
| `get_session` | Getting raw session data |

### Learning (compounds over time)

| Tool | Use When |
|------|----------|
| `suggest_approach` | Starting a task — combines patterns + failures + lineage |
| `recall_patterns` | Finding what has worked in similar contexts |
| `recall_failures` | Checking what to avoid (critical always included) |
| `query_lineage` | Finding past attempts at similar tasks |
| `record_attempt` | Before starting significant work |
| `record_outcome` | After completing or failing at a task |
| `reflect` | Analyzing WHY something worked or failed |
| `extract_pattern` | Saving a successful approach for reuse |
| `record_failure` | Recording a mistake to prevent recurrence |

### Skill & Sync

| Tool | Use When |
|------|----------|
| `distill_project_skill` | Regenerating SKILL.md from accumulated learnings |
| `add_instruction` | Manually adding a project-specific instruction |
| `get_project_instructions` | Listing all manual instructions |
| `list_niches` | Viewing solution clusters by quality dimension |
| `sync_learnings` | Exporting patterns/failures to .codegraph/ JSON files |

## Rules

### Always Do

- `smart_context` on session start and after compaction
- `suggest_approach` before complex tasks
- `search_symbols` before reading files (92% token savings vs grep)
- `record_attempt` before significant changes
- `reflect` after success or failure — this is how the system learns
- `add_decision` for architectural or design choices
- Read `.codegraph/SKILL.md` if it exists — it's accumulated project knowledge

### Never Do

- Read entire files to find functions — use `search_symbols`
- Grep for usages — use `get_neighbors`
- Re-explain context after compaction — use `smart_context`
- Repeat known mistakes — check `recall_failures` first
- Ignore past attempts — check `query_lineage`
- Skip reflection after failures — that's how knowledge compounds

## The Learning Loop

```
PLAN
├── suggest_approach       → Combined recommendation from all knowledge
├── recall_patterns        → What works in this context
├── recall_failures        → What to avoid
└── record_attempt         → Log your plan (get solution ID)

EXECUTE
├── Use code graph tools for navigation (NOT grep/read)
├── set_context            → Track files/symbols being modified
└── add_decision           → Record important choices

REFLECT
├── record_outcome         → Success/failure/partial
├── reflect                → WHY did it work/fail? (creates pattern or failure)
│   ├── Success? → Pattern created (reusable for similar tasks)
│   └── Failure? → Failure recorded (prevents recurrence)
└── sync_learnings         → Persist to disk
```

## Reflection Format

When calling `reflect`, provide:

```
INTENT:      What were you trying to achieve? (1 sentence)
MECHANISM:   How did it work/fail? (1 sentence, optional)
ROOT_CAUSE:  WHY at a fundamental level? (not "it worked" or "syntax error")
LESSON:      "When [situation], do [action] because [reason]"
CONFIDENCE:  0.0-1.0
SCOPE:       File patterns and tags this applies to
```

Good reflections are specific and causal. Bad: "It failed because of an error." Good: "When modifying FK-constrained tables, create parent rows before child rows because SQLite enforces FK constraints immediately on INSERT."

## Project Skill

Codegraph auto-generates a project-level SKILL.md at `.codegraph/SKILL.md` containing:
- Architecture conventions
- Testing practices
- Gotchas to avoid
- Navigation hints
- Workflow requirements

**Read it when it exists.** Regenerate with `distill_project_skill` after accumulating new learnings.

## How It Compounds

```
Session 1:   Token savings + session memory
Session 5:   First patterns and failures recorded
Session 20:  Meaningful suggest_approach recommendations
Session 50:  SKILL.md is a comprehensive project guide
Session 100: 60%+ fewer failed attempts, near-zero repeated mistakes
```

Every `reflect` call makes the next session better.
