//! Plan-catalog parser + side-channel gate scanner (PG.A vocabulary).
//!
//! Two additive passes over the raw `.lzi` source — they never touch the
//! existing per-callable struct literals (`CommandDecl`, `ApiDecl`, `Job`,
//! `Webhook`, `ListQueryDecl`, ...). The analyzer (PG.B) consumes both
//! outputs and threads them through IR + doctor as a parallel side-table.
//!
//! ## What this module owns
//!
//! - `parse_plan_blocks` — top-level `plan <name>` blocks, siblings of
//!   `feature` / `app` at indent 0. Children sit at indent 2 with the
//!   closed catalog `features`, `limits`, `trial`.
//! - `parse_feature_gates` — scans a feature `.lzi` for
//!   `gate behind plan.feature: <id>` and `gate quota plan.limit: <id>`
//!   directives inside callable bodies (`command`, `job`, `webhook`,
//!   `api`, the `query.*` family). Returns a `FeatureGatesAst` keyed by
//!   qualified callable id.
//!
//! Validation of cross-plan references and feature-catalog union is the
//! analyzer's job; this parser only enforces the closed grammar
//! (identifier shape, value types, single trial block per plan).
//!
//! ## Why `is_plan_ident` is `pub(super)`
//!
//! Both `parse_plan_blocks` and `parse_feature_gates` share the
//! `[a-z][a-z0-9_]*` ident rule; the helper is visible to sibling parser
//! modules (currently only this one consumes it).
//!
//! ## See also
//!
//! - `docs/proposals/plan-gate-vocab.md` — the PG vocabulary spec.
//! - `lazuli_ir::nodes::plan` — the typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, source_lines};
use super::super::error::ParseError;

use crate::ast::{
    FeatureGatesAst, GateDirectiveAst, PlanBlockAst, PlanFeatureRefAst, PlanLimitRefAst,
    PlanTrialAst, Span,
};

// =============================================================================
// PG.A — top-level plan-catalog parser + side-channel gate scanner.
// -----------------------------------------------------------------------------
// These two passes are deliberately additive: they walk the raw source
// independently and never touch the existing per-callable struct
// literals (CommandDecl, ApiDecl, Job, Webhook, ListQueryDecl, ...).
// The analyzer (PG.B) consumes both outputs and threads them through
// IR + doctor as a parallel side-table.
// =============================================================================

/// Parse top-level `plan <name>` blocks. Returns one entry per authored
/// block in source order. Plan blocks are siblings of `feature`/`app`
/// at indent 0; their children sit at indent 2.
///
/// Validation of cross-plan references and feature catalog union is
/// the analyzer's job; this parser only enforces the closed grammar
/// (identifier shape, value types, single trial block per plan).
///
/// ## Examples
///
/// ```
/// use lazuli_syntax::parse_plan_blocks;
///
/// let src = "plan starter\n  features customer\n  limits seats 5\n";
/// let plans = parse_plan_blocks(src).expect("parses");
/// assert_eq!(plans.len(), 1);
/// assert_eq!(plans[0].name, "starter");
/// ```
pub fn parse_plan_blocks(source: &str) -> Result<Vec<PlanBlockAst>, ParseError> {
    let lines = source_lines(source);
    let mut plans = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent == 0 {
            if let Some(rest) = trimmed.strip_prefix("plan ") {
                let name = rest.trim().to_owned();
                if name.is_empty() {
                    return Err(line_error(
                        line,
                        "plan header requires a name: `plan <name>`",
                    ));
                }
                if !is_plan_ident(&name) {
                    return Err(line_error(line, "plan name must match `[a-z][a-z0-9_]*`"));
                }
                let (block, next) = parse_plan_block(&lines, i, name)?;
                plans.push(block);
                i = next;
                continue;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    Ok(plans)
}

fn parse_plan_block(
    lines: &[SourceLine<'_>],
    start: usize,
    name: String,
) -> Result<(PlanBlockAst, usize), ParseError> {
    let header = &lines[start];
    let mut features: Vec<PlanFeatureRefAst> = Vec::new();
    let mut limits: Vec<PlanLimitRefAst> = Vec::new();
    let mut trial: Option<PlanTrialAst> = None;
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent == 0 {
            break;
        }
        if line.indent != 2 {
            return Err(line_error(
                line,
                "`plan` children use two-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("features ") {
            features.extend(parse_plan_feature_refs(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("limits ") {
            limits.extend(parse_plan_limit_refs(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("trial ") {
            if trial.is_some() {
                return Err(line_error(
                    line,
                    "`plan` may declare at most one `trial` block",
                ));
            }
            trial = Some(parse_plan_trial(line, rest)?);
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`plan` children are `features`, `limits`, or `trial duration ..., then ...`",
            ));
        }
    }
    if features.is_empty() && limits.is_empty() && trial.is_none() {
        return Err(line_error(
            header,
            "`plan` requires at least one of `features`, `limits`, or `trial`",
        ));
    }
    Ok((
        PlanBlockAst {
            name,
            features,
            limits,
            trial,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_plan_feature_refs(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<Vec<PlanFeatureRefAst>, ParseError> {
    let mut out = Vec::new();
    for piece in rest.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let Some(target) = piece.strip_suffix(".features") {
            if !is_plan_ident(target) {
                return Err(line_error(
                    line,
                    "cross-plan `<other>.features` reference requires a lowercase plan id",
                ));
            }
            out.push(PlanFeatureRefAst::CrossPlan(target.to_owned()));
        } else if is_plan_ident(piece) {
            out.push(PlanFeatureRefAst::Ident(piece.to_owned()));
        } else {
            return Err(line_error(
                line,
                "`features` entries must be `[a-z][a-z0-9_]*` or `<other>.features`",
            ));
        }
    }
    Ok(out)
}

fn parse_plan_limit_refs(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<Vec<PlanLimitRefAst>, ParseError> {
    let mut out = Vec::new();
    for piece in rest.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let Some(target) = piece.strip_suffix(".limits") {
            if !is_plan_ident(target) {
                return Err(line_error(
                    line,
                    "cross-plan `<other>.limits` reference requires a lowercase plan id",
                ));
            }
            out.push(PlanLimitRefAst::CrossPlan(target.to_owned()));
            continue;
        }
        let mut tokens = piece.split_whitespace();
        let name = tokens
            .next()
            .ok_or_else(|| line_error(line, "`limits` entry requires `<name> <value>`"))?;
        let value = tokens
            .next()
            .ok_or_else(|| line_error(line, "`limits` entry requires a value after the name"))?;
        if tokens.next().is_some() {
            return Err(line_error(
                line,
                "`limits` entry must be `<name> <integer>` or `<name> unlimited`",
            ));
        }
        if !is_plan_ident(name) {
            return Err(line_error(
                line,
                "`limits` entry name must match `[a-z][a-z0-9_]*`",
            ));
        }
        if value == "unlimited" {
            out.push(PlanLimitRefAst::Unlimited {
                name: name.to_owned(),
            });
        } else if let Ok(v) = value.parse::<u64>() {
            out.push(PlanLimitRefAst::Integer {
                name: name.to_owned(),
                value: v,
            });
        } else {
            return Err(line_error(
                line,
                "`limits` value must be a positive integer or `unlimited`",
            ));
        }
    }
    Ok(out)
}

fn parse_plan_trial(line: &SourceLine<'_>, rest: &str) -> Result<PlanTrialAst, ParseError> {
    let body = rest.trim();
    let (left, right) = body.split_once(',').ok_or_else(|| {
        line_error(
            line,
            "`trial` requires `duration <d>, then <plan>` (comma separator)",
        )
    })?;
    let duration = left
        .trim()
        .strip_prefix("duration ")
        .ok_or_else(|| {
            line_error(
                line,
                "`trial` left side must be `duration <integer><s|m|h|d>`",
            )
        })?
        .trim()
        .to_owned();
    if !is_valid_trial_duration(&duration) {
        return Err(line_error(
            line,
            "`trial duration` must be `<integer><s|m|h|d>` (e.g. `14d`, `48h`)",
        ));
    }
    let then_plan = right
        .trim()
        .strip_prefix("then ")
        .ok_or_else(|| line_error(line, "`trial` right side must be `then <plan>`"))?
        .trim()
        .to_owned();
    if !is_plan_ident(&then_plan) {
        return Err(line_error(
            line,
            "`trial then <plan>` requires a lowercase plan id",
        ));
    }
    Ok(PlanTrialAst {
        duration,
        then_plan,
        span: Span::new(line.start, line.end),
    })
}

fn is_valid_trial_duration(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    let last = s.chars().last().unwrap();
    if !matches!(last, 's' | 'm' | 'h' | 'd') {
        return false;
    }
    let head = &s[..s.len() - 1];
    !head.is_empty() && head.chars().all(|c| c.is_ascii_digit())
}

/// PG.A — plan/feature/limit ident regex: `[a-z][a-z0-9_]*`.
fn is_plan_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// PG.A — scan a feature `.lzi` source for `gate behind plan.feature: ...`
/// and `gate quota plan.limit: ...` directives inside callable bodies.
/// Returns a map keyed by qualified callable id
/// (`command:<name>` / `job:<name>` / `webhook:<name>` /
/// `api:<name>` / `query.list:<name>` / `query.lookup:<name>` /
/// `query.sql:<name>` / `query.view:<name>`) into the gate directives declared in source
/// order for that callable.
///
/// The scanner is a single-pass text walker. It tracks the current
/// callable header so a `gate ...` line indented under that header is
/// attributed correctly. Gates outside any callable body are reported
/// as a parse error.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_syntax::parse_feature_gates;
///
/// // Real fixture lives next to the analyzer's plan tests — gates only
/// // make sense inside a full callable body, so a minimal `feature
/// // <name>\n  command <name>\n    gate ...` string omits required
/// // siblings and is rejected.
/// let src = include_str!("../fixtures/feature_with_gates.lzi");
/// let gates = parse_feature_gates(src).expect("parses");
/// assert!(!gates.callables.is_empty());
/// ```
pub fn parse_feature_gates(source: &str) -> Result<FeatureGatesAst, ParseError> {
    let lines = source_lines(source);
    let mut callables: std::collections::BTreeMap<String, Vec<GateDirectiveAst>> =
        std::collections::BTreeMap::new();
    let mut current: Option<(String, usize)> = None; // (qualified-key, header-indent)

    for line in &lines {
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            continue;
        }

        // Has the current callable scope ended? (Sibling header at the
        // same indent or shallower.)
        if let Some((_, header_indent)) = &current {
            if line.indent <= *header_indent {
                current = None;
            }
        }

        // Detect a new callable header.
        if let Some(key) = callable_header_key(trimmed) {
            current = Some((key, line.indent));
            continue;
        }

        // Inside a callable scope, recognise gate lines.
        if let Some(rest) = trimmed.strip_prefix("gate ") {
            let directive = parse_gate_directive_line(line, rest)?;
            match &current {
                Some((key, _)) => {
                    callables
                        .entry(key.clone())
                        .or_insert_with(Vec::new)
                        .push(directive);
                }
                None => {
                    return Err(line_error(
                        line,
                        "`gate` directive must appear inside a callable body",
                    ));
                }
            }
        }
    }

    Ok(FeatureGatesAst { callables })
}

/// Recognise `command <name>` / `job <name>` / `webhook <name>` /
/// `api <name>` / `query.list <name>` / `query.lookup <name>` /
/// `query.sql <name>` / `query.view <name>` headers and produce the qualified key the
/// downstream side-table uses.
fn callable_header_key(trimmed: &str) -> Option<String> {
    let prefixes: &[(&str, &str)] = &[
        ("command ", "command"),
        ("job ", "job"),
        ("webhook ", "webhook"),
        ("api ", "api"),
        ("query.list ", "query.list"),
        ("query.lookup ", "query.lookup"),
        ("query.sql ", "query.sql"),
        ("query.view ", "query.view"),
    ];
    for (prefix, kind) in prefixes {
        if let Some(rest) = trimmed.strip_prefix(*prefix) {
            // Take only the first whitespace-separated token (header may
            // carry trailing `by <field>: <Type>` etc.).
            let name = rest.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                return Some(format!("{}:{}", kind, name));
            }
        }
    }
    None
}

fn parse_gate_directive_line(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<GateDirectiveAst, ParseError> {
    let body = rest.trim();
    if let Some(after) = body.strip_prefix("behind plan.feature:") {
        let feature = after.trim();
        if feature.is_empty() {
            return Err(line_error(
                line,
                "`gate behind plan.feature:` requires a feature identifier",
            ));
        }
        if !is_plan_ident(feature) {
            return Err(line_error(
                line,
                "`gate behind plan.feature:` requires a lowercase identifier ([a-z][a-z0-9_]*)",
            ));
        }
        Ok(GateDirectiveAst::Behind {
            feature: feature.to_owned(),
            span: Span::new(line.start, line.end),
        })
    } else if let Some(after) = body.strip_prefix("quota plan.limit:") {
        let limit = after.trim();
        if limit.is_empty() {
            return Err(line_error(
                line,
                "`gate quota plan.limit:` requires a limit identifier",
            ));
        }
        if !is_plan_ident(limit) {
            return Err(line_error(
                line,
                "`gate quota plan.limit:` requires a lowercase identifier ([a-z][a-z0-9_]*)",
            ));
        }
        Ok(GateDirectiveAst::Quota {
            limit: limit.to_owned(),
            span: Span::new(line.start, line.end),
        })
    } else {
        Err(line_error(
            line,
            "`gate` directives are `gate behind plan.feature: <name>` or `gate quota plan.limit: <name>`",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plan_blocks_empty_source_returns_empty_vec() {
        let plans = parse_plan_blocks("").expect("empty source parses");
        assert!(plans.is_empty());
    }

    #[test]
    fn parse_feature_gates_empty_source_returns_empty_map() {
        let gates = parse_feature_gates("").expect("empty source parses");
        assert!(gates.callables.is_empty());
    }
}
