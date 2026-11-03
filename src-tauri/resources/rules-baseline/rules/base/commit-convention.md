---
name: commit-conventions
description: Enforce Conventional Commits format for all git commit messages.
alwaysApply: true
tags: [git, commits, conventional-commits]
---

# Git Commit Conventions

All commit messages must follow the [Conventional Commits](https://www.conventionalcommits.org/) specification.
This is strictly required because automated release tooling (like `release-please`) uses these commits to calculate semantic versioning bumps and generate changelogs.

## Format

```
<type>(<optional scope>): <subject>
```

- **type**: Must be one of the following:
  - `feat`: A new feature (triggers a MINOR version bump).
  - `fix`: A bug fix (triggers a PATCH version bump).
  - `docs`: Documentation only changes.
  - `style`: Changes that do not affect the meaning of the code (white-space, formatting, etc).
  - `refactor`: A code change that neither fixes a bug nor adds a feature.
  - `perf`: A code change that improves performance.
  - `test`: Adding missing tests or correcting existing tests.
  - `build`: Changes that affect the build system or external dependencies.
  - `ci`: Changes to CI configuration files and scripts.
  - `chore`: Other changes that don't modify src or test files.
- **scope** (optional): A noun describing a section of the codebase (e.g., `rules`, `convert`, `harness`, `docs`).
- **subject**: A short description in the imperative mood (e.g., "add", not "added").

## Rules

- Use the **imperative, present tense** (e.g., "fix bug" not "fixed bug").
- Do **not** capitalize the first letter of the subject.
- Do **not** end the subject line with a period.
- Keep the subject line under **72 characters**.
- Separate the subject from the body with a blank line.
- Use the body to explain *what* and *why* (not *how*).
- Use footers for referencing issues (e.g., `Closes #123`) or declaring breaking changes (`BREAKING CHANGE: <description>`).

## Examples

```text
feat(rules): add python async conventions
fix(antigravity): join globs with comma instead of space
docs(spec): clarify trigger precedence for model_decision
chore(deps): bump biome to 2.5.1
feat(harness): support Cursor rules format

BREAKING CHANGE: The `trigger` field is now required for all Antigravity rules.
```
