//! `xtask` library surface — exposes the registry projectors (tmLanguage
//! keyword rules, keyword reference doc, closed-catalog reference) so
//! integration tests (and `main`) can drive them. See [`tmlanguage`],
//! [`keyword_reference`], and [`catalog_reference`].

pub mod catalog_reference;
pub mod docs_staleness;
pub mod keyword_reference;
pub mod tmlanguage;
