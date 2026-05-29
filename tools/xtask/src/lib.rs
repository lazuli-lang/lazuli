//! `xtask` library surface — exposes the registry projectors (tmLanguage
//! keyword rules, keyword reference doc) so integration tests (and `main`)
//! can drive them. See [`tmlanguage`] and [`keyword_reference`].

pub mod keyword_reference;
pub mod tmlanguage;
