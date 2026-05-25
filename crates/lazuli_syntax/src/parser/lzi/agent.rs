//! `agent <name>` block parser — first-class LLM capability declaration.
//!
//! The block sits at feature child indent (2 spaces); body children at
//! agent child indent (4 spaces); inner blocks (`input`, `tools`,
//! `evals`, `expose http`, `case`) lift their leaves to grandchild
//! indent (6 spaces); eval-case child assertions live at eight-space
//! great-grandchild indent.
//!
//! ## Closed catalog (agent body)
//!
//! - `input` — block of `<name>: <Type> [required|optional]` slots.
//! - `context <path>` — single token (e.g. a `record` reference).
//! - `policy <atoms>` — comma-separated atom list (closed catalog).
//! - `rate_limit "<spec>" [in <env>, ...]` — folded into
//!   `RateLimitSpecAst` via the shared helpers.
//! - `output <Type>` / `output stream <Type>` /
//!   `output discriminator <Enum>`.
//! - `model <name>`, `temperature <f64>`, `max_tokens <u32>`,
//!   `top_p <f64>`, `seed <i64>`, `prompt "..."`.
//! - `safety <atoms>` — comma-separated.
//! - `tools` block — one qualified reference per six-space child line.
//! - `evals` block — `case <name>` headers + their assertions.
//! - `expose http` block — method / path / route slots / audience /
//!   rate_limit override.
//!
//! ## Eval predicate catalog
//!
//! `requires`/`forbids <body>` accepts three closed shapes:
//!
//! - `tools.calls includes|excludes <ToolRef>` — typed.
//! - `<ref> contains "<literal>"` / `<ref> contains @semantic.<Type>` —
//!   typed.
//! - any other body is wrapped as `AgentEvalPredicate::Closed` — the
//!   analyzer is responsible for the typed lift.
//!
//! ## See also
//!
//! - `docs/proposals/ai-primitives-v0-implementation.md` §3.3.
//! - `lazuli_ir::nodes::agent` — typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::numerics::{
    fold_rate_limit_line, parse_float, parse_int64, parse_rate_limit_line_body, parse_uint32,
};
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD,
    AGENT_INDENT_GREAT_GRANDCHILD,
};
use crate::ast::{
    Agent, AgentEvalAssertion, AgentEvalCase, AgentEvalGolden, AgentEvalKind, AgentEvalPredicate,
    AgentExpose, AgentExposeRouteSlot, AgentInputSlot, AgentOutput, AgentTool, ContainsRhs,
    HttpMethod, RateLimitSpecAst, Span, ToolsCallsOp,
};

pub(super) fn parse_agent(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Agent, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("agent ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "agent header must be `agent <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "agent header requires a name"));
    }

    let mut input = Vec::new();
    let mut context: Option<String> = None;
    let mut policy: Option<Vec<String>> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut output: Option<AgentOutput> = None;
    let mut model: Option<String> = None;
    let mut temperature: Option<f64> = None;
    let mut max_tokens: Option<u32> = None;
    let mut top_p: Option<f64> = None;
    let mut seed: Option<i64> = None;
    let mut prompt: Option<String> = None;
    let mut safety = Vec::new();
    let mut tools = Vec::new();
    let mut evals = Vec::new();
    let mut expose: Option<AgentExpose> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "agent body children use four-space indentation",
            ));
        }

        if trimmed == "input" {
            let (slots, next) = parse_agent_input(lines, i)?;
            input = slots;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("context ") {
            context = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(split_policy_atoms(rest));
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("output ") {
            output = Some(parse_agent_output_value(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("model ") {
            model = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("temperature ") {
            temperature = Some(parse_float(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("max_tokens ") {
            max_tokens = Some(parse_uint32(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("top_p ") {
            top_p = Some(parse_float(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("seed ") {
            seed = Some(parse_int64(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("prompt ") {
            prompt = Some(unquote_lzx_value(rest.trim()).to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("safety ") {
            safety = split_policy_atoms(rest);
            i += 1;
        } else if trimmed == "tools" {
            let (parsed, next) = parse_agent_tools(lines, i)?;
            tools = parsed;
            i = next;
        } else if trimmed == "evals" {
            let (parsed, next) = parse_agent_evals(lines, i)?;
            evals = parsed;
            i = next;
        } else if trimmed == "expose http" {
            if expose.is_some() {
                return Err(line_error(
                    line,
                    "agent may declare at most one `expose http` block",
                ));
            }
            let (parsed, next) = parse_agent_expose(lines, i)?;
            expose = Some(parsed);
            i = next;
        } else {
            return Err(line_error(
                line,
                "agent children are `input`, `context`, `policy`, `rate_limit`, `output`, `model`, `temperature`, `max_tokens`, `top_p`, `seed`, `prompt`, `safety`, `tools`, `evals`, or `expose http`",
            ));
        }

        last_end = lines[i.saturating_sub(1).max(start)].end;
    }

    Ok((
        Agent {
            name,
            input,
            context,
            policy,
            rate_limit,
            output,
            model,
            temperature,
            max_tokens,
            top_p,
            seed,
            prompt,
            safety,
            tools,
            evals,
            expose,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_agent_expose(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AgentExpose, usize), ParseError> {
    let header = &lines[start];
    let header_start = header.start;
    let mut method: Option<HttpMethod> = None;
    let mut path: Option<String> = None;
    let mut route_slots: Vec<AgentExposeRouteSlot> = Vec::new();
    let mut audience: Option<String> = None;
    let mut rate_limit_override: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`expose http` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("method ") {
            let token = rest.trim();
            let kind = HttpMethod::from_token(token).ok_or_else(|| {
                line_error(
                    line,
                    "`method` must be one of GET / POST / PUT / PATCH / DELETE",
                )
            })?;
            method = Some(kind);
        } else if let Some(rest) = trimmed.strip_prefix("path ") {
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            route_slots.push(parse_agent_expose_route_slot(line, rest)?);
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            rate_limit_override = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`expose http` children are `method`, `path`, `route`, `audience`, or `rate_limit`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let method = method.ok_or_else(|| {
        line_error(
            header,
            "`expose http` requires `method <GET|POST|PUT|PATCH|DELETE>`",
        )
    })?;
    let path = path.ok_or_else(|| line_error(header, "`expose http` requires `path \"<url>\"`"))?;

    Ok((
        AgentExpose {
            method,
            path,
            route_slots,
            audience,
            rate_limit_override,
            span: Span::new(header_start, last_end),
        },
        i,
    ))
}

fn parse_agent_expose_route_slot(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<AgentExposeRouteSlot, ParseError> {
    let (name_part, type_part) = rest
        .split_once(':')
        .ok_or_else(|| line_error(line, "`route` slot must be `route <name>: <Type>`"))?;
    let name = name_part.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(line, "`route` slot is missing a name"));
    }
    let type_text = type_part.trim().to_owned();
    if type_text.is_empty() {
        return Err(line_error(line, "`route` slot is missing a type"));
    }
    Ok(AgentExposeRouteSlot {
        name,
        type_text,
        span: Span::new(line.start, line.end),
    })
}

fn parse_agent_input(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentInputSlot>, usize), ParseError> {
    let mut slots = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "agent input slots use six-space indentation",
            ));
        }

        slots.push(parse_agent_input_slot(line)?);
        i += 1;
    }

    Ok((slots, i))
}

fn parse_agent_input_slot(line: &SourceLine<'_>) -> Result<AgentInputSlot, ParseError> {
    let trimmed = line.text.trim_start();
    let (name_part, rest) = trimmed.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "input slot must be `<name>: <type> [required|optional]`",
        )
    })?;

    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(line, "input slot is missing a name"));
    }

    let mut required = false;
    let mut optional = false;
    let mut type_tokens: Vec<&str> = Vec::new();
    for token in rest.split_whitespace() {
        match token {
            "required" => required = true,
            "optional" => optional = true,
            other => type_tokens.push(other),
        }
    }

    if type_tokens.is_empty() {
        return Err(line_error(line, "input slot is missing a type"));
    }
    if required && optional {
        return Err(line_error(
            line,
            "input slot cannot be both `required` and `optional`",
        ));
    }

    Ok(AgentInputSlot {
        name: name.to_owned(),
        type_text: type_tokens.join(" "),
        required,
        optional,
        span: Span::new(line.start, line.end),
    })
}

/// Parse the value side of an `output ...` declaration. The leading
/// `output ` prefix has already been consumed by the caller.
fn parse_agent_output_value(line: &SourceLine<'_>, rest: &str) -> Result<AgentOutput, ParseError> {
    let trimmed = rest.trim();
    if let Some(rest) = trimmed.strip_prefix("stream ") {
        let type_ref = rest.trim();
        if type_ref.is_empty() {
            return Err(line_error(line, "`output stream` requires a type"));
        }
        if type_ref.split_whitespace().next() == Some("discriminator") {
            return Err(line_error(
                line,
                "`output stream` cannot also carry `discriminator`; pick one form",
            ));
        }
        Ok(AgentOutput::Stream(type_ref.to_owned()))
    } else if let Some(rest) = trimmed.strip_prefix("discriminator ") {
        let target = rest.trim();
        if target.is_empty() {
            return Err(line_error(line, "`output discriminator` requires a type"));
        }
        if target.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`output discriminator` accepts a single enum reference",
            ));
        }
        Ok(AgentOutput::Discriminator(target.to_owned()))
    } else if trimmed.is_empty() {
        Err(line_error(line, "`output` requires a value"))
    } else {
        // Bare type ref form: `output <Type>` (legacy default; lowering
        // disambiguates record-with-discriminator vs Text).
        if trimmed.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`output <Type>` accepts a single type reference",
            ));
        }
        Ok(AgentOutput::Plain(trimmed.to_owned()))
    }
}

fn parse_agent_tools(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentTool>, usize), ParseError> {
    let mut tools = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "tool entries use six-space indentation, one reference per line",
            ));
        }

        if trimmed.split_whitespace().count() != 1 {
            return Err(line_error(
                line,
                "each tool entry is a single qualified reference",
            ));
        }

        tools.push(AgentTool {
            reference: trimmed.to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    Ok((tools, i))
}

fn parse_agent_evals(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentEvalCase>, usize), ParseError> {
    let mut cases = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "eval `case` headers use six-space indentation",
            ));
        }

        let case_name = trimmed
            .strip_prefix("case ")
            .map(|rest| rest.trim().to_owned())
            .ok_or_else(|| line_error(line, "eval children must be `case <name>` blocks"))?;
        if case_name.is_empty() {
            return Err(line_error(line, "`case` requires a name"));
        }
        let case_start = line.start;
        let mut case_end = line.end;

        let mut assertions = Vec::new();
        let mut golden: Option<AgentEvalGolden> = None;
        i += 1;
        while i < lines.len() {
            let inner = &lines[i];
            let inner_trimmed = inner.text.trim_start();

            if is_trivia(inner_trimmed) {
                i += 1;
                continue;
            }

            if inner.indent <= AGENT_INDENT_GRANDCHILD {
                break;
            }

            if inner.indent != AGENT_INDENT_GREAT_GRANDCHILD {
                return Err(line_error(
                    inner,
                    "eval case children use eight-space indentation",
                ));
            }

            if let Some(rest) = inner_trimmed.strip_prefix("golden ") {
                if golden.is_some() {
                    return Err(line_error(
                        inner,
                        "`case` may declare at most one `golden` reference",
                    ));
                }
                golden = Some(parse_eval_golden(inner, rest)?);
            } else {
                assertions.push(parse_eval_assertion(inner)?);
            }
            case_end = inner.end;
            i += 1;
        }

        if assertions.is_empty() && golden.is_none() {
            return Err(line_error(
                line,
                "`case <name>` must declare at least one `requires`/`forbids` assertion or a `golden \"./path\"` reference",
            ));
        }

        cases.push(AgentEvalCase {
            name: case_name,
            assertions,
            golden,
            span: Span::new(case_start, case_end),
        });
    }

    Ok((cases, i))
}

/// Parse `golden "./path.jsonl"` or `golden "./path.jsonl" min_score 0.85`.
/// The path is required; `min_score` is optional and must parse as a
/// float when present. Adapter convention defaults to 0.85 when
/// omitted; the parser records `None` so authors can override at
/// adapter level without language-side ambiguity.
fn parse_eval_golden(line: &SourceLine<'_>, rest: &str) -> Result<AgentEvalGolden, ParseError> {
    let trimmed = rest.trim();
    if !trimmed.starts_with('"') {
        return Err(line_error(
            line,
            "`golden` requires a quoted file path: `golden \"./path.jsonl\"`",
        ));
    }
    // Find the closing quote without scanning past min_score.
    let body = &trimmed[1..];
    let Some(closing) = body.find('"') else {
        return Err(line_error(line, "`golden` path is missing a closing quote"));
    };
    let path = body[..closing].to_owned();
    let after = body[closing + 1..].trim();
    let min_score = if after.is_empty() {
        None
    } else if let Some(score_text) = after.strip_prefix("min_score ") {
        let value: f64 = score_text
            .trim()
            .parse()
            .map_err(|_| line_error(line, "`min_score` must be a decimal between 0.0 and 1.0"))?;
        if !(0.0..=1.0).contains(&value) {
            return Err(line_error(
                line,
                "`min_score` must be in the range 0.0..=1.0",
            ));
        }
        Some(value)
    } else {
        return Err(line_error(
            line,
            "trailing tokens after `golden \"./path\"` must be `min_score <N>`",
        ));
    };
    Ok(AgentEvalGolden {
        path,
        min_score,
        span: Span::new(line.start, line.end),
    })
}

fn parse_eval_assertion(line: &SourceLine<'_>) -> Result<AgentEvalAssertion, ParseError> {
    let trimmed = line.text.trim_start();
    let (kind, body) = if let Some(rest) = trimmed.strip_prefix("requires ") {
        (AgentEvalKind::Requires, rest.trim())
    } else if let Some(rest) = trimmed.strip_prefix("forbids ") {
        (AgentEvalKind::Forbids, rest.trim())
    } else {
        return Err(line_error(
            line,
            "eval assertions start with `requires` or `forbids`",
        ));
    };

    if body.is_empty() {
        return Err(line_error(line, "eval assertion requires a predicate"));
    }

    let predicate = parse_eval_predicate(line, body)?;
    Ok(AgentEvalAssertion {
        kind,
        predicate,
        span: Span::new(line.start, line.end),
    })
}

fn parse_eval_predicate(
    line: &SourceLine<'_>,
    body: &str,
) -> Result<AgentEvalPredicate, ParseError> {
    let body = body.trim();

    if let Some(rest) = body.strip_prefix("tools.calls ") {
        let mut parts = rest.split_whitespace();
        let op_token = parts.next().ok_or_else(|| {
            line_error(
                line,
                "`tools.calls` requires `includes` or `excludes` followed by a tool reference",
            )
        })?;
        let target = parts
            .next()
            .ok_or_else(|| line_error(line, "`tools.calls` requires a tool reference target"))?;
        if parts.next().is_some() {
            return Err(line_error(
                line,
                "`tools.calls <op> <ref>` accepts a single tool reference",
            ));
        }
        let op = match op_token {
            "includes" => ToolsCallsOp::Includes,
            "excludes" => ToolsCallsOp::Excludes,
            _ => {
                return Err(line_error(
                    line,
                    "`tools.calls` operator must be `includes` or `excludes`",
                ));
            }
        };
        return Ok(AgentEvalPredicate::ToolsCalls {
            op,
            target: target.to_owned(),
        });
    }

    if let Some(idx) = find_contains_keyword(body) {
        let lhs = body[..idx].trim().to_owned();
        let rhs = body[idx + " contains ".len()..].trim();
        if lhs.is_empty() {
            return Err(line_error(
                line,
                "`contains` predicate requires a left-hand reference",
            ));
        }
        let rhs = parse_contains_rhs(line, rhs)?;
        return Ok(AgentEvalPredicate::Contains { lhs, rhs });
    }

    Ok(AgentEvalPredicate::Closed {
        text: body.to_owned(),
    })
}

/// Locate the ` contains ` infix inside an eval predicate body. Returns the
/// byte index of the leading space so callers can split lhs/rhs without
/// re-scanning. Returns `None` when no `contains` keyword appears as a
/// stand-alone token (we never match inside quoted strings).
fn find_contains_keyword(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if !in_quote && body[i..].starts_with(" contains ") {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_contains_rhs(line: &SourceLine<'_>, rhs: &str) -> Result<ContainsRhs, ParseError> {
    let rhs = rhs.trim();
    if rhs.is_empty() {
        return Err(line_error(line, "`contains` requires a right-hand value"));
    }
    if rhs.starts_with('"') {
        let stripped = rhs
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .ok_or_else(|| line_error(line, "`contains` string literal must be quoted"))?;
        return Ok(ContainsRhs::Literal(stripped.to_owned()));
    }
    if rhs.starts_with("@semantic.") {
        if rhs.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`contains @semantic.<Type>` accepts a single semantic-type reference",
            ));
        }
        return Ok(ContainsRhs::SemanticType(rhs.to_owned()));
    }
    Err(line_error(
        line,
        "`contains` rhs must be a quoted string literal or a `@semantic.<Type>` reference",
    ))
}

fn split_policy_atoms(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
