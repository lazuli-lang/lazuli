//! Command IR — declarative writes (`command <name>`).
//!
//! A [`Command`] is the lowered shape of one `command <name> { … }` block.
//! It declares one of four [`CommandKind`]s — `create`, `update`, `delete`,
//! or `returns` — plus all the cross-cutting decorators that gate the
//! write: `policy`, `audit`, `approval`, `invalidates`, `external_calls`,
//! `timeout`, `retry`, `idempotency`, `write_window`, `deprecated`,
//! `handler`, `tests`, and lifecycle `triggers`.
//!
//! ## Catalog
//!
//! - [`Command`] — root.
//! - [`CommandKind`] — closed catalog `Create` / `Update` / `Delete` /
//!   `Returns`.
//! - [`CommandEffect`] + [`CreateEffect`] / [`UpdateEffect`] /
//!   [`DeleteEffect`] / [`ReturnsEffect`] — typed effect shapes.
//! - [`Assignment`] — `<field> = <expr>` slot.
//! - [`CommandInput`] + [`TypedSlot`] — short list or typed block.
//! - [`RouteSlot`] + [`RouteSlotKind`] — typed path-param declarations.
//! - [`TargetExpr`] + [`NamedArg`] + [`LetBinding`] — predicate-style
//!   bindings.
//! - [`PolicyRef`] — `@policy.<name>` / `@role.*` / `@scope.*` resolved
//!   reference. Default `None` means feature-level default applies.
//! - [`Deprecation`] + [`DeprecationReplacement`] — typed deprecation
//!   marker.
//! - [`AuditSpec`] — declarative audit policy (subjects + emit_to +
//!   retention + record_before/after).
//! - [`ApprovalSpec`] + [`ApprovalThen`] — Cut A.9 approval block.
//! - [`InvalidatesSpec`] — `invalidates query.<name>(args)` reference.
//! - [`CommandWriteWindow`] — `write_window by <path> within <duration>`.

use serde::{Deserialize, Serialize};

use crate::nodes::async_work::{ExternalCallRef, IdempotencyKey, RetryPolicy};
use crate::nodes::error_vocab::TranslationKeyRef;
use crate::nodes::lifecycle::HandlerRef;
use crate::nodes::plan_and_gate::SynthesizedFromCapFile;
use crate::nodes::query::{Expr, Path};
use crate::nodes::resource::{FieldConstraints, OwnerScopeSql, TypeRef};
use crate::nodes::test_and_policy::TestBlock;
use crate::{PolicyExpr, PublicContract, QualifiedName, RateLimitSpec, SpanRef, is_false};

include!("command_p1.rs");
include!("command_p2.rs");
