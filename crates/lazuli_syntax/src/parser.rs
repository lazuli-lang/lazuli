use pest::Parser;
use pest::error::InputLocation;
use pest::iterators::Pair;
use pest_derive::Parser;
use thiserror::Error;

use crate::ast::{Aggregate, Command, Document, Field, FieldModifier, Query, Span, Surface};

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct LazuliParser;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("{message}")]
    Pest { message: String, span: Span },

    #[error("internal parser error: expected {expected}")]
    Expected { expected: &'static str },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            Self::Pest { span, .. } => *span,
            Self::Expected { .. } => Span::new(0, 1),
        }
    }
}

pub fn parse_document(source: &str) -> Result<Document, ParseError> {
    let mut pairs =
        LazuliParser::parse(Rule::program, source).map_err(|error| ParseError::Pest {
            message: error.to_string(),
            span: pest_error_span(&error),
        })?;
    let program = pairs.next().ok_or(ParseError::Expected {
        expected: "program",
    })?;

    let span = pair_span(&program);
    let mut app = None;
    let mut aggregates = Vec::new();

    for pair in program.into_inner() {
        match pair.as_rule() {
            Rule::app_decl => app = Some(parse_app(pair)?),
            Rule::aggregate => aggregates.push(parse_aggregate(pair)?),
            Rule::EOI => {}
            _ => {}
        }
    }

    Ok(Document {
        app,
        aggregates,
        span,
    })
}

fn parse_app(pair: Pair<'_, Rule>) -> Result<String, ParseError> {
    pair.into_inner()
        .find(|inner| inner.as_rule() == Rule::ident)
        .map(|inner| inner.as_str().to_owned())
        .ok_or(ParseError::Expected {
            expected: "app name",
        })
}

fn parse_aggregate(pair: Pair<'_, Rule>) -> Result<Aggregate, ParseError> {
    let span = pair_span(&pair);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "aggregate name")?
        .as_str()
        .to_owned();
    let mut fields = Vec::new();
    let mut commands = Vec::new();
    let mut queries = Vec::new();
    let mut surfaces = Vec::new();

    for item in inner {
        match item.as_rule() {
            Rule::field => fields.push(parse_field(item)?),
            Rule::command => commands.push(parse_command(item)?),
            Rule::query => queries.push(parse_query(item)?),
            Rule::surface => surfaces.push(parse_surface(item)?),
            _ => {}
        }
    }

    Ok(Aggregate {
        name,
        fields,
        commands,
        queries,
        surfaces,
        span,
    })
}

fn parse_field(pair: Pair<'_, Rule>) -> Result<Field, ParseError> {
    let span = pair_span(&pair);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "field name")?
        .as_str()
        .to_owned();
    let ty = expect_rule(&mut inner, Rule::ident, "field type")?
        .as_str()
        .to_owned();
    let mut modifiers = Vec::new();

    for modifier in inner {
        if modifier.as_rule() != Rule::field_modifier {
            continue;
        }

        let mut parts = modifier.into_inner();
        let part = parts.next().ok_or(ParseError::Expected {
            expected: "field modifier",
        })?;

        match part.as_rule() {
            Rule::required_modifier => modifiers.push(FieldModifier::Required),
            Rule::unique_modifier => modifiers.push(FieldModifier::Unique),
            Rule::default_modifier => {
                let value = part
                    .into_inner()
                    .next()
                    .ok_or(ParseError::Expected {
                        expected: "default value",
                    })?
                    .as_str()
                    .trim_matches('"')
                    .to_owned();
                modifiers.push(FieldModifier::Default(value));
            }
            _ => {}
        }
    }

    Ok(Field {
        name,
        ty,
        modifiers,
        span,
    })
}

fn parse_command(pair: Pair<'_, Rule>) -> Result<Command, ParseError> {
    let span = pair_span(&pair);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "command name")?
        .as_str()
        .to_owned();
    let mut input = Vec::new();
    let mut policy = None;
    let mut emits = Vec::new();

    for item in inner {
        match item.as_rule() {
            Rule::input_stmt => input.extend(parse_ident_list_statement(item)?),
            Rule::policy_stmt => {
                policy = item
                    .into_inner()
                    .find(|inner| inner.as_rule() == Rule::dotted_ident)
                    .map(|inner| inner.as_str().to_owned());
            }
            Rule::emits_stmt => {
                let event = item
                    .into_inner()
                    .find(|inner| inner.as_rule() == Rule::ident)
                    .ok_or(ParseError::Expected {
                        expected: "event name",
                    })?;
                emits.push(event.as_str().to_owned());
            }
            _ => {}
        }
    }

    Ok(Command {
        name,
        input,
        policy,
        emits,
        span,
    })
}

fn parse_query(pair: Pair<'_, Rule>) -> Result<Query, ParseError> {
    let span = pair_span(&pair);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "query name")?
        .as_str()
        .to_owned();
    let mut search = Vec::new();
    let mut filters = Vec::new();

    for item in inner {
        match item.as_rule() {
            Rule::search_stmt => search.extend(parse_ident_list_statement(item)?),
            Rule::filter_stmt => filters.extend(parse_ident_list_statement(item)?),
            _ => {}
        }
    }

    Ok(Query {
        name,
        search,
        filters,
        span,
    })
}

fn parse_surface(pair: Pair<'_, Rule>) -> Result<Surface, ParseError> {
    let span = pair_span(&pair);
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "surface name")?
        .as_str()
        .to_owned();
    let mut list_columns = Vec::new();
    let mut form_fields = Vec::new();
    let mut detail_fields = Vec::new();

    for item in inner {
        match item.as_rule() {
            Rule::list_stmt => list_columns.extend(parse_ident_list_statement(item)?),
            Rule::form_stmt => form_fields.extend(parse_ident_list_statement(item)?),
            Rule::detail_stmt => detail_fields.extend(parse_ident_list_statement(item)?),
            _ => {}
        }
    }

    Ok(Surface {
        name,
        list_columns,
        form_fields,
        detail_fields,
        span,
    })
}

fn parse_ident_list_statement(pair: Pair<'_, Rule>) -> Result<Vec<String>, ParseError> {
    let list = pair
        .into_inner()
        .find(|inner| inner.as_rule() == Rule::ident_list)
        .ok_or(ParseError::Expected {
            expected: "identifier list",
        })?;

    Ok(list
        .into_inner()
        .filter(|inner| inner.as_rule() == Rule::ident)
        .map(|inner| inner.as_str().to_owned())
        .collect())
}

fn expect_rule<'a>(
    pairs: &mut impl Iterator<Item = Pair<'a, Rule>>,
    rule: Rule,
    expected: &'static str,
) -> Result<Pair<'a, Rule>, ParseError> {
    pairs
        .find(|pair| pair.as_rule() == rule)
        .ok_or(ParseError::Expected { expected })
}

fn pair_span(pair: &Pair<'_, Rule>) -> Span {
    let span = pair.as_span();
    Span::new(span.start(), span.end())
}

fn pest_error_span(error: &pest::error::Error<Rule>) -> Span {
    match error.location {
        InputLocation::Pos(pos) => Span::new(pos, pos.saturating_add(1)),
        InputLocation::Span((start, end)) => Span::new(start, end),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_document;
    use crate::FieldModifier;

    #[test]
    fn parses_aggregate_fields_commands_queries_and_surfaces() {
        let source = include_str!("../../../examples/crm.lzi");
        let document = parse_document(source).expect("valid document");

        assert_eq!(document.app.as_deref(), Some("CRM"));
        assert_eq!(document.aggregates.len(), 2);

        let customer = &document.aggregates[0];
        assert_eq!(customer.name, "Customer");
        assert_eq!(customer.fields[0].name, "name");
        assert_eq!(customer.fields[0].ty, "Text");
        assert!(
            customer.fields[0]
                .modifiers
                .contains(&FieldModifier::Required)
        );
        assert_eq!(customer.commands[0].input, vec!["name", "email"]);
        assert_eq!(customer.queries[0].filters, vec!["status"]);
        assert_eq!(
            customer.surfaces[0].list_columns,
            vec!["name", "email", "status"]
        );
    }
}
