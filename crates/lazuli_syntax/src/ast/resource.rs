//! `resource <Name>` declaration AST (Phase L Tier 4c).
//!
//! Resources live inside `domain` at indent 4. Their children at indent
//! 6 cover the full record-shape contract: fields, `has_many`,
//! `previously`, `soft_delete`, `timestamps`, `retention`, `validates`,
//! `lifecycle`, `invariant`, `lock`, `composite_key`, `conventions`,
//! `lifecycle_routes`, `index`, `unique`.
//!
//! Tier 4c lifts the entire canonical-indent surface through
//! `lower_resource_decl`; the legacy brace MVP keeps emitting the same
//! `ir::Resource` so the two surfaces converge.
//!
//! Field constraints (`FieldConstraintsDecl`, L0 #3 §10) are captured
//! as a typed bag at parse time. Combination + default-compat rules
//! live in the analyzer; the parser only records what the author
//! actually wrote.
//!
//! `OwnerAxisAst` mirrors `ir::OwnerAxis` for the
//! `ir-resource-conventions-owner-scope` §7.1 `@owner_axis(through: ...)`
//! field annotation. The decorator is peeled out of the type text by
//! the parser so the analyzer can project directly into
//! `ir::Field.owner_axis`.

use serde::{Deserialize, Serialize};

use super::{DefaultsTenancy, PublicContractDeclAst, Span};

include!("resource_p1.rs");
include!("resource_p2.rs");
