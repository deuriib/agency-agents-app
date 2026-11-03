---
name: mcp-gateway-protocol
description: Apply this guidelines and strict rules for interacting with the Gateway MCP server.
alwaysApply: true
tags: [mcp, gateway, tools]
---


## MCP Rules — Gateway Protocol

All MCP tools run via `gateway_*` helpers. **Mandatory order:**

1. `gateway_listToolFiles` — discover servers
2. `gateway_readToolFile` — read `servers/<Name>.pyi` stub for exact tool names + params
3. `gateway_getToolDocs` (optional) — full docs when stub is truncated
4. `gateway_executeTool` — run Starlark: `Server.tool(param=value)`
5. `gateway_callManyTool` (optional) — fan out 1–50 independent calls in one round trip; a failed item returns `ok=false` without cancelling its siblings. Never use it for chained calls: if one call needs another's result, write that in step 4.

> **Parallel:** Step 1 first. Steps 2-4 can run concurrently across servers/tools. Step 5 replaces N sequential step-4 calls with one round trip.

### Calling Convention (Starlark)

```python
result = Server.tool(param=value, params....)
value = result["key"]  # brackets, not dot
```

- Sync only, keyword args, no `try/except`, no classes, no imports.
- Each `executeTool` scope is fresh — re-fetch or persist via MCP.

### Anti-Patterns

| Anti-Pattern                        | Fix                                       |
| :---------------------------------- | :---------------------------------------- |
| Guessing tool names                 | Stub from `readToolFile` is authoritative |
| Skipping `listToolFiles`            | Always discover first when unsure         |
| `executeTool` or `callManyTool` before `readToolFile` | Confirm signature first                   |
| Assuming cross-call state           | Every call is isolated                    |
