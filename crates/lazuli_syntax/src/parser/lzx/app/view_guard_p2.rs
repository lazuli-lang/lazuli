/// `ir-route-guard-escape-hatch-2026-05-28` §4.1 — parse `requires
/// <feature>.lookup_my.<field> = <literal> on_unmet redirect "<path>"`.
///
/// `head_text` is everything after the leading `requires ` keyword on
/// the head line. `tail_text` is the joined continuation (typically the
/// `on_unmet redirect "..."` child line; empty when the whole clause
/// appears on the head line). `span_end` is the end offset of the last
/// consumed source line.
pub(crate) fn parse_lzx_requires_field(
    line: &SourceLine<'_>,
    head_text: &str,
    tail_text: &str,
    span_end: usize,
) -> Result<LzxRequiresField, ParseError> {
    // Join head + tail. Trim so `<head>` alone or `<head>\n  <tail>`
    // are handled uniformly.
    let joined = if tail_text.is_empty() {
        head_text.to_owned()
    } else {
        format!("{} {}", head_text, tail_text)
    };

    // Split on `on_unmet` so the redirect tail is isolated.
    let (predicate, redirect) = joined.split_once(" on_unmet ").ok_or_else(|| {
        line_error(
            line,
            "`requires` must include `on_unmet redirect \"<path>\"` (multi-line form: place `on_unmet redirect \"<path>\"` on the next line, indented two spaces deeper than the `requires` line)",
        )
    })?;

    // SPEC-05 — predicate equality is `==`: `<feature>.lookup_my.<field> == <literal>`.
    let (path_part, literal_part) = predicate.split_once("==").ok_or_else(|| {
        line_error(
            line,
            "`requires` predicate must be `<feature>.lookup_my.<field> == <literal>`",
        )
    })?;
    let path_part = path_part.trim();
    let literal_part = literal_part.trim();
    let segments: Vec<&str> = path_part.split('.').collect();
    if segments.len() != 3 {
        return Err(line_error(
            line,
            "`requires` predicate path must be `<feature>.lookup_my.<field>` (3 dot-separated segments)",
        ));
    }
    let feature = segments[0].trim();
    let lookup_literal = segments[1].trim();
    let field = segments[2].trim();
    if !is_lzx_bare_ident(feature) || feature.chars().next().is_some_and(|c| c.is_ascii_uppercase())
    {
        return Err(line_error(
            line,
            "`requires` feature segment must be a snake_case identifier (use the feature index name, NOT the resource name)",
        ));
    }
    if lookup_literal != "lookup_my" {
        return Err(line_error(
            line,
            "`requires` middle segment MUST be the literal `lookup_my` (per ir-route-guard-escape-hatch §4.1.1 grammar — the parser fixes this; the analyzer resolves `lookup_my_<resource>` against the feature)",
        ));
    }
    if !is_lzx_bare_ident(field) || field.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return Err(line_error(
            line,
            "`requires` field segment must be a snake_case identifier",
        ));
    }

    let expected = parse_lzx_scalar_literal(line, literal_part)?;

    let redirect = redirect.trim();
    let rest = redirect.strip_prefix("redirect ").ok_or_else(|| {
        line_error(
            line,
            "`requires ... on_unmet` must be followed by `redirect \"<path>\"`",
        )
    })?;
    let target = rest.trim();
    if !target.starts_with('"') || !target.ends_with('"') || target.len() < 2 {
        return Err(line_error(
            line,
            "`requires ... on_unmet redirect` target must be a double-quoted string",
        ));
    }
    let on_unmet_redirect = target[1..target.len() - 1].to_owned();

    Ok(LzxRequiresField {
        feature: feature.to_owned(),
        field: field.to_owned(),
        expected,
        on_unmet_redirect,
        span: Span::new(line.start, span_end),
    })
}

/// Parse one scalar literal (`"…"`, integer, `true`, `false`, `null`)
/// for the right-hand side of `requires <path> = <literal>`.
fn parse_lzx_scalar_literal(
    line: &SourceLine<'_>,
    raw: &str,
) -> Result<LzxScalarLiteral, ParseError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(line_error(
            line,
            "`requires` literal cannot be empty — use `\"<string>\"`, an integer, `true`, `false`, or `null`",
        ));
    }
    match text {
        "true" => Ok(LzxScalarLiteral::Boolean(true)),
        "false" => Ok(LzxScalarLiteral::Boolean(false)),
        "null" => Ok(LzxScalarLiteral::Null),
        _ => {
            if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
                Ok(LzxScalarLiteral::String(text[1..text.len() - 1].to_owned()))
            } else if let Ok(n) = text.parse::<i64>() {
                Ok(LzxScalarLiteral::Integer(n))
            } else {
                Err(line_error(
                    line,
                    "`requires` literal must be a string, integer, `true`, `false`, or `null`",
                ))
            }
        }
    }
}

fn parse_lzx_view_guard_policy(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<Vec<String>, ParseError> {
    if value.is_empty() {
        return Err(line_error(line, "`policy` requires a policy reference"));
    }

    if !value.starts_with('[') {
        return Ok(vec![value.to_owned()]);
    }

    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Err(line_error(
            line,
            "`policy` list form is `policy [@policy.a, @policy.b]`",
        ));
    };
    if inner.trim().is_empty() {
        return Err(line_error(
            line,
            "`policy` list requires at least one policy reference",
        ));
    }

    let mut policies = Vec::new();
    for atom in inner.split(',') {
        let atom = atom.trim();
        if atom.is_empty() {
            return Err(line_error(
                line,
                "`policy` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        policies.push(atom.to_owned());
    }
    Ok(policies)
}

fn parse_lzx_redirect_clause(line: &SourceLine<'_>, value: &str) -> Result<String, ParseError> {
    let Some(rest) = value.strip_prefix("redirect ") else {
        return Err(line_error(
            line,
            "route guard redirect clauses use `redirect \"<path>\"`",
        ));
    };
    let target = rest.trim();
    if !target.starts_with('"') || !target.ends_with('"') {
        return Err(line_error(
            line,
            "route guard redirect targets must be quoted strings",
        ));
    }
    Ok(unquote_lzx_value(target).to_owned())
}

pub(crate) fn parse_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LzxRequiresLifecycle, ParseError> {
    let Some((resource, state)) = rest.split_once('=') else {
        return Err(line_error(
            line,
            "`requires_lifecycle` uses `requires_lifecycle <Resource> = <state>`",
        ));
    };
    let resource = resource.trim();
    let state = state.trim();
    let (state, substep) = parse_lzx_optional_substep_tail(line, state, "`requires_lifecycle`")?;
    if resource.is_empty()
        || !resource
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        || !is_lzx_bare_ident(resource)
    {
        return Err(line_error(
            line,
            "`requires_lifecycle` resource must be an upper-case resource identifier",
        ));
    }
    if !is_lzx_bare_ident(state) {
        return Err(line_error(
            line,
            "`requires_lifecycle` state must be a bare lifecycle state identifier",
        ));
    }
    Ok(LzxRequiresLifecycle {
        resource: resource.to_owned(),
        state: state.to_owned(),
        substep,
        span: Span::new(line.start, line.end),
    })
}

pub(crate) fn parse_lzx_optional_substep_tail<'a>(
    line: &SourceLine<'_>,
    value: &'a str,
    context: &str,
) -> Result<(&'a str, Option<String>), ParseError> {
    let parts: Vec<_> = value.split_whitespace().collect();
    match parts.as_slice() {
        [state] => Ok((*state, None)),
        [state, "substep", substep] => {
            if !is_lzx_bare_ident(substep) {
                return Err(line_error_owned(
                    line,
                    format!("{context} substep must be a bare identifier"),
                ));
            }
            Ok((*state, Some((*substep).to_owned())))
        }
        _ => Err(line_error_owned(
            line,
            format!("{context} accepts an optional `substep <name>` tail"),
        )),
    }
}

pub(crate) fn parse_lzx_on_lifecycle_pending(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<String, ParseError> {
    let target = if let Some(target) = rest.trim().strip_prefix("@resume ") {
        target.trim()
    } else if let Some(target) = rest.trim().strip_prefix("@resume.") {
        target.trim()
    } else {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` uses `on_lifecycle_pending @resume <name>`",
        ));
    };
    if !is_lzx_resume_ref(target) {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` resume reference must be `<name>` or `<feature>.<name>`",
        ));
    }
    Ok(target.to_owned())
}

pub(crate) fn attach_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: LzxRequiresLifecycle,
) -> Result<(), ParseError> {
    if guard.requires_lifecycle.is_some() {
        return Err(line_error(
            line,
            "view declares `requires_lifecycle` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(parsed.span.end);
    guard.requires_lifecycle = Some(parsed);
    Ok(())
}

pub(crate) fn attach_lzx_on_lifecycle_pending(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: String,
    span_end: usize,
) -> Result<(), ParseError> {
    if guard.on_lifecycle_pending.is_some() {
        return Err(line_error(
            line,
            "view declares `on_lifecycle_pending` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(span_end);
    guard.on_lifecycle_pending = Some(parsed);
    Ok(())
}
