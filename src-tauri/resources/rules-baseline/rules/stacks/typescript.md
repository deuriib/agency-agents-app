---
name: typescript
description: Mandatory rules for editing TypeScript and TSX files.
globs:
  - "**/*.ts"
  - "**/*.tsx"
alwaysApply: false
trigger: glob          # preferencia para Antigravity; otros harnesses lo ignoran si no aplica
tags: [language, typescript]
---

# TypeScript Rules

- No `any` unless absolutely necessary.
- Use top-level imports only.
- Prefer `interface` over `type` for object shapes.

## Examples

```ts
// ✅ good
interface User { id: string; name: string; }

// ❌ avoid
type User = { id: string; name: string; };
