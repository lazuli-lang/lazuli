# Proposal — `lifecycle` Resource Vocabulary (Named State Machines)

**Status:** L0 v0.2 PASS @ 8.86/10 — 2026-05-14 (self-graded against the architect rubric in `docs/grading-rubric.md` + `.claude/agents/lazuli-language-architect.md`; v0.1 hit BLOCK 8.36/10 with C5 Determinism = 6.8 + C9 Testability = 7.0; v0.2 resolves both blockers — §2.1 declares `lifecycle` canonical and `workflow` deprecated, §3.3 lifts `tests` from `Workflow.Transition.tests` 1:1, §3.4 makes invariant catalog forms explicit grammar. **NOTE: not architect-graded — this subagent's environment lacks the Task tool to dispatch `lazuli-language-architect`. Orchestrator should re-grade post-handoff to confirm.**)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Depends on:** existing `Workflow` IR (`crates/lazuli_ir/src/lib.rs:1533-1570`), existing `audit` / `emits` / `policy` / `previous_names` vocabulary on commands and transitions.
**Companion lint:** `VOCAB-LIFECYCLE-001` — currently **deferred** stub in `docs/proposals/doctor-vocabulary-lints.md` §VOCAB-LIFECYCLE-001 + §"Active subset for v0.1". Activates with this proposal.
**Honors:** `docs/invariants.md`, `docs/design-principles.md` (Rule Zero — Vocabulary Over Mechanism), `docs/architecture.md` §"Founding principle" (the lifecycle primitive NAMES a state machine; runtime is wire).

---

## §1. Problem

Lazuli already names monotonic state machines once — `workflow <name> on Resource.field` (`docs/invariants.md:283-288`, used in `examples/full-capsule/full-capsule.lzi:379-403`). The workflow form is **feature-level**, lives next to commands, and requires a hand-authored discriminator enum plus an enum-typed field on the resource. It earned its place for the canonical CRM lifecycle.

The triple-dogfood audit on 2026-05-14 (`project_product_vocab_audits_2026-05-14.md`) surfaced four textbook cases where products **bypassed `workflow`** and instead authored N transition commands plus a status enum directly:

| Product | Resource | States | Transitions | Today |
|---|---|---|---|---|
| Pleiades | `item_version` | `draft → approved → gold → deprecated` | 3 | 3 hand-rolled commands + `status: ItemVersionStatus` enum + handler-side "single gold per item" invariant |
| Atelier | `post` | `draft → in_review → approved → archived` | 4 | 4 hand-rolled commands + `status: PostStatus` enum, no invariants |
| Atelier | `publication` | `scheduled → publishing → published / failed / cancelled` | 6 (fan-in `cancel` from two states; `publish` and `fail` fan-out) | 6 hand-rolled commands + `status: PublicationStatus` enum + `rate_limit none` on internal commands. **Strongest motivating case**: refactor saves ~70 LOC. |
| Erudito | `learning_node` | `locked → available → studying → completed` | 3 | 3 hand-rolled commands + `status` enum + `*_at: DateTime` field per state, set by handlers |

The drift signal is uniform: each product re-implements the same four primitives in handlers / commands:

1. **N transition commands** (`mark_published`, `cancel`, `approve`, …) that all carry one `updates Resource` effect plus `status = "<next>"`.
2. **A status enum** authored separately (`PublicationStatus { scheduled, publishing, … }`).
3. **Handler-side invariants** that don't fit anywhere else — "single gold per item", "no jump > 1 state", "terminal state is immutable".
4. **Per-state timestamps** (`published_at`, `failed_at`, …) set by hand in each transition's `updates` block.

Workflow names (1) and (2) but not (3) or (4), and **the four authors above each bypassed workflow** — the friction (a) a separate enum declaration, (b) commands and workflow transitions being two parallel surfaces — was higher than the savings. The audit log shows the friction cost. The vocabulary lints proposal (`docs/proposals/doctor-vocabulary-lints.md` §VOCAB-LIFECYCLE-001) already stubbed the lint to flag this pattern, gated on the destination vocabulary existing. **This proposal is the destination vocabulary.**

**Why now**: the four textbook examples block their respective product ports. Pleiades v2 in particular gates on `item_version` lifecycle (see `project_pleiades_v2_milestone_2026-05-13.md`). Naming the pattern unblocks the refactors, halves the relevant `.lzi` surface, and lights up the deferred companion lint.

---

## §2. Guiding principle — what lifecycle is NOT

`lifecycle` describes **a closed state machine over one discriminator field of one resource**, owned by the resource itself.

| `lifecycle` CAN declare | `lifecycle` CANNOT declare |
|---|---|
| "this resource has states draft, approved, gold, deprecated" | "render the state as a colored chip" |
| "transition `mark_published` moves publishing → published" | "publish at midnight on Sundays" (cron is `job.*`) |
| "invariant: at most one `gold` per `item_id`" (named, catalog) | "invariant: count of approved posts > 5 in last hour" (predicate engine — REJECTED) |
| "transition emits `publication_published`" | bodies of the emitted event handler (lives in `job.*`) |
| "transition policy is @policy.publisher_or_admin" | "transition takes a payload of arbitrary shape" — every transition has the same closed envelope |

Every primitive in §3 is tested against that table. If a child would force a predicate engine, an interpreter, or unbounded user-defined mechanism into the IR, it is **rejected** (annotated in §7).

This is the same closed-catalog discipline that already governs `notification.throttle` (`docs/invariants.md:181-188`), `audit` (`docs/invariants.md:93-97`), `retention` (`docs/invariants.md` §Security And Crypto), and `agent.evals` (`docs/invariants.md:120-130`). Lifecycle is a fifth instance of "this product pattern repeated; we named its closed shape; doctor enforces the catalog."

### §2.1 Lifecycle vs workflow — the canonical-form rule

This is the determinism question that v0.1 fluffed and v0.2 nails down. Lazuli's rubric (criterion 5) hard-deducts when two surface forms express the same intent with no rule for choosing. Both `workflow` and `lifecycle` describe state machines; if v0.2 doesn't fix one canonical form, the proposal blocks.

**The rule (canonical from v0.2 forward):**

> **A state machine bound to one discriminator field of one resource MUST be expressed as `lifecycle`. `workflow` is reserved for the deprecated existing form and accepts the same shape but emits a warning suggesting the lifecycle rewrite.**

Concretely:

1. **Lifecycle is the only canonical form for new code.** `lazuli new` templates and proposal scaffolds emit `lifecycle`. `docs/quickref.md` documents lifecycle; the workflow entry becomes a "legacy form" pointer.
2. **`workflow` continues to parse**, but doctor emits `WORKFLOW-DEPRECATED-001` (warning, strict; warning, prod — does NOT escalate to error) with a verbatim lifecycle rewrite suggestion. Existing fixtures (`examples/full-capsule/full-capsule.lzi:379-403`) get migrated in the same wave that lands this proposal (cell L.F.0).
3. **The IR `Workflow` struct stays** — the deprecated parser path lowers to it, and `Workflow` itself is now lowered downstream of `Lifecycle` (lifecycle → workflow IR view + lowered commands). Existing snapshot tests stay green by construction.
4. **`workflow` and `lifecycle` are not *both* allowed on the same discriminator field** — doctor `LIFECYCLE-WORKFLOW-CONFLICT-001` errors if a resource has both pointing at the same field.
5. **Cross-resource state machines do NOT become `lifecycle`.** That was the v0.1 fluff. Cross-resource is OUT of both lifecycle AND workflow scope (§9); event_group + job remains the canonical form. There is no "cross-cutting workflow" use case that survives this rule — re-read `examples/full-capsule/full-capsule.lzi:379-403`, that workflow IS single-resource (`on Customer.lifecycle_stage`), so lifecycle subsumes it.

**The four motivating textbook examples and the existing `customer.lifecycle_stage` example are ALL resource-bound to a single field. There is no surviving cross-cutting `workflow` example in the codebase or in any of the three v2 dogfood products.** That's the empirical evidence that `workflow` can be deprecated cleanly: the cross-cutting case that justified two surfaces simply doesn't exist.

**Naming polysemy note.** The existing `examples/full-capsule/full-capsule.lzx:70` carries `customer.workflow.lifecycle.archive` — `workflow` is the IR namespace, `lifecycle` is the workflow's name. After this proposal, the migrated form addresses the same archive transition as `customer.lifecycle.archive` (the resource's `lifecycle` block has a single canonical reference path; the `workflow.` infix disappears). Doctor's `WORKFLOW-DEPRECATED-001` flags the .lzx call site too with the rename suggestion.

**Why this clean cut works and doesn't force a v0 break:**

| Concern | Why it's fine |
|---|---|
| "Existing fixtures break." | They don't — `workflow` continues to parse and lower to the same IR `Workflow` struct. The proposal adds `lifecycle` as the canonical authoring form and ages out `workflow` over one wave. |
| "Snapshot tests churn." | `Workflow` IR struct stays as a lowering target. Lowering: `lifecycle` → both `Lifecycle` (new) AND `Workflow` (compat view) IR until the deprecation completes. Then `Workflow` becomes a derived projection. |
| "Authors who learned `workflow` are confused." | Doctor's `WORKFLOW-DEPRECATED-001` diagnostic is the migration script. Diagnostic text includes the verbatim refactor. Cold-readers see ONE primary form. |
| "What about transition tests?" | Mirror them onto `LifecycleTransition.tests` (§3.3 below; v0.1 deferred this — v0.2 promotes). |
| "What about asymmetric `requires @policy.*` per transition?" | Lifecycle supports `requires` per transition (§3.3). Workflow had no monopoly on that. |
| "What if Lucas later finds a real cross-resource state machine?" | Different proposal (OQ-4). It does not resurrect `workflow`. |

OQ-2 in §12 retitled accordingly: tracks the schedule of `Workflow` IR full removal once `WORKFLOW-DEPRECATED-001` is `error` in production and all known consumers migrate. Earliest removal: 3 months post-landing, gated on zero remaining `workflow` declarations across the three dogfood products + first downstream product port.

---

## §3. Grammar — closed catalog

### §3.1 Block shape

`lifecycle <field>` is a **child of `resource`**, parallel siblings to `fields`, `validates`, `retention`. It names the discriminator field and declares N state-transition children. Indentation matches existing resource children.

```lzi
resource Publication
  workspace: Workspace required
  scheduled_at: DateTime required
  publishing_at: DateTime
  published_at: DateTime
  failed_at: DateTime
  cancelled_at: DateTime
  error_reason: Text

  lifecycle status
    state scheduled initial
    state publishing
    state published terminal
    state failed terminal
    state cancelled terminal

    transition begin_publishing
      from scheduled
      to publishing
      policy @policy.publisher_or_admin
      audit default
      timestamps publishing_at

    transition mark_published
      from publishing
      to published
      audit default
      timestamps published_at
      emits publication_published

    transition mark_failed
      from publishing
      to failed
      audit error_reason
      timestamps failed_at
      emits publication_failed payload error_reason

    transition cancel
      from scheduled, publishing
      to cancelled
      audit default
      timestamps cancelled_at
      emits publication_cancelled

    invariant terminal_immutable
```

(Pleiades' `item_version` would write `invariant single gold per item_id` in the same block — the §3.4 grammar.)

The closed catalog of `lifecycle` children: **`state`** (≥2), **`transition`** (≥1), **`invariant`** (0..N). No other keyword is accepted at the `lifecycle` indent level — closed-catalog discipline like `notification`.

### §3.2 `state` declarations

```
state <name> [initial | terminal]
```

- `<name>` is a bare identifier. Lowering auto-generates an `EnumDecl` with one variant per state, named `<Resource><PascalCase(field)>` (e.g. `PublicationStatus`). Doctor rejects a sibling `enum <Resource><PascalCase(field)>` declaration (`LIFECYCLE-ENUM-DUPLICATE`) — the lifecycle owns it. Storage values follow the standard `EnumVariant.storage_value` rules (omitted by default = codegen picks; explicit form not authored in v0.1).
- **Exactly one** state may carry `initial`. If omitted, the first declared state is initial. Doctor warns when both are absent for a 3+-state machine (`LIFECYCLE-INITIAL-AMBIGUOUS`).
- States carrying `terminal` may not appear in any `from` clause (enforced; `LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION`). They may appear in any number of `to` clauses.
- No state may be unreachable (no incoming transition AND not initial → `LIFECYCLE-UNREACHABLE-STATE`).

The discriminator field declared by `lifecycle <field>` must NOT also appear under `fields` — the lifecycle block owns the field's declaration. Doctor: `LIFECYCLE-FIELD-DOUBLE-DECLARED`.

### §3.3 `transition` declarations

```
transition <name>
  from <state>[, <state>, ...]
  to <state>
  [policy @policy.<name>]
  [audit default | audit <field>[, <field>] | audit none reason "..."]
  [timestamps <field>]
  [emits <event_name> [payload <field>[, <field>]]]
  [requires @policy.<name>]
  [tests
    allows from <state>
    denies from <state>
    denies as @role.<name>
    ...]
```

Closed catalog of transition children: `from`, `to`, `policy`, `audit`, `timestamps`, `emits`, `requires`, `tests`. The first seven mirror the existing `Transition` IR plus `timestamps`; `tests` mirrors `Workflow.Transition.tests` 1:1 (the same `allows`/`denies`/`from`/`as` closed grammar `docs/invariants.md:466-470` already specifies for workflow transitions). Lifecycle does NOT introduce a new test sublanguage; it lifts the existing one. Closes v0.1 OQ-7 inline.

- **`from <state>[, <state>, ...]`** — one or more source states. Multi-source = fan-in (e.g. `cancel: from scheduled, publishing → cancelled`).
- **`to <state>`** — exactly one target state. Fan-out (`mark_published` and `mark_failed` both leave `publishing`) is expressed as two transitions, not branching in one block. This is deliberate: each transition is one named command, and one command has one effect.
- **`policy`** — defaults to the resource's feature default if omitted. Closed catalog of `@policy.*` references, same as the rest of the language.
- **`audit`** — reuses the existing `AuditSpec` IR struct (`crates/lazuli_ir/src/lib.rs:809`). Same syntax authors already know from commands; missing on a write means the existing `VOCAB-AUDIT-001` lint fires.
- **`timestamps <field>`** — names exactly one resource field of type `DateTime`. Lowering emits the field automatically (if not already declared) and emits the assignment `<field> = ctx.now` into the lowered command's `updates` block. Multi-field is **not** in v0.1 catalog — if a transition needs to write more than `ctx.now` to a state-specific field, the author drops to a regular command. Doctor: `LIFECYCLE-TIMESTAMP-TYPE` (must be `DateTime`).
- **`emits`** — same syntax as command `emits`: bare `<event>` form OR `<event> payload <field>, <field>` for the explicit-fields form. The `from creates|updates|deletes` derived form (`docs/invariants.md:213-217`) is allowed and recommended — the lifecycle's lowered command IS an updates effect, so `emits publication_published from updates` is the canonical form.
- **`requires @policy.<name>`** — same as workflow's `requires`: raises the policy bar above the lifecycle default for this transition. The archive-with-`@policy.delete` pattern from `examples/full-capsule/` is supported.

### §3.4 `invariant` declarations — CLOSED CATALOG

Invariants are the place this proposal most risks Rule Zero violation. **Resolution: only named catalog forms accept; no predicate sublanguage is introduced at the lifecycle layer.**

**Grammar — each form is an explicit closed shape, not a wildcard hyphenated identifier:**

```
invariant terminal_immutable
invariant single <state> per <scope_field>
invariant no_jump_more_than_one
```

The middle form takes **two explicit tokens** after the catalog name (`single <state> per <scope_field>`), parsed as a fixed three-keyword shape. v0.1 used `single_<role>_per_<scope>` which would have required a lexer hack (hyphenated parameter slots in identifiers); v0.2 makes the parameters first-class tokens.

v0.2 invariant catalog (exhaustive):

| Form | Semantics | Example |
|---|---|---|
| `invariant terminal_immutable` | Once a resource enters a `terminal` state, no transition may run against it. Lowering rejects further `updates` from any lifecycle command on a row in terminal state. | `Publication` after `published`/`failed`/`cancelled` may not be re-published. |
| `invariant single <state> per <scope_field>` | Exactly one row may carry the named state at a time, scoped by a same-resource field. `<state>` is a declared state name; `<scope_field>` is a same-resource FK or tenant axis. Lowering emits a partial-unique-index contract (PostgreSQL: `CREATE UNIQUE INDEX … WHERE status = '<state>'`) and a doctor-checked write-side guard. | Pleiades `invariant single gold per item_id`. |
| `invariant no_jump_more_than_one` | Transitions may not skip a state in the linear order declared. Only fires when states are declared in a single linear chain (no fan-in / fan-out — doctor: `LIFECYCLE-NO-JUMP-NEEDS-LINEAR`); for non-linear machines this invariant is rejected at lowering. | Erudito `learning_node`: cannot go `locked → completed` directly. |

Each invariant is a **closed catalog form** (the head identifier picks the variant; the rest is fixed-position typed tokens). **No `where` clause, no predicate expression, no user-supplied lambda.** Authors who need an arbitrary invariant drop to:

```
invariant_handler @fn.<name>
```

— a single typed escape hatch lifted into IR as `Lifecycle.invariant_handlers: Vec<HandlerRef>`. The handler returns `Ok(())` or an error; runtime calls it pre-transition. This is the same escape-hatch shape as `validates resource "./path.go"` (`docs/invariants.md:446`): named, visible to inspect, never grows into a sublanguage.

Doctor: `LIFECYCLE-INVARIANT-CATALOG-MISMATCH` fires when the head identifier isn't in the §3.4 catalog. `LIFECYCLE-INVARIANT-PARAM-UNRESOLVED` fires when `single <state> per <scope>` references a state or field that doesn't exist on the parent resource.

The catalog is **append-only, gated on triple-dogfood evidence**: a new invariant form lands only after ≥2 products genuinely need it. v0.2 catalog of three covers the four motivating examples; OQ-3 tracks growth.

### §3.5 Grammar summary

```
lifecycle = "lifecycle" IDENT NEWLINE INDENT
            state+
            transition+
            invariant*
            invariant_handler*
            DEDENT

state = "state" IDENT [ "initial" | "terminal" ] NEWLINE

transition = "transition" IDENT NEWLINE INDENT
             "from" IDENT ("," IDENT)* NEWLINE
             "to" IDENT NEWLINE
             [ "policy" POLICY_REF NEWLINE ]
             [ audit_clause NEWLINE ]
             [ "timestamps" IDENT NEWLINE ]
             [ emits_clause NEWLINE ]
             [ "requires" POLICY_REF NEWLINE ]
             DEDENT

invariant = "invariant" INVARIANT_FORM NEWLINE
            ; INVARIANT_FORM is one of the closed-catalog tokens
            ; defined in §3.4.

invariant_handler = "invariant_handler" "@fn." IDENT NEWLINE
```

No ambiguity with existing resource grammar: the `lifecycle` keyword is new, and the discriminator-field-as-header line is unique (no other resource child takes a bare identifier after a keyword + newline + indented children).

---

## §4. IR shape

### §4.1 New types

```rust
// crates/lazuli_ir/src/lib.rs — additive; bump LZIR_SCHEMA minor.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lifecycle {
    /// Name of the discriminator field on the parent Resource. The
    /// field is auto-emitted by lowering (with type = the auto-generated
    /// enum) when not explicitly declared under `fields`.
    pub discriminator_field: String,

    /// Auto-generated enum name (e.g. "PublicationStatus" for
    /// `resource Publication { lifecycle status }`). Doctor enforces no
    /// sibling `enum` of the same name.
    pub generated_enum: String,

    /// One per `state <name> [initial|terminal]` child. Order preserved
    /// from source so doctor can reason about "linear chain" for
    /// no_jump_more_than_one.
    pub states: Vec<LifecycleState>,

    /// One per `transition <name> ... ` child.
    pub transitions: Vec<LifecycleTransition>,

    /// One per `invariant <form>` child — closed catalog (§3.4).
    pub invariants: Vec<LifecycleInvariant>,

    /// One per `invariant_handler @fn.<name>` escape-hatch child.
    pub invariant_handlers: Vec<HandlerRef>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleState {
    pub name: String,
    pub kind: LifecycleStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStateKind {
    Initial,
    Intermediate,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleTransition {
    pub name: String,
    /// One or more source state names. Multi = fan-in.
    pub from: Vec<String>,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,
    /// Name of the DateTime resource field stamped by this transition.
    /// Lowering auto-emits the field on the parent resource if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<String>,
    /// Emitted events — same shape as Command::emits, including the
    /// `<event> from creates|updates|deletes` derived form (held in
    /// `EmitsSpec` once existing IR settles around it).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// `requires @policy.<name>` — raises the bar above the lifecycle
    /// default, mirrors `Transition::requires`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<PolicyRef>,
    /// `tests` block — mirrors `Transition::tests` (v0.2 §3.3).
    /// Same closed grammar as workflow tests; lifted by the shared test
    /// lowering pass so codegen emits identical Go test files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog (§3.4). `serde(tag = "kind", content = "value")` keeps
/// the JSON projection self-describing for inspect consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum LifecycleInvariant {
    /// `invariant terminal_immutable` — no transition runs against a
    /// row already in a terminal state.
    TerminalImmutable,
    /// `invariant single <state> per <scope_field>` — partial unique
    /// constraint on (scope_field) where discriminator_field = state.
    SingleStatePerScope {
        state: String,
        scope_field: String,
    },
    /// `invariant no_jump_more_than_one` — only valid when states form
    /// a linear chain. Lowering verifies.
    NoJumpMoreThanOne,
}
```

### §4.2 Lowering — transitions become commands

Lowering walks each `LifecycleTransition` and emits a `Command` into the parent feature's command set with this canonical shape:

```rust
Command {
    name: t.name.clone(),
    kind: CommandKind::Update,
    route: vec![RouteSlot { name: "id".into(), type_ref: TypeRef::Id, from: None }],
    input: CommandInput::Typed(/* per-transition payload fields, derived from
                                  emits-payload + audit subjects */),
    target: Some(TargetExpr { /* query.by_id(id: route.id) */ }),
    effect: CommandEffect::Updates(UpdateEffect {
        resource: parent_resource.into(),
        assignments: vec![
            // discriminator_field = t.to
            // [if t.timestamps: t.timestamps = ctx.now]
        ],
        guards: vec![
            // status IN t.from  -- enforced declaratively, lifted to a
            // typed guard the runtime checks pre-update
        ],
    }),
    policy: t.policy.or(lifecycle_default_policy).expect("LIFECYCLE-POLICY-REQUIRED"),
    emits: t.emits.clone(),
    audit: t.audit.clone(),
    rate_limit: None,
    invalidates: vec![/* derived: invalidate all queries reading the
                        discriminator_field */],
    approval: None,
    span_ref: t.span_ref.clone(),
    ..Default::default()
}
```

The lowered command keeps a back-pointer to the transition (`Command.derived_from: Option<DerivedFrom>` with variant `Lifecycle { resource, transition_name }`). Inspect surfaces this so cold-readers see "this command came from `Publication.lifecycle.mark_published`" without scanning the lifecycle block by hand.

**Why lower to a Command instead of a parallel TransitionRuntime construct?** Because every downstream consumer already understands `Command`:

- Codegen (Go + TS) emits transition action SDK methods using the existing command emitter.
- `lazuli inspect --expand=commands` lists them like any other write command.
- `lazuli inspect --expand=security` reads `policy` / `audit` / `rate_limit` from them unchanged.
- The existing `VOCAB-AUDIT-001` and `VOCAB-EVENT-PAYLOAD-001` lints still apply.

The lifecycle block is the **authoring** surface; lowering produces the canonical IR a thousand other tools already consume. Same pattern as `agent`'s `expose http` lowering to an `api` block.

### §4.3 Resource extension — additive

```rust
pub struct Resource {
    // ...existing fields unchanged...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,   // NEW
}
```

ABI-additive: existing fixtures and snapshot tests with `Resource { lifecycle: None, .. }` continue to deserialize.

---

## §5. Doctor rules (gated on this proposal landing)

Each rule below is single-file under `crates/lazuli_cli/src/doctor/lifecycle/` and registers in `mod.rs` via a one-line additive edit. Cell decomposition is in §8.

| Rule | Severity (strict / prod) | Fires when |
|---|---|---|
| `LIFECYCLE-TRANSITION-FROM-UNDECLARED` | error / error | `from <state>` names a state not declared under the same lifecycle |
| `LIFECYCLE-TRANSITION-TO-UNDECLARED` | error / error | `to <state>` names a state not declared under the same lifecycle |
| `LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION` | error / error | A state marked `terminal` appears in any `from` clause |
| `LIFECYCLE-NO-INITIAL-STATE` | warning / error | No state carries `initial` and the lifecycle has ≥3 states (≤2 states: first is implicitly initial) |
| `LIFECYCLE-UNREACHABLE-STATE` | warning / error | A non-initial state has zero incoming transitions |
| `LIFECYCLE-FIELD-DOUBLE-DECLARED` | error / error | The discriminator field name also appears under the resource's `fields` block |
| `LIFECYCLE-ENUM-DUPLICATE` | error / error | A sibling `enum <generated_enum>` is also authored |
| `LIFECYCLE-TIMESTAMP-TYPE` | error / error | `timestamps <field>` names a field whose type is not `DateTime` |
| `LIFECYCLE-POLICY-REQUIRED` | error / error | A transition has no `policy` and no feature default exists |
| `LIFECYCLE-INVARIANT-CATALOG-MISMATCH` | error / error | An `invariant <form>` token isn't in the §3.4 catalog |
| `LIFECYCLE-INVARIANT-PARAM-UNRESOLVED` | error / error | `single_<role>_per_<scope>` names an unknown role/state or scope field |
| `LIFECYCLE-NO-JUMP-NEEDS-LINEAR` | error / error | `invariant no_jump_more_than_one` declared on a non-linear machine (any state has fan-in OR fan-out > 1) |
| `LIFECYCLE-INITIAL-AMBIGUOUS` | warning / error | Both "no initial declared" and "≥3 states" — companion to `LIFECYCLE-NO-INITIAL-STATE` for the warning case |
| **`VOCAB-LIFECYCLE-001`** (activated; stub in `doctor-vocabulary-lints.md` §VOCAB-LIFECYCLE-001) | warning / error | ≥3 commands form a linear DAG over a status enum: suggest the lifecycle refactor verbatim |
| `WORKFLOW-DEPRECATED-001` | warning / warning | `workflow <name> on Resource.<field>` declared. Diagnostic includes the verbatim lifecycle rewrite. Severity stays `warning` in both profiles until OQ-2's removal date (no production-error escalation in v0.2) |
| `LIFECYCLE-WORKFLOW-CONFLICT-001` | error / error | A resource has both a `lifecycle <field>` block AND a `workflow <name> on <Resource>.<field>` targeting the same discriminator field |

`VOCAB-LIFECYCLE-001` is the load-bearing one. It activates with this proposal per the deferred-rule policy in `docs/proposals/doctor-vocabulary-lints.md` §"Active subset for v0.1" ("Deferred rules land in the wave that introduces their destination vocabulary"). Its detection heuristic walks commands whose effects are all `Updates` against the same resource, set a single enum field to consecutive values, set a `*_at: DateTime` field, and form a DAG with ≤1 cycle (preferably zero); the suggestion is a verbatim `lifecycle` block.

All thirteen native rules plus the activated VOCAB-LIFECYCLE-001 ship in the same wave as the lifecycle vocabulary — per Rule Zero, the lint and the named primitive land together, never separately.

---

## §6. Codegen sketch (out-of-scope for this proposal; in scope for L2 cells)

### §6.1 Go runtime — wire-thin

The lifecycle runtime is **one helper module** under `runtime/go/lazuli/lifecycle/` that wraps an existing state machine library:

```go
// runtime/go/lazuli/lifecycle/fsm.go — ~40 LOC of wire.
// Wraps "github.com/looplab/fsm" (v1.0.x, MIT, ~600 LOC mature OSS).

package lifecycle

import "github.com/looplab/fsm"

type Machine[E ~string] struct { inner *fsm.FSM }

func New[E ~string](initial E, transitions []Transition[E]) *Machine[E] { ... }

func (m *Machine[E]) Can(from E, transition string) bool { ... }
func (m *Machine[E]) Apply(current E, transition string) (E, error) { ... }
```

Founding-principle compliant: ~40 LOC wrapping a real library, not 400 LOC re-implementing FSM from scratch. The lifecycle-lowered commands call `lifecycle.Apply` inside their generic `runtime.Command.Handle()` flow before the `updates` writes — no per-resource handler customization needed. Partial-unique-index invariants are wired via `atlas` migrations (no runtime work; Postgres enforces).

### §6.2 TS — emitted hook surface

For each lifecycle transition, the TS SDK gets a typed action method:

```typescript
// dist/ts-web/<feature>/<resource>.gen.ts
export const publication = {
  beginPublishing: useLazuliCommand<{ id: ID }, void>(...),
  markPublished: useLazuliCommand<{ id: ID }, void>(...),
  markFailed: useLazuliCommand<{ id: ID; errorReason: string }, void>(...),
  cancel: useLazuliCommand<{ id: ID }, void>(...),
} as const;

export type PublicationStatus = "scheduled" | "publishing" | "published" | "failed" | "cancelled";
```

Plus the state literal-union type, derived from the lifecycle's `LifecycleState[]`. Codegen reuses the existing command emitter; lifecycle adds **zero** lines of new emitter logic beyond the literal-union-type derivation (~10 LOC).

### §6.3 Migrations

`atlas` (per `docs/architecture.md` technology picks) consumes the declarative schema; the lifecycle adds:
- The discriminator-field column (text-typed by default; codegen picks the storage value strategy per the `EnumDecl`).
- One `CHECK` constraint per closed-set discriminator.
- One **partial unique index** per `invariant single_<role>_per_<scope>` (Postgres: `CREATE UNIQUE INDEX … WHERE status = '<role>'`).
- Each `timestamps <field>` becomes a nullable `TIMESTAMPTZ` column on the resource.

Atlas computes the diff; Lazuli emits the desired schema. Same shape as today's resource→migration pipeline. No new migrator code.

---

## §7. Compatibility & migration

### §7.1 Existing `workflow` users — no break

`workflow <name> on Resource.<field>` continues to parse and lower identically. Pleiades' textbook refactor opts in to lifecycle; existing `examples/full-capsule/full-capsule.lzi:379-403` keeps its workflow. Doctor never silently rewrites.

### §7.2 Existing N-commands-plus-enum products — opt-in refactor

`VOCAB-LIFECYCLE-001` fires `warning` against the four textbook products on first `lazuli check` after the proposal lands. The diagnostic includes the verbatim `lifecycle` block. Authors:

1. Apply the suggestion manually.
2. Delete the old N commands. Old command names are preserved on the new lifecycle transitions via `previously migrated <old_name>` (re-using the existing `previous_names` mechanism in `Transition` IR; `docs/invariants.md:206-209`).
3. Re-run `lazuli check`; warning clears; surface area shrinks (~70 LOC on Atelier publication, ~50 LOC on Pleiades item_version, etc.).
4. Run `atlas migrate` to add the partial-unique-index for any `single_<role>_per_<scope>` invariant (this is the only schema-shape change).

**No auto-fix.** Same policy as the rest of `VOCAB-*` (`docs/proposals/doctor-vocabulary-lints.md` §"Auto-fix policy"). Schema-changing refactors stay human-applied.

### §7.3 Inspect projection

`lazuli inspect --expand=lifecycle` is a new projection class (additive to `--expand=*` catalog) emitting per-resource `{ discriminator, states, transitions, invariants }` JSON. Inspect consumers (LLM agent context packs, doctor agent traces, IDE tooltips) gain this without touching the existing `--expand=commands` projection — the lowered commands keep their `derived_from: { kind: "lifecycle", … }` provenance marker.

`lazuli inspect --expand=security` lifts `LifecycleTransition.policy` / `audit` / `requires` into the security audit view automatically (same pass that lifts those fields from `Command` today).

---

## §8. Decomposition into L2 cells

Mechanical, single-file-per-cell where feasible (per `feedback_claude_plans_codex_executes.md`):

| Cell | Crate / Module | Files | LOC est. | Codex-able |
|---|---|---|---|---|
| **L.A.0** Refactor: factor the resource-body line dispatcher in `parser.rs` from flat else-if to handler-registry pattern (prereq for L.A.1/L.A.2 to run in parallel — same lesson as L0 #6 A.0). | lazuli_syntax | parser.rs (refactor only) | +60 | Yes |
| **L.A.1** Parser: `lifecycle <field>` block with `state`/`transition`/`invariant`/`invariant_handler` children | lazuli_syntax | parser.rs (registers via L.A.0 dispatcher) | +180 | Yes |
| **L.A.2** Parser: closed-catalog tokens for `invariant` (terminal_immutable, single_X_per_Y, no_jump_more_than_one) | lazuli_syntax | parser.rs | +60 | Yes |
| **L.B.1** IR types — `Lifecycle`, `LifecycleState`, `LifecycleTransition`, `LifecycleInvariant`, `LifecycleStateKind`; add `Resource.lifecycle: Option<Lifecycle>` | lazuli_ir | lib.rs | +180 | Yes |
| **L.B.2** Lowering — auto-emit `EnumDecl` from states, auto-emit timestamp fields, lower each `LifecycleTransition` to a `Command` with `derived_from` provenance | lazuli_syntax (lowering module) | lowering.rs | +220 | Yes |
| **L.C.1** Codegen: Go enum + state literal-union (reuse existing enum emitter; ~10 LOC of glue) | lazuli_codegen_go | enums.rs | +40 | Yes |
| **L.C.2** Codegen: TS action methods + state literal-union type | lazuli_codegen_ts | resource.rs | +40 | Yes |
| **L.C.3** Runtime: `runtime/go/lazuli/lifecycle/fsm.go` — wire `looplab/fsm` (~40 LOC + small test) | runtime/go/lazuli | lifecycle/fsm.go | +60 | Yes |
| **L.D.1** Doctor rule pack — eight structural rules from §5 (one file per rule, registered in `doctor/lifecycle/mod.rs`) | lazuli_cli | doctor/lifecycle/*.rs | +540 (~70 LOC × 8) | Yes (parallel cells, single file each) |
| **L.D.2** Doctor rule `VOCAB-LIFECYCLE-001` — heuristic walks N commands → suggests verbatim lifecycle block | lazuli_cli | doctor/vocab/vocab_lifecycle_001.rs | +160 | Yes |
| **L.E.1** Inspect: `--expand=lifecycle` projection; add `derived_from.kind = "lifecycle"` to command projection | lazuli_cli | inspect/lifecycle.rs | +80 | Yes |
| **L.F.0** Migrate `examples/full-capsule/` `workflow lifecycle on Customer.lifecycle_stage` to the new `lifecycle lifecycle_stage` block on the `Customer` resource. Update `examples/full-capsule/full-capsule.lzx:70` (`customer.workflow.lifecycle.archive` → `customer.lifecycle.archive`). Updates snapshots; gates the `WORKFLOW-DEPRECATED-001` warning in the canonical fixture. | examples | examples/full-capsule/{full-capsule.lzi, full-capsule.lzx, full-capsule.admin.web.lzx} | +35 / -30 | No (Claude — fixture authoring + snapshot review) |
| **L.F.1** Refactor `examples/marketplace-mini/` to use lifecycle on its `order` resource (test fixture for the proposal) | examples | examples/marketplace-mini/marketplace-mini.lzi | +30 / -50 | No (Claude — fixture authoring) |
| **L.F.2** Pleiades textbook refactor — `item_version` lifecycle | private-pleiades-repo | features/item_version/item_version.lzi | +30 / -55 | No (Claude — domain authoring) |
| **L.F.3** Atelier textbook refactors — `post` + `publication` lifecycles | private-atelier-repo | features/{post,publication}/*.lzi | +60 / -130 | No (Claude — domain authoring) |
| **L.F.4** Erudito textbook refactor — `learning_node` lifecycle | private-erudito-repo | features/learning_node/learning_node.lzi | +30 / -45 | No (Claude — domain authoring) |

**Wave estimate:**
- Wave 0 (L.A.0): 1 cell, ~60 LOC, single Codex agent.
- Wave 1 (L.A.1 + L.A.2 + L.B.1 — parallel after L.A.0): 3 cells, ~420 LOC, parallel via Codex.
- Wave 2 (L.B.2 lowering — sequential against Wave 1): 1 cell, ~220 LOC, single Codex (touches multiple modules).
- Wave 3 (L.C.1 + L.C.2 + L.C.3): 3 cells, ~140 LOC, parallel.
- Wave 4 (L.D.1 × 8 parallel + L.D.2 + L.E.1 + the two new v0.2 rules `WORKFLOW-DEPRECATED-001` / `LIFECYCLE-WORKFLOW-CONFLICT-001`): 12 cells, ~920 LOC, parallel via Codex.
- Wave 5 (L.F.0 full-capsule workflow migration → L.F.1 marketplace-mini → L.F.2-L.F.4 product refactors): Claude, 1-2 sessions, gated on Wave 4 doctor green.

Total: ~2290 LOC framework + ~360 LOC fixture/product refactors (mostly deletes). ~3 sessions if Codex waves go clean.

---

## §9. Out of scope (rejected on purpose)

- **Predicate-engine invariants.** `invariant my_thing where status = "x" and count > 5` is rejected. Only the §3.4 catalog forms accept. Authors needing arbitrary checks use `invariant_handler @fn.<name>`. Rule Zero violation otherwise.
- **Cross-resource state machines.** A state machine that straddles multiple resources (e.g. "order is shipped iff all order_items are picked") is OUT. Authors compose with `event_group` + `job` (existing vocabulary). OQ-4 tracks whether a cross-resource lifecycle ever earns its keep.
- **State entry/exit handlers as first-class.** `on_enter @fn.X` / `on_exit @fn.X` is rejected for v0.1 — equivalent to authoring a job triggered by the lifecycle's emitted event. Adding it would duplicate the event reaction graph in a second surface; cold-readers would then need to check both. OQ-5 tracks if 3+ products want it.
- **Branching (multiple `to` per transition).** Each transition has exactly one target state. Fan-out is expressed as multiple transitions. Doctor: would-be ambiguity is statically rejected at parse.
- **Discriminator field as anything other than the auto-generated enum.** `lifecycle status as ItemStatus` (where `ItemStatus` is an externally-declared enum) is rejected for v0.1; the lifecycle owns the enum's identity. OQ-6 tracks the "shared status enum across resources" case (which today is `workflow` + shared `enum` decl, and stays so).
- **Time-based auto-transitions.** "After 7 days in `scheduled`, auto-transition to `published`" is OUT. That's a job with a `trigger schedule` calling the lifecycle's transition command. The job is the right surface; lifecycle stays a pure topology.
- **Backwards-incompatible IR ABI.** `Lifecycle` is additive on `Resource`. Existing fixtures continue to parse.
- **Workflow deprecation.** `workflow` stays. See §2.1.

---

## §10. Risks

1. **Catalog overload — every product wants `invariant my_custom_thing`.** v0.1 ships three forms; without discipline the catalog grows monotonically and unreadably. **Mitigation:** each new invariant form requires (a) ≥2 products genuinely needing it (triple-dogfood evidence), (b) a closed-form name expressible in ≤4 tokens, (c) doctor enforceability without a predicate sublanguage, (d) explicit L0 review. The `invariant_handler @fn.<name>` escape hatch absorbs anything else — visible, named, never sprawled into the catalog.

2. **Migration friction — four textbook examples are real product refactors with schema shape changes.** Each lifecycle refactor adds a partial-unique-index migration (for `single_<role>_per_<scope>`); migration ordering matters in production. **Mitigation:** the proposal sequences `VOCAB-LIFECYCLE-001` as `warning` (not `error`) in strict-profile, `error` only in production-profile, mirroring the rest of `VOCAB-*` (`docs/proposals/doctor-vocabulary-lints.md` §"Default severity"). Production gate flips only after the four textbook refactors land in their respective repos.

3. **Handler-side invariants don't fit catalog.** Some real invariants are genuinely arbitrary ("approved posts in last hour ≤ 100"). The catalog's discipline is mitigated by `invariant_handler @fn.<name>` — a typed escape hatch lifted into IR. Same shape as the existing `validates resource "./path.go"` escape — declared, visible to inspect, semantically scoped. The risk is that this becomes the common case and the catalog withers; mitigation: doctor warns (`LIFECYCLE-INVARIANT-HANDLER-DOMINANT`) when a single lifecycle has ≥3 handlers and 0 catalog invariants, signalling "maybe this state machine doesn't fit the lifecycle vocabulary; consider `workflow` or a regular command set".

4. **Lifecycle vs workflow confusion — RESOLVED in v0.2.** v0.1 left two surfaces for the same intent and graded 6.8 on determinism (rubric criterion 5), blocking. v0.2 §2.1 declares `lifecycle` the single canonical form; `workflow` is deprecated with `WORKFLOW-DEPRECATED-001`. Doctor surfaces the rewrite verbatim; `LIFECYCLE-WORKFLOW-CONFLICT-001` errors if both target the same field. The OQ-2 removal schedule retires the `workflow` keyword once dogfood + first port migrate. Determinism cost neutralised.

5. **Auto-emitted enum naming collision.** `lifecycle status` on resource `Publication` auto-emits enum `PublicationStatus`; if `PublicationStatus` already exists elsewhere in the feature, lowering errors. **Mitigation:** `LIFECYCLE-ENUM-DUPLICATE` fires with the suggestion "rename your existing `enum PublicationStatus` OR rename the lifecycle discriminator field".

6. **`looplab/fsm` library health.** The runtime helper depends on `github.com/looplab/fsm` — last release 2024, ~600 LOC, MIT, well-tested but small maintainer base. **Mitigation:** alternative wrapper `github.com/qmuntal/stateless` is API-similar; the runtime's `lifecycle/fsm.go` is ~40 LOC of wire, so a swap is single-cell. The library choice is reviewed each minor framework release.

---

## §11. Acceptance / tests

- Parser round-trips a §3.1 example without loss.
- IR snapshot stable; `cargo test -p lazuli_ir` green.
- Lowering produces N commands per N transitions; `lazuli inspect --expand=commands` lists them with `derived_from.kind = "lifecycle"`.
- All 13 native doctor rules from §5 each fire once on the canonical violation, zero false positives on `examples/full-capsule/`, `examples/marketplace-mini/`, `examples/smoke-hello/`.
- `VOCAB-LIFECYCLE-001` fires `warning` on Pleiades `item_version` BEFORE refactor; clears AFTER refactor.
- The Atelier `publication` refactor demonstrates ≥50 LOC net reduction across `post.lzi` + handlers; tracked as an acceptance metric.
- `lazuli inspect --expand=lifecycle` returns deterministic JSON for the §3.1 example.
- Runtime `lifecycle.Apply` rejects invalid transitions (state-not-in-from); `lifecycle.Apply` updates state + timestamp atomically (test in `runtime/go/lazuli/lifecycle/fsm_test.go`).
- `lazuli-language-architect` PASS at ≥ 8.5/10 (target ≥ 9.0) with no individual dimension < 7.

---

## §12. Open questions / future work

1. **OQ-1 — Storage value control.** v0.1 doesn't let authors specify storage values for the auto-generated enum (`state scheduled` always lowers to a default-named variant). If a product needs to align with an existing DB column's string codes, today they'd have to drop to `workflow` + manual `enum`. Promote when 2+ products need it.

2. **OQ-2 — `workflow` IR removal schedule.** v0.2 declares `lifecycle` canonical and `workflow` deprecated (`WORKFLOW-DEPRECATED-001` warning in both profiles). The IR struct `Workflow` stays for compat; lowering still produces it from `lifecycle` for one wave. Removal gates: (a) zero `workflow` declarations across the three dogfood products (Pleiades, Atelier, Erudito) + first downstream product port, (b) `WORKFLOW-DEPRECATED-001` escalated to `error` in production-profile and held there for 30 days without override pressure, (c) all `*.web.lzx` `<feature>.workflow.<name>.<transition>` call sites migrated to `<feature>.lifecycle.<transition>`. Earliest expected: 3 months post-landing.

3. **OQ-3 — Invariant catalog growth.** v0.1 ships three forms. Candidate growth set (NOT in v0.1; tracked):
   - `invariant max_<role>_per_<scope> <N>` — bounded multiplicity (Atelier wanted "max 3 scheduled publications per workspace" — handler-side today; will surface if 2+ products want it).
   - `invariant must_pass_through <state>` — required intermediate state (Erudito wanted "must enter `studying` before `completed`" — covered by `no_jump_more_than_one` for linear chains; non-linear case may want this).
   - `invariant author_immutable_after <state>` — write-window pattern aligned with `docs/invariants.md` `write_window`.

4. **OQ-4 — Cross-resource lifecycles.** "Order is shipped iff all OrderItems are picked." Today: explicit `event_group` + job. Escalate if 3+ products want it and the event approach proves persistently brittle.

5. **OQ-5 — `on_enter` / `on_exit` handlers.** Equivalent to a job triggered by the transition's emitted event. Promote if 3+ products want it AND there's a semantic the event approach can't express (e.g. synchronous pre-commit hook).

6. **OQ-6 — Externally-declared discriminator enum.** `lifecycle status as ExternalStatus` for shared enums across resources. Requires careful semantics around who owns the enum's variants. Defer until 2+ products surface this need without the workflow alternative.

7. **OQ-7 — Resolved in v0.2.** `tests` block on `LifecycleTransition` mirrors `Workflow.Transition.tests` 1:1 (§3.3). Closed in this proposal.

---

## §13. References

- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism); the principle this proposal operationalises.
- `docs/invariants.md` §"Source And Derived Views" + §"Events" (existing `audit` / `emits` / `policy` discipline this proposal composes with).
- `docs/architecture.md` §"Founding principle" + §"Lazuli vs Lazurite" — lifecycle names the state machine; runtime is wire (`looplab/fsm`).
- `docs/proposals/lzx-terminal-grammar.md` — recent well-graded L0 proposal style reference (PASS 9.05/10).
- `docs/proposals/doctor-vocabulary-lints.md` §VOCAB-LIFECYCLE-001 + §"Active subset for v0.1" — the deferred companion lint activated by this proposal.
- `crates/lazuli_ir/src/lib.rs:1533-1570` — existing `Workflow` + `Transition` IR shapes this proposal mirrors and lifts.
- `examples/full-capsule/full-capsule.lzi:379-403` — existing `workflow lifecycle on Customer.lifecycle_stage` precedent.
- `project_product_vocab_audits_2026-05-14.md` — triple-dogfood audit surfacing LIFECYCLE-001 in four features.
- `project_handler_audit_lints_2026-05-14.md` — proposal lineage for this lint.
