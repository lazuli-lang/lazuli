//! i18n bucket cycle aggregator (row 54).
//!
//! Surfaces up to 15 locale/translation diagnostics across the app
//! manifest's `app.locale` block, per-runtime-unit + per-api
//! `locale_negotiate` overlays, and each feature's `translation` block:
//!
//! App-level (anchored at `app.lzi`):
//! - `app_locale_default_unsupported`      — error    (`app.locale.default` not in supported)
//! - `app_locale_fallback_unknown_source`  — error    (`fallback from -> to` source unknown)
//! - `app_locale_fallback_unknown_dest`    — error    (`fallback from -> to` dest unknown)
//! - `app_locale_fallback_cycle`           — error    (fallback chain forms a cycle)
//!
//! Per-`locale_negotiate` block (mounted on `app.runtime` or per-`api`):
//! - `locale_negotiate_source_invalid`     — error    (`source` not in closed catalog)
//! - `locale_negotiate_strategy_invalid`   — error    (`strategy` not in closed catalog)
//! - reuse of `app_locale_fallback_unknown_dest`      (fallback locale not supported)
//!
//! Per-feature `translation` (anchored at the translation block):
//! - `translation_catalog_path_missing`    — warning  (path lacks `<locale>` placeholder)
//! - `translation_locale_unsupported`      — error    (variant locale outside `supported`)
//! - `translation_locale_missing_for_default`         — error  (no variant for default)
//! - `translation_locale_missing_for_supported`       — warning (no variant for supported)
//! - `cldr_plural_arm_invalid`             — error    (plural arm outside CLDR catalog)
//! - `rule_message_ref_unresolved`         — error    (`@translation.<key>` does not resolve)
//! - `translation_key_unused`              — warning  (declared key never referenced)
//! - `notification_template_placeholder_unknown`     — error  (template uses `<locale>` but no negotiate mounted)
//!
//! Catalog filesystem checks (`translation_catalog_path_missing`
//! filesystem variant) are deferred to `lazuli translate extract --check`
//! — doctor does not touch the filesystem here.
//!
//! See `docs/proposals/bucket-i18n-cycle.md` §Doctor/LSP for the full
//! 15-rule closed catalog and per-rule rationale.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::{
    DoctorAppManifest, DoctorDiagnostic, DoctorFile, DoctorSeverity, Tier3FeatureFacts,
};

mod locale_negotiate;

use locale_negotiate::{CLDR_PLURAL_ARMS, check_locale_negotiate};

/// Aggregate every i18n finding into the canonical `DoctorDiagnostic`
/// envelope. Returns an empty vec when no `app.lzi` is loaded (the rules
/// all key off the app's locale catalog).
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    app: Option<&DoctorAppManifest>,
    files: &[DoctorFile],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let Some(app_facts) = app else {
        return diagnostics;
    };
    let app_locale = app_facts.manifest.locale.as_ref();
    let supported: BTreeSet<String> = app_locale
        .map(|l| l.supported.iter().cloned().collect())
        .unwrap_or_default();
    let default_locale = app_locale.map(|l| l.default.as_str()).unwrap_or("");
    let app_path = app_facts.path.clone();

    // ---- App-level: locale default / supported / fallback ----
    if let Some(locale) = app_locale {
        // `app_locale_default_unsupported`.
        if !locale.default.is_empty() && !supported.contains(&locale.default) {
            diagnostics.push(DoctorDiagnostic {
                path: app_path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_locale_default_unsupported".to_owned(),
                message: format!(
                    "`app.locale.default` `{}` must appear in `supported`.",
                    locale.default
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // Build adjacency for cycle + unknown-tag checks.
        let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for fb in &locale.fallbacks {
            if !supported.contains(&fb.from) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_locale_fallback_unknown_source".to_owned(),
                    message: format!(
                        "fallback `{} -> {}` source `{}` is not in `app.locale.supported`.",
                        fb.from, fb.to, fb.from
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            if !supported.contains(&fb.to) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_locale_fallback_unknown_dest".to_owned(),
                    message: format!(
                        "fallback `{} -> {}` destination `{}` is not in `app.locale.supported`.",
                        fb.from, fb.to, fb.to
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            graph
                .entry(fb.from.clone())
                .or_default()
                .push(fb.to.clone());
        }

        // Cycle detection via DFS.
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut on_stack: BTreeSet<String> = BTreeSet::new();
        let mut found_cycle: Option<String> = None;
        let nodes: Vec<String> = graph.keys().cloned().collect();
        for start in nodes {
            if found_cycle.is_some() {
                break;
            }
            if visited.contains(&start) {
                continue;
            }
            // Iterative DFS with path stack.
            let mut stack: Vec<(String, usize)> = vec![(start.clone(), 0)];
            on_stack.insert(start.clone());
            visited.insert(start.clone());
            while let Some((node, idx)) = stack.last().cloned() {
                let nbrs = graph.get(&node).cloned().unwrap_or_default();
                if idx >= nbrs.len() {
                    on_stack.remove(&node);
                    stack.pop();
                    continue;
                }
                if let Some(top) = stack.last_mut() {
                    top.1 = idx + 1;
                }
                let next = nbrs[idx].clone();
                if on_stack.contains(&next) {
                    found_cycle = Some(format!(
                        "{} -> {} -> ... -> {}",
                        stack
                            .iter()
                            .map(|(n, _)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(" -> "),
                        next,
                        next
                    ));
                    break;
                }
                if !visited.contains(&next) {
                    visited.insert(next.clone());
                    on_stack.insert(next.clone());
                    stack.push((next, 0));
                }
            }
        }
        if let Some(cycle) = found_cycle {
            diagnostics.push(DoctorDiagnostic {
                path: app_path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_locale_fallback_cycle".to_owned(),
                message: format!("fallback chain creates a cycle: `{}`.", cycle),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // ---- Per-runtime-unit / per-api `locale_negotiate` ----
    for unit in &app_facts.manifest.runtime {
        if let Some(ln) = &unit.locale_negotiate {
            check_locale_negotiate(ln, &supported, &app_path, 1, &mut diagnostics);
        }
    }

    // ---- Per-feature translation ----
    for feature in facts {
        if let Some(api) = feature.apis.first() {
            // No-op placeholder: api-level locale_negotiate doctor rules
            // would attach here once the api facts surface line maps.
            // Today the rules use the api block itself; we walk
            // `feature.apis` instead.
            let _ = api;
        }
        for api in &feature.apis {
            if let Some(ln) = &api.locale_negotiate {
                check_locale_negotiate(
                    ln,
                    &supported,
                    &feature.path,
                    feature.feature_line,
                    &mut diagnostics,
                );
            }
        }

        let Some(translation) = &feature.translation else {
            continue;
        };
        let tline = feature.translation_line;

        if !translation.catalog.contains("<locale>") {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line: tline,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "translation_catalog_path_missing".to_owned(),
                message: format!(
                    "translation catalog path `{}` should contain a `<locale>` placeholder so the runtime can load per-locale files.",
                    translation.catalog
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // Build set of declared key names + referenced key set is below.
        let declared: BTreeSet<&str> = translation.keys.iter().map(|k| k.name.as_str()).collect();

        for key in &translation.keys {
            let mut variant_locales: BTreeSet<&str> = BTreeSet::new();
            for variant in &key.variants {
                variant_locales.insert(variant.locale.as_str());
                if !supported.contains(&variant.locale) && !supported.is_empty() {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: tline,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "translation_locale_unsupported".to_owned(),
                        message: format!(
                            "translation key `{}.{}` declares variant `{}` outside `app.locale.supported`.",
                            feature.feature, key.name, variant.locale
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
            if !default_locale.is_empty() && !variant_locales.contains(default_locale) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: tline,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "translation_locale_missing_for_default".to_owned(),
                    message: format!(
                        "translation key `{}.{}` is missing a variant for default locale `{}`.",
                        feature.feature, key.name, default_locale
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            for tag in &supported {
                if tag == default_locale {
                    continue;
                }
                if !variant_locales.contains(tag.as_str()) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: tline,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "translation_locale_missing_for_supported".to_owned(),
                        message: format!(
                            "translation key `{}.{}` is missing a variant for supported locale `{}`.",
                            feature.feature, key.name, tag
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
            for plural in &key.plurals {
                if !CLDR_PLURAL_ARMS.contains(&plural.arm.as_str()) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: tline,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cldr_plural_arm_invalid".to_owned(),
                        message: format!(
                            "translation key `{}.{}` plural arm `{}` is not a CLDR category: zero|one|two|few|many|other.",
                            feature.feature, key.name, plural.arm
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

        // `translation_key_unresolved`: any `@translation.<key>` in a
        // rule message that does not resolve to a declared key.
        // `translation_key_unused`: any declared key never referenced
        // anywhere in the feature.
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for command in &feature.commands {
            // commands today carry no rule-message slot; skip.
            let _ = command;
        }
        // Walk file source for `message @translation.<key>` references
        // since `Rule.message_ref` is exposed via the analyzer's lifted
        // Tier 4d resource rules; the legacy rule walker still owns the
        // file-local lift. Doctor uses text-pattern here to bridge the
        // gap until the rule lift lands. We read from the in-memory
        // package files first (so tests work without filesystem
        // round-trips) and fall back to the filesystem.
        let source = files
            .iter()
            .find(|f| f.path == feature.path)
            .map(|f| f.source.clone())
            .or_else(|| std::fs::read_to_string(&feature.path).ok());
        if let Some(text) = source {
            // 2026-05-27 — broadened from strip_prefix("message @translation.")
            // to any occurrence of `@translation.<key>` anywhere in the line.
            // The narrow walker missed the canonical `<binding_name> message
            // @translation.<key>` form used in `errors` blocks (where the
            // binding name precedes `message`), producing false-positive
            // `translation_key_unused` warnings for every key referenced
            // from an errors block. The wider walker captures both forms
            // without dropping the original semantics.
            for line in text.lines() {
                let mut rest = line;
                while let Some(pos) = rest.find("@translation.") {
                    let after = &rest[pos + "@translation.".len()..];
                    // Stop at first non-identifier char (matches the previous
                    // split_whitespace behavior plus snake_case boundary chars
                    // common in templates: ., ,, ), space, tab, etc).
                    let key_end = after
                        .find(|c: char| !c.is_alphanumeric() && c != '_')
                        .unwrap_or(after.len());
                    let key = &after[..key_end];
                    if !key.is_empty() {
                        referenced.insert(key.to_owned());
                        if !declared.contains(key) {
                            diagnostics.push(DoctorDiagnostic {
                                path: feature.path.clone(),
                                line: tline,
                                column: 1,
                                severity: DoctorSeverity::Error,
                                code: "rule_message_ref_unresolved".to_owned(),
                                message: format!(
                                    "`@translation.{}` in feature `{}` does not resolve. Declared keys: {}.",
                                    key,
                                    feature.feature,
                                    declared
                                        .iter()
                                        .copied()
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                        }
                    }
                    rest = &after[key_end..];
                }
            }
        }
        for key in &translation.keys {
            if !referenced.contains(&key.name) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: tline,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "translation_key_unused".to_owned(),
                    message: format!(
                        "translation key `{}.{}` is declared but never referenced via `@translation.<key>`.",
                        feature.feature, key.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // `notification_template_placeholder_unknown`: a notification
        // template path containing `<locale>` requires a mounted
        // `locale_negotiate`. The fixture authors templates via
        // `notification template "./outreach/...mjml"` in the IR
        // notifications slot; doctor checks each.
        let mount_count = app_facts
            .manifest
            .runtime
            .iter()
            .filter(|u| u.locale_negotiate.is_some())
            .count();
        for notification in &feature.notifications {
            if notification.template.contains("<locale>") && mount_count == 0 {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature
                        .notification_lines
                        .get(&notification.name)
                        .copied()
                        .unwrap_or(feature.feature_line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "notification_template_placeholder_unknown".to_owned(),
                    message: format!(
                        "notification `{}` template path contains `<locale>` but no `locale_negotiate` is mounted in `app.runtime`.",
                        notification.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // `translation_catalog_path_missing` (filesystem check) is
        // deferred to `lazuli translate extract --check`. Doctor does
        // not touch the filesystem.
    }

    diagnostics
}
