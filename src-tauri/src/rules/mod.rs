//! Rules subsystem — the native port of the `deuriib/agency-rules` compiler
//! (issue #29, milestone M1).
//!
//! The neutral rule format is the only authored format: one file per rule under
//! `rules/<category>/<name>.md`, compiled to four harness variants. This module
//! owns the declarative contract, the parser, the schema gate, and (in later
//! workstreams) the renderers, the managed corpus, the OpenCode plugin merge and
//! the command surface.
//!
//! Module layout ruling **A1**: `rules/<name>.rs` behind this file, matching the
//! `corpus/` precedent (commands live in their domain module, not
//! `src-tauri/src/commands/`). `render`, `corpus`, `opencode` and `commands`
//! declare themselves here as their tasks land.

// W1 (tasks 1-4) lands the contract, parser and schema gate ahead of the
// renderers, corpus and commands that consume them (W2-W6 in the same
// milestone), so a production build cannot reach this surface yet. The allow is
// scoped to the module declarations and goes away when `commands.rs` is
// registered in `lib.rs`'s `invoke_handler`.
#[allow(dead_code)]
pub mod parse;
#[allow(dead_code)]
pub mod registry;
#[allow(dead_code)]
pub mod validate;
