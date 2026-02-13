---
name: codegraph
description: Self-improving code intelligence. Persistent code graph + learning memory. Use for navigation, context restoration, and accumulating codebase knowledge. Gets smarter over time.
---

# Codegraph — Persistent Memory & Learning

You have a persistent memory system. **Use it or lose context.**

## CRITICAL: Three Mandatory Checkpoints

### 1. SESSION START — Restore State
**Before doing anything else:**
```
smart_context          → Restores task, decisions, working files from last session
recall_failures        → What NOT to do in this codebase
suggest_approach       → What past experience recommends
```
If results come back, **read and acknowledge them**. Do not ignore past experience.

### 2. DURING WORK — Track State
**Every time you start editing a file or make a decision:**
```
set_context            → Track files/symbols you're working on
add_decision           → Record non-obvious choices with reasoning
update_task            → Mark progress on subtasks
```
**If you don't write state, `smart_context` has nothing to restore after compaction.**

### 3. TASK COMPLETION — Run Learning Loop (MANDATORY)
**Every time you finish a task, subtask, or encounter a failure, you MUST execute all three:**
```
record_outcome         → Log success/failure/partial with description
reflect                → Analyze WHY → creates a pattern or failure record
sync_learnings         → Persist to disk
```
**Do NOT skip this.** This is how the system learns. If you skip it, the knowledge DB stays empty and future sessions get zero benefit from past work.

**Reflection must be specific and causal:**
- BAD: "It worked." / "There was an error."
- GOOD: "When [situation], do [action] because [reason]"

## COMPACTION RECOVERY

When context is compacted, your **immediate next action** is:
```
smart_context
```
No exceptions. Do not re-read files. Do not ask the user what you were doing. The state is persisted.

## Before Complex Changes
```
suggest_approach       → Combined recommendation from patterns + failures + lineage
recall_failures        → What to avoid
record_attempt         → Log your plan BEFORE executing (returns solution_id)
```

## Tool Reference

### Session & Memory
| Tool | When |
|------|------|
| `smart_context` | Session start, after compaction |
| `start_session` | Beginning a new task |
| `update_task` | Progress updates, add items, manage blockers |
| `add_decision` | Record choices with reasoning |
| `set_context` | Track working files/symbols/notes |

### Learning (compounds over time)
| Tool | When |
|------|------|
| `suggest_approach` | Before starting a task |
| `recall_patterns` | What has worked in similar contexts |
| `recall_failures` | What to avoid (critical always included) |
| `record_attempt` | Before significant work |
| `record_outcome` | After completing or failing |
| `reflect` | After every outcome — creates pattern or failure |
| `extract_pattern` | Manually save a successful approach |
| `record_failure` | Manually record a gotcha |
| `query_lineage` | Find past attempts at similar tasks |
| `sync_learnings` | Persist to .codegraph/ JSON files |

### Code Graph (supplementary to Serena)
| Tool | When |
|------|------|
| `search_symbols` | Find symbols by name |
| `get_neighbors` | Find callers/callees/imports with depth |
| `get_file_symbols` | File structure overview |
| `infer_cross_edges` | Detect frontend→backend API connections |
| `get_api_connections` | Get API connections for a file |
| `index_project` | After external file changes |

### Skill & Sync
| Tool | When |
|------|------|
| `distill_project_skill` | Generate .codegraph/SKILL.md from learnings |
| `add_instruction` | Add manual project instruction |
| `get_project_instructions` | List manual instructions |
| `sync_learnings` | Export to .codegraph/ JSON |

### Token Compression (RTK-style)
| Tool | When |
|------|------|
| `bash_compressed` | Execute bash with 60-90% token reduction |
| `compression_stats` | View token savings statistics |

**Use `bash_compressed` for:**
- `git status`, `git diff`, `git log` → grouped & summarized
- `ls`, `find`, `tree` → files grouped by directory/extension
- `grep`, `rg` → matches grouped by file, truncated
- Test runners → failures only, ~90% reduction
- Docker, npm, cargo → progress bars removed

## The Compound Effect

```
Session 1:   Session memory only (learning DB empty — this is expected)
Session 5:   First patterns and failures from reflect calls
Session 20:  suggest_approach gives real recommendations
Session 50+: Near-zero repeated mistakes, SKILL.md is a project guide
```

**Every `reflect` call makes the next session smarter. Every skipped `reflect` is lost knowledge.**
