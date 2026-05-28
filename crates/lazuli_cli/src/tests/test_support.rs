    // Shared codegen-test fixtures + helpers — split from
    // `crates/lazuli_cli/src/tests.rs` by the R10-A pass. These helpers were
    // private fns inside the original `mod tests { ... }` block; promoting
    // them to `pub(super)` lets sibling test sub-modules reach them via
    // `use super::test_support::*;` without exposing anything outside the
    // `#[cfg(test)]`-gated test module.

    pub(super) fn enum_sdk_fixture(
        include_unused_enum: bool,
        include_second_resource: bool,
    ) -> (lazuli_ir::Feature, lazuli_ir::Module) {
        let mut enums = vec![lazuli_ir::EnumDecl {
            name: "ItemType".to_owned(),
            public_contract: None,
            variants: vec![
                lazuli_ir::EnumVariant {
                    name: "Doc".to_owned(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                },
                lazuli_ir::EnumVariant {
                    name: "Decision".to_owned(),
                    storage_value: Some(lazuli_ir::StorageValue::String("decision".to_owned())),
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                },
            ],
            previous_names: vec![],
            span_ref: None,
        }];
        if include_unused_enum {
            enums.push(lazuli_ir::EnumDecl {
                name: "Unused".to_owned(),
                public_contract: None,
                variants: vec![lazuli_ir::EnumVariant {
                    name: "Legacy".to_owned(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                }],
                previous_names: vec![],
                span_ref: None,
            });
        }

        let mut resources = vec![resource(
            "Item",
            vec![
                field("type", lazuli_ir::TypeRef::EnumRef(local_qn("ItemType"))),
                field(
                    "tags",
                    lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::Builtin(
                        lazuli_ir::BuiltinType::Text,
                    ))),
                ),
                field(
                    "categories",
                    lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::EnumRef(local_qn(
                        "ItemType",
                    )))),
                ),
            ],
        )];
        if include_second_resource {
            resources.push(resource(
                "Note",
                vec![field(
                    "type",
                    lazuli_ir::TypeRef::EnumRef(local_qn("ItemType")),
                )],
            ));
        }

        let feature = lazuli_ir::Feature {
            name: "item".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: lazuli_ir::Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums,
            resources,
            events: vec![],
            rules: vec![],
            policies: lazuli_ir::Policies::default(),
            errors: None,
            commands: vec![],
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
        };
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        (feature, module)
    }

    pub(super) fn resource(name: &str, fields: Vec<lazuli_ir::Field>) -> lazuli_ir::Resource {
        lazuli_ir::Resource {
            name: name.to_owned(),
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
            conventions: vec![],
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        }
    }

    pub(super) fn field(name: &str, type_ref: lazuli_ir::TypeRef) -> lazuli_ir::Field {
        lazuli_ir::Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    // typed_slot consolidated to a-1's 3-arg form (explicit required).
    // HEAD's 2-arg callers updated to pass `true` for the required flag.
    pub(super) fn typed_slot(
        name: &str,
        type_ref: lazuli_ir::TypeRef,
        required: bool,
    ) -> lazuli_ir::TypedSlot {
        lazuli_ir::TypedSlot {
            name: name.to_owned(),
            type_ref,
            required,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        }
    }

    pub(super) fn command_with_typed_input(
        name: &str,
        slots: Vec<lazuli_ir::TypedSlot>,
    ) -> lazuli_ir::Command {
        command(
            name,
            lazuli_ir::CommandKind::Update,
            lazuli_ir::CommandInput::Typed(slots),
            lazuli_ir::CommandEffect::None,
        )
    }

    pub(super) fn command(
        name: &str,
        kind: lazuli_ir::CommandKind,
        input: lazuli_ir::CommandInput,
        effect: lazuli_ir::CommandEffect,
    ) -> lazuli_ir::Command {
        lazuli_ir::Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
            route: vec![],
            input,
            target: None,
            lets: vec![],
            effect,
            policy: lazuli_ir::PolicyRef::None,
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
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    pub(super) fn list_query(name: &str) -> lazuli_ir::Query {
        lazuli_ir::Query::List(lazuli_ir::ListQuery {
            name: name.to_owned(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        })
    }

    pub(super) fn pure_read_list_command(name: &str, resource_name: &str) -> lazuli_ir::Command {
        command(
            name,
            lazuli_ir::CommandKind::Returns,
            lazuli_ir::CommandInput::Typed(vec![]),
            lazuli_ir::CommandEffect::Returns(lazuli_ir::ReturnsEffect {
                return_type: lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::UserDefined(
                    local_qn(resource_name),
                ))),
            }),
        )
    }

    pub(super) fn local_qn(name: &str) -> lazuli_ir::QualifiedName {
        lazuli_ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    pub(super) fn occurrences(haystack: &str, needle: &str) -> usize {
        haystack.match_indices(needle).count()
    }
