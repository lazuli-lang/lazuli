//! `xtask` library surface — exposes the registry projectors (tmLanguage
//! keyword rules, keyword reference doc, closed-catalog reference) so
//! integration tests (and `main`) can drive them. See [`tmlanguage`],
//! [`keyword_reference`], and [`catalog_reference`].

// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
pub mod catalog_reference;
pub mod docs_staleness;
pub mod keyword_reference;
pub mod tmlanguage;
