//! Cut A AI primitives — `agent`, `api`, tools, evals.
//!
//! `agent` is Lazuli's typed-LLM declaration. An author writes a single
//! `agent <name>` block with input slots, an output type (or stream, or
//! discriminator), a model reference (`@llm.<name>`), tool bindings,
//! evals, and optional HTTP exposure. Codegen emits one Go handler per
//! agent that delegates dispatch to the runtime LLM adapter; doctor
//! enforces every typed contract before the agent ever runs.
//!
//! ## Why a typed primitive
//!
//! LLM authoring at scale degrades into:
//!
//! - Provider drift (OpenAI vs Anthropic vs Bedrock with subtle API
//!   shape differences).
//! - Untyped tool calls (any function with `name + args` is a "tool";
//!   nothing catches argument-shape drift before runtime).
//! - Eval gaps (each team writes its own eval harness).
//!
//! Lazuli moves the contract upstream. [`Agent::tools`] are
//! [`ToolBinding`]s with closed-catalog [`ToolKind`] and [`ToolEffect`]
//! — the analyzer rewrites every cross-feature reference and resolves
//! adapter tools to their `RegistryToolEntry`. [`Agent::evals`] are
//! [`EvalCase`]s with typed [`EvalAssertion`]s and a bounded
//! [`EvalPredicate`] catalog. The model is a `@llm.<name>` atom that
//! doctor binds to one runtime adapter.
//!
//! ## Output kinds
//!
//! [`AgentOutputKind`] is a closed catalog:
//! - `Text` — bare `output <Type>`.
//! - `Stream` — `output stream <Type>`.
//! - `DiscriminatedEnum` — `output discriminator <Enum>`.
//! - `DiscriminatedRecord` — `output <Record>` where the record's
//!   `discriminator` field carries the enum tag.
//!
//! [`DiscriminatorRef`] captures the resolved discriminator target so
//! codegen can emit the right unmarshaller without re-deriving the
//! schema.
//!
//! ## HTTP exposure
//!
//! [`HttpExposure`] is Cut A.7's auto-mount: declaring `expose http`
//! mounts the agent on an HTTP endpoint with the agent's policy /
//! rate_limit / output applied at the gateway. The language declares
//! method + path + audience; the runtime wires the rest.
//!
//! ## `api <name>` (Phase L Tier 4b)
//!
//! [`Api`] is the freeform-handler escape valve for HTTP endpoints
//! that don't fit `command` / `query` / `agent`. It carries its own
//! policy, rate_limit, output, handler reference, and optional
//! deprecation. The shape mirrors `Command` but binds an HTTP method +
//! path; the handler file is the escape hatch.
//!
//! ## Eval predicates
//!
//! [`EvalPredicate`] is the closed catalog for `requires` / `forbids`
//! eval assertions: closed-language `Predicate`, substring/semantic
//! `Contains`, `tools.calls includes|excludes`, and a permissive
//! `Unparsed(String)` while the typed lifter catches up. The
//! `Unparsed` variant surfaces as a doctor warning so authors know
//! they're paying the cost of bypassing the typed catalog.
//!
//! ## See also
//!
//! - `docs/proposals/ai-primitives-v0.md` + the implementation plan
//!   `docs/proposals/ai-primitives-v0-implementation.md` §4.1 — IR
//!   shape rationale.
//! - `docs/proposals/ai-primitives-cut-a-7.md` — `expose http`.
//! - `docs/proposals/ai-primitives-cut-a-8.md` — built-in trace events
//!   (sibling concern; lives in `nodes::event`).
//! - `docs/proposals/ai-primitives-cut-a-10.md` — `golden` evals.

use serde::{Deserialize, Serialize};

use crate::{
    Deprecation, LocaleNegotiate, Path, PathRef, PolicyExpr, PolicyRef, Predicate, QualifiedName,
    RateLimitSpec, SpanRef, TargetExpr, TranslationKeyRef, TypeRef, TypedSlot,
};

/// Root IR node for an `agent <name> { … }` block — Lazuli's typed
/// LLM-callable surface. Carries the inputs/outputs/model parameters,
/// the policy + rate-limit decorators, declared safety validators,
/// tool bindings, eval cases, and optional HTTP exposure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    /// Feature this agent lives in (canonical lower-snake name).
    pub feature: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    /// IR Error-Vocab — reserved-slot per-agent override for the
    /// `policy_denied` error message. v1 codegen does not consume this
    /// slot (agents only surface errors when exposed via HTTP, which is
    /// modeled separately on `Agent.expose_http`); the IR shape exists
    /// so v2 promotion is purely additive. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    /// `rate_limit "<N per period per scope>"` with optional
    /// env-qualified overrides per `ir-rate-limit-env-aware` (cell 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,
    pub output_kind: AgentOutputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<TypeRef>,
    /// Resolved discriminator target. `None` for `Text` / `Stream` outputs;
    /// `Some(Enum)` for `output discriminator <Enum>`; `Some(RecordField)`
    /// for `output <Record>` after lowering disambiguates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_discriminator: Option<DiscriminatorRef>,
    /// `@llm.<name>` reference. The closed-namespace catalog enforces the
    /// prefix; doctor checks the name resolves to a known LLM adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_path: Option<String>,
    /// `@validator.*` references. Cut A allows 0 or 1; Cut A.5 widens to
    /// many (the `Vec` shape is already correct, so A.5 lands by adding
    /// the coverage diagnostic without an IR shape change).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety: Vec<QualifiedName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evals: Vec<EvalCase>,
    /// Cut A.7 — `expose http` block. Auto-mounts the agent as an
    /// HTTP endpoint with the agent's policy / rate_limit / output
    /// applied at the gateway. Doctor cross-checks path conflicts +
    /// audience reachability; LSP catches local shape issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expose_http: Option<HttpExposure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `expose http` block on an [`Agent`]. Auto-mounts the agent as an
/// HTTP endpoint with the agent's policy / rate_limit / output applied
/// at the gateway. Doctor cross-checks path conflicts + audience
/// reachability; LSP catches local shape issues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpExposure {
    pub method: HttpMethod,
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_slots: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of HTTP methods. Mirrors the existing `api.method`
/// text-pattern catalog (`GET | POST | PUT | PATCH | DELETE`) but now
/// typed in IR. JSON form is uppercase ASCII for wire-stability with
/// HTTP standard conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Phase L Tier 4b — `api <name>` declaration lifted from the
/// canonical-indent slice. Sibling of `Command` but with HTTP transport
/// bound. Replaces the `collect_api_paths` text-pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Api {
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form (see `Command.policy_expr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-api override for the `policy_denied` error
    /// message. Custom HTTP boundaries reach end users directly, so the
    /// override seam mirrors `Command.policy_when_denied`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    /// `rate_limit "<N per period per scope>"` with optional
    /// env-qualified overrides per `ir-rate-limit-env-aware` (cell 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,
    /// `output <TypeRef>` — required for canonical APIs today. Captured
    /// as a `TypeRef` so `@cap.File(...)` outputs project the same way
    /// as command outputs.
    pub output: TypeRef,
    /// `handler "./api/..."` — required for legacy text-pattern APIs;
    /// canonical APIs may opt out in a future cut. Captured as a path.
    pub handler: PathRef,
    /// i18n bucket cycle — per-api `locale_negotiate` override. When
    /// `Some`, supersedes the runtime unit's default for this endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_negotiate: Option<LocaleNegotiate>,
    /// OpenAPI bucket cycle — `deprecated` child block for public APIs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of agent output shapes. Discriminates plain text vs
/// streaming vs the two discriminated-value forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutputKind {
    /// `output <Type>` — bare type reference; the agent returns plain text
    /// (or, for a record with a `discriminator` field, a discriminated
    /// record — see `output_discriminator`).
    Text,
    /// `output stream <Type>` — streaming response.
    Stream,
    /// `output discriminator <Enum>` — single enum-variant response.
    DiscriminatedEnum,
    /// `output <Record>` where the record carries a `discriminator` field.
    DiscriminatedRecord,
}

/// Resolved target of an agent's discriminator output. `Enum` for
/// `output discriminator <Enum>`; `RecordField` for `output <Record>`
/// when the record carries a typed discriminator field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DiscriminatorRef {
    /// `output discriminator <Enum>` — payload is the enum.
    Enum(QualifiedName),
    /// `output <Record>` — payload is the record; one of its fields
    /// carries the `discriminator` marker. The analyzer resolves the
    /// field + its enum type at lowering.
    RecordField {
        record: QualifiedName,
        field: String,
        enum_type: QualifiedName,
    },
}

/// One `tools` entry on an [`Agent`]. `reference` names the tool;
/// the `resolved_*` slots are populated by the expand pass (effect,
/// policy, PII classes) so doctor + codegen can reason about agent
/// permissions cold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBinding {
    pub reference: QualifiedToolRef,
    /// Populated by the expand pass when the workspace IR is loaded;
    /// `None` after pure lowering. Proposal §A1 / plan §4.3 mandate this
    /// derivation runs only under `--expand=tools` / `--expand=security`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_effect: Option<ToolEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_pii_classes: Vec<QualifiedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// JSON shape: `{"target": "Local", "kind": "...", "name": "..."}` for
/// `Local`/`CrossFeature` variants (the inner `kind` is the tool kind);
/// `{"target": "Adapter", "dotted": [...]}` for adapter tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target")]
pub enum QualifiedToolRef {
    /// `query.by_id`, `command.create`, `api.export` — same-feature
    /// shorthand. The analyzer rewrites to `CrossFeature` at expand time.
    Local { kind: ToolKind, name: String },
    /// `customer.query.by_id` — explicit cross-feature reference.
    CrossFeature {
        feature: String,
        kind: ToolKind,
        name: String,
    },
    /// `@tool.web_search`, `@tool.calendar.create_event` — adapter tool.
    /// The dotted tail joins the segments under `@tool.`.
    Adapter { dotted: Vec<String> },
}

/// Closed catalog of tool reference subkinds. Mirrors the authored
/// prefix (`query.list`, `command`, `api`, ...). `QueryUnspecified` is
/// the lowering placeholder before the analyzer resolves the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// `query.list` — collection read.
    QueryList,
    /// `query.lookup` — single-record read.
    QueryLookup,
    /// `query.sql` — opaque SQL read.
    QuerySql,
    /// `query.view` — typed screen-read SQL projection.
    QueryView,
    /// `command` — write.
    Command,
    /// `api` — custom HTTP endpoint; effect derived from `method`.
    Api,
    /// `query` — unspecified subkind; the analyzer narrows to
    /// `QueryList`/`QueryLookup`/`QuerySql`/`QueryView` once the target is known.
    QueryUnspecified,
}

/// Closed catalog of tool side-effect classes. `Read` tools are
/// safe to call repeatedly; `Write` tools mutate state and are gated
/// by policy + audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    /// Read-only tool.
    Read,
    /// State-mutating tool.
    Write,
}

/// One `evals.<name>` block inside an [`Agent`]. Carries the typed
/// assertions and optional golden-file binding the runtime evaluator
/// uses to score the agent's outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub assertions: Vec<EvalAssertion>,
    /// Cut A.10 — optional `golden "./path.jsonl" min_score N`
    /// reference. The runtime adapter loads the file and scores the
    /// agent's output against it; the language stays out of the
    /// scoring algorithm itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub golden: Option<GoldenSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `golden "./path.jsonl" min_score N` — points the eval at a
/// golden-file dataset and pins the gate threshold. Resolution is
/// runtime-side; the language stays out of the scoring algorithm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenSpec {
    /// File path captured verbatim. The runtime resolves it.
    pub path: String,
    /// Optional `min_score N` gate threshold (0.0..=1.0). `None`
    /// means the adapter's default (0.85 by convention) applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `requires <pred>` / `forbids <pred>` assertion inside an
/// [`EvalCase`]. The `kind` axis flips the polarity; `predicate` is
/// the typed predicate sublanguage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalAssertion {
    pub kind: EvalAssertionKind,
    pub predicate: EvalPredicate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog flipping eval assertion polarity. `Requires` holds
/// when the predicate is true; `Forbids` holds when it is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalAssertionKind {
    /// `requires <pred>` — predicate must hold.
    Requires,
    /// `forbids <pred>` — predicate must NOT hold.
    Forbids,
}

/// Typed predicate sublanguage admitted in [`EvalAssertion`]. The
/// `Closed` variant reuses the read-side predicate catalog; the
/// `Contains` / `ToolsCalls` variants cover eval-specific shapes;
/// `Unparsed` is the lowering fallback for shapes the parser has not
/// yet been taught.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EvalPredicate {
    /// The closed predicate sublanguage. Lowering parses simple `<path>
    /// <op> <literal>` forms; richer shapes hit `Unparsed` until a future
    /// cut extends the predicate parser.
    Closed(Predicate),
    /// `<ref> contains <token-literal>` / `<ref> contains <@semantic.Type>`.
    /// Semantic-type validators dispatch at `lazuli test --evals` only —
    /// `lazuli check` validates predicate shape, never dispatches.
    Contains { lhs: Path, rhs: EvalContainsRhs },
    /// `tools.calls includes|excludes <tool-ref>`.
    ToolsCalls {
        op: ToolsCallsOp,
        target: QualifiedToolRef,
    },
    /// Source text the lowering could not yet structure. Doctor surfaces
    /// these as warnings; later predicate-parser extensions promote them
    /// to `Closed`.
    Unparsed(String),
}

/// Closed catalog of right-hand-side shapes admitted in a `contains`
/// eval predicate. `Literal` is the simple substring case; `SemanticType`
/// dispatches the type's auto validator under `lazuli test --evals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EvalContainsRhs {
    /// `"active"` — substring literal.
    Literal(String),
    /// `@semantic.Email` — membership matched by the type's auto validator.
    SemanticType(QualifiedName),
}

/// Closed catalog of `tools.calls <op> <ref>` operators. `Includes`
/// asserts the agent did call the tool; `Excludes` asserts it didn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolsCallsOp {
    /// Agent must have called this tool.
    Includes,
    /// Agent must NOT have called this tool.
    Excludes,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn http_method_serializes_uppercase() {
        assert_eq!(serde_json::to_value(HttpMethod::Post).unwrap(), json!("POST"));
        assert_eq!(serde_json::to_value(HttpMethod::Patch).unwrap(), json!("PATCH"));
    }

    #[test]
    fn agent_output_kind_snake_case() {
        for (k, s) in [
            (AgentOutputKind::Text, "text"),
            (AgentOutputKind::Stream, "stream"),
            (AgentOutputKind::DiscriminatedEnum, "discriminated_enum"),
            (AgentOutputKind::DiscriminatedRecord, "discriminated_record"),
        ] {
            assert_eq!(serde_json::to_value(k).unwrap(), json!(s));
        }
    }

    #[test]
    fn tool_effect_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(ToolEffect::Write).unwrap(),
            json!("write")
        );
    }

    #[test]
    fn qualified_tool_ref_local_round_trips() {
        let r = QualifiedToolRef::Local {
            kind: ToolKind::QueryLookup,
            name: "by_id".to_owned(),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["target"], json!("Local"));
        assert_eq!(v["kind"], json!("query_lookup"));
        let back: QualifiedToolRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn qualified_tool_ref_adapter_serializes_dotted() {
        let r = QualifiedToolRef::Adapter {
            dotted: vec!["calendar".to_owned(), "create_event".to_owned()],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["target"], json!("Adapter"));
        assert_eq!(v["dotted"], json!(["calendar", "create_event"]));
        let back: QualifiedToolRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn discriminator_ref_enum_round_trips() {
        let d = DiscriminatorRef::Enum(QualifiedName {
            feature: None,
            name: "Status".to_owned(),
        });
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["kind"], json!("Enum"));
        let back: DiscriminatorRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn eval_assertion_kind_round_trips() {
        for k in [EvalAssertionKind::Requires, EvalAssertionKind::Forbids] {
            let v = serde_json::to_value(k).unwrap();
            let back: EvalAssertionKind = serde_json::from_value(v).unwrap();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn tools_calls_op_round_trips() {
        for op in [ToolsCallsOp::Includes, ToolsCallsOp::Excludes] {
            let v = serde_json::to_value(op).unwrap();
            let back: ToolsCallsOp = serde_json::from_value(v).unwrap();
            assert_eq!(back, op);
        }
    }
}
