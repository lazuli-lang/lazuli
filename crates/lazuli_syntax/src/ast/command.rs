//! `command <name>` declaration AST + shared declarative spine
//! (Phase L Tier 4b).
//!
//! The command surface is the largest single contract in the language:
//! it weaves together input typing, policy, route slots, audit,
//! approval, the declarative effect (`creates`/`updates`/`deletes`),
//! event emission, query invalidation, external calls, retry/timeout,
//! write windows, and tests — all in one block.
//!
//! Authoring shape (canonical-indent, fixture-style):
//!
//! ```text
//! command create_customer
//!   route tenant_id: ID from ctx.tenant.id
//!   input
//!     name: Text required
//!     email: @semantic.Email @pii.contact required
//!   policy @policy.staff
//!     when_denied @translation.customer.policy_denied
//!   rate_limit "60 per minute per actor"
//!   audit actor, target.id
//!     emit_to customer.events
//!   approval
//!     required_when input.high_risk
//!     by @role.admin
//!     timeout "24h"
//!     then deny
//!   target query.by_id(id: input.id)
//!   let normalized_email = lower(input.email)
//!   validate @validator.unique_email(email: normalized_email)
//!   creates Customer
//!     email = normalized_email
//!     name = input.name
//!   emits customer_created
//!   invalidates query.list
//!   timeout "5s"
//!   retry 3 backoff exponential
//!   tests
//!     ...
//!   deprecated since "2026-04" replacement create_customer_v2
//! ```
//!
//! The declarative spine (`TargetExprDecl`, `LetBindingDecl`,
//! `CommandEffectDecl`, `AssignmentDecl`, `CommandEmit`,
//! `InvalidatesDecl`) is **shared with** Job (Phase L Tier 4b) and
//! re-exported through `super::*`.
//!
//! Expressions inside `target.<query>(args)`, `let <name> = <expr>`, and
//! assignment RHS are captured as raw text — the analyzer re-parses
//! them against the canonical `Expr` AST. The parser's only job here is
//! block boundaries + indent contract.

use serde::{Deserialize, Serialize};

use super::resource::is_false_bool;
use super::{
    FieldConstraintsDecl, JobExternalCall, JobHandler, JobRetry, PolicyExprAst,
    PublicContractDeclAst, RateLimitSpecAst, Span, TranslationKeyRefAst,
};

/// `command <name>` block — Phase L Tier 4b declarative command surface.
///
/// The widest contract in the language. Stitches together input typing,
/// policy, route slots, audit, approval, the declarative effect spine
/// (`creates`/`updates`/`deletes`), event emission, query invalidation,
/// external calls, retry/timeout, write windows, and tests in one
/// block. See module-level docs for the authoring shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `previously migrated <old>` (one entry per `previously` line).
    pub previously: Vec<String>,
    /// `route <name>: <Type>` slots.
    pub route: Vec<CommandRouteSlot>,
    /// `input` block — `Empty` when absent, `Short` for `input <ref>`
    /// shorthand, `Typed` for typed-name lists.
    pub input: CommandInputDecl,
    /// `policy @policy.<name>` atom. Captured verbatim.
    pub policy: Option<String>,
    /// RB.S6 — structured form of `policy <expr>` when the policy
    /// string includes `has_role` / `has_permission` / `authenticated`
    /// predicates or boolean combinators. Coexists with `policy` (raw)
    /// for back-compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// IR Error-Vocab (Cell PARSE-1) — optional `when_denied
    /// @translation.<key>` child at indent 6 under the `policy` line.
    /// Highest-precedence step of the resolution chain (§2.A). Lowered
    /// into `ir::Command.policy_when_denied`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRefAst>,
    /// `rate_limit "<N per period per scope>"` declarations on the
    /// command. Per `ir-rate-limit-env-aware` (cell 1) the parser
    /// accepts one default line + any number of `in <env_list>`
    /// qualified lines and folds them into a single
    /// `RateLimitSpecAst`. The single-line source shape stays
    /// 100% backward-compatible.
    pub rate_limit: Option<RateLimitSpecAst>,
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
    /// W4 GAP-REORDER-01 — `reorder <Resource> by <position_field>` body.
    /// A batch-reorder verb: the command accepts an ordered list of ids and
    /// emits a single batch UPDATE of the position column. Mutually
    /// exclusive with `effect` / `returns`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reorder: Option<CommandReorderDecl>,
    /// `handler "./..."` escape hatch — verbatim path literal. Mutually
    /// exclusive with the declarative body.
    pub handler: Option<JobHandler>,
    /// `emits <event>` lines with optional `from creates`/`from updates`
    /// suffix or assignment child block.
    pub emits: Vec<CommandEmit>,
    /// `triggers transition <name>[, <name>]` transition names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
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
    /// `write_window by <path> within <duration_or_ref>`.
    pub write_window: Option<CommandWriteWindow>,
    /// `tests` block — captured as raw lines until a typed test grammar
    /// lands. The body is the indented child list, trimmed.
    pub tests: Vec<String>,
    /// OpenAPI bucket cycle — `deprecated [since ".." replacement <ref> sunset ".."]`.
    pub deprecated: Option<CommandDeprecatedDecl>,
    pub span: Span,
}

/// W4 GAP-REORDER-01 — `reorder <Resource> by <position_field>` body on a
/// [`CommandDecl`]. The command takes an ordered list of row ids (or
/// `[{id, position}]`) and emits a single batch UPDATE of the position
/// column. The position field must be a declared `Integer` field on the
/// target resource (doctor `REORDER-POSITION-FIELD-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReorderDecl {
    /// Target resource name. May be qualified (`feature.Resource`); the
    /// analyzer resolves against the local feature first.
    pub resource: String,
    /// `by <field>` — the integer position column to batch-update.
    pub position_field: String,
    pub span: Span,
}

/// `write_window by <path> within <duration_or_ref>` on a [`CommandDecl`].
///
/// Idempotency / rate-shaping window keyed by `by` (e.g. `actor.id`)
/// over the given duration window. Doctor checks that `within` is a
/// duration literal or a known reference; surface keeps verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandWriteWindow {
    /// `by <path>` — keying expression.
    pub by: String,
    /// `within <duration_or_ref>` — window span.
    pub within: String,
    pub span: Span,
}

/// OpenAPI bucket — `deprecated [since "..." replacement <ref> sunset "..."]`
/// metadata authored on a [`CommandDecl`] / [`ApiDecl`].
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

/// One `route <name>: <Type>` placeholder inside a [`CommandDecl`] /
/// [`ApiDecl`] route slot list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRouteSlot {
    pub name: String,
    /// Type literal verbatim (`ID`, `Text`, `@semantic.X`, ...).
    pub type_text: String,
    /// `from ctx.customer.id` default expression — verbatim text.
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "is_plain_command_route_slot_kind")]
    pub kind: CommandRouteSlotKind,
    pub span: Span,
}

/// Closed catalog of route-slot token kinds (`plain` / `opaque_token` /
/// `signed_token`). Drives how the runtime decodes the URL placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRouteSlotKind {
    /// Untransformed identifier (e.g. `id: ID`).
    Plain,
    /// Opaque token issued by the framework; lookup decodes server-side.
    OpaqueToken,
    /// Signed token (HMAC-derived) carrying its own integrity envelope.
    SignedToken,
}

impl Default for CommandRouteSlotKind {
    fn default() -> Self {
        CommandRouteSlotKind::Plain
    }
}

fn is_plain_command_route_slot_kind(kind: &CommandRouteSlotKind) -> bool {
    matches!(kind, CommandRouteSlotKind::Plain)
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

/// One row inside a `CommandInputDecl::Typed` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInputSlot {
    pub name: String,
    /// Raw type text including decorator chain — `@semantic.Email
    /// @pii.contact required` is parsed by the analyzer into `TypeRef`
    /// + modifiers.
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    /// L0 #3 §10 — inline constraints (`min N`, `max N`, `pattern
    /// STRING`, `between A and B`, `length N`, `in [...]`). Parser
    /// captures them; the analyzer validates combination rules and
    /// lifts into `ir::FieldConstraints`.
    #[serde(default, skip_serializing_if = "FieldConstraintsDecl::is_empty")]
    pub constraints: FieldConstraintsDecl,
    pub span: Span,
}

/// `audit <subjects>` canonical block — declares which actor/target
/// data points the command's write-event records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAudit {
    /// `actor`, `target.id`, `input.<field>`, etc.
    pub subjects: Vec<String>,
    /// `emit_to <event_group>` — optional child.
    pub emit_to: Option<String>,
    /// `data_subject <field>` — optional child naming the affected
    /// resource field that identifies the data subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_subject: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_before: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_after: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_for: Option<String>,
    pub span: Span,
}

/// Cut A.9 `approval` block — declarative human approval gate on a
/// [`CommandDecl`].
///
/// W4 GAP-06 added the ordered-chain form
/// (`approval chain [@role.manager, @role.admin] sequential timeout 24h then
/// deny`). The single-approver `by` form lifts to a 1-element `chain` with
/// `by == chain[0]`; the `chain` form populates `chain` directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandApproval {
    /// `required_when <predicate>` — verbatim predicate text.
    pub required_when: Option<String>,
    /// `by @role.<name>` or `by @actor.<name>` — the first approver atom
    /// (always equals `chain[0]`).
    pub by: String,
    /// W4 GAP-06 — ordered approver chain. Single-approver forms produce a
    /// 1-element chain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
    /// W4 GAP-06 — `sequential` flag (chain order is enforced).
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub sequential: bool,
    /// `timeout "24h"` — duration literal (quotes stripped).
    pub timeout: Option<String>,
    /// `then deny | allow | escalate`.
    pub then: ApprovalThenDecl,
    pub span: Span,
}

/// Closed three-arm catalog for the `then` clause of [`CommandApproval`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalThenDecl {
    /// `then deny` — block the command on timeout / rejection.
    Deny,
    /// `then allow` — auto-allow on timeout (best-effort approval).
    Allow,
    /// `then escalate` — route to a wider approver pool.
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

/// One `<name>: <expr>` named-arg pair inside a [`TargetExprDecl`] or
/// [`InvalidatesDecl`] reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetArgDecl {
    pub name: String,
    /// Verbatim RHS expression. The analyzer re-parses against `Expr`.
    pub value: String,
    pub span: Span,
}

/// One `let <name> = <expr>` binding inside a [`CommandDecl`] / Job body.
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

/// Closed three-arm catalog for the `creates`/`updates`/`deletes` head
/// keyword of a [`CommandEffectDecl`]. Wire token is lowercase
/// (`"creates"` etc.) to match authoring shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandEffectKindDecl {
    /// `creates X` — inserts a new resource row.
    Creates,
    /// `updates X` — mutates an existing row matched by `target`.
    Updates,
    /// `deletes X` — removes the row matched by `target`.
    Deletes,
}

/// One `<field> = <expr>` row inside a [`CommandEffectDecl`] body or
/// child of [`CommandEmit`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentDecl {
    pub field: String,
    /// Verbatim RHS text. The analyzer re-parses against `Expr`.
    pub value: String,
    pub span: Span,
}

/// One `emits <event> [from <kind>]` row on a [`CommandDecl`] or Job.
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

/// One `invalidates query.<name>(args?)` reference on a [`CommandDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidatesDecl {
    /// Qualified query reference, e.g. `query.list` or
    /// `customer.query.by_id`.
    pub query: String,
    /// Named args, e.g. `id: route.id`.
    pub args: Vec<TargetArgDecl>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_route_slot_kind_default_is_plain() {
        assert_eq!(CommandRouteSlotKind::default(), CommandRouteSlotKind::Plain);
    }

    #[test]
    fn command_effect_kind_decl_serializes_lowercase() {
        assert_eq!(
            serde_json::to_value(CommandEffectKindDecl::Creates).unwrap(),
            serde_json::json!("creates")
        );
    }

    #[test]
    fn approval_then_decl_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(ApprovalThenDecl::Escalate).unwrap(),
            serde_json::json!("escalate")
        );
    }

    #[test]
    fn command_input_decl_serde_tagged_variants() {
        let v = serde_json::to_value(CommandInputDecl::Empty).unwrap();
        assert_eq!(v["kind"], "Empty");
        let v2 = serde_json::to_value(CommandInputDecl::Short("ref".into())).unwrap();
        assert_eq!(v2["kind"], "Short");
        assert_eq!(v2["value"], "ref");
    }
}
