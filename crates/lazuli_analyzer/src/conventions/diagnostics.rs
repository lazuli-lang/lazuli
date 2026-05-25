//! §11 diagnostic codes emitted by `synthesize_conventions`.
//!
//! Cell C4 / M3 formats these into user-facing strings; Cell C3 just
//! records them on the per-pass `Vec<CrudSynthDiagnostic>` return value.
//!
//! ## Layout
//!
//! * `ConventionSynthDiagnostic` — the canonical enum. Originally
//!   `CrudSynthDiagnostic`; extended in M2 to cover both `crud` and
//!   `me` bundles (variants prefixed `Me*`). Type alias preserved.
//! * `diagnostic_code` / `severity` — the wire-level identifiers.
//!   Cell C4 reads these to pick the user-facing format string and
//!   severity rendering.
//!
//! Per crud §11 / me §11 / owner-scope §11.1 the catalog is closed
//! and additive — new variants land here when a new convention
//! bundle introduces a new failure mode (RULE-VOCAB-03: the synth
//! pass is authoring-time dispatch; the emitted IR is branchless).

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
