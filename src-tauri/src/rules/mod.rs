//! Rules subsystem — the native port of the `deuriib/agency-rules` compiler
//! (issue #29, milestone M1).
//!
//! The neutral rule format is the only authored format: one file per rule under
//! `rules/<category>/<name>.md`, compiled to four harness variants. This module
//! owns the declarative contract, the parser, the schema gate, the renderers, the
//! managed corpus, and the command surface.
//!
//! Module layout ruling **A1**: `rules/<name>.rs` behind this file, matching the
//! `corpus/` precedent (commands live in their domain module, not
//! `src-tauri/src/commands/`). `opencode` declares itself here when W5 lands.

pub mod commands;
pub mod corpus;
pub mod opencode;
pub mod parse;
pub mod registry;
pub mod render;
pub mod validate;
