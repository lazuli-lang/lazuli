//! Inline field-constraint parser. Shared between resource fields,
//! command/api input slots, and query params.
//!
//! L0 #3 §10 promoted the constraint vocabulary from a doctor-only
//! grep pass into a typed surface: every `<TypeRef> [decorators...]
//! [required|optional|unique] [<constraint>...] [= <default>] [derived
//! from <expr>]` tail is sliced here and surfaced as a
//! `FieldConstraintsDecl` next to the type text.
//!
//! Closed catalog (see `docs/canonical-semantics.md` §"Constraints"):
//!
//! ```text
//! min <int>
//! max <int>
//! length <int>
//! pattern "<regex>"
//! between <A> and <B>
//! in [<value>, <value>, ...]                     # quoted strings or bare ints
//! validate sanitize_html(<profile>)
//! validate utf8_safe
//! validate max_recursion:<u32>
//! validate max_size:<u64>
//! validator covers_pii[:<sub-tag>]
//! ```
//!
//! Combination-rule enforcement (e.g. min ≤ max, length conflicts with
//! between on integers) lives analyzer-side. The parser only checks
//! shape: integer parsing, unbalanced brackets, missing quoted-string
//! terminators, and duplicate keywords on the same field.
//!
//! Visibility: every cross-cluster consumer (command.rs, query.rs,
//! the resource parser) calls into this module through:
//!
//! - `split_resource_field_after` — full peel of constraints / default
//!   / derived / modifiers, called once per resource field line.
//! - `extract_field_constraints` — just the inline-constraint peel
//!   (constraints only), called from command input and query param
//!   slots which have their own default / modifier parsing.
//!
//! All other helpers stay private; the catalog is fully closed.

mod parsers;

#[cfg(test)]
mod tests;

use super::super::common::{SourceLine, find_token, line_error, line_error_owned};
use super::super::error::ParseError;

use crate::ast::{
    ComputedDateAst, ComputedDateBaseAst, ComputedDateOffsetAst, FieldConstraintsDecl,
};

use parsers::{
    ParsedValidateConstraint, parse_constraint_between, parse_constraint_in_list,
    parse_constraint_int, parse_constraint_string, parse_constraint_validate,
    parse_constraint_validator,
};

include!("mod_p1.rs");
include!("mod_p2.rs");
