use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub app: Option<String>,
    pub aggregates: Vec<Aggregate>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aggregate {
    pub name: String,
    pub fields: Vec<Field>,
    pub commands: Vec<Command>,
    pub queries: Vec<Query>,
    pub surfaces: Vec<Surface>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub ty: String,
    pub modifiers: Vec<FieldModifier>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum FieldModifier {
    Required,
    Unique,
    Default(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub input: Vec<String>,
    pub policy: Option<String>,
    pub emits: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub name: String,
    pub search: Vec<String>,
    pub filters: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    pub name: String,
    pub list_columns: Vec<String>,
    pub form_fields: Vec<String>,
    pub detail_fields: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxDocument {
    pub app: Option<LzxApp>,
    pub routes: Vec<LzxRoute>,
    pub experiences: Vec<LzxExperience>,
    pub surfaces: Vec<LzxSurface>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxApp {
    pub name: String,
    pub title: Option<String>,
    pub version: Option<String>,
    pub targets: Vec<String>,
    pub default_locale: Option<String>,
    pub default_timezone: Option<String>,
    pub auth_failed_redirect: Option<String>,
    pub not_found: Option<String>,
    pub uses: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxRoute {
    pub name: String,
    pub path: Option<String>,
    pub routes: Vec<String>,
    pub to: Option<String>,
    pub surface: Option<String>,
    pub audience: Option<String>,
    pub lazy: Option<bool>,
    pub prerender: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExperience {
    pub name: String,
    pub imports: Vec<String>,
    pub views: Vec<LzxExperienceView>,
    pub extensions: Vec<LzxViewExtension>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExperienceView {
    pub name: String,
    pub anchor: Option<String>,
    pub routes: Vec<String>,
    pub extensible_by: Vec<String>,
    pub source: Option<String>,
    pub submit: Option<String>,
    pub blocks: Vec<String>,
    pub actions: Vec<LzxAction>,
    pub opens: Vec<String>,
    pub tests: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAction {
    pub name: String,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxViewExtension {
    pub anchor: String,
    pub blocks: Vec<String>,
    pub slots: Vec<LzxExtensionSlot>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionSlot {
    pub name: String,
    pub order: Option<LzxExtensionOrder>,
    pub blocks: Vec<String>,
    pub platforms: Vec<String>,
    pub audiences: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxExtensionOrder {
    pub relation: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxSurface {
    pub experience: String,
    pub platform: LzxPlatform,
    pub uses_experience: Option<String>,
    pub audiences: Vec<LzxAudience>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LzxPlatform {
    Web,
    Mobile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxAudience {
    pub name: String,
    pub qualifiers: Vec<String>,
    pub views: Vec<LzxPlatformView>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LzxPlatformView {
    pub name: String,
    pub view_type: String,
    pub columns: Vec<String>,
    pub fields: Vec<String>,
    pub sections: Vec<String>,
    pub search: Vec<String>,
    pub filter: Vec<String>,
    pub cells: Vec<String>,
    pub actions: Vec<String>,
    pub submit: Option<String>,
    pub blocks: Vec<String>,
    pub span: Span,
}

// =============================================================================
// Cut A — canonical-indent slice for `feature` skeletons and `agent` blocks.
//
// Sibling to `Document` (legacy brace MVP). The slice deliberately covers
// only `feature <name>` headers and indented `agent <name>` blocks plus
// their Cut A children (tools / evals / discriminated output). Other
// feature children (resources, commands, queries, workflows, ...) remain
// in the legacy pipeline until later cuts migrate them.
//
// See docs/proposals/ai-primitives-v0-implementation.md §3.2 / §3.4.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureSkeleton {
    pub name: String,
    pub agents: Vec<Agent>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub input: Vec<AgentInputSlot>,
    pub context: Option<String>,
    pub policy: Option<Vec<String>>,
    pub rate_limit: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExpose {
    pub method: HttpMethod,
    pub path: String,
    pub route_slots: Vec<AgentExposeRouteSlot>,
    pub audience: Option<String>,
    pub rate_limit_override: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExposeRouteSlot {
    pub name: String,
    pub type_text: String,
    pub span: Span,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInputSlot {
    pub name: String,
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum AgentOutput {
    /// `output stream <Type>` — streaming output of the named type.
    Stream(String),
    /// `output discriminator <Enum>` — single enum-variant output.
    Discriminator(String),
    /// `output <Type>` — bare type reference. Disambiguated at lowering:
    /// records with a `discriminator` marker field become DiscriminatedRecord;
    /// everything else becomes Text (legacy form, soft-warned per Q-impl-5).
    Plain(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTool {
    /// Canonical source text: `customer.query.by_id`, `@tool.web_search`,
    /// `query.by_id` (local shorthand). Lowering qualifies and resolves.
    pub reference: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalCase {
    pub name: String,
    pub assertions: Vec<AgentEvalAssertion>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvalAssertion {
    pub kind: AgentEvalKind,
    pub predicate: AgentEvalPredicate,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEvalKind {
    Requires,
    Forbids,
}

/// Parser-level eval predicate. Captures the three shapes the EBNF (§14)
/// allows inside `requires` / `forbids`:
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
    Closed { text: String },
    Contains { lhs: String, rhs: ContainsRhs },
    ToolsCalls { op: ToolsCallsOp, target: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ContainsRhs {
    /// `requires output contains "active"` — substring literal match.
    Literal(String),
    /// `forbids output contains @semantic.Email` — semantic-type membership.
    /// Validation dispatches at `lazuli test --evals`, never at check-time.
    SemanticType(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolsCallsOp {
    Includes,
    Excludes,
}
