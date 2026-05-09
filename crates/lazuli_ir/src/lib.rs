//! Lazuli intermediate representation.
//!
//! Shape governance lives in `docs/ir-abi.md`. This crate exposes types only;
//! it has no public mutator. The single producer is `lazuli_analyzer::lower_document`.
//! All consumers (codegens, planner, LSP, MCP, CLI) read this data and never
//! write back. Re-authoring means rewriting `.lzi`.
//!
//! Phase 1a foundation: `Module` / `Feature` / `Resource` / `Field` (with
//! `TypeRef` enum), `EnumDecl`, `Command` (with `Effect`), `Query` (List /
//! Lookup / Sql), and a minimal `Predicate` AST. Workflows, rules, events,
//! surfaces, jobs, webhooks, auth, escape routes, and extension contracts are
//! reserved for later phases.

use serde::{Deserialize, Serialize};

/// Schema version for the IR JSON ABI. See `docs/ir-abi.md`.
pub const LZIR_SCHEMA: &str = "0.1.0";

/// Span back-reference into the source AST. Debug-only; not part of the
/// published JSON ABI. Consumers must opt in via `--with-spans`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanRef {
    pub start: usize,
    pub end: usize,
}

/// A module is the IR root. It groups one or more features that flowed
/// through the same compilation. There is no `app` concept in canonical
/// Lazuli; the legacy `app NAME` header is lowered into a synthetic feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    pub features: Vec<Feature>,
}

/// A feature is the unit of product capability authored in one `.lzi` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_goals: Vec<NonGoal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<String>,
    pub defaults: Defaults,
    pub uses: Vec<String>,
    pub enums: Vec<EnumDecl>,
    pub resources: Vec<Resource>,
    pub events: Vec<Event>,
    pub rules: Vec<Rule>,
    pub commands: Vec<Command>,
    pub queries: Vec<Query>,
    pub workflows: Vec<Workflow>,
    pub jobs: Vec<Job>,
    pub webhooks: Vec<Webhook>,
    pub surfaces: Vec<Surface>,
    pub extensions: Vec<Extension>,
    pub escape_routes: Vec<EscapeRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    /// Authored storage value. `None` means the codegen picks per target;
    /// derived storage values do not enter the IR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_value: Option<StorageValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum StorageValue {
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    /// Tenancy axis: `tenancy org`, `tenancy team`, or `tenancy none` (opt-out).
    /// `None` means inherit from feature `defaults`. After lowering's derived
    /// pass this should be resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenancy: Option<Tenancy>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub soft_delete: bool,
    /// `None` means inherit from feature `defaults`. `Some(true)` = explicit
    /// `timestamps`, `Some(false)` = explicit `no_timestamps` opt-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<bool>,
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    /// Resource-level inline validator: `validate "./domain/validate_row.go"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate: Option<PathRef>,
    /// Field-level inline validators: `validates <field> "./hooks/validate_tier.go"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validates: Vec<FieldValidation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub type_ref: TypeRef,
    pub required: bool,
    pub unique: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of type references. Strings are forbidden; the analyzer
/// decides which variant a syntactic type name resolves to. Unrecognised
/// names become `TypeRef::Unresolved` so downstream consumers can surface a
/// targeted diagnostic without crashing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum TypeRef {
    Builtin(BuiltinType),
    UserDefined(QualifiedName),
    EnumRef(QualifiedName),
    Many(Box<TypeRef>),
    Unresolved(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltinType {
    Id,
    Text,
    Boolean,
    Integer,
    Decimal,
    Date,
    DateTime,
    Json,
    SemanticEmail,
    SemanticMoney,
    CapSecret,
    CapFile,
}

/// Qualified name for a feature-scoped or local symbol. `feature` is `None`
/// for local references; cross-feature references carry the feature id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifiedName {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DefaultValue {
    String(String),
    Integer(i64),
    Boolean(bool),
    EnumLiteral(EnumLiteral),
    Nil,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumLiteral {
    /// `None` when the literal is unqualified and the type comes from context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<QualifiedName>,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub kind: CommandKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<RouteSlot>,
    pub input: CommandInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lets: Vec<LetBinding>,
    pub effect: CommandEffect,
    pub policy: PolicyRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandKind {
    Create,
    Update,
    Delete,
    Returns,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteSlot {
    pub name: String,
    pub type_ref: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandInput {
    /// Short list — every entry maps 1:1 to a field on the command's local
    /// `creates`/`updates` resource.
    Short(Vec<String>),
    /// Typed block — explicit name/type pairs.
    Typed(Vec<TypedSlot>),
    /// Empty inputs (`delete` commands often have none).
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedSlot {
    pub name: String,
    pub type_ref: TypeRef,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetExpr {
    pub query: QualifiedName,
    pub args: Vec<NamedArg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedArg {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandEffect {
    Creates(CreateEffect),
    Updates(UpdateEffect),
    Deletes(DeleteEffect),
    /// Pure request/response command — declares `returns` instead of an effect.
    Returns(ReturnsEffect),
    /// No effect declared yet (legacy lowering path).
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEffect {
    pub resource: QualifiedName,
    /// True when the command body uses `creates X from input`.
    pub from_input: bool,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateEffect {
    pub resource: QualifiedName,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteEffect {
    pub resource: QualifiedName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnsEffect {
    pub return_type: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub field: String,
    pub value: Expr,
}

/// Policy reference. `Local` = feature-local policy category. `Atom` = closed
/// `@role.*`/`@scope.*`/`@actor.*` namespace. `External` = `<feature>.<name>`.
/// `Unresolved` covers legacy strings until full lowering lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PolicyRef {
    Local(String),
    Atom(String),
    External { feature: String, name: String },
    Unresolved(String),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Query {
    List(ListQuery),
    Lookup(LookupQuery),
    Sql(SqlQuery),
}

impl Query {
    pub fn name(&self) -> &str {
        match self {
            Query::List(q) => &q.name,
            Query::Lookup(q) => &q.name,
            Query::Sql(q) => &q.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<OrderBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    pub keys: Vec<KeyClause>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    pub returns: TypeRef,
    pub sql_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub predicate: Predicate,
    /// `Some(param_name)` for guarded `when params.X` filters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyClause {
    pub path: Path,
    pub equals: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderBy {
    pub field: String,
    pub direction: OrderDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderDir {
    Asc,
    Desc,
}

/// Closed predicate sublanguage. See `docs/canonical-semantics.md` "Predicate
/// Expressions" for the ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Predicate {
    Comparison {
        left: Expr,
        op: CompareOp,
        right: Expr,
    },
    Has {
        collection: Expr,
        element: Expr,
    },
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Expr {
    Path(Path),
    String(String),
    Integer(i64),
    Boolean(bool),
    Enum(EnumLiteral),
    Nil,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Path {
    pub segments: Vec<String>,
}

impl Path {
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

// =============================================================================
// Phase 1b — events, rules, workflows, surfaces
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub kind: EventKind,
    pub payload: Vec<EventField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// Standard domain event published into the feature reaction graph.
    Domain,
    /// `event.trace` — intentionally not part of the reaction graph; for logs,
    /// audit streams, and external observers.
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventField {
    pub name: String,
    pub type_ref: TypeRef,
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Author's prose title: `rule "archived customers cannot be reassigned"`.
    pub title: String,
    pub denies: OperationRef,
    pub when: Predicate,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRef {
    pub resource: QualifiedName,
    pub op_name: String,
    pub kind: OperationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    Command,
    Transition,
    /// Resolution deferred to the analyzer; default for legacy lowering.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub on: FieldRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_emits: Vec<String>,
    pub transitions: Vec<Transition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRef {
    pub resource: QualifiedName,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub from: String,
    pub to: String,
    /// `requires <category>` raises the policy bar for this transition above
    /// the workflow default (e.g. `requires delete`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    /// Surface words joined by space: `surface web admin` -> `name = "web admin"`.
    pub name: String,
    pub views: Vec<View>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed view kinds. New kinds enter via minor bump (see `docs/ir-abi.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum View {
    Table(TableView),
    SidePanel(SidePanelView),
    Form(FormView),
    Custom(CustomView),
}

impl View {
    pub fn name(&self) -> &str {
        match self {
            View::Table(v) => &v.name,
            View::SidePanel(v) => &v.name,
            View::Form(v) => &v.name,
            View::Custom(v) => &v.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableView {
    pub name: String,
    pub source: SourceRef,
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensible_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidePanelView {
    pub name: String,
    pub source: SourceRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensible_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormView {
    pub name: String,
    pub submit: QualifiedName,
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomView {
    pub name: String,
    /// Authored type label: `SidePanel`, `KanbanBoard`, etc. Lazuli does not
    /// generate a renderer for these; they reach extension contracts.
    pub view_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub query: QualifiedName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<NamedArg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellBinding {
    pub field: String,
    /// `@client.<name>` reference resolved against the feature's `extensions`.
    pub renderer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockBinding {
    pub renderer: String,
}

// =============================================================================
// Phase 1c — feature defaults, resource enrichment, extensions, escape routes
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonGoal {
    /// Boundary key. Either a feature id (`customer_auth`) or
    /// `anti_pattern.<slug>` for non-feature design boundaries.
    pub key: String,
    pub description: String,
}

/// Feature-level `defaults` block. Resource-local declarations override these.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenancy: Option<Tenancy>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub timestamps: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
}

/// Tenancy axis. Non-tenant resources use `Tenancy::None` (the explicit
/// `tenancy none` opt-out); resources inheriting feature defaults carry
/// `Resource.tenancy = None` until the derived pass resolves them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "axis", content = "value")]
pub enum Tenancy {
    Org,
    Team,
    /// Custom axis identifier (`tenancy workspace`, etc.).
    Custom(String),
    /// Explicit `tenancy none` opt-out.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Constraint {
    Unique(UniqueConstraint),
    Index(IndexConstraint),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniqueConstraint {
    pub fields: Vec<String>,
    /// `unique email per org` -> `qualifier = Some("org")`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexConstraint {
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldValidation {
    pub field: String,
    pub path: PathRef,
}

/// An extension contract declared under `extensions` and resolved to a
/// filesystem implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extension {
    pub name: String,
    pub contract: ExtensionContract,
    pub resolved_path: PathRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of extension contracts. Adding a contract is a minor bump;
/// changing one is a major bump. See `docs/canonical-semantics.md`
/// "Extension Path Convention" for the full table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ExtensionContract {
    /// `client <name>: CellRenderer[X]`
    CellRenderer { type_arg: TypeRef },
    /// `client <name>: ViewBlock[X]` or single-use `block <name>: ViewBlock[X]`
    ViewBlock { type_arg: TypeRef },
    /// `client <name>: FormField[X]`
    FormField { type_arg: TypeRef },
    /// `hook <name>: Hook[X]`
    Hook { type_arg: TypeRef },
    /// `validator <name>: Validator[X]`
    Validator { type_arg: TypeRef },
    /// `fn <name>: Function[X, Y]`
    Function {
        input: TypeRef,
        output: TypeRef,
    },
    /// `query_modifier <name>: QueryModifier[X]`
    QueryModifier { type_arg: TypeRef },
    /// `adapter <name>: IntegrationAdapter[X]`
    IntegrationAdapter { type_arg: TypeRef },
}

/// Filesystem path with provenance. `Convention` paths are derived from the
/// extension name + contract kind via the table in `docs/canonical-semantics.md`;
/// `Authored` paths come from an explicit `at "..."` clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRef {
    pub path: String,
    pub source: PathSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathSource {
    Convention,
    Authored,
}

impl PathRef {
    pub fn convention(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: PathSource::Convention,
        }
    }

    pub fn authored(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: PathSource::Authored,
        }
    }
}

/// Pages Lazuli should know about but should not govern internally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeRoute {
    pub route: String,
    pub at: PathRef,
    pub policy: PolicyRef,
    /// Coarse tenant axis for the escape page. `None` = no tenant scope claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<Tenancy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

// =============================================================================
// Phase 1d — async work: jobs and webhooks
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    pub trigger: JobTrigger,
    /// Execution lane for queued workers. `None` runs the reactor inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    pub body: JobBody,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobTrigger {
    /// `trigger event customer.customer_archived` — feature-qualified or local.
    Event { event: QualifiedName },
    /// `trigger schedule "0 2 * * *"` — cron expression.
    Schedule { cron: String },
}

/// Derived operational kind for inspect output. Authoring never sets this;
/// the analyzer resolves `Schedule` -> Scheduled, event without queue ->
/// Reactor, event with queue -> QueuedWorker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobOperationalKind {
    Scheduled,
    Reactor,
    QueuedWorker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyKey {
    /// Path expression: `envelope.id`, `payload.batch_id`, `payload.external_id`.
    pub by: Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub count: u32,
    pub backoff: BackoffStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffStrategy {
    Fixed,
    Exponential,
}

/// A job has exactly one body style. Handler-backed jobs may still declare
/// `emits`; declarative bodies bind a target and apply one write effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum JobBody {
    Handler(JobHandler),
    Declarative(JobDeclarative),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHandler {
    pub path: PathRef,
    /// `handler "./..." returns Customer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDeclarative {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lets: Vec<LetBinding>,
    pub effect: CommandEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub name: String,
    /// Inbound HTTP path: `"/webhooks/stripe/invoice-paid"`.
    pub route: String,
    pub verify: PathRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    pub handler: PathRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}
