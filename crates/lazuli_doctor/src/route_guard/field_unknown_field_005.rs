//! ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005 — `requires
//! <feature>.lookup_my.<field>` names a field that does not exist on
//! the resource backing `lookup_my_<resource>`.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles.
//!
//! ## Trigger cue
//!
//! Cue: the `requires_field` slot's field name doesn't match any
//! `Field.name` on any resource of the named feature. The doctor
//! resolves the candidate resource set by checking every resource
//! the feature owns (the slot doesn't carry the resource name; we
//! accept the field if any resource owns it).
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3 + §4.1.1 edge-case row 4.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, Feature, ViewGuard};

/// One ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub owner: String,
    pub feature: String,
    pub field: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005";

    /// Render the "field doesn't exist on resource" message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::field_unknown_field_005::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `home`".into(),
    ///     feature: "user".into(),
    ///     field: "typo".into(),
    /// };
    /// assert!(f.message().contains("typo"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `requires {}.lookup_my.{}` but field `{}` is not declared on any resource of feature `{}`.",
            self.owner, self.feature, self.field, self.field, self.feature
        )
    }
}

/// Walk every guard in `module` and flag `requires_field` slots
/// whose field doesn't exist on any resource of the named feature.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::field_unknown_field_005::check;
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
            let Some(feature) = features.iter().find(|f| f.name == rf.feature) else {
                // Unknown feature is the domain of code 004; stay
                // silent here so we don't double-flag.
                return None;
            };
            let exists = feature
                .resources
                .iter()
                .any(|r| r.fields.iter().any(|f| f.name == rf.field));
            if exists {
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
    use lazuli_ir::{
        AppRoute, BuiltinType, DefaultValue, Defaults, ExperienceModule, Feature, Field,
        FieldConstraints, Policies, RequiresField, Resource, TypeRef, ViewGuard,
    };

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::new(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_feature_with_field(feature_name: &str, field_name: &str) -> Feature {
        let resource = Resource {
            name: "User".into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![mk_field(field_name, TypeRef::Builtin(BuiltinType::Boolean))],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        };
        Feature {
            name: feature_name.into(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: vec![resource],
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
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

    fn mk_module(feature: &str, field: &str) -> ExperienceModule {
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
    fn fires_when_field_does_not_exist_on_any_resource() {
        let feature = mk_feature_with_field("user", "is_phone_verified");
        let module = mk_module("user", "typo_field");
        let findings = check(&module, &[feature], Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "typo_field");
        assert_eq!(Finding::CODE, "ROUTE-GUARD-FIELD-UNKNOWN-FIELD-005");
    }

    #[test]
    fn quiet_when_field_exists_on_resource() {
        let feature = mk_feature_with_field("user", "is_phone_verified");
        let module = mk_module("user", "is_phone_verified");
        assert!(check(&module, &[feature], Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_feature_is_unknown_avoiding_double_flag() {
        // Unknown feature is the responsibility of code 004.
        let module = mk_module("user", "is_phone_verified");
        assert!(check(&module, &[], Path::new("hostpoint.lzx")).is_empty());
    }
}
