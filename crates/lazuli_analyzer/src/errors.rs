//! `AnalyzeError` + the `conventions` closed-catalog suggestion helper.
//!
//! ## Why this is a separate module
//!
//! Every per-slot lowering returns `Result<_, AnalyzeError>`. The error
//! enum sits at the public surface of the crate, so it ends up imported
//! from doctor, the CLI, and the LSP. Keeping the variant declarations
//! together with their inline `#[error("...")]` strings in one file
//! makes the closed-vocabulary contract auditable: every diagnostic
//! identifier ships next to its message text.
//!
//! The `conventions` catalog helpers live here for the same reason — the
//! `ConventionsUnknown` variant references `format_conventions_unknown`
//! directly via the `#[error("{}", ...)]` form, and the
//! `conventions_unknown_suggestion` helper feeds the variant's
//! `suggestion` field. Keeping the trio together prevents the formatter
//! and the catalog from drifting apart.
//!
//! No public alias paths change: `lazuli_analyzer::AnalyzeError`,
//! `lazuli_analyzer::CONVENTION_CATALOG`, and
//! `lazuli_analyzer::conventions_unknown_suggestion` all re-export from
//! this module.

use crate::helpers::conventions_levenshtein;
use thiserror::Error;

/// Closed catalog of analyzer-time failures.
///
/// Every per-slot lowering returns `Result<_, AnalyzeError>`; this enum
/// is the shared rejection vocabulary the CLI, doctor, and LSP all
/// consume. Variants carry the field names that appear in the
/// `#[error("...")]` template so the formatted message round-trips
/// without callsite interpolation.
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
    /// Stable doctor-style code for the variant, when one exists.
    ///
    /// Most analyzer errors are surfaced via their `Display` message
    /// alone (build-blocking), but a handful round-trip through the
    /// doctor diagnostics pipeline and need a stable code attached.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_analyzer::AnalyzeError;
    ///
    /// let err = AnalyzeError::UnknownInvalidateTarget {
    ///     cmd: "create".into(),
    ///     target: "query.bogus".into(),
    ///     target_feature: "Customer".into(),
    /// };
    /// assert_eq!(err.diagnostic_code(), Some("@correctness.unknown_invalidate_target"));
    /// ```
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
///
/// ## Examples
///
/// ```
/// use lazuli_analyzer::CONVENTION_CATALOG;
///
/// assert!(CONVENTION_CATALOG.contains(&"crud"));
/// assert!(CONVENTION_CATALOG.windows(2).all(|w| w[0] < w[1]));
/// ```
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
///
/// ## Examples
///
/// ```
/// use lazuli_analyzer::conventions_unknown_suggestion;
///
/// assert_eq!(conventions_unknown_suggestion("crd"), Some("crud"));
/// assert_eq!(conventions_unknown_suggestion("xxxxxxx"), None);
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventions_unknown_suggestion_finds_typo() {
        assert_eq!(conventions_unknown_suggestion("crd"), Some("crud"));
        assert_eq!(conventions_unknown_suggestion("crue"), Some("crud"));
    }

    #[test]
    fn conventions_unknown_suggestion_returns_none_for_far_misses() {
        assert!(conventions_unknown_suggestion("absolutely-different").is_none());
    }
}
