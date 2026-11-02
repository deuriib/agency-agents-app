## Summary

<!-- What changed and why. One or two paragraphs, no line-by-line diff. -->

Closes #

## Type of change

- [ ] Bug fix — `fix:`
- [ ] New feature — `feat:`
- [ ] Refactor / internal change — `refactor:`, `perf:`
- [ ] Documentation — `docs:`
- [ ] CI, build, or dependencies — `ci:`, `build:`, `chore:`
- [ ] Breaking change (explain below)

<!-- If breaking: what breaks, and what users must do. -->

## How was this tested?

<!-- Exact commands plus any manual steps. Screenshots for UI changes. -->

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib --tests
npm run check
npm run build
```

## Screenshots / recordings

<!-- Before/after for UI changes. Delete this section if not applicable. -->

## Checklist

- [ ] The PR title follows Conventional Commits — it becomes the changelog entry.
- [ ] I ran the checks above locally, or explain why they do not apply.
- [ ] Behaviour changes include tests.
- [ ] Documentation and `memory-bank/` are updated where behaviour or decisions changed.
- [ ] No secrets, tokens, credentials, or personal paths are included.
