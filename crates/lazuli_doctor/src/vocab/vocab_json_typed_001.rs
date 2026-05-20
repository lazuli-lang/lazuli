//! VOCAB-JSON-TYPED-001 — untyped JSON bag + sibling closed-catalog enum.
//!
//! Fires when a resource carries a `JSON` field while the same feature declares
//! a related enum that is not referenced by any typed slot. That pattern means
//! the enum documents a closed shape but the IR still sees an unconstrained bag.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{
    BuiltinType, CommandEffect, CommandInput, EnumDecl, Feature, Field, JobBody, Query, Resource,
    TypeRef, TypedSlot,
};

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-JSON-TYPED-001 finding: a JSON bag with an orphan enum catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// The untyped JSON field.
    pub json_field: String,
    /// Related same-feature enum that is not referenced anywhere.
    pub orphan_enum: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-JSON-TYPED-001";

    pub fn message(&self) -> String {
        format!(
            "resource `{}` has untyped `{}: JSON` field with sibling enum `{}` \
             that documents the shape but isn't referenced anywhere — \
             consider a discriminated union OR a `record` type so the IR \
             carries the constraint, not just the documentation.",
            self.resource, self.json_field, self.orphan_enum
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-JSON-TYPED-001 over one feature's resources.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let referenced_enums = collect_referenced_enums(feature);
    feature
        .resources
        .iter()
        .flat_map(|resource| check_resource(resource, &feature.enums, &referenced_enums, path))
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

fn collect_referenced_enums(feature: &Feature) -> HashSet<&str> {
    let mut refs = HashSet::new();

    for resource in &feature.resources {
        collect_field_refs(&resource.fields, &mut refs);
    }
    for record in &feature.records {
        collect_field_refs(&record.fields, &mut refs);
    }
    for event in &feature.events {
        for field in &event.payload {
            collect_type_ref(&field.type_ref, &mut refs);
        }
    }
    for command in &feature.commands {
        for route_slot in &command.route {
            collect_type_ref(&route_slot.type_ref, &mut refs);
        }
        if let CommandInput::Typed(slots) = &command.input {
            collect_typed_slot_refs(slots, &mut refs);
        }
        if let CommandEffect::Returns(effect) = &command.effect {
            collect_type_ref(&effect.return_type, &mut refs);
        }
    }
    for query in &feature.queries {
        collect_query_refs(query, &mut refs);
    }
    for job in &feature.jobs {
        if let JobBody::Handler(handler) = &job.body {
            if let Some(return_type) = &handler.returns {
                collect_type_ref(return_type, &mut refs);
            }
        }
    }
    for webhook in &feature.webhooks {
        if let Some(return_type) = &webhook.returns {
            collect_type_ref(return_type, &mut refs);
        }
    }

    refs
}

fn collect_field_refs<'a>(fields: &'a [Field], refs: &mut HashSet<&'a str>) {
    for field in fields {
        collect_type_ref(&field.type_ref, refs);
    }
}

fn collect_typed_slot_refs<'a>(slots: &'a [TypedSlot], refs: &mut HashSet<&'a str>) {
    for slot in slots {
        collect_type_ref(&slot.type_ref, refs);
    }
}

fn collect_query_refs<'a>(query: &'a Query, refs: &mut HashSet<&'a str>) {
    match query {
        Query::List(q) => collect_typed_slot_refs(&q.params, refs),
        Query::Lookup(q) => collect_typed_slot_refs(&q.params, refs),
        Query::Sql(q) => {
            collect_typed_slot_refs(&q.params, refs);
            collect_type_ref(&q.returns, refs);
        }
    }
}

fn collect_type_ref<'a>(type_ref: &'a TypeRef, refs: &mut HashSet<&'a str>) {
    match type_ref {
        TypeRef::EnumRef(qn) => {
            refs.insert(qn.name.as_str());
        }
        TypeRef::Many(inner) => collect_type_ref(inner, refs),
        _ => {}
    }
}

fn check_resource(
    resource: &Resource,
    enums: &[EnumDecl],
    referenced: &HashSet<&str>,
    path: &Path,
) -> Vec<Finding> {
    if resource.fields.len() < 2 {
        return vec![];
    }

    let json_fields: Vec<&Field> = resource
        .fields
        .iter()
        .filter(|field| matches!(&field.type_ref, TypeRef::Builtin(BuiltinType::Json)))
        .collect();

    if json_fields.is_empty() {
        return vec![];
    }

    let mut findings = Vec::new();
    for json_field in json_fields {
        // Future false-positive guard: when source-map facts expose comments,
        // suppress fields with an explicit `# typed-by <enum>` pragma.
        for enum_decl in enums {
            if referenced.contains(enum_decl.name.as_str()) {
                continue;
            }
            if !thematically_related(&enum_decl.name, &json_field.name, &resource.name) {
                continue;
            }

            findings.push(Finding {
                path: path.to_path_buf(),
                resource: resource.name.clone(),
                json_field: json_field.name.clone(),
                orphan_enum: enum_decl.name.clone(),
            });
        }
    }
    findings
}

fn thematically_related(enum_name: &str, field_name: &str, resource_name: &str) -> bool {
    let enum_lower = enum_name.to_lowercase();
    let field_lower = field_name.to_lowercase();
    let resource_lower = resource_name.to_lowercase();
    enum_lower.contains(&field_lower)
        || enum_lower.contains(&format!("{resource_lower}type"))
        || enum_lower.starts_with(&resource_lower)
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, EnumVariant, Policies, QualifiedName, Resource};

    fn mk_field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            span_ref: None,
        }
    }

    fn mk_enum(name: &str, variants: &[&str]) -> EnumDecl {
        EnumDecl {
            name: name.into(),
            public_contract: None,
            variants: variants
                .iter()
                .map(|variant| EnumVariant {
                    name: variant.to_string(),
                    storage_value: None,
                    previous_names: vec![],
                })
                .collect(),
            previous_names: vec![],
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
        }
    }

    fn mk_feature(enums: Vec<EnumDecl>, resources: Vec<Resource>) -> Feature {
        Feature {
            name: "test".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums,
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
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
            span_ref: None,
        }
    }

    fn enum_ref(name: &str) -> TypeRef {
        TypeRef::EnumRef(QualifiedName {
            feature: None,
            name: name.into(),
        })
    }

    fn json() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Json)
    }

    fn text() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    #[test]
    fn positive_quiz_questions_json_with_orphan_question_type_enum_fires() {
        let feature = mk_feature(
            vec![mk_enum(
                "QuizQuestionType",
                &["MultipleChoice", "TrueFalse", "FillIn"],
            )],
            vec![mk_resource(
                "Quiz",
                vec![
                    mk_field("title", text(), true),
                    mk_field("questions", json(), true),
                ],
            )],
        );

        let findings = check(&feature, Path::new("features/quiz/quiz.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Quiz");
        assert_eq!(findings[0].json_field, "questions");
        assert_eq!(findings[0].orphan_enum, "QuizQuestionType");
        assert_eq!(Finding::CODE, "VOCAB-JSON-TYPED-001");
    }

    #[test]
    fn negative_enum_referenced_elsewhere_does_not_fire() {
        let feature = mk_feature(
            vec![mk_enum(
                "QuizQuestionType",
                &["MultipleChoice", "TrueFalse", "FillIn"],
            )],
            vec![
                mk_resource(
                    "Quiz",
                    vec![
                        mk_field("title", text(), true),
                        mk_field("questions", json(), true),
                    ],
                ),
                mk_resource(
                    "Question",
                    vec![
                        mk_field("text", text(), true),
                        mk_field("kind", enum_ref("QuizQuestionType"), true),
                    ],
                ),
            ],
        );

        let findings = check(&feature, Path::new("features/quiz/quiz.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_json_field_without_matching_enum_does_not_fire() {
        let feature = mk_feature(
            vec![mk_enum("WorkflowStatus", &["Draft", "Live"])],
            vec![mk_resource(
                "Article",
                vec![
                    mk_field("title", text(), true),
                    mk_field("metadata", json(), true),
                ],
            )],
        );

        let findings = check(&feature, Path::new("features/article/article.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_resource_with_only_json_field_does_not_fire() {
        let feature = mk_feature(
            vec![mk_enum("PayloadType", &["A", "B"])],
            vec![mk_resource(
                "Payload",
                vec![mk_field("payload", json(), true)],
            )],
        );

        let findings = check(&feature, Path::new("features/payload/payload.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_resources_each_fire_independently() {
        let feature = mk_feature(
            vec![
                mk_enum("QuizQuestionType", &["MultipleChoice", "TrueFalse"]),
                mk_enum("SurveyAnswerType", &["Scale", "FreeText"]),
            ],
            vec![
                mk_resource(
                    "Quiz",
                    vec![
                        mk_field("title", text(), true),
                        mk_field("questions", json(), true),
                    ],
                ),
                mk_resource(
                    "Survey",
                    vec![
                        mk_field("name", text(), true),
                        mk_field("answers", json(), true),
                    ],
                ),
            ],
        );

        let findings = check(&feature, Path::new("features/forms/forms.lzi"));

        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].resource, "Quiz");
        assert_eq!(findings[1].resource, "Survey");
    }
}
