//! ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004 — `requires
//! <feature>.lookup_my.<field>` references a feature that has no
//! `lookup_my_*` query.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles.
//!
//! ## Trigger cue
//!
//! Cue: a `requires_field` slot whose `feature` doesn't appear in
//! the supplied feature index, OR the feature exists but does not
//! ship any `query.lookup my_<x>` shape (i.e. the convention's
//! `lookup_my_<resource>` is absent).
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3 + §4.1.1 edge-case row 1.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, Feature, Query, ViewGuard};

/// One ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub owner: String,
    pub feature: String,
    pub field: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004";

    /// Render the "feature has no lookup_my query" message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::field_unknown_feature_004::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `home`".into(),
    ///     feature: "user".into(),
    ///     field: "is_phone_verified".into(),
    /// };
    /// assert!(f.message().contains("user"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `requires {}.lookup_my.{}` but feature `{}` ships no `lookup_my_*` query. Add `query.lookup my_<resource>` to feature `{}`, or correct the qualified path on the `requires` slot.",
            self.owner, self.feature, self.field, self.feature, self.feature
        )
    }
}

/// Walk every guard in `module` and flag `requires_field` slots
/// whose `feature` has no `lookup_my_*` query.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::field_unknown_feature_004::check;
///
/// let module: lazuli_ir::ExperienceModule = unimplemented!("lower");
/// let _ = check(&module, &[], Path::new("hostpoint.lzx"));
/// ```
pub fn check(module: &ExperienceModule, features: &[Feature], path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for route in &module.routes {
        if let Some(guard) = route.guard.as_ref() {
            out.extend(check_guard(
                guard,
                format!("route `{}`", route.name),
                features,
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
                    features,
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
    features: &[Feature],
    path: &Path,
) -> Vec<Finding> {
    guard
        .requires_field
        .iter()
        .filter_map(|rf| {
            if feature_has_lookup_my(features, &rf.feature) {
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

/// Returns `true` when `features` contains a feature named
/// `feature_name` that ships at least one `query.lookup my_<x>` query.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::route_guard::field_unknown_feature_004::feature_has_lookup_my;
///
/// let features: Vec<lazuli_ir::Feature> = vec![];
/// assert!(!feature_has_lookup_my(&features, "user"));
/// ```
pub fn feature_has_lookup_my(features: &[Feature], feature_name: &str) -> bool {
    let Some(feature) = features.iter().find(|f| f.name == feature_name) else {
        return false;
    };
    feature.queries.iter().any(|q| {
        matches!(q, Query::Lookup(lookup) if lookup.name.starts_with("my_"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppRoute, Defaults, DefaultValue, ExperienceModule, Feature, LookupQuery, Policies, Query,
        RequiresField, ViewGuard,
    };

    fn mk_feature_with_lookup(name: &str, lookup_name: &str) -> Feature {
        let lookup = LookupQuery {
            name: lookup_name.into(),
            public_contract: None,
            params: Vec::new(),
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: Default::default(),
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        };
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: vec![Query::Lookup(lookup)],
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn mk_module_with_requires_field(feature: &str, field: &str) -> ExperienceModule {
        let guard = ViewGuard {
            requires_field: vec![RequiresField {
                feature: feature.into(),
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
    fn fires_when_feature_is_missing() {
        let module = mk_module_with_requires_field("user", "is_phone_verified");
        let findings = check(&module, &[], Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "user");
        assert_eq!(Finding::CODE, "ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004");
        assert!(findings[0].message().contains("`user`"));
    }

    #[test]
    fn fires_when_feature_exists_but_ships_no_lookup_my() {
        let feature = mk_feature_with_lookup("user", "by_id");
        let module = mk_module_with_requires_field("user", "is_phone_verified");
        let findings = check(&module, &[feature], Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn quiet_when_feature_has_lookup_my() {
        let feature = mk_feature_with_lookup("user", "my_user");
        let module = mk_module_with_requires_field("user", "is_phone_verified");
        assert!(check(&module, &[feature], Path::new("hostpoint.lzx")).is_empty());
    }
}
