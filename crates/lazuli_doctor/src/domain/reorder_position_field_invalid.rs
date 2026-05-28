//! REORDER-POSITION-FIELD-001 — a `reorder <Resource> by <field>` command
//! names a `by` field that isn't a declared `Integer` field on the target
//! resource.
//!
//! W4 GAP-REORDER-01. The `reorder` verb emits a single batch UPDATE that
//! rewrites the position column from an ordered id list. That column must
//! exist on the target resource and be an `Integer` — otherwise the emitted
//! SQL would write to a missing / mistyped column. This rule fires when, for
//! a command whose effect is `Reorders { resource, position_field }`:
//!  - `resource` isn't a declared resource on the feature, OR
//!  - `position_field` isn't a declared field on it, OR
//!  - `position_field` is declared but is not an `Integer` field.
//!
//! Severity: `error`. Rule Zero — a reorder against a non-integer / missing
//! position column miscompiles silently. Same-resource field-reference walk,
//! mirroring COMPUTED-DATE-EXPR-001.
//!
//! Reference: GAP-REORDER-01 (`docs/proposals/ir-pauta-gaps-bundle-2026-05-28.md`).

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, CommandEffect, Feature, Resource, TypeRef};

/// What about a `reorder` command's `by` field failed to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// `reorder <Resource>` names a resource not declared on the feature.
    ResourceUnknown,
    /// `by <field>` names a field not declared on the resource.
    FieldUnknown,
    /// `by <field>` is declared but is not an `Integer` field.
    FieldNotInteger,
}

impl Reason {
    fn describe(&self, operand: &str) -> String {
        match self {
            Reason::ResourceUnknown => {
                format!("target resource `{operand}` is not declared in this feature")
            }
            Reason::FieldUnknown => {
                format!("position field `{operand}` is not a declared field")
            }
            Reason::FieldNotInteger => {
                format!("position field `{operand}` is not an `Integer` field")
            }
        }
    }
}

/// One REORDER-POSITION-FIELD-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the command.
    pub path: PathBuf,
    /// Feature containing the command.
    pub feature: String,
    /// The `reorder` command whose `by` field failed to resolve.
    pub command: String,
    /// The offending operand (resource or position-field identifier).
    pub operand: String,
    /// What failed.
    pub reason: Reason,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "REORDER-POSITION-FIELD-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::reorder_position_field_invalid::{Finding, Reason};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("jobs.lzi"),
    ///     feature: "jobs".into(),
    ///     command: "reorder_steps".into(),
    ///     operand: "position".into(),
    ///     reason: Reason::FieldNotInteger,
    /// };
    /// assert!(f.message().contains("reorder_steps"));
    /// assert!(f.message().contains("Integer"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`reorder` command `{}` is invalid: {}. The `by <field>` must \
             reference a declared `Integer` field on the target resource.",
            self.command,
            self.reason.describe(&self.operand),
        )
    }
}

/// Run REORDER-POSITION-FIELD-001 over one feature.
///
/// Only commands whose effect is `Reorders` are checked. The target
/// resource + position field are resolved against the feature's own
/// resources (same-feature only; `reorder` writes a local resource).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::reorder_position_field_invalid::check;
///
/// let findings = check(&feature, Path::new("jobs.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for command in &feature.commands {
        let CommandEffect::Reorders(effect) = &command.effect else {
            continue;
        };

        let mut push = |operand: String, reason: Reason| {
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                command: command.name.clone(),
                operand,
                reason,
            });
        };

        let Some(resource) = find_resource(feature, &effect.resource.name) else {
            push(effect.resource.name.clone(), Reason::ResourceUnknown);
            continue;
        };

        match find_field_type(resource, &effect.position_field) {
            None => push(effect.position_field.clone(), Reason::FieldUnknown),
            Some(t) if !is_integer(t) => {
                push(effect.position_field.clone(), Reason::FieldNotInteger)
            }
            Some(_) => {}
        }
    }

    findings
}

fn find_resource<'a>(feature: &'a Feature, name: &str) -> Option<&'a Resource> {
    feature.resources.iter().find(|r| r.name == name)
}

fn find_field_type<'a>(resource: &'a Resource, name: &str) -> Option<&'a TypeRef> {
    resource
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| &f.type_ref)
}

fn is_integer(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Builtin(BuiltinType::Integer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Command, CommandInput, CommandKind, Defaults, Field, FieldConstraints, Policies, PolicyRef,
        QualifiedName, ReorderEffect, Resource, TypeRef,
    };

    fn mk_field(name: &str, builtin: BuiltinType) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(builtin),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            append_only: false,
        }
    }

    fn mk_reorder_command(name: &str, resource: &str, position_field: &str) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Reorder,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Reorders(ReorderEffect {
                resource: QualifiedName {
                    feature: None,
                    name: resource.into(),
                },
                position_field: position_field.into(),
            }),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "jobs".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_integer_position_field_passes() {
        let resource = mk_resource(
            "JobStep",
            vec![mk_field("position", BuiltinType::Integer)],
        );
        let cmd = mk_reorder_command("reorder_steps", "JobStep", "position");
        let feature = mk_feature(vec![resource], vec![cmd]);
        assert!(check(&feature, Path::new("j.lzi")).is_empty());
    }

    #[test]
    fn negative_missing_position_field_fires() {
        let resource = mk_resource("JobStep", vec![mk_field("name", BuiltinType::Text)]);
        let cmd = mk_reorder_command("reorder_steps", "JobStep", "position");
        let feature = mk_feature(vec![resource], vec![cmd]);
        let findings = check(&feature, Path::new("j.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::FieldUnknown);
        assert_eq!(Finding::CODE, "REORDER-POSITION-FIELD-001");
    }

    #[test]
    fn negative_non_integer_position_field_fires() {
        let resource = mk_resource("JobStep", vec![mk_field("position", BuiltinType::Text)]);
        let cmd = mk_reorder_command("reorder_steps", "JobStep", "position");
        let feature = mk_feature(vec![resource], vec![cmd]);
        let findings = check(&feature, Path::new("j.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::FieldNotInteger);
    }

    #[test]
    fn negative_unknown_resource_fires() {
        let cmd = mk_reorder_command("reorder_steps", "Ghost", "position");
        let feature = mk_feature(vec![], vec![cmd]);
        let findings = check(&feature, Path::new("j.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::ResourceUnknown);
    }
}
