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
}

pub fn lower_document(document: &syntax::Document) -> Result<ir::Application, AnalyzeError> {
    let mut aggregate_names = BTreeSet::new();
    let mut resources = Vec::new();

    for aggregate in &document.aggregates {
        if !aggregate_names.insert(aggregate.name.clone()) {
            return Err(AnalyzeError::DuplicateAggregate {
                name: aggregate.name.clone(),
            });
        }

        resources.push(lower_aggregate(aggregate)?);
    }

    Ok(ir::Application {
        name: document
            .app
            .clone()
            .unwrap_or_else(|| "LazuliApp".to_owned()),
        resources,
    })
}

fn lower_aggregate(aggregate: &syntax::Aggregate) -> Result<ir::Resource, AnalyzeError> {
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

    Ok(ir::Resource {
        name: aggregate.name.clone(),
        fields,
        commands: aggregate.commands.iter().map(lower_command).collect(),
        queries: aggregate.queries.iter().map(lower_query).collect(),
        surfaces: aggregate.surfaces.iter().map(lower_surface).collect(),
    })
}

fn lower_field(field: &syntax::Field) -> ir::Field {
    let mut required = false;
    let mut unique = false;
    let mut default = None;

    for modifier in &field.modifiers {
        match modifier {
            syntax::FieldModifier::Required => required = true,
            syntax::FieldModifier::Unique => unique = true,
            syntax::FieldModifier::Default(value) => default = Some(value.clone()),
        }
    }

    ir::Field {
        name: field.name.clone(),
        kind: field.ty.clone(),
        required,
        unique,
        default,
    }
}

fn lower_command(command: &syntax::Command) -> ir::Command {
    ir::Command {
        name: command.name.clone(),
        input: command.input.clone(),
        policy: command.policy.clone(),
        emits: command.emits.clone(),
    }
}

fn lower_query(query: &syntax::Query) -> ir::Query {
    ir::Query {
        name: query.name.clone(),
        search: query.search.clone(),
        filters: query.filters.clone(),
    }
}

fn lower_surface(surface: &syntax::Surface) -> ir::Surface {
    ir::Surface {
        name: surface.name.clone(),
        list_columns: surface.list_columns.clone(),
        form_fields: surface.form_fields.clone(),
        detail_fields: surface.detail_fields.clone(),
    }
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

#[cfg(test)]
mod tests {
    use lazuli_syntax::parse_document;

    use super::{AnalyzeError, lower_document};

    #[test]
    fn lowers_valid_document_to_ir() {
        let document = parse_document(include_str!("../../../examples/crm.lzi")).unwrap();
        let app = lower_document(&document).unwrap();

        assert_eq!(app.name, "CRM");
        assert_eq!(app.resources.len(), 2);
        assert_eq!(app.resources[0].fields[1].name, "email");
        assert!(app.resources[0].fields[1].unique);
    }

    #[test]
    fn rejects_unknown_field_references() {
        let source = r#"
            aggregate Customer {
              name: Text

              command Create {
                input email
              }
            }
        "#;

        let document = parse_document(source).unwrap();
        let error = lower_document(&document).unwrap_err();

        assert!(matches!(error, AnalyzeError::UnknownField { .. }));
    }
}
