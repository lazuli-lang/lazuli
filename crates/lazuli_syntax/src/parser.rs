use pest::Parser;
use pest::error::InputLocation;
use pest::iterators::Pair;
use pest_derive::Parser;
use thiserror::Error;

use crate::ast::{
    Aggregate, Command, Document, Field, FieldModifier, LzxAction, LzxAudience, LzxDocument,
    LzxExperience, LzxExperienceView, LzxPlatform, LzxPlatformView, LzxSurface, Query, Span,
    Surface,
};

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

pub fn parse_lzx_document(source: &str) -> Result<LzxDocument, ParseError> {
    let lines = source_lines(source);
    let mut experiences = Vec::new();
    let mut surfaces = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `.lzx` declarations are not indented",
            ));
        }

        if trimmed.starts_with("experience ") {
            let (experience, next) = parse_lzx_experience(&lines, index)?;
            experiences.push(experience);
            index = next;
        } else if trimmed.starts_with("surface ") {
            let (surface, next) = parse_lzx_surface(&lines, index)?;
            surfaces.push(surface);
            index = next;
        } else {
            return Err(line_error(
                line,
                "expected `experience <name>` or `surface <experience> <platform>`",
            ));
        }
    }

    Ok(LzxDocument {
        experiences,
        surfaces,
        span: Span::new(0, source.len()),
    })
}

fn parse_lzx_experience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "`experience` uses `experience <name>`"));
    }

    let mut imports = Vec::new();
    let mut views = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "experience children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("imports ") {
            imports.extend(split_lzx_list(rest));
            index += 1;
        } else if trimmed.starts_with("view ") {
            let (view, next) = parse_lzx_experience_view(lines, index)?;
            views.push(view);
            index = next;
        } else {
            return Err(line_error(
                line,
                "experience children are `imports` or `view` declarations",
            ));
        }
    }

    Ok((
        LzxExperience {
            name: parts[1].to_owned(),
            imports,
            views,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_experience_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperienceView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 && !(parts.len() == 4 && parts[2] == "id") {
        return Err(line_error(
            header,
            "experience views use `view <name>` or `view <name> id @anchor.<name>`",
        ));
    }

    let mut source = None;
    let mut submit = None;
    let mut actions = Vec::new();
    let mut opens = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(line, "view children use four-space indentation"));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("action ") {
            let Some((name, target)) = rest.split_once(" -> ") else {
                return Err(line_error(line, "actions use `action <name> -> <target>`"));
            };
            actions.push(LzxAction {
                name: name.trim().to_owned(),
                target: target.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("opens ") {
            opens.push(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "view children are `source`, `submit`, `action`, or `opens`",
            ));
        }

        index += 1;
    }

    Ok((
        LzxExperienceView {
            name: parts[1].to_owned(),
            anchor: (parts.len() == 4).then(|| parts[3].to_owned()),
            source,
            submit,
            actions,
            opens,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_surface(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxSurface, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "surfaces use `surface <experience> web|mobile`",
        ));
    }

    let platform = match parts[2] {
        "web" => LzxPlatform::Web,
        "mobile" => LzxPlatform::Mobile,
        _ => {
            return Err(line_error(
                header,
                "surface platform must be `web` or `mobile`",
            ));
        }
    };

    let mut uses_experience = None;
    let mut audiences = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "surface children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("uses experience ") {
            uses_experience = Some(rest.trim().to_owned());
            index += 1;
        } else if trimmed.starts_with("audience ") {
            let (audience, next) = parse_lzx_audience(lines, index)?;
            audiences.push(audience);
            index = next;
        } else if trimmed.starts_with("view ") {
            return Err(line_error(
                line,
                "concrete platform views live under `audience ...` blocks",
            ));
        } else {
            return Err(line_error(
                line,
                "surface children are `uses experience` or `audience` declarations",
            ));
        }
    }

    Ok((
        LzxSurface {
            experience: parts[1].to_owned(),
            platform,
            uses_experience,
            audiences,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_audience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxAudience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() < 2 {
        return Err(line_error(header, "audience blocks use `audience <name>`"));
    }

    let mut views = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 || !trimmed.starts_with("view ") {
            return Err(line_error(
                line,
                "audience children are complete `view <name> <type>` declarations",
            ));
        }

        let (view, next) = parse_lzx_platform_view(lines, index)?;
        views.push(view);
        index = next;
    }

    Ok((
        LzxAudience {
            name: parts[1].to_owned(),
            qualifiers: parts[2..].iter().map(|part| (*part).to_owned()).collect(),
            views,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_platform_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxPlatformView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "platform views use `view <name> <type>`",
        ));
    }

    let mut columns = Vec::new();
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut search = Vec::new();
    let mut filter = Vec::new();
    let mut cells = Vec::new();
    let mut actions = Vec::new();
    let mut submit = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 4 {
            break;
        }

        if line.indent != 6 {
            return Err(line_error(
                line,
                "platform view children use six-space indentation",
            ));
        }

        if trimmed.contains("+=") || trimmed.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole view",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("columns ") {
            columns = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("fields ") {
            fields = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("sections ") {
            sections = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("search ") {
            search = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("filter ") {
            filter = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("cells ") {
            cells = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("actions ") {
            actions = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "platform view children are `columns`, `fields`, `sections`, `search`, `filter`, `cells`, `actions`, or `submit`",
            ));
        }

        index += 1;
    }

    Ok((
        LzxPlatformView {
            name: parts[1].to_owned(),
            view_type: parts[2].to_owned(),
            columns,
            fields,
            sections,
            search,
            filter,
            cells,
            actions,
            submit,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
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

#[derive(Debug)]
struct SourceLine<'a> {
    text: &'a str,
    indent: usize,
    start: usize,
    end: usize,
}

fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let mut offset = 0;
    let mut lines = Vec::new();

    for text in source.lines() {
        let start = offset;
        let end = start + text.len();
        lines.push(SourceLine {
            text,
            indent: text.bytes().take_while(|byte| *byte == b' ').count(),
            start,
            end,
        });
        offset = end + 1;
    }

    lines
}

fn is_trivia(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#')
}

fn split_lzx_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn line_error(line: &SourceLine<'_>, message: &'static str) -> ParseError {
    ParseError::Pest {
        message: message.to_owned(),
        span: Span::new(line.start, line.end.max(line.start + 1)),
    }
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
    use super::{parse_document, parse_lzx_document};
    use crate::{FieldModifier, LzxPlatform};

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

    #[test]
    fn parses_lzx_experience_and_platform_surface() {
        let experience =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.lzx")).unwrap();
        assert_eq!(experience.experiences.len(), 1);
        assert_eq!(experience.experiences[0].name, "customer");
        assert_eq!(experience.experiences[0].imports, vec!["customer"]);
        assert_eq!(experience.experiences[0].views[0].name, "list");
        assert_eq!(
            experience.experiences[0].views[0].source.as_deref(),
            Some("customer.query.list")
        );
        assert_eq!(experience.experiences[0].views[1].anchor.as_deref(), None);

        let surface =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.web.lzx")).unwrap();
        assert_eq!(surface.surfaces.len(), 1);
        assert_eq!(surface.surfaces[0].experience, "customer");
        assert_eq!(surface.surfaces[0].platform, LzxPlatform::Web);
        assert_eq!(
            surface.surfaces[0].uses_experience.as_deref(),
            Some("customer")
        );
        assert_eq!(surface.surfaces[0].audiences[0].name, "admin");
        assert_eq!(
            surface.surfaces[0].audiences[0].views[0].columns,
            vec!["name", "email", "status", "created_at"]
        );
        assert_eq!(
            surface.surfaces[0].audiences[0].views[0].filter,
            vec!["status"]
        );
        assert_eq!(
            surface.surfaces[0].audiences[0].views[0].cells,
            vec!["status @client.status_cell"]
        );
    }

    #[test]
    fn rejects_lzx_partial_overrides() {
        let source = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      columns += score
"#;

        let error = parse_lzx_document(source).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("partial overrides are not valid in `.lzx`")
        );
    }
}
