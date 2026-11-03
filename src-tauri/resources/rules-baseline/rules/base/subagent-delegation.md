---
name: subagent-delegation
description: Guidelines for delegating tasks to subagents using spawn, dispatch, or invoke.
trigger: model_decision
tags: [workflow, subagents, context-management]
---

# Subagent Delegation Rules

- **SUB-01** — Trigger Words. When the user says `spawn`, `dispatch`, or `invoke`, they are explicitly asking you to delegate the task to a subagent. Do not attempt to execute the task in the main thread.
- **SUB-02** — When to Delegate. Use subagents for:
  - **Context-heavy tasks:** Searching large codebases, reading multiple files, or exploring documentation.
  - **Parallelizable work:** Multiple independent fixes or searches that can happen concurrently.
  - **Specialized tasks:** Writing tests, refactoring specific modules, or applying strict formatting.
- **SUB-03** — Clear Prompts. When spawning a subagent, provide a self-contained prompt. Do not assume the subagent has access to the current conversation history. Pass all necessary context, file paths, and constraints explicitly.
- **SUB-04** — Await and Synthesize. Wait for the subagent to complete and return its result. Verify the output and synthesize it back into the main workflow before presenting the final answer to the user.
- **SUB-05** — Fail Safely. If a subagent fails or hits a limit, do not silently retry in the main thread. Report the failure, analyze the error, and decide whether to retry the subagent with a better prompt or escalate to the user.
