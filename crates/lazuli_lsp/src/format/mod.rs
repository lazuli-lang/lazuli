//! Source formatters for `.lzi` / `.lzx` documents.
//!
//! Each sub-module owns one shape of formatter. Today only the
//! canonical-feature formatter lives here; new formatters (e.g. an
//! lzx-projection tidy or a registry alphabetiser) drop in alongside
//! and re-export via `pub(crate) use <name>::*;` in `lib.rs`.

pub(crate) mod canonical;
