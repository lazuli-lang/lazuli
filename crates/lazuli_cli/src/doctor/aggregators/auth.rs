//! Auth cross-feature aggregator — emits the four `auth_*` IR-driven
//! diagnostics that cross-check `feature.auth` against its identity
//! resource, session resource, password storage cap, and OAuth adapter
//! bindings.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 4.
//! Helpers (`cap_hashed_algorithm`, `is_identity_shaped`,
//! `resolve_resource_for_feature`) move with the aggregator since
//! they have no other consumers.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::parsers::auth_session_ttl_seconds;
use crate::doctor::{
    AuthFacts, DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity, ResourceFact,
    ResourceFieldFact,
};

/// Public entrypoint called by the doctor dispatcher.
pub(crate) fn diagnostics(
    auth_facts: &[AuthFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_adapters: &BTreeMap<String, BTreeSet<String>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    auth_diagnostics(auth_facts, feature_resources, feature_adapters, feature_uses, registry)
}

/// Phase L Tier 4 follow-up — read the `algorithm:<X>` axis out of a
/// typed `CapabilityRef::Hashed(...)`. Returns `None` when the field is
/// not a `@cap.Hashed` decorator. Replaces the text-walking version
/// that re-parsed `@cap.Hashed(algorithm:…)` from `type_text`.
pub(super) fn cap_hashed_algorithm(type_ref: &lazuli_ir::TypeRef) -> Option<&'static str> {
    match type_ref {
        lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::Hashed(h)) => {
            Some(match h.algorithm {
                lazuli_ir::HashAlgorithm::Argon2id => "argon2id",
                lazuli_ir::HashAlgorithm::Bcrypt => "bcrypt",
            })
        }
        _ => None,
    }
}

/// Phase L Tier 4 follow-up — typed `is_identity_shaped`. Identity
/// fields are either tagged `@semantic.Email` / `@semantic.Phone`,
/// declared as `ID`, or carry the typed `unique` axis. Rejects free-
/// form `Text` fields used as login identities.
pub(super) fn is_identity_shaped(field: &ResourceFieldFact) -> bool {
    use lazuli_ir::{BuiltinType, TypeRef};
    match &field.type_ref {
        TypeRef::Builtin(BuiltinType::SemanticEmail | BuiltinType::SemanticPhone) => true,
        TypeRef::Builtin(BuiltinType::Id) => true,
        _ => field.unique,
    }
}

/// Resolve `<Resource>` for a feature by searching its own resources
/// first, then falling back to resources declared in features it
/// `uses`. Returns the first hit.
pub(crate) fn resolve_resource_for_feature<'a>(
    feature: &str,
    resource_name: &str,
    feature_resources: &'a BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Option<&'a ResourceFact> {
    if let Some(local) = feature_resources
        .get(feature)
        .and_then(|m| m.get(resource_name))
    {
        return Some(local);
    }
    if let Some(uses) = feature_uses.get(feature) {
        for dep in uses {
            if let Some(hit) = feature_resources
                .get(dep)
                .and_then(|m| m.get(resource_name))
            {
                return Some(hit);
            }
        }
    }
    None
}

/// Emit the four `auth_*` cross-feature diagnostics. Each diagnostic
/// is anchored at the offending subblock line; the `auth` header is
/// only used as a fallback.
pub(super) fn auth_diagnostics(
    auth_facts: &[AuthFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_adapters: &BTreeMap<String, BTreeSet<String>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let registry_integrations: BTreeSet<String> = registry
        .map(|r| {
            r.manifest
                .integrations
                .iter()
                .map(|i| i.name.clone())
                .collect()
        })
        .unwrap_or_default();

    for fact in auth_facts {
        let feature = fact.feature.as_str();

        // 1. `auth_identity_field_unknown` — resource and field must
        //    resolve in the same feature (or one it `uses`), and the
        //    field must be identity-shaped.
        let identity_resource = fact.auth.identity.field.resource.name.as_str();
        let identity_field = fact.auth.identity.field.field.as_str();
        let identity_resource_fact = resolve_resource_for_feature(
            feature,
            identity_resource,
            feature_resources,
            feature_uses,
        );
        match identity_resource_fact {
            None => diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.identity_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "auth_identity_field_unknown".to_owned(),
                message: format!(
                    "auth.identity `{identity_resource}.{identity_field}` does not resolve: resource not found in feature `{feature}`.",
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
            Some(resource) => match resource.fields.get(identity_field) {
                None => diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.identity_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_identity_field_unknown".to_owned(),
                    message: format!(
                        "auth.identity `{identity_resource}.{identity_field}` does not resolve: field not found on `{identity_resource}`.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }),
                Some(field) => {
                    if !is_identity_shaped(field) {
                        diagnostics.push(DoctorDiagnostic {
                            path: fact.path.clone(),
                            line: fact.identity_line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "auth_identity_field_unknown".to_owned(),
                            message: format!(
                                "auth.identity `{identity_resource}.{identity_field}` does not resolve: field is not identity-shaped (missing @semantic.Email / @semantic.Phone / unique).",
                            ),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
            },
        }

        // 2. `auth_password_no_session` — password login without an
        //    `auth.sessions` block can validate credentials but cannot
        //    issue durable sessions.
        if fact.auth.password.is_some() && fact.auth.sessions.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.password_line.unwrap_or(fact.line),
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "auth_password_no_session".to_owned(),
                message:
                    "auth.password is declared but auth.sessions is missing; login will not issue sessions."
                        .to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // 3. `auth_oauth_no_password_alt` — OAuth-only signin is a
        //    valid contract, but many apps want password fallback for
        //    break-glass administration.
        if !fact.auth.oauth.is_empty() && fact.auth.password.is_none() {
            let line = fact
                .auth
                .oauth
                .first()
                .and_then(|provider| fact.oauth_lines.get(provider.provider.as_str()).copied())
                .unwrap_or(fact.line);
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Info,
                code: "auth_oauth_no_password_alt".to_owned(),
                message:
                    "auth.oauth is declared without auth.password; signin is OAuth-only with no password fallback for break-glass access."
                        .to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // 4. `auth_sessions_resource_unknown` — sessions resource must
        //    resolve in the same feature (or one it `uses`).
        if let Some(sessions) = fact.auth.sessions.as_ref() {
            let sessions_name = sessions.resource.name.as_str();
            let resolved = resolve_resource_for_feature(
                feature,
                sessions_name,
                feature_resources,
                feature_uses,
            );
            if resolved.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.sessions_resource_line.unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_sessions_resource_unknown".to_owned(),
                    message: format!(
                        "auth.sessions.resource `{sessions_name}` does not name a resource declared in feature `{feature}`.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if auth_session_ttl_seconds(&sessions.ttl)
                .map(|seconds| seconds < 60 * 60)
                .unwrap_or(false)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.sessions_line.unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "auth_session_ttl_too_short".to_owned(),
                    message: "session TTL <1h forces frequent re-login; ensure intentional."
                        .to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            // AUTH-SESSION-TENANT-001 — every extra column must map to
            // `lazuli.ID`; non-ID Go types cannot be tenant-pinned by the
            // v1 shim.
            for col in &sessions.extra_columns {
                if col.go_type != "lazuli.ID" {
                    diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.sessions_resource_line.unwrap_or(fact.line),
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "AUTH-SESSION-TENANT-001".to_owned(),
                        message: format!(
                            "session resource `{sessions_name}` extra column `{}` has Go type `{}` but only `lazuli.ID` is allowed; declare the field as a resource reference.",
                            col.field_name, col.go_type,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            // AUTH-SESSION-EXTRA-001 — more than one extra column means
            // the generated shim has positional parameters whose order
            // matches DSL declaration; reordering silently changes tenant
            // scope.
            if sessions.extra_columns.len() > 1 {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.sessions_resource_line.unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "AUTH-SESSION-EXTRA-001".to_owned(),
                    message: format!(
                        "session resource `{sessions_name}` declares {} extra columns; v1 emits them positionally in DSL order — reordering silently changes tenant scope. Reduce to at most 1, or verify caller argument order carefully.",
                        sessions.extra_columns.len(),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // 5. `auth_password_algorithm_hash_mismatch` — when both
        //    `auth.password.algorithm` and the session resource carry
        //    a `@cap.Hashed(algorithm:…)` field, the two axes must
        //    match.
        if let (Some(password), Some(sessions)) =
            (fact.auth.password.as_ref(), fact.auth.sessions.as_ref())
        {
            let pw_algo = password.algorithm.trim();
            if !pw_algo.is_empty() {
                let sessions_name = sessions.resource.name.as_str();
                if let Some(resource) = resolve_resource_for_feature(
                    feature,
                    sessions_name,
                    feature_resources,
                    feature_uses,
                ) {
                    // Find the first hash-shaped field on the session
                    // resource that carries a `@cap.Hashed(...)`
                    // decorator. Multiple is allowed; we pin the
                    // first divergence.
                    let mut found_hash_axis = None;
                    for (field_name, field) in &resource.fields {
                        if let Some(axis) = cap_hashed_algorithm(&field.type_ref) {
                            found_hash_axis = Some((field_name.clone(), axis.to_owned()));
                            if axis != pw_algo {
                                diagnostics.push(DoctorDiagnostic {
                                    path: fact.path.clone(),
                                    line: fact
                                        .password_algorithm_line
                                        .unwrap_or(fact.password_line.unwrap_or(fact.line)),
                                    column: 1,
                                    severity: DoctorSeverity::Error,
                                    code: "auth_password_algorithm_hash_mismatch".to_owned(),
                                    message: format!(
                                        "auth.password.algorithm `{pw_algo}` must match `@cap.Hashed(algorithm:{pw_algo})` on the session resource's hash field (found `{axis}` on `{sessions_name}.{field_name}`).",
                                        pw_algo = pw_algo,
                                        axis = axis,
                                        sessions_name = sessions_name,
                                        field_name = field_name,
                                    ),
                                    category: None,
                                    feature_name: None,
                                    construct: None,
                                    fix: None,
                                    group: None,
                                });
                                break;
                            }
                        }
                    }
                    let _ = found_hash_axis;
                }
            }
        }

        // 6. `auth_oauth_adapter_unbound` — each oauth provider's
        //    adapter must resolve in the feature's `extensions
        //    adapter <name>` list or `registry.integrations`.
        let feature_adapter_names = feature_adapters.get(feature);
        for provider in &fact.auth.oauth {
            let adapter_ref = provider.adapter.as_str();
            let local_name = adapter_ref.strip_prefix("@adapter.").unwrap_or("");
            let in_feature = !local_name.is_empty()
                && feature_adapter_names
                    .map(|s| s.contains(local_name))
                    .unwrap_or(false);
            let in_registry = !local_name.is_empty() && registry_integrations.contains(local_name);
            if !in_feature && !in_registry {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact
                        .oauth_lines
                        .get(provider.provider.as_str())
                        .copied()
                        .unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_oauth_adapter_unbound".to_owned(),
                    message: format!(
                        "auth.oauth.`{provider}`.adapter `{adapter_ref}` is not declared in `extensions` of feature `{feature}` or `integrations` in `registry.lzi`.",
                        provider = provider.provider,
                        adapter_ref = adapter_ref,
                        feature = feature,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }
    diagnostics
}
