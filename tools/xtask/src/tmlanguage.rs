//! tmLanguage keyword-rule generation from `lazuli_keywords::ALL`.
//!
//! # What this generates
//!
//! The **keyword-alternation repository rules** (`#kw-*`) of the VS Code
//! grammar — one repo rule per `(Context, scope, sigil-shape)` group in the
//! [`GROUPS`] allowlist. Each rule's match is `^\s+(<literals, longest-first>)\b`
//! and its `name` is the group's TextMate scope leaf. Because a literal valid
//! in N contexts is N rows in the registry (context-as-data), grouping by
//! `(Context, scope)` reproduces the grammar's per-block scope leaves exactly.
//!
//! Two tiers (see [`GROUPS`]):
//!
//! 1. **Wired** rules an `include` in a hand-written `begin/end` block points
//!    at (`kw-cookie`, `kw-audit`, …) — editing the registry widens the live
//!    highlight.
//! 2. **Fallback-coverage** rules (WT-3) that back the curated multi-group
//!    fallback alternations (`command`/`query`/`view*`/`agent`/… bodies). These
//!    are GENERATED so every registry keyword is a literal substring of the
//!    grammar (the `keyword_surface_parity` highlight half is drift-proof by
//!    construction — this retired the 44-entry `HIGHLIGHT_SURFACE_GAP`) but are
//!    intentionally left UN-`include`d, so they change the generation source,
//!    not a rendered token (snapshots stay byte-for-byte green).
//!
//! # What it does NOT generate (stays hand-written, structural)
//!
//! * block `begin`/`end` rules, entity-name captures, the top-level
//!   `patterns` include list, and the `{ "include": "#kw-*" }` references
//!   (which block points at which generated rule is a hand-written decision);
//! * strings, comments, operators, punctuation, `#references`, `#types`,
//!   `#decorators`, `#modifiers`, `#constants` (cross-cutting / regex-shaped);
//! * the `locale_negotiate` sub-block alternations (`source`/`strategy` are
//!   closed-catalog rules, not a single-group keyword projection) and the
//!   value-catalog alternations. (The app-level `locale` block statement
//!   alternation — `default`/`supported`/`fallback` — IS generated as
//!   `#kw-locale`.)
//!
//! # Freshness
//!
//! `gen-tmlanguage --check` regenerates the `#kw-*` section in memory and
//! asserts it is byte-identical to the committed grammar. CI / the
//! `editors/vscode` grammar test wires this so a hand-edit or a forgotten
//! regen fails loudly.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lazuli_keywords::{ALL, Context, Sigil};
use serde_json::{Map, Value};

include!("tmlanguage_p1.rs");
include!("tmlanguage_p2.rs");
