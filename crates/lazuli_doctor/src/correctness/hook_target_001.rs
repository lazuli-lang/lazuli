//! HOOK-TARGET-001 — extension hook references an undefined target.
//!
//! Doctor correctness rule (NOT a vocab rule — surfaces a typo / dangling
//! reference, not vocabulary drift). Fires when `hook x: Hook[Foo]`
//! references `Foo` and no command/query/job/record/event/resource of that
//! name exists in the same feature.
//!
//! Severity: `error` (strict and production). Dangling references are
//! correctness bugs, not style.
//!
//! Reference: docs/proposals/doctor-vocabulary-lints.md (correctness, not
//! `VOCAB-*` catalog).

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Extension, ExtensionContract, Feature, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

/// One HOOK-TARGET-001 finding: a hook target that does not resolve locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Feature containing the hook.
    pub feature: String,
    /// Extension hook name (`hook <name>: Hook[...]`).
    pub hook_name: String,
    /// Missing target inside `Hook[...]`.
    pub unresolved_target: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "HOOK-TARGET-001";

    /// Render the "hook target not declared" message naming the hook,
    /// the missing target, and the feature it was expected in.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::hook_target_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     hook_name: "after_charge".into(),
    ///     unresolved_target: "MissingCommand".into(),
    /// };
    /// assert!(f.message().contains("Hook["));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "hook `{}: Hook[{}]` references `{}` but no command/query/job/record/event/resource \
             named `{}` exists in feature `{}`. Either declare the target or remove the hook.",
            self.hook_name,
            self.unresolved_target,
            self.unresolved_target,
            self.unresolved_target,
            self.feature
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run HOOK-TARGET-001 for all extension hooks in one feature.
///
/// `path` is the source `.lzi` file; no I/O is performed here. Cross-feature
/// hook targets (`other_feature.Foo`) are intentionally skipped because this
/// rule only checks same-feature resolution.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::hook_target_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with extension hooks");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let declared = collect_declared_names(feature);

    feature
        .extensions
        .iter()
        .filter_map(|ext| hook_target(ext).map(|target| (ext, target)))
        .filter(|(_, target)| !target.contains('.') && !declared.contains(target.as_ref()))
        .map(|(ext, target)| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            hook_name: ext.name.clone(),
            unresolved_target: target.into_owned(),
        })
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

fn collect_declared_names(feature: &Feature) -> HashSet<String> {
    let mut declared = HashSet::new();

    declared.extend(feature.commands.iter().map(|cmd| cmd.name.clone()));
    declared.extend(feature.queries.iter().map(|query| query.name().to_owned()));
    declared.extend(feature.jobs.iter().map(|job| job.name.clone()));
    declared.extend(feature.records.iter().map(|record| record.name.clone()));
    declared.extend(feature.events.iter().map(|event| event.name.clone()));
    declared.extend(
        feature
            .resources
            .iter()
            .map(|resource| resource.name.clone()),
    );

    declared
}

fn hook_target(ext: &Extension) -> Option<Cow<'_, str>> {
    match &ext.contract {
        ExtensionContract::Hook { type_arg } => type_ref_name(type_arg),
        _ => None,
    }
}

fn type_ref_name(type_ref: &TypeRef) -> Option<Cow<'_, str>> {
    match type_ref {
        TypeRef::Unresolved(name) => Some(Cow::Borrowed(name.as_str())),
        TypeRef::UserDefined(name) | TypeRef::EnumRef(name) => {
            if let Some(feature) = &name.feature {
                Some(Cow::Owned(format!("{}.{}", feature, name.name)))
            } else {
                Some(Cow::Borrowed(name.name.as_str()))
            }
        }
        _ => None,
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Event, EventKind,
        Field, FieldConstraints, OutboxMode, Policies, PolicyRef, QualifiedName, Record, Resource,
        ReturnsEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn qualified_qn(feature: &str, name: &str) -> QualifiedName {
        QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    }

    fn mk_feature(
        commands: Vec<Command>,
        events: Vec<Event>,
        extensions: Vec<Extension>,
    ) -> Feature {
        Feature {
            name: "post".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events,
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
            extensions,
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

    fn mk_cmd(name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(BuiltinType::Boolean),
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
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_event(name: &str) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload: vec![],
            payload_none: true,
            level: None,
            outbox: OutboxMode::None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_hook(name: &str, target: TypeRef) -> Extension {
        Extension {
            name: name.to_owned(),
            contract: ExtensionContract::Hook { type_arg: target },
            resolved_path: lazuli_ir::PathRef::convention(format!("handlers/{name}.go")),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_non_hook(name: &str) -> Extension {
        Extension {
            name: name.to_owned(),
            contract: ExtensionContract::CellRenderer {
                type_arg: TypeRef::UserDefined(qn("Post")),
            },
            resolved_path: lazuli_ir::PathRef::convention(format!("web/{name}.tsx")),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields: vec![Field {
                name: "id".into(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
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
            }],
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
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

    #[test]
    fn positive_hook_unresolved_target_fires() {
        let feature = mk_feature(
            vec![],
            vec![],
            vec![mk_hook(
                "refresh_ltv",
                TypeRef::Unresolved("NonExistent".into()),
            )],
        );

        let findings = check(&feature, Path::new("features/post/post.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].hook_name, "refresh_ltv");
        assert_eq!(findings[0].unresolved_target, "NonExistent");
        assert_eq!(Finding::CODE, "HOOK-TARGET-001");
        assert!(findings[0].message().contains("Hook[NonExistent]"));
    }

    #[test]
    fn negative_hook_resolved_to_command_does_not_fire() {
        let feature = mk_feature(
            vec![mk_cmd("update_post")],
            vec![],
            vec![mk_hook(
                "after_update",
                TypeRef::Unresolved("update_post".into()),
            )],
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_hook_resolved_to_event_does_not_fire() {
        let feature = mk_feature(
            vec![],
            vec![mk_event("post_archived")],
            vec![mk_hook(
                "after_archive",
                TypeRef::UserDefined(qn("post_archived")),
            )],
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_qualified_target_skipped() {
        let feature = mk_feature(
            vec![],
            vec![],
            vec![mk_hook(
                "after_external",
                TypeRef::UserDefined(qualified_qn("other_feature", "Foo")),
            )],
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_non_hook_extension_skipped() {
        let feature = mk_feature(vec![], vec![], vec![mk_non_hook("ltv_cell")]);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_hook_resolved_to_record_or_resource_does_not_fire() {
        let mut feature = mk_feature(
            vec![],
            vec![],
            vec![
                mk_hook("record_hook", TypeRef::UserDefined(qn("CustomerLtv"))),
                mk_hook("resource_hook", TypeRef::UserDefined(qn("Post"))),
            ],
        );
        feature.records = vec![mk_record("CustomerLtv")];
        feature.resources = vec![mk_resource("Post")];

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
