---
name: Lazuli Language Audit
description: How to audit Lazuli for friction, gaps, and missing primitives. Anchored to the full-capsule fixture and the design boundary between Lazuli, Drusa, and adapters.
---

# Lazuli Language Audit

Use this skill when you're proposing improvements to Lazuli — vocabulary
cleanup, doctor/LSP/inspect coverage, ergonomics, or new primitives.

## Anchor: cold-read the full-capsule fixture

Every audit starts here. Read it like an LLM that has never seen the
codebase:

```
examples/full-capsule/
├── app.lzi             # operational contract
├── registry.lzi        # env, capabilities, packs, integrations
├── workspace.lzi       # distributed-system contract
├── profiles.lzi        # environment overrides
├── full-capsule.lzi    # domain features (the canonical exercise)
├── full-capsule.lzx    # abstract experience
├── full-capsule.web.lzx
├── full-capsule.account.web.lzx
├── full-capsule.admin.web.lzx
├── full-capsule.public.web.lzx
├── full-capsule.sales.mobile.lzx
└── contracts/ai.lzi    # external contract
```

If something requires you to consult a doc to understand the intent
**of a single line** in those files, that's an audit finding. Note it.

## The audit dimensions

Walk these in order. Don't merge them.

### 1. Vocabulary & polysemy

Look for words that mean different things in different contexts. The
test: would an LLM trained only on this fixture infer the right
semantics from the name?

Known historical hot spots (already cleaned, but re-check):

- `route` — top-level URL route declaration vs. path/context locator
  slot inside view/command vs. workspace gateway path mapping. Now
  unified to `route <name>: <Type>` for slots and `route.<name>` for
  references. Look for new polysemy.
- `path` — URL string vs. file path. Acceptable, but reject any new
  use that's a parameter reference (`path.id`).
- `params` — query/API read arguments only. Reject use as path slot.
- `audience` — should appear inside surfaces and routes; reject if it
  starts proliferating elsewhere.
- `capability` (registry) vs. `agent` (LLM primitive) — kept separate
  to avoid collision. Don't unify.

For each finding, propose: rename, deprecation path, or merge — with a
concrete cost estimate (number of occurrences, breaking changes).

### 2. Doctor / LSP / inspect / highlighting / IR coverage

For every construct in the language, four artifacts must exist:

| Artifact | Question |
|---|---|
| **Doctor** | Does `lazuli doctor` cross-check this construct against neighbors? |
| **LSP** | Does the LSP emit a diagnostic when the construct is misused? |
| **Inspect** | Does `lazuli inspect --format=json` expose the construct with `origin`? |
| **Highlighting** | Does `editors/vscode/syntaxes/lazuli.tmLanguage.json` color it? |
| **IR** | Does `crates/lazuli_ir` carry the typed shape? |

Walk every construct in the fixture. Any "no" is an audit finding.

Quick coverage probe:
```
cargo run -q -p lazuli_cli -- inspect examples/full-capsule/<file> | jq keys
```

### 3. Ergonomics & token economy

For each repeated pattern in the fixture, ask:

- **Is this verbose?** Count tokens for one occurrence × number of
  occurrences. If >50 tokens of pure boilerplate, propose a sugar.
- **Is there exactly one canonical form?** Two ways to express the same
  thing is an LLM-confusion bug. Either delete one or document the
  rule for choosing.
- **Does the name match the verb?** `creates`, `updates`, `deletes`,
  `emits`, `invalidates`, `target`, `let`, `route`, `params`,
  `audit`, `derived`, `has_many`, `agent` should self-explain. New
  additions must pass the same bar.

### 4. Missing primitives

The cliff test: in a real product, what would an author write as a
freeform `handler "./..."` because no language primitive exists? Each
of those is a candidate primitive.

Already promoted (don't re-propose). Before listing any candidate as
"missing", grep the fixture for the construct name and verify it's truly
absent — the audit pipeline has hallucinated already-shipped primitives
when this list went stale.

- `audit` (commands/queries/jobs/webhooks)
- `derived from` (computed fields)
- `has_many` (collections)
- `agent` (LLM capabilities, with optional `temperature` / `max_tokens` /
  `top_p` / `seed` config siblings)
- `notification` (multi-channel: email, push, sms, in_app — replaces
  `job` + `handler "./..."` for outreach dispatch)
- `validates @validator.<name>` (typed reference; scope is encoded in the
  validator's `Validator[<scope>]` type under `extensions`. Legacy forms
  `validates field <name> @validator.<name>` and `validates resource
  @validator.<name>` warn because the scope keyword duplicates the typed
  declaration.)
- `previously migrated|alias <old>` as a child of the block it migrates,
  uniformly across fields, resources, commands, transitions, and other
  named blocks. Inline header forms still parse but warn.
- `invalidates query.<name>` / `invalidates query.*` (same-feature short
  form and wildcard, on top of fully qualified)
- `event.trace <name>` (audit/observability events outside the
  feature-to-feature reaction graph)
- `escape_route "<path>"` (controlled exit route with `at` / `policy` /
  `tenancy`)
- `emits <event> from creates|updates|deletes` (auto-derive event payload
  from the surrounding command effect's bindings)
- AI-first dimensions on `contract` operations: `output stream <Type>`,
  `retry <count> [backoff <strategy>]`, `idempotency by <field>...`, and
  `error <Name> status <code> expose <fields>` (with schema-defined fields,
  not the command-level `message|code|data` envelope)

Likely candidates worth exploring (not yet implemented). Each must declare
at least 2 use sites in the fixture; otherwise drop it.

- `feature_flag <name>` / `experiment <name>` (gated rollout contract)
- `analytics_event` / `metric` (typed product analytics)
- `import_pipeline` (CSV/external-source ingestion as primitive)
- `webhook_outbound` (declarative outgoing webhook with retry/dlq
  contract — distinct from `webhook` which is inbound)

Each candidate must declare: what feature in the fixture would use it,
what it removes (handler files? jobs? duplicate state?), and the
boundary against Drusa/adapters.

### 5. AI-first sanity check

For each construct, ask:

- Can an LLM author it cold given a 1-line spec?
- Can an LLM read it cold and explain what it does?
- Is there at least one negative case the LSP catches that an LLM
  could plausibly emit by mistake?

If the answer is "no" to any, the construct needs a tighter contract.

## Output shape

When the audit ends, emit a markdown table with three columns:

| Finding | Dimension | Cost / Value |
|---|---|---|

Sort by cost-adjusted value descending. For each finding, anchor with
`path:line` references. Distinguish:

- **Polish** (rename / cosmetic / typo) — small, low-risk.
- **Coverage gap** (LSP/doctor/inspect missing) — medium, mechanical.
- **Primitive** (new construct) — large, design-heavy.
- **Boundary violation** (Lazuli reaching into Drusa/adapter
  territory) — must be reverted, not deferred.

The user picks what ships. Don't auto-implement unless explicitly
asked.

## What NOT to do

- Don't propose `vocabulary.lzi` — the namespace catalog is enforced
  in code (`crates/lazuli_lsp/src/lib.rs:is_allowed_reference_namespace`)
  and documented in `docs/invariants.md`. Adding a separate doc is
  redundant.
- Don't propose `container.lzi` — DI mechanics are Drusa, not Lazuli.
- Don't propose making `workspace.lzi` mandatory — it's optional by
  design.
- Don't propose primitives whose only motivation is "other frameworks
  have it." The motivation must be a fixture pattern that currently
  falls through to a handler file.
