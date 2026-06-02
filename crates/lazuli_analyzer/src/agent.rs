//! Agent + tool reference + eval-case lowering — the LLM capability
//! slot of a feature.
//!
//! ## Role in the pipeline
//!
//! Lifts a `syntax::Agent` (the parsed `agent <name> { ... }` block)
//! onto `ir::Agent`. The IR carries an LLM capability with:
//!
//! * typed `input` slots (mirrors `command` input shape),
//! * optional `policy` atom + `policy_when_denied` redirect,
//! * `output` projection: `text` / `stream` / `enum` discriminator /
//!   record with discriminator field (promoted at expand-pass time
//!   based on resolved type),
//! * `model`, `safety` list, `temperature`, `max_tokens`, `top_p`,
//!   `seed`, `prompt_path`, `rate_limit`,
//! * `tools` (typed `ToolBinding` references — local short form,
//!   cross-feature qualified form, and `@tool.<adapter>.<...>` adapter
//!   form),
//! * `evals` (closed assertion catalog with `requires` / `forbids`
//!   over `contains` / `tools.calls` / a narrow closed predicate
//!   sub-language).
//!
//! Tool-reference lowering recognizes three shapes:
//!
//!   * `@tool.<adapter>.<...>` → `QualifiedToolRef::Adapter`
//!   * `<feature>.<kind>.<name>` (or `<feature>.query.list.<name>`) →
//!     `QualifiedToolRef::CrossFeature`
//!   * `<kind>.<name>` (e.g. `query.by_id`, `command.create`,
//!     `api.export`) → `QualifiedToolRef::Local`
//!
//! Eval predicates lower closed assertion shapes; unrecognized
//! free-form predicates fall through to `EvalPredicate::Unparsed`
//! so doctor can surface them without the analyzer needing a full
//! predicate parser.
//!
//! ## Cross-references
//!
//! * Input: `lazuli_syntax::ast::Agent`, `AgentOutput`, `AgentExpose`,
//!   `AgentEvalCase`, `AgentEvalPredicate`, `ContainsRhs`,
//!   `ToolsCallsOp`, `HttpMethod`.
//! * Output: `lazuli_ir::Agent`, `ToolBinding`, `QualifiedToolRef`,
//!   `ToolKind`, `EvalCase`, `EvalAssertion`, `EvalPredicate`,
//!   `HttpExposure`, `AgentOutputKind`, `DiscriminatorRef`.
//! * Diagnostic: `InvalidToolRef` — emitted when a tool reference
//!   fails all three shape recognizers.
//!
//! ## ABI guarantee
//!
//! `lower_agent` is `pub` (consumed by `lazuli_cli` via the canonical
//! `lazuli_analyzer::lower_agent` path); per-slot helpers stay
//! `pub(crate)`.

use crate::expr::{expr_from_text, lower_policy_atom};
use crate::helpers::{find_top_level_operator, span_of};
use crate::resource::lower_rate_limit_spec;
use crate::{AnalyzeError, qualified_name_local, qualified_namespace, type_ref_from_text};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Lower a single `agent` AST node into the IR form. The `feature` arg
/// pins the owning feature name on the IR record so cross-feature doctor
/// checks can rebuild `<feature>.agent.<name>` references.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::lower_agent;
/// use lazuli_syntax::Agent;
///
/// let agent: Agent = unimplemented!("from canonical-indent parse");
/// let lowered = lower_agent("Support", &agent)?;
/// assert!(!lowered.name.is_empty());
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
pub fn lower_agent(feature: &str, agent: &syntax::Agent) -> Result<ir::Agent, AnalyzeError> {
    let input = agent
        .input
        .iter()
        .map(|slot| ir::TypedSlot {
            name: slot.name.clone(),
            type_ref: type_ref_from_text(&slot.type_text),
            required: slot.required,
            constraints: ir::FieldConstraints::default(),
            validate_skip: false,
        })
        .collect();

    let policy = agent
        .policy
        .as_ref()
        .and_then(|atoms| atoms.first())
        .map(|first| lower_policy_atom(first));

    let (output_kind, output_type, output_discriminator) = match &agent.output {
        Some(syntax::AgentOutput::Stream(ty)) => (
            ir::AgentOutputKind::Stream,
            Some(type_ref_from_text(ty)),
            None,
        ),
        Some(syntax::AgentOutput::Discriminator(name)) => (
            ir::AgentOutputKind::DiscriminatedEnum,
            None,
            Some(ir::DiscriminatorRef::Enum(qualified_name_local(name))),
        ),
        Some(syntax::AgentOutput::Plain(ty)) => (
            // Lowering can't tell `Text` from `DiscriminatedRecord` without
            // the feature scope (enum vs record). Default to `Text`; the
            // expand pass (Phase 5) promotes to `DiscriminatedRecord` when
            // the resolved type is a record with a `discriminator` field.
            ir::AgentOutputKind::Text,
            Some(type_ref_from_text(ty)),
            None,
        ),
        None => (ir::AgentOutputKind::Text, None, None),
    };

    let model = agent.model.as_ref().map(|s| qualified_namespace(s));

    let safety = agent
        .safety
        .iter()
        .map(|s| qualified_namespace(s))
        .collect();

    let mut tools = Vec::with_capacity(agent.tools.len());
    for tool_ast in &agent.tools {
        tools.push(ir::ToolBinding {
            reference: lower_tool_ref(&tool_ast.reference, feature)?,
            resolved_effect: None,
            resolved_policy: None,
            resolved_pii_classes: Vec::new(),
            span_ref: Some(span_of(tool_ast.span)),
        });
    }

    let mut evals = Vec::with_capacity(agent.evals.len());
    for case_ast in &agent.evals {
        evals.push(lower_eval_case(case_ast, feature)?);
    }

    let expose_http = agent.expose.as_ref().map(lower_agent_expose);

    Ok(ir::Agent {
        name: agent.name.clone(),
        feature: feature.to_owned(),
        input,
        context: None, // Phase 1 parser does not yet structure context expressions.
        policy,
        policy_when_denied: None,
        rate_limit: agent.rate_limit.as_ref().map(lower_rate_limit_spec),
        output_kind,
        output_type,
        output_discriminator,
        model,
        temperature: agent.temperature,
        max_tokens: agent.max_tokens,
        top_p: agent.top_p,
        seed: agent.seed,
        prompt_path: agent.prompt.clone(),
        safety,
        tools,
        evals,
        expose_http,
        span_ref: Some(span_of(agent.span)),
    })
}

/// Cut A.7 — lower an `expose http` AST block. Method enum maps 1:1;
/// route slots become `TypedSlot`s with `required: true` (path params
/// are inherently required); audience / rate-limit pass-through as
/// strings.
pub(crate) fn lower_agent_expose(expose: &syntax::AgentExpose) -> ir::HttpExposure {
    let route_slots = expose
        .route_slots
        .iter()
        .map(|slot| ir::TypedSlot {
            name: slot.name.clone(),
            type_ref: type_ref_from_text(&slot.type_text),
            required: true,
            constraints: ir::FieldConstraints::default(),
            validate_skip: false,
        })
        .collect();
    ir::HttpExposure {
        method: match expose.method {
            syntax::HttpMethod::Get => ir::HttpMethod::Get,
            syntax::HttpMethod::Post => ir::HttpMethod::Post,
            syntax::HttpMethod::Put => ir::HttpMethod::Put,
            syntax::HttpMethod::Patch => ir::HttpMethod::Patch,
            syntax::HttpMethod::Delete => ir::HttpMethod::Delete,
        },
        path: expose.path.clone(),
        route_slots,
        audience: expose.audience.clone(),
        rate_limit_override: expose.rate_limit_override.clone(),
        span_ref: Some(span_of(expose.span)),
    }
}

/// Lower a single tool reference. `feature` is the owning feature so the
/// short form `query.by_id` rewrites to `Local` and the analyzer
/// preserves the same-feature locality for the expand pass to resolve.
pub(crate) fn lower_tool_ref(
    raw: &str,
    _feature: &str,
) -> Result<ir::QualifiedToolRef, AnalyzeError> {
    if let Some(rest) = raw.strip_prefix("@tool.") {
        if rest.is_empty() {
            return Err(AnalyzeError::InvalidToolRef {
                reference: raw.to_owned(),
            });
        }
        let dotted: Vec<String> = rest.split('.').map(str::to_owned).collect();
        return Ok(ir::QualifiedToolRef::Adapter { dotted });
    }

    // Tail tokens after the feature prefix (if any). The reference is
    // dotted; `query.list` / `query.lookup` / `query.sql` are the
    // three legal three-segment kinds.
    let segments: Vec<&str> = raw.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(AnalyzeError::InvalidToolRef {
            reference: raw.to_owned(),
        });
    }

    // Local shorthand: `query.by_id`, `command.create`, `api.export`,
    // `query.list.by_email`, etc.
    if let Some((kind, name)) = parse_tool_kind_local(&segments) {
        return Ok(ir::QualifiedToolRef::Local { kind, name });
    }

    // Cross-feature: `<feature>.<kind>.<name>` or
    // `<feature>.query.list.<name>` / `query.lookup.<name>` / `query.sql.<name>`.
    if segments.len() >= 3 {
        let feature = segments[0].to_owned();
        if let Some((kind, name)) = parse_tool_kind_local(&segments[1..]) {
            return Ok(ir::QualifiedToolRef::CrossFeature {
                feature,
                kind,
                name,
            });
        }
    }

    Err(AnalyzeError::InvalidToolRef {
        reference: raw.to_owned(),
    })
}

/// Recognize the trailing `(kind, name)` of a tool reference. Accepts:
///
///   - `query.list.<name>`     -> QueryList
///   - `query.lookup.<name>`   -> QueryLookup
///   - `query.sql.<name>`      -> QuerySql
///   - `query.view.<name>`     -> QueryView
///   - `query.<name>`          -> QueryUnspecified
///   - `command.<name>`        -> Command
///   - `api.<name>`            -> Api
fn parse_tool_kind_local(segments: &[&str]) -> Option<(ir::ToolKind, String)> {
    match segments {
        ["query", "list", name] => Some((ir::ToolKind::QueryList, (*name).to_owned())),
        ["query", "lookup", name] => Some((ir::ToolKind::QueryLookup, (*name).to_owned())),
        ["query", "sql", name] => Some((ir::ToolKind::QuerySql, (*name).to_owned())),
        ["query", "view", name] => Some((ir::ToolKind::QueryView, (*name).to_owned())),
        ["query", name] => Some((ir::ToolKind::QueryUnspecified, (*name).to_owned())),
        ["command", name] => Some((ir::ToolKind::Command, (*name).to_owned())),
        ["api", name] => Some((ir::ToolKind::Api, (*name).to_owned())),
        _ => None,
    }
}

pub(crate) fn lower_eval_case(
    case: &syntax::AgentEvalCase,
    feature: &str,
) -> Result<ir::EvalCase, AnalyzeError> {
    let mut assertions = Vec::with_capacity(case.assertions.len());
    for assertion in &case.assertions {
        assertions.push(ir::EvalAssertion {
            kind: match assertion.kind {
                syntax::AgentEvalKind::Allows => ir::EvalAssertionKind::Allows,
                syntax::AgentEvalKind::Denies => ir::EvalAssertionKind::Denies,
            },
            predicate: lower_eval_predicate(&assertion.predicate, feature)?,
            span_ref: Some(span_of(assertion.span)),
        });
    }
    let golden = case.golden.as_ref().map(|g| ir::GoldenSpec {
        path: g.path.clone(),
        min_score: g.min_score,
        span_ref: Some(span_of(g.span)),
    });
    Ok(ir::EvalCase {
        name: case.name.clone(),
        assertions,
        golden,
        span_ref: Some(span_of(case.span)),
    })
}

pub(crate) fn lower_eval_predicate(
    predicate: &syntax::AgentEvalPredicate,
    feature: &str,
) -> Result<ir::EvalPredicate, AnalyzeError> {
    match predicate {
        syntax::AgentEvalPredicate::Contains { lhs, rhs } => Ok(ir::EvalPredicate::Contains {
            lhs: ir::Path::from_segments(lhs.split('.').map(str::to_owned)),
            rhs: match rhs {
                syntax::ContainsRhs::Literal(s) => ir::EvalContainsRhs::Literal(s.clone()),
                syntax::ContainsRhs::SemanticType(s) => {
                    ir::EvalContainsRhs::SemanticType(qualified_namespace(s))
                }
            },
        }),
        syntax::AgentEvalPredicate::ToolsCalls { op, target } => {
            Ok(ir::EvalPredicate::ToolsCalls {
                op: match op {
                    syntax::ToolsCallsOp::Includes => ir::ToolsCallsOp::Includes,
                    syntax::ToolsCallsOp::Excludes => ir::ToolsCallsOp::Excludes,
                },
                target: lower_tool_ref(target, feature)?,
            })
        }
        syntax::AgentEvalPredicate::Closed { text } => Ok(parse_closed_predicate(text)),
    }
}

/// Parse the simple `<path> <op> <literal>` subset of the closed predicate
/// language. Richer shapes (compound `AND`/`OR`, `has`, parenthesised
/// expressions) fall through to `EvalPredicate::Unparsed` so doctor can
/// surface them — the parser stays narrow until the canonical predicate
/// parser lands.
pub fn parse_closed_predicate(text: &str) -> ir::EvalPredicate {
    let trimmed = text.trim();
    // Try ordered ops first (longest token wins to avoid `<=` parsing as `<`).
    // SPEC-05 — `==` is THE equality operator in the closed predicate
    // language. Bare `=` is no longer a comparison token (it keeps its
    // assignment/default/enum-storage roles elsewhere); a `=`-as-equality
    // predicate matches nothing here and falls through to `Unparsed`, which
    // `PREDICATE-EQ-OPERATOR-001` keys on with a fix-it to `==`.
    for (token, op) in [
        ("<=", ir::CompareOp::Le),
        (">=", ir::CompareOp::Ge),
        ("!=", ir::CompareOp::Ne),
        ("<", ir::CompareOp::Lt),
        (">", ir::CompareOp::Gt),
        ("==", ir::CompareOp::Eq),
    ] {
        if let Some(idx) = find_top_level_operator(trimmed, token) {
            let (lhs_text, rhs_text) = trimmed.split_at(idx);
            let rhs_text = &rhs_text[token.len()..];
            let lhs = lhs_text.trim();
            let rhs = rhs_text.trim();
            if lhs.is_empty() || rhs.is_empty() {
                return ir::EvalPredicate::Unparsed(text.to_owned());
            }
            return ir::EvalPredicate::Closed(ir::Predicate::Comparison {
                left: expr_from_text(lhs),
                op,
                right: expr_from_text(rhs),
            });
        }
    }
    ir::EvalPredicate::Unparsed(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_kind_local_recognizes_query_kinds() {
        assert!(matches!(
            parse_tool_kind_local(&["query", "by_id"]),
            Some((ir::ToolKind::QueryUnspecified, _))
        ));
        assert!(matches!(
            parse_tool_kind_local(&["query", "list", "customers"]),
            Some((ir::ToolKind::QueryList, _))
        ));
        assert!(matches!(
            parse_tool_kind_local(&["command", "create"]),
            Some((ir::ToolKind::Command, _))
        ));
    }

    #[test]
    fn parse_tool_kind_local_rejects_unknown_shape() {
        assert!(parse_tool_kind_local(&["nonsense"]).is_none());
        assert!(parse_tool_kind_local(&["query", "view", "x", "y"]).is_none());
    }
}
