#![doc = include_str!("lib.md")]
// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
mod agent;
mod auth;
mod auto_photo;
pub mod checks;
mod command;
mod command_decl;
mod conventions;
mod design;
mod errors;
mod events;
mod expr;
mod feature;
mod feature_meta;
mod helpers;
mod jobs;
mod lifecycle;
mod lzx;
mod plan_gate;
mod query;
pub mod rbac;
mod report;
mod resource;
mod resource_rate_limit;
mod resource_validators;
pub mod source_map;
mod surface;
pub mod symbol_origin;
mod test_lowering;
mod types;
mod workflow;

pub use agent::{lower_agent, parse_closed_predicate};
pub use auth::lower_auth;
pub(crate) use command_decl::{DeprecationTarget, lower_command_decl, lower_deprecated};
pub use conventions::{
    ConventionSynthDiagnostic, CrudSynthDiagnostic, build_owner_scope_cte_prefix_for_test,
    build_owner_scope_where_for_test, synthesize_conventions,
};
pub(crate) use conventions::{collect_crud_overlays, synthesize_conventions_with_overlays};
pub use design::lower_design;
pub use errors::{AnalyzeError, CONVENTION_CATALOG, conventions_unknown_suggestion};
pub use events::{
    lower_channel, lower_event_group, lower_mcp_server, lower_notification, lower_tenant_migration,
    lower_webhook,
};
pub use feature::{lower_feature_skeleton, resolve_ctx_convention};
pub(crate) use feature_meta::{
    lower_aggregate_decl, lower_api_decl, lower_defaults, lower_enum_decl,
    lower_feature_errors_decl, lower_invariant_decl, lower_public_contract, lower_record_decl,
    lower_translation_decl,
};
pub use jobs::{lower_job, lower_poller};
pub use lzx::{lower_lzx_document, lower_lzx_feature_surfaces};
pub use plan_gate::{
    PlanGateCode, PlanGateDiagnostic, PlanGateFacts, aggregate_plan_gate_facts,
    diagnose_plan_gate_facts, parse_subscription_anchor,
};
pub use surface::lower_surface;
pub use symbol_origin::build_symbol_origin_index;
#[cfg(test)]
pub(crate) use types::parse_cap_file_type;
pub use types::type_ref_from_syntax_public;
pub(crate) use types::{
    parse_cap_pii_type, parse_default, type_ref_from_syntax, type_ref_from_text,
};

use expr::{lower_qualified_name, lower_translation_key_ref};

use helpers::span_of;

use lazuli_ir as ir;
use lazuli_syntax as syntax;

// `lower_lzx_document` + `lower_surface` and the entire `.lzx`
// surface family moved to `lzx.rs` (app layer) and `surface.rs`
// (ViewModel layer).
//
// `type_ref_from_syntax` + `type_ref_from_text` + the `@cap.*`
// capability parsers (`@cap.File`, `@cap.PII`, `@cap.Hashed`,
// `@cap.Encrypted`, `@cap.E2ee`, `@cap.Token`), `@semantic.Money`
// parsing, `parse_default`, and the closed-catalog primitive-type
// match moved to `types.rs`.

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
            // GAP-09 — lower each predicate-gated atom's verbatim `when`
            // text through the shared closed-predicate entry point (same
            // machinery as `unique ... when` / `invariant when`). The atom
            // string passes through verbatim; doctor cross-checks the
            // predicate `input.*` refs against the consuming command.
            conditional_atoms: c
                .conditional_atoms
                .iter()
                .map(|ca| ir::ConditionalPolicyAtom {
                    atom: ca.atom.clone(),
                    when: agent::parse_closed_predicate(&ca.when),
                })
                .collect(),
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

/// Analyzer-level resolution for `Command.invalidates`. This pass is
/// intentionally module-scoped: same-feature refs were normalized during
/// per-feature lowering, but cross-feature refs can only be validated once
/// all feature IR is present.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::resolve_invalidates_targets;
/// use lazuli_ir::Module;
///
/// let mut module: Module = unimplemented!("obtain from analyzer pipeline");
/// resolve_invalidates_targets(&mut module)?;
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
pub fn resolve_invalidates_targets(module: &mut ir::Module) -> Result<(), AnalyzeError> {
    normalize_legacy_invalidates_targets(&mut module.features);
    validate_invalidates_targets(&module.features)
}

/// Spec 0014 GAP-1 — module-level re-resolution of `restrict on_delete`
/// guard scopes (`tenant_scoped` / `soft_delete`) against EVERY feature's
/// resources.
///
/// The per-feature pass (`resolve_restrict_on_delete_scopes`, run during
/// `lower_feature_skeleton`) only sees same-feature resources, so a guard
/// whose referencing relation lives in ANOTHER feature (e.g. a
/// `billing_config` guard referencing `customer_management.Customer`) never
/// gets its derived flags set. The emitted `EXISTS` would then OMIT
/// `AND tenant_id = $N` — a CROSS-TENANT breach (the guard would probe every
/// tenant's rows) — and `AND deleted_at IS NULL` (soft-deleted rows would
/// still block deletion).
///
/// This pass rebuilds the `relation → (tenant_scoped, soft_delete)` index
/// from the resources of ALL features in the module and re-applies it to
/// every guard, so cross-feature relations resolve. It is intentionally
/// module-scoped — same shape as [`resolve_invalidates_targets`], which also
/// can only validate cross-feature command/query refs once all feature IR is
/// present. Idempotent: same-feature guards already carry the correct flags
/// from the per-feature pass; re-applying the (now-global) index is a no-op
/// for them.
///
/// Resolution is purely by relation name (snake-cased) across the whole
/// module. The module's `uses` graph is not consulted: a guard's referencing
/// relation is the table that carries the FK, and two features can only share
/// a table name by referring to the same resource, so a global by-name index
/// is the correct resolution surface (mirrors how the migration emitter and
/// FK topo-sort already treat resource names as module-global).
pub fn resolve_restrict_on_delete_scopes_module(module: &mut ir::Module) {
    let scopes: std::collections::HashMap<String, (bool, bool)> = module
        .features
        .iter()
        .flat_map(|feature| feature.resources.iter())
        .map(resource::restrict_on_delete_scope_entry)
        .collect();
    for feature in &mut module.features {
        resource::apply_restrict_on_delete_scopes(&mut feature.resources, &scopes);
    }
}

/// Module-level validation that every `Command.invalidates` target
/// names a query that actually exists on the referenced feature.
///
/// Called by [`resolve_invalidates_targets`] after the legacy-form
/// normalization pass; exposed publicly so the doctor/inspect CLIs can
/// run validation on already-normalized IR without re-walking.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::validate_invalidates_targets;
/// use lazuli_ir::Feature;
///
/// let features: Vec<Feature> = vec![];
/// validate_invalidates_targets(&features)?;
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
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
        materialize: None,
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
