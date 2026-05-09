//! Lowering from `lazuli_syntax::Document` (the legacy `aggregate { ... }`
//! parser AST) into the canonical `lazuli_ir::Module`.
//!
//! The legacy parser predates canonical syntax. We bridge it by synthesising
//! one feature per parsed `Document`. Each `aggregate Foo` becomes a
//! `Resource Foo` inside that synthetic feature. Commands, queries, and
//! surfaces from the legacy AST are lowered into their nearest canonical IR
//! shape with conservative defaults; richer constructs (workflows, rules,
//! events, raw queries, `route`, `let`, typed inputs) only land when the
//! canonical parser arrives in a later phase.
//!
//! Phase 1a goal: every `examples/crm.lzi` shape lowers cleanly. Anything
//! requiring canonical-only constructs will surface as `TypeRef::Unresolved`
//! or `PolicyRef::Unresolved` rather than fabricated facts.

use std::collections::BTreeSet;

use lazuli_ir as ir;
use lazuli_syntax as syntax;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("duplicate aggregate `{name}`")]
    DuplicateAggregate { name: String },

    #[error("duplicate field `{field}` in aggregate `{aggregate}`")]
    DuplicateField { aggregate: String, field: String },

    #[error("unknown field `{field}` referenced by `{context}` in aggregate `{aggregate}`")]
    UnknownField {
        aggregate: String,
        context: String,
        field: String,
    },

    #[error("command `{command}` in aggregate `{aggregate}` is missing an explicit policy")]
    MissingCommandPolicy { aggregate: String, command: String },
}

pub fn lower_document(document: &syntax::Document) -> Result<ir::Module, AnalyzeError> {
    let mut aggregate_names = BTreeSet::new();
    let mut resources = Vec::new();
    let mut commands = Vec::new();
    let mut queries = Vec::new();

    for aggregate in &document.aggregates {
        if !aggregate_names.insert(aggregate.name.clone()) {
            return Err(AnalyzeError::DuplicateAggregate {
                name: aggregate.name.clone(),
            });
        }

        let lowered = lower_aggregate(aggregate)?;
        resources.push(lowered.resource);
        commands.extend(lowered.commands);
        queries.extend(lowered.queries);
    }

    let feature_name = document
        .app
        .clone()
        .map(|name| name.to_ascii_lowercase())
        .unwrap_or_else(|| "lazuli_app".to_owned());

    let feature = ir::Feature {
        name: feature_name,
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: ir::Defaults::default(),
        uses: Vec::new(),
        enums: Vec::new(),
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies: ir::Policies::default(),
        commands,
        queries,
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        previous_names: Vec::new(),
        span_ref: Some(ir::SpanRef {
            start: document.span.start,
            end: document.span.end,
        }),
    };

    Ok(ir::Module {
        features: vec![feature],
    })
}

pub fn lower_lzx_document(document: &syntax::LzxDocument) -> ir::ExperienceModule {
    ir::ExperienceModule {
        experiences: document.experiences.iter().map(lower_experience).collect(),
        surfaces: document
            .surfaces
            .iter()
            .map(lower_platform_surface)
            .collect(),
    }
}

fn lower_experience(experience: &syntax::LzxExperience) -> ir::Experience {
    ir::Experience {
        name: experience.name.clone(),
        imports: experience.imports.clone(),
        views: experience.views.iter().map(lower_experience_view).collect(),
        span_ref: Some(span_of(experience.span)),
    }
}

fn lower_experience_view(view: &syntax::LzxExperienceView) -> ir::ExperienceView {
    ir::ExperienceView {
        name: view.name.clone(),
        anchor: view.anchor.clone(),
        source: view.source.clone(),
        submit: view.submit.clone(),
        actions: view.actions.iter().map(lower_experience_action).collect(),
        opens: view.opens.clone(),
        span_ref: Some(span_of(view.span)),
    }
}

fn lower_experience_action(action: &syntax::LzxAction) -> ir::ExperienceAction {
    ir::ExperienceAction {
        name: action.name.clone(),
        target: action.target.clone(),
        span_ref: Some(span_of(action.span)),
    }
}

fn lower_platform_surface(surface: &syntax::LzxSurface) -> ir::PlatformSurface {
    ir::PlatformSurface {
        experience: surface.experience.clone(),
        platform: match surface.platform {
            syntax::LzxPlatform::Web => ir::Platform::Web,
            syntax::LzxPlatform::Mobile => ir::Platform::Mobile,
        },
        uses_experience: surface.uses_experience.clone(),
        audiences: surface
            .audiences
            .iter()
            .map(lower_audience_surface)
            .collect(),
        span_ref: Some(span_of(surface.span)),
    }
}

fn lower_audience_surface(audience: &syntax::LzxAudience) -> ir::AudienceSurface {
    ir::AudienceSurface {
        name: audience.name.clone(),
        qualifiers: audience.qualifiers.clone(),
        views: audience.views.iter().map(lower_platform_view).collect(),
        span_ref: Some(span_of(audience.span)),
    }
}

fn lower_platform_view(view: &syntax::LzxPlatformView) -> ir::PlatformView {
    ir::PlatformView {
        name: view.name.clone(),
        view_type: view.view_type.clone(),
        columns: view.columns.clone(),
        fields: view.fields.clone(),
        sections: view.sections.clone(),
        actions: view.actions.clone(),
        submit: view.submit.clone(),
        span_ref: Some(span_of(view.span)),
    }
}

struct LoweredAggregate {
    resource: ir::Resource,
    commands: Vec<ir::Command>,
    queries: Vec<ir::Query>,
}

fn lower_aggregate(aggregate: &syntax::Aggregate) -> Result<LoweredAggregate, AnalyzeError> {
    let mut field_names = BTreeSet::new();
    let mut fields = Vec::new();

    for field in &aggregate.fields {
        if !field_names.insert(field.name.clone()) {
            return Err(AnalyzeError::DuplicateField {
                aggregate: aggregate.name.clone(),
                field: field.name.clone(),
            });
        }

        fields.push(lower_field(field));
    }

    for command in &aggregate.commands {
        validate_known_fields(
            aggregate,
            &field_names,
            format!("command {}", command.name),
            &command.input,
        )?;
    }

    for query in &aggregate.queries {
        validate_known_fields(
            aggregate,
            &field_names,
            format!("query {} search", query.name),
            &query.search,
        )?;
        validate_known_fields(
            aggregate,
            &field_names,
            format!("query {} filters", query.name),
            &query.filters,
        )?;
    }

    for surface in &aggregate.surfaces {
        validate_known_fields(
            aggregate,
            &field_names,
            format!("surface {} list", surface.name),
            &surface.list_columns,
        )?;
        validate_known_fields(
            aggregate,
            &field_names,
            format!("surface {} form", surface.name),
            &surface.form_fields,
        )?;
        validate_known_fields(
            aggregate,
            &field_names,
            format!("surface {} detail", surface.name),
            &surface.detail_fields,
        )?;
    }

    let resource_name = aggregate.name.clone();
    let resource = ir::Resource {
        name: resource_name.clone(),
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(aggregate.span)),
    };

    let commands = aggregate
        .commands
        .iter()
        .map(|c| lower_command(c, &resource_name))
        .collect::<Result<Vec<_>, _>>()?;

    let queries = aggregate.queries.iter().map(lower_query).collect();

    Ok(LoweredAggregate {
        resource,
        commands,
        queries,
    })
}

fn lower_field(field: &syntax::Field) -> ir::Field {
    let mut required = false;
    let mut unique = false;
    let mut default: Option<ir::DefaultValue> = None;

    for modifier in &field.modifiers {
        match modifier {
            syntax::FieldModifier::Required => required = true,
            syntax::FieldModifier::Unique => unique = true,
            syntax::FieldModifier::Default(value) => {
                default = Some(parse_default(value));
            }
        }
    }

    ir::Field {
        name: field.name.clone(),
        type_ref: type_ref_from_syntax(&field.ty),
        required,
        unique,
        default,
        previous_names: Vec::new(),
        span_ref: Some(span_of(field.span)),
    }
}

fn lower_command(
    command: &syntax::Command,
    resource_name: &str,
) -> Result<ir::Command, AnalyzeError> {
    let policy = command
        .policy
        .as_ref()
        .map(|raw| ir::PolicyRef::Unresolved(raw.clone()))
        .ok_or_else(|| AnalyzeError::MissingCommandPolicy {
            aggregate: resource_name.to_owned(),
            command: command.name.clone(),
        })?;

    // Legacy `command Create { input ... emits ... }` is treated as a create
    // effect over the parent aggregate with `from_input` semantics. The
    // canonical parser will replace this heuristic with explicit
    // `creates`/`updates`/`deletes` keywords.
    let effect = ir::CommandEffect::Creates(ir::CreateEffect {
        resource: ir::QualifiedName {
            feature: None,
            name: resource_name.to_owned(),
        },
        from_input: true,
        assignments: Vec::new(),
    });

    Ok(ir::Command {
        name: command.name.clone(),
        kind: ir::CommandKind::Create,
        route: Vec::new(),
        input: ir::CommandInput::Short(command.input.clone()),
        target: None,
        lets: Vec::new(),
        effect,
        policy,
        emits: command.emits.clone(),
        tests: None,
        previous_names: Vec::new(),
        span_ref: Some(span_of(command.span)),
    })
}

fn lower_query(query: &syntax::Query) -> ir::Query {
    // Legacy `query List { search ... filter ... }` lowers into a list query
    // with field filters. Search currently has no canonical home and is
    // dropped on the floor; it will return as a typed query construct in a
    // later phase.
    let filters = query
        .filters
        .iter()
        .map(|name| ir::Filter {
            predicate: ir::Predicate::Comparison {
                left: ir::Expr::Path(ir::Path::from_segments([name.clone()])),
                op: ir::CompareOp::Eq,
                right: ir::Expr::Path(ir::Path::from_segments(["params".to_owned(), name.clone()])),
            },
            when: Some(name.clone()),
        })
        .collect();

    ir::Query::List(ir::ListQuery {
        name: query.name.clone(),
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters,
        order: Vec::new(),
        paginate: None,
        modifier: None,
        previous_names: Vec::new(),
        span_ref: Some(span_of(query.span)),
    })
}

fn validate_known_fields(
    aggregate: &syntax::Aggregate,
    known: &BTreeSet<String>,
    context: String,
    fields: &[String],
) -> Result<(), AnalyzeError> {
    for field in fields {
        if !known.contains(field) {
            return Err(AnalyzeError::UnknownField {
                aggregate: aggregate.name.clone(),
                context,
                field: field.clone(),
            });
        }
    }

    Ok(())
}

fn type_ref_from_syntax(ty: &str) -> ir::TypeRef {
    match ty {
        "ID" | "Id" => ir::TypeRef::Builtin(ir::BuiltinType::Id),
        "Text" | "String" => ir::TypeRef::Builtin(ir::BuiltinType::Text),
        "Boolean" | "Bool" => ir::TypeRef::Builtin(ir::BuiltinType::Boolean),
        "Integer" | "Int" => ir::TypeRef::Builtin(ir::BuiltinType::Integer),
        "Decimal" | "Float" | "Money" => ir::TypeRef::Builtin(ir::BuiltinType::Decimal),
        "Date" => ir::TypeRef::Builtin(ir::BuiltinType::Date),
        "DateTime" => ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
        "JSON" | "Json" => ir::TypeRef::Builtin(ir::BuiltinType::Json),
        "Email" => ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
        other => ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: other.to_owned(),
        }),
    }
}

fn parse_default(raw: &str) -> ir::DefaultValue {
    if raw == "true" {
        return ir::DefaultValue::Boolean(true);
    }
    if raw == "false" {
        return ir::DefaultValue::Boolean(false);
    }
    if raw == "nil" {
        return ir::DefaultValue::Nil;
    }
    if let Ok(value) = raw.parse::<i64>() {
        return ir::DefaultValue::Integer(value);
    }

    if raw
        .chars()
        .next()
        .map(|c| c.is_alphabetic() || c == '_')
        .unwrap_or(false)
        && raw.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return ir::DefaultValue::EnumLiteral(ir::EnumLiteral {
            type_name: None,
            variant: raw.to_owned(),
        });
    }

    ir::DefaultValue::String(raw.to_owned())
}

fn span_of(span: syntax::Span) -> ir::SpanRef {
    ir::SpanRef {
        start: span.start,
        end: span.end,
    }
}

#[cfg(test)]
mod tests {
    use lazuli_syntax::{parse_document, parse_lzx_document};

    use super::{AnalyzeError, lower_document, lower_lzx_document};

    #[test]
    fn lowers_valid_document_to_ir() {
        let document = parse_document(include_str!("../../../examples/crm.lzi")).unwrap();
        let module = lower_document(&document).unwrap();

        assert_eq!(module.features.len(), 1);
        let feature = &module.features[0];
        assert_eq!(feature.name, "crm");
        assert_eq!(feature.resources.len(), 2);

        let customer = &feature.resources[0];
        assert_eq!(customer.name, "Customer");
        assert_eq!(customer.fields[1].name, "email");
        assert!(customer.fields[1].unique);
    }

    #[test]
    fn rejects_unknown_field_references() {
        let source = r#"
            aggregate Customer {
              name: Text

              command Create {
                input email
                policy customer.create
              }
            }
        "#;

        let document = parse_document(source).unwrap();
        let error = lower_document(&document).unwrap_err();

        assert!(matches!(error, AnalyzeError::UnknownField { .. }));
    }

    #[test]
    fn rejects_commands_without_policy() {
        let source = r#"
            aggregate Customer {
              name: Text

              command Create {
                input name
              }
            }
        "#;

        let document = parse_document(source).unwrap();
        let error = lower_document(&document).unwrap_err();

        assert!(matches!(error, AnalyzeError::MissingCommandPolicy { .. }));
    }

    #[test]
    fn lowers_lzx_experience_and_surface_to_ir() {
        let experience =
            parse_lzx_document(include_str!("../../../examples/customer.lzx")).unwrap();
        let surface =
            parse_lzx_document(include_str!("../../../examples/customer.web.lzx")).unwrap();

        let experience_ir = lower_lzx_document(&experience);
        let surface_ir = lower_lzx_document(&surface);

        assert_eq!(experience_ir.experiences[0].name, "customer");
        assert_eq!(experience_ir.experiences[0].imports, vec!["customer"]);
        assert_eq!(
            experience_ir.experiences[0].views[0].actions[0].target,
            "customer.command.create"
        );
        assert_eq!(surface_ir.surfaces[0].experience, "customer");
        assert_eq!(
            surface_ir.surfaces[0].uses_experience.as_deref(),
            Some("customer")
        );
        assert_eq!(surface_ir.surfaces[0].audiences[0].name, "admin");
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].columns,
            vec!["name", "email", "tier", "score", "lifecycle_stage"]
        );
    }
}
