//! Per-file `.lzi` processor for `DoctorPackage::load`.
//!
//! Walks one `DoctorFile` whose path is a `.lzi` source and pushes
//! every Tier-3 fact family into the shared `LoadAccumulator`:
//!
//! - Workspace / app / registry / contracts manifests (with WS-001 /
//!   APP-001 / REG-001 duplicate-declaration diagnostics).
//! - Cut A agent IR + per-feature symbol tables + registry tool
//!   defects.
//! - Per-feature semantic-type diagnostics (MONEY-1, VOCAB-TESTS-MISSING-001,
//!   Wave 1 test-discipline) emitted into `file.local_diagnostics`.
//! - Tier-3 fact harvest (`Tier3FeatureFacts` row per feature with
//!   non-empty body).
//! - Command policy map, resource field map, command/job
//!   external-call edges populated from IR.
//! - Approval presences, feature adapters, feature uses, app profiles,
//!   canonical operational facts collected through their existing
//!   walker helpers.

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_manifest::app_manifest::{
    RegistryParseOutput, parse_app_contracts, parse_app_manifest, parse_app_profiles,
    parse_app_registry_with_defects, parse_app_workspace,
};
use lazuli_syntax::parse_feature_skeletons;

use super::super::{
    AgentFacts, AuthFacts, DoctorAppContract, DoctorAppManifest, DoctorAppProfile,
    DoctorAppRegistry, DoctorAppWorkspace, DoctorDiagnostic, DoctorFile, DoctorSeverity,
    RegistryToolDefect, aggregators, collect_approval_block_presence, collect_auth_anchors,
    collect_canonical_facts, collect_feature_adapters, collect_feature_uses, line_col_for_offset,
    money_arithmetic_001_diagnostics, money_compare_001_diagnostics,
    populate_command_external_calls_from_ir, populate_commands_from_ir,
    populate_feature_resources_from_ir, populate_job_external_calls_from_ir,
    semantic_type_unknown_diagnostics_for_feature,
    semantic_type_unknown_diagnostics_for_syntax_feature, vocab_tests_missing_001_diagnostics,
};
use super::state::{LoadAccumulator, LoadContext};
use super::tier3_harvest::harvest_tier3_facts;

pub(super) fn process_lzi_file(
    file: &mut DoctorFile,
    acc: &mut LoadAccumulator,
    ctx: &LoadContext<'_>,
) {
    acc.contracts.extend(
        parse_app_contracts(&file.source)
            .into_iter()
            .map(|manifest| DoctorAppContract {
                path: file.path.clone(),
                manifest,
            }),
    );
    if let Some(manifest) = parse_app_workspace(&file.source) {
        if acc.workspace.is_none() {
            acc.workspace = Some(DoctorAppWorkspace {
                path: file.path.clone(),
                manifest,
            });
        } else {
            file.local_diagnostics.push(DoctorDiagnostic {
                path: file.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-001".to_owned(),
                message: "package should declare at most one workspace contract.".to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
    if let Some(manifest) = parse_app_manifest(&file.source) {
        if acc.app.is_none() {
            acc.app = Some(DoctorAppManifest {
                path: file.path.clone(),
                source: file.source.clone(),
                manifest,
            });
        } else {
            file.local_diagnostics.push(DoctorDiagnostic {
                path: file.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-001".to_owned(),
                message: "package should declare exactly one app manifest entrypoint.".to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
    let RegistryParseOutput {
        registry: parsed_registry,
        tool_defects,
    } = parse_app_registry_with_defects(&file.source);
    if let Some(manifest) = parsed_registry {
        if acc.registry.is_none() {
            acc.registry = Some(DoctorAppRegistry {
                path: file.path.clone(),
                manifest,
            });
        } else {
            file.local_diagnostics.push(DoctorDiagnostic {
                path: file.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "REG-001".to_owned(),
                message: "package should declare at most one registry manifest.".to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
    acc.registry_tool_defects
        .extend(tool_defects.into_iter().map(|defect| RegistryToolDefect {
            path: file.path.clone(),
            line: defect.line,
            name: defect.name,
            reason: defect.reason,
        }));

    process_feature_skeletons(file, acc, ctx);

    collect_approval_block_presence(file, &mut acc.approval_presences);
    collect_feature_adapters(file, &mut acc.feature_adapters);
    collect_feature_uses(file, &mut acc.feature_uses);
    acc.profiles
        .extend(
            parse_app_profiles(&file.source)
                .into_iter()
                .map(|profile| DoctorAppProfile {
                    path: file.path.clone(),
                    profile,
                }),
        );
    collect_canonical_facts(file, &mut acc.operational);
}

fn process_feature_skeletons(
    file: &mut DoctorFile,
    acc: &mut LoadAccumulator,
    ctx: &LoadContext<'_>,
) {
    // Cut A — agent IR collection + feature symbol scan.
    match parse_feature_skeletons(&file.source) {
        Ok(features) => {
            for skeleton in &features {
                match lower_feature_skeleton(skeleton) {
                    Ok(feature) => {
                        let header_line = line_col_for_offset(&file.source, skeleton.span.start).0;
                        process_lowered_feature(file, acc, ctx, skeleton, feature, header_line);
                    }
                    Err(error) => {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: line_col_for_offset(&file.source, skeleton.span.start).0,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "agent_lower_failed_diagnostics".to_owned(),
                            message: format!("agent lowering failed: {error}"),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
            }
        }
        Err(error) => {
            file.local_diagnostics.push(DoctorDiagnostic {
                path: file.path.clone(),
                line: line_col_for_offset(&file.source, error.span().start).0,
                column: line_col_for_offset(&file.source, error.span().start).1,
                severity: DoctorSeverity::Error,
                code: "agent_parse_failed_diagnostics".to_owned(),
                message: error.to_string(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
}

fn process_lowered_feature(
    file: &mut DoctorFile,
    acc: &mut LoadAccumulator,
    ctx: &LoadContext<'_>,
    skeleton: &lazuli_syntax::FeatureSkeleton,
    feature: lazuli_ir::Feature,
    header_line: usize,
) {
    let semantic_type_diagnostics =
        semantic_type_unknown_diagnostics_for_feature(&file.path, &file.source, &feature);
    file.local_diagnostics.extend(semantic_type_diagnostics);
    let semantic_type_surface_diagnostics =
        semantic_type_unknown_diagnostics_for_syntax_feature(&file.path, &file.source, skeleton);
    file.local_diagnostics
        .extend(semantic_type_surface_diagnostics);
    // MONEY-1 §3.2 — currency-tagged Money doctor checks. Severity
    // is fixed at `Error` regardless of security profile because
    // mixed-currency arithmetic / comparison is a structural bug
    // (loses money silently), not a style nit.
    file.local_diagnostics
        .extend(money_compare_001_diagnostics(&file.path, &feature));
    file.local_diagnostics
        .extend(money_arithmetic_001_diagnostics(&file.path, &feature));
    // Wave 0 — wire VOCAB-TESTS-MISSING-001 through `DoctorPackage::load`'s
    // per-feature loop. The detector has existed since 2026-05-15 but was
    // never invoked from any dispatcher;
    // see Issue Zero of `docs/proposals/tdd-bdd-first-2026-05-23.md`.
    file.local_diagnostics
        .extend(vocab_tests_missing_001_diagnostics(
            &file.path,
            &feature,
            header_line,
            ctx.security_profile,
        ));
    // Wave 1 — test-discipline + adjacent runtime/migration lints.
    // Seven rules dispatched per-feature; the rule modules live in
    // `lazuli_doctor::test_discipline`. Resolve handler app_root from
    // manifest (defaults to project_root when manifest absent or
    // [lazurite].app_dir unset; handler rules gracefully return empty
    // when path doesn't exist).
    let app_root_for_handlers = ctx
        .lazurite_manifest
        .as_ref()
        .map(|m| m.app_root(&ctx.project_root))
        .unwrap_or_else(|| ctx.project_root.clone());
    // W1.5 — resolve [doctor.test_discipline].preset once per feature
    // loop. Under `tdd-iron-hand`, every TEST-* / DOCTOR-* /
    // MIGRATION-* / RUNTIME-* rule fires at Error regardless of
    // profile.
    //
    // v2 — sourced from the caller-supplied severity `config` (CLI: disk;
    // LSP: unsaved buffer-first) rather than the on-disk manifest, so
    // unsaved `[doctor.test_discipline] preset` edits drive in-editor
    // test-discipline severity. Byte-identical for the CLI (its config is
    // built from the same on-disk preset).
    let test_discipline_preset = ctx.config.test_discipline_preset;
    file.local_diagnostics
        .extend(aggregators::test_discipline::diagnostics(
            &file.path,
            &ctx.project_root,
            &app_root_for_handlers,
            &feature,
            &file.source,
            ctx.security_profile,
            test_discipline_preset,
        ));
    // Tier 3 facts harvest — done before `feature.agents` is consumed
    // below. Migrations bucket cycle Route C — resource/field rename
    // facts harvested from the IR's `previous_names` slots.
    harvest_tier3_facts(file, acc, skeleton, &feature, header_line);
    // Phase L Tier 4 follow-up — populate the command policy/route map
    // from the lifted IR instead of the text-walker. Mirrors the
    // legacy `collect_feature_commands` contract: only commands with
    // `policy` are inserted.
    populate_commands_from_ir(&feature, &mut acc.commands);
    // Phase L Tier 4 follow-up — populate the resource field map from
    // typed IR. Replaces `collect_feature_resources` for `auth_*`
    // cross-checks.
    populate_feature_resources_from_ir(
        &file.path,
        &file.source,
        &feature,
        &mut acc.feature_resources,
    );
    // Phase L Tier 4 follow-up — emit the typed command + job
    // `external_calls` facts (replaces the retired
    // `collect_external_calls_in_block`).
    populate_command_external_calls_from_ir(file, &feature, &mut acc.operational);
    populate_job_external_calls_from_ir(file, &feature, &mut acc.operational);
    for agent in feature.agents {
        let agent_line = agent
            .span_ref
            .as_ref()
            .map(|s| line_col_for_offset(&file.source, s.start).0)
            .unwrap_or(header_line);
        acc.agents.push(AgentFacts {
            feature: feature.name.clone(),
            agent,
            path: file.path.clone(),
            line: agent_line,
        });
    }
    if let Some(auth) = feature.auth {
        let auth_line = auth
            .span_ref
            .as_ref()
            .map(|s| line_col_for_offset(&file.source, s.start).0)
            .unwrap_or(header_line);
        let anchors = collect_auth_anchors(&file.source, auth_line);
        acc.auth_facts.push(AuthFacts {
            feature: feature.name.clone(),
            auth,
            path: file.path.clone(),
            line: auth_line,
            identity_line: anchors.identity_line,
            password_line: anchors.password_line,
            password_algorithm_line: anchors.password_algorithm_line,
            sessions_line: anchors.sessions_line,
            sessions_resource_line: anchors.sessions_resource_line,
            mfa_line: anchors.mfa_line,
            oauth_lines: anchors.oauth_lines,
        });
    }
}
