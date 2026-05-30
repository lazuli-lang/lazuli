//! OpenAPI deprecation + text-pattern API aggregator (row 48).
//!
//! Surfaces five closed-catalog deprecation rules across every feature's
//! commands and lifted APIs:
//!
//! - `deprecated-no-replacement`           — warning  (deprecated callable without `replacement`)
//! - `deprecated-replacement-unknown`      — error    (`replacement` target does not resolve)
//! - `deprecated_sunset_date_invalid`      — error    (sunset is not ISO-8601 `YYYY-MM-DD`)
//! - `deprecated-sunset-past`              — info     (sunset is before today)
//! - `openapi_text_pattern_api_block`      — warning  (api block still text-pattern, not lifted to typed IR)
//!
//! Resolution strategy: build a two-tier index (`commands_by_feature`,
//! `apis_by_feature`) once over the package, then walk each feature's
//! deprecated callables and resolve each `replacement` against the
//! matching index. The `today_pivot` is cached once per call so the
//! sunset-past check stays deterministic across a single dispatch.
//!
//! `api_changelog_breaking_change` is intentionally absent here — that
//! rule is only meaningful in the `lazuli docs changelog` pipeline where
//! a baseline OpenAPI snapshot exists to diff against. Doctor surfaces
//! a guard no-op for it.
//!
//! See `docs/proposals/bucket-openapi-cycle.md` §Doctor/LSP for the
//! closed catalog and per-rule rationale.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::parsers::{openapi_today_pivot, parse_iso_date};
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Aggregate every OpenAPI-deprecation finding across all Tier 3 features
/// into the canonical `DoctorDiagnostic` envelope.
pub(crate) fn diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let mut commands_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut apis_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for feature in facts {
        let command_set = commands_by_feature
            .entry(feature.feature.as_str())
            .or_default();
        for c in &feature.commands {
            command_set.insert(c.name.as_str());
        }
        let api_set = apis_by_feature.entry(feature.feature.as_str()).or_default();
        for api in &feature.apis {
            api_set.insert(api.name.as_str());
        }
    }

    let today_pivot = openapi_today_pivot();

    for feature in facts {
        for command in &feature.commands {
            let Some(dep) = &command.deprecated else {
                continue;
            };
            let line = feature
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or(feature.feature_line);
            deprecated_callable_diagnostics(
                &mut diagnostics,
                feature,
                "command",
                &command.name,
                line,
                dep,
                &commands_by_feature,
                &apis_by_feature,
                today_pivot,
            );
        }
        for api in &feature.apis {
            let Some(dep) = &api.deprecated else {
                continue;
            };
            let line = feature
                .api_lines
                .get(&api.name)
                .copied()
                .unwrap_or(feature.feature_line);
            deprecated_callable_diagnostics(
                &mut diagnostics,
                feature,
                "api",
                &api.name,
                line,
                dep,
                &commands_by_feature,
                &apis_by_feature,
                today_pivot,
            );
        }
    }

    // 4) `openapi_text_pattern_api_block` — surface once per unique
    // text-pattern api name across the package. The IR-lifted `Api`s
    // shadow text-pattern entries; subtract them so the warning only
    // fires for genuinely un-lifted authoring sites.
    let typed_api_names: BTreeSet<&str> = facts
        .iter()
        .flat_map(|f| f.apis.iter().map(|a| a.name.as_str()))
        .collect();
    let mut surfaced: BTreeSet<String> = BTreeSet::new();
    for feature in facts {
        for name in &feature.api_names_text_pattern {
            if typed_api_names.contains(name.as_str()) {
                continue;
            }
            if !surfaced.insert(name.clone()) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line: feature.feature_line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "openapi_text_pattern_api_block".to_owned(),
                message: format!(
                    "api `{}` is text-pattern; OpenAPI emission falls back to a stub with `x-lazuli-text-pattern-skip: true`. Lift to typed IR via Phase L Tier 4.",
                    name
                ),
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

#[allow(clippy::too_many_arguments)]
fn deprecated_callable_diagnostics(
    diagnostics: &mut Vec<DoctorDiagnostic>,
    feature: &Tier3FeatureFacts,
    kind: &str,
    name: &str,
    line: usize,
    dep: &lazuli_ir::Deprecation,
    commands_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    apis_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    today_pivot: (u16, u8, u8),
) {
    if dep.replacement.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "deprecated-no-replacement".to_owned(),
            message: format!(
                "{kind} `{name}` is deprecated without a replacement; declare `replacement {kind}.<name>` when a successor exists."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    } else if let Some(replacement) = &dep.replacement {
        match replacement {
            lazuli_ir::DeprecationReplacement::LocalCommand(target) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "command",
                    feature.feature.as_str(),
                    target,
                    commands_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::LocalApi(target) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "api",
                    feature.feature.as_str(),
                    target,
                    apis_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::Qualified(q) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "command",
                    q.feature.as_deref().unwrap_or(feature.feature.as_str()),
                    &q.name,
                    commands_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::QualifiedApi(q) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "api",
                    q.feature.as_deref().unwrap_or(feature.feature.as_str()),
                    &q.name,
                    apis_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::Url(url) => {
                let cleaned = url.trim();
                if !(cleaned.starts_with("http://") || cleaned.starts_with("https://"))
                    || cleaned.len() < "https://x".len()
                {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "deprecated-replacement-unknown".to_owned(),
                        message: format!(
                            "{kind} `{name}`.deprecated.replacement `{url}` does not resolve: url malformed."
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
    }

    if let Some(sunset) = &dep.sunset {
        match parse_iso_date(sunset) {
            None => diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "deprecated_sunset_date_invalid".to_owned(),
                message: format!(
                    "{kind} `{name}`.deprecated.sunset `{sunset}` is not a valid ISO-8601 date (`YYYY-MM-DD`)."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
            Some(date) if date < today_pivot => diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Info,
                code: "deprecated-sunset-past".to_owned(),
                message: format!(
                    "{kind} `{name}`.deprecated.sunset `{sunset}` is in the past; consumers should expect this endpoint to be removed soon."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
            Some(_) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_unknown_replacement_if_missing(
    diagnostics: &mut Vec<DoctorDiagnostic>,
    feature: &Tier3FeatureFacts,
    kind: &str,
    name: &str,
    line: usize,
    target_kind: &str,
    target_feature: &str,
    target_name: &str,
    index: &BTreeMap<&str, BTreeSet<&str>>,
) {
    let resolves = index
        .get(target_feature)
        .map(|set| set.contains(target_name))
        .unwrap_or(false);
    if resolves {
        return;
    }
    diagnostics.push(DoctorDiagnostic {
        path: feature.path.clone(),
        line,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "deprecated-replacement-unknown".to_owned(),
        message: format!(
            "{kind} `{name}`.deprecated.replacement `{target_feature}.{target_kind}.{target_name}` does not resolve."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    });
}
