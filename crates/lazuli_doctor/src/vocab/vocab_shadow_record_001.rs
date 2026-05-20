//! VOCAB-SHADOW-RECORD-001 — multi-declaration shadow.
//!
//! Per `docs/proposals/vocab-shadow-record-vo-extraction.md` v0.2 §4.1.
//! Fires when two or more declaration sites within the SAME feature share
//! N or more `(name, type_ref)` pairs after universal-column filtering AND
//! the intersection is ≥ 50% of each side's post-filter field count.
//!
//! Declaration sites considered: `Resource.fields`, `Record.fields`, and
//! `Command.input` (only when `CommandInput::Typed` — `Short` variants
//! reuse the target resource's fields and contribute no new structural
//! information).
//!
//! Severity: `warning` (strict and production profiles).
//! Reference: docs/proposals/vocab-shadow-record-vo-extraction.md
//!            docs/next-checklist.md §VOCAB-SHADOW-RECORD-001

use std::path::{Path, PathBuf};

use lazuli_ir::{CommandInput, Feature, Module, TypeRef};

use super::universal_columns::{is_universal_column, is_view_projection_record};

/// Minimum cluster size for a finding. Authors can override via
/// `Lazurite.toml [doctor.vocab.shadow_record].min_cluster_fields`.
pub const DEFAULT_MIN_CLUSTER_FIELDS: usize = 4;

/// Minimum intersection ratio relative to each declaration's post-filter
/// field count. Authors can override via
/// `Lazurite.toml [doctor.vocab.shadow_record].min_cluster_ratio`.
pub const DEFAULT_MIN_CLUSTER_RATIO: f64 = 0.5;

/// One VOCAB-SHADOW-RECORD-001 finding: a pair of declarations with a
/// structurally similar field cluster. Per-pair finding; if three
/// declarations all share the cluster, the rule emits 3 pairwise findings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub left: DeclarationRef,
    pub right: DeclarationRef,
    pub shared_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationRef {
    pub kind: DeclarationKind,
    pub name: String,
    pub post_filter_field_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationKind {
    Resource,
    Record,
    CommandInput,
}

impl DeclarationKind {
    fn label(&self) -> &'static str {
        match self {
            DeclarationKind::Resource => "resource",
            DeclarationKind::Record => "record",
            DeclarationKind::CommandInput => "command input",
        }
    }
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-SHADOW-RECORD-001";

    pub fn message(&self) -> String {
        format!(
            "{} `{}` and {} `{}` share {} fields with matching types ({}). \
             Consider extracting a `record` and referencing it from both \
             declarations. If they will diverge, add \
             `# doctor:allow VOCAB-SHADOW-RECORD-001 — reason \"...\"` on \
             each declaration.",
            self.left.kind.label(),
            self.left.name,
            self.right.kind.label(),
            self.right.name,
            self.shared_fields.len(),
            self.shared_fields.join(", "),
        )
    }
}

/// Internal projection of a declaration into a comparable field-set form.
struct DeclarationView {
    kind: DeclarationKind,
    name: String,
    fields: Vec<(String, TypeRef)>,
}

/// Run VOCAB-SHADOW-RECORD-001 over one feature's declarations.
pub fn check(feature: &Feature, module: &Module, path: &Path) -> Vec<Finding> {
    check_with_config(
        feature,
        module,
        path,
        DEFAULT_MIN_CLUSTER_FIELDS,
        DEFAULT_MIN_CLUSTER_RATIO,
    )
}

pub fn check_with_config(
    feature: &Feature,
    module: &Module,
    path: &Path,
    min_cluster_fields: usize,
    min_cluster_ratio: f64,
) -> Vec<Finding> {
    let views = collect_declaration_views(feature, module);
    let mut findings = Vec::new();
    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let left = &views[i];
            let right = &views[j];
            let mut shared: Vec<String> = left
                .fields
                .iter()
                .filter(|(name, ty)| {
                    right
                        .fields
                        .iter()
                        .any(|(n2, t2)| n2 == name && t2 == ty)
                })
                .map(|(name, _)| name.clone())
                .collect();
            if shared.len() < min_cluster_fields {
                continue;
            }
            let left_total = left.fields.len();
            let right_total = right.fields.len();
            if left_total == 0 || right_total == 0 {
                continue;
            }
            let left_ratio = shared.len() as f64 / left_total as f64;
            let right_ratio = shared.len() as f64 / right_total as f64;
            if left_ratio < min_cluster_ratio || right_ratio < min_cluster_ratio {
                continue;
            }
            shared.sort();
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                left: DeclarationRef {
                    kind: left.kind.clone(),
                    name: left.name.clone(),
                    post_filter_field_count: left_total,
                },
                right: DeclarationRef {
                    kind: right.kind.clone(),
                    name: right.name.clone(),
                    post_filter_field_count: right_total,
                },
                shared_fields: shared,
            });
        }
    }
    findings
}

fn collect_declaration_views(feature: &Feature, module: &Module) -> Vec<DeclarationView> {
    let mut out = Vec::new();
    for resource in &feature.resources {
        let fields = resource
            .fields
            .iter()
            .filter(|f| !is_universal_column(f, &resource.name, feature, module))
            .map(|f| (f.name.clone(), f.type_ref.clone()))
            .collect::<Vec<_>>();
        if !fields.is_empty() {
            out.push(DeclarationView {
                kind: DeclarationKind::Resource,
                name: resource.name.clone(),
                fields,
            });
        }
    }
    for record in &feature.records {
        if is_view_projection_record(record) {
            continue;
        }
        let fields = record
            .fields
            .iter()
            .filter(|f| !is_universal_column(f, &record.name, feature, module))
            .map(|f| (f.name.clone(), f.type_ref.clone()))
            .collect::<Vec<_>>();
        if !fields.is_empty() {
            out.push(DeclarationView {
                kind: DeclarationKind::Record,
                name: record.name.clone(),
                fields,
            });
        }
    }
    for command in &feature.commands {
        if let CommandInput::Typed(slots) = &command.input {
            let fields = slots
                .iter()
                .map(|s| (s.name.clone(), s.type_ref.clone()))
                .collect::<Vec<_>>();
            if !fields.is_empty() {
                out.push(DeclarationView {
                    kind: DeclarationKind::CommandInput,
                    name: command.name.clone(),
                    fields,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Field,
        FieldConstraints, Module, Policies, PolicyRef, Record, Resource, TypedSlot,
    };

    fn builtin_field(name: &str, ty: BuiltinType, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(ty),
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
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
        }
    }

    fn mk_record(name: &str, fields: Vec<Field>) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields,
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn mk_command_with_typed_input(name: &str, slots: Vec<TypedSlot>) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Typed(slots),
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
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

    fn slot(name: &str, ty: BuiltinType) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(ty),
            required: false,
            constraints: FieldConstraints::default(),
        }
    }

    fn mk_feature(
        name: &str,
        resources: Vec<Resource>,
        records: Vec<Record>,
        commands: Vec<Command>,
    ) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands,
            apis: vec![],
            records,
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
            span_ref: None,
        }
    }

    fn mk_module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features,
        }
    }

    #[test]
    fn two_resources_sharing_a_cluster_fire() {
        let order = mk_resource(
            "Order",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
                builtin_field("country", BuiltinType::Text, true),
            ],
        );
        let return_label = mk_resource(
            "ReturnLabel",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
                builtin_field("country", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("shipping", vec![order, return_label], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/shipping/shipping.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shared_fields.len(), 5);
        assert_eq!(findings[0].left.name, "Order");
        assert_eq!(findings[0].right.name, "ReturnLabel");
        assert_eq!(Finding::CODE, "VOCAB-SHADOW-RECORD-001");
        let msg = findings[0].message();
        assert!(msg.contains("Order"));
        assert!(msg.contains("ReturnLabel"));
        assert!(msg.contains("doctor:allow"));
        assert!(msg.contains("— reason"));
    }

    #[test]
    fn three_way_intersection_emits_three_pairwise_findings() {
        let a = mk_resource(
            "A",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let b = mk_resource(
            "B",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let c = mk_resource(
            "C",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b, c], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert_eq!(findings.len(), 3, "three-way pairwise = 3 findings");
    }

    #[test]
    fn cluster_too_small_does_not_fire() {
        let a = mk_resource(
            "A",
            vec![
                builtin_field("name", BuiltinType::Text, true),
                builtin_field("description", BuiltinType::Text, false),
                builtin_field("created_at", BuiltinType::DateTime, true),
            ],
        );
        let b = mk_resource(
            "B",
            vec![
                builtin_field("name", BuiltinType::Text, true),
                builtin_field("description", BuiltinType::Text, false),
                builtin_field("created_at", BuiltinType::DateTime, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        // After filtering created_at, only 2 fields remain — below N=4.
        assert!(findings.is_empty());
    }

    #[test]
    fn ratio_below_50_percent_does_not_fire() {
        // A has 10 fields, B has 4 fields. Intersection is 4. 4/10=40%,
        // 4/4=100%. Left ratio 40% < 50% — must not fire.
        let mut a_fields = vec![
            builtin_field("street", BuiltinType::Text, true),
            builtin_field("city", BuiltinType::Text, true),
            builtin_field("state", BuiltinType::Text, true),
            builtin_field("postal_code", BuiltinType::Text, true),
        ];
        for i in 0..6 {
            a_fields.push(builtin_field(
                &format!("extra_{i}"),
                BuiltinType::Text,
                false,
            ));
        }
        let a = mk_resource("WideResource", a_fields);
        let b = mk_resource(
            "Narrow",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn resource_vs_command_input_fires() {
        let order = mk_resource(
            "Order",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let update = mk_command_with_typed_input(
            "update_order_address",
            vec![
                slot("street", BuiltinType::Text),
                slot("city", BuiltinType::Text),
                slot("state", BuiltinType::Text),
                slot("postal_code", BuiltinType::Text),
            ],
        );
        let feature = mk_feature("orders", vec![order], vec![], vec![update]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/orders/orders.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(matches!(
            findings[0].right.kind,
            DeclarationKind::CommandInput
        ));
    }

    #[test]
    fn record_vs_resource_fires() {
        let record = mk_record(
            "Address",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let property = mk_resource(
            "Property",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("realty", vec![property], vec![record], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/realty/realty.lzi"));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn view_projection_record_is_excluded() {
        // PendingReviewEntry ends in Entry + has review_id: ID required —
        // matches view-projection filter. Even if its fields mirror a
        // resource, it must not produce a finding.
        let view = mk_record(
            "PendingReviewEntry",
            vec![
                builtin_field("review_id", BuiltinType::Id, true),
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let resource = mk_resource(
            "Order",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![resource], vec![view], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn empty_declarations_do_not_produce_division_by_zero() {
        let empty_resource = mk_resource("Empty", vec![]);
        let other = mk_resource(
            "Other",
            vec![
                builtin_field("a", BuiltinType::Text, true),
                builtin_field("b", BuiltinType::Text, true),
                builtin_field("c", BuiltinType::Text, true),
                builtin_field("d", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![empty_resource, other], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn config_override_lowers_ratio_threshold() {
        // Same setup as ratio_below_50_percent_does_not_fire, but lower
        // the threshold and confirm the rule then fires.
        let mut a_fields = vec![
            builtin_field("street", BuiltinType::Text, true),
            builtin_field("city", BuiltinType::Text, true),
            builtin_field("state", BuiltinType::Text, true),
            builtin_field("postal_code", BuiltinType::Text, true),
        ];
        for i in 0..6 {
            a_fields.push(builtin_field(
                &format!("extra_{i}"),
                BuiltinType::Text,
                false,
            ));
        }
        let a = mk_resource("WideResource", a_fields);
        let b = mk_resource(
            "Narrow",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check_with_config(
            &feature,
            &module,
            Path::new("features/x/x.lzi"),
            DEFAULT_MIN_CLUSTER_FIELDS,
            0.30,
        );
        assert_eq!(findings.len(), 1);
    }
}
