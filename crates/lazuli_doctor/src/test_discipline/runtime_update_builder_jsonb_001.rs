//! RUNTIME-UPDATE-BUILDER-JSONB-001 — `updates` assigns a slice to a JSONB column.
//!
//! Catches LAZ-RUNTIME-SETSLICE-JSONB — `lazuli.SetIfNotNilSlice(b, col, &slice)`
//! writes a Go slice directly to pgx, which encodes as a Postgres ARRAY and
//! fails on JSONB target columns with `unable to encode []T into text format
//! for unknown type (OID 0)`. The fix is for runtime to detect the column
//! type and JSON-marshal when the target is JSONB, OR for codegen to pick a
//! distinct helper (`SetIfNotNilJSONSlice`).
//!
//! Detection (IR-only, conservative):
//!
//! Walk every `Command.effect` that is an `UpdateEffect`. For each assignment,
//! resolve the target field on the command's resource; fire when the field's
//! type is `Many<...>` (collection) — the codegen lowers `Many` fields to a
//! JSONB column and the runtime emits `SetIfNotNilSlice`. Author-aware
//! refactor: replace the slice helper with the JSON-encoding variant once it
//! lands, OR split the assignment into a JSON-marshalled value.
//!
//! Severity: `warning` (strict + production). Warning rather than error
//! because v0.1 fires on the construct shape, not the proven failure mode;
//! once the runtime ships the JSONB-aware helper the rule's job shifts to
//! "use the right helper".

use std::path::{Path, PathBuf};

use lazuli_ir::{Assignment, CommandEffect, Feature, Resource, SpanRef, TypeRef};

/// One RUNTIME-UPDATE-BUILDER-JSONB-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` source path that declares the command.
    pub path: PathBuf,
    /// Feature containing the command.
    pub feature: String,
    /// Command performing the update.
    pub command: String,
    /// Target resource of the update.
    pub resource: String,
    /// Field name (slice/JSONB) on which the broken helper would land.
    pub field: String,
    /// Optional span pointer for editor jumps.
    pub span: Option<SpanRef>,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "RUNTIME-UPDATE-BUILDER-JSONB-001";

    /// Render the user-facing diagnostic body — explains the
    /// `SetIfNotNilSlice` vs JSONB encoding mismatch.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::runtime_update_builder_jsonb_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("billing.lzi"),
    ///     feature: "billing".into(),
    ///     command: "update_tags".into(),
    ///     resource: "Invoice".into(),
    ///     field: "tags".into(),
    ///     span: None,
    /// };
    /// assert!(f.message().contains("update_tags"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "command `{}` updates `{}.{}` (slice/JSONB) via the generated \
             `SetIfNotNilSlice` path — pgx encodes slices as Postgres ARRAY \
             and fails on JSONB targets. Either use the JSONB-aware update \
             helper or move the assignment off the slice helper.",
            self.command, self.resource, self.field
        )
    }
}

/// Run RUNTIME-UPDATE-BUILDER-JSONB-001 over a feature. Inspects every
/// `Update` command and reports field-level cases where the codegen
/// would route a slice through the JSONB-incompatible helper.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::runtime_update_builder_jsonb_001::check;
///
/// let findings = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, source_path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for cmd in &feature.commands {
        let CommandEffect::Updates(update) = &cmd.effect else {
            continue;
        };
        let Some(resource) = lookup_resource(feature, &update.resource.name) else {
            continue;
        };
        for Assignment { field, .. } in &update.assignments {
            let Some(target_field) = resource.fields.iter().find(|f| &f.name == field) else {
                continue;
            };
            if is_slice_jsonb(&target_field.type_ref) {
                findings.push(Finding {
                    path: source_path.to_path_buf(),
                    feature: feature.name.clone(),
                    command: cmd.name.clone(),
                    resource: resource.name.clone(),
                    field: field.clone(),
                    span: cmd.span_ref.clone(),
                });
            }
        }
    }
    findings
}

fn lookup_resource<'a>(feature: &'a Feature, name: &str) -> Option<&'a Resource> {
    feature.resources.iter().find(|r| r.name == name)
}

/// True when the field's type lowers to a JSONB column AND comes from a
/// slice in Go. `Many<T>` is the canonical case; the codegen emits both
/// the JSONB column AND the `SetIfNotNilSlice` helper for it.
fn is_slice_jsonb(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Many(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Expr, Field,
        Policies, PolicyRef, QualifiedName, Resource, UpdateEffect,
    };

    fn mk_resource_with_field(field_name: &str, type_ref: TypeRef) -> Resource {
        Resource {
            name: "Property".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![Field {
                name: field_name.to_owned(),
                type_ref,
                required: false,
                unique: false,
                slug: false,
                default: None,
                derived_from: None,
                computed_date: None,
                constraints: Default::default(),
                full_text: false,
                previous_names: vec![],
                pii: None,
                owner_axis: None,
                cross_feature_target: None,
                span_ref: None,
            }],
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
            many_through: Vec::new(),
            append_only: false,
        }
    }

    fn mk_update_command(name: &str, resource: &str, field_name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Updates(UpdateEffect {
                resource: QualifiedName {
                    feature: None,
                    name: resource.to_owned(),
                },
                assignments: vec![Assignment {
                    field: field_name.to_owned(),
                    value: Expr::Path(lazuli_ir::Path::from_segments(["input", field_name])),
                }],
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
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: None,
            derived_from: None,
        }
    }

    fn mk_feature(resource: Resource, command: Command) -> Feature {
        Feature {
            name: "catalog".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![resource],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![command],
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
    fn update_with_many_field_fires() {
        let many_type = TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Text)));
        let resource = mk_resource_with_field("amenities", many_type);
        let command = mk_update_command("update_property", "Property", "amenities");
        let feature = mk_feature(resource, command);
        let findings = check(&feature, Path::new("catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "amenities");
    }

    #[test]
    fn update_with_scalar_field_does_not_fire() {
        let resource = mk_resource_with_field("name", TypeRef::Builtin(BuiltinType::Text));
        let command = mk_update_command("update_property", "Property", "name");
        let feature = mk_feature(resource, command);
        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }

    #[test]
    fn create_effect_does_not_fire() {
        let many_type = TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Text)));
        let resource = mk_resource_with_field("amenities", many_type);
        let command = Command {
            effect: CommandEffect::None,
            ..mk_update_command("create_property", "Property", "amenities")
        };
        let feature = mk_feature(resource, command);
        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }
}
