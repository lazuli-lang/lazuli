//! Lowering from `lazuli_syntax` canonical AST slices into `lazuli_ir`.
//!
//! ## Role in the compile pipeline
//!
//! `lazuli_analyzer` sits between `lazuli_syntax` (canonical AST) and
//! `lazuli_ir` (typed lowered shape). Its job is **mechanical
//! projection plus structural validation**: lift the parser's verbatim
//! AST onto the IR shape that downstream consumers (codegen, doctor,
//! LSP, inspect) read. Anything that needs cross-module reasoning
//! lives in `lazuli_cli` (the `expand` pass) or `lazuli_doctor`;
//! anything per-file lives here.
//!
//! ## Submodule layout (R3-E — rails-style refactor)
//!
//! The lowering pipeline is organised into per-concern sibling
//! modules. Each one carries the projection rules for a single
//! "slot" in the vocabulary:
//!
//! ### Cross-cutting primitives
//!
//! * [`helpers`] — pure utility predicates (case conversion, span
//!   bridging, edit-distance, balanced-paren walkers). No AST shape,
//!   no IR shape larger than `SpanRef`. Shared by every slice.
//! * [`expr`] — pure mechanical "text → IR atom" projections
//!   (paths, qualified names, raw exprs, policy atoms, translation
//!   keys). Every other slice calls into this slot.
//! * [`source_map`] — source-position bookkeeping consumed by LSP.
//! * [`symbol_origin`] — origin tagging (handwritten vs synthesized
//!   vs pack-derived) used by inspect and doctor.
//!
//! ### Per-domain lowering (R2 — Wave 4.6)
//!
//! * [`command`] — command effect cluster (`creates|updates|deletes`),
//!   target / let / named-arg / assignment leaves, and the
//!   `invalidates query.<name>` cross-feature reference resolver.
//! * [`workflow`] — async-work leaf lowerings shared by `job`,
//!   `poller`, `webhook`, `tenant_migration`, `channel`,
//!   `notification`, `mcp_server`, `event_group`: retry, fanout,
//!   external-call refs, emit predicates, MCP leaves, digest /
//!   throttle, event-variant fields, job body / trigger.
//! * [`lzx`] — `.lzx` *app layer* (routes, experiences, platform
//!   surfaces). One entry point: `lower_lzx_document`.
//! * [`surface`] — `.lzx` *ViewModel layer* (per-feature audiences +
//!   views + cells + drawers + route params). One entry point:
//!   `lower_surface`.
//!
//! ### Per-domain lowering (R3-E)
//!
//! * [`resource`] — `resource <Foo> { ... }` decl + field-level
//!   lowering (`@cap.PII` extraction, modifier recovery,
//!   inline-validator constraint lift, the four `validate_constraint_*`
//!   gates) + rate-limit literal projection.
//! * [`query`] — `query.list` / `query.lookup` / `query.sql` lowering,
//!   filter line parser (WAR-VOCAB-QUERY-ENUM-01), cache profile
//!   resolution (CL.C.3), and `lower_command_input_to_typed` for
//!   typed query/command input slots.
//! * [`auth`] — `auth { identity | password | sessions | mfa | oauth }`
//!   lowering. The non-trivial bit is `<Resource>.<field>` ->
//!   `FieldRef` splitting; the rest is structural.
//! * [`agent`] — LLM capability lowering: input slots, policy atom,
//!   output projection (text|stream|enum|record-discriminator),
//!   tool reference resolution (Adapter|Local|CrossFeature), eval
//!   case + closed-predicate parser, HTTP expose.
//! * [`design`] — closed-catalog design token lowering (colors,
//!   typography, spaces, radii, shadows, motion, breakpoints,
//!   z-indices, custom). Cheap structural validation per group.
//! * [`plan_gate`] — package-wide `PlanGateFacts` aggregator
//!   (subscription anchor + plan catalog + per-callable gates)
//!   and the six PG diagnostic codes.
//! * [`lifecycle`] — resource lifecycle synthesis hooks.
//! * [`checks`] — public per-file structural checks invoked by
//!   `lazuli_cli` / `lazuli_doctor`. Stays public because external
//!   tools depend on it.
//! * [`rbac`] — RBAC closure construction over a feature's policies.
//!
//! Per-feature orchestration (`lower_feature_skeleton`, jobs / pollers
//! / webhooks / notifications / channels / event groups orchestration,
//! reports, conventions / CRUD synthesis, auto-photo synthesis) still
//! lives in this file. The per-domain leaves above are called from
//! there.
//!
//! ## Vocabulary cross-reference
//!
//! Source AST shapes are defined in `lazuli_syntax::ast` (Wave 4.4).
//! Destination IR shapes are defined in `lazuli_ir` (Wave 4.1). When
//! a lowering function feels like it's "thinking" rather than just
//! "translating", the design pressure belongs upstream (parser
//! enforcement, IR shape change) — not here.
//!
//! ## ABI guarantee
//!
//! Public items historically reachable at `lazuli_analyzer::Foo`
//! remain reachable at the same path. Internal helpers used across
//! sibling modules are `pub(crate)`.

mod agent;
mod auth;
pub mod checks;
mod command;
mod design;
mod expr;
mod helpers;
mod lifecycle;
mod lzx;
mod plan_gate;
mod query;
pub mod rbac;
mod resource;
pub mod source_map;
mod surface;
pub mod symbol_origin;
mod workflow;

pub use agent::lower_agent;
pub use auth::lower_auth;
pub use design::lower_design;
pub use lzx::lower_lzx_document;
pub use plan_gate::{
    PlanGateCode, PlanGateDiagnostic, PlanGateFacts, aggregate_plan_gate_facts,
    diagnose_plan_gate_facts, parse_subscription_anchor,
};
pub use surface::lower_surface;
pub use symbol_origin::build_symbol_origin_index;

use agent::parse_closed_predicate;
use command::{
    lower_command_effect, lower_invalidates_query_ref, lower_let_binding, lower_named_arg,
    lower_target_expr,
};
use expr::{
    lower_path_string, lower_policy_atom, lower_policy_expr, lower_qualified_name,
    lower_translation_key_ref,
};
use query::{lower_cache_profile_decl, lower_query_decl, strip_validate_skip};
use resource::{
    lift_field_constraints, lower_rate_limit_spec, lower_resource_decl, lower_resource_field,
    validate_constraint_combinations, validate_constraint_pattern_compile,
    validate_constraint_range_invariant, validate_constraint_type_compatibility,
};
use workflow::{
    lower_emit_predicate, lower_event_variant_field, lower_external_call, lower_fanout,
    lower_job_body, lower_job_trigger, lower_mcp_prompt, lower_mcp_resource, lower_mcp_tool,
    lower_notification_digest, lower_notification_throttle, lower_retry,
    lower_tenant_migration_target,
};

use helpers::{
    conventions_levenshtein, first_paren_balanced_token, levenshtein, pascal_to_snake,
    quoted_ident, quoted_table, snake_to_pascal, span_of,
};

use lazuli_ir as ir;
use lazuli_syntax as syntax;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("invalid tool reference `{reference}`")]
    InvalidToolRef { reference: String },

    /// Phase L — `auth identity` field reference must split exactly once
    /// into `<Resource>.<field>`. Parser already rejects missing-dot
    /// shapes; this guards downstream lowering against multi-dot or
    /// empty-segment forms slipping through.
    #[error("invalid auth identity `{reference}` — expected `<Resource>.<field>`")]
    InvalidAuthIdentity { reference: String },

    /// Phase L Tier 3 — `webhook verify <scheme>` only accepts `hmac`
    /// today. Adapters that ship other schemes lift through the
    /// registry adapter binding, not the verifier surface.
    #[error("unsupported webhook verify scheme `{scheme}` (use `hmac`)")]
    UnsupportedVerifyScheme { scheme: String },

    /// L0 #8 — `poller` block missing a required child. Surfaces at
    /// lowering when the parser somehow allowed a structurally
    /// incomplete poller through (defense-in-depth; parser already
    /// rejects most). See `docs/proposals/poller-vocab.md` §3.
    #[error("POLLER-MISSING-FIELD: `{kind}` `{name}` is missing required field `{field}`")]
    MissingField {
        kind: String,
        name: String,
        field: String,
    },

    /// L0 #8 — `poller retry backoff <strategy>` outside the closed
    /// catalog (`fixed` | `linear` | `exponential`).
    #[error(
        "POLLER-UNKNOWN-ENUM: `{kind}` carries unknown value `{value}` outside the closed catalog"
    )]
    UnknownEnum { kind: String, value: String },

    /// L0 #2 — `design <X>` declared `extends <Y>`. Cut B (post-pilot).
    /// v0 keeps the keyword reserved at parse time but rejects at
    /// lowering. See `docs/proposals/design-tokens.md` §3.6.
    #[error(
        "DESIGN-EXTENDS-CUT-B: theme inheritance via `extends` ships in Cut B (post-pilot); for v0 declare a standalone `design <X>` block with full token values (got `extends {target}`)"
    )]
    DesignExtendsCutB { target: String },

    /// L0 #3 — view source did not parse as
    /// `<feature>.query.<short>` or
    /// `<feature>.query.{list|lookup|sql}.<short>`.
    #[error(
        "LZX-BAD-QUERY-REF: view `{view}` source `{value}` must be `<feature>.query.<name>` (or `.query.{{list|lookup|sql}}.<name>`)"
    )]
    LzxBadQueryRef { view: String, value: String },

    /// L0 #3 — `submit` or `actions` entry did not parse as a command
    /// reference. Accepts `<feature>.command.<name>` (qualified) or a
    /// bare local short name (`create`).
    #[error(
        "LZX-BAD-COMMAND-REF: command reference `{value}` must be `<feature>.command.<name>` or a bare local short name"
    )]
    LzxBadCommandRef { value: String },

    /// L0 #3 §11 `lzx-cell-slot-orphan` — a `cells <field> @client.<slot>`
    /// binding references a field that isn't in the view's column /
    /// section / fields list. v0 surfaces this at lowering; doctor may
    /// downgrade to a warning.
    #[error(
        "LZX-CELL-SLOT-ORPHAN: view `{view}` cell binding for field `{field}` is not in its columns / sections / fields list"
    )]
    LzxCellSlotOrphan { view: String, field: String },

    /// L0 #3 — the cell slot identifier itself is malformed (empty or
    /// non-kebab/snake characters). Parser-time check; this guards
    /// against direct AST construction.
    #[error(
        "LZX-CELL-SLOT-INVALID: view `{view}` cell slot `{slot}` must be a kebab/snake identifier"
    )]
    LzxCellSlotInvalid { view: String, slot: String },

    /// L0 #3 §11 `lzx-route-param-missing-binding` — a `:name`
    /// placeholder in the `at "<path>"` string has no matching
    /// `route <name>: <Type> from path` declaration.
    #[error(
        "LZX-ROUTE-PARAM-MISSING-BINDING: view `{view}` path placeholder `:{placeholder}` has no `route {placeholder}: <Type> from path` declaration"
    )]
    LzxRouteParamMissingBinding { view: String, placeholder: String },

    /// L0 #3 §11 `lzx-route-param-orphan` — a `route <name>: Type from
    /// path` declaration has no matching `:name` placeholder in the
    /// view's `at "<path>"`.
    #[error(
        "LZX-ROUTE-PARAM-ORPHAN: view `{view}` declared route param `{param}` but the `at` path has no `:{param}` placeholder"
    )]
    LzxRouteParamOrphan { view: String, param: String },

    /// L0 #2 — a `shadow <name> "<value>"` entry carried a top-level
    /// comma, indicating multi-layer composition. Closed v0 grammar
    /// accepts only single-layer shadows; declare separate tokens
    /// (`shadow.elevated_outer`, `shadow.elevated_inner`) and compose
    /// at component level. See `docs/proposals/design-tokens.md` §4.6.
    #[error(
        "DESIGN-SHADOW-MULTI-LAYER: shadow `{name}` is multi-layer (top-level comma); v0 accepts single-layer only — declare separate tokens and compose at component level"
    )]
    DesignShadowMultiLayer { name: String },

    /// L0 #2 — a color hex value did not match `#[0-9a-fA-F]{3,8}`.
    /// Covers all four color-state slots plus flat-form entries.
    #[error(
        "DESIGN-COLOR-HEX-INVALID: color `{token}` state `{state}` carries invalid hex `{value}` (expected `#RGB`, `#RRGGBB`, or `#RRGGBBAA`)"
    )]
    DesignColorHexInvalid {
        token: String,
        state: String,
        value: String,
    },

    /// L0 #2 — a color sub-block named a state outside the closed
    /// catalog `{base, hover, active, foreground}`. Adding new states
    /// requires a Lazuli core proposal (Rule Zero).
    #[error(
        "DESIGN-COLOR-STATE-UNKNOWN: color `{token}` declared unknown state `{state}` (allowed: `base`, `hover`, `active`, `foreground`)"
    )]
    DesignColorStateUnknown { token: String, state: String },

    /// L0 #2 — `typography.weight` value did not parse as a `u16`.
    #[error(
        "DESIGN-WEIGHT-INVALID: typography.weight `{name}` has non-integer value `{value}` (expected 100-1000)"
    )]
    DesignWeightInvalid { name: String, value: String },

    /// L0 #2 — `z <name> <value>` value did not parse as `i32`.
    #[error("DESIGN-Z-INVALID: z token `{name}` has non-integer value `{value}`")]
    DesignZInvalid { name: String, value: String },

    /// L0 #3 §10.2 — conflicting inline field constraints. Per the
    /// proposal: `length` rejects `min`/`max`, `between` rejects
    /// `min`/`max`, and `in [...]` rejects `pattern`. The `combo`
    /// string names the rejected pair (e.g. `length+min`,
    /// `between+max`, `in+pattern`).
    #[error(
        "FIELD-CONSTRAINT-CONFLICT: field `{field}` has incompatible constraints (`{combo}`); see docs/proposals/lzx-integration-codegen.md §10.2"
    )]
    ConstraintConflict { field: String, combo: String },

    /// L0 #3 §10.3 — a `default` value does not satisfy the field's
    /// declared inline constraints. The analyzer accepts the value
    /// verbatim from the parser; here we check it against `min`,
    /// `max`, `length`, `between`, and `in [...]`. `pattern` is
    /// honoured for string defaults too.
    #[error(
        "FIELD-DEFAULT-VIOLATES-CONSTRAINT: field `{field}` default `{value}` violates `{rule}`; see docs/proposals/lzx-integration-codegen.md §10.3"
    )]
    DefaultViolatesConstraint {
        field: String,
        value: String,
        rule: String,
    },

    /// `inline_validator_range_invariant_001` (Wave-B-CL4) — a
    /// numeric bound pair is logically empty: `min N max M` with N>M,
    /// or `between A and B` with A>B. These would produce an
    /// uninhabited domain at runtime; reject at compile time. The
    /// `rule` string identifies which pair (`min>max`, `between`).
    /// `low` / `high` carry the violating literals (verbatim text)
    /// so the error message shows the author what they wrote.
    #[error(
        "INLINE-VALIDATOR-RANGE-INVARIANT: field `{field}` has empty range `{rule}` (`{low}` > `{high}`); swap the bounds or pick one side"
    )]
    InlineValidatorRangeInvariant {
        field: String,
        rule: String,
        low: String,
        high: String,
    },

    /// `inline_validator_type_mismatch_001` (Wave-B-CL4) — a
    /// constraint keyword was applied to a field whose builtin type
    /// is not in §10.1's "Applies to" column. Examples: `pattern` on
    /// `Boolean`, `length` on `Integer`, `between` on `Text`.
    /// `constraint` names the offending keyword (`pattern`, `length`,
    /// `between`); `field_type` echoes the source `type_text` so the
    /// author sees what they typed (vs the resolved BuiltinType,
    /// which is internal vocabulary).
    #[error(
        "INLINE-VALIDATOR-TYPE-MISMATCH: field `{field}: {field_type}` cannot use `{constraint}` (applies to {applies_to} only); see docs/proposals/lzx-integration-codegen.md §10.1"
    )]
    InlineValidatorTypeMismatch {
        field: String,
        field_type: String,
        constraint: String,
        applies_to: String,
    },

    /// `inline_validator_pattern_compile_001` (Wave-B-CL4) — the
    /// `pattern "STRING"` regex failed a structural well-formedness
    /// check at compile time. We do NOT pull in the `regex` crate
    /// (Lazuli analyzer stays regex-free by design — see comment in
    /// `validate_default_against_constraints`). Instead we check
    /// bracket/paren balance and reject the few unambiguous RE2
    /// shape errors (unbalanced `[`, unbalanced `(`, trailing `\`).
    /// Runtime regex compilation in Go/JS is still the authoritative
    /// validator; this just catches the trivial typos at author time.
    #[error(
        "INLINE-VALIDATOR-PATTERN-COMPILE: field `{field}` pattern `{pattern}` is malformed: {reason}"
    )]
    InlineValidatorPatternCompile {
        field: String,
        pattern: String,
        reason: String,
    },

    #[error(
        "INLINE-VALIDATOR-UNKNOWN-SANITIZE-HTML: field `{field}` uses unknown sanitize_html profile `{profile}` (allowed: strict, basic, markdown_safe)"
    )]
    UnknownSanitizeHtmlProfile { field: String, profile: String },

    /// `conventions_unknown` — a `conventions [<ident>]` entry on a
    /// resource named an identifier outside the closed in-core catalog.
    /// The parser (Cell C2) is responsible for actually emitting this
    /// when it sees the bad identifier; Cell C1 wires the variant and
    /// the suggestion helper so the parser has somewhere to land. The
    /// `suggestion` field carries the nearest catalog entry (within
    /// Levenshtein distance ≤ 2) or `None` when no close match exists.
    /// Format via the dedicated `Display` impl below — `thiserror`'s
    /// inline format-string syntax does not support the conditional
    /// "did you mean" suffix we want. See
    /// `docs/proposals/ir-resource-conventions-crud.md` §4.3.
    #[error("{}", format_conventions_unknown(.resource, .identifier, .suggestion.as_deref()))]
    ConventionsUnknown {
        resource: String,
        identifier: String,
        suggestion: Option<String>,
    },

    /// `owner_axis_on_non_fk` — `ir-resource-conventions-owner-scope`
    /// §11.1. The `@owner_axis(through: <ident>)` annotation was applied
    /// to a field whose lowered `TypeRef` is not `UserDefined` (i.e. not
    /// a foreign-key reference to another resource). Primitives, builtins,
    /// tenant-scope columns, and capability-typed fields can't carry an
    /// ownership chain; the synth pass (O2) has no FK target to walk to.
    /// `type_text` echoes the raw decorator-chain text the parser saw so
    /// the author understands what the analyzer rejected.
    #[error(
        "OWNER-AXIS-ON-NON-FK: field `{field}: {type_text}` cannot carry `@owner_axis(...)` — annotation is only valid on FK fields (typed as another resource)"
    )]
    OwnerAxisOnNonFk { field: String, type_text: String },

    /// `@correctness.unknown_invalidate_target` — a command invalidates
    /// a query that is not declared in the resolved target feature.
    /// Emitted during analyzer resolution so codegen never sees a cache
    /// invalidation edge it cannot wire.
    #[error(
        "command '{cmd}' invalidates '{target}', but no such query is declared in feature '{target_feature}'."
    )]
    UnknownInvalidateTarget {
        cmd: String,
        target: String,
        target_feature: String,
    },
}

impl AnalyzeError {
    pub fn diagnostic_code(&self) -> Option<&'static str> {
        match self {
            AnalyzeError::UnknownInvalidateTarget { .. } => {
                Some("@correctness.unknown_invalidate_target")
            }
            _ => None,
        }
    }
}

/// Format helper for `AnalyzeError::ConventionsUnknown`. Kept out of
/// the `thiserror` attribute so the conditional "did you mean" suffix
/// stays readable and testable. When `suggestion` is `Some` the
/// message ends with ` — did you mean \`<sug>\`?`; otherwise the
/// suggestion clause is omitted entirely.
fn format_conventions_unknown(
    resource: &str,
    identifier: &str,
    suggestion: Option<&str>,
) -> String {
    let base = format!(
        "CONVENTIONS-UNKNOWN: resource `{resource}` declared `conventions [{identifier}]` — `{identifier}` is not in the convention catalog (allowed today: `crud`)"
    );
    match suggestion {
        Some(sug) => format!("{base} — did you mean `{sug}`?"),
        None => base,
    }
}

/// ir-resource-conventions-crud Cell C1 — closed catalog of resource
/// convention identifiers accepted by the parser. Grows additively per
/// future proposals (`timestamped`, `pii_aware`, `soft_delete`,
/// `slugged`, `paginated`). `me` was added by cell M1 of
/// `ir-resource-conventions-me.md` §4.3.
///
/// Sorted alphabetically for diff hygiene; keep new entries in order.
pub const CONVENTION_CATALOG: &[&str] = &["crud", "me"];

/// Resolve the closest catalog entry to a misspelled `conventions`
/// identifier using plain Levenshtein distance. Returns the catalog
/// entry when its distance is ≤ 1 (the §4.3 spec — "single-character
/// Levenshtein"). Distance-1 covers the documented case `crd` → `crud`
/// (one insertion) and the obvious neighbours (`crue`, `cru d` etc.);
/// anything further falls through to `None` so the diagnostic doesn't
/// suggest gibberish.
///
/// Reused by the parser (Cell C2) when it sees `conventions [<ident>]`
/// outside the catalog: the parser constructs
/// `AnalyzeError::ConventionsUnknown { suggestion: conventions_unknown_suggestion(ident).map(str::to_owned), ... }`.
pub fn conventions_unknown_suggestion(identifier: &str) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for &candidate in CONVENTION_CATALOG {
        let d = conventions_levenshtein(identifier, candidate);
        if d > 1 {
            continue;
        }
        match best {
            None => best = Some((candidate, d)),
            Some((_, prev_d)) if d < prev_d => best = Some((candidate, d)),
            _ => {}
        }
    }
    best.map(|(name, _)| name)
}

// `lower_lzx_document` + `lower_surface` and the entire `.lzx`
// surface family moved to `lzx.rs` (app layer) and `surface.rs`
// (ViewModel layer).

/// Public wrapper around `type_ref_from_syntax` so the inspect CLI can
/// reuse the analyzer's `@cap.File(...)` typing pass without re-implementing
/// the parser. The bare function stays private for the rest of the crate so
/// future internal callers keep their existing access path.
pub fn type_ref_from_syntax_public(ty: &str) -> ir::TypeRef {
    type_ref_from_syntax(ty)
}

pub(crate) fn type_ref_from_syntax(ty: &str) -> ir::TypeRef {
    let raw = ty.trim();
    // Canonical authoring allows `list of <Type>` (legacy) and `list <Type>`
    // (Wave 0 canonical, per ir-returns-list-2026-05-22). Both lift to the
    // same `TypeRef::Many` shape. The `list of` form stays for back-compat;
    // `list ` is the form codegen + LSP completion will favour going
    // forward (parity with `api.output list of <X>` callsites in
    // atelier/erudito and with `command.returns list <X>` in pilots that
    // commented-out blocks waiting on this lift). Must run before
    // `first_paren_balanced_token` because that helper stops at the
    // first whitespace boundary and would otherwise lower the whole
    // construct as `List`.
    if let Some(inner) = raw
        .strip_prefix("list of ")
        .or_else(|| raw.strip_prefix("List of "))
        .or_else(|| raw.strip_prefix("list "))
        .or_else(|| raw.strip_prefix("List "))
    {
        let inner = type_ref_from_syntax(inner.trim());
        return ir::TypeRef::Many(Box::new(inner));
    }
    // Phase L Tier 4 follow-up — the canonical-indent parser captures
    // the whole post-`:` head as `type_text`, including trailing
    // decorator markers like `@pii.contact` that follow the type but
    // precede modifiers. The legacy text-walker peeled them as
    // "modifiers"; here we take the first paren-balanced token as the
    // actual type and drop the rest. This matches the behaviour of
    // `parse_resource_field` in the retired doctor walker.
    let ty = first_paren_balanced_token(raw);
    // Codegen follow-up (2026-05-12) — `Type[]` array form lifts to
    // `TypeRef::Many(<inner>)` so emitters can render `[]<inner>` in
    // their target language. Before this peel, `returns CustomerLtv[]`
    // landed as flat `UserDefined("CustomerLtv[]")` and codegen
    // sanitised to `CustomerLtv__`. Strip exactly one `[]` suffix
    // and recurse — nested arrays (`[][]`) are unusual but the peel
    // is correct under recursion.
    if let Some(stripped) = ty.strip_suffix("[]") {
        let inner = type_ref_from_syntax(stripped.trim_end());
        return ir::TypeRef::Many(Box::new(inner));
    }
    // Codegen follow-up — `<Type>.ID` member access (route slot
    // syntax `route owner_id: User.ID required`). The IR currently
    // has no member-access carrier on `TypeRef`; pragmatic peel:
    // any `.ID` / `.Id` suffix resolves to `BuiltinType::Id` because
    // every resource carries its identity in the same canonical
    // `lazuli.ID` type. Member access on non-ID fields is rejected
    // (falls through to `UserDefined` with the dotted name; doctor
    // will surface as unresolved).
    if let Some(prefix) = ty.strip_suffix(".ID").or_else(|| ty.strip_suffix(".Id")) {
        if !prefix.is_empty() && !prefix.contains('.') {
            return ir::TypeRef::Builtin(ir::BuiltinType::Id);
        }
    }
    // Phase L Tier 2 — typed `@cap.File(...)` capability.
    if let Some(file) = parse_cap_file_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::File(file));
    }
    if let Some(pii) = parse_cap_pii_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::PII(pii));
    }
    // Phase L Tier 4 follow-up — typed `@cap.Hashed/Encrypted/Token`.
    if let Some(hashed) = parse_cap_hashed_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Hashed(hashed));
    }
    if let Some(encrypted) = parse_cap_encrypted_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Encrypted(encrypted));
    }
    if let Some(e2ee) = parse_cap_e2ee_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::E2ee(e2ee));
    }
    if let Some(token) = parse_cap_token_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Token(token));
    }
    // MONEY-1 §3.2 — `@semantic.Money(currency:<ISO>)` carries the
    // declared currency through to IR so doctor checks
    // (MONEY-COMPARE-001, MONEY-ARITHMETIC-001) can read it without
    // re-walking surface text. Unknown currencies fall through to
    // `UserDefined` so existing "unknown semantic" surfaces them.
    if let Some(money) = parse_semantic_money_type(ty) {
        return ir::TypeRef::Builtin(money);
    }
    // Phase L Tier 4 follow-up — typed `@semantic.*` shorthand for the
    // closed catalog (Email/Phone/Url/Uuid). Other `@semantic.<X>`
    // names still fall through to `UserDefined` so the language can
    // surface "unknown semantic" diagnostics rather than silently
    // accepting them.
    match ty {
        "@semantic.Email" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
        "@semantic.Phone" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticPhone),
        "@semantic.Url" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticUrl),
        "@semantic.Uuid" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticUuid),
        "@semantic.Currency" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticCurrency),
        "@semantic.GeoPoint" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticGeoPoint),
        // Bare `@semantic.Money` (no args) is Hostpoint-pilot reality:
        // single-currency app, defaults to BRL.
        "@semantic.Money" => {
            return ir::TypeRef::Builtin(ir::BuiltinType::SemanticMoney {
                currency: ir::CurrencyCode::BRL,
            });
        }
        _ => {}
    }
    match ty {
        "ID" | "Id" => ir::TypeRef::Builtin(ir::BuiltinType::Id),
        "Text" | "String" => ir::TypeRef::Builtin(ir::BuiltinType::Text),
        "Boolean" | "Bool" => ir::TypeRef::Builtin(ir::BuiltinType::Boolean),
        "Integer" | "Int" => ir::TypeRef::Builtin(ir::BuiltinType::Integer),
        "Decimal" | "Float" => ir::TypeRef::Builtin(ir::BuiltinType::Decimal),
        // Per proposal `semantic-types-money-brazilian.md` v0.3, `Money`
        // is the currency-aware semantic type, NOT a Decimal alias.
        // Default currency is BRL (Hostpoint-pilot reality); authors
        // override per-field via `@semantic.Money(currency:<ISO>)`.
        // Codegen emits `<field>_currency` with a CHECK constraint
        // pinned to the declared currency; doctor lint VOCAB-MONEY-002
        // catches authors who meant Decimal.
        "Money" => ir::TypeRef::Builtin(ir::BuiltinType::SemanticMoney {
            currency: ir::CurrencyCode::BRL,
        }),
        "Date" => ir::TypeRef::Builtin(ir::BuiltinType::Date),
        "DateTime" => ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
        "JSON" | "Json" => ir::TypeRef::Builtin(ir::BuiltinType::Json),
        "Email" => ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
        other => ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: other.to_owned(),
        }),
    }
}

/// Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:<X>)`. Closed
/// catalog `{argon2id, bcrypt}`. Returns `None` if the algorithm is
/// missing or unrecognised so callers fall through to `UserDefined`
/// (LSP surfaces shape errors).
fn parse_cap_hashed_type(ty: &str) -> Option<ir::HashedCapability> {
    let inner = ty.strip_prefix("@cap.Hashed(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let algorithm = match args.get("algorithm")?.as_str() {
        "argon2id" => ir::HashAlgorithm::Argon2id,
        "bcrypt" => ir::HashAlgorithm::Bcrypt,
        _ => return None,
    };
    Some(ir::HashedCapability { algorithm })
}

/// Phase L Tier 4 follow-up — `@cap.Encrypted(key:@key.<scope>)`. Key
/// reference is stored verbatim with its `@key.` prefix.
fn parse_cap_encrypted_type(ty: &str) -> Option<ir::EncryptedCapability> {
    let inner = ty.strip_prefix("@cap.Encrypted(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let key = args.get("key")?.clone();
    if !key.starts_with("@key.") {
        return None;
    }
    Some(ir::EncryptedCapability { key })
}

/// Encryption bucket cycle — `@cap.E2ee(key:@key.<scope>)`. Mirror of
/// `parse_cap_encrypted_type` for end-to-end-encrypted fields that
/// the server stores but never reads.
/// See `docs/proposals/encryption-vocab.md` §Lowering.
fn parse_cap_e2ee_type(ty: &str) -> Option<ir::E2eeCapability> {
    let inner = ty.strip_prefix("@cap.E2ee(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let key = args.get("key")?.clone();
    if !key.starts_with("@key.") {
        return None;
    }
    Some(ir::E2eeCapability { key })
}

/// Phase L Tier 4 follow-up — `@cap.Token(ttl:<dur>,single_use:<bool>,
/// store:<storage>)`. All three dimensions are mandatory; closed
/// catalog `store:{hashed}` and `single_use:{true,false}`.
fn parse_cap_token_type(ty: &str) -> Option<ir::TokenCapability> {
    let inner = ty.strip_prefix("@cap.Token(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let ttl = args.get("ttl")?.clone();
    let single_use = match args.get("single_use")?.as_str() {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    let store = match args.get("store")?.as_str() {
        "hashed" => ir::TokenStore::Hashed,
        _ => return None,
    };
    Some(ir::TokenCapability {
        ttl,
        single_use,
        store,
    })
}

/// Parse `@cap.File(max_size:25mb,accept:text/csv[,visibility:...,signed_ttl:...])`
/// into a typed `FileCapability`. Returns `None` for any malformed shape so
/// the caller falls through to the legacy `UserDefined` fallback — the LSP
/// already surfaces shape errors for the same patterns.
fn parse_cap_file_type(ty: &str) -> Option<ir::FileCapability> {
    let ty = first_paren_balanced_token(ty);
    let inner = ty.strip_prefix("@cap.File(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);

    let max_size = parse_file_size(args.get("max_size")?)?;
    let accept = parse_mime_list(args.get("accept")?)?;
    if accept.is_empty() {
        return None;
    }
    let visibility = args
        .get("visibility")
        .map(|s| s.as_str())
        .and_then(parse_file_visibility);
    let signed_ttl = args.get("signed_ttl").map(|s| s.clone());
    let auto_photo_policy = args.get("auto_photo_policy").cloned();

    Some(ir::FileCapability {
        max_size,
        accept,
        visibility,
        signed_ttl,
        auto_photo_policy,
    })
}

/// IR-VOCAB-REST — `@cap.PII(class:<X>,retention:<dur>,
/// log_redact:<bool>)`. `class` is required; retention and log_redact
/// are optional passive slots for follow-up doctor/runtime cells.
pub(crate) fn parse_cap_pii_type(ty: &str) -> Option<ir::PiiCapability> {
    let ty = first_paren_balanced_token(ty);
    let inner = ty.strip_prefix("@cap.PII(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let class = unquote_capability_arg(args.get("class")?).to_owned();
    if class.is_empty() {
        return None;
    }
    let retention = args
        .get("retention")
        .map(|value| unquote_capability_arg(value).to_owned());
    let log_redact = match args.get("log_redact").map(String::as_str) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        Some(_) => return None,
        None => None,
    };
    Some(ir::PiiCapability {
        class,
        retention,
        log_redact,
    })
}

fn unquote_capability_arg(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

/// MONEY-1 §3.2 — `@semantic.Money(currency:<ISO>)`. Reuses the
/// capability-arg syntax (`key:value`) for consistency with `@cap.*`.
/// Returns `None` when:
///   * the prefix doesn't match `@semantic.Money(`
///   * the closing paren is missing
///   * `currency` is absent
///   * the ISO code isn't in the closed `CurrencyCode` catalog
/// All four cases fall through to the existing `UserDefined`-with-
/// diagnostic path so authors see a single consistent error surface.
fn parse_semantic_money_type(ty: &str) -> Option<ir::BuiltinType> {
    let inner = ty.strip_prefix("@semantic.Money(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let raw = args.get("currency")?;
    let currency = ir::CurrencyCode::from_iso(raw)?;
    Some(ir::BuiltinType::SemanticMoney { currency })
}

fn parse_capability_args(inner: &str) -> std::collections::BTreeMap<String, String> {
    inner
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            part.split_once(':')
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect()
}

fn parse_file_size(raw: &str) -> Option<ir::FileSize> {
    let digit_count = raw.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 || digit_count == raw.len() {
        return None;
    }
    let amount: u32 = raw[..digit_count].parse().ok()?;
    let unit = &raw[digit_count..];
    let literal = match unit {
        "kb" => ir::FileSizeLiteral::Kb(amount),
        "mb" => ir::FileSizeLiteral::Mb(amount),
        "gb" => ir::FileSizeLiteral::Gb(amount),
        _ => return None,
    };
    Some(ir::FileSize {
        bytes: literal.bytes(),
        literal,
    })
}

fn parse_mime_list(raw: &str) -> Option<Vec<ir::MimeType>> {
    let mut out = Vec::new();
    for token in raw.split('|') {
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        let (family, subtype) = token.split_once('/')?;
        let family = family.trim();
        let subtype = subtype.trim();
        if family.is_empty() || subtype.is_empty() {
            return None;
        }
        out.push(ir::MimeType {
            family: family.to_owned(),
            subtype: subtype.to_owned(),
        });
    }
    Some(out)
}

fn parse_file_visibility(raw: &str) -> Option<ir::FileVisibility> {
    match raw {
        "public" => Some(ir::FileVisibility::Public),
        "private" => Some(ir::FileVisibility::Private),
        "signed" => Some(ir::FileVisibility::Signed),
        _ => None,
    }
}

pub(crate) fn parse_default(raw: &str) -> ir::DefaultValue {
    if raw == "true" {
        return ir::DefaultValue::Boolean(true);
    }
    if raw == "false" {
        return ir::DefaultValue::Boolean(false);
    }
    if raw == "nil" {
        return ir::DefaultValue::Nil;
    }
    if let Ok(value) = raw.parse::<i64>() {
        return ir::DefaultValue::Integer(value);
    }

    if raw
        .chars()
        .next()
        .map(|c| c.is_alphabetic() || c == '_')
        .unwrap_or(false)
        && raw.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return ir::DefaultValue::EnumLiteral(ir::EnumLiteral {
            type_name: None,
            variant: raw.to_owned(),
        });
    }

    ir::DefaultValue::String(raw.to_owned())
}

// =============================================================================
// Cut A — agent lowering (canonical-indent slice).
//
// `lower_feature_skeleton(&syntax::FeatureSkeleton)` projects the new
// canonical-indent AST into an `ir::Feature` carrying `agents: Vec<Agent>`.
// Other feature children stay in the legacy pipeline; this function
// returns a `Feature` with zeroed siblings so callers (CLI / LSP / tests)
// can merge it against the legacy lowering result if both pipelines are
// running.
//
// Resolved tool fields (`ToolBinding.resolved_effect`,
// `resolved_policy`, `resolved_pii_classes`) stay `None` here — the
// expand pass in `lazuli_cli` populates them when the full workspace IR
// is loaded (plan §4.3).
//
// See docs/proposals/ai-primitives-v0-implementation.md §4.
// =============================================================================

/// FR-3a — for each resource with a `user: User required unique`
/// field carrying an optional `@cap.File(...)` typed field, append
/// the 4 auto-photo commands + 2 records to the feature.
///
/// Trigger conditions (all must hold):
///   1. Resource has a field named `user` of type `User` required unique.
///   2. The `@cap.File(...)` field is declared `optional`.
///   3. No author-written command in `feature.commands` shares the
///      synthesized name for that field's command role (request,
///      confirm, clear, get_url) — name collision skips THAT one
///      role and emits the other 3.
pub(crate) fn synthesize_auto_photo(feature: &mut ir::Feature) {
    let mut to_add_commands: Vec<ir::Command> = Vec::new();
    let mut to_add_records: Vec<ir::Record> = Vec::new();

    let existing_command_names: std::collections::HashSet<String> =
        feature.commands.iter().map(|c| c.name.clone()).collect();
    let existing_record_names: std::collections::HashSet<String> =
        feature.records.iter().map(|r| r.name.clone()).collect();

    // Resolve the policy to attach. Heuristic per D5: pick the
    // feature-level policy whose name matches `<resource_singular>_only`
    // for the *current* resource; fall back to `authenticated`.
    let policy_name_for = |resource: &str| -> Option<String> {
        let snake = pascal_to_snake(resource);
        let target = format!("{}_only", snake);
        if feature.policies.categories.iter().any(|p| p.name == target) {
            return Some(target);
        }
        let compact_target = format!("{}_only", resource.to_ascii_lowercase());
        if feature
            .policies
            .categories
            .iter()
            .any(|p| p.name == compact_target)
        {
            return Some(compact_target);
        }
        if feature
            .policies
            .categories
            .iter()
            .any(|p| p.name == "authenticated")
        {
            return Some("authenticated".to_owned());
        }
        None
    };

    for resource in &feature.resources {
        let has_user_unique = resource.fields.iter().any(|f| {
            f.name == "user"
                && f.required
                && f.unique
                && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
        });
        if !has_user_unique {
            continue;
        }

        for field in &resource.fields {
            if field.required {
                continue;
            }
            let cap_file = match &field.type_ref {
                ir::TypeRef::Capability(ir::CapabilityRef::File(spec)) => spec.clone(),
                _ => continue,
            };

            let pascal_field = snake_to_pascal(&field.name);
            let intent_name = format!("{}UploadIntent", pascal_field);
            let display_name = format!("{}DisplayUrl", pascal_field);

            // Wave §6 (2026-05-23) — prefer the author's explicit
            // `auto_photo_policy: @policy.<name>` over the
            // resource-singular heuristic. The heuristic produces
            // surprises when the convention name happens to match a
            // policy with a different audience (e.g. a feature with
            // both `host_only` and `host_and_operator` policies); the
            // explicit declaration is the only ground truth.
            //
            // Doctor `CAP-FILE-POLICY-IMPLICIT` flags any `@cap.File`
            // site that didn't declare an explicit policy.
            let policy_name = if let Some(explicit) = cap_file
                .auto_photo_policy
                .as_deref()
                .and_then(|raw| raw.strip_prefix("@policy.").or(Some(raw)))
            {
                explicit.to_owned()
            } else {
                match policy_name_for(&resource.name) {
                    Some(n) => n,
                    None => continue, // no policy => skip whole resource silently
                }
            };

            // 2 records first (idempotent: skip if author already declared).
            if !existing_record_names.contains(&intent_name) {
                to_add_records.push(auto_photo_intent_record(&intent_name));
            }
            if !existing_record_names.contains(&display_name) {
                to_add_records.push(auto_photo_display_record(&display_name));
            }

            // 4 commands. Each role checks for name collision.
            for role in [
                ir::AutoPhotoCommandRole::Request,
                ir::AutoPhotoCommandRole::Confirm,
                ir::AutoPhotoCommandRole::Clear,
                ir::AutoPhotoCommandRole::GetUrl,
            ] {
                let cmd_name = auto_photo_command_name(&field.name, role);
                if existing_command_names.contains(&cmd_name) {
                    continue;
                }
                to_add_commands.push(build_auto_photo_command(
                    cmd_name,
                    &resource.name,
                    &field.name,
                    role,
                    &intent_name,
                    &display_name,
                    &policy_name,
                ));
            }

            // Suppress unused warning on cap_file — adapters read it
            // via the field's TypeRef anyway. Reserved here for
            // future per-site validations (max_size, accept).
            let _ = cap_file;
        }
    }

    feature.commands.extend(to_add_commands);
    feature.records.extend(to_add_records);
}

fn auto_photo_command_name(field: &str, role: ir::AutoPhotoCommandRole) -> String {
    match role {
        ir::AutoPhotoCommandRole::Request => format!("request_{}_upload", field),
        ir::AutoPhotoCommandRole::Confirm => format!("confirm_{}_upload", field),
        ir::AutoPhotoCommandRole::Clear => format!("clear_{}", field),
        ir::AutoPhotoCommandRole::GetUrl => format!("get_{}_url", field),
    }
}

fn auto_photo_intent_record(name: &str) -> ir::Record {
    ir::Record {
        name: name.to_owned(),
        public_contract: None,
        fields: vec![
            simple_required_field("url", builtin_text()),
            simple_required_field("method", builtin_text()),
            simple_required_field("headers_content_type", builtin_text()),
            simple_required_field("key", builtin_text()),
            simple_required_field("expires_at", builtin_datetime()),
        ],
        discriminator_field: None,
        span_ref: None,
    }
}

fn auto_photo_display_record(name: &str) -> ir::Record {
    ir::Record {
        name: name.to_owned(),
        public_contract: None,
        fields: vec![
            simple_optional_field("url", builtin_text()),
            simple_optional_field("expires_at", builtin_datetime()),
        ],
        discriminator_field: None,
        span_ref: None,
    }
}

pub(crate) fn build_auto_photo_command(
    name: String,
    resource: &str,
    field: &str,
    role: ir::AutoPhotoCommandRole,
    intent_name: &str,
    display_name: &str,
    policy_name: &str,
) -> ir::Command {
    use ir::*;
    let (input, effect, rate_limit) = match role {
        AutoPhotoCommandRole::Request => (
            CommandInput::Typed(vec![
                TypedSlot {
                    name: "content_type".to_owned(),
                    type_ref: builtin_text(),
                    required: true,
                    constraints: FieldConstraints::default(),
                    validate_skip: false,
                },
                TypedSlot {
                    name: "size_bytes".to_owned(),
                    type_ref: builtin_integer(),
                    required: true,
                    constraints: FieldConstraints::default(),
                    validate_skip: false,
                },
            ]),
            CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: intent_name.to_owned(),
                }),
            }),
            "30 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::Confirm => (
            CommandInput::Typed(vec![TypedSlot {
                name: "key".to_owned(),
                type_ref: builtin_text(),
                required: true,
                constraints: FieldConstraints::default(),
                validate_skip: false,
            }]),
            CommandEffect::None,
            "30 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::Clear => (
            CommandInput::Empty,
            CommandEffect::None,
            "10 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::GetUrl => (
            CommandInput::Empty,
            CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: display_name.to_owned(),
                }),
            }),
            "60 per 10 minutes per ip",
        ),
    };

    let _ = (resource, field); // currently only used via marker
    Command {
        name,
        public_contract: None,
        kind: CommandKind::Returns,
        route: Vec::new(),
        input,
        target: None,
        lets: Vec::new(),
        effect,
        policy: PolicyRef::Local(policy_name.to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: Some(RateLimitSpec::from_default(rate_limit.to_owned())),
        audit: Some(AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        }),
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: Some(SynthesizedFromCapFile {
            resource: resource.to_owned(),
            field: field.to_owned(),
            role,
        }),
        owner_scope_sql: None,
    }
}

fn simple_required_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    simple_field(name, type_ref, true)
}

fn simple_optional_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    simple_field(name, type_ref, false)
}

fn simple_field(name: &str, type_ref: ir::TypeRef, required: bool) -> ir::Field {
    ir::Field {
        name: name.to_owned(),
        type_ref,
        required,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        constraints: ir::FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        span_ref: None,
    }
}

fn builtin_text() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::Text)
}

fn builtin_integer() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::Integer)
}

fn builtin_datetime() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::DateTime)
}

// =============================================================================
// `conventions [crud]` auto-synthesis pass
// =============================================================================
//
// Spec: `docs/proposals/ir-resource-conventions-crud.md` §5.
//
// For each `Resource` with `ConventionRef::Crud` in `conventions`, the
// pass appends 5 entries to the feature (3 commands + 2 queries) using
// the shapes from §5.2 through §5.6. Override semantics (§6): any name
// already authored in the feature is left alone — no warning, no
// `@deprecated`, no doctor flag. The other 4 still synthesize.
//
// RULE-VOCAB-03 (§7) — zero workflow: each synth maps to exactly one
// of the existing declarative IR shapes (`CommandEffect::Creates` /
// `Updates` / `Deletes`, `Query::Lookup`, `Query::List`). No new
// lowering path is introduced; the pass just produces IR nodes the
// existing emitters already know how to lower to one SQL each.
//
// Diagnostics (§11) returned via the `Vec<CrudSynthDiagnostic>` return
// value — Cell C4 wires them to the user-facing doctor surface.

/// §11 diagnostic codes emitted by `synthesize_conventions`. Cell C4
/// formats these into user-facing strings; Cell C3 just records them.
///
/// Originally `CrudSynthDiagnostic`; extended to cover both `crud` and
/// `me` bundles when M2 of `ir-resource-conventions-me` landed
/// (variants prefixed `Me*`). A type alias preserves the legacy name
/// for any callers; the canonical name going forward is
/// `ConventionSynthDiagnostic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConventionSynthDiagnostic {
    /// `crud_synth_policy_not_found` — feature has no `authenticated`
    /// policy. Carries the resource name for the suggestion. Also fires
    /// for `me_synth_policy_not_found` (the `me` bundle reuses
    /// `authenticated` per `ir-resource-conventions-me.md` §5.4); Cell
    /// M3 selects the user-visible code by reading
    /// `resource.conventions`.
    PolicyNotFound { resource: String },
    /// `crud_synth_no_required_fields` — every required field is in the
    /// Tenant or Auto group, so `create_<resource>.input` would be
    /// empty. Likely an authoring mistake. Crud-only.
    NoRequiredFields { resource: String },
    /// `@correctness.crud_synth_author_signature_mismatch` — author wrote a same-named
    /// command/query but its input field list or return type diverges
    /// from the canonical convention shape. Carries the resource +
    /// synth name + a short reason for Cell C4 to format. Crud-only.
    SignatureMismatch {
        resource: String,
        synth_name: String,
        reason: String,
    },
    /// `me_synth_no_actor_resolution` — resource declared
    /// `conventions [me]` but has neither `user: User required` nor
    /// `org: Org required` AND is not itself named `User`. The synth
    /// has no key to filter on. See
    /// `ir-resource-conventions-me.md` §11.1 (named
    /// `me_synth_no_owner_axis` in the proposal; M2's diagnostic key
    /// is `me_synth_no_actor_resolution` per the cell brief — same
    /// condition, more explicit wording).
    MeNoActorResolution { resource: String },
    /// `me_synth_signature_mismatch` — author wrote
    /// `query lookup_my_<resource>` (or the declarative
    /// `query.lookup my_<resource>`) whose return shape diverges
    /// from the canonical `me` synth (route-less Lookup query
    /// returning the resource row).
    MeSignatureMismatch {
        resource: String,
        synth_name: String,
        reason: String,
    },
    /// `owner_axis_unknown_through` — `@owner_axis(through: <col>)`
    /// names a column that doesn't exist on the FK target resource.
    /// O3 formats the user-facing message with a nearest-name hint.
    /// See `ir-resource-conventions-owner-scope.md` §7.4 + §11.1.
    OwnerAxisUnknownThrough {
        resource: String,
        field: String,
        through: String,
        fk_target: String,
        suggestion: Option<String>,
    },
    /// `owner_axis_through_not_user_keyed` — the FK target's `through:`
    /// column is not typed as `User` (or `@semantic.UserID`). The
    /// emitted chain can't resolve to `ctx.User.ID`. O3 surfaces this
    /// as a warning. See §7.4 + §11.1.
    OwnerAxisThroughNotUserKeyed {
        resource: String,
        field: String,
        through: String,
        fk_target: String,
    },
    /// `owner_axis_collides_with_unique_user` — the resource carries
    /// BOTH `user: User required unique` AND
    /// `@owner_axis(through: <col>)` on another field. The two scopes
    /// would compose redundantly; the unique-user mode already
    /// provides ownership. See §7.4 + §11.1.
    OwnerAxisCollidesWithUniqueUser { resource: String, field: String },
}

/// Legacy alias preserved during the M2 rename. Downstream code should
/// migrate to `ConventionSynthDiagnostic` over time; M3 carries the
/// final downstream migration into doctor / inspect.
pub type CrudSynthDiagnostic = ConventionSynthDiagnostic;

impl ConventionSynthDiagnostic {
    pub fn diagnostic_code(&self) -> &'static str {
        match self {
            ConventionSynthDiagnostic::SignatureMismatch { .. } => {
                "@correctness.crud_synth_author_signature_mismatch"
            }
            ConventionSynthDiagnostic::PolicyNotFound { .. } => "crud_synth_policy_not_found",
            ConventionSynthDiagnostic::NoRequiredFields { .. } => "crud_synth_no_required_fields",
            ConventionSynthDiagnostic::MeNoActorResolution { .. } => "me_synth_no_actor_resolution",
            ConventionSynthDiagnostic::MeSignatureMismatch { .. } => "me_synth_signature_mismatch",
            ConventionSynthDiagnostic::OwnerAxisUnknownThrough { .. } => {
                "owner_axis_unknown_through"
            }
            ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed { .. } => {
                "owner_axis_through_not_user_keyed"
            }
            ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser { .. } => {
                "owner_axis_collides_with_unique_user"
            }
        }
    }

    pub fn severity(&self) -> &'static str {
        match self {
            ConventionSynthDiagnostic::SignatureMismatch { .. } => "warning",
            _ => "error",
        }
    }
}

/// Run the `conventions [...]` auto-synthesis pass on a feature.
/// Today covers two bundles in catalog order: `crud` (5 entries) and
/// `me` (1 entry — `lookup_my_<resource>`). Returns diagnostics from
/// crud §11 + me §11; Cell C4 / M3 wires the user-facing rendering.
/// Public so doctor / tests can call it directly.
///
/// **RULE-VOCAB-03 (crud §7 + me §7) — zero workflow:** every `if`/
/// `match` in this function is **authoring-time** dispatch — it
/// selects which IR node shape to emit. The emitted IR nodes contain
/// zero control flow; downstream codegen lowers each to one fixed
/// SQL per crud §7 / me §7.
pub fn synthesize_conventions(feature: &mut ir::Feature) -> Vec<CrudSynthDiagnostic> {
    let mut diagnostics: Vec<CrudSynthDiagnostic> = Vec::new();
    let mut to_add_commands: Vec<ir::Command> = Vec::new();
    let mut to_add_queries: Vec<ir::Query> = Vec::new();
    // §11 inspect surface (Cell C4 / M3) — Feature.synth_origins
    // records every name in a convention's set: `Synthesized(<bundle>)`
    // for names the pass appended; `AuthorOverride(<bundle>)` for names
    // the author wrote (synth skipped per crud §6 / me §6). Inspect
    // uses these markers to render `[conv:<bundle>]` /
    // `[author override; convention skipped]`.
    let mut synth_origins_inserts: Vec<(String, ir::ConventionOrigin)> = Vec::new();

    let existing_command_names: std::collections::HashSet<String> =
        feature.commands.iter().map(|c| c.name.clone()).collect();
    let existing_query_names: std::collections::HashSet<String> = feature
        .queries
        .iter()
        .map(|q| q.name().to_owned())
        .collect();

    // §5.8 — default policy is the feature's `authenticated` policy.
    let has_authenticated = feature
        .policies
        .categories
        .iter()
        .any(|p| p.name == "authenticated");

    for resource in &feature.resources {
        // Per-bundle dispatch — each resource may declare zero, one,
        // or both bundles in `conventions [...]`. Bundle blocks are
        // independent; the override-collision logic (`existing_*`
        // sets) is shared. crud §6.1 / me §6.1: zero name collisions
        // by construction because `crud` owns `lookup_<r>` while `me`
        // owns `lookup_my_<r>`.
        let has_crud = resource.conventions.contains(&ir::ConventionRef::Crud);
        let has_me = resource.conventions.contains(&ir::ConventionRef::Me);
        if !has_crud && !has_me {
            continue;
        }

        // owner-scope §7.3 — resolve once per resource so the crud and
        // me blocks share one decision. Composability §5.3 / §6.1:
        // one annotation drives mode for every bundle that synths
        // against the resource. Diagnostics (§11.1) are pushed
        // regardless of which bundles are active (they're a property
        // of the resource shape, not of the bundle).
        let owner_scope = resolve_owner_scope(feature, resource, &mut diagnostics);

        // ===== `crud` bundle (§5) — gated; runs only when declared. =====
        if has_crud {
            // §5.8 — guard: policy `authenticated` must exist.
            if !has_authenticated {
                diagnostics.push(CrudSynthDiagnostic::PolicyNotFound {
                    resource: resource.name.clone(),
                });
                // We still synthesize with `PolicyRef::Local("authenticated")`
                // even though it's unresolved — Cell C4 will surface the
                // diagnostic; the IR shape stays uniform. This mirrors the
                // FR-3a auto-photo precedent (which returns silently when
                // no policy is found; here we surface a typed diagnostic
                // instead).
            }

            let categorised = categorize_fields(resource);

            // §11 `crud_synth_no_required_fields` — `create.input` would be
            // empty if every required-on-resource field is Tenant or Auto.
            // Detect by looking at the create-input list.
            let create_input_fields = categorised.create_input_fields();
            if create_input_fields.is_empty() {
                diagnostics.push(CrudSynthDiagnostic::NoRequiredFields {
                    resource: resource.name.clone(),
                });
            }

            let resource_snake = pascal_to_snake(&resource.name);

            // §5.1 — the 5 synth names, in canonical order.
            let create_name = format!("create_{}", resource_snake);
            let update_name = format!("update_{}", resource_snake);
            let delete_name = format!("delete_{}", resource_snake);
            let lookup_name = format!("lookup_{}", resource_snake);
            let list_name = format!("list_{}s", resource_snake);

            // §6 — per-name override. If the author wrote the same name we
            // skip *just that name* with no warning, unless the author's
            // signature diverges from the canonical shape — that lands the
            // `crud_synth_author_signature_mismatch` diagnostic (§11 / §9).
            //
            // The `if existing_*.contains(...)` checks below are
            // authoring-time controls (which synth to add), NOT lowering
            // control flow over the emitted IR — RULE-VOCAB-03 (§7) is
            // preserved.

            // 1) create_<resource>
            if existing_command_names.contains(&create_name) {
                if let Some(reason) = check_command_signature_mismatch(
                    feature,
                    &create_name,
                    &create_input_fields,
                    CanonicalReturn::CreatesResource(&resource.name),
                ) {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: create_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    create_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                let mut cmd =
                    build_create_command(&create_name, &resource.name, &create_input_fields);
                // §8.5.A — owner-scope create-side CTE-INSERT. The CREATE
                // synth carries the *full* OwnerScopeSql (cte_owner_check
                // populated) so codegen can paste the CTE prefix in front
                // of the INSERT. Tenant-only resources keep
                // `owner_scope_sql: None` and emit the same shape as
                // before this cell.
                if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                    cmd.owner_scope_sql = Some(scope.clone());
                }
                cmd.invalidates =
                    synth_crud_invalidates(&lookup_name, &list_name, has_me, &resource_snake);
                to_add_commands.push(cmd);
                synth_origins_inserts.push((
                    create_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 2) update_<resource>
            if existing_command_names.contains(&update_name) {
                let canonical_update_inputs = categorised.update_input_fields();
                if let Some(reason) = check_command_signature_mismatch(
                    feature,
                    &update_name,
                    &canonical_update_inputs,
                    CanonicalReturn::UpdatesResource(&resource.name),
                ) {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: update_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    update_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                let mut cmd = build_update_command(
                    &update_name,
                    &resource.name,
                    &categorised.update_input_fields(),
                );
                // §8.2 — owner-scope WHERE on UPDATE. The carrier carries
                // ONLY the `where_predicate`; codegen drops the
                // `cte_owner_check` (None here, since UPDATE doesn't need
                // the CTE wrapper). We share the resolution by cloning;
                // codegen reads only what it needs per shape.
                if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                    cmd.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
                cmd.invalidates =
                    synth_crud_invalidates(&lookup_name, &list_name, has_me, &resource_snake);
                to_add_commands.push(cmd);
                synth_origins_inserts.push((
                    update_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 3) delete_<resource>
            if existing_command_names.contains(&delete_name) {
                if let Some(reason) = check_command_signature_mismatch(
                    feature,
                    &delete_name,
                    &[],
                    CanonicalReturn::DeletesResource(&resource.name),
                ) {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: delete_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    delete_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                let mut cmd = build_delete_command(&delete_name, &resource.name);
                // §8.1 — owner-scope WHERE on DELETE. Same shape as the
                // pre-absorption hand-rolled handler in §1.1 trigger
                // evidence. CTE not used on DELETE; only the predicate.
                if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                    cmd.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
                cmd.invalidates =
                    synth_crud_invalidates(&lookup_name, &list_name, has_me, &resource_snake);
                to_add_commands.push(cmd);
                synth_origins_inserts.push((
                    delete_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 4) lookup_<resource>
            let mut canonical_lookup = build_lookup_query(&lookup_name, &resource.name);
            // §8.3 — owner-scope WHERE on LOOKUP. The Lookup query's
            // canonical keys (id = $1) get extended with the chain
            // predicate emitted by codegen via `owner_scope_sql`.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                if let ir::Query::Lookup(lq) = &mut canonical_lookup {
                    lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
            }
            if existing_query_names.contains(&lookup_name) {
                if let Some(reason) =
                    check_query_signature_mismatch(feature, &lookup_name, &canonical_lookup)
                {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: lookup_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    lookup_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                to_add_queries.push(canonical_lookup);
                synth_origins_inserts.push((
                    lookup_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 5) list_<resource>s
            let mut canonical_list = build_list_query(&list_name, &resource.name);
            // §8.4 — owner-scope WHERE on LIST. Same predicate; the
            // synth's pagination shape is unaffected.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                if let ir::Query::List(lq) = &mut canonical_list {
                    lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
            }
            if existing_query_names.contains(&list_name) {
                if let Some(reason) =
                    check_query_signature_mismatch(feature, &list_name, &canonical_list)
                {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: list_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    list_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                to_add_queries.push(canonical_list);
                synth_origins_inserts.push((
                    list_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }
        } // ===== end `crud` bundle =====

        // ===== `me` bundle (me §5) — singleton-per-actor lookup. =====
        //
        // Authoring-time mode classification (me §5.3). The synth picks
        // ONE of four shapes from the resource's static structure; the
        // emitted IR node contains zero branches (me §7 / RULE-VOCAB-03).
        if has_me {
            // me §5.4 — default policy is `authenticated`. Reuses the
            // crud policy probe; a missing policy emits the diagnostic
            // (no _Me suffix on the variant — `PolicyNotFound` covers
            // both bundles since the policy slot has the same name).
            if !has_authenticated {
                // Only emit once per resource even if both bundles
                // declared `me` and `crud`; the crud block above will
                // have already pushed `PolicyNotFound` if it ran.
                // Dedupe by inspecting `diagnostics` for an existing
                // entry on this resource.
                let already_emitted = diagnostics.iter().any(|d| {
                    matches!(
                        d,
                        ConventionSynthDiagnostic::PolicyNotFound { resource: r }
                            if r == &resource.name
                    )
                });
                if !already_emitted {
                    diagnostics.push(ConventionSynthDiagnostic::PolicyNotFound {
                        resource: resource.name.clone(),
                    });
                }
            }

            let resource_snake = pascal_to_snake(&resource.name);
            let lookup_my_name = format!("lookup_my_{}", resource_snake);

            // me §5.3 — classify the resource's actor axis. Four-mode
            // closed table; `None` triggers `me_synth_no_actor_resolution`.
            // The classification is a STATIC truth table over resource
            // shape; no runtime branching is introduced into the
            // emitted IR.
            let mode = classify_me_mode(resource);

            match mode {
                Some(m) => {
                    // me §6 — per-name override. Author wrote
                    // `lookup_my_<resource>` (or the `query.lookup
                    // my_<resource>` declarative form, which lowers to
                    // the same IR `Query::Lookup` name).
                    if existing_query_names.contains(&lookup_my_name) {
                        if let Some(reason) = check_me_lookup_signature_mismatch(
                            feature,
                            &lookup_my_name,
                            &resource.name,
                        ) {
                            diagnostics.push(ConventionSynthDiagnostic::MeSignatureMismatch {
                                resource: resource.name.clone(),
                                synth_name: lookup_my_name.clone(),
                                reason,
                            });
                        }
                        synth_origins_inserts.push((
                            lookup_my_name.clone(),
                            ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me),
                        ));
                    } else {
                        let mut q = build_lookup_my_query(&lookup_my_name, &resource.name, m);
                        // §6.1 composition — `[crud, me]` + `@owner_axis`
                        // composes uniformly: the `me` synth also reads
                        // the resource-level annotation and appends the
                        // chain predicate. The unique-user variant is
                        // mutually exclusive with `@owner_axis` per
                        // §11.1 collision check, so this path only
                        // attaches scope when the resource is NOT
                        // user-keyed and the resolution succeeded.
                        if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                            if let ir::Query::Lookup(lq) = &mut q {
                                lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                                    cte_owner_check: None,
                                    ..scope.clone()
                                });
                            }
                        }
                        to_add_queries.push(q);
                        synth_origins_inserts.push((
                            lookup_my_name.clone(),
                            ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me),
                        ));
                    }
                }
                None => {
                    // me §11.1 — no actor axis. Resource has no `user`
                    // field, no `org` field, and is not itself the
                    // `User` resource. The synth has no key to filter
                    // on; emit diagnostic, skip synth.
                    diagnostics.push(ConventionSynthDiagnostic::MeNoActorResolution {
                        resource: resource.name.clone(),
                    });
                }
            }
        } // ===== end `me` bundle =====
    }

    feature.commands.extend(to_add_commands);
    feature.queries.extend(to_add_queries);
    feature.synth_origins.extend(synth_origins_inserts);
    diagnostics
}

// =============================================================================
// `conventions [me]` synthesis helpers — Cell M2
//
// Spec: `docs/proposals/ir-resource-conventions-me.md` §§5.3, 5.5, 5.6.
//
// `classify_me_mode` is the entire decision surface — a 4-row truth
// table over resource shape, evaluated once at synth time. Once a mode
// is picked, `build_lookup_my_query` emits ONE fixed `Query::Lookup`
// shape with a mode-specific `keys` vector (the WHERE-clause builder).
// **The emitted IR contains zero branches** — RULE-VOCAB-03 (me §7).
// =============================================================================

/// me §5.3 — the four key-resolution modes. Classification is static;
/// each variant carries no runtime state. The selected variant uniquely
/// determines the `KeyClause` vector emitted into the synthesized
/// `Query::Lookup` (me §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeMode {
    /// Resource has `user: User required` (with or without `unique`)
    /// AND an `org`-bearing field. `WHERE org_id = ctx.User.OrgID AND
    /// "user" = ctx.User.ID`.
    UserKeyed,
    /// Resource has `user: User required` AND no `org` field.
    /// `WHERE "user" = ctx.User.ID`.
    UserKeyedNoOrg,
    /// Resource has `org: Org required` AND no `user: User required`.
    /// `WHERE org_id = ctx.User.OrgID`.
    OrgKeyed,
    /// Resource IS the User table (name == "User"). `WHERE id = ctx.User.ID`.
    SelfKeyed,
}

/// me §5.3 — classify the resource's actor axis. Pure inspection of the
/// resource's field list + name. Returns `None` only when the resource
/// has neither `user` nor `org` and is not named `User`, triggering
/// `me_synth_no_actor_resolution`.
///
/// **RULE-VOCAB-03 affirmation**: this function is the entire
/// authoring-time decision surface for the `me` bundle. Its `if`/`match`
/// statements pick which IR shape the *synth pass* emits; the emitted
/// IR contains exactly one fixed `Query::Lookup` per call site, with
/// no branches in the runtime lowering path.
fn classify_me_mode(resource: &ir::Resource) -> Option<MeMode> {
    // me §5.3 row 4 — `self_keyed`: the resource IS the User table.
    // Checked first because a resource literally named `User` could in
    // principle declare its own `user` self-reference field; the
    // self-keyed shape (`WHERE id = ctx.User.ID`) is the correct one.
    if resource.name == "User" {
        return Some(MeMode::SelfKeyed);
    }

    let has_user_required = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });
    let has_org_field = resource.fields.iter().any(|f| {
        f.name == "org" && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "Org")
    });

    // me §5.3 rows 1 and 2 — `user_keyed` variants.
    if has_user_required {
        if has_org_field {
            return Some(MeMode::UserKeyed);
        }
        return Some(MeMode::UserKeyedNoOrg);
    }

    // me §5.3 row 3 — `org_keyed` (org-singleton resource).
    let has_org_required = resource.fields.iter().any(|f| {
        f.name == "org"
            && f.required
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "Org")
    });
    if has_org_required {
        return Some(MeMode::OrgKeyed);
    }

    // me §5.3 row 5 — no key. Diagnostic.
    None
}

/// me §5.2 — build the `lookup_my_<resource>` `Query::Lookup` IR. The
/// `keys` vector is the WHERE-clause builder; its shape is fixed by
/// `mode` at synth time (me §7 / RULE-VOCAB-03). The emitted IR carries
/// no `params` (route-less per me §5.2) and no `filters`.
///
/// Path-on-the-right-hand-side uses `ctx.User.*` paths, mirroring the
/// already-proven IR shape used by hand-authored
/// `query.lookup ... filters` blocks (e.g.,
/// `traveler.lzi:79-83` references `ctx.actor.user_id`; the IR-level
/// `KeyClause.equals` carries an `Expr::Path` per `Path::from_segments`).
pub(crate) fn build_lookup_my_query(name: &str, resource: &str, mode: MeMode) -> ir::Query {
    let _ = resource; // reserved for future signature-mismatch detail
    // Runtime `readCtx` (runtime/go/lazuli/handle.go:893) accepts only
    // canonical snake-case ctx paths: `actor.user_id` / `actor.org_id`.
    // PascalCase variants (`ctx.User.OrgID`) fall through to default
    // and return 500 "unknown ctx path". Emit the canonical segments
    // matching the existing commands' FromCtx convention.
    let keys: Vec<ir::KeyClause> = match mode {
        // §5.3 user_keyed: WHERE org_id = ctx.actor.org_id AND "user" = ctx.actor.user_id
        MeMode::UserKeyed => vec![
            ir::KeyClause {
                path: ir::Path::from_segments(["org".to_owned()]),
                equals: ir::Expr::Path(ir::Path::from_segments([
                    "ctx".to_owned(),
                    "actor".to_owned(),
                    "org_id".to_owned(),
                ])),
            },
            ir::KeyClause {
                path: ir::Path::from_segments(["user".to_owned()]),
                equals: ir::Expr::Path(ir::Path::from_segments([
                    "ctx".to_owned(),
                    "actor".to_owned(),
                    "user_id".to_owned(),
                ])),
            },
        ],
        // §5.3 user_keyed_no_org: WHERE "user" = ctx.actor.user_id
        MeMode::UserKeyedNoOrg => vec![ir::KeyClause {
            path: ir::Path::from_segments(["user".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "user_id".to_owned(),
            ])),
        }],
        // §5.3 org_keyed: WHERE org_id = ctx.actor.org_id
        MeMode::OrgKeyed => vec![ir::KeyClause {
            path: ir::Path::from_segments(["org".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "org_id".to_owned(),
            ])),
        }],
        // §5.3 self_keyed: WHERE id = ctx.actor.user_id
        MeMode::SelfKeyed => vec![ir::KeyClause {
            path: ir::Path::from_segments(["id".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "user_id".to_owned(),
            ])),
        }],
    };

    ir::Query::Lookup(ir::LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        // me §5.2 — NO route, NO params. The actor IS the input.
        params: Vec::new(),
        keys,
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        // me §5.4 — default policy is `authenticated`.
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// me §11.1 — `me_synth_signature_mismatch` trigger. Compares an
/// author-written `lookup_my_<resource>` query to the canonical shape.
/// Returns `None` when the signatures match.
///
/// The canonical `me` synth produces a `Query::Lookup` (route-less,
/// returning the resource row). Mismatches the author can introduce:
/// - Wrong query kind (`Query::List` or `Query::Sql` under the same
///   name). The `me` bundle owns the `lookup_my_*` name prefix.
/// - Author-supplied `params` (the canonical shape is parameter-less).
fn check_me_lookup_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    resource: &str,
) -> Option<String> {
    let _ = resource; // reserved for future richer diff messages.
    let query = feature.queries.iter().find(|q| q.name() == name)?;

    match query {
        ir::Query::Lookup(lq) => {
            // me §5.2 — canonical shape is parameter-less. An author
            // who introduces `params` diverges from the canonical
            // route-less actor-keyed shape.
            if !lq.params.is_empty() {
                return Some(format!(
                    "author-written `{}` declares params; canonical `me` shape is route-less + parameter-less",
                    name
                ));
            }
            None
        }
        // §11.1 mismatch — `lookup_my_<r>` should be a Lookup query.
        _ => Some(format!(
            "author-written `{}` is not a `query.lookup`; canonical `me` shape is route-less Lookup",
            name
        )),
    }
}

/// §5.7 field categorisation result. Each field on a resource lands in
/// exactly one group; `create_input_fields` and `update_input_fields`
/// project the Required + Optional groups into the canonical input lists
/// per §5.2 / §5.3.
struct CategorisedFields<'a> {
    required: Vec<&'a ir::Field>,
    optional: Vec<&'a ir::Field>,
}

impl<'a> CategorisedFields<'a> {
    /// §5.2 — `create.input` shape: required fields stay required,
    /// optional fields stay optional. Order matches resource field order.
    fn create_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, true));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }

    /// §5.3 — `update.input` shape: every non-immutable field becomes
    /// optional regardless of its required-on-resource flag. Field-by-
    /// field optional update — fields omitted from input are not touched.
    fn update_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, false));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }
}

/// §5.7 — split a resource's fields into Tenant / Auto / Required /
/// Optional groups. Only Required + Optional are returned (Tenant and
/// Auto have no presence in the synth input lists).
fn categorize_fields(resource: &ir::Resource) -> CategorisedFields<'_> {
    // Detect the `user: User required unique` shape per §5.7.
    let has_user_unique = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && f.unique
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });

    // Discriminator field is in the Auto group when lifecycle is set.
    let lifecycle_discriminator: Option<&str> = resource
        .lifecycle
        .as_ref()
        .map(|lc| lc.discriminator_field.as_str());

    let mut required: Vec<&ir::Field> = Vec::new();
    let mut optional: Vec<&ir::Field> = Vec::new();

    for field in &resource.fields {
        // Tenant group.
        let is_tenant = field.name == "org" || (has_user_unique && field.name == "user");
        // Auto group.
        let is_auto = matches!(field.name.as_str(), "id" | "created_at" | "updated_at")
            || lifecycle_discriminator.is_some_and(|d| d == field.name);

        if is_tenant || is_auto {
            continue;
        }

        if field.required {
            required.push(field);
        } else {
            optional.push(field);
        }
    }

    CategorisedFields { required, optional }
}

// =============================================================================
// `@owner_axis(through: <col>)` synth-pass extension — Cell O2.
//
// Spec: `docs/proposals/ir-resource-conventions-owner-scope.md`
// §7.3 (`build_where_clause` extension), §8 (auto-synth worked
// examples), §8.5.A (CTE-INSERT for create-side verification),
// §9 (override semantics), §11.1 (3 new doctor codes).
//
// **RULE-VOCAB-03 (§7 + §8.6)**: each shape composed here lowers to
// exactly ONE SQL statement. The CTE-INSERT (§8.5.A) is a single
// CTE-wrapped INSERT — Postgres evaluates the CTE either yields a
// row and the INSERT fires once, or yields zero rows and the INSERT
// fires zero times. No procedural sequencing; no runtime branching;
// no two-roundtrip check-then-insert.
// =============================================================================

/// §7.3 — resolution result for the owner-scope synth lookup. Returned
/// by `resolve_owner_scope`, consumed by the crud + me synth blocks.
///
/// - `Scoped(...)`: at least one `@owner_axis` field resolved cleanly;
///   the analyzer should emit the WHERE / CTE fragments for downstream
///   codegen consumption.
/// - `Tenant`: no `@owner_axis` annotation present — fall back to the
///   pre-existing tenant-only synth (today's default).
/// - `Diagnostic(...)`: an `@owner_axis` annotation was found but
///   doesn't resolve cleanly — surface the diagnostic and skip
///   owner-scope emission for the offending field. Other fields on
///   the same resource may still resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OwnerScopeResolution {
    Scoped(ir::OwnerScopeSql),
    Tenant,
}

/// §7.3 — resolve a resource's `@owner_axis` annotations into an
/// emittable `OwnerScopeSql` carrier, OR emit diagnostics for the 3
/// new doctor codes (§11.1) when the annotation can't resolve.
///
/// The function visits every field; the *first* cleanly-resolving
/// `@owner_axis` wins for the synth's WHERE-clause (the pilot has
/// exactly one owner-axis per resource — Property's `host`,
/// Service's `host`, CustomServiceCategory's `host`). Multi-axis
/// composition is deferred per §13.
///
/// Diagnostics are pushed into `diagnostics_out` for the caller; the
/// return value indicates whether the synth should still emit
/// owner-scope IR (yes for `Scoped`, no for `Tenant` — which also
/// covers "diagnostic emitted, fell back to tenant-only").
fn resolve_owner_scope(
    feature: &ir::Feature,
    resource: &ir::Resource,
    diagnostics_out: &mut Vec<ConventionSynthDiagnostic>,
) -> OwnerScopeResolution {
    // §7.4 / §11.1 `owner_axis_collides_with_unique_user` — resource
    // carries BOTH the user-keyed shape (`user: User required unique`)
    // AND an `@owner_axis(through: ...)` on another field. The two
    // scopes would compose redundantly; the unique-user mode already
    // restricts to `WHERE "user" = ctx.User.ID`.
    let has_user_unique = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && f.unique
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });

    let mut emitted_collision_diag = false;
    let mut chosen: Option<ir::OwnerScopeSql> = None;

    for field in &resource.fields {
        let Some(axis) = field.owner_axis.as_ref() else {
            continue;
        };

        // §11.1 — collision check: declarative `user-keyed` mode
        // already provides ownership; surface a warning and skip the
        // owner-axis emission to avoid double-restriction. We emit
        // the diagnostic once per resource even if multiple fields
        // collide (rare; the spec describes "the resource has BOTH").
        if has_user_unique {
            if !emitted_collision_diag {
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                });
                emitted_collision_diag = true;
            }
            continue;
        }

        // The annotated field must be a UserDefined FK to another
        // resource. Primitive-field misuse is `owner_axis_on_non_fk`
        // and lives in O1's parser-time surface (§7.4); the analyzer
        // re-checks defensively so a hand-constructed IR fixture is
        // still surfaced (otherwise this code path would silently
        // skip the annotation).
        let ir::TypeRef::UserDefined(fk_qname) = &field.type_ref else {
            // Out-of-scope for O2 — O1 owns this diagnostic. Skip
            // silently rather than double-emit; downstream check
            // catches it.
            continue;
        };
        let fk_target = fk_qname.name.clone();

        // §11.1 `owner_axis_unknown_through` — the `through:` column
        // doesn't exist on the FK target resource. Resolve the FK
        // target in the feature's resource list.
        //
        // Cross-feature note: the FK target may live in another
        // feature (Hostpoint's catalog.Property → host.Host is the
        // motivating case). Synth runs per-feature without a Module
        // handle, so we can only validate the through-column when the
        // target is in the SAME feature. For cross-feature targets we
        // skip the diagnostic checks and trust the @owner_axis
        // annotation — the doctor pass (which has Module context)
        // surfaces missing-FK-target / wrong-through-type errors at a
        // higher layer. The SQL composition below only needs
        // `fk_target` (name) and `axis.through_column` (column name)
        // verbatim from the annotation; it does NOT need fk_resource
        // to exist locally.
        let fk_resource = feature.resources.iter().find(|r| r.name == fk_target);

        if let Some(fk_resource) = fk_resource {
            let through_field = fk_resource
                .fields
                .iter()
                .find(|f| f.name == axis.through_column);
            let Some(through_field) = through_field else {
                let suggestion = nearest_field_name(&axis.through_column, &fk_resource.fields);
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisUnknownThrough {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    through: axis.through_column.clone(),
                    fk_target: fk_target.clone(),
                    suggestion,
                });
                continue;
            };

            // §11.1 `owner_axis_through_not_user_keyed` — the resolved
            // `through:` column must be typed as `User` (a UserDefined
            // ref to the User resource). Other actor types
            // (`@semantic.UserID` etc.) are deferred per §13.
            let is_user_keyed = matches!(
                &through_field.type_ref,
                ir::TypeRef::UserDefined(q) if q.name == "User"
            );
            if !is_user_keyed {
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    through: axis.through_column.clone(),
                    fk_target: fk_target.clone(),
                });
                // Warning, not error per §11.1 — still emit the chain so
                // codegen can produce SQL the author can hand-correct.
            }
        }
        // else: cross-feature FK target — skip per-field validation,
        // trust annotation, compose SQL below.

        // §7.3 / §8.1-8.4 — compose the WHERE predicate fragment.
        // Shape per §1.1 trigger evidence: literal Postgres
        // `<fk_col> IN (SELECT id FROM "<fk_table>" WHERE "<through>" = ctx.User.ID)`.
        // Single statement; the IN-subquery is a semi-join in the
        // planner (§8.6). The `ctx.User.ID` literal is a
        // codegen-substituted placeholder — downstream codegen
        // resolves to `$N` per its parameter-binding policy.
        let where_predicate = format!(
            "{fk_col} IN (SELECT id FROM {fk_table} WHERE {through} = ctx.User.ID)",
            fk_col = field.name,
            fk_table = quoted_table(&fk_target),
            through = quoted_ident(&axis.through_column),
        );

        // §8.5.A — CTE prefix for `create_<resource>`. The CREATE
        // synth pastes this in front of its INSERT; the INSERT then
        // selects FROM the CTE so a zero-row CTE yields a zero-row
        // INSERT (the synth surfaces a `not_owner` envelope via
        // existing RowsAffected==0 handling in `delete_*` per §8.7,
        // mirrored on create-side). One SQL statement total.
        let cte_owner_check = Some(format!(
            "WITH owner_check AS (SELECT 1 FROM {fk_table} WHERE id = ${fk_col} AND {through} = ctx.User.ID)",
            fk_col = field.name,
            fk_table = quoted_table(&fk_target),
            through = quoted_ident(&axis.through_column),
        ));

        if chosen.is_none() {
            chosen = Some(ir::OwnerScopeSql {
                field_name: field.name.clone(),
                fk_target,
                through_column: axis.through_column.clone(),
                where_predicate,
                cte_owner_check,
            });
        }
        // Multi-axis composition (multiple `@owner_axis` on one
        // resource) is deferred per §13. We take the first.
    }

    match chosen {
        Some(scope) => OwnerScopeResolution::Scoped(scope),
        None => OwnerScopeResolution::Tenant,
    }
}

/// §11.1 `owner_axis_unknown_through` — produce a nearest-name
/// suggestion from the FK target's field list. Returns `None` when
/// the closest candidate is not similar enough to be useful (Levenshtein
/// distance > half the input length — same threshold used by the
/// pre-existing nearest-string suggestions elsewhere in the doctor).
fn nearest_field_name(target: &str, fields: &[ir::Field]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for f in fields {
        let dist = levenshtein(target, &f.name);
        match best {
            Some((b, _)) if dist >= b => {}
            _ => best = Some((dist, f.name.as_str())),
        }
    }
    let (dist, name) = best?;
    if dist <= target.len().max(1) / 2 + 1 {
        Some(name.to_owned())
    } else {
        None
    }
}

/// `pub` re-export of the §7.3 WHERE-clause builder for direct test
/// access. Tests assert on the emitted SQL string; downstream codegen
/// pulls the same string off `Command.owner_scope_sql.where_predicate`.
///
/// **Direct call form** (no diagnostic surface). Used in tests that
/// construct a synthetic `Field` + `Resource` and want to round-trip
/// the SQL without running the whole `synthesize_conventions` pass.
/// For real synth, use `resolve_owner_scope` via `synthesize_conventions`.
#[doc(hidden)]
pub fn build_owner_scope_where_for_test(
    fk_col: &str,
    fk_target_resource: &str,
    through_column: &str,
) -> String {
    format!(
        "{fk_col} IN (SELECT id FROM {fk_table} WHERE {through} = ctx.User.ID)",
        fk_col = fk_col,
        fk_table = quoted_table(fk_target_resource),
        through = quoted_ident(through_column),
    )
}

/// §8.5.A — `pub` re-export of the CTE-INSERT prefix builder for
/// direct test access. Same role as `build_owner_scope_where_for_test`.
#[doc(hidden)]
pub fn build_owner_scope_cte_prefix_for_test(
    fk_col: &str,
    fk_target_resource: &str,
    through_column: &str,
) -> String {
    format!(
        "WITH owner_check AS (SELECT 1 FROM {fk_table} WHERE id = ${fk_col} AND {through} = ctx.User.ID)",
        fk_col = fk_col,
        fk_table = quoted_table(fk_target_resource),
        through = quoted_ident(through_column),
    )
}

/// Canonical return shape for `crud_synth_signature_mismatch` (§9 / §11).
/// The carried resource name is read by `check_command_signature_mismatch`
/// to compare against the author's effect target; the query variants
/// are matched only on kind today and reserve the name for Cell C4 if
/// it needs a richer diff message.
#[allow(dead_code)]
enum CanonicalReturn<'a> {
    CreatesResource(&'a str),
    UpdatesResource(&'a str),
    DeletesResource(&'a str),
    ReturnsResource(&'a str),
    ReturnsResourceMany(&'a str),
}

/// §11 `crud_synth_author_signature_mismatch` trigger — compare an authored
/// command to its canonical convention shape and return a reason string
/// when the input field list OR the effect/return type diverges.
/// Returns `None` when the signatures match (no diagnostic). Cell C4
/// formats `reason` into the user-facing message.
fn check_command_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    canonical_inputs: &[(&ir::Field, bool)],
    canonical_return: CanonicalReturn<'_>,
) -> Option<String> {
    let cmd = feature.commands.iter().find(|c| c.name == name)?;

    // Compare effect kind.
    let effect_matches = match (&cmd.effect, &canonical_return) {
        (ir::CommandEffect::Creates(e), CanonicalReturn::CreatesResource(name)) => {
            e.resource.name == *name
        }
        (ir::CommandEffect::Updates(e), CanonicalReturn::UpdatesResource(name)) => {
            e.resource.name == *name
        }
        (ir::CommandEffect::Deletes(e), CanonicalReturn::DeletesResource(name)) => {
            e.resource.name == *name
        }
        _ => false,
    };
    if !effect_matches {
        return Some(format!(
            "effect / target resource diverges from canonical shape for `{}`",
            name
        ));
    }

    // Compare input field names. Order-insensitive set check is enough
    // here — Cell C4 may surface a richer diff.
    let canonical_names: std::collections::BTreeSet<String> = canonical_inputs
        .iter()
        .map(|(f, _)| f.name.clone())
        .collect();
    let author_names: std::collections::BTreeSet<String> = match &cmd.input {
        ir::CommandInput::Short(names) => names.iter().cloned().collect(),
        ir::CommandInput::Typed(slots) => slots.iter().map(|s| s.name.clone()).collect(),
        ir::CommandInput::Empty => std::collections::BTreeSet::new(),
    };
    if author_names != canonical_names {
        return Some(format!(
            "input field list diverges from canonical shape for `{}`",
            name
        ));
    }

    None
}

/// §11 `crud_synth_author_signature_mismatch` trigger for queries. Returns a
/// reason string when the author-written query diverges from the exact
/// canonical query shape the `crud` bundle would have emitted.
fn check_query_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    canonical_query: &ir::Query,
) -> Option<String> {
    let query = feature.queries.iter().find(|q| q.name() == name)?;

    match (query, canonical_query) {
        (ir::Query::Lookup(author), ir::Query::Lookup(canonical)) => {
            if author.params != canonical.params {
                return Some(format!(
                    "query params diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.keys != canonical.keys {
                return Some(format!(
                    "lookup keys diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.filters != canonical.filters {
                return Some(format!(
                    "query filters diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.policy != canonical.policy || author.policy_expr != canonical.policy_expr {
                return Some(format!(
                    "query policy diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.owner_scope_sql != canonical.owner_scope_sql {
                return Some(format!(
                    "owner-scope query shape diverges from canonical shape for `{}`",
                    name
                ));
            }
            None
        }
        (ir::Query::List(author), ir::Query::List(canonical)) => {
            if author.params != canonical.params {
                return Some(format!(
                    "query params diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.filters != canonical.filters {
                return Some(format!(
                    "query filters diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.order != canonical.order {
                return Some(format!(
                    "query order diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.paginate != canonical.paginate {
                return Some(format!(
                    "pagination diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.policy != canonical.policy || author.policy_expr != canonical.policy_expr {
                return Some(format!(
                    "query policy diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.owner_scope_sql != canonical.owner_scope_sql {
                return Some(format!(
                    "owner-scope query shape diverges from canonical shape for `{}`",
                    name
                ));
            }
            None
        }
        _ => Some(format!(
            "query kind / return shape diverges from canonical for `{}`",
            name
        )),
    }
}

/// §5.2 — build `create_<resource>` command IR.
pub(crate) fn build_create_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Create,
        route: Vec::new(),
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Creates(ir::CreateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            from_input: true,
            // §5.2 — one `<field> = input.<field>` assignment per input
            // slot so the codegen emits a populated `lazuli.Bindings{}`
            // body. Without this, the synthesized INSERT had no columns
            // to bind and tripped runtime panics at first call. The
            // emitter checks `TypedSlot.required` to decide between
            // `FromInput` (required) and `FromInputOptional` (optional,
            // skip-on-nil so column defaults apply).
            assignments: input_field_assignments(input_fields),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.3 — build `update_<resource>` command IR.
pub(crate) fn build_update_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Update,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Updates(ir::UpdateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            // §5.3 — every input slot becomes a `<field> = input.<field>`
            // assignment. `update_input_fields` marks all of them
            // optional, so the codegen emits `FromInputOptional` and
            // the runtime skips columns whose input pointer was nil —
            // i.e. fields the wire payload didn't include stay
            // untouched (partial-update semantics, §5.3 third para).
            assignments: input_field_assignments(input_fields),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// Build the canonical `invalidates` list for a synth `create_<R>` /
/// `update_<R>` / `delete_<R>` command. Without this list, clients
/// (TS `useLazuliCommand`) never refresh `lookup_<R>` and
/// `list_<R>s` after a mutation — the cached query result is shown
/// until next manual reload. The 2026-05-22 hostpoint settings save
/// outage surfaced exactly this: after the partial-update bug was
/// fixed, users still saw stale data on re-entering the panel
/// because every synth command shipped with `invalidates: []`.
///
/// When the resource also declares `conventions [me]`, the `me`
/// bundle's `lookup_my_<R>` query is appended too — it shares the
/// same row set, just keyed off the actor instead of the route id.
fn synth_crud_invalidates(
    lookup_name: &str,
    list_name: &str,
    has_me: bool,
    resource_snake: &str,
) -> Vec<ir::InvalidatesSpec> {
    let mut out = vec![
        ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: lookup_name.to_owned(),
            },
            args: Vec::new(),
        },
        ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: list_name.to_owned(),
            },
            args: Vec::new(),
        },
    ];
    if has_me {
        out.push(ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: format!("lookup_my_{}", resource_snake),
            },
            args: Vec::new(),
        });
    }
    out
}

/// Build one `<field> = input.<field>` assignment per input slot.
/// Used by both `build_create_command` and `build_update_command` so the
/// codegen has a populated `Bindings` body to emit. The emitter inspects
/// `TypedSlot.required` on the command's input to pick between
/// `FromInput` (required slot) and `FromInputOptional` (optional slot,
/// skip-on-nil at runtime).
fn input_field_assignments(input_fields: &[(&ir::Field, bool)]) -> Vec<ir::Assignment> {
    input_fields
        .iter()
        .map(|(f, _required)| ir::Assignment {
            field: f.name.clone(),
            value: ir::Expr::Path(ir::Path::from_segments([
                "input".to_owned(),
                f.name.clone(),
            ])),
        })
        .collect()
}

/// §5.4 — build `delete_<resource>` command IR.
pub(crate) fn build_delete_command(name: &str, resource: &str) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Delete,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Deletes(ir::DeleteEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.5 — build `lookup_<resource>` query IR.
pub(crate) fn build_lookup_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::Lookup(ir::LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        params: Vec::new(),
        keys: vec![ir::KeyClause {
            path: ir::Path::from_segments(["id".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments(["id".to_owned()])),
        }],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// §5.6 — build `list_<resource>s` query IR.
pub(crate) fn build_list_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::List(ir::ListQuery {
        name: name.to_owned(),
        public_contract: None,
        params: vec![
            ir::TypedSlot {
                name: "limit".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
            ir::TypedSlot {
                name: "offset".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
        ],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        // §5.6 default limit 50.
        paginate: Some(50),
        modifier: None,
        cache: None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// Project a `(field, required)` list into a typed `CommandInput`.
fn input_to_command_input(fields: &[(&ir::Field, bool)]) -> ir::CommandInput {
    if fields.is_empty() {
        return ir::CommandInput::Empty;
    }
    let slots: Vec<ir::TypedSlot> = fields
        .iter()
        .map(|(f, required)| ir::TypedSlot {
            name: f.name.clone(),
            type_ref: f.type_ref.clone(),
            required: *required,
            constraints: f.constraints.clone(),
            validate_skip: false,
        })
        .collect();
    ir::CommandInput::Typed(slots)
}

/// Common command-shape defaults applied to every synthesized CRUD
/// command. `policy authenticated`, `audit default`, `rate_limit` set
/// by the caller (write vs read uses different limits per §5.9).
fn default_synth_command(rate_limit: &str) -> ir::Command {
    ir::Command {
        name: String::new(),
        public_contract: None,
        kind: ir::CommandKind::Returns,
        route: Vec::new(),
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: Some(ir::RateLimitSpec::from_default(rate_limit.to_owned())),
        audit: Some(ir::AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        }),
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        // owner-scope §7.3 — left `None` here; the synth pass mutates
        // each synthesized command to attach the resolved scope (see
        // `synthesize_conventions`). The default keeps tenant-only
        // shape stable for command IR not produced by the synth pass.
        owner_scope_sql: None,
    }
}

/// §5.9 — create / update / delete share `rate_limit "100 per 10 minutes per ip"`.
fn crud_write_rate_limit() -> &'static str {
    "100 per 10 minutes per ip"
}

/// Lower a canonical-indent feature skeleton into an `ir::Feature`.
pub fn lower_feature_skeleton(
    skeleton: &syntax::FeatureSkeleton,
) -> Result<ir::Feature, AnalyzeError> {
    let mut agents = Vec::with_capacity(skeleton.agents.len());
    for agent_ast in &skeleton.agents {
        agents.push(lower_agent(&skeleton.name, agent_ast)?);
    }
    let auth = match &skeleton.auth {
        Some(auth_ast) => Some(lower_auth(auth_ast)?),
        None => None,
    };
    let mut jobs = Vec::with_capacity(skeleton.jobs.len());
    for job_ast in &skeleton.jobs {
        jobs.push(lower_job(&skeleton.name, job_ast)?);
    }
    let mut webhooks = Vec::with_capacity(skeleton.webhooks.len());
    for webhook_ast in &skeleton.webhooks {
        webhooks.push(lower_webhook(webhook_ast)?);
    }
    let mut notifications = Vec::with_capacity(skeleton.notifications.len());
    for notification_ast in &skeleton.notifications {
        notifications.push(lower_notification(&skeleton.name, notification_ast)?);
    }
    let mut pollers = Vec::with_capacity(skeleton.pollers.len());
    for poller_ast in &skeleton.pollers {
        pollers.push(lower_poller(poller_ast)?);
    }
    let mut event_groups = Vec::with_capacity(skeleton.event_groups.len());
    for group_ast in &skeleton.event_groups {
        event_groups.push(lower_event_group(group_ast));
    }
    let mut tenant_migrations = Vec::with_capacity(skeleton.tenant_migrations.len());
    for tm_ast in &skeleton.tenant_migrations {
        tenant_migrations.push(lower_tenant_migration(tm_ast)?);
    }
    let defaults = match &skeleton.defaults {
        Some(d) => lower_defaults(d),
        None => ir::Defaults::default(),
    };
    let commands = skeleton
        .commands
        .iter()
        .map(|command| lower_command_decl(&skeleton.name, command))
        .collect::<Result<Vec<_>, _>>()?;
    let apis = skeleton.apis.iter().map(lower_api_decl).collect();
    let resources = skeleton
        .resources
        .iter()
        .map(lower_resource_decl)
        .collect::<Result<Vec<_>, _>>()?;
    let queries = skeleton
        .queries
        .iter()
        .map(|q| lower_query_decl(&skeleton.name, q, &skeleton.caches))
        .collect::<Result<Vec<_>, _>>()?;
    let records = skeleton
        .records
        .iter()
        .map(lower_record_decl)
        .collect::<Result<Vec<_>, _>>()?;
    let policies = skeleton
        .policies
        .as_ref()
        .map(lower_policies_decl)
        .unwrap_or_default();
    let enums = skeleton.enums.iter().map(lower_enum_decl).collect();
    let reports = skeleton
        .reports
        .iter()
        .map(|r| lower_report_decl(&skeleton.name, r))
        .collect::<Result<Vec<_>, _>>()?;
    // CL.C.4 — lower `aggregate <Name>` blocks from the surface AST.
    let aggregates = skeleton
        .aggregates
        .iter()
        .map(lower_aggregate_decl)
        .collect::<Vec<_>>();
    // MCP bucket cycle — lower `mcp_server <name>` blocks. Lowering is
    // value-preserving except for the closed-catalog `transport` mapping
    // (rejects unknown literals with a typed error).
    let mcp_servers: Vec<ir::MCPServerSpec> = skeleton
        .mcp_servers
        .iter()
        .map(lower_mcp_server)
        .collect::<Result<Vec<_>, _>>()?;
    // Cross-feature contracts §5.4 — lift the feature-level
    // `uses <feature>[, ...]+ [version v<N>]` clauses into parallel
    // `uses` / `uses_spans` / `uses_versions` lists. Each clause from a
    // single `uses` line becomes one entry in each parallel vector.
    let uses: Vec<String> = skeleton
        .uses_clauses
        .iter()
        .map(|c| c.feature.clone())
        .collect();
    let uses_spans: Vec<ir::SpanRef> = skeleton
        .uses_clauses
        .iter()
        .map(|c| span_of(c.span))
        .collect();
    let uses_versions: Vec<Option<u16>> = skeleton.uses_clauses.iter().map(|c| c.version).collect();

    // Iron-hand context vocabulary — lower the surface AST into IR
    // shapes. `purpose` is stored as the raw quoted-string text (empty
    // strings preserved so the lint can fire). `non_goals` are flat
    // strings on the surface; we map each into `NonGoal { key,
    // description }` with `key = ""` (the IR carries a richer shape for
    // future delegated_to / out_of_scope partitioning, but the
    // wire-thin grammar only authors descriptions today). `attach_ctx`
    // becomes the verbatim path; resolution + content-length check
    // happens in `VOCAB-CONTEXT-CTXMD-001`.
    let purpose = skeleton.purpose.as_ref().map(|p| p.text.clone());
    let non_goals = skeleton
        .non_goals
        .as_ref()
        .map(|block| {
            block
                .entries
                .iter()
                .map(|description| ir::NonGoal {
                    key: String::new(),
                    description: description.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let context_path = skeleton.attach_ctx.as_ref().map(|c| c.path.clone());

    let mut feature = ir::Feature {
        name: skeleton.name.clone(),
        purpose,
        non_goals,
        context_path,
        defaults,
        uses,
        uses_spans,
        uses_versions,
        requirements: Vec::new(),
        enums,
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies,
        // IR Error-Vocab (Cell PARSE-1) — lower the optional `errors`
        // block onto the typed IR slot. Pre-vocab fixtures (no `errors`
        // block) keep `None`; codegen treats `None` identically to a
        // block with no overrides.
        errors: skeleton.errors.as_ref().map(lower_feature_errors_decl),
        commands,
        apis,
        records,
        queries,
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs,
        webhooks,
        notifications,
        event_groups,
        tenant_migrations,
        translation: skeleton.translation.as_ref().map(lower_translation_decl),
        pollers,
        auth,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents,
        reports,
        channels: skeleton.channels.iter().map(lower_channel).collect(),
        caches: skeleton
            .caches
            .iter()
            .map(lower_cache_profile_decl)
            .collect(),
        aggregates,
        mcp_servers,
        previous_names: Vec::new(),
        // Cell C4 (inlined): empty until C3's synthesis pass populates the
        // map per `docs/proposals/ir-resource-conventions-crud.md` §11.
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: Some(span_of(skeleton.span)),
    };
    lifecycle::lower_lifecycles(&mut feature, &skeleton.resources);
    synthesize_auto_photo(&mut feature);
    // ir-resource-conventions-crud §5 — synthesize 3 commands + 2
    // queries per resource that opts into `conventions [crud]`. The
    // bridge to populate `Feature.synth_origins` (so the inspect
    // surface from Cell C4 can annotate `[conv:crud]`) is wired in
    // `synthesize_conventions` itself. Diagnostics returned here are
    // currently dropped; the bridge cycle wires them through to
    // doctor per §11.
    let _ = synthesize_conventions(&mut feature);
    Ok(feature)
}

/// CL.C.4 — lower an `AggregateDecl` from the surface AST into
/// `ir::Aggregate`. Resource references stay unqualified `QualifiedName`
/// (feature `None`); doctor resolves them against the surrounding
/// feature's resource list.
pub(crate) fn lower_aggregate_decl(decl: &syntax::AggregateDecl) -> ir::Aggregate {
    ir::Aggregate {
        name: decl.name.clone(),
        root: ir::QualifiedName {
            feature: None,
            name: decl.root.clone(),
        },
        contains: decl
            .contains
            .iter()
            .map(|m| ir::QualifiedName {
                feature: None,
                name: m.clone(),
            })
            .collect(),
        invariants: decl.invariants.iter().map(lower_invariant_decl).collect(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// CL.C.4 — lower an `InvariantDecl` (shared by aggregate-scoped and
/// resource-scoped sites) into `ir::Invariant`. The `when` expression
/// is run through the closed-predicate parser used by agent `evals`
/// (`parse_closed_predicate`); when the shape isn't recognized the
/// `EvalPredicate::Unparsed(text)` variant carries the verbatim source
/// so doctor can echo it on failure.
pub(crate) fn lower_invariant_decl(decl: &syntax::InvariantDecl) -> ir::Invariant {
    ir::Invariant {
        name: decl.name.clone(),
        when: parse_closed_predicate(&decl.when),
        message: decl.message.clone(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// Phase L Tier 4d — lower a canonical-indent `record` block into
/// `ir::Record`.
pub(crate) fn lower_record_decl(r: &syntax::RecordDecl) -> Result<ir::Record, AnalyzeError> {
    let fields = r
        .fields
        .iter()
        .map(lower_resource_field)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ir::Record {
        name: r.name.clone(),
        public_contract: lower_public_contract(&r.public_contract),
        fields,
        discriminator_field: r.discriminator_field.clone(),
        span_ref: Some(span_of(r.span)),
    })
}

/// Phase L Tier 4 follow-up — lower a canonical-indent `policies` block
/// into `ir::Policies`. The AST mirrors the IR shape 1:1 so this is a
/// structural copy: category atoms and per-resource field overrides
/// project directly. Closed-catalog validation lives in doctor.
pub(crate) fn lower_policies_decl(decl: &syntax::PoliciesDecl) -> ir::Policies {
    let categories = decl
        .categories
        .iter()
        .map(|c| ir::PolicyCategory {
            name: c.name.clone(),
            atoms: c.atoms.clone(),
            previous_names: Vec::new(),
            // IR Error-Vocab (Cell PARSE-1) — lower the optional
            // `when_denied @translation.<key>` child onto the typed IR
            // slot. Same-feature scope; cross-feature key resolution
            // lives in doctor (`translation_key_unknown` + ERR-VOCAB-002).
            when_denied: c.when_denied.as_ref().map(lower_translation_key_ref),
            when_denied_route: c.when_denied_route.as_ref().map(lower_when_denied_route),
        })
        .collect();
    let fields = decl
        .fields
        .iter()
        .map(|f| ir::FieldPolicies {
            resource: lower_qualified_name(&f.resource),
            fields: f
                .fields
                .iter()
                .map(|fp| ir::FieldPolicy {
                    field: fp.field.clone(),
                    read: fp.read.clone(),
                    write: fp.write.clone(),
                    previous_names: Vec::new(),
                })
                .collect(),
        })
        .collect();
    ir::Policies {
        categories,
        fields,
        span_ref: Some(span_of(decl.span)),
    }
}

pub(crate) fn lower_when_denied_route(route: &syntax::WhenDeniedRouteAst) -> ir::WhenDeniedRoute {
    ir::WhenDeniedRoute {
        unauthenticated: route
            .unauthenticated
            .as_ref()
            .map(lower_route_redirect_target),
        role_mismatch: route
            .role_mismatch
            .iter()
            .map(|arm| ir::RoleMismatchArm {
                role: arm.role.clone(),
                target: lower_route_redirect_target(&arm.target),
                span_ref: Some(span_of(arm.span)),
            })
            .collect(),
        default: route.default.as_ref().map(lower_route_redirect_target),
        span_ref: Some(span_of(route.span)),
    }
}

pub(crate) fn lower_route_redirect_target(
    target: &syntax::RouteRedirectTargetAst,
) -> ir::RouteRedirectTarget {
    match target {
        syntax::RouteRedirectTargetAst::View(view) => ir::RouteRedirectTarget::View(view.clone()),
        syntax::RouteRedirectTargetAst::Path(path) => ir::RouteRedirectTarget::Path(path.clone()),
    }
}

/// Cross-feature contracts — lower the optional `public contract <X> as v<N>`
/// AST clause into the IR `PublicContract` per
/// `docs/proposals/cross-feature-contracts.md` §5.1.
pub(crate) fn lower_public_contract(
    decl: &Option<syntax::PublicContractDeclAst>,
) -> Option<ir::PublicContract> {
    decl.as_ref().map(|d| ir::PublicContract {
        version: d.version,
        span_ref: Some(span_of(d.span)),
    })
}

/// Phase L Tier 4 follow-up — lower a canonical-indent `enum <Name>`
/// declaration into `ir::EnumDecl`. Variant storage values project
/// directly onto `ir::StorageValue`; absent values leave the codegen
/// target free to pick.
pub(crate) fn lower_enum_decl(decl: &syntax::EnumDeclAst) -> ir::EnumDecl {
    ir::EnumDecl {
        name: decl.name.clone(),
        public_contract: lower_public_contract(&decl.public_contract),
        variants: decl
            .variants
            .iter()
            .map(|v| ir::EnumVariant {
                name: v.name.clone(),
                storage_value: v.storage.as_ref().map(|s| match s {
                    syntax::EnumStorageValueDecl::Integer(n) => ir::StorageValue::Integer(*n),
                    syntax::EnumStorageValueDecl::String(s) => ir::StorageValue::String(s.clone()),
                }),
                label_key: v.label_key.clone(),
                hint_key: v.hint_key.clone(),
                icon_key: v.icon_key.clone(),
                previous_names: Vec::new(),
            })
            .collect(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// Phase L Tier 4b — lower a canonical-indent `command` block into
/// `ir::Command`. The kind is inferred from the body shape: `creates`
/// → Create, `updates` → Update, `deletes` → Delete, `returns` → Returns,
/// `handler`-only → Returns (the escape hatch case).
pub(crate) fn lower_command_decl(
    feature: &str,
    c: &syntax::CommandDecl,
) -> Result<ir::Command, AnalyzeError> {
    let kind = match c.effect.as_ref().map(|e| e.kind) {
        Some(syntax::CommandEffectKindDecl::Creates) => ir::CommandKind::Create,
        Some(syntax::CommandEffectKindDecl::Updates) => ir::CommandKind::Update,
        Some(syntax::CommandEffectKindDecl::Deletes) => ir::CommandKind::Delete,
        None => ir::CommandKind::Returns,
    };
    let route = c
        .route
        .iter()
        .map(|r| ir::RouteSlot {
            name: r.name.clone(),
            type_ref: type_ref_from_text(&r.type_text),
            from: r.from.clone(),
            kind: lower_route_slot_kind(r.kind),
        })
        .collect();
    let input = match &c.input {
        syntax::CommandInputDecl::Empty => ir::CommandInput::Empty,
        syntax::CommandInputDecl::Short(name) => ir::CommandInput::Short(vec![name.clone()]),
        syntax::CommandInputDecl::Typed(slots) => {
            // L0 #3 §10.2 — apply combination + default-compat checks
            // to each typed input slot too. Wave-B-CL4 — also run the
            // range / type-compatibility / pattern-compile checks so
            // command inputs aren't a back door past the resource-side
            // diagnostics.
            let mut lifted = Vec::with_capacity(slots.len());
            for s in slots {
                validate_constraint_combinations(&s.name, &s.constraints)?;
                validate_constraint_range_invariant(&s.name, &s.constraints)?;
                let (cleaned, vskip) = strip_validate_skip(&s.type_text);
                validate_constraint_type_compatibility(&s.name, &cleaned, &s.constraints)?;
                validate_constraint_pattern_compile(&s.name, &s.constraints)?;
                lifted.push(ir::TypedSlot {
                    name: s.name.clone(),
                    type_ref: type_ref_from_text(&cleaned),
                    required: s.required,
                    constraints: lift_field_constraints(&s.name, &s.constraints)?,
                    validate_skip: vskip,
                });
            }
            ir::CommandInput::Typed(lifted)
        }
    };
    let target = c.target.as_ref().map(lower_target_expr);
    let lets = c.lets.iter().map(lower_let_binding).collect();
    let effect = if let Some(e) = c.effect.as_ref() {
        lower_command_effect(e)
    } else if let Some(returns) = c.returns.as_deref() {
        ir::CommandEffect::Returns(ir::ReturnsEffect {
            return_type: type_ref_from_text(returns),
        })
    } else {
        ir::CommandEffect::None
    };
    let policy = c
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let emits = c.emits.iter().map(|e| e.name.clone()).collect();
    let audit = c.audit.as_ref().map(|a| ir::AuditSpec {
        subjects: a.subjects.clone(),
        emit_to: a.emit_to.clone(),
        data_subject: a.data_subject.clone(),
        record_before: a.record_before,
        record_after: a.record_after,
        retain_for: a.retain_for.clone(),
    });
    let approval = c.approval.as_ref().map(|a| ir::ApprovalSpec {
        required_when: a.required_when.clone(),
        by: a.by.clone(),
        timeout: a.timeout.clone(),
        then: match a.then {
            syntax::ApprovalThenDecl::Deny => ir::ApprovalThen::Deny,
            syntax::ApprovalThenDecl::Allow => ir::ApprovalThen::Allow,
            syntax::ApprovalThenDecl::Escalate => ir::ApprovalThen::Escalate,
        },
    });
    let invalidates = c
        .invalidates
        .iter()
        .map(|inv| ir::InvalidatesSpec {
            query: lower_invalidates_query_ref(feature, &inv.query),
            args: inv.args.iter().map(lower_named_arg).collect(),
        })
        .collect();
    let external_calls = c.external_calls.iter().map(lower_external_call).collect();
    let deprecated = c
        .deprecated
        .as_ref()
        .map(|dep| lower_deprecated(dep, DeprecationTarget::Command));
    // Phase L Tier 4 follow-up — lift `timeout`/`retry`/`idempotency by`
    // mirrors of `parse_job`. Doctor cross-checks against
    // `external_calls` for the `INT-CALL-*` integration coverage rules.
    let timeout = c.timeout.clone();
    let retry = c.retry.as_ref().map(lower_retry);
    let idempotency = c
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let write_window = c.write_window.as_ref().map(|w| ir::CommandWriteWindow {
        by: lower_path_string(&w.by),
        within: w.within.clone(),
        span_ref: Some(span_of(w.span)),
    });
    let policy_expr = c.policy_expr.as_ref().map(lower_policy_expr);
    // WAR-RUNTIME-COMMAND-01 (Effect half): lift `handler @fn.<name>`
    // into the typed `HandlerRef`. `handler "./path.go"` (file escape
    // hatch) lifts as namespace=`path`, name=verbatim path. Codegen uses
    // the `fn` form to auto-wire `Effect: lazuli.Returns(...)` when the
    // declarative body has no other effect.
    let handler = c.handler.as_ref().map(|h| {
        let path = h.path.trim();
        if let Some(name) = path.strip_prefix("@fn.") {
            ir::HandlerRef {
                namespace: "fn".to_owned(),
                name: name.to_owned(),
                span_ref: Some(span_of(c.span)),
            }
        } else {
            ir::HandlerRef {
                namespace: "path".to_owned(),
                name: path.to_owned(),
                span_ref: Some(span_of(c.span)),
            }
        }
    });
    // IR Error-Vocab (Cell PARSE-1) — lift the optional `when_denied
    // @translation.<key>` child captured by the parser under `policy`.
    // Resolution-chain step 1 (proposal §2.A).
    let policy_when_denied = c.policy_when_denied.as_ref().map(lower_translation_key_ref);
    Ok(ir::Command {
        name: c.name.clone(),
        public_contract: lower_public_contract(&c.public_contract),
        kind,
        route,
        input,
        target,
        lets,
        effect,
        policy,
        policy_expr,
        policy_when_denied,
        emits,
        rate_limit: c.rate_limit.as_ref().map(lower_rate_limit_spec),
        audit,
        approval,
        invalidates,
        external_calls,
        timeout,
        retry,
        idempotency,
        write_window,
        deprecated,
        handler,
        tests: None,
        triggers: c.triggers.clone(),
        synthesized_from_cap_file: None,
        // owner-scope §7.3 — author-written commands default to
        // tenant-only. The synth pass mutates the slot post-build for
        // crud / me bundle outputs when the resource carries
        // `@owner_axis`.
        owner_scope_sql: None,
        previous_names: c.previously.clone(),
        span_ref: Some(span_of(c.span)),
    })
}

#[derive(Clone, Copy)]
enum DeprecationTarget {
    Command,
    Api,
}

/// OpenAPI bucket cycle — lower an authored `deprecated` decorator into
/// the typed IR shape. `replacement` is classified by syntactic shape:
/// `https?://` → Url, `[<feature>.]command.<name>` / `[<feature>.]api.<name>`
/// → typed callable ref, otherwise → same-kind local ref.
pub(crate) fn lower_deprecated(
    decl: &syntax::CommandDeprecatedDecl,
    target: DeprecationTarget,
) -> ir::Deprecation {
    let replacement = decl.replacement.as_ref().map(|raw| {
        let trimmed = raw.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            ir::DeprecationReplacement::Url(trimmed.to_owned())
        } else if let Some(stripped) = trimmed.strip_prefix("@") {
            // `@adapter.command.<name>` or similar — store as Url-style
            // verbatim escape hatch.
            ir::DeprecationReplacement::Url(format!("@{}", stripped))
        } else {
            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() == 2 && parts[0] == "command" {
                ir::DeprecationReplacement::LocalCommand(parts[1].to_owned())
            } else if parts.len() == 2 && parts[0] == "api" {
                ir::DeprecationReplacement::LocalApi(parts[1].to_owned())
            } else if parts.len() == 3 && parts[1] == "command" {
                ir::DeprecationReplacement::Qualified(ir::QualifiedName {
                    feature: Some(parts[0].to_owned()),
                    name: parts[2].to_owned(),
                })
            } else if parts.len() == 3 && parts[1] == "api" {
                ir::DeprecationReplacement::QualifiedApi(ir::QualifiedName {
                    feature: Some(parts[0].to_owned()),
                    name: parts[2].to_owned(),
                })
            } else {
                match target {
                    DeprecationTarget::Command => {
                        ir::DeprecationReplacement::LocalCommand(trimmed.to_owned())
                    }
                    DeprecationTarget::Api => {
                        ir::DeprecationReplacement::LocalApi(trimmed.to_owned())
                    }
                }
            }
        }
    });
    ir::Deprecation {
        since: decl.since.clone(),
        replacement,
        sunset: decl.sunset.clone(),
    }
}

pub(crate) fn lower_route_slot_kind(kind: syntax::CommandRouteSlotKind) -> ir::RouteSlotKind {
    match kind {
        syntax::CommandRouteSlotKind::Plain => ir::RouteSlotKind::Plain,
        syntax::CommandRouteSlotKind::OpaqueToken => ir::RouteSlotKind::OpaqueToken,
        syntax::CommandRouteSlotKind::SignedToken => ir::RouteSlotKind::SignedToken,
    }
}

/// Phase L Tier 4b — lower a canonical-indent `api` block into `ir::Api`.
pub(crate) fn lower_api_decl(a: &syntax::ApiDecl) -> ir::Api {
    let method = match a.method {
        syntax::HttpMethod::Get => ir::HttpMethod::Get,
        syntax::HttpMethod::Post => ir::HttpMethod::Post,
        syntax::HttpMethod::Put => ir::HttpMethod::Put,
        syntax::HttpMethod::Patch => ir::HttpMethod::Patch,
        syntax::HttpMethod::Delete => ir::HttpMethod::Delete,
    };
    let policy = a
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let handler = a
        .handler
        .as_deref()
        .map(ir::PathRef::authored)
        .unwrap_or_else(|| ir::PathRef::convention(format!("./api/{}.go", a.name)));
    let policy_expr = a.policy_expr.as_ref().map(lower_policy_expr);
    let deprecated = a
        .deprecated
        .as_ref()
        .map(|dep| lower_deprecated(dep, DeprecationTarget::Api));
    ir::Api {
        name: a.name.clone(),
        method,
        path: a.path.clone(),
        policy,
        policy_expr,
        policy_when_denied: None,
        rate_limit: a.rate_limit.as_ref().map(lower_rate_limit_spec),
        output: type_ref_from_text(&a.output),
        handler,
        locale_negotiate: a.locale_negotiate.as_ref().map(lower_locale_negotiate_decl),
        deprecated,
        span_ref: Some(span_of(a.span)),
    }
}

// -----------------------------------------------------------------------------
// Report vocab — lower `report <name>` AST onto IR.
// -----------------------------------------------------------------------------

/// Lower a `ReportDecl` AST into `ir::Report`. Visibility defaults to
/// `signed` (per proposal §Slot inventory); formats outside the closed
/// `{csv, xlsx}` catalog drop silently — doctor reports
/// `REPORT-FORMAT-UNKNOWN-001` against the AST. Filename tokens are
/// parsed via the closed catalog (`{format}`, `{ctx.now:<strftime>}`,
/// `{ctx.user.id}`, `{ctx.tenant.id}`); unknown tokens land as
/// `FilenameToken::CtxNowStrftime("")` placeholders only if a parsing
/// helper rejects them — but we instead keep the literal verbatim and
/// surface unknown tokens via doctor.
pub(crate) fn lower_report_decl(
    _feature: &str,
    r: &syntax::ReportDecl,
) -> Result<ir::Report, AnalyzeError> {
    let source = lower_report_source(&r.source);

    let columns: Vec<ir::ReportColumn> = r
        .columns
        .iter()
        .map(|col| ir::ReportColumn {
            name: col.name.clone(),
            source: lower_report_column_source(&col.source),
            label: col.label.clone(),
            format: col.format.clone(),
            span_ref: Some(span_of(col.span)),
        })
        .collect();

    let formats: Vec<ir::ReportFormat> = r
        .formats
        .iter()
        .filter_map(|token| ir::ReportFormat::from_token(token.as_str()))
        .collect();

    let storage = r.storage.as_deref().map(lower_qualified_name);

    let visibility = match r.visibility.as_deref() {
        Some("public") => ir::FileVisibility::Public,
        Some("private") => ir::FileVisibility::Private,
        // Default per proposal §Slot inventory; doctor enforces signed
        // pairing with `signed_ttl`.
        _ => ir::FileVisibility::Signed,
    };

    let filename = r.filename.as_deref().map(lower_report_filename);

    let policy = r
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);

    let audit = r.audit.as_ref().map(|a| ir::AuditSpec {
        subjects: a.subjects.clone(),
        // Proposal v0.2 forbids `emit_to` on reports; doctor surfaces
        // any author-supplied value. The lowering preserves what was
        // written so the doctor lint sees the offending edge.
        emit_to: a.emit_to.clone(),
        data_subject: a.data_subject.clone(),
        record_before: a.record_before,
        record_after: a.record_after,
        retain_for: a.retain_for.clone(),
    });

    let policy_expr = r.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Report {
        name: r.name.clone(),
        source,
        columns,
        formats,
        storage,
        visibility,
        signed_ttl: r.signed_ttl.clone(),
        filename,
        policy,
        policy_expr,
        rate_limit: r.rate_limit.as_ref().map(lower_rate_limit_spec),
        audit,
        span_ref: Some(span_of(r.span)),
    })
}

pub(crate) fn lower_report_source(text: &str) -> ir::ReportSource {
    // Source forms:
    //   - `query.<name>`         (local short)
    //   - `<feature>.query.<name>` (cross-feature)
    //   - `<feature>.query.list.<name>` / `.lookup.<name>` / `.sql.<name>`
    //     (kind-qualified). The analyzer collapses the kind segment;
    //     doctor enforces the kind from the resolved target.
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    let qn = match parts.as_slice() {
        ["query", name] => ir::QualifiedName {
            feature: None,
            name: (*name).to_owned(),
        },
        [feature, "query", name] => ir::QualifiedName {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        [feature, "query", _kind, name] => ir::QualifiedName {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        _ => lower_qualified_name(trimmed),
    };
    ir::ReportSource::Query(qn)
}

pub(crate) fn lower_report_column_source(
    src: &syntax::ReportColumnSourceAst,
) -> ir::ReportColumnSource {
    match src {
        syntax::ReportColumnSourceAst::RowField(field) => {
            ir::ReportColumnSource::RowField(field.clone())
        }
        syntax::ReportColumnSourceAst::FnCall { name, args } => {
            ir::ReportColumnSource::Fn(ir::FnInvocation {
                name: name.clone(),
                args: args.clone(),
            })
        }
    }
}

/// Parse a filename template string into the closed `FilenameToken`
/// catalog. Unknown `{...}` tokens are silently dropped from the typed
/// token list; the literal is preserved so doctor's
/// `REPORT-FILENAME-TOKEN-UNKNOWN-001` rule can scan the literal and
/// report user-facing diagnostics.
pub(crate) fn lower_report_filename(literal: &str) -> ir::ReportFilenamePattern {
    let mut tokens = Vec::new();
    let bytes = literal.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(close) = literal[i + 1..].find('}') {
                let raw = &literal[i + 1..i + 1 + close];
                if let Some(token) = parse_filename_token(raw) {
                    tokens.push(token);
                }
                i = i + 1 + close + 1;
                continue;
            }
        }
        i += 1;
    }
    ir::ReportFilenamePattern {
        literal: literal.to_owned(),
        tokens,
    }
}

fn parse_filename_token(raw: &str) -> Option<ir::FilenameToken> {
    match raw {
        "format" => Some(ir::FilenameToken::Format),
        "ctx.user.id" => Some(ir::FilenameToken::CtxUserId),
        "ctx.tenant.id" => Some(ir::FilenameToken::CtxTenantId),
        _ => {
            if let Some(strftime) = raw.strip_prefix("ctx.now:") {
                return Some(ir::FilenameToken::CtxNowStrftime(strftime.to_owned()));
            }
            None
        }
    }
}

/// i18n bucket cycle — lower an authored `translation` block onto
/// `ir::Translation`. Variant locales and plural arms come through
/// verbatim; doctor validates them against `app.locale.supported` and
/// the CLDR plural catalog.
///
/// IR Error-Vocab (Cell PARSE-1) — lower a surface `FeatureErrorsDecl`
/// onto `ir::FeatureErrors`. The `default hide` / `default expose` and
/// `expose client 4xx|5xx <fields>` slots project 1:1; per-code message
/// overrides keep their verbatim `code` so analyzer-side closed-catalog
/// enforcement (ERR-VOCAB-CODE-UNKNOWN) can report the offending token.
pub(crate) fn lower_feature_errors_decl(decl: &syntax::FeatureErrorsDecl) -> ir::FeatureErrors {
    ir::FeatureErrors {
        default: decl.default.map(|d| match d {
            syntax::ErrorExposureDefaultAst::Hide => ir::ErrorExposureDefault::Hide,
            syntax::ErrorExposureDefaultAst::Expose => ir::ErrorExposureDefault::Expose,
        }),
        exposure_4xx: decl.exposure_4xx.clone(),
        exposure_5xx: decl.exposure_5xx.clone(),
        audience_exposure: decl
            .audience_exposure
            .iter()
            .map(|r| ir::ErrorExposeRule {
                audience: r.audience.clone(),
                fields: r.fields.clone(),
                span_ref: Some(span_of(r.span)),
            })
            .collect(),
        redact_patterns: decl.redact_patterns.clone(),
        messages: decl
            .messages
            .iter()
            .map(|m| ir::FeatureErrorMessage {
                code: m.code.clone(),
                message: lower_translation_key_ref(&m.message),
                span_ref: Some(span_of(m.span)),
            })
            .collect(),
        // Reserved for v2 — per-field validator-error references. v1
        // parser leaves the slot empty (see proposal §3.4 deferral row).
        field_messages: Vec::new(),
        span_ref: Some(span_of(decl.span)),
    }
}

pub(crate) fn lower_translation_decl(t: &syntax::TranslationDecl) -> ir::Translation {
    ir::Translation {
        catalog: t.catalog.clone(),
        keys: t
            .keys
            .iter()
            .map(|key| ir::TranslationKey {
                name: key.name.clone(),
                variants: key
                    .variants
                    .iter()
                    .map(|v| ir::TranslationVariant {
                        locale: v.locale.clone(),
                        text: v.text.clone(),
                    })
                    .collect(),
                plurals: key
                    .plurals
                    .iter()
                    .map(|p| ir::TranslationPluralArm {
                        arm: p.arm.clone(),
                        variants: p
                            .variants
                            .iter()
                            .map(|v| ir::TranslationVariant {
                                locale: v.locale.clone(),
                                text: v.text.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// i18n bucket cycle — lower a per-api `locale_negotiate` block onto
/// `ir::LocaleNegotiate`. The runtime-unit form is parsed elsewhere
/// (`crates/lazuli_cli/src/app_manifest.rs`) since it lives on the
/// `app.lzi` side rather than feature side.
pub(crate) fn lower_locale_negotiate_decl(n: &syntax::LocaleNegotiateDecl) -> ir::LocaleNegotiate {
    ir::LocaleNegotiate {
        source: n.source.clone(),
        strategy: n.strategy.clone(),
        fallback: n.fallback.clone(),
    }
}

/// Phase L Tier 4a — lower a canonical-indent `defaults` block into
/// `ir::Defaults`. `policy_for` entries collapse onto `Defaults.policy`
/// when a single entry is authored; multi-entry `policy_for` (different
/// atoms per kind list) is captured by reading the first entry — the
/// language disallows conflicting defaults by convention. Doctor cross-
/// checks the surface form by walking the typed
/// `feature.policies.categories` slot (`populate_commands_from_ir`);
/// the legacy `collect_policy_atoms` text walker is retired.
pub(crate) fn lower_defaults(defaults: &syntax::FeatureDefaults) -> ir::Defaults {
    let tenancy = defaults.tenancy.as_ref().map(|t| match t {
        syntax::DefaultsTenancy::Org => ir::Tenancy::Org,
        syntax::DefaultsTenancy::Team => ir::Tenancy::Team,
        syntax::DefaultsTenancy::None => ir::Tenancy::None,
        syntax::DefaultsTenancy::Custom(name) => ir::Tenancy::Custom(name.clone()),
    });
    let policy = defaults
        .policy_for
        .first()
        .map(|entry| lower_policy_atom(entry.atom.as_str()))
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    ir::Defaults {
        tenancy,
        timestamps: defaults.timestamps,
        policy,
    }
}

/// Phase L Tier 3 — lower a canonical-indent `job` block into `ir::Job`.
/// Handler-backed bodies lower fully; declarative bodies preserve the
/// raw spine (`raw_target`, `raw_lets`, `raw_effect`) until Tier 4.
pub fn lower_job(feature: &str, job: &syntax::Job) -> Result<ir::Job, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &job.trigger);
    let idempotency = job
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = job.retry.as_ref().map(lower_retry);
    let tenant_from = job
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let fanout = job.fanout.as_ref().map(lower_fanout);
    let external_calls = job.external_calls.iter().map(lower_external_call).collect();
    let policy = job
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let policy = match policy {
        ir::PolicyRef::None => None,
        other => Some(other),
    };
    let body = lower_job_body(&job.body);

    let policy_expr = job.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Job {
        name: job.name.clone(),
        trigger,
        queue: job.queue.clone(),
        idempotency,
        retry,
        policy,
        policy_expr,
        policy_when_denied: None,
        tenant_from,
        fanout,
        timeout: job.timeout.clone(),
        external_calls,
        body,
        emits: job.emits.clone(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(job.span)),
    })
}

// =============================================================================
// L0 #8 — poller lowering (docs/proposals/poller-vocab.md §4).
//
// AST → IR is purely structural; doctor rules enforce the closed-catalog
// validity invariants (cursor field shapes, terminal-state existence,
// handler orphan, etc.). The lowering never fails on AST alone — it
// applies the defaults (`tick.every = 30s`, `tick.batch = 100`) and
// surfaces structurally well-formed IR for downstream consumers.
// =============================================================================

/// Default tick interval when `tick every <duration>` is omitted in source.
/// Per proposal §3.8.
const POLLER_DEFAULT_TICK_EVERY: &str = "30s";
const POLLER_DEFAULT_TICK_BATCH: u32 = 100;

pub fn lower_poller(poller: &syntax::PollerBlockAst) -> Result<ir::Poller, AnalyzeError> {
    let cursor_ast = poller
        .cursor
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "cursor".to_owned(),
        })?;
    let retry_ast = poller
        .retry
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "retry".to_owned(),
        })?;
    let resolve_name =
        poller
            .resolve_handler
            .as_deref()
            .ok_or_else(|| AnalyzeError::MissingField {
                kind: "poller".to_owned(),
                name: poller.name.clone(),
                field: "resolve via @fn.<name>".to_owned(),
            })?;
    if poller.idempotency.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "idempotency".to_owned(),
        });
    }
    if poller.states.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "states".to_owned(),
        });
    }

    let cursor = ir::PollerCursor {
        next_at_field: cursor_ast.next_at_field.clone(),
        resolved_at_field: cursor_ast.resolved_at_field.clone(),
        attempts_field: cursor_ast.attempts_field.clone(),
        span_ref: Some(span_of(cursor_ast.span)),
    };

    let backoff = match retry_ast.backoff_strategy.as_str() {
        "fixed" => ir::PollerBackoff::Fixed {
            base: retry_ast.backoff_base.clone(),
        },
        "linear" => ir::PollerBackoff::Linear {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        "exponential" => ir::PollerBackoff::Exponential {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("poller `{}` backoff", poller.name),
                value: other.to_owned(),
            });
        }
    };
    let retry = ir::PollerRetry {
        max_attempts: retry_ast.max_attempts,
        backoff,
        span_ref: Some(span_of(retry_ast.span)),
    };

    let states = poller
        .states
        .iter()
        .map(|s| ir::PollerState {
            name: s.name.clone(),
            kind: match s.kind_keyword.as_deref() {
                Some("initial") => ir::PollerStateKind::Initial,
                Some("terminal") => ir::PollerStateKind::Terminal,
                Some("intermediate") | None => ir::PollerStateKind::Intermediate,
                Some(_) => ir::PollerStateKind::Intermediate,
            },
            span_ref: Some(span_of(s.span)),
        })
        .collect::<Vec<_>>();

    let tick = match poller.tick.as_ref() {
        Some(t) => ir::PollerTick {
            every: t.every.clone(),
            batch: t.batch.unwrap_or(POLLER_DEFAULT_TICK_BATCH),
        },
        None => ir::PollerTick {
            every: POLLER_DEFAULT_TICK_EVERY.to_owned(),
            batch: POLLER_DEFAULT_TICK_BATCH,
        },
    };

    let tenant_from = poller
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });

    let idempotency = ir::IdempotencyKey {
        by: ir::Path {
            segments: poller.idempotency.iter().cloned().collect(),
        },
    };

    let audit = poller.audit.as_deref().map(|raw: &str| {
        let rest = raw.strip_prefix("audit ").unwrap_or(raw).trim();
        if rest == "default" {
            ir::AuditSpec {
                subjects: vec!["actor".to_owned(), "target.id".to_owned()],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        } else if let Some(reason) = rest.strip_prefix("none ") {
            ir::AuditSpec {
                subjects: vec![format!("none {}", reason)],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        } else {
            ir::AuditSpec {
                subjects: rest
                    .split(',')
                    .map(str::trim)
                    .filter(|s: &&str| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        }
    });

    let retry_quirks = poller
        .retry_quirks
        .iter()
        .filter_map(|q| match q.kind.as_str() {
            "gender_flip_once" => Some(ir::PollerRetryQuirk::GenderFlipOnce {
                when: q.when.clone(),
                counter_field: q.counter_field.clone(),
                gender_field: q.mutate_field.clone(),
            }),
            // Unknown catalog entries are dropped during lowering;
            // doctor `POLLER-QUIRK-CATALOG-MISMATCH-001` surfaces the
            // diagnostic at the AST layer.
            _ => None,
        })
        .collect();

    Ok(ir::Poller {
        name: poller.name.clone(),
        source: poller.source.clone(),
        cursor,
        retry,
        states,
        resolve_handler: ir::HandlerRef {
            namespace: "fn".to_owned(),
            name: resolve_name.to_owned(),
            span_ref: Some(span_of(poller.span)),
        },
        terminal_status_field: poller.terminal_status_field.clone(),
        terminal_result_field: poller.terminal_result_field.clone(),
        tick,
        tenant_from,
        idempotency,
        audit,
        emits: poller.emits.clone(),
        retry_quirks,
        span_ref: Some(span_of(poller.span)),
    })
}

/// Phase L Tier 3 — lower a canonical-indent `webhook` block into
/// `ir::Webhook`. `verify: PathRef` falls back to a conventional path
/// derived from the webhook name (the legacy IR field is non-optional);
/// `structured_verify` carries the real structured spec lifted by
/// `parse_webhook_verify`.
pub fn lower_webhook(webhook: &syntax::Webhook) -> Result<ir::Webhook, AnalyzeError> {
    let structured_verify = Some(ir::VerifySpec {
        scheme: match webhook.verify.scheme.as_str() {
            "hmac" => ir::VerifyScheme::Hmac,
            other => {
                return Err(AnalyzeError::UnsupportedVerifyScheme {
                    scheme: other.to_owned(),
                });
            }
        },
        algorithm: webhook.verify.algorithm.clone(),
        secret_env: webhook
            .verify
            .secret_env
            .as_deref()
            .map(extract_env_binding)
            .unwrap_or_default(),
        header: webhook.verify.header.clone().unwrap_or_default(),
    });
    let tenant_from = webhook
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = webhook
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let policy = webhook
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));

    let (handler, returns) = match &webhook.handler {
        Some(h) => (
            ir::PathRef::authored(&h.path),
            h.returns.as_deref().map(|t| type_ref_from_text(t)),
        ),
        None => (
            ir::PathRef::convention(format!("./webhooks/{}.go", webhook.name)),
            None,
        ),
    };

    // Webhooks expanded cycle — typed payload reference (`payload from
    // webhook_events.<name>`). The parser stripped the catalog prefix
    // already, so the IR just keeps the suffix.
    let payload_from = webhook
        .payload_from
        .as_deref()
        .map(|name| ir::WebhookEventRef {
            name: name.to_owned(),
        });

    // `replay` short form (`replay allow within "..."`) and long form
    // (nested children) collapse onto the same `ReplaySpec`.
    let replay = webhook.replay.as_ref().map(|r| ir::ReplaySpec {
        mode: match r.mode.as_str() {
            "deny" => ir::ReplayMode::Deny,
            _ => ir::ReplayMode::Allow,
        },
        within: r.within.clone(),
        dedupe_by: r.dedupe_by.as_deref().map(lower_path_string),
    });

    // `dlq` discriminator (mutual exclusion enforced by the parser).
    let dlq = webhook.dlq.as_ref().map(|d| match d {
        syntax::WebhookDlq::Emit { event, .. } => ir::DlqSpec::Emit {
            event: event.clone(),
        },
        syntax::WebhookDlq::Handler { path, .. } => ir::DlqSpec::Handler {
            path: ir::PathRef::authored(path),
        },
        syntax::WebhookDlq::Drop { reason, .. } => ir::DlqSpec::Drop {
            reason: reason.clone(),
        },
    });

    // Inbound retry shares the jobs `RetryPolicy` shape (Atrito #5).
    let retry = webhook.retry.as_ref().map(lower_retry);

    let policy_expr = webhook.policy_expr.as_ref().map(lower_policy_expr);
    let scope_global = webhook
        .scope_global
        .as_ref()
        .map(|sg| ir::WebhookScopeGlobalSpec {
            reason: sg.reason.clone(),
        });
    // B5 framework gap 2 — lift per-branch emit predicates onto the
    // typed `EmitPredicate` shape. The AST carries the raw `when`
    // clauses; we promote `path = "literal"` and
    // `path in ("a", "b")` to typed variants and fall back to
    // `EmitPredicateKind::Other { raw }` for anything else. Length
    // matches `webhook.emits` when any predicate is authored; an
    // empty vec means "flat list, no per-branch dispatch".
    let emit_predicates = if webhook.emits_predicates.is_empty() {
        Vec::new()
    } else {
        webhook
            .emits_predicates
            .iter()
            .map(|raw| raw.as_deref().map(lower_emit_predicate))
            .collect::<Vec<_>>()
    };

    Ok(ir::Webhook {
        name: webhook.name.clone(),
        route: webhook.route.clone(),
        verify: ir::PathRef::convention(format!("./webhooks/{}_verify.go", webhook.name)),
        structured_verify,
        tenant_from,
        scope_global,
        idempotency,
        policy,
        policy_expr,
        policy_when_denied: None,
        handler,
        returns,
        emits: webhook.emits.clone(),
        emit_predicates,
        payload_from,
        replay,
        dlq,
        retry,
        previous_names: Vec::new(),
        span_ref: Some(span_of(webhook.span)),
    })
}

/// B5 framework gap 2 — lift a raw `when <predicate>` clause into the
/// typed `ir::EmitPredicate`. Recognised shapes:
///
/// * `path = "literal"` — equality.
/// * `path in ("a", "b")` — set membership.
/// * anything else — `EmitPredicateKind::Other { raw }`.
///
/// The lift is intentionally conservative: shapes that don't match
/// the typed catalog are preserved verbatim so codegen can emit a
/// runtime-evaluated stub without losing authoring intent.
/// Realtime bucket cycle MVP — lower a canonical-indent `channel`
/// block into `ir::Channel`. Mechanical projection: the parser
/// already enforces presence of all three required children, so the
/// lowering only wraps the verbatim strings into the typed shapes
/// (`TenantFromSpec`, `PolicyRef::Atom`, payload string verbatim).
/// Doctor `CHANNEL-PAYLOAD-001` resolves the payload reference
/// downstream.
pub fn lower_channel(channel: &syntax::Channel) -> ir::Channel {
    ir::Channel {
        name: channel.name.clone(),
        tenant_from: ir::TenantFromSpec {
            path: lower_path_string(&channel.tenant_from),
        },
        policy: lower_policy_atom(&channel.policy),
        policy_when_denied: None,
        payload: channel.payload.clone(),
        span_ref: Some(span_of(channel.span)),
    }
}

/// Phase L Tier 3 — lower a canonical-indent `notification` block into
/// `ir::Notification`. Reuses `JobTrigger`, `IdempotencyKey`,
/// `RetryPolicy`, `TenantFromSpec` from the job lowering helpers.
pub fn lower_notification(
    feature: &str,
    notification: &syntax::Notification,
) -> Result<ir::Notification, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &notification.trigger);
    let tenant_from = notification
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = notification
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = notification.retry.as_ref().map(lower_retry);
    let policy = notification
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    let digest = notification.digest.as_ref().map(lower_notification_digest);
    let throttle = notification
        .throttle
        .as_ref()
        .map(lower_notification_throttle);
    let policy_expr = notification.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Notification {
        name: notification.name.clone(),
        trigger,
        channels: notification.channels.clone(),
        recipient: notification.recipient.clone(),
        template: notification.template.clone(),
        policy,
        policy_expr,
        tenant_from,
        idempotency,
        retry,
        emits: notification.emits.clone(),
        digest,
        throttle,
        previous_names: Vec::new(),
        span_ref: Some(span_of(notification.span)),
    })
}

/// MCP bucket cycle — lower a canonical-indent `mcp_server` block into
/// `ir::MCPServerSpec`. Value-preserving except for the closed-catalog
/// `transport` mapping, which rejects unknown literals at lower-time.
pub fn lower_mcp_server(server: &syntax::McpServer) -> Result<ir::MCPServerSpec, AnalyzeError> {
    let transport = match server.transport.as_str() {
        "stdio" => ir::MCPTransport::Stdio,
        "http_sse" => ir::MCPTransport::HttpSse,
        "http_streamable" => ir::MCPTransport::HttpStreamable,
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("MCP-TRANSPORT-001 mcp_server `{}` transport", server.name),
                value: other.to_owned(),
            });
        }
    };
    let auth = server.auth.as_deref().and_then(parse_mcp_auth);
    let metadata = ir::MCPServerMetadata {
        name: server.metadata.name.clone(),
        description: server.metadata.description.clone(),
        version: server.metadata.version.clone(),
    };
    let tools = server.tools.iter().map(lower_mcp_tool).collect::<Vec<_>>();
    let resources = server
        .resources
        .iter()
        .map(lower_mcp_resource)
        .collect::<Vec<_>>();
    let prompts = server
        .prompts
        .iter()
        .map(lower_mcp_prompt)
        .collect::<Vec<_>>();
    Ok(ir::MCPServerSpec {
        name: server.name.clone(),
        transport,
        scope_feature: server.scope_feature.clone(),
        auth,
        metadata,
        tools,
        resources,
        prompts,
        span_ref: Some(span_of(server.span)),
    })
}

/// Parse `bearer env.<NAME>` into `ir::MCPAuth::BearerEnvVar`. Anything
/// else (future `oauth ...`, malformed line) returns `None`; doctor
/// `MCP-AUTH-001` (registered in proposal) catches malformed shapes.
fn parse_mcp_auth(raw: &str) -> Option<ir::MCPAuth> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("bearer env.") {
        let env = rest.trim().to_owned();
        if env.is_empty() {
            return None;
        }
        return Some(ir::MCPAuth::BearerEnvVar { env });
    }
    None
}

/// Phase L Tier 3 — lower a canonical-indent `event_group` into
/// `ir::EventGroup`. The payload bag and authored events stay as raw
/// strings; B5 framework gap 1 lifts the per-event typed payload
/// blocks into `variants`.
pub fn lower_event_group(group: &syntax::EventGroup) -> ir::EventGroup {
    // EVENT-OUTBOX §3.3 — lower the parallel bool vec into the typed
    // `OutboxMode` catalog. Index-paired with `events`; when the AST
    // emits an empty vec (legacy / pre-outbox payloads) we expand to
    // a same-length `None` vec so downstream code can read by index.
    let events_outbox: Vec<ir::OutboxMode> = if group.events_outbox_guaranteed.is_empty() {
        vec![ir::OutboxMode::None; group.events.len()]
    } else {
        group
            .events_outbox_guaranteed
            .iter()
            .map(|g| {
                if *g {
                    ir::OutboxMode::Guaranteed
                } else {
                    ir::OutboxMode::None
                }
            })
            .collect()
    };

    // B5 framework gap 1 — lift per-event field bodies into
    // `EventVariant` records. Each variant carries its `EventField`s
    // lifted via `type_ref_from_syntax`, the closed kind catalog
    // (committed vs trace), and the outbox flag mirrored from the
    // parallel slot above. Back-compat: variants whose body was
    // empty come through with an empty `fields` Vec; legacy fixtures
    // that didn't author `event_variants`/`event_variant_kinds` at
    // all leave `variants` empty.
    let variants: Vec<ir::EventVariant> =
        if group.event_variants.is_empty() && group.event_variant_kinds.is_empty() {
            Vec::new()
        } else {
            group
                .events
                .iter()
                .enumerate()
                .map(|(idx, short_name)| {
                    let kind = match group
                        .event_variant_kinds
                        .get(idx)
                        .copied()
                        .unwrap_or(syntax::EventVariantKindAst::Committed)
                    {
                        syntax::EventVariantKindAst::Committed => ir::EventVariantKind::Committed,
                        syntax::EventVariantKindAst::Trace => ir::EventVariantKind::Trace,
                    };
                    let fields = group
                        .event_variants
                        .get(idx)
                        .map(|rows| {
                            rows.iter()
                                .map(lower_event_variant_field)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let outbox = events_outbox
                        .get(idx)
                        .copied()
                        .unwrap_or(ir::OutboxMode::None);
                    ir::EventVariant {
                        name: short_name.clone(),
                        kind,
                        outbox,
                        fields,
                        span_ref: group
                            .event_variants
                            .get(idx)
                            .and_then(|rows| rows.first().map(|f| span_of(f.span))),
                    }
                })
                .collect()
        };

    ir::EventGroup {
        pattern: group.pattern.clone(),
        on_resource: group.on_resource.clone(),
        raw_payload: group.payload.clone(),
        raw_audit: group.audit.clone(),
        events: group.events.clone(),
        events_outbox,
        variants,
        span_ref: Some(span_of(group.span)),
    }
}

/// Migrations bucket cycle Route C — lower a canonical-indent
/// `tenant_migration` block into `ir::TenantMigration`. Mirrors
/// `lower_job` for the shared spine (idempotency / retry / timeout /
/// handler) and adds the `target tenants <axis>` slot. The lowering
/// does **not** enforce that `idempotency` is authored; that is
/// `TM-IDEMP-001`'s job downstream.
pub fn lower_tenant_migration(
    tm: &syntax::TenantMigration,
) -> Result<ir::TenantMigration, AnalyzeError> {
    let idempotency = tm
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path })
        .unwrap_or_else(|| ir::IdempotencyKey {
            by: ir::Path::from_segments(Vec::<String>::new()),
        });
    let retry = tm.retry.as_ref().map(lower_retry);
    Ok(ir::TenantMigration {
        name: tm.name.clone(),
        target: ir::TenantMigrationTarget {
            operation: tm.target_ref.as_deref().map(lower_tenant_migration_target),
            axis: tm.target_axis.clone(),
        },
        idempotency,
        retry,
        timeout: tm.timeout.clone(),
        handler: ir::PathRef::authored(&tm.handler),
        previous_names: Vec::new(),
        span_ref: Some(span_of(tm.span)),
    })
}

/// Analyzer-level resolution for `Command.invalidates`. This pass is
/// intentionally module-scoped: same-feature refs were normalized during
/// per-feature lowering, but cross-feature refs can only be validated once
/// all feature IR is present.
pub fn resolve_invalidates_targets(module: &mut ir::Module) -> Result<(), AnalyzeError> {
    normalize_legacy_invalidates_targets(&mut module.features);
    validate_invalidates_targets(&module.features)
}

pub fn validate_invalidates_targets(features: &[ir::Feature]) -> Result<(), AnalyzeError> {
    let index = InvalidatesQueryIndex::from_features(features);
    for feature in features {
        for command in &feature.commands {
            for invalidates in &command.invalidates {
                let target_feature = invalidates
                    .query
                    .feature
                    .as_deref()
                    .unwrap_or(feature.name.as_str());
                if !index.has_query(target_feature, &invalidates.query.name) {
                    return Err(AnalyzeError::UnknownInvalidateTarget {
                        cmd: command.name.clone(),
                        target: invalidates_target_display(&feature.name, &invalidates.query),
                        target_feature: target_feature.to_owned(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn normalize_legacy_invalidates_targets(features: &mut [ir::Feature]) {
    for feature in features {
        for command in &mut feature.commands {
            for invalidates in &mut command.invalidates {
                match invalidates.query.feature.as_deref() {
                    Some("query") | None => {
                        invalidates.query.feature = Some(feature.name.clone());
                    }
                    _ => {}
                }
            }
        }
    }
}

fn invalidates_target_display(current_feature: &str, query: &ir::QualifiedName) -> String {
    match query.feature.as_deref() {
        Some(feature) if feature == current_feature => format!("query.{}", query.name),
        Some(feature) => format!("{feature}.query.{}", query.name),
        None => format!("query.{}", query.name),
    }
}

struct InvalidatesQueryIndex {
    queries_by_feature: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

impl InvalidatesQueryIndex {
    fn from_features(features: &[ir::Feature]) -> Self {
        let queries_by_feature = features
            .iter()
            .map(|feature| {
                (
                    feature.name.clone(),
                    feature
                        .queries
                        .iter()
                        .map(|query| query.name().to_owned())
                        .collect(),
                )
            })
            .collect();
        Self { queries_by_feature }
    }

    fn has_query(&self, feature: &str, query: &str) -> bool {
        self.queries_by_feature
            .get(feature)
            .is_some_and(|queries| queries.contains(query))
    }
}

/// Extract the env binding name from `env.<NAME>` (`secret env.X`).
fn extract_env_binding(raw: &str) -> String {
    raw.trim()
        .strip_prefix("env.")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|| raw.trim().to_owned())
}

/// Build a feature-local `QualifiedName` (no feature prefix).
pub(crate) fn qualified_name_local(name: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

/// Treat the entire namespace literal as a single name (e.g.
/// `@llm.default`, `@validator.pii_email_scrub`, `@semantic.Email`).
/// Doctor + LSP enforce the closed-namespace catalog elsewhere; this
/// helper keeps the raw form so resolution stays uniform.
pub(crate) fn qualified_namespace(raw: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: raw.to_owned(),
    }
}

#[cfg(test)]
pub(crate) fn lower_policy_atom_with_args(text: &str) -> ir::PolicyAtom {
    let raw = text.trim().strip_prefix('@').unwrap_or(text.trim());
    let (ns_name, args) = match raw.split_once('(') {
        Some((head, tail)) => (head.trim(), Some(tail.trim_end_matches(')').to_owned())),
        None => (raw.trim(), None),
    };
    let (namespace, name) = ns_name
        .split_once('.')
        .map(|(namespace, name)| (namespace.to_owned(), name.to_owned()))
        .unwrap_or_else(|| ("".to_owned(), ns_name.to_owned()));
    ir::PolicyAtom {
        namespace,
        name,
        args,
    }
}

#[cfg(test)]
pub(crate) fn lower_audit_block(src: &str) -> ir::AuditSpec {
    let mut spec = ir::AuditSpec {
        subjects: Vec::new(),
        emit_to: None,
        data_subject: None,
        record_before: false,
        record_after: false,
        retain_for: None,
    };
    for line in src.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(rest) = line.strip_prefix("audit data_subject ") {
            spec.data_subject = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("data_subject ") {
            spec.data_subject = Some(rest.trim().to_owned());
        } else if line == "audit before" || line == "before" {
            spec.record_before = true;
        } else if line == "audit after" || line == "after" {
            spec.record_after = true;
        } else if let Some(rest) = line
            .strip_prefix("audit retain ")
            .or_else(|| line.strip_prefix("retain "))
        {
            spec.retain_for = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("audit ") {
            for part in rest
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                match part {
                    "before" => spec.record_before = true,
                    "after" => spec.record_after = true,
                    _ => spec.subjects.push(part.to_owned()),
                }
            }
        } else if let Some(rest) = line.strip_prefix("emit_to ") {
            spec.emit_to = Some(rest.trim().to_owned());
        }
    }
    spec
}

/// The Phase 1 parser captures type references as raw source text. Turn
/// that into a minimal `TypeRef` so doctor and inspect can read it; the
/// canonical-indent migration replaces this with a real type-ref parser.
pub(crate) fn type_ref_from_text(text: &str) -> ir::TypeRef {
    // Single canonical lifter for type tokens. Previously a slimmer
    // duplicate of `type_ref_from_syntax` with drift bugs (notably:
    // matched `"Json"` only, lost `"JSON"`; always lowered `@semantic.*`
    // to `SemanticEmail`). Delegating fixes both at the source.
    type_ref_from_syntax(text.trim())
}

// =============================================================================
// L0 #3 §10 — inline field constraint analyzer tests (Cells D.1+D.2+D.3).
//
// Combination rules per §10.2 (length / between / in conflicts) plus

// =============================================================================
// `conventions [crud]` synthesis pass — Cell C3 tests
//
// Spec: `docs/proposals/ir-resource-conventions-crud.md` §5–§11.
//
// Tests build `ir::Feature` values programmatically because Cell C2's
// parser shim for `conventions [crud]` lands in parallel. The synth
// pass operates on the post-parse IR so direct construction is the

// =============================================================================
// `conventions [me]` synthesis pass — Cell M2 tests
//
// Spec: `docs/proposals/ir-resource-conventions-me.md` §§5–§11.
//
// Tests build `ir::Feature` values programmatically because M1's parser
// shim for `conventions [me]` lands in parallel. The synth pass operates
// on the post-parse IR so direct construction is the canonical surface
// to exercise here.
//
// Coverage:
// - 4 mode tests: user_keyed, user_keyed_no_org, org_keyed, self_keyed.
// - Override test: author wrote `lookup_my_customer` → synth skipped,
//   `synth_origins` records `AuthorOverride(Me)`.
// - Composition test: `conventions [crud, me]` → 6 entries, no collisions.
// - Diagnostic: `MeNoActorResolution` when resource has neither axis.

// =============================================================================
// Cell O2 — `@owner_axis(through: <col>)` synth-pass tests.
//
// Spec: `docs/proposals/ir-resource-conventions-owner-scope.md`
// §7.3 + §8 + §8.5.A + §11.1.
//
// Coverage matrix:
//   1. Mode: owner-scope `delete_*` emits chain WHERE.
//   2. Mode: owner-scope `update_*` / `lookup_*` / `list_*` emit chain WHERE.
//   3. CTE: owner-scope `create_*` emits CTE-INSERT shape via `cte_owner_check`.
//   4. Composition: `[crud, me]` + `@owner_axis` -> `lookup_my_*` ALSO carries scope.
//   5. Diagnostic: `owner_axis_unknown_through`.
//   6. Diagnostic: `owner_axis_through_not_user_keyed`.
//   7. Diagnostic: `owner_axis_collides_with_unique_user`.
//   8. Override: author's `command delete_<r>` skips synth; no diagnostic; scope
//      is NOT attached to the author's command.
//   9. Direct-call form: `build_owner_scope_where_for_test` round-trips the SQL.
//
// RULE-VOCAB-03 affirmation: each test asserts on the *single* SQL shape the

#[cfg(test)]
mod tests;
