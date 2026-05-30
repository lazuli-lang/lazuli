//! `lazuli_diagnostics_registry` — the **convergence bridge** (Wave C1).
//!
//! This crate ASSERTS the logical "one declaration per capability" invariant:
//! every diagnostic code the doctor can emit is *claimed* by exactly one
//! language capability's `produces[]` facet (in [`lazuli_keywords::ALL`]) OR by
//! the documented cross-cutting [`lazuli_keywords::GLOBAL_DIAGNOSTICS`] bucket.
//!
//! It does **not** re-home anything. The doctor rule files stay authoritative
//! for diagnostic logic; `lazuli_keywords` stays the pure-data leaf that owns
//! the capability table. This crate sits *над* both (`deps:
//! lazuli_keywords + lazuli_doctor`) and is the one place where the two
//! manifests are linked and checked for coherence:
//!
//! * **Resolvable** — every claimed `code` is a live `lazuli_doctor` rule
//!   `CODE` const; its mirrored `category` equals
//!   [`lazuli_doctor::RuleCategory::from_code_prefix`]; its mirrored
//!   `base_severity` is a well-formed [`lazuli_doctor::DoctorSeverity`].
//! * **Complete** — every live `lazuli_doctor` `CODE` const is claimed exactly
//!   once (no ORPHAN diagnostic with no capability home, no DUPLICATE claim).
//!   This is the assertion that makes the convergence *enforceable*: a future
//!   diagnostic added without a capability home (or to GLOBAL) fails the build.
//!
//! The set of live codes is enumerated at test time by scanning the
//! `lazuli_doctor/src/**` source for `const … CODE …: &'static str = "…"`
//! declarations — the same proven-complete pattern the H1
//! `lazuli_keywords::tests::proven_complete` test uses to scan the parser
//! source. (`lazuli_doctor` has no central runtime code registry; the `CODE`
//! const co-located with each rule is the source of truth.)

#![forbid(unsafe_code)]
// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
use lazuli_keywords::{ALL, DiagnosticFacet, GLOBAL_DIAGNOSTICS};

/// Every claimed diagnostic facet: the union of every capability's
/// `produces[]` and the cross-cutting [`GLOBAL_DIAGNOSTICS`] bucket.
///
/// This is the "code matrix" the doctor severity-parity test (Wave D4) sources
/// from — closing the loop: the codes the doctor escalates are exactly the
/// codes capabilities declare they produce.
pub fn claimed_facets() -> impl Iterator<Item = &'static DiagnosticFacet> {
    ALL.iter()
        .flat_map(|c| c.produces.iter())
        .chain(GLOBAL_DIAGNOSTICS.iter())
}

/// The bare claimed code strings, in declaration order (capability `produces[]`
/// first, then GLOBAL). May contain duplicates if a code is double-claimed —
/// the `complete` assert is what rejects that.
pub fn claimed_codes() -> impl Iterator<Item = &'static str> {
    claimed_facets().map(|f| f.code)
}
