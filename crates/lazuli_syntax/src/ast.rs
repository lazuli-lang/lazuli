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
    /// Phase L — `auth` block. At most one per feature. Lowered into
    /// `ir::Auth` via the analyzer; the surface AST mirrors the IR
    /// shape so the only translation the analyzer performs is field
    /// resolution (`Customer.email` → `FieldRef`).
    pub auth: Option<Auth>,
    /// Phase L Tier 3 — `job <name>` blocks.
    pub jobs: Vec<Job>,
    /// Phase L Tier 3 — `webhook <name>` blocks.
    pub webhooks: Vec<Webhook>,
    /// Phase L Tier 3 — `notification <name>` blocks.
    pub notifications: Vec<Notification>,
    /// Phase L Tier 3 — `event_group <pattern> on <Resource>` blocks.
    pub event_groups: Vec<EventGroup>,
    /// Phase L Tier 4a — `defaults` block. Optional; at most one per
    /// feature. Children captured: `tenancy <axis>`, `timestamps`,
    /// `policy_for <kinds>: <atom-list>`.
    pub defaults: Option<FeatureDefaults>,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// Phase L Tier 4a — feature-level `defaults` block.
//
// The `defaults` block declares feature-level inheritance for tenancy,
// timestamps, and policy. Resource-local declarations override these.
// The IR already carries `ir::Defaults`; this AST mirrors that shape so
// lowering is structural.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDefaults {
    /// `tenancy org`, `tenancy team`, `tenancy none`, or a custom axis.
    pub tenancy: Option<DefaultsTenancy>,
    /// `timestamps` declared verbatim. Absent when not authored.
    pub timestamps: bool,
    /// `policy_for jobs, webhooks: @actor.system` style entries. Each
    /// entry binds a list of construct kinds (`jobs`, `webhooks`,
    /// `commands`, ...) to a single policy atom.
    pub policy_for: Vec<DefaultsPolicyFor>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DefaultsTenancy {
    /// `tenancy org`.
    Org,
    /// `tenancy team`.
    Team,
    /// `tenancy none` — explicit opt-out.
    None,
    /// `tenancy workspace` and similar custom identifiers.
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultsPolicyFor {
    /// Construct kinds the policy applies to (`jobs`, `webhooks`,
    /// `commands`, `apis`, etc.). Comma-separated in source.
    pub kinds: Vec<String>,
    /// The policy atom literal, e.g. `@actor.system`. Captured verbatim
    /// so the analyzer can decide between `PolicyRef::Atom` and other
    /// variants without re-parsing surface text.
    pub atom: String,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// Phase L — `auth` block (canonical-indent slice)
//
// `auth` declares the identity domain of a feature: a single identity
// field plus optional password / mfa / sessions / oauth subcontracts.
// One `auth` block per feature.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    pub identity: AuthIdentity,
    pub password: Option<AuthPassword>,
    pub sessions: Option<AuthSessions>,
    pub mfa: Option<AuthMfa>,
    pub oauth: Vec<AuthOAuthProvider>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthIdentity {
    /// Raw source text `Customer.email`. Lowering splits into
    /// `FieldRef { resource, field }`.
    pub field: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPassword {
    /// `algorithm argon2id` — required.
    pub algorithm: String,
    /// `hash @fn.<name>` — extension fn reference.
    pub hash: String,
    /// `verify @fn.<name>` — extension fn reference.
    pub verify: String,
    /// `rate_limit "5 per 10 minutes"` — optional declarative throttle.
    pub rate_limit: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessions {
    /// `resource CustomerSession` — name only; analyzer resolves the
    /// resource against the feature's domain.
    pub resource: String,
    /// `ttl "7 days"` — duration string parsed by the adapter.
    pub ttl: String,
    /// `refresh true|false` — whether refresh tokens are issued.
    pub refresh: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMfa {
    /// MFA method id, e.g. `totp`, `sms`, `webauthn`. Adapter-specific
    /// beyond this.
    pub method: String,
    /// `enroll @fn.<name>` — required extension fn reference.
    pub enroll: String,
    /// `verify @validator.<name>` or `@fn.<name>` — required.
    pub verify: String,
    /// `adapter @adapter.<name>` — optional adapter reference.
    pub adapter: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOAuthProvider {
    /// Provider id, e.g. `google`, `github`, `microsoft`.
    pub provider: String,
    /// `adapter @adapter.<provider>_oauth` — required.
    pub adapter: String,
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
    /// Cut A.10 — optional `golden "./path.jsonl" min_score N`
    /// reference. The runtime adapter loads the file and scores the
    /// agent's output against it; `min_score` (0.0–1.0) is the gate
    /// threshold. Language stays out of the scoring algorithm.
    pub golden: Option<AgentEvalGolden>,
    pub span: Span,
}

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

// =============================================================================
// Phase L Tier 3 — job / webhook / notification / event_group skeletons.
//
// All four constructs are feature children authored at
// AGENT_INDENT_FEATURE_CHILD (2 spaces). Their grandchildren mirror the IR
// shapes (`ir::Job`, `ir::Webhook`, `ir::Notification`, `ir::EventGroup`)
// so lowering is structural.
//
// Route C (`docs/proposals/phase-l-tier-3-job-effect-scope.md:292-348`):
// declarative-body grammar (`target query.by_id(...)`, `let new_score = ...`,
// `updates Customer ... emits ...`) is captured as raw strings until Tier 4
// lifts the shared declarative spine alongside `parse_command`. Handler-backed
// bodies (`handler "./..."`) lower fully.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    pub trigger: JobTrigger,
    /// `queue customer_imports` — execution lane for queued workers.
    pub queue: Option<String>,
    /// `tenant_from payload.<axis>_id` — path captured verbatim.
    pub tenant_from: Option<String>,
    /// `fanout tenants <axis>` — scheduled-job fanout directive.
    pub fanout: Option<JobFanout>,
    /// `idempotency by <path>` — path captured verbatim.
    pub idempotency_by: Option<String>,
    /// `retry <count> backoff <strategy>` — pair captured directly.
    pub retry: Option<JobRetry>,
    /// `policy @policy.<...>` — captured verbatim for lowering.
    pub policy: Option<String>,
    /// `timeout "30s"` — adapter-parsed duration literal.
    pub timeout: Option<String>,
    /// `calls <slot>.<op>` blocks lifted as `ExternalCallRef` shapes.
    pub external_calls: Vec<JobExternalCall>,
    /// Body of the job. Handler-backed bodies fully lower; declarative
    /// bodies stay as raw lines until Tier 4.
    pub body: JobBody,
    /// `emits <event>` lines. Each is one event name (qualified or not).
    pub emits: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobTrigger {
    /// `trigger event customer.activated`.
    Event(String),
    /// `trigger schedule "0 2 * * *"`.
    Schedule(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFanout {
    /// `tenants` — closed scope catalog today.
    pub scope: String,
    pub axis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRetry {
    pub count: u32,
    /// `fixed` or `exponential` — closed strategy catalog today.
    pub backoff: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExternalCall {
    pub slot: String,
    pub op: String,
    /// `arg_name = path.expr` pairs captured verbatim. Tier 4 lifts
    /// the right-hand-side expressions.
    pub args: Vec<JobExternalCallArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExternalCallArg {
    pub name: String,
    /// Right-hand side captured verbatim until Tier 4.
    pub value: String,
    pub span: Span,
}

/// Body of a job. `Handler` is a path reference; `Declarative` is
/// captured as raw lines until Tier 4 lifts the shared spine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobBody {
    Handler(JobHandler),
    Declarative(JobDeclarativeRaw),
    /// No `handler` and no `target` / `updates` / `creates` / `deletes`
    /// authored. Some fixture jobs ship only `emits` (event reactors
    /// with no declarative body); analyzer treats this as a parse error
    /// only when neither effect nor emits is declared.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHandler {
    /// `"./jobs/process_import.go"` — quotes stripped.
    pub path: String,
    /// Optional `returns <Type>` suffix.
    pub returns: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDeclarativeRaw {
    /// `target query.by_id(...)` line, sans the `target ` prefix.
    pub target: Option<String>,
    /// `let new_score = ...` lines, sans the `let ` prefix.
    pub lets: Vec<String>,
    /// `updates Customer\n  score = new_score` collapsed into a single
    /// raw block string. Tier 4 will lift the assignments.
    pub effect: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub name: String,
    /// `path "/webhooks/..."` — raw HTTP route literal.
    pub route: String,
    /// `verify hmac sha256` + nested `secret`/`header`. Required.
    pub verify: WebhookVerify,
    /// `tenant_from payload.<axis>_id` — path captured verbatim.
    pub tenant_from: Option<String>,
    /// `idempotency by <path>` — captured verbatim.
    pub idempotency_by: Option<String>,
    pub policy: Option<String>,
    /// `handler "./..."` — required for canonical webhooks today.
    pub handler: Option<WebhookHandler>,
    /// `emits <event>` lines.
    pub emits: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookVerify {
    /// `hmac` — closed scheme catalog today.
    pub scheme: String,
    /// `sha256`, etc. — adapter-parsed algorithm token.
    pub algorithm: String,
    /// `secret env.<NAME>` — env binding for the shared secret.
    pub secret_env: Option<String>,
    /// `header "X-..."` — quoted header literal.
    pub header: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookHandler {
    pub path: String,
    pub returns: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub name: String,
    /// `channel email, in_app` — comma-split list.
    pub channels: Vec<String>,
    /// `recipient target.email` — path captured verbatim.
    pub recipient: String,
    /// `trigger event ...` or `trigger schedule "..."`.
    pub trigger: JobTrigger,
    /// `tenant_from payload.<axis>_id`.
    pub tenant_from: Option<String>,
    /// `idempotency by <path>`.
    pub idempotency_by: Option<String>,
    pub retry: Option<JobRetry>,
    /// `template "./outreach/welcome.mjml"`.
    pub template: String,
    pub policy: Option<String>,
    pub emits: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventGroup {
    /// `customer_*` glob pattern.
    pub pattern: String,
    /// `on Customer` — owning resource type.
    pub on_resource: Option<String>,
    /// `payload` child lines captured verbatim.
    pub payload: Vec<String>,
    /// `audit ...` line captured verbatim.
    pub audit: Option<String>,
    /// Concrete `event <name>` headers under this group, recorded as
    /// name strings. The full event bodies stay in the legacy lowering
    /// pipeline; this slot drives doctor's pattern-prefix rule.
    pub events: Vec<String>,
    pub span: Span,
}
