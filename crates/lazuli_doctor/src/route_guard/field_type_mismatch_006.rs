//! ROUTE-GUARD-FIELD-TYPE-MISMATCH-006 — the literal supplied to a
//! `requires <feature>.lookup_my.<field> = <literal>` slot does not
//! match the field's declared IR type.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles.
//!
//! ## Trigger cue
//!
//! Cue: `requires user.lookup_my.is_phone_verified = "yes"` where
//! `is_phone_verified` is `Boolean` — the literal is a string, the
//! field is a boolean, so the equality check would never succeed.
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3 + §4.1.1 edge-case row 2.

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, DefaultValue, ExperienceModule, Feature, TypeRef, ViewGuard};

/// One ROUTE-GUARD-FIELD-TYPE-MISMATCH-006 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub owner: String,
    pub feature: String,
    pub field: String,
    /// Snake-case label of the field's IR type (`boolean`, `text`, ...).
    pub field_type: String,
    /// Snake-case label of the literal supplied on the RHS.
    pub literal_kind: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-FIELD-TYPE-MISMATCH-006";

    /// Render the "literal type does not match field type" message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::field_type_mismatch_006::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `home`".into(),
    ///     feature: "user".into(),
    ///     field: "is_phone_verified".into(),
    ///     field_type: "boolean".into(),
    ///     literal_kind: "string".into(),
    /// };
    /// assert!(f.message().contains("boolean"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `requires {}.lookup_my.{} = <{literal}>` but field `{}` is of type `{ft}` — the literal type does not match.",
            self.owner,
            self.feature,
            self.field,
            self.field,
            literal = self.literal_kind,
            ft = self.field_type,
        )
    }
}

/// Walk every guard in `module` and flag `requires_field` slots
/// where the literal kind doesn't match the field's IR type.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::field_type_mismatch_006::check;
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
            let feature = features.iter().find(|f| f.name == rf.feature)?;
            let field = feature
                .resources
                .iter()
                .flat_map(|r| r.fields.iter())
                .find(|f| f.name == rf.field)?;
            if literal_matches_type(&rf.expected, &field.type_ref) {
                None
            } else {
                Some(Finding {
                    path: path.to_path_buf(),
                    owner: owner.clone(),
                    feature: rf.feature.clone(),
                    field: rf.field.clone(),
                    field_type: type_label(&field.type_ref),
                    literal_kind: literal_label(&rf.expected),
                })
            }
        })
        .collect()
}

/// Returns `true` when the IR literal kind is compatible with the
/// field's IR type. `Nil` is admissible against any field type (the
/// `= null` literal is universally compatible since every field can
/// be checked against null at runtime).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_ir::{BuiltinType, DefaultValue, TypeRef};
/// use lazuli_doctor::route_guard::field_type_mismatch_006::literal_matches_type;
///
/// assert!(literal_matches_type(
///     &DefaultValue::Boolean(true),
///     &TypeRef::Builtin(BuiltinType::Boolean),
/// ));
/// ```
pub fn literal_matches_type(lit: &DefaultValue, ty: &TypeRef) -> bool {
    if matches!(lit, DefaultValue::Nil) {
        return true;
    }
    match (lit, ty) {
        (DefaultValue::Boolean(_), TypeRef::Builtin(BuiltinType::Boolean)) => true,
        (
            DefaultValue::Integer(_),
            // W1 GAP-05 — Percentage is Decimal-backed; an integer literal
            // (e.g. `= 50`) is an admissible route-guard RHS.
            TypeRef::Builtin(BuiltinType::Integer | BuiltinType::SemanticPercentage),
        ) => true,
        (
            DefaultValue::String(_),
            TypeRef::Builtin(
                BuiltinType::Text
                | BuiltinType::Id
                | BuiltinType::SemanticEmail
                | BuiltinType::SemanticPhone
                | BuiltinType::SemanticUrl
                | BuiltinType::SemanticUuid
                | BuiltinType::SemanticCurrency
                // W1 GAP-04 — HexColor is a text carrier (`#RRGGBB`).
                | BuiltinType::SemanticHexColor,
            ),
        ) => true,
        // Enum literal vs EnumRef is admissible.
        (DefaultValue::EnumLiteral(_), TypeRef::EnumRef(_)) => true,
        _ => false,
    }
}

fn literal_label(lit: &DefaultValue) -> String {
    match lit {
        DefaultValue::String(_) => "string".into(),
        DefaultValue::Integer(_) => "integer".into(),
        DefaultValue::Boolean(_) => "boolean".into(),
        DefaultValue::EnumLiteral(_) => "enum".into(),
        DefaultValue::Nil => "null".into(),
    }
}

fn type_label(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Builtin(b) => format!("{:?}", b).to_ascii_lowercase(),
        TypeRef::EnumRef(_) => "enum".into(),
        TypeRef::UserDefined(_) => "user_defined".into(),
        TypeRef::Many(_) => "many".into(),
        TypeRef::Unresolved(_) => "unresolved".into(),
        TypeRef::Capability(_) => "capability".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppRoute, BuiltinType, Defaults, ExperienceModule, Feature, Field, FieldConstraints,
        Policies, RequiresField, Resource, TypeRef, ViewGuard,
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

    fn mk_feature(feature_name: &str, fields: Vec<Field>) -> Feature {
        let resource = Resource {
            name: "User".into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
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
            restrict_on_delete: Vec::new(),
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

    fn mk_module(feature: &str, field: &str, expected: DefaultValue) -> ExperienceModule {
        let guard = ViewGuard {
            requires_field: vec![RequiresField {
                feature: feature.into(),
                field: field.into(),
                expected,
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
    fn fires_when_string_literal_matched_against_boolean_field() {
        let feature = mk_feature(
            "user",
            vec![mk_field(
                "is_phone_verified",
                TypeRef::Builtin(BuiltinType::Boolean),
            )],
        );
        let module = mk_module(
            "user",
            "is_phone_verified",
            DefaultValue::String("yes".into()),
        );
        let findings = check(&module, &[feature], Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field_type, "boolean");
        assert_eq!(findings[0].literal_kind, "string");
        assert_eq!(Finding::CODE, "ROUTE-GUARD-FIELD-TYPE-MISMATCH-006");
    }

    #[test]
    fn quiet_when_boolean_literal_matches_boolean_field() {
        let feature = mk_feature(
            "user",
            vec![mk_field(
                "is_phone_verified",
                TypeRef::Builtin(BuiltinType::Boolean),
            )],
        );
        let module = mk_module("user", "is_phone_verified", DefaultValue::Boolean(true));
        assert!(check(&module, &[feature], Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_null_literal_against_any_field_type() {
        // `null` is admissible against any field type — typically used
        // for nullable timestamps (`kyc_passed_at = null`).
        let feature = mk_feature(
            "user",
            vec![mk_field(
                "kyc_passed_at",
                TypeRef::Builtin(BuiltinType::DateTime),
            )],
        );
        let module = mk_module("user", "kyc_passed_at", DefaultValue::Nil);
        assert!(check(&module, &[feature], Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_string_literal_matches_text_field() {
        let feature = mk_feature(
            "user",
            vec![mk_field(
                "preferred_language",
                TypeRef::Builtin(BuiltinType::Text),
            )],
        );
        let module = mk_module(
            "user",
            "preferred_language",
            DefaultValue::String("pt-BR".into()),
        );
        assert!(check(&module, &[feature], Path::new("hostpoint.lzx")).is_empty());
    }
}
