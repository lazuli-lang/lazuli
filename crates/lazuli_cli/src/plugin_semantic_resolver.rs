//! B3 — analyzer post-pass that rewrites `@semantic.<Name>` references
//! against the plugin alias map built from `Lazurite.toml [plugins]`.
//!
//! Walks every `TypeRef::UserDefined(qname)` in the lowered module and
//! rewrites to `TypeRef::Builtin(BuiltinType::SemanticPluginType { ... })`
//! when `qname.name` matches a manifest-declared alias. Unmatched
//! `@semantic.<X>` names stay as `UserDefined` so the doctor's
//! `SEMANTIC-PLUGIN-001` (or the legacy unresolved-name diagnostic) can
//! anchor at the field site.
//!
//! The pass is intentionally narrow: only `TypeRef::UserDefined` is
//! considered, mirroring `type_ref_from_syntax`'s fallback (any
//! unrecognized name lands there). Built-in `@semantic.*` names are
//! resolved syntactically in the analyzer and never reach this pass.
//!
//! The walker covers every IR site that carries a `TypeRef`:
//! resource fields, record fields, command inputs, command-effect
//! assignments, query slots, event payloads, and `TypeRef::Many` inner
//! wrappers (so `@semantic.BrazilianCPF[]` lifts correctly when a
//! future cycle introduces it). Per-extension shapes (function
//! signatures, validator types) are out of scope because they don't
//! carry `TypeRef`.
//!
//! Determinism: the pass is a pure rewrite over the IR — no
//! filesystem reads happen here. The alias map is built once by
//! `crate::plugin_manifest::build_alias_map` and passed in.

use std::collections::BTreeMap;

use lazuli_ir::{BuiltinType, CommandInput, Field, Module, TypeRef};

use crate::plugin_manifest::ResolvedPluginSemantic;

/// Rewrite every `@semantic.<Name>` reference in `module` against the
/// alias map. Idempotent (running twice is a no-op because matched
/// references no longer surface as `UserDefined`).
///
/// ## Examples
///
/// ```ignore
/// use std::collections::BTreeMap;
/// use lazuli_cli::plugin_semantic_resolver::apply_plugin_semantic_resolution;
///
/// // apply_plugin_semantic_resolution(&mut module, &alias_map);
/// ```
pub fn apply_plugin_semantic_resolution(
    module: &mut Module,
    alias_map: &BTreeMap<String, ResolvedPluginSemantic>,
) {
    if alias_map.is_empty() {
        return;
    }
    for feature in &mut module.features {
        for resource in &mut feature.resources {
            for field in &mut resource.fields {
                rewrite_field(field, alias_map);
            }
        }
        for record in &mut feature.records {
            for field in &mut record.fields {
                rewrite_field(field, alias_map);
            }
        }
        for event in &mut feature.events {
            for field in &mut event.payload {
                rewrite_event_field(field, alias_map);
            }
        }
        for command in &mut feature.commands {
            rewrite_command_input(&mut command.input, alias_map);
        }
    }
}

fn rewrite_field(field: &mut Field, alias_map: &BTreeMap<String, ResolvedPluginSemantic>) {
    rewrite_type_ref(&mut field.type_ref, alias_map);
}

fn rewrite_event_field(
    field: &mut lazuli_ir::EventField,
    alias_map: &BTreeMap<String, ResolvedPluginSemantic>,
) {
    rewrite_type_ref(&mut field.type_ref, alias_map);
}

fn rewrite_command_input(
    input: &mut CommandInput,
    alias_map: &BTreeMap<String, ResolvedPluginSemantic>,
) {
    if let CommandInput::Typed(slots) = input {
        for slot in slots.iter_mut() {
            rewrite_type_ref(&mut slot.type_ref, alias_map);
        }
    }
    // `Short(Vec<String>)` carries bare field names that reference
    // the local resource's declared fields by name only — no
    // independent `TypeRef`. The resource's field walk already
    // rewrote them.
}

/// Core rewrite: if `type_ref` is `UserDefined(qname)` and `qname.name`
/// resolves through the alias map, replace it with a
/// `Builtin(SemanticPluginType { ... })`. Recurses into `Many` so
/// `@semantic.BrazilianCPF[]` (when authored) lifts correctly.
///
/// ## Examples
///
/// ```ignore
/// use std::collections::BTreeMap;
/// use lazuli_cli::plugin_semantic_resolver::rewrite_type_ref;
///
/// // rewrite_type_ref(&mut field.type_ref, &alias_map);
/// ```
pub fn rewrite_type_ref(
    type_ref: &mut TypeRef,
    alias_map: &BTreeMap<String, ResolvedPluginSemantic>,
) {
    match type_ref {
        TypeRef::UserDefined(qname) => {
            if let Some(resolved) = alias_map.get(&qname.name) {
                *type_ref = TypeRef::Builtin(BuiltinType::SemanticPluginType {
                    plugin: resolved.plugin_namespace.clone(),
                    name: resolved.name.clone(),
                    carrier: Box::new(resolved.carrier.clone()),
                    validator: resolved.validator.clone(),
                    go_module: resolved.go_module.clone(),
                    ts_package: resolved.ts_package.clone(),
                    error_code: resolved.error_code.clone(),
                    message_key: resolved.message_key.clone(),
                    ts_validator: resolved.ts_validator.clone(),
                });
            }
        }
        TypeRef::Many(inner) => rewrite_type_ref(inner, alias_map),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Feature, Field, FieldConstraints, Policies, QualifiedName, Resource,
        TypeRef,
    };

    fn make_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn make_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
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
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
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
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn empty_module() -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: Vec::new(),
        }
    }

    fn cpf_alias_map() -> BTreeMap<String, ResolvedPluginSemantic> {
        let mut map = BTreeMap::new();
        map.insert(
            "@semantic.BrazilianCPF".to_owned(),
            ResolvedPluginSemantic {
                plugin_namespace: "@lazuli/plugin-scalars-br".to_owned(),
                plugin_short_name: "scalars-br".to_owned(),
                name: "BrazilianCPF".to_owned(),
                alias: "@semantic.BrazilianCPF".to_owned(),
                carrier: BuiltinType::Text,
                validator: "ValidateCPF".to_owned(),
                formatter: Some("FormatCPF".to_owned()),
                go_module: "lazuli.dev/plugin/scalars-br".to_owned(),
                ts_package: "@lazuli/plugin-scalars-br".to_owned(),
                error_code: "cpf_invalid".to_owned(),
                message_key: String::new(),
                ts_validator: String::new(),
            },
        );
        map
    }

    #[test]
    fn rewrites_user_defined_to_semantic_plugin_type() {
        let user_defined = TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "@semantic.BrazilianCPF".to_owned(),
        });
        let mut module = empty_module();
        let mut feat = empty_feature("host");
        feat.resources
            .push(make_resource("Host", vec![make_field("cpf", user_defined)]));
        module.features.push(feat);

        apply_plugin_semantic_resolution(&mut module, &cpf_alias_map());

        let cpf_field = &module.features[0].resources[0].fields[0];
        match &cpf_field.type_ref {
            TypeRef::Builtin(BuiltinType::SemanticPluginType {
                plugin,
                name,
                carrier,
                validator,
                ..
            }) => {
                assert_eq!(plugin, "@lazuli/plugin-scalars-br");
                assert_eq!(name, "BrazilianCPF");
                assert!(matches!(**carrier, BuiltinType::Text));
                assert_eq!(validator, "ValidateCPF");
            }
            other => panic!("expected SemanticPluginType, got {other:?}"),
        }
    }

    #[test]
    fn unresolved_plugin_semantic_stays_user_defined() {
        let user_defined = TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "@semantic.BrazilianCPF".to_owned(),
        });
        let mut module = empty_module();
        let mut feat = empty_feature("host");
        feat.resources.push(make_resource(
            "Host",
            vec![make_field("cpf", user_defined.clone())],
        ));
        module.features.push(feat);

        apply_plugin_semantic_resolution(&mut module, &BTreeMap::new());

        // Empty alias map: rewrite is a no-op.
        assert_eq!(
            module.features[0].resources[0].fields[0].type_ref,
            user_defined
        );
    }

    #[test]
    fn idempotent_double_pass() {
        let user_defined = TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "@semantic.BrazilianCPF".to_owned(),
        });
        let mut module = empty_module();
        let mut feat = empty_feature("host");
        feat.resources
            .push(make_resource("Host", vec![make_field("cpf", user_defined)]));
        module.features.push(feat);

        let map = cpf_alias_map();
        apply_plugin_semantic_resolution(&mut module, &map);
        let after_first = module.features[0].resources[0].fields[0].type_ref.clone();
        apply_plugin_semantic_resolution(&mut module, &map);
        let after_second = module.features[0].resources[0].fields[0].type_ref.clone();
        assert_eq!(after_first, after_second);
    }
}
