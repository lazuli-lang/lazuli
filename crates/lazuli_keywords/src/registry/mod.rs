//! The populated capability registry — `ALL`.
//!
//! Every keyword/construct the Lazuli parser recognizes, ported from the
//! three pre-existing hand-maintained copies:
//!
//! * **parser literals** (`lazuli_syntax/src/parser/**`) — the
//!   authoritative set of what EXISTS; the proven-complete test asserts
//!   every parser keyword literal appears here.
//! * **LSP catalogs** — `lazuli_lsp/src/keywords.rs` `KEYWORDS` /
//!   `DESIGN_KEYWORDS` (hover text), the `*_BODY_KINDS` /
//!   `*_STATEMENT_KINDS` (per-context membership), `feature.rs`
//!   `FEATURE_BODY_KINDS`.
//! * **tmLanguage scopes** — `editors/vscode/syntaxes/lazuli.tmLanguage.json`
//!   + `editors/vscode/SCOPES.md` (the TextMate scope each keyword gets).
//!
//! A keyword valid in N contexts with different scopes is N rows
//! (context-as-data). Wave C1 backfilled the `produces` diagnostic-code
//! facets: each producing capability row is wrapped with `produces(row,
//! P_<CAP>)`, where `P_<CAP>` mirrors the live `lazuli_doctor` rule `CODE`
//! consts that capability guards. Cross-cutting codes not bound to a single
//! keyword live in [`crate::GLOBAL_DIAGNOSTICS`]. The
//! `lazuli_diagnostics_registry` bridge crate asserts every facet resolves
//! to a live rule and that every live code is claimed exactly once.

//!
//! SPEC-19: the formerly-monolithic 3.7k-LOC file is split into
//! `facets` (diagnostic groups), `builders` (CapabilitySpec constructors),
//! and `sections/*` (the `ALL` rows), concatenated below with `constcat`.

use crate::CapabilitySpec;

pub(crate) mod builders;
pub(crate) mod facets;
mod sections;

/// Every keyword/construct the Lazuli parser recognizes. Concatenated
/// from the `sections/*` slices (SPEC-19 split) via `constcat`.
pub const ALL: &[CapabilitySpec] = constcat::concat_slices!([CapabilitySpec]: sections::s01::ROWS, sections::s02::ROWS, sections::s03::ROWS, sections::s04::ROWS, sections::s05::ROWS, sections::s06::ROWS, sections::s07::ROWS, sections::s08::ROWS, sections::s09::ROWS, sections::s10::ROWS, sections::s11::ROWS);
