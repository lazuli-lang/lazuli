//! ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001 — a Shape-C client gate
//! (`requires <feature>.lookup_my.<field>`) exists with no paired
//! server-side enforcement (SQL trigger named `enforce_*_<field>` in
//! `migrations/`, OR `policy` block with `requires_field "<field>"`
//! on a related command in the same feature).
//!
//! ## Rule statement
//!
//! Fires when any `requires_field` slot references `<field>` and the
//! supplied [`ServerSidePairIndex`] does not list `<field>` as
//! defense-in-depth-paired. The route guard is **UX-only by design**
//! (§5.5); without the paired server-side gate it looks like
//! security but isn't.
//!
//! Doctor takes the index as input so the rule is unit-testable
//! against synthetic migration / policy fixtures; the CLI builder
//! walks `migrations/*.sql` and the analyzer's `Policies` index to
//! populate the input.
//!
//! ## Severity profile
//!
//! Severity: `warning` at strict, `error` at production. The
//! `lazuli_doctor` severity table escalates at production.
//!
//! ## Trigger cue
//!
//! Cue: `requires_field.field` not in
//! [`ServerSidePairIndex::sql_triggered_fields`] AND not in
//! [`ServerSidePairIndex::policy_required_fields`].
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3 + §5.5.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, ViewGuard};

/// Cross-source index of server-side enforcement evidence for one
/// or more fields. Populated by the CLI doctor builder; tests may
/// supply it inline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerSidePairIndex {
    /// Field names that appear in a SQL trigger named
    /// `enforce_*_<field>` somewhere under `migrations/`. The doctor
    /// scans the trigger source's `CREATE TRIGGER` statements.
    pub sql_triggered_fields: BTreeSet<String>,
    /// Field names referenced by a `requires_field "<field>"` policy
    /// directive on any command in the same feature.
    pub policy_required_fields: BTreeSet<String>,
}

impl ServerSidePairIndex {
    /// Convenience constructor for tests — registers `field` as a
    /// SQL-trigger-enforced server-side pair.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_doctor::route_guard::field_missing_server_pair_001::ServerSidePairIndex;
    ///
    /// let idx = ServerSidePairIndex::with_sql_field("is_phone_verified");
    /// assert!(idx.covers("is_phone_verified"));
    /// ```
    pub fn with_sql_field(field: &str) -> Self {
        let mut idx = Self::default();
        idx.sql_triggered_fields.insert(field.into());
        idx
    }

    /// Convenience constructor for tests — registers `field` as a
    /// `requires_field "<field>"` policy companion.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_doctor::route_guard::field_missing_server_pair_001::ServerSidePairIndex;
    ///
    /// let idx = ServerSidePairIndex::with_policy_field("kyc_passed");
    /// assert!(idx.covers("kyc_passed"));
    /// ```
    pub fn with_policy_field(field: &str) -> Self {
        let mut idx = Self::default();
        idx.policy_required_fields.insert(field.into());
        idx
    }

    /// Returns `true` when the field is enforced by at least one
    /// server-side mechanism (SQL trigger OR policy companion).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_doctor::route_guard::field_missing_server_pair_001::ServerSidePairIndex;
    ///
    /// let idx = ServerSidePairIndex::default();
    /// assert!(!idx.covers("is_phone_verified"));
    /// ```
    pub fn covers(&self, field: &str) -> bool {
        self.sql_triggered_fields.contains(field)
            || self.policy_required_fields.contains(field)
    }
}

/// One ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub owner: String,
    pub feature: String,
    pub field: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001";

    /// Render the "missing server-side pair" message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::field_missing_server_pair_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `home`".into(),
    ///     feature: "user".into(),
    ///     field: "is_phone_verified".into(),
    /// };
    /// assert!(f.message().contains("UX-only"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `requires {}.lookup_my.{}` (a UX-only client gate) but no paired server-side enforcement was found — no SQL trigger `enforce_*_{}` in migrations/ AND no command-level `requires_field \"{}\"` policy companion. Either add the server-side gate, OR suppress this rule with `doctor:allow ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001 -- reason \"UX-cosmetic only, ...\"`.",
            self.owner, self.feature, self.field, self.field, self.field,
        )
    }
}

/// Walk every guard in `module` and flag `requires_field` slots
/// whose field is not in `server_pairs` (no SQL trigger, no policy
/// companion).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::field_missing_server_pair_001::{
///     check, ServerSidePairIndex,
/// };
///
/// let module: lazuli_ir::ExperienceModule = unimplemented!("lower");
/// let server_pairs = ServerSidePairIndex::default();
/// let _ = check(&module, &server_pairs, Path::new("hostpoint.lzx"));
/// ```
pub fn check(
    module: &ExperienceModule,
    server_pairs: &ServerSidePairIndex,
    path: &Path,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for route in &module.routes {
        if let Some(guard) = route.guard.as_ref() {
            out.extend(check_guard(
                guard,
                format!("route `{}`", route.name),
                server_pairs,
                path,
            ));
        }
    }
    for experience in &module.experiences {
        for view in &experience.views {
            if let Some(guard) = view.guard.as_ref() {
                out.extend(check_guard(
                    guard,
                    format!("view `{}.{}`", experience.name, view.name),
                    server_pairs,
                    path,
                ));
            }
        }
    }
    out
}

fn check_guard(
    guard: &ViewGuard,
    owner: String,
    server_pairs: &ServerSidePairIndex,
    path: &Path,
) -> Vec<Finding> {
    guard
        .requires_field
        .iter()
        .filter_map(|rf| {
            if server_pairs.covers(&rf.field) {
                None
            } else {
                Some(Finding {
                    path: path.to_path_buf(),
                    owner: owner.clone(),
                    feature: rf.feature.clone(),
                    field: rf.field.clone(),
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{AppRoute, DefaultValue, ExperienceModule, RequiresField, ViewGuard};

    fn mk_module(field: &str) -> ExperienceModule {
        let guard = ViewGuard {
            requires_field: vec![RequiresField {
                feature: "user".into(),
                field: field.into(),
                expected: DefaultValue::Boolean(true),
                on_unmet_redirect: "/x".into(),
                span_ref: None,
            }],
            ..ViewGuard::default()
        };
        ExperienceModule {
            app: None,
            routes: vec![AppRoute {
                name: "home".into(),
                path: Some("/home".into()),
                routes: Vec::new(),
                route_params: Vec::new(),
                to: None,
                surface: None,
                audience: None,
                lazy: None,
                prerender: None,
                guard: Some(guard),
                loaders: Vec::new(),
                pending_view: None,
                error_view: None,
                parent: None,
                span_ref: None,
            }],
            experiences: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn fires_when_no_server_side_pair_is_registered() {
        let module = mk_module("is_phone_verified");
        let server_pairs = ServerSidePairIndex::default();
        let findings = check(&module, &server_pairs, Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "is_phone_verified");
        assert_eq!(Finding::CODE, "ROUTE-GUARD-FIELD-MISSING-SERVER-PAIR-001");
    }

    #[test]
    fn quiet_when_sql_trigger_covers_the_field() {
        let module = mk_module("is_phone_verified");
        let server_pairs = ServerSidePairIndex::with_sql_field("is_phone_verified");
        assert!(check(&module, &server_pairs, Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_policy_companion_covers_the_field() {
        let module = mk_module("is_phone_verified");
        let server_pairs = ServerSidePairIndex::with_policy_field("is_phone_verified");
        assert!(check(&module, &server_pairs, Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn fires_per_unmatched_field_not_for_covered_ones() {
        let guard = ViewGuard {
            requires_field: vec![
                RequiresField {
                    feature: "user".into(),
                    field: "is_phone_verified".into(),
                    expected: DefaultValue::Boolean(true),
                    on_unmet_redirect: "/x".into(),
                    span_ref: None,
                },
                RequiresField {
                    feature: "user".into(),
                    field: "kyc_passed".into(),
                    expected: DefaultValue::Boolean(true),
                    on_unmet_redirect: "/y".into(),
                    span_ref: None,
                },
            ],
            ..ViewGuard::default()
        };
        let module = ExperienceModule {
            app: None,
            routes: vec![AppRoute {
                name: "home".into(),
                path: Some("/home".into()),
                routes: Vec::new(),
                route_params: Vec::new(),
                to: None,
                surface: None,
                audience: None,
                lazy: None,
                prerender: None,
                guard: Some(guard),
                loaders: Vec::new(),
                pending_view: None,
                error_view: None,
                parent: None,
                span_ref: None,
            }],
            experiences: Vec::new(),
            surfaces: Vec::new(),
        };
        let server_pairs = ServerSidePairIndex::with_sql_field("is_phone_verified");
        let findings = check(&module, &server_pairs, Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "kyc_passed");
    }
}
