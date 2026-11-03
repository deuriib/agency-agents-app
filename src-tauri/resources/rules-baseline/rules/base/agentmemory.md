---
name: agentmemory
description: Guidelines and strict rules for interacting with the Agentmemory MCP server via Gateway.
alwaysApply: true
tags: [mcp, memory, context]
---

# Agentmemory MCP Rules via Gateway

- **MEM-01** — Recall Before Asking. At the START of any task, call `memory_recall` (or `memory_smart_search`) with the task context before asking the user to repeat anything.
- **MEM-02** — Save at Decision Points. When you learn something durable (a decision, a fix, a gotcha, a user preference, a project convention), call `memory_save` in the same session — do not batch it for later.
- **MEM-03** — Lessons From Corrections. Signals this applies: explicit "always/never", "learn this", "from now on", a repeated user fix to the same behavior, "I told you before". Call `memory_lesson_save` with a confidence score; re-saving duplicate content strengthens the existing lesson instead of forking it. Before similar work, call `memory_lesson_recall` — check what was learned before deciding. One concept per lesson; state when and where it applies via context and tags.
- **MEM-04** — Resume, Don't Restart. Resume / recap / "where were we" → `memory_sessions` plus `memory_timeline` around the anchor before doing anything else. Lead with any unanswered question from the prior session. Use `memory_timeline` for chronological context and `memory_smart_search` with `expandIds` for detail — never re-derive what history already holds.
- **MEM-05** — Track Multi-Step Work as Actions. Work with 3+ steps → `memory_action_create` with typed dependencies; pull from `memory_frontier` / `memory_next`, not vibes. Memory lease acquire before starting an action; release or renew explicitly — leases prevent double-work across agents. Update status in real time; resolving to done unblocks dependents. Hand off via `memory_signal_send`, pick up via `memory_signal_read`.
- **MEM-06** — Slots Are Small, Pinned State. Durable user or project state (persona, pending items) → memory slots: `memory_slot_get` / `memory_slot_list` first, then `memory_slot_replace`. Appends fail past `sizeLimit` — compact via replace first, never loop blind appends. Seeded defaults are starting values, not gospel; replace them when the user corrects them.
