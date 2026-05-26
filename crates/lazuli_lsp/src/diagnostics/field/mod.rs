//! Diagnostics for the `field` / `type` / `extension` family.
//!
//! These producers cover the type-namespace and field-shape contracts
//! that govern resource bodies and extension call sites — the
//! cross-cutting "what does this colon-separated declaration mean"
//! family. They are file-local; cross-feature resolution
//! (extension target existence, derived expression typing) is doctor's
//! job.
//!
//! Sub-concerns:
//!
//! | Module | Concern |
//! |---|---|
//! | [`types`] | `Email`/`Money`/`File`/`Secret` namespace + `query.sql`/`query.view` return-type lookup. |
//! | [`derived`] | `derived from` requiredness/default contract + `has_many` shape. |
//! | [`validation`] | `validates @validator.<name>` canonical form vs legacy `validate ...` / scoped forms. |
//! | [`extensions`] | `extensions.<keyword>` declaration namespace + obsolete `ext.*` references. |
//!
//! Shared helpers (`split_derived_from`, `contains_top_level_eq`,
//! `field_typed_rhs`, `typed_line_type`, `extension_declaration`,
//! `expected_extension_keyword`, `canonical_return_type_name`,
//! `is_builtin_return_type`, `collect_declared_type_names_by_feature`)
//! live in their owning sub-modules and ride the
//! `pub(crate) use diagnostics::field::*;` re-export.

mod derived;
mod extensions;
mod types;
mod validation;

#[allow(unused_imports)]
pub(crate) use derived::{
    contains_top_level_eq, derived_field_diagnostics, field_typed_rhs, has_many_diagnostics,
    split_derived_from,
};
#[allow(unused_imports)]
pub(crate) use extensions::{
    expected_extension_keyword, extension_declaration, extension_declaration_diagnostics,
    extension_reference_diagnostics,
};
#[allow(unused_imports)]
pub(crate) use types::{
    canonical_return_type_name, collect_declared_type_names_by_feature, is_builtin_return_type,
    sql_return_type_diagnostics, type_namespace_diagnostics, typed_line_type,
};
#[allow(unused_imports)]
pub(crate) use validation::validation_syntax_diagnostics;
