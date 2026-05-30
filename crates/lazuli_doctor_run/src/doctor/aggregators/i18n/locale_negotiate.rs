//! `locale_negotiate` block validator + its closed-catalog tables.
//!
//! Powers two doctor codes: `locale_negotiate_source_invalid` and
//! `locale_negotiate_strategy_invalid`. Also re-emits the shared
//! `app_locale_fallback_unknown_dest` when the negotiate block's
//! fallback tag is missing from `app.locale.supported`.
//!
//! Lifted out of the `i18n` god-file in the rails-style R9 split.

use std::collections::BTreeSet;
use std::path::Path;

use crate::doctor::{DoctorDiagnostic, DoctorSeverity};

pub(super) const LOCALE_NEGOTIATE_SOURCES: &[&str] = &[
    "accept_language",
    "query_param",
    "cookie",
    "user_profile",
    "subdomain",
];

pub(super) const LOCALE_NEGOTIATE_STRATEGIES: &[&str] =
    &["best_match", "prefix_match", "exact_match"];

pub(super) const CLDR_PLURAL_ARMS: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Emits `locale_negotiate_source_invalid`,
/// `locale_negotiate_strategy_invalid`, and reuses
/// `app_locale_fallback_unknown_dest` when the fallback tag is missing.
pub(super) fn check_locale_negotiate(
    ln: &lazuli_ir::LocaleNegotiate,
    supported: &BTreeSet<String>,
    path: &Path,
    line: usize,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    if let Some(source) = &ln.source
        && !LOCALE_NEGOTIATE_SOURCES.contains(&source.as_str())
    {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "locale_negotiate_source_invalid".to_owned(),
            message: format!(
                "`locale_negotiate.source` `{}` must be one of: {}.",
                source,
                LOCALE_NEGOTIATE_SOURCES.join(", ")
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    if let Some(strategy) = &ln.strategy
        && !LOCALE_NEGOTIATE_STRATEGIES.contains(&strategy.as_str())
    {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "locale_negotiate_strategy_invalid".to_owned(),
            message: format!(
                "`locale_negotiate.strategy` `{}` must be one of: {}.",
                strategy,
                LOCALE_NEGOTIATE_STRATEGIES.join(", ")
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    if let Some(fallback) = &ln.fallback
        && !supported.is_empty()
        && !supported.contains(fallback)
    {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "app_locale_fallback_unknown_dest".to_owned(),
            message: format!(
                "`locale_negotiate.fallback` `{}` is not in `app.locale.supported`.",
                fallback
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}
