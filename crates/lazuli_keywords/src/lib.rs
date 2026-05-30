//! `lazuli_keywords` — the single canonical source of truth for every
//! Lazuli language keyword/construct and its presentation + LSP metadata.
//!
//! This crate is a **pure-data leaf**. It depends on nothing internal
//! (optionally `serde` for serialization); `lazuli_syntax`, `lazuli_lsp`,
//! the tmLanguage generator, and the parity tests all depend *on* it,
//! never the reverse. Keeping it a leaf is what lets `lazuli_syntax`
//! (the parser) reference it without a dependency cycle.
//!
//! The optional `serde` feature derives **`Serialize` only** — the
//! registry is a `const` table of `&'static str` references, so the
//! Wave-H2 tmLanguage generator can project it to JSON, but there is
//! nothing to deserialize *into* (you can't borrow `'static` from a
//! decoder).
//!
//! ## What lives here
//!
//! [`ALL`] is the one iterable every downstream surface derives-from or is
//! asserted-against:
//!
//! * the **parser** is proven-complete against it — every keyword literal
//!   the hand-rolled recursive-descent parser recognizes must appear in
//!   [`ALL`] (enforced by `tests/proven_complete.rs`, Wave H1);
//! * the **tmLanguage** keyword-alternation rules are generated from the
//!   [`CapabilitySpec::scope`] field (Wave H2, not yet built);
//! * the **LSP** keyword catalog / `*_BODY_KINDS` / hover one-liners are
//!   generated/asserted from [`CapabilitySpec`] (Wave H3, not yet built);
//! * the **semantic-token** legend is the [`SemanticToken`] projection
//!   (Wave H4, not yet built);
//! * the **diagnostic codes** a capability produces are the [`produces`]
//!   facet — backfilled in Wave C1 and asserted coherent + complete by the
//!   `lazuli_diagnostics_registry` bridge crate.
//!
//! [`produces`]: CapabilitySpec::produces
//!
//! ## Context-as-data
//!
//! A literal valid in N contexts with different scopes is N separate
//! [`CapabilitySpec`] rows — e.g. `default` inside a `cookie` block vs an
//! `errors` block vs an `env` block. This mirrors exactly how the
//! tmLanguage already disambiguates (`entity.name.function.statement.cookie`
//! vs `.errors`) and how the LSP `*_BODY_KINDS` already partition the
//! vocabulary. Context-sensitive scoping therefore falls out of the data
//! rather than needing per-call-site logic.

#![forbid(unsafe_code)]
// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
mod registry;

pub use registry::ALL;

include!("lib_p1.rs");
include!("lib_p2.rs");
