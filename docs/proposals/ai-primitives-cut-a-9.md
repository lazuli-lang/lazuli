# Proposal: Cut A.9 — `approval` primitive in commands

**Status**: Draft proposal. Depends on Cut A
(`docs/proposals/ai-primitives-v0.md`) `Agent` IR + tools[] surface so
the write-tool guard extension is meaningful.

**Owner**: TBD. **Target version**: `LZI_LANG` minor bump after Cut A
and Cut A.7 ship.

## Motivation

Cut A's `agent_tool_write_unguarded_diagnostics` (proposal §A1)
mandates that an agent dispatching a write-effect tool must declare
`safety @validator.<name>`. The plan §5.4 Q-impl-4 deferred
`idempotency by` to Cut B (alongside `flow`), leaving `safety` as the
only write-tool guard in Cut A.

Real products need a third path: **human approval**. The pattern
appears in every regulated product (refunds, enterprise tier changes,
data deletions, irreversible state transitions) where the right
discipline is "execute under a human's sign-off" rather than "let a
validator pre-scrub the input". `safety` validates content;
`approval` gates execution.

The pressure surfaces wherever multi-agent flows want to dispatch a
write command without baking the approval into the agent's prompt or
the command's predicate. Today authors smuggle the approval through
the command's `policy @policy.<name>` lattice, which forces a coarse
binary "you can or can't" instead of a graduated "you can call this
under approval when condition X holds". That's a security smell —
real-world approval is conditional, time-bounded, and audited.

The fix is small: commands gain an optional `approval` block that
declares the predicate triggering approval, the role(s) authorised to
approve, the timeout, and the default action on timeout. The write-tool
guard extends to accept `approval` alongside `safety`, so agents
dispatching approval-gated commands are guarded transitively.

## Scope

- `approval` child of `command` with:
  - `required_when <predicate>` — closed predicate language; same
    shape as `rule`/`workflow transition` predicates. Empty
    predicate means "always required".
  - `by <role-list>` — one or more `@role.<name>` references.
  - `timeout <duration>` — string parsed by adapter
    (e.g. `"24h"`, `"30 minutes"`).
  - `then <deny | proceed>` — default action when timeout fires.
- `Command.approval: Option<ApprovalSpec>` IR field.
- Doctor diagnostics:
  - `approval_role_unresolved_diagnostics` (error) — `by @role.<x>`
    references a role not in the feature's policy atoms.
  - `approval_timeout_invalid_diagnostics` (error) — `timeout` is
    empty or syntactically malformed.
  - `approval_predicate_invalid_diagnostics` (error) — `required_when`
    body fails to parse against the closed predicate language.
- `agent_tool_write_unguarded_diagnostics` (extension) — accepts
  either `agent.safety` or `command.approval` on the called command
  as satisfying the write-tool guard.
- Inspect projection in `--expand=security` extends with per-command
  approval block; `--expand=summary` per-command gains an
  `approval: true` marker.
- LSP file-local `approval_contract_diagnostics` — required children
  validation, role-shape check.

## Promotion gate

Cut A.9 lands when **at least one pilot product has a command whose
real-world dispatch requires conditional human approval AND that
command is dispatched by an agent** (the second condition makes the
write-tool guard extension load-bearing). The first condition alone
is generic command discipline; the second is what makes it part of
the AI-first cut series.

The canonical fixture's `customer.command.archive` is a candidate to
exercise the surface — soft-deletes are typically approval-gated in
production CRMs — but the fixture-only requirement does not gate
landing. Wait for a pilot if the timing is loose.

## Syntax

```lazuli
command archive
  route id: ID
  policy @policy.delete
  rate_limit "10 per hour per user"
  approval
    required_when target.tier = enterprise
    by @role.admin
    timeout "24h"
    then deny
  deletes Customer
```

Empty predicate for always-required:

```lazuli
command reassign
  approval
    by @role.admin
    timeout "1h"
    then proceed
  updates Customer
```

`required_when` admits the same closed predicate language used in
`rule` bodies (Cut A canonical-semantics §Predicate Expressions). The
target reference is the command's resource row when the command
declares `target` semantics; otherwise it binds to `input`.

## Rules (normative)

- **Block shape**: `approval` opens an indented body. Required
  children: `by <role-list>`, `timeout <duration>`, `then <deny |
  proceed>`. Optional child: `required_when <predicate>` — omission
  means "always required".
- **Role references**: each entry in `by` is `@role.<name>` exactly.
  No bare strings, no `@scope.*` (approvers are roles, not scopes —
  scopes are who-can-see, roles are who-can-act). Doctor verifies
  every role exists in the policy atom catalog (`is_allowed_reference_namespace`
  enforces the prefix; doctor verifies the suffix resolves).
- **Timeout**: a quoted duration string accepted by the runtime's
  adapter. Doctor enforces non-empty and basic shape (digit + unit
  pattern); the adapter parses canonical form.
- **Then**: closed catalog `deny` or `proceed`. No other values; no
  custom-action escape hatch — approvals that need custom timeout
  handling delegate to a `job ... trigger event approval_expired`
  pattern (canonical observability path).
- **Predicate**: `required_when` admits any predicate the closed
  predicate language already supports. No new operators. No
  `contains`/`tools.calls` extensions (those are eval-only per Cut A
  proposal §A3).
- **Write-tool guard extension**: agent A's `tools customer.command.X`
  satisfies `agent_tool_write_unguarded_diagnostics` when *either*:
  1. the agent declares `safety @validator.<name>` (Cut A baseline), or
  2. `customer.command.X` declares an `approval` block.
  Condition (2) is the new path. The agent's own `safety` remains
  the gate for PII propagation (`agent_pii_unsafetied_warning`); it
  is not subsumed by approval.
- **Inheritance**: an agent dispatching multiple write tools where
  *some* have `approval` and *others* require `safety` still needs
  the agent-level `safety` for the unprotected ones. Doctor reports
  per-tool which guard satisfied it.

## Diagnostics

| Id | Severity | Pipeline | Source |
|---|---|---|---|
| `approval_role_unresolved_diagnostics` | error | doctor | A9 |
| `approval_timeout_invalid_diagnostics` | error | LSP | A9 |
| `approval_predicate_invalid_diagnostics` | error | analyzer/doctor | A9 |
| `approval_contract_diagnostics` | error | LSP | A9 |
| `agent_tool_write_unguarded_diagnostics` (extension) | error | doctor | A9 |

`approval_contract_diagnostics` is file-local: required children
present, no unknown children, `then` value in the closed catalog,
`by` non-empty.

`approval_role_unresolved_diagnostics` is cross-feature: roles may
come from any feature's `policies` block or from `app.lzi`'s
`policy_for` defaults. Doctor walks the package's policy atom set.

## Layer placement (language)

The `approval` block is **language**:

- It changes static analysis (the write-tool guard lattice).
- It changes security proof (a command's effective authorisation
  surface).
- It's checkable from existing IR fields plus a small additive
  surface — no runtime mechanics in source.
- It plugs into the existing `policy`/`@role.*` catalog without
  introducing new namespaces.

The runtime owns:
- Showing the approval UI / sending the approval message.
- Persisting pending approvals.
- Honoring the `timeout` + `then` decision.
- Emitting trace events when approvals are requested / granted /
  denied / expired.

Adapters own:
- The transport (in-app prompt, Slack, email, SMS).
- The persistence backing (Postgres table, durable queue).

This split matches `docs/capability-layering.md` exactly.

## IR delta

```rust
// crates/lazuli_ir/src/lib.rs — extending Command:

pub struct Command {
    // ...existing fields...
    pub approval: Option<ApprovalSpec>,
}

pub struct ApprovalSpec {
    /// `required_when <predicate>`. None means "always required".
    pub required_when: Option<Predicate>,
    /// `@role.<name>` references; one or more.
    pub by: Vec<QualifiedName>,
    /// `timeout "<duration>"`. Adapter parses; doctor validates
    /// non-empty + shape.
    pub timeout: String,
    /// `then <deny | proceed>`.
    pub then: ApprovalTimeoutAction,
    pub span_ref: Option<SpanRef>,
}

pub enum ApprovalTimeoutAction {
    Deny,
    Proceed,
}
```

`LZIR_SCHEMA`: minor bump (additive).
`LZI_LANG`: minor bump.

## Inspect deltas

- `--expand=summary` per-command gains `approval: bool` (presence
  marker; full shape under security).
- `--expand=security` per-command extends with the full block —
  predicate text, role list, timeout, then-action.

## Coordination with the runtime team

The runtime owns the approval UX. Lazuli ships the contract; Drusa
ships the dispatch + persistence + UX wiring; adapters ship the
transport (Slack/email/SMS). The language-side cut is independent.

## Acceptance criteria

- Cut A's `Agent` IR + Cut A's write-tool guard already shipped.
- `approval` block parses, lowers, and is honored.
- All five diagnostics implemented and tested.
- Agent dispatching a command with `approval` no longer triggers
  `agent_tool_write_unguarded_diagnostics` without `safety`.
- `examples/full-capsule/full-capsule.lzi` exercises `approval` on
  at least one write command (recommended:
  `customer.command.archive` with `target.tier = enterprise`
  predicate).
- `cargo run -q -p lazuli_cli -- check examples/full-capsule/full-capsule.lzi`,
  `lazuli doctor examples/full-capsule`, and
  `lazuli inspect --expand=security` all pass with the new surface
  visible.
- `docs/invariants.md` adds the `approval` invariant.
- `docs/quickref.md` (or `quickref-write.md`) adds a `## Approval`
  section.
- `docs/design-decisions.md` records: *`approval` is the third
  write-tool guard alongside `safety` and (Cut B) `idempotency by`.
  It is the only guard that introduces a runtime gating step;
  `safety` and `idempotency by` are pre-flight checks. The three are
  not subsets of each other.*

## Non-goals

- **Approval workflow chaining** (multiple sequential approvals).
  Belongs in Cut B's `flow` once that lands.
- **Conditional roles** (`by @role.admin when target.tier =
  enterprise`). Roles are static; conditional gating goes in
  `required_when`. Splitting the case across two blocks would
  duplicate logic.
- **Approver UI**. Adapter / runtime concern.
- **Approval audit trail format**. Implicit via the runtime's
  trace events (Cut A.8 territory); not a language primitive.

## Relation to `docs/design-decisions.md`

If Cut A.9 lands, add the following decision: *`approval` is the
third write-tool guard alongside `safety` and (Cut B) `idempotency
by`. It is the only guard that introduces a runtime gating step;
`safety` and `idempotency by` are pre-flight checks. The three are
not subsets of each other and each addresses a different threat
shape.*

## Changelog

- Initial draft. Cut A.9 motivation surfaced during the post-Cut A
  evaluation (the second-opinion analysis identified `approval` as
  a missing primitive; the boundary test confirmed it qualifies as
  language territory).
