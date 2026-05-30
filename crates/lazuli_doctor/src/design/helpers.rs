//! Shared utilities for the `design-token-*` doctor rules.
//!
//! - `Allowlist` — typed view over `dist/ts-web/design/allowlist.json`,
//!   the closed enum of legal Tailwind classes emitted by Cell B
//!   (`docs/proposals/design-tokens.md` §4.1).
//! - `read_allowlist` — loads the allowlist; returns `None` when the file
//!   is missing so that callers can suppress every design-token rule
//!   before `design.lzi` exists.
//! - `walk_tsx_files` — recursive walk that visits authoring `.tsx` files
//!   only (skips `node_modules`, `dist`, `.lazuli`, `target`, `.git`,
//!   `.next`, `.expo`, and test/story files).
//! - `scan_lines` — yields `(line_number_1_based, line)` pairs.
//! - `is_allowed_by_escape_comment` — implements the §6.3 inline escape
//!   hatch (`// lazuli-allow: <code> — <reason>`).
//! - `iter_class_strings` / `iter_style_block_segments` — small text
//!   slicers shared by the rule implementations. Regex-free, single-pass.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── allowlist ────────────────────────────────────────────────────────────────

include!("helpers_p1.rs");
include!("helpers_p2.rs");
