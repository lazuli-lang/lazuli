//! VOCAB-LIFECYCLE-001 — transition commands over a status enum.
//!
//! Fires when a resource declares a status-like enum field and at least three
//! commands update that resource by advancing the enum through named states.
//! Suggests refactoring the repeated commands + enum into a resource-local
//! `lifecycle` block.
//!
//! Severity: `warning` (strict-profile), `error` (production-profile).
//! Reference: docs/proposals/doctor-vocabulary-lints.md §VOCAB-LIFECYCLE-001
//! Destination: docs/proposals/lifecycle-vocab.md §3

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{
    Command, CommandEffect, EnumDecl, Expr, Feature, Field, QualifiedName, Resource, TypeRef,
};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-LIFECYCLE-001 finding: a resource still spelling a lifecycle as
/// N transition commands plus a status enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource that owns the status field.
    pub resource: String,
    /// Status-like enum field (`status`, `state`, `phase`, `stage`, `step`).
    pub status_field: String,
    /// Enum type backing the status field.
    pub enum_name: String,
    /// Commands that look like state transitions.
    pub transition_commands: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-LIFECYCLE-001";

    pub fn message(&self) -> String {
        format!(
            "resource `{}` has {} transition command(s) ({}) advancing field `{}: {}` \
             — refactor to a `lifecycle {}` block on the resource. The new vocabulary \
             collapses commands + enum + invariants into one declaration with closed-\
             catalog invariants (terminal_immutable, single <state> per <scope>, \
             no_jump_more_than_one). See docs/proposals/lifecycle-vocab.md §3.",
            self.resource,
            self.transition_commands.len(),
            self.transition_commands.join(", "),
            self.status_field,
            self.enum_name,
            self.status_field
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-LIFECYCLE-001 over one feature's resources.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let variants = build_variant_map(&feature.enums);
    feature
        .resources
        .iter()
        .filter_map(|resource| check_resource(resource, feature, &variants, path))
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

/// `enum_name -> variant_names_lowercase` for same-feature enums.
fn build_variant_map(enums: &[EnumDecl]) -> HashMap<String, HashSet<String>> {
    enums
        .iter()
        .map(|e| {
            let variants = e.variants.iter().map(|v| v.name.to_lowercase()).collect();
            (e.name.clone(), variants)
        })
        .collect()
}

fn check_resource(
    resource: &Resource,
    feature: &Feature,
    variants_by_enum: &HashMap<String, HashSet<String>>,
    path: &Path,
) -> Option<Finding> {
    if resource.lifecycle.is_some() {
        return None;
    }

    let (status_field, enum_name) = resource
        .fields
        .iter()
        .find_map(|field| status_enum_field(field, variants_by_enum))?;
    let variants = variants_by_enum.get(enum_name.as_str())?;

    let transition_commands: Vec<String> = feature
        .commands
        .iter()
        .filter(|cmd| updates_resource(cmd, &resource.name))
        .filter(|cmd| command_advances_status(cmd, status_field.as_str(), variants))
        .map(|cmd| cmd.name.clone())
        .collect();

    if transition_commands.len() < 3 {
        return None;
    }

    Some(Finding {
        path: path.to_path_buf(),
        resource: resource.name.clone(),
        status_field,
        enum_name,
        transition_commands,
    })
}

fn status_enum_field(
    field: &Field,
    variants_by_enum: &HashMap<String, HashSet<String>>,
) -> Option<(String, String)> {
    if !is_status_like_field(&field.name) {
        return None;
    }

    let TypeRef::EnumRef(qn) = &field.type_ref else {
        return None;
    };

    if qn.feature.is_none() && variants_by_enum.contains_key(&qn.name) {
        Some((field.name.clone(), qn.name.clone()))
    } else {
        None
    }
}

fn is_status_like_field(name: &str) -> bool {
    matches!(name, "status" | "state" | "phase" | "stage" | "step")
}

fn updates_resource(cmd: &Command, resource_name: &str) -> bool {
    match &cmd.effect {
        CommandEffect::Updates(effect) => qn_matches_local(&effect.resource, resource_name),
        _ => false,
    }
}

fn qn_matches_local(qn: &QualifiedName, name: &str) -> bool {
    qn.feature.is_none() && qn.name == name
}

fn command_advances_status(cmd: &Command, status_field: &str, variants: &HashSet<String>) -> bool {
    assignment_sets_status_to_variant(cmd, status_field, variants)
        || command_name_matches_variant(&cmd.name, variants)
}

fn assignment_sets_status_to_variant(
    cmd: &Command,
    status_field: &str,
    variants: &HashSet<String>,
) -> bool {
    let CommandEffect::Updates(effect) = &cmd.effect else {
        return false;
    };

    effect.assignments.iter().any(|assignment| {
        assignment.field == status_field && expr_names_variant(&assignment.value, variants)
    })
}

fn expr_names_variant(expr: &Expr, variants: &HashSet<String>) -> bool {
    match expr {
        Expr::Enum(lit) => variants.contains(&lit.variant.to_lowercase()),
        Expr::Path(path) => path
            .segments
            .last()
            .is_some_and(|segment| variants.contains(&segment.to_lowercase())),
        _ => false,
    }
}

fn command_name_matches_variant(name: &str, variants: &HashSet<String>) -> bool {
    let lower = name.to_lowercase();
    variants.iter().any(|variant| {
        lower == *variant
            || lower == format!("mark_{variant}")
            || lower == format!("set_{variant}")
            || lower == format!("transition_to_{variant}")
            || lower.ends_with(&format!("_{variant}"))
    })
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Assignment, CommandInput, CommandKind, Defaults, EnumLiteral, EnumVariant,
        FieldConstraints, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, PolicyRef, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn enum_ref(name: &str) -> TypeRef {
        TypeRef::EnumRef(qn(name))
    }

    fn mk_enum(name: &str, variants: &[&str]) -> EnumDecl {
        EnumDecl {
            name: name.into(),
            variants: variants
                .iter()
                .map(|variant| EnumVariant {
                    name: (*variant).to_owned(),
                    storage_value: None,
                    previous_names: vec![],
                })
                .collect(),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, status_field: &str, lifecycle: Option<Lifecycle>) -> Resource {
        Resource {
            name: name.into(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![mk_field(status_field, enum_ref("PublicationStatus"))],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle,
        }
    }

    fn mk_lifecycle() -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states: vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("publishing", LifecycleStateKind::Intermediate),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            transitions: vec![mk_transition("begin_publishing", "scheduled", "publishing")],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_state(name: &str, kind: LifecycleStateKind) -> LifecycleState {
        LifecycleState {
            name: name.into(),
            kind,
            span_ref: None,
        }
    }

    fn mk_transition(name: &str, from: &str, to: &str) -> LifecycleTransition {
        LifecycleTransition {
            name: name.into(),
            from: vec![from.into()],
            to: to.into(),
            policy: None,
            audit: None,
            timestamps: None,
            emits: vec![],
            requires: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_cmd(name: &str, resource: &str, status_field: &str, variant: &str) -> Command {
        Command {
            name: name.into(),
            kind: CommandKind::Update,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Updates(UpdateEffect {
                resource: qn(resource),
                assignments: vec![Assignment {
                    field: status_field.into(),
                    value: Expr::Enum(EnumLiteral {
                        type_name: Some(qn("PublicationStatus")),
                        variant: variant.into(),
                    }),
                }],
            }),
            policy: PolicyRef::None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            deprecated: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resource: Resource, commands: Vec<Command>, enum_field_name: &str) -> Feature {
        Feature {
            name: "publishing".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            requirements: vec![],
            enums: vec![mk_enum(
                enum_field_name,
                &["scheduled", "publishing", "published", "cancelled"],
            )],
            resources: vec![resource],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn three_transition_commands(status_field: &str) -> Vec<Command> {
        vec![
            mk_cmd("mark_publishing", "Publication", status_field, "publishing"),
            mk_cmd("mark_published", "Publication", status_field, "published"),
            mk_cmd("mark_cancelled", "Publication", status_field, "cancelled"),
        ]
    }

    #[test]
    fn positive_three_transition_commands_fire() {
        let resource = mk_resource("Publication", "status", None);
        let feature = mk_feature(
            resource,
            three_transition_commands("status"),
            "PublicationStatus",
        );

        let findings = check(&feature, Path::new("features/publication/publication.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].status_field, "status");
        assert_eq!(findings[0].enum_name, "PublicationStatus");
        assert_eq!(Finding::CODE, "VOCAB-LIFECYCLE-001");
    }

    #[test]
    fn negative_resource_already_has_lifecycle_does_not_fire() {
        let resource = mk_resource("Publication", "status", Some(mk_lifecycle()));
        let feature = mk_feature(
            resource,
            three_transition_commands("status"),
            "PublicationStatus",
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "resource with lifecycle must not trigger VOCAB-LIFECYCLE-001"
        );
    }

    #[test]
    fn negative_fewer_than_three_commands_does_not_fire() {
        let resource = mk_resource("Publication", "status", None);
        let feature = mk_feature(
            resource,
            vec![
                mk_cmd("mark_publishing", "Publication", "status", "publishing"),
                mk_cmd("mark_published", "Publication", "status", "published"),
            ],
            "PublicationStatus",
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "two transition commands are below the rule threshold"
        );
    }

    #[test]
    fn negative_non_status_field_name_does_not_fire() {
        let resource = mk_resource("Publication", "category", None);
        let feature = mk_feature(
            resource,
            three_transition_commands("category"),
            "PublicationStatus",
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "enum field names outside the closed status catalog must not fire"
        );
    }

    #[test]
    fn positive_finding_lists_command_names() {
        let resource = mk_resource("Publication", "status", None);
        let feature = mk_feature(
            resource,
            three_transition_commands("status"),
            "PublicationStatus",
        );

        let findings = check(&feature, Path::new("features/publication/publication.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].transition_commands,
            vec![
                "mark_publishing".to_owned(),
                "mark_published".to_owned(),
                "mark_cancelled".to_owned(),
            ]
        );
    }
}
