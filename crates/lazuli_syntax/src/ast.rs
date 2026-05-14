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
    /// Migrations bucket cycle Route C — `tenant_migration <name>`
    /// blocks. Mirrors `jobs` exactly: zero or more per feature.
    pub tenant_migrations: Vec<TenantMigration>,
    /// Phase L Tier 4a — `defaults` block. Optional; at most one per
    /// feature. Children captured: `tenancy <axis>`, `timestamps`,
    /// `policy_for <kinds>: <atom-list>`.
    pub defaults: Option<FeatureDefaults>,
    /// Phase L Tier 4b — `command <name>` blocks.
    pub commands: Vec<CommandDecl>,
    /// Phase L Tier 4b — `api <name>` blocks.
    pub apis: Vec<ApiDecl>,
    /// Phase L Tier 4c — `resource <Name>` blocks (authored inside
    /// `domain`).
    pub resources: Vec<ResourceDecl>,
    /// Phase L Tier 4d — `query.list` / `query.lookup` / `query.sql`
    /// declarations.
    pub queries: Vec<QueryDecl>,
    /// Phase L Tier 4d — `record <Name>` declarations (typed value
    /// records for projection outputs, distinct from resources).
    pub records: Vec<RecordDecl>,
    /// Phase L Tier 4 follow-up — `policies` block. At most one per
    /// feature. Lowered into `ir::Policies` via the analyzer.
    pub policies: Option<PoliciesDecl>,
    /// Phase L Tier 4 follow-up — `enum <Name>` declarations
    /// (authored inside `domain`).
    pub enums: Vec<EnumDeclAst>,
    /// i18n bucket cycle — `translation` block. At most one per
    /// feature. Lowered into `ir::Translation` via the analyzer.
    pub translation: Option<TranslationDecl>,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// Phase L Tier 4 follow-up — `policies` block surface AST.
//
// The `policies` block lives at indent 2 under the feature header. Its
// direct children are either:
//
//   * Named category atoms (indent 4): `create: @role.admin, @role.sales`.
//   * Per-resource field overrides (indent 4): `fields <Resource>` with
//     grandchild field names at indent 6 and `read:` / `write:` at indent 8.
//
// The IR shape (`ir::Policies` / `ir::PolicyCategory` / `ir::FieldPolicies`
// / `ir::FieldPolicy`) is mirrored 1:1 so lowering is structural.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoliciesDecl {
    pub categories: Vec<PolicyCategoryDecl>,
    pub fields: Vec<FieldPoliciesDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyCategoryDecl {
    pub name: String,
    /// Verbatim atom literals (`@role.admin`, `@scope.same_org`, ...).
    /// Atoms not prefixed with `@` are dropped silently — matches the
    /// retired `collect_policy_atoms` walker.
    pub atoms: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPoliciesDecl {
    /// `fields <Resource>` — captured verbatim (qualifier-free identifier
    /// in the fixture).
    pub resource: String,
    pub fields: Vec<FieldPolicyDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPolicyDecl {
    pub field: String,
    pub read: Option<Vec<String>>,
    pub write: Option<Vec<String>>,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// Phase L Tier 4 follow-up — `enum <Name>` declaration surface AST.
//
// Authored inside `domain` at indent 4. Variants at indent 6 are either
// bare identifiers (`free`) or `<name> = <value>` (storage value is `i64`
// or quoted string).
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDeclAst {
    pub name: String,
    pub variants: Vec<EnumVariantDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariantDecl {
    pub name: String,
    /// `None` when no `= <value>` is authored. `Some(Integer(_))` for
    /// `<name> = <number>`; `Some(String(_))` for `<name> = "<text>"`.
    pub storage: Option<EnumStorageValueDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EnumStorageValueDecl {
    Integer(i64),
    String(String),
}

/// i18n bucket cycle — surface AST for a `translation` block. The
/// analyzer copies this into `ir::Translation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationDecl {
    /// `catalog "<path>"` — required. Carries `<locale>` placeholder.
    pub catalog: String,
    pub keys: Vec<TranslationKeyDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKeyDecl {
    pub name: String,
    pub variants: Vec<TranslationVariantDecl>,
    pub plurals: Vec<TranslationPluralArmDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVariantDecl {
    /// BCP-47 tag, e.g. `pt-BR`.
    pub locale: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPluralArmDecl {
    /// CLDR plural category: `zero`, `one`, `two`, `few`, `many`, `other`.
    pub arm: String,
    pub variants: Vec<TranslationVariantDecl>,
}

/// i18n bucket cycle — surface AST for a `locale_negotiate` block.
/// Sits inside `api <name>` (per-endpoint override). The `app.runtime
/// unit api locale_negotiate` form is parsed by `app_manifest.rs` since
/// it lives on `app.lzi`. Both forms lower to `ir::LocaleNegotiate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleNegotiateDecl {
    pub source: Option<String>,
    pub strategy: Option<String>,
    pub fallback: Option<String>,
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

// =============================================================================
// Phase L Tier 4b — `command` / `api` declarations and the shared
// declarative spine (`target`, `let`, `creates`/`updates`/`deletes`).
//
// The AST mirrors the IR shape as closely as practical so lowering is
// structural. Expressions inside `target.<query>(args)` and
// assignment RHS are captured as raw text and re-parsed by the analyzer
// — the parser's job is to determine block boundaries and the indent
// contract, not to type-check expressions.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDecl {
    pub name: String,
    /// `previously migrated <old>` (one entry per `previously` line).
    pub previously: Vec<String>,
    /// `route <name>: <Type>` slots.
    pub route: Vec<CommandRouteSlot>,
    /// `input` block — `Empty` when absent, `Short` for `input <ref>`
    /// shorthand, `Typed` for typed-name lists.
    pub input: CommandInputDecl,
    /// `policy @policy.<name>` atom. Captured verbatim.
    pub policy: Option<String>,
    /// `rate_limit "<N per period per scope>"` literal (quotes stripped).
    pub rate_limit: Option<String>,
    /// `audit <subject>, <subject>, ...` line + optional `emit_to <group>` child.
    pub audit: Option<CommandAudit>,
    /// Cut A.9 `approval` block.
    pub approval: Option<CommandApproval>,
    /// `target query.<name>(args)` — at most one per command.
    pub target: Option<TargetExprDecl>,
    /// `let <name> = <expr>` bindings — order preserved.
    pub lets: Vec<LetBindingDecl>,
    /// `validate @validator.<name>(args)` lines. Doctor-only today; the
    /// surface keeps the verbatim invocation.
    pub validate: Vec<String>,
    /// `creates`/`updates`/`deletes` body. `None` for `returns`-only
    /// commands or commands with `handler` opt-outs (none in the
    /// fixture today).
    pub effect: Option<CommandEffectDecl>,
    /// `returns <TypeRef>` for pure request/response commands. Mutually
    /// exclusive with `effect`.
    pub returns: Option<String>,
    /// `handler "./..."` escape hatch — verbatim path literal. Mutually
    /// exclusive with the declarative body.
    pub handler: Option<JobHandler>,
    /// `emits <event>` lines with optional `from creates`/`from updates`
    /// suffix or assignment child block.
    pub emits: Vec<CommandEmit>,
    /// `invalidates query.<name>(args?)` references.
    pub invalidates: Vec<InvalidatesDecl>,
    /// `calls <slot>.<op>` references inside the command body.
    pub external_calls: Vec<JobExternalCall>,
    /// Phase L Tier 4 follow-up — `timeout "<duration>"` literal.
    /// Mirrors `Job.timeout`. Adapter parses; surface keeps verbatim.
    pub timeout: Option<String>,
    /// Phase L Tier 4 follow-up — `retry <count> [backoff <strategy>]`.
    /// Mirrors `Job.retry`.
    pub retry: Option<JobRetry>,
    /// Phase L Tier 4 follow-up — `idempotency by <field>[, ...]`.
    /// Mirrors `Job.idempotency_by`.
    pub idempotency_by: Option<String>,
    /// `tests` block — captured as raw lines until a typed test grammar
    /// lands. The body is the indented child list, trimmed.
    pub tests: Vec<String>,
    /// OpenAPI bucket cycle — `deprecated [since ".." replacement <ref> sunset ".."]`.
    pub deprecated: Option<CommandDeprecatedDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDeprecatedDecl {
    /// Authored `since "<version>"` — verbatim (semver, calendar, git-sha).
    pub since: Option<String>,
    /// Authored `replacement <ref>` — verbatim dotted ref or quoted URL.
    pub replacement: Option<String>,
    /// Authored `sunset "<YYYY-MM-DD>"` — verbatim ISO-8601 string.
    pub sunset: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRouteSlot {
    pub name: String,
    pub type_text: String,
    /// `from ctx.customer.id` default expression — verbatim text.
    pub from: Option<String>,
    pub span: Span,
}

/// `input` block: empty, short reference, or typed slot list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandInputDecl {
    /// `input` block absent.
    Empty,
    /// `input <name>` short form — one inline name. The analyzer resolves
    /// it against the command's local resource fields.
    Short(String),
    /// `input` block with typed children:
    ///
    ///   input
    ///     name: Text required
    ///     email: @semantic.Email @pii.contact required
    Typed(Vec<CommandInputSlot>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInputSlot {
    pub name: String,
    /// Raw type text including decorator chain — `@semantic.Email
    /// @pii.contact required` is parsed by the analyzer into `TypeRef`
    /// + modifiers.
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAudit {
    /// `actor`, `target.id`, `input.<field>`, etc.
    pub subjects: Vec<String>,
    /// `emit_to <event_group>` — optional child.
    pub emit_to: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandApproval {
    /// `required_when <predicate>` — verbatim predicate text.
    pub required_when: Option<String>,
    /// `by @role.<name>` or `by @actor.<name>` — single approver atom.
    pub by: String,
    /// `timeout "24h"` — duration literal (quotes stripped).
    pub timeout: Option<String>,
    /// `then deny | allow | escalate`.
    pub then: ApprovalThenDecl,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalThenDecl {
    Deny,
    Allow,
    Escalate,
}

/// `target query.<name>(args)` — at most one per command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetExprDecl {
    /// Qualified query reference. The parser keeps the dotted form
    /// (`customer.query.by_id` or `query.by_id`); the analyzer splits.
    pub query: String,
    /// `name: <expr>` pairs. The right-hand side is captured verbatim.
    pub args: Vec<TargetArgDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetArgDecl {
    pub name: String,
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetBindingDecl {
    pub name: String,
    /// Verbatim expression text. The analyzer re-parses against the
    /// canonical `Expr` AST.
    pub value: String,
    pub span: Span,
}

/// `creates X` / `updates X` / `deletes X` body. Carries the qualified
/// resource name + child assignments. `from_input` is true when the
/// command author writes `creates X from input` shorthand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEffectDecl {
    pub kind: CommandEffectKindDecl,
    /// Resource name. May be qualified (`customer.Customer`); the
    /// analyzer resolves against the local feature first.
    pub resource: String,
    pub from_input: bool,
    pub assignments: Vec<AssignmentDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandEffectKindDecl {
    Creates,
    Updates,
    Deletes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentDecl {
    pub field: String,
    /// Verbatim RHS text. The analyzer re-parses against `Expr`.
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEmit {
    /// `customer_created`, `customer_reassigned`, etc.
    pub name: String,
    /// `from creates` / `from updates` / `from deletes` suffix.
    pub from: Option<CommandEffectKindDecl>,
    /// Optional child `<key> = <expr>` lines (e.g.
    /// `emits customer_reassigned\n  to_owner_id = input.owner_id`).
    pub fields: Vec<AssignmentDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidatesDecl {
    /// Qualified query reference, e.g. `query.list` or
    /// `customer.query.by_id`.
    pub query: String,
    /// Named args, e.g. `id: route.id`.
    pub args: Vec<TargetArgDecl>,
    pub span: Span,
}

// =============================================================================
// Phase L Tier 4c — `resource <Name>` declaration.
//
// Resources live inside `domain` at indent 4. Children at indent 6 are
// fields, `has_many`, `previously`, `soft_delete`, `retention`,
// `validates`. The legacy lowering pipeline already produces an
// `ir::Resource` from the brace MVP; the canonical-indent slice now
// produces one too via `lower_resource_decl`.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDecl {
    pub name: String,
    /// `previously migrated <old>` (one entry per `previously` line).
    pub previously: Vec<String>,
    /// `tenancy <axis>` resource-local override.
    pub tenancy: Option<DefaultsTenancy>,
    /// Field declarations (`<name>: <Type> [modifiers...]`).
    pub fields: Vec<ResourceFieldDecl>,
    /// `has_many <name>: <Resource> [inverse <field>]` lines.
    pub has_many: Vec<ResourceHasMany>,
    /// `soft_delete` declared verbatim.
    pub soft_delete: bool,
    /// `timestamps` declared verbatim.
    pub timestamps: bool,
    /// `retention <duration> then <action>` policy.
    pub retention: Option<ResourceRetention>,
    /// `validates @validator.<name>` and `validates resource "./..."`
    /// declarations. Captured as raw text; the analyzer dispatches
    /// between resource-level and field-level validators.
    pub validates: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFieldDecl {
    pub name: String,
    /// Raw type text including decorator chain. The analyzer projects
    /// to `TypeRef` via `type_ref_from_text`.
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    pub unique: bool,
    /// `= <expr>` default value (verbatim).
    pub default: Option<String>,
    /// `derived from <expr>` computed-field expression (Phase L Tier 4c).
    pub derived_from: Option<String>,
    /// Child `previously migrated <old>` lines beneath the field.
    pub previously: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHasMany {
    pub name: String,
    /// Resource type reference, e.g. `CustomerNote`.
    pub type_text: String,
    /// `inverse <field>` clause — captured verbatim.
    pub inverse: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRetention {
    /// Duration literal, e.g. `7y`, `30d`. Captured verbatim.
    pub duration: String,
    /// `Anonymize | Delete | Archive` closed catalog.
    pub action: ResourceRetentionAction,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRetentionAction {
    Anonymize,
    Delete,
    Archive,
}

// =============================================================================
// Phase L Tier 4d — `query.list`, `query.lookup`, `query.sql`, and
// `record` declarations.
//
// Three query shapes with overlapping (but not identical) children.
// Records are simple typed-field bags (no constraints, no tenancy);
// they live under `domain` alongside resources and queries.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum QueryDecl {
    List(ListQueryDecl),
    Lookup(LookupQueryDecl),
    Sql(SqlQueryDecl),
}

impl QueryDecl {
    pub fn name(&self) -> &str {
        match self {
            QueryDecl::List(q) => &q.name,
            QueryDecl::Lookup(q) => &q.name,
            QueryDecl::Sql(q) => &q.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListQueryDecl {
    pub name: String,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// `modifier @query_modifier.<name>` reference.
    pub modifier: Option<String>,
    /// `params` block (typed slots).
    pub params: Vec<CommandInputSlot>,
    /// `scope override` flag — when set, the query opts out of feature
    /// default tenancy.
    pub scope_override: bool,
    /// `scope override\n  reason "..."` text.
    pub scope_reason: Option<String>,
    /// `scope override\n  deleted_at = nil` raw assignments captured
    /// for cross-check; not yet lowered to typed predicate.
    pub scope_assignments: Vec<String>,
    /// `scope` block (without `override`) — verbatim lines for now;
    /// the legacy lowering produces typed predicates.
    pub scope_lines: Vec<String>,
    /// `filters` block lines (`field when params.field`).
    pub filters: Vec<String>,
    /// `search params.<key> over <fields>` line with optional `mode contains`.
    pub search: Option<QuerySearch>,
    /// `cache` block — verbatim lines.
    pub cache: Vec<String>,
    /// `paginate <N>` page size.
    pub paginate: Option<u32>,
    /// `order <field> <asc|desc>` declarations.
    pub order: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupQueryDecl {
    pub name: String,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// `by <field>: <Type>` keys. Authored on the same line as the
    /// header in the fixture (`query.lookup by_id by id: ID`).
    pub keys: Vec<LookupKey>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupKey {
    pub name: String,
    pub type_text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlQueryDecl {
    pub name: String,
    pub policy: Option<String>,
    /// `params` block.
    pub params: Vec<CommandInputSlot>,
    /// `scope` block — verbatim lines.
    pub scope_lines: Vec<String>,
    /// `returns <Type>` declaration (required for SQL queries).
    pub returns: String,
    /// `sql "./queries/<name>.sql"` path literal.
    pub sql_path: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuerySearch {
    /// `params.search` source path.
    pub source: String,
    /// `over name, email` list.
    pub fields: Vec<String>,
    /// `mode contains` (closed catalog — `contains` only today).
    pub mode: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordDecl {
    pub name: String,
    pub fields: Vec<ResourceFieldDecl>,
    /// `discriminator` field marker name when authored. Cut A.6 used
    /// `record` types with a discriminator field for tagged-union
    /// agent outputs.
    pub discriminator_field: Option<String>,
    pub span: Span,
}

// -----------------------------------------------------------------------------
// `api <name>` declaration.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiDecl {
    pub name: String,
    /// `method GET|POST|PUT|PATCH|DELETE`. Captured as a typed enum.
    pub method: HttpMethod,
    /// `path "/api/customers/export"` — verbatim path literal.
    pub path: String,
    /// `output <TypeRef>` — captured as raw type text. The analyzer
    /// projects to `TypeRef`.
    pub output: String,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// `rate_limit "<N per period per scope>"`.
    pub rate_limit: Option<String>,
    /// `handler "./api/<name>.go"`.
    pub handler: Option<String>,
    /// i18n bucket cycle — per-api `locale_negotiate` block override.
    pub locale_negotiate: Option<LocaleNegotiateDecl>,
    /// `route <name>: <Type>` slots — path placeholders bound to typed
    /// values. Captured verbatim; codegen currently materializes them
    /// as args inferred from the path string.
    #[serde(default)]
    pub route: Vec<CommandRouteSlot>,
    /// `input` block — typed body fields. Captured verbatim; codegen
    /// does not lower these yet (handler @fn.<name> reads the request
    /// body itself).
    #[serde(default)]
    pub input: Option<CommandInputDecl>,
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

/// Body of a job. `Handler` is a path reference; `Declarative` is the
/// typed spine (Phase L Tier 4b lifted; previously a raw-line carve-out
/// in `JobDeclarativeRaw`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobBody {
    Handler(JobHandler),
    Declarative(JobDeclarativeTyped),
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

/// Phase L Tier 4b — declarative job body using the typed spine helpers
/// (`TargetExprDecl`, `LetBindingDecl`, `CommandEffectDecl`). Replaces
/// the Tier 3 `JobDeclarativeRaw` carve-out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDeclarativeTyped {
    pub target: Option<TargetExprDecl>,
    pub lets: Vec<LetBindingDecl>,
    pub effect: Option<CommandEffectDecl>,
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
    /// Webhooks expanded cycle — `payload from webhook_events.<name>`
    /// (verbatim suffix after `webhook_events.`). `None` when the
    /// inbound webhook does not declare a typed envelope yet.
    pub payload_from: Option<String>,
    /// Webhooks expanded cycle — `replay` block (short or long form).
    pub replay: Option<WebhookReplay>,
    /// Webhooks expanded cycle — `dlq` block (three closed variants).
    pub dlq: Option<WebhookDlq>,
    /// Webhooks expanded cycle — `retry <count> backoff <strategy>`
    /// inbound retry policy. Reuses the jobs-side `JobRetry` shape so
    /// codegen and doctor diagnostics stay single-pathed (Atrito #5
    /// of the canonical proposal).
    pub retry: Option<JobRetry>,
    pub span: Span,
}

/// Webhooks expanded cycle — surface form of `replay` on an inbound
/// webhook.
///
/// Short form: `replay allow within "24h"` (single line).
/// Long form: a `replay` header with nested `allow`/`deny` + `within
/// "..."` + optional `dedupe by <path>` children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookReplay {
    /// `allow` or `deny` — closed catalog enforced by the parser.
    pub mode: String,
    /// `within "<duration>"` — quoted duration verbatim.
    pub within: Option<String>,
    /// `dedupe by <path>` — path expression captured verbatim. `None`
    /// reuses the webhook's `idempotency by ...` path.
    pub dedupe_by: Option<String>,
    pub span: Span,
}

/// Webhooks expanded cycle — surface form of `dlq` on an inbound
/// webhook. The parser fails if more than one variant is authored on
/// the same webhook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookDlq {
    /// `dlq emit <event>` — publish a tombstone event after retry
    /// exhaustion.
    Emit { event: String, span: Span },
    /// `dlq handler "./path.go"` — adapter-side handler.
    Handler { path: String, span: Span },
    /// `dlq drop reason "..."` — explicit waiver. Mirrors `verify
    /// none reason "..."`.
    Drop { reason: String, span: Span },
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
    /// Notifications expanded bucket cycle — optional `digest` sub-block.
    /// Captured verbatim from the canonical-indent slice and lowered to
    /// `ir::NotificationDigest` in the analyzer.
    pub digest: Option<NotificationDigest>,
    /// Notifications expanded bucket cycle — optional `throttle` sub-block.
    /// Distinct from scalar `rate_limit`; lowered to
    /// `ir::NotificationThrottle` in the analyzer.
    pub throttle: Option<NotificationThrottle>,
    pub span: Span,
}

/// Notifications expanded bucket cycle — AST sidecar for the `digest`
/// sub-block. Fields are captured verbatim; closed-catalog validation
/// for `template_strategy` and the `every`/`max_size` shape lives in
/// the analyzer/doctor layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDigest {
    /// `every "<duration>"` — required.
    pub every: String,
    /// `group_by <path>` — optional.
    pub group_by: Option<String>,
    /// `max_size <N>` — optional.
    pub max_size: Option<u32>,
    /// `template_strategy <merge|append>` — optional.
    pub template_strategy: Option<String>,
    pub span: Span,
}

/// Notifications expanded bucket cycle — AST sidecar for the
/// `throttle` sub-block. Distinct keyword from scalar `rate_limit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationThrottle {
    /// `max_per "<duration>"` — required.
    pub max_per: String,
    /// `per_recipient` flag — bare child line.
    pub per_recipient: bool,
    /// `per_channel` flag — bare child line.
    pub per_channel: bool,
    /// `burst <N>` — optional.
    pub burst: Option<u32>,
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

/// Migrations bucket cycle Route C — `tenant_migration <name>` AST
/// surface. Mirrors `Job`'s spine subset (no body styles, no `emits`,
/// no `policy`): a tenant migration is by design pure schema work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantMigration {
    pub name: String,
    /// `target tenants <axis>` — required.
    pub target_axis: String,
    /// `idempotency by <path>` — mandatory per `TM-IDEMP-001`; stored
    /// as `Option<String>` so the parser surfaces the absence as an
    /// IR-level diagnostic rather than a parse error (matches `Job`).
    pub idempotency_by: Option<String>,
    /// `retry <count> backoff <strategy>` — optional.
    pub retry: Option<JobRetry>,
    /// `timeout "<duration>"` — optional adapter-parsed literal.
    pub timeout: Option<String>,
    /// `handler "<path>"` — required path to the Go handler.
    pub handler: String,
    pub span: Span,
}

// =============================================================================
// L0 #2 — `design.lzi` declaration AST.
//
// Parser shape: every value is preserved as raw text (hex strings, rem/px
// literals, font stacks, cubic-bezier strings, weight integers as text)
// so the analyzer can apply lowering-time validation (hex regex check,
// shadow single-layer check, extends rejection, integer parsing for z).
// The IR mirror in `lazuli_ir::Design` is the typed surface.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDeclAst {
    pub name: String,
    /// `extends <name>` — captured if present (lowering rejects in v0).
    pub extends: Option<String>,
    pub colors: Vec<ColorTokenAst>,
    pub typography: TypographyAst,
    pub spaces: Vec<ScaleTokenAst>,
    pub radii: Vec<ScaleTokenAst>,
    pub shadows: Vec<ShadowTokenAst>,
    pub motion: MotionAst,
    pub breakpoints: Vec<ScaleTokenAst>,
    pub z_indices: Vec<ZTokenAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorTokenAst {
    pub name: String,
    pub states: Vec<ColorStateAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorStateAst {
    /// One of `base | hover | active | foreground`. The analyzer maps
    /// to `ir::ColorStateKind`; unknown names raise a lowering error.
    pub kind: String,
    /// Hex literal verbatim, e.g. `"#7c3aed"`.
    pub value: String,
    /// Optional `dark <hex>` suffix, verbatim.
    pub dark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TypographyAst {
    pub families: Vec<FamilyTokenAst>,
    pub scale: Vec<TextScaleTokenAst>,
    pub weights: Vec<WeightTokenAst>,
    pub tracking: Vec<TrackingTokenAst>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextScaleTokenAst {
    pub name: String,
    pub size: String,
    pub line_height: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightTokenAst {
    pub name: String,
    /// Weight literal as text (lowering parses to u16).
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MotionAst {
    pub durations: Vec<ScaleTokenAst>,
    pub easings: Vec<EasingTokenAst>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EasingTokenAst {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZTokenAst {
    pub name: String,
    /// Integer literal as text (lowering parses to i32).
    pub value: String,
}
