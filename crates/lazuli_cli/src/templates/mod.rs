#[allow(dead_code)]
pub static DEFAULT_TEMPLATE: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../lazurite/templates/default");

// ---------------------------------------------------------------
// Frontend scaffold templates (L0 #1 §6.1).
// Activated when `lazuli new --frontends web|mobile|web,mobile`.
//
// Newlines are LITERAL `\n` — these strings are written verbatim by
// `cmd_new_frontends::scaffold_frontend_*`. Cross-platform: emitted
// files use LF on every host (Lazuli runs on Windows too).
//
// ---------------------------------------------------------------
// Wave G — Tier-2 stack picks shipped 2026-05-17.
//
// The W1-W7 web deps and M2-M6 mobile deps below are the architect-
// confirmed picks from the wave-2 grading cycle:
//   - W1-W7 source: docs/proposals/lazurite-frontend-stack-web-grading-2026-05-17.md
//     (architect re-grade PASS, self-grade 9.05).
//   - M2-M6 source: docs/proposals/lazurite-frontend-stack-mobile-grading-2026-05-17.md
//     (architect re-grade PASS, self-grade 8.87; M1 deferred honestly).
//   - Architect re-grade + 3 mechanical tightenings:
//     docs/proposals/architect-regrade-cycle-wave-2-2026-05-17.md.
//
// Proposal-pending status is REMOVED once the first pilot adopts the
// scaffolded shape end-to-end (per CHOICE-12 of
// `[[lazurite-vs-frameworks]]`).
//
// The 6+6 closed-catalog skeleton emitted by `scaffold_frontend_web`
// (and mirrored on mobile) is sourced from
// `docs/decisions/client_src_canonical_architecture_2026-05-17.md`
// §3 — top-level `{shell, routes, ui, theme, state, assets}` and
// `ui/{forms, feedback, navigation, display, overlays, layout}`.
// Doctor rule `VOCAB-CLIENT-SRC-001` enforces both catalogs; a fresh
// scaffold output is guaranteed to produce ZERO diagnostics.
// ---------------------------------------------------------------


// Wave R7-3 extract — frontend scaffold templates moved into per-stack
// sibling modules. The static `DEFAULT_TEMPLATE` directory embed stays
// here; the FRONTEND_* constants are re-exported below so every
// existing `crate::templates::FRONTEND_*` call site keeps compiling.

mod manifest;
mod mobile;
mod web;

pub use manifest::*;
pub use mobile::*;
pub use web::*;
