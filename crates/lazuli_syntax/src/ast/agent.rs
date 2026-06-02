//! `agent <name>` AST surface — LLM agent declaration.
//!
//! Agents are first-class language constructs (not just adapters): the
//! input shape, output shape, prompt, tools, evals, and HTTP exposure
//! are typed at the source level. The Lazuli Go runtime supplies the
//! LLM transport; the language commits to nothing about the provider.
//!
//! Authoring shape (canonical):
//!
//! ```text
//! agent summarize_customer
//!   input
//!     customer_id: ID required
//!   context @context.tenant
//!   policy @policy.agent
//!   rate_limit "100 per minute per tenant"
//!   output stream Summary
//!   model "claude-opus-4-7"
//!   temperature 0.2
//!   prompt @prompt.summarize_customer
//!   tools customer.query.by_id, @tool.web_search
//!   evals
//!     case basic
//!       requires output contains "active"
//!     case golden_set
//!       golden "./evals/customer_summaries.jsonl" min_score 0.9
//!   expose http
//!     POST "/agents/summarize_customer/:customer_id"
//!     route customer_id: ID
//! ```
//!
//! Eval predicates are tri-modal (`AgentEvalPredicate`):
//! - **Closed**: passes through the predicate-DSL string for lowering
//!   to re-parse against the canonical predicate AST.
//! - **Contains**: `<ref> contains <STRING | @semantic.Type>`.
//! - **ToolsCalls**: `tools.calls includes|excludes <tool-ref>`.
//!
//! `expose` is Cut A.7 — auto-mounts the agent as an HTTP endpoint that
//! inherits the agent's policy / rate_limit / output.

use serde::{Deserialize, Serialize};

use super::{RateLimitSpecAst, Span};

/// `agent <name>` block — LLM agent declaration.
///
/// First-class language construct (not an adapter): the input shape,
/// output shape, prompt, tools, evals, and optional HTTP exposure are
/// typed at source level. The runtime supplies the LLM transport; the
/// language pins nothing about the provider. See the module-level docs
/// for the full authoring shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub input: Vec<AgentInputSlot>,
    pub context: Option<String>,
    pub policy: Option<Vec<String>>,
    /// `rate_limit "<N per period per scope>"` declarations on the
    /// agent. Env-aware per `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    pub output: Option<AgentOutput>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    pub seed: Option<i64>,
    pub prompt: Option<String>,
    pub safety: Vec<String>,
    pub tools: Vec<AgentTool>,
    pub evals: Vec<AgentEvalCase>,
    /// Cut A.7 — `expose http` block. Auto-mounts the agent as an
    /// HTTP endpoint; the agent's policy / rate_limit / output apply
    /// to the exposed surface.
    pub expose: Option<AgentExpose>,
    pub span: Span,
}

/// Cut A.7 `expose http` block on an [`Agent`].
///
/// Mounts the agent as an HTTP endpoint that inherits the agent's
/// policy / rate_limit / output. The endpoint is generated; no Go
/// handler is required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExpose {
    /// HTTP method authored on the exposure line.
    pub method: HttpMethod,
    /// Path literal verbatim (placeholders kept inline).
    pub path: String,
    /// Path placeholder declarations (`route <name>: <Type>`).
    pub route_slots: Vec<AgentExposeRouteSlot>,
    /// `audience @audience.<name>` — optional restriction.
    pub audience: Option<String>,
    /// `rate_limit "..."` — optional override over the agent's own limit.
    pub rate_limit_override: Option<String>,
    pub span: Span,
}

/// One `route <name>: <Type>` placeholder inside an [`AgentExpose`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExposeRouteSlot {
    pub name: String,
    /// Type literal verbatim (`ID`, `Text`, ...).
    pub type_text: String,
    pub span: Span,
}

/// Closed catalog of HTTP methods recognised by `api` / `agent expose`.
///
/// Serialises as the uppercase canonical token (`GET`, `POST`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// Parse a canonical uppercase method token. Returns `None` on
    /// unknown tokens — callers turn that into a `ParseError`.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::HttpMethod;
    ///
    /// assert_eq!(HttpMethod::from_token("GET"), Some(HttpMethod::Get));
    /// assert_eq!(HttpMethod::from_token("POST"), Some(HttpMethod::Post));
    /// assert_eq!(HttpMethod::from_token("get"), None); // lowercase rejected
    /// assert_eq!(HttpMethod::from_token("OPTIONS"), None);
    /// ```
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            _ => None,
        }
    }
}

/// One `input` row inside an [`Agent`]: `<name>: <type> [required|optional]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInputSlot {
    pub name: String,
    /// Type literal verbatim (`ID`, `Text`, `@semantic.Email`, ...).
    pub type_text: String,
    /// `required` modifier authored.
    pub required: bool,
    /// `optional` modifier authored.
    pub optional: bool,
    pub span: Span,
}

/// Closed three-arm catalog for `output ...` on an [`Agent`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum AgentOutput {
    /// `output stream <Type>` — streaming output of the named type.
    Stream(String),
    /// `output discriminator <Enum>` — single enum-variant output.
    Discriminator(String),
    /// `output <Type>` — bare type reference. Lowered to `Text`; a future
    /// expand pass would resolve records with a `discriminator` marker
    /// field (unbuilt). Legacy form, soft-warned per Q-impl-5.
    Plain(String),
}

/// One `tools <ref>` entry on an [`Agent`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTool {
    /// Canonical source text: `customer.query.by_id`, `@tool.web_search`,
    /// `query.by_id` (local shorthand). Lowering qualifies and resolves.
    pub reference: String,
    pub span: Span,
}

/// One `case <name>` row inside an [`Agent`]'s `evals` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalCase {
    pub name: String,
    pub assertions: Vec<AgentEvalAssertion>,
    /// Cut A.10 — optional `golden "./path.jsonl" min_score N`
    /// reference. The runtime adapter loads the file and scores the
    /// agent's output against it; `min_score` (0.0–1.0) is the gate
    /// threshold. Language stays out of the scoring algorithm.
    pub golden: Option<AgentEvalGolden>,
    pub span: Span,
}

/// Cut A.10 `golden "./path.jsonl" min_score N` sidecar on an [`AgentEvalCase`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalGolden {
    /// File path captured verbatim. The runtime resolves it.
    pub path: String,
    /// Optional `min_score N` threshold (0.0..=1.0). The default
    /// when omitted is 0.85 by adapter convention; language pins
    /// only what the author wrote.
    pub min_score: Option<f64>,
    pub span: Span,
}

/// One assertion row inside an [`AgentEvalCase`] (`allows` or `denies`
/// + a predicate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalAssertion {
    pub kind: AgentEvalKind,
    pub predicate: AgentEvalPredicate,
    pub span: Span,
}

/// Closed two-arm catalog distinguishing `allows` from `denies` on
/// an [`AgentEvalAssertion`].
///
/// SPEC-08 folded eval polarity into the same authored `allows`/`denies`
/// dialect every other authored test uses; the eval predicate subject
/// (the agent-output assertion) names the dimension, not a bespoke verb.
/// The retired `requires`/`forbids` spellings hard-error in the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEvalKind {
    /// `allows <predicate>` — must hold for the case to pass.
    Allows,
    /// `denies <predicate>` — must NOT hold for the case to pass.
    Denies,
}

/// Parser-level eval predicate. Captures the three shapes the EBNF (§14)
/// allows inside `allows` / `denies`:
///
/// - the closed predicate language (recorded verbatim for lowering),
/// - `<ref> contains <STRING | @semantic.Type>`,
/// - `tools.calls includes|excludes <tool-ref>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AgentEvalPredicate {
    /// Source text passed through to lowering, which re-parses against the
    /// canonical predicate AST. The parser captures the raw form here so
    /// any predicate-language extensions land without churn in this crate.
    Closed {
        text: String,
    },
    Contains {
        lhs: String,
        rhs: ContainsRhs,
    },
    ToolsCalls {
        op: ToolsCallsOp,
        target: String,
    },
}

/// Right-hand side of a `<ref> contains <RHS>` predicate inside an
/// [`AgentEvalPredicate::Contains`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ContainsRhs {
    /// `allows output contains "active"` — substring literal match.
    Literal(String),
    /// `denies output contains @semantic.Email` — semantic-type membership.
    /// Validation dispatches at `lazuli test --evals`, never at check-time.
    SemanticType(String),
}

/// Closed two-arm catalog for the `tools.calls <op>` predicate operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolsCallsOp {
    /// `tools.calls includes <tool-ref>` — call must occur.
    Includes,
    /// `tools.calls excludes <tool-ref>` — call must NOT occur.
    Excludes,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_from_token_matches_uppercase_only() {
        assert_eq!(HttpMethod::from_token("GET"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_token("get"), None);
        assert_eq!(HttpMethod::from_token("UNKNOWN"), None);
    }

    #[test]
    fn agent_output_serde_token_tagged() {
        let v = serde_json::to_value(AgentOutput::Stream("Summary".into())).unwrap();
        assert_eq!(v["kind"], "Stream");
        assert_eq!(v["value"], "Summary");
    }

    #[test]
    fn agent_eval_kind_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(AgentEvalKind::Allows).unwrap(),
            serde_json::json!("allows")
        );
    }
}
