//! COMMAND-INPUT-SHADOWS-FIELD-001 - typed command input shadows a resource
//! field with a different declared type.
//!
//! Fires when a `command.input` typed slot has the same name as a field on the
//! command's `creates`/`updates` resource, but declares a different `TypeRef`.
//! This catches handlers accepting a coarser or different type than the stored
//! field while the names imply direct assignment.

use std::path::{Path, PathBuf};

use lazuli_ir::{CommandEffect, CommandInput, Feature, Resource, TypeRef};

// output

/// One COMMAND-INPUT-SHADOWS-FIELD-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub command: String,
    pub field_name: String,
    pub resource: String,
    pub input_type_label: String,
    pub field_type_label: String,
}

impl Finding {
    pub const CODE: &'static str = "COMMAND-INPUT-SHADOWS-FIELD-001";

    pub fn message(&self) -> String {
        format!(
            "command `{}` accepts input `{}: {}` which shadows resource `{}` \
             field `{}: {}` with a different type. The handler will silently \
             narrow/widen the value; either align the input type to the field \
             type OR rename the input to make the conversion explicit.",
            self.command,
            self.field_name,
            self.input_type_label,
            self.resource,
            self.field_name,
            self.field_type_label
        )
    }
}

// detection

/// Run COMMAND-INPUT-SHADOWS-FIELD-001 for all commands in one feature.
///
/// `path` is the source `.lzi` file - used to anchor findings; no I/O is
/// performed here. Cross-feature resources are out of scope for v1 and are
/// suppressed when the target resource is not present in `feature.resources`.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();

    for command in &feature.commands {
        let resource_name = match &command.effect {
            CommandEffect::Creates(effect) => &effect.resource.name,
            CommandEffect::Updates(effect) => &effect.resource.name,
            CommandEffect::Deletes(_) | CommandEffect::Returns(_) | CommandEffect::None => {
                continue;
            }
        };

        let resource = match find_resource(feature, resource_name) {
            Some(resource) => resource,
            None => continue,
        };

        let slots = match &command.input {
            CommandInput::Typed(slots) => slots,
            CommandInput::Short(_) | CommandInput::Empty => continue,
        };

        for slot in slots {
            if matches!(slot.type_ref, TypeRef::Unresolved(_)) {
                continue;
            }

            let field = match resource.fields.iter().find(|field| field.name == slot.name) {
                Some(field) => field,
                None => continue,
            };

            if slot.type_ref != field.type_ref {
                out.push(Finding {
                    path: path.to_path_buf(),
                    command: command.name.clone(),
                    field_name: slot.name.clone(),
                    resource: resource.name.clone(),
                    input_type_label: type_ref_label(&slot.type_ref),
                    field_type_label: type_ref_label(&field.type_ref),
                });
            }
        }
    }

    out
}

// internals

fn find_resource<'a>(feature: &'a Feature, name: &str) -> Option<&'a Resource> {
    feature
        .resources
        .iter()
        .find(|resource| resource.name == name)
}

fn type_ref_label(t: &TypeRef) -> String {
    match t {
        TypeRef::Builtin(b) => format!("{:?}", b),
        TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::UserDefined(qn) => qn.name.clone(),
        TypeRef::Many(inner) => format!("list of {}", type_ref_label(inner)),
        TypeRef::Capability(_) => "@cap.*".to_owned(),
        TypeRef::Unresolved(s) => format!("unresolved({})", s),
    }
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandKind, CreateEffect, Defaults, Field, FieldConstraints,
        Policies, PolicyRef, QualifiedName, ReturnsEffect, TypedSlot, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn builtin(builtin: BuiltinType) -> TypeRef {
        TypeRef::Builtin(builtin)
    }

    fn enum_ref(name: &str) -> TypeRef {
        TypeRef::EnumRef(qn(name))
    }

    fn mk_slot(name: &str, type_ref: TypeRef) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref,
            required: true,
            constraints: FieldConstraints::default(),
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            span_ref: None,
        }
    }

    fn mk_resource(fields: Vec<Field>) -> Resource {
        Resource {
            name: "Publication".to_owned(),
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
        }
    }

    fn mk_cmd(name: &str, input: CommandInput, effect: CommandEffect) -> Command {
        let kind = match &effect {
            CommandEffect::Creates(_) => CommandKind::Create,
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            _ => CommandKind::Returns,
        };

        Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
            route: vec![],
            input,
            target: None,
            lets: vec![],
            effect,
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(command: Command, resource: Resource) -> Feature {
        Feature {
            name: "publishing".into(),
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

    fn creates_effect() -> CommandEffect {
        CommandEffect::Creates(CreateEffect {
            resource: qn("Publication"),
            from_input: false,
            assignments: vec![],
        })
    }

    fn updates_effect() -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn("Publication"),
            assignments: vec![],
        })
    }

    fn returns_effect() -> CommandEffect {
        CommandEffect::Returns(ReturnsEffect {
            return_type: builtin(BuiltinType::Boolean),
        })
    }

    #[test]
    fn positive_input_text_shadows_enum_field_fires() {
        let command = mk_cmd(
            "publish",
            CommandInput::Typed(vec![mk_slot("status", builtin(BuiltinType::Text))]),
            creates_effect(),
        );
        let feature = mk_feature(
            command,
            mk_resource(vec![mk_field("status", enum_ref("PublicationStatus"))]),
        );

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "COMMAND-INPUT-SHADOWS-FIELD-001");
        assert_eq!(findings[0].command, "publish");
        assert_eq!(findings[0].field_name, "status");
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].input_type_label, "Text");
        assert_eq!(findings[0].field_type_label, "PublicationStatus");
        assert!(findings[0].message().contains("publish"));
    }

    #[test]
    fn positive_input_integer_shadows_text_field_fires() {
        let command = mk_cmd(
            "score",
            CommandInput::Typed(vec![mk_slot("score", builtin(BuiltinType::Integer))]),
            updates_effect(),
        );
        let feature = mk_feature(
            command,
            mk_resource(vec![mk_field("score", builtin(BuiltinType::Text))]),
        );

        let findings = check(&feature, Path::new("f.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].input_type_label, "Integer");
        assert_eq!(findings[0].field_type_label, "Text");
    }

    #[test]
    fn negative_input_matches_field_type_does_not_fire() {
        let command = mk_cmd(
            "publish",
            CommandInput::Typed(vec![mk_slot("status", enum_ref("PublicationStatus"))]),
            creates_effect(),
        );
        let feature = mk_feature(
            command,
            mk_resource(vec![mk_field("status", enum_ref("PublicationStatus"))]),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_input_no_matching_field_does_not_fire() {
        let command = mk_cmd(
            "dry_run",
            CommandInput::Typed(vec![mk_slot("dry_run", builtin(BuiltinType::Boolean))]),
            creates_effect(),
        );
        let feature = mk_feature(
            command,
            mk_resource(vec![mk_field("status", builtin(BuiltinType::Text))]),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_short_input_does_not_fire() {
        let command = mk_cmd(
            "publish",
            CommandInput::Short(vec!["status".to_owned()]),
            creates_effect(),
        );
        let feature = mk_feature(
            command,
            mk_resource(vec![mk_field("status", enum_ref("PublicationStatus"))]),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_returns_command_does_not_fire() {
        let command = mk_cmd(
            "preview",
            CommandInput::Typed(vec![mk_slot("status", builtin(BuiltinType::Text))]),
            returns_effect(),
        );
        let feature = mk_feature(
            command,
            mk_resource(vec![mk_field("status", enum_ref("PublicationStatus"))]),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_unresolved_slot_does_not_fire() {
        let command = mk_cmd(
            "publish",
            CommandInput::Typed(vec![mk_slot(
                "status",
                TypeRef::Unresolved("PublicationStatus".to_owned()),
            )]),
            creates_effect(),
        );
        let feature = mk_feature(
            command,
            mk_resource(vec![mk_field("status", enum_ref("PublicationStatus"))]),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
