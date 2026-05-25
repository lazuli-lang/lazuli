//! IR Error-Vocab aggregator.
//!
//! Surfaces the seven `ERR-VOCAB-*` rules from
//! `lazuli_doctor::error_vocab` that walk each feature's typed
//! `errors` block, `policies` block, `commands`, and translation-key
//! catalog. Per `docs/proposals/ir-error-messages-vocab.md` §6.8:
//!
//! - `ERR-VOCAB-001`                    — warning  (policies authored, no `when_denied`)
//! - `ERR-VOCAB-002`                    — error    (unknown `@translation.<key>`)
//! - `ERR-VOCAB-003`                    — warning  (command falls back to built-in error)
//! - `ERR-VOCAB-CODE-UNKNOWN`           — error    (unknown error code in `errors` block)
//! - `ERR-VOCAB-EXPOSE-UNKNOWN`         — error    (`expose client <range>` not in catalog)
//! - `ERR-VOCAB-WHEN-DENIED-NO-POLICY`  — error    (`when_denied` references no policy)
//! - `ERR-VOCAB-EXPOSE-5XX-MESSAGE`     — error    (5xx code authored with a message)
//!
//! Cross-feature key resolution: `ERR-VOCAB-002` walks `Feature.uses` to
//! find translation keys declared in imported features. The aggregator
//! pre-computes a `BTreeMap<feature_name, BTreeSet<key_name>>` index
//! once per dispatch so the inner check is O(1) per key reference.
//!
//! Line anchoring goes through `span_line` over the loaded `DoctorFile`
//! list — each finding's `SpanRef` resolves to the matching source
//! offset, falling back to the feature header when no span is captured
//! (the typed errors block has no span, or the policy lift dropped it).
//!
//! See `docs/proposals/ir-error-messages-vocab.md` §6 for the full
//! catalog and per-rule rationale.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::{DoctorDiagnostic, DoctorFile, DoctorSeverity, Tier3FeatureFacts, span_line};

/// Aggregate every Error-Vocab finding across all Tier 3 features into
/// the canonical `DoctorDiagnostic` envelope. `files` is needed for
/// `SpanRef -> source line` resolution; without it the findings would
/// anchor at the feature header rather than the offending construct.
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    files: &[DoctorFile],
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::error_vocab::error_vocab;

    // Build the cross-feature translation-key index once. Maps each
    // feature name to the set of `@translation.<key>` it declares.
    let mut keys_by_feature: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for fact in facts {
        let mut declared = BTreeSet::new();
        if let Some(translation) = &fact.translation {
            for key in &translation.keys {
                declared.insert(key.name.clone());
            }
        }
        keys_by_feature
            .entry(fact.feature.clone())
            .or_default()
            .extend(declared);
    }

    let mut diagnostics = Vec::new();
    for fact in facts {
        let feature = make_synthetic_feature_for_error_vocab(fact);

        // ERR-VOCAB-001 — warning
        for finding in error_vocab::check_policies_no_when_denied(&feature, &fact.path) {
            let line = span_line(files, &fact.path, finding.policies_span, fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: error_vocab::PoliciesNoWhenDeniedFinding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ERR-VOCAB-002 — error
        for finding in
            error_vocab::check_translation_key_unknown(&feature, &fact.path, &keys_by_feature)
        {
            let line = span_line(files, &fact.path, finding.span, fact.feature_line);
            // Render the declared-keys list using the visible set
            // (same-feature + cross-feature `uses`).
            let declared: Vec<String> = visible_keys_for_message(&feature, &keys_by_feature);
            let declared_refs: Vec<&str> = declared.iter().map(String::as_str).collect();
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(&declared_refs),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: error_vocab::KeyUnknownFinding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ERR-VOCAB-003 — warning
        for finding in error_vocab::check_builtin_fallback(&feature, &fact.path) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or_else(|| span_line(files, &fact.path, finding.span, fact.feature_line));
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: error_vocab::BuiltinFallbackFinding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ERR-VOCAB-CODE-UNKNOWN — error
        for finding in error_vocab::check_code_unknown(&feature, &fact.path) {
            let line = span_line(files, &fact.path, finding.span, fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: error_vocab::CodeUnknownFinding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ERR-VOCAB-EXPOSE-UNKNOWN — error
        for finding in error_vocab::check_expose_unknown(&feature, &fact.path) {
            let line = span_line(files, &fact.path, finding.span, fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: error_vocab::ExposeUnknownFinding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ERR-VOCAB-WHEN-DENIED-NO-POLICY — error. Per-command findings
        // anchor at the command header (via `command_lines` lookup);
        // per-policy findings anchor at the policy span captured during
        // lowering, falling back to the feature header.
        for finding in error_vocab::check_when_denied_no_policy(&feature, &fact.path) {
            let line = match &finding.site {
                error_vocab::WhenDeniedSite::Command(name) => {
                    fact.command_lines.get(name).copied().unwrap_or_else(|| {
                        span_line(files, &fact.path, finding.span, fact.feature_line)
                    })
                }
                error_vocab::WhenDeniedSite::Policy(_) => {
                    span_line(files, &fact.path, finding.span, fact.feature_line)
                }
            };
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: error_vocab::WhenDeniedNoPolicyFinding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ERR-VOCAB-EXPOSE-5XX-MESSAGE — error
        for finding in error_vocab::check_expose_5xx_message(&feature, &fact.path) {
            let line = span_line(files, &fact.path, finding.span, fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: error_vocab::Expose5xxMessageFinding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
    diagnostics
}

/// IR Error-Vocab — render the visible-keys list (same-feature first,
/// then cross-feature via `uses`) for an `ERR-VOCAB-002` message body.
fn visible_keys_for_message(
    feature: &lazuli_ir::Feature,
    keys_by_feature: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    if let Some(translation) = &feature.translation {
        for key in &translation.keys {
            keys.insert(key.name.clone());
        }
    }
    for used in &feature.uses {
        if let Some(declared) = keys_by_feature.get(used) {
            for key in declared {
                keys.insert(key.clone());
            }
        }
    }
    keys.into_iter().collect()
}

/// IR Error-Vocab — synthesize a minimal `Feature` view from a
/// `Tier3FeatureFacts` so the typed `error_vocab::check_*` functions can
/// run without needing the doctor scaffolding. Only the slots the rules
/// read are populated; everything else stays default. Mirrors the
/// `make_synthetic_feature_for_reports` pattern.
fn make_synthetic_feature_for_error_vocab(fact: &Tier3FeatureFacts) -> lazuli_ir::Feature {
    lazuli_ir::Feature {
        name: fact.feature.clone(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: lazuli_ir::Defaults::default(),
        uses: fact.uses.clone(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: fact.policies.clone(),
        errors: fact.errors.clone(),
        commands: fact.commands.clone(),
        apis: fact.apis.clone(),
        records: Vec::new(),
        queries: Vec::new(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: fact.translation.clone(),
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        pollers: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: Vec::new(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}
