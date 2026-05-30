//! Closed keyword catalogs surfaced through LSP completion and hover.
//!
//! Two arrays:
//!
//! * `KEYWORDS` — every reserved word valid inside canonical (.lzi)
//!   sources. Drives `lazuli_keyword_completion_items` (the global
//!   completion list authors see when no narrower context-aware list
//!   fires) and `keyword_description` (hover one-liners).
//! * `DESIGN_KEYWORDS` — vocabulary specific to `*.design.lzi` sources.
//!   Drives `design_keyword_completion_items` (Backend's completion
//!   path branches on `is_design_lzi_uri`).
//!
//! New keywords land here as comment-tagged blocks so the audit trail
//! survives future rebases. The lists are intentionally hand-curated
//! rather than auto-generated from the grammar — adding a keyword
//! requires a deliberate cut so the LSP completion experience can
//! keep pace with new sugar.
//!
//! ## See also
//! * `lib.rs::lazuli_keyword_completion_items` — wraps `KEYWORDS` into
//!   `CompletionItem`s with hover detail.
//! * `lib.rs::design_keyword_completion_items` — same shape, driven by
//!   `DESIGN_KEYWORDS`.
//! * `crate::hover::keyword_description` — hover one-liner table.
//! * `crate::catalogs` — closed-value catalogs (auth, deploy strategy,
//!   etc.); keep distinct from keyword catalogs.

include!("keywords_p1.rs");
include!("keywords_p2.rs");
