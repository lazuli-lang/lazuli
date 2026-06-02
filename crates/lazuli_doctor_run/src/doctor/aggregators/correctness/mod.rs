//! Correctness aggregator.
//!
//! Dispatches every rule in `lazuli_doctor::correctness` that reads a
//! synthesized `&Feature` view — twelve closed-catalog checks covering
//! command/resource shape, event/notification payload typing, hook
//! targets, full-text indexing, composite keys, schema migration
//! presence, channel payload resolution, and webhook predicate fields.
//!
//! Each rule lives as its own module under
//! `crates/lazuli_doctor/src/correctness/` and returns a typed
//! `Finding`. This aggregator turns each `Finding` into the canonical
//! `DoctorDiagnostic` envelope and resolves the line anchor through the
//! per-feature `command_lines` / `webhook_lines` / `event_group_lines`
//! tables so the diagnostic surfaces at the offending construct, not
//! the feature header.
//!
//! Why a synthetic feature: the underlying correctness rules expect a
//! full `lazuli_ir::Feature` (records, commands, resources, etc.). The
//! Tier 3 fact bundle holds only the slices that doctor lifted; rebuilding
//! the minimal `Feature` shape here keeps the rule signatures uniform
//! across the two callers (the package-level dispatcher and per-feature
//! unit tests).
//!
//! Severity policy: every correctness rule is `Error`-by-default except
//! `record_column_storage` (info, gated behind the Strict profile so
//! production output stays signal-dense) and `schema_migration_present`
//! (warning, suppressed in single-file mode where there's no project
//! migration tree to compare against).
//!
//! See `docs/proposals/correctness-tier3.md` and per-rule docstrings in
//! `crates/lazuli_doctor/src/correctness/<rule>.rs` for the closed
//! catalog of correctness checks.

use std::path::Path;

use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts, correctness};

/// Aggregate every correctness finding across all Tier 3 features into
/// the canonical `DoctorDiagnostic` envelope.
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    registry: Option<&lazuli_ir::AppRegistry>,
    project_root: &Path,
    app_root: &Path,
    security_profile: SecurityProfile,
    skip_project_migration_check: bool,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Webhook envelope catalog (registry side) — `WEBHOOK-EMIT-PREDICATE-FIELD-001`
    // resolves predicate paths against the webhook's typed payload contract.
    let webhook_events: Vec<lazuli_ir::WebhookEvent> = registry
        .map(|r| r.webhook_events.clone())
        .unwrap_or_default();

    // Severity policy resolver for the three new "history-blind" /
    // signature-drift rules wired below. Per the proposals each is:
    //   - prototype   → info / warning (downgraded per rule),
    //   - strict      → warning,
    //   - production  → error.
    let migration_severity = match security_profile {
        SecurityProfile::Prototype => DoctorSeverity::Info,
        SecurityProfile::Strict => DoctorSeverity::Warning,
        SecurityProfile::Production | SecurityProfile::IronHand => DoctorSeverity::Error,
    };
    let handler_sig_severity = match security_profile {
        SecurityProfile::Prototype | SecurityProfile::Strict => DoctorSeverity::Warning,
        SecurityProfile::Production | SecurityProfile::IronHand => DoctorSeverity::Error,
    };

    // Codegen root holding `<dist_root>/go/<feature>/command.gen.go`.
    // Resolved relative to `project_root` because codegen always writes
    // into `<project_root>/dist`, regardless of where `app_root` points.
    let dist_root = project_root.join("dist");

    for fact in facts {
        let feature = make_synthetic_feature_for_correctness(fact);

        // CHANNEL-PAYLOAD-001 — error.
        for finding in correctness::channel_payload_unresolved_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::channel_payload_unresolved_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // CODEGEN-UNRESOLVED-BINDING-SOURCE-001 — error. A command-effect
        // binding RHS that resolves to no known source kind would silently
        // lower to a `FromConst("<raw>")` garbage string in the Go emitter;
        // this is the hard pre-emit diagnostic ("Cell I4").
        for finding in
            correctness::codegen_unresolved_binding_source_001::check(&feature, &fact.path)
        {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::codegen_unresolved_binding_source_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // RUNTIME-REACHABLE-STUB-001 — error. A DSL construct that lowers to
        // a runtime path which is a KNOWN not-implemented 501 stub: a
        // `target.<field>` source (→ `lazuli.FromTarget` → `sourceTarget` 501
        // in `runtime/go/lazuli/handle.go`) used in a command binding or query
        // filter, or a resource `retention ... then archive` policy (→
        // `ErrRetentionArchiveNotImplemented` in
        // `runtime/go/lazuli/retention.go`). The feature compiles and
        // `go build`s but returns 501 at the first request; this surfaces
        // "you used feature X but the runtime doesn't implement it yet" at
        // doctor time. Command findings anchor at the command line; query /
        // resource findings anchor at the feature header.
        for finding in correctness::runtime_reachable_stub_001::check(&feature, &fact.path) {
            let line = if finding.kind == "command" {
                fact.command_lines
                    .get(&finding.owner)
                    .copied()
                    .unwrap_or(fact.feature_line)
            } else {
                fact.feature_line
            };
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::runtime_reachable_stub_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // CTX-PATH-UNRESOLVED-001 — error. An author-written `ctx.<tail>`
        // binding whose tail is not a known ctx slot (per the SoT catalog
        // `runtime/go/lazuli/ctx_path_catalog.json`) would lower to
        // `lazuli.FromCtx("<tail>")` and 500 at runtime with
        // "unknown ctx path". This is the "author-tail" gap the ctx-path
        // face-parity harness admitted it could not cover.
        for finding in correctness::ctx_path_unresolved_001::check(&feature, &fact.path) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::ctx_path_unresolved_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // COMMAND-INPUT-SHADOWS-FIELD-001 — error.
        for finding in correctness::command_input_shadows_field_001::check(&feature, &fact.path) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::command_input_shadows_field_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // COMPOSITE-KEY-CONTRACT-001 — error.
        for finding in correctness::composite_key_contract_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::composite_key_contract_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // CODEGEN-GO-IDENT-COLLISION-008 — error. Two DSL constructs
        // (enum / lifecycle-generated-enum / query / command / transition)
        // that lower to the SAME exported Go identifier in the per-feature
        // `<feature>gen` package would produce a `go build` double
        // declaration. This pre-emit pass computes each construct's emitted
        // identifier (via the same acronym-aware caser the Go emitter uses)
        // and fires before codegen writes the broken Go. Anchored at the
        // feature header — the collision is feature-scoped and the fact
        // bundle carries no per-construct line table that spans all five
        // construct families.
        for finding in correctness::go_ident_collision_008::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::go_ident_collision_008::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // EVENT-GROUP-VARIANT-TYPE-001 — error.
        for finding in correctness::event_group_variant_type_001::check(&feature, &fact.path) {
            let line = fact
                .event_group_lines
                .get(&finding.group_pattern)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::event_group_variant_type_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // ENUM-VARIANT-UNDECLARED-001 — error. A filter-predicate RHS or
        // field-default enum literal naming a variant the enum never declared
        // would silently lower to a `FromConst("<typo>")` literal that never
        // matches; this rule rejects the typo before codegen.
        for finding in correctness::enum_variant_undeclared_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::enum_variant_undeclared_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // EVENT-OUTBOX-001 — error (payments-class only; the rule
        // self-gates on feature name).
        for finding in correctness::event_outbox_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::event_outbox_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // FULL-TEXT-TYPE-001 — error.
        for finding in correctness::full_text_type_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::full_text_type_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // HOOK-TARGET-001 — error.
        for finding in correctness::hook_target_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::hook_target_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // RESOURCE-LOCK-CONTRACT-001 — error.
        for finding in correctness::resource_lock_contract_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::resource_lock_contract_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // WEBHOOK-EMIT-PREDICATE-FIELD-001 — error. Needs the
        // registry-side `webhook_events` catalog to resolve typed
        // payload paths.
        for finding in correctness::webhook_emit_predicate_field_001::check(
            &feature,
            &webhook_events,
            &fact.path,
        ) {
            let line = fact
                .webhook_lines
                .get(&finding.webhook)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::webhook_emit_predicate_field_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // @info.record_column_jsonb — informational; only surfaced
        // under the strict profile so production profile output stays
        // signal-dense.
        if matches!(security_profile, SecurityProfile::Strict) {
            for finding in correctness::record_column_storage::check(&feature, &fact.path) {
                diagnostics.push(DoctorDiagnostic {
                    message: finding.message(),
                    path: finding.path,
                    line: fact.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Info,
                    code: correctness::record_column_storage::Finding::CODE.to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // @correctness.migration_out_of_sync — warning. Needs the
        // project root to locate `dist/go/migrations/`; single-file
        // doctor invocations and synthetic test packages have no
        // project migration tree to compare against.
        if !skip_project_migration_check {
            for finding in
                correctness::schema_migration_present::check(&feature, &fact.path, project_root)
            {
                diagnostics.push(DoctorDiagnostic {
                    message: finding.message(),
                    path: finding.path,
                    line: fact.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: correctness::schema_migration_present::Finding::CODE.to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            // MIGRATION-ALTER-MISSING-001 — per-feature. Detects IR
            // additions that no ALTER TABLE migration applies. Skipped
            // in single-file mode for the same reason
            // `schema_migration_present` is: there is no project
            // migration tree to compare against.
            for finding in
                correctness::migration_alter_missing_001::check(&feature, &fact.path, project_root)
            {
                diagnostics.push(DoctorDiagnostic {
                    message: finding.message(),
                    path: finding.path,
                    line: fact.feature_line,
                    column: 1,
                    severity: migration_severity,
                    code: correctness::migration_alter_missing_001::Finding::CODE.to_owned(),
                    category: None,
                    feature_name: Some(finding.feature),
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // HANDLER-SIGNATURE-MISMATCH-001 — per-feature. Detects drift
        // between hand-written handler Go signatures and codegen-emitted
        // `Command[I, O]`. Needs `app_root` to locate handler files and
        // `dist_root` to locate the codegen artifacts.
        for finding in correctness::handler_signature_mismatch_001::check(
            &feature, &fact.path, app_root, &dist_root,
        ) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: handler_sig_severity,
                code: correctness::handler_signature_mismatch_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: Some(finding.feature),
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // MIGRATION-IDEMPOTENT-CREATE-001 — once per project. Walks
    // `<project_root>/dist/go/migrations/` for non-baseline
    // `CREATE TABLE IF NOT EXISTS` clauses. Skipped in single-file
    // mode where no project migration tree exists.
    if !skip_project_migration_check {
        for finding in correctness::migration_idempotent_create_001::check(project_root) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: 1,
                column: 1,
                severity: migration_severity,
                code: correctness::migration_idempotent_create_001::Finding::CODE.to_owned(),
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

/// Build the synthetic `Feature` view the correctness rules expect.
///
/// Only the slices the correctness rules touch are populated from the
/// fact bundle — the rest stay default. The synthetic view is a
/// read-only adapter; the rules never mutate it.
pub(crate) fn make_synthetic_feature_for_correctness(
    fact: &Tier3FeatureFacts,
) -> lazuli_ir::Feature {
    // 2026-05-27 — synthesize defaults from the fact bundle (tenancy_axis
    // + defaults_timestamps). Without this, correctness rules see every
    // feature as tenancy=None / timestamps=false, which breaks rules like
    // `@correctness.migration_out_of_sync` that expect `org_id` in the
    // emitted column set for tenancy=Org resources.
    let synth_defaults = lazuli_ir::Defaults {
        tenancy: fact.tenancy_axis.as_deref().map(|axis| match axis {
            "org" => lazuli_ir::Tenancy::Org,
            "team" => lazuli_ir::Tenancy::Team,
            "none" => lazuli_ir::Tenancy::None,
            custom => lazuli_ir::Tenancy::Custom(custom.to_owned()),
        }),
        policy: fact.defaults_policy.clone(),
        timestamps: fact.defaults_timestamps,
        // 0004 — this synth Defaults is built from a doctor fact that does
        // not (yet) carry the rate_limit/audit hoist; the correctness
        // aggregator only consumes tenancy/timestamps/policy here.
        rate_limit: None,
        audit: None,
    };
    lazuli_ir::Feature {
        name: fact.feature.clone(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: synth_defaults,
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: fact.enums.clone(),
        resources: fact.resources.clone(),
        events: fact.events.clone(),
        rules: Vec::new(),
        policies: fact.policies.clone(),
        errors: fact.errors.clone(),
        commands: fact.commands.clone(),
        apis: fact.apis.clone(),
        records: fact.records.clone(),
        queries: fact.queries.clone(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: fact.jobs.clone(),
        webhooks: fact.webhooks.clone(),
        notifications: fact.notifications.clone(),
        event_groups: fact.event_groups.clone(),
        tenant_migrations: fact.tenant_migrations.clone(),
        translation: fact.translation.clone(),
        auth: None,
        surfaces: Vec::new(),
        extensions: fact.extensions.clone(),
        escape_routes: Vec::new(),
        agents: fact.agents.clone(),
        reports: fact.reports.clone(),
        pollers: Vec::new(),
        channels: fact.channels.clone(),
        caches: fact.caches.clone(),
        aggregates: fact.aggregates.clone(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}

mod rule_dispatchers;

pub(crate) use rule_dispatchers::{
    creates_empty_bindings_diagnostics, duplicate_query_name_diagnostics,
    missing_policy_on_query_diagnostics, mutation_without_readback_diagnostics,
    policy_ref_unresolved_diagnostics, route_id_effect_consistency_diagnostics,
    updates_missing_updated_at_diagnostics,
};
