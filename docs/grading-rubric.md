# Lazuli Grading Rubric

**Status**: Normative reference. Used by `/lazuli-grade` and
`/lazuli-improve` slash commands, by the
`lazuli-language-architect` subagent, and by human reviewers
auditing proposals.

The rubric is biased on purpose: Lazuli's mission is **AI-first
authoring + human cold-readability**. Criteria that don't serve
those goals are absent. This document is the single source of
truth for the rubric. The agent definition at
`.claude/agents/lazuli-language-architect.md` and the slash
command at `.claude/commands/lazuli-grade.md` reference this file.

## Changelog

- **2026-05-27** — Reaffirmed Criterion 11 (Framework error message
  contract, 6%) per proposal
  `c:/Users/lucas/lazuli-ops/docs/proposals/grader-error-message-criterion.md`
  AND Criterion 4 Prisma-trap runbook + Vocab Governance Rules per
  proposal
  `c:/Users/lucas/lazuli-ops/docs/proposals/grader-vocab-governance.md`
  as a single co-shipped revision. Both proposals were already
  structurally landed (2026-05-18 and 2026-05-20 respectively) but
  carried dangling "Source proposal:" references and "internal review
  notes" placeholders from prior changelog edits; this revision restores
  the full proposal-path provenance and consolidates the two as a single
  co-cited revision in §"Notable changes." No weight redistribution; no
  criterion text changes; no probe changes. Composes cleanly with the
  2026-05-27 C13 / C12 / C8 / C8.5 hardening above (none of the touched
  runbook bodies overlap with C11 or C4). **Forward-only — no past PASS
  retroactively becomes BLOCK.**
- **2026-05-27** — Added Criterion 13 (Generated-runtime contract
  honesty, 6%) per proposal `grader-anti-theater-hardening.md`.
  Weights redistributed (C1 −1, C3 −1, C8 −1, C10 −1, C12 −2). Sum
  stays at 100%. AI-first ceiling (C2 + C8.5 + C10 + C11 =
  17 + 3 + 7 + 6 = 33%) unchanged in shape — C10's −1 stays inside
  the AI-first cluster, preserving the 35% ceiling discipline
  (cluster previously at 34%, now at 33%; ceiling never crossed).
  C12 extended with `spec_polarity` layer + Probes Q-E/Q-F +
  `TEST-STUB-ASSERTION-001` / `TEST-PINS-STUB-VOCAB-001` auto-BLOCK
  escalation. C8.5 runbook extended with migration-safety anchor
  citing `MIGRATION-IDEMPOTENT-CREATE-001`. C8 runbook extended
  with the operational schema evolution sub-anchor. New
  boundary-violation line: codegen-emitted contracts must not
  silently disagree with hand-authored Go (fires automatically when
  `HANDLER-SIGNATURE-MISMATCH-001` or `HANDLER-SQL-COLUMN-DRIFT-001`
  is emitted). **Forward-only hardening — no past PASS retroactively
  becomes BLOCK.** Triggered by the canonical pilot-A 2026-05-27
  incident (Google sign-in production break under green-doctor
  iron-hand). Cross-validation subsection added under §"How the
  rubric is enforced" mapping each of the five staged bug classes
  to the refined-rubric verdict.
- **2026-05-24** — Added Criterion 12 (Test discipline + per-layer
  coverage, 5%) per proposal `tdd-bdd-first-2026-05-23.md` Wave 6.
  The proposal text named the new criterion "Criterion 11" but slot
  11 was already taken by the framework error message contract; the
  slot was filled by the time Wave 6 landed, so the criterion ships
  as **Criterion 12** with the same semantic content. Weight 5%
  redistributed from C3 (Token efficiency −1) and C10 (AI-first
  readiness −1) so the AI-first cluster still bears the cost of
  test-culture enforcement. Anchors:
  - **pass** — `lazuli doctor --coverage` emits a coverage report;
    no per-layer block thresholds are violated under the active
    profile.
  - **pass-with-notes** — at least one warn-tier layer under the
    active profile; no block-tier breach; no `TEST-FIXTURE-LITERAL-001`
    error if Wave 1 has shipped.
  - **block** — any layer below `block_under` under the active
    profile, OR any `TEST-FIXTURE-LITERAL-001` error.
  Per-layer thresholds replace any single-number aggregate (the
  proposal's load-bearing point). Forward-only hardening — no past
  PASS retroactively becomes BLOCK.
- **2026-05-20** — Criterion 4 (Escape hatches) hardened with the
  Prisma-trap runbook (4 probes P-A..P-D); added §Vocab Governance
  Rules (RULE-VOCAB-01..04); added 3 new boundary-violation lines.
  Purely additive — no weight redistribution, no past PASS retroactive
  BLOCK. Triggered by the canonical pilot vocab-saturation analysis. Source
  proposal: `grader-vocab-governance.md`.
- **2026-05-18** — Added Criterion 11 (framework error message
  contract, 6%) per proposal `grader-error-message-criterion.md`;
  weights redistributed (C1 −1, C2 −1, C3 −1, C7 −1, C10 −2). Sum
  remains 100%. **Forward-only hardening — no past PASS retroactively
  becomes BLOCK.** Full provenance in §Versioning → "Notable changes."
- **2026-05-17** — Inserted Criterion 8.5 (Diagnostic identifier
  truthfulness, 3%); AI-first weight 14% → 11%. See §Versioning →
  "Notable changes" for the full record.

## How to use

This rubric exists to turn vibes into a number. Three classes of
user:

1. **Proposal authors** — predict your proposal's score before
   asking for a grade. The eight criteria in §Self-Assessment lay
   out what to check.
2. **Reviewers** (human or agent) — score the language, a
   proposal, or a fixture against §Criteria. Always anchor with
   `path:line`. Apply the gate at the end.
3. **`/lazuli-grade` orchestration** — the rubric drives the
   pipeline DAG; outputs follow §Output shape.

## Scope

The rubric grades:

- **The language as a whole** — typically against the canonical
  fixture `examples/full-capsule/` plus the docs that ship with
  it.
- **A proposal** — the proposed shape against the same criteria,
  scored on what the proposal would land if implemented.
- **A patch / cut** — a specific PR's effect on the criteria.
  Often only 3–4 criteria move; the others stay the same.

It does not grade:

- Runtime correctness (Lazuli Go / generated code).
- Performance.
- Test coverage of the *implementation*. The implementation
  pipeline is its own concern.

## Criteria

Sum of weights = 100%.

| # | Criterion | Weight | What you're measuring |
|---|---|---|---|
| 1 | Legibility (cold human read) | 10% | Can a senior dev read 1000+ lines of fixture top-to-bottom without backtracking or doc-lookup? |
| 2 | Semantic density for LLM | 17% | Are `@policy.*`, `@cap.*`, `@semantic.*`, `@actor.*`, `@pii.*`, `@key.*`, `@llm.*`, `@tool.*` namespaces tight, closed, and unambiguous? |
| 3 | Token efficiency | 7% | Is there gordura recorrente? Count tokens of repeated boilerplate × number of repetitions. |
| 4 | Escape hatches | 8% | Can authors drop to `handler "./..."`, `validates resource "./..."`, custom Go without polluting source? Are the hatches minimal and visible? |
| 5 | Determinism (one way to say each thing) | 10% | If the same intent has two surface forms with no rule for choosing, that's a deduction. |
| 6 | Composability | 8% | Do `extends @anchor.*`, `extensible_by`, `packs`, `has_many`, `event_group` combine cleanly? |
| 7 | Multi-target fit (Go/React/Expo) | 7% | Are surface projections (`.web.lzx` / `.mobile.lzx`) clean? Does any contract leak transport mechanics? |
| 8 | Operational coverage | 5% | Do `runtime`, `deploy`, `profiles`, `services`, `architecture` cover real production needs without becoming Kubernetes config? Includes the **operational schema evolution** sub-anchor — see §"Criterion 8 — Operational schema evolution sub-anchor (runbook)" below. |
| 8.5 | Diagnostic identifier truthfulness | 3% | For every diagnostic code named in a proposal's acceptance lists: does the code (a) exist in `crates/lazuli_cli/src/doctor/mod.rs` or `crates/lazuli_lsp/src/lib.rs`, or (b) explicitly appear under a `## New diagnostics` heading as net-new? Mechanical grep check. See §"How the rubric is enforced" for the runbook. |
| 9 | Declarative testability | 6% | Are `tests` blocks expressive enough for rules / transitions / anchors / commands without becoming a mock framework? |
| 10 | AI-first readiness | 7% | Does the language treat LLMs as first-class consumers (`agent`, namespaces, inspect contracts, doctor messages)? |
| 11 | Framework error message contract | 6% | Are framework-emitted runtime errors (anything that reaches the HTTP wire without passing through an authored `rule "..." message @translation.<key>` block) keyed by a translation identifier under `@translation.<key>` (or equivalent message-namespace identifier), negotiated against the active locale, and override-able by app or feature surface? Hardcoded English in `Message:` fields of `&Error{...}` constructors in the runtime is an automatic 0. See §"Criterion 11 — Framework error message contract (runbook)" below. |
| 12 | Test discipline (per-layer coverage + polarity) | 3% | Does `lazuli doctor --coverage` emit a per-layer report (seven layers: `spec_predicate`, `spec_actor_matrix`, `spec_transition_state`, `view_extensibility`, `view_e2e_pair`, `handler_go`, **`spec_polarity`**) with profile-aware thresholds (prototype reports only; strict warns; production blocks)? Per-layer thresholds are canonical; any aggregate is opt-in only with method disclosure. Auto-BLOCK if any layer is below its `block_under` under the active profile, OR if `TEST-FIXTURE-LITERAL-001` / `TEST-STUB-ASSERTION-001` / `TEST-PINS-STUB-VOCAB-001` errors are present. See §"Criterion 12 — Test discipline + per-layer coverage (runbook)" below. |
| 13 | Generated-runtime contract honesty | 6% | Does every contract the codegen *emits* stand up to runtime usage? For each command/resource pair in the IR, do the codegen-emitted artifacts (struct shapes, `Command[I, O]` generic instantiations, `db:"..."` tags, migration SQL) verifiably match the hand-authored Go that consumes them? See §"Criterion 13 — Generated-runtime contract honesty (runbook)" below. |

## Scoring scale

| Score | Meaning |
|---|---|
| 9.5–10 | Exemplary. Better than current best-in-class DSLs. |
| 8.5–9.4 | Publishable. A real product can ship on this. |
| 7.5–8.4 | Usable but with clear friction. Not yet AI-first by Lazuli's own bar. |
| 6.5–7.4 | Needs structural work before adoption. |
| < 6.5 | Design problem, not polish. |

## Anchoring discipline

Every score must include:

- One `path:line` reference for the **strongest evidence**.
- One `path:line` reference for the **weakest spot**.

If you can't anchor, you can't grade. Re-read the source. A score
without anchors is rejected and the reviewer is asked to re-grade.

This rule is what keeps grades from collapsing into vibes. It also
catches inflation: if every criterion's "weakest spot" is in the
same file, that file is the audit finding.

## Quality gate

Compute the weighted average → that's the **score**. Then apply:

| Condition | Verdict |
|---|---|
| Score ≥ 8.5 **and** no criterion below 7 | **PASS** (ship as-is) |
| Score ≥ 8.5 **but** at least one criterion below 7 | **PASS with notes** (ship; log the weak criterion as a tracked cut in the operational next-checklist) |
| Score < 8.5 **or** any criterion below 6 | **BLOCK** (do not publish; resolve the weak criterion first) |

**Boundary violations always block, regardless of score.** A
boundary violation is any of:

- Provider-specific names in core syntax (Stripe, AWS, MercadoPago,
  Kubernetes, OpenAI, Anthropic). Provider names live behind
  `@runtime/...`, `@plugin/...`, `@adapter.<local>`.
- `container.lzi` being introduced before registry pressure
  justifies it.
- `workspace.lzi` becoming mandatory for single-app projects.
- Magic discovery without `lazuli inspect` / `lazuli doctor` /
  LSP visibility.
- Lazuli runtime mechanics (DI, broker plumbing, transport details)
  pushed into the language layer.
- Framework runtime constructs an error envelope whose `Message`,
  `message`, or equivalent user-visible field is a hardcoded literal
  in the runtime's authoring language (e.g., English in `runtime/go/`)
  *and* the proposal does not introduce a translation-key surface for
  it. Hardcoded English message **with** a documented migration to
  `@translation.<key>` is not a violation; it's a tracked cut. The
  principle is symmetric to the line above: the language must not leak
  transport mechanics into the runtime, *and* the runtime must not
  leak prose into the wire.
- Vocab additions that introduce control flow (`if/else`,
  `when/otherwise`, `for/each`, `let` bindings beyond field
  references, retry policies, error-handling primitives) into the
  IR. Multi-step orchestration belongs in `handler @fn.X`. The IR's
  job is data shape + policy + lifecycle + single-step effects.
- Codegen emissions that produce opaque runtime engines, interpreters,
  or non-source artifacts (wasm, compiled binaries) in place of
  readable per-feature source files. The Prisma engine is the
  negative pattern. The author MUST be able to read the emitted Go
  (or target language) for any command in `dist/<target>/<feature>/`.
- `@deprecated`, `@warning`, or `@legacy` annotations on commands
  using `handler @fn.X` when an equivalent vocab path also exists.
  The escape hatch is first-class; positioning vocab as "preferred"
  and handler as "legacy" inverts the relationship.
- Codegen-emitted contracts (struct shapes, generic instantiations
  like `Command[I, O]`, `db:"..."` tags, migration SQL) that
  silently disagree with the hand-authored Go that consumes them.
  Codegen contracts cannot disagree silently with Go authoral. The
  doctor must reject the drift at `lazuli doctor` time, before the
  runtime's first invocation. **Operational trigger:** if the
  dispatcher (or `lazuli doctor` static walk) emits
  `HANDLER-SIGNATURE-MISMATCH-001` or `HANDLER-SQL-COLUMN-DRIFT-001`,
  that is an automatic boundary violation regardless of the active
  severity profile. The principle is symmetric to the
  "framework runtime must not leak prose to the wire" line above:
  the codegen must not leak unverified contracts to the handler tree.

A boundary violation is a *deletion*, not a *deferral*. Reject in
line; do not log as a tracked cut.

## Output shape (grade)

```markdown
## Score: <weighted average>/10 — <PASS | PASS with notes | BLOCK>

| # | Criterion | Score | Best evidence | Weakest spot |
|---|---|---|---|---|
| 1 | Legibility | 9.0 | path:line | path:line |
| 2 | Semantic density | 9.2 | path:line | path:line |
| ... |
| 10 | AI-first readiness | 8.7 | path:line | path:line |
| 11 | Framework error message contract | 7.0 | path:line | path:line |
| 12 | Test discipline (per-layer coverage + polarity) | 7.0 | path:line | path:line |
| 13 | Generated-runtime contract honesty | 7.0 | path:line | path:line |

### Top atritos
- path:line — 1-line description — affects criterion N.

### Top faltas
- path:line — 1-line description — what it would unlock.

### Tracked cuts (if PASS with notes)
- Suggested rows for the operational next-checklist.
```

Don't editorialize the output. The rubric is the editorial.

## Self-assessment for proposal authors

Before asking the architect to grade your proposal, walk these
eight checks. If you can't answer all of them in the proposal's
first 100 lines, the proposal is not ready.

1. **What's the boundary?** Language (contracts), runtime (the
   Lazuli Go and TS libraries), adapters (providers). Where does
   each piece of this proposal live? Cite
   `docs/capability-layering.md`.
2. **What does the closed-namespace catalog do here?** If the
   proposal introduces a name, is it under an existing
   `@<namespace>.*`, or is it inventing one? Inventing one is a
   structural change that needs its own proposal.
3. **What's the canonical form?** If the proposal allows two
   surface shapes for the same intent, what's the rule for
   choosing? If the rule is "author preference," cut one form.
4. **What does doctor enforce?** Each new construct needs a
   diagnostic. Anchor with the diagnostic ID and severity.
5. **What does inspect surface?** If the IR shape changes, the
   inspect projection must also change. Specify which `--expand=...`
   class gains the field.
6. **What's the IR delta?** Additive minor, or structural major?
   The rubric reads `LZIR_SCHEMA` and `LZI_LANG` bumps as
   evidence of discipline. Bigger isn't worse, but unannounced
   bigger is.
7. **What is the promotion gate?** Per
   `docs/capability-layering.md` lifecycle: custom → pack →
   pack+doctor → language-light → core. What evidence does the
   proposal have that justifies its placement? "It feels right" is
   not evidence.
8. **What does it remove?** A proposal that only adds is suspect.
   Lazuli stays small by deleting more than it adds. If the
   proposal removes nothing, name what it makes irrelevant in
   user code.

Self-graded score formula (rough): start at 8.5; subtract 0.5 for
each unanswered question. If you arrive below 7.5, the proposal
isn't ready for the architect.

## Examples (anchored)

These are real grades from past architect reviews, kept for
calibration.

### Example 1 — the `ai-primitives-v0` proposal (operational archive) (first pass)

> **Verdict**: BLOCK as one cut. Weighted score 7.6.
>
> Six primitives in one cut. `knowledge` violated promotion
> lifecycle. `budget cost per tenant per month` was runtime
> metering. Q1 (registry effect) and Q3 (flow entry) deferred but
> load-bearing. Architect recommended split into Cut A (tools,
> discriminator, evals — language) and Cut B (flow, budget,
> knowledge — pack/deferred). The score failed the "no axis below
> 7" gate on three axes (static-analysis surface, open questions,
> bloat).

Lesson: a 7.6 weighted with three sub-7 axes blocks. The fix is
not raising the weighted; it's raising the floors.

### Example 2 — the `ai-primitives-v0` proposal (operational archive) (second pass)

> **Verdict**: APPROVE.
>
> After split into Cut A + Cut B and resolution of B1–B4. Cut A
> graded 8.8 (AI-first coverage), 9.2 (layer placement), 8.5
> (coherence), 9.0 (static-analysis surface). Three non-blocking
> nits applied inline.

Lesson: the gate rewards discipline. A 0.4-point increase on the
worst axis turned a BLOCK into an APPROVE.

### Example 3 — Cut A.5 (single-pass)

> **Verdict**: APPROVE WITH NOTES.
>
> Architect already endorsed the shape twice in prior reviews;
> sanity-check pass found one IR-naming drift (`ValidatorExt`
> didn't exist; should extend the existing `Extension` wrapper),
> one promotion-gate softness ("multi-class PII fan-in" is
> trivially satisfiable), and a missing migration design-decision
> entry. All non-blocking; fixed in 4 small edits.

Lesson: a small focused proposal can pass first time if it
extends the IR shape that an already-approved proposal commits
to. The architect's bar lowers when the prerequisite is solid.

## How the rubric is enforced

Three enforcement points:

1. **`/lazuli-grade` slash command** — runs the multi-stage DAG
   in `.claude/commands/lazuli-grade.md`. The architect subagent
   walks the criteria here, grades, and emits the gate verdict.
2. **`lazuli-language-architect` subagent** — used directly via
   the `Task` tool when a single-shot grade is needed. The
   subagent's instructions reference this file as the rubric of
   record.
3. **Human reviewers** — read this document directly when
   proposing a cut, reviewing a PR, or auditing the language.
   Anchor scores with `path:line`. Same gate applies.

### Criterion 8 — Operational schema evolution sub-anchor (runbook)

Added 2026-05-27 per `grader-anti-theater-hardening.md` §5.6. C8
historically scoped to `runtime`, `deploy`, `profiles`, `services`,
`architecture` — the operational outer ring. The pilot-A incident
(Bug C.1 — migration ALTER missing) surfaced that **schema evolution
across the lifetime of a deployed database** sits inside C8's outer
ring but was not named as a positive requirement; C8 read as if it
excluded migrations (the exclusion of "Kubernetes config" was being
generalized too broadly).

The sub-anchor makes the requirement explicit: every proposal whose
scope touches migration emission (anywhere under
`crates/lazuli_codegen_go/src/emitter/`, or any proposal naming
`migrations/` in its body) MUST satisfy the following probes:

- Does the proposal name `CREATE TABLE IF NOT EXISTS` as a footgun
  when used outside the first migration for a table? (The footgun is
  silent re-emission becoming a no-op against pre-existing tables
  with drifted schemas.)
- Does the proposal require ALTER-discipline — forward column adds
  via `ALTER TABLE ADD COLUMN`, never via baseline rewrite of the
  original `CREATE TABLE` migration?
- Does the proposal cite `MIGRATION-IDEMPOTENT-CREATE-001` (or a
  named-equivalent diagnostic) when the migration emission surface
  is in scope? `MIGRATION-IDEMPOTENT-CREATE-001` is the operational-
  discipline finding (runtime behaviour of the migration tool
  against pre-existing schema); its codegen-contract companion
  `MIGRATION-ALTER-MISSING-001` lives under Criterion 13 Probe R-C.
  Citing one does not exempt the proposal from the other.

**Carve-outs:**

- Proposals that don't touch migration emission are vacuously
  satisfied (same shape as C13's per-target carve-outs).
- Greenfield projects with zero `migrations/` history are vacuously
  satisfied — the first emission is legitimately a `CREATE TABLE`;
  the footgun only fires from the second migration forward.

The sub-anchor is the operational-discipline complement to C13's
codegen-contract framing of the same root incident. The two
criteria measure different surfaces — C8 the outer operational
ring (does the proposal reason about already-deployed databases?),
C13 the inner codegen ring (does codegen emit ALTER when it
should?) — and the two findings are filed under their respective
homes rather than duplicated.

### Criterion 8.5 — Diagnostic identifier truthfulness (runbook)

For every diagnostic identifier (anything matching `*_diagnostics`,
`*-001` / `*-002` / ..., or any heading under a "Doctor diagnostics"
list in an acceptance section) named in a proposal, the grader runs:

```bash
rg --no-heading --no-line-number -F '<code>' \
   crates/lazuli_cli/src/doctor/mod.rs \
   crates/lazuli_cli/src/doctor/ \
   crates/lazuli_doctor/src/ \
   crates/lazuli_lsp/src/lib.rs
```

For each named code:

- **≥ 1 hit** → the code exists. The proposal's claim about shipping
  state must be consistent (don't say "new" if it exists; don't say
  "already exists text-side" if zero hits).
- **0 hits AND** the code appears under a `## New diagnostics` (or
  equivalent — `### Net-new doctor producers`, `## Doctor diagnostics
  to add`) heading in the proposal body → fine, the proposal is
  honest about introducing the code.
- **0 hits AND** no `## New diagnostics` heading anchors the code →
  **Criterion 8.5 = 0**. By the existing gate rule (any criterion
  below 6 → BLOCK), the proposal blocks.

Codes appearing inside **audit tables** that explicitly document
false-negatives (rows where the proposal's purpose is to flag the
zero-hits state) do not count toward 8.5 — they are observations,
not assertions of existence. The audit's table header or row prose
must make this clear (e.g. "exact-match grep → 0 hits" in the row).

Closes the false-negative-by-naming pattern surfaced 2026-05-17 during
the framework's internal naming-reconciliation audit.

**Migration-safety sub-anchor (added 2026-05-27).** When a proposal
touches **migration emission** — anything under
`crates/lazuli_codegen_go/src/emitter/` that produces `.sql` files,
or any proposal whose body references `CREATE TABLE` /
`IF NOT EXISTS` / `ALTER TABLE` shapes — the grader runs an
additional probe alongside the standard diagnostic-ID grep:

```bash
# Verify the proposal acknowledges either:
# 1. Idempotent re-emission is safe in its scope (carve-out), OR
# 2. ALTER-after-CREATE discipline is enforced by a named diagnostic.
rg -n 'IF NOT EXISTS|CREATE TABLE' <cited codegen path>
rg -n 'MIGRATION-IDEMPOTENT-CREATE-001|MIGRATION-ALTER-MISSING-001|@correctness\.migration_out_of_sync|ALTER TABLE' \
   crates/lazuli_doctor/src/correctness/ crates/lazuli_codegen_go/src/emitter/
```

If the proposal touches migration emission AND fails to name a
diagnostic that catches re-emission as a no-op against drifted
prod tables (`MIGRATION-IDEMPOTENT-CREATE-001` is the canonical
named diagnostic for the codegen-side footgun;
`MIGRATION-ALTER-MISSING-001` is its codegen-contract companion),
C8.5 caps ≤ 5. This is the operational-discipline complement to
C13's codegen-contract framing of the same bug class (Bug C.1 in
the canonical pilot-A incident): the two findings come from one
root incident but live in different rubric homes — C8.5 grades
diagnostic naming honesty, C13 grades codegen-contract honesty.
Carve-out: proposals that don't touch migration emission are
vacuously satisfied (no additional probe runs).

### Criterion 11 — Framework error message contract (runbook)

For every proposal that claims a score ≥ 7 on Criterion 11, the grader
runs four probes. The probe commands are deliberately written against
the current and near-future shape of the runtime; at grading time, a
file may not yet exist, and that's the probe's signal.

```bash
# Probe 1: framework runtime emits raw English strings on Error.Message.
# Expect: 0 hits when the runtime is correct. Each hit -> deduct from 10.
rg --no-heading -F 'Message: "' runtime/go/lazuli/ \
   --glob '!*_test.go' --glob '!*/i18n/*'

# Probe 2: a translation key namespace exists for framework errors.
# Expect: ≥ 1 hit in the IR catalog or docs naming the framework
# message surface (e.g. `@translation.framework.policy_denied`).
rg --no-heading '@(translation|framework_message)\.' \
   crates/lazuli_ir/src docs/

# Probe 3: doctor lints missing framework-message coverage.
# Expect: ≥ 1 hit naming a diagnostic that fires on missing coverage.
rg --no-heading 'framework_message|policy_message_missing|error_message_locale' \
   crates/lazuli_cli/src/doctor/mod.rs crates/lazuli_doctor/src/

# Probe 4: error contract documents the message-key surface.
# Expect (for scores ≥ 9): both terms anchored in docs/error-contract.md.
rg --no-heading -F 'when_denied' docs/error-contract.md
rg --no-heading -F 'message_key' docs/error-contract.md
```

Scoring anchors (mirroring §4.2 of the source proposal):

| Score | What the grader sees in the candidate |
|---|---|
| 0 | Any framework error path constructs `Message: "<english literal>"` (or equivalent in the codegen target's runtime) and that string reaches the JSON wire unmodified. Probe 1 returns ≥ 1 hit. Auto-BLOCK per the boundary-violation amendment if no migration is documented. |
| 3 | Defaults exist as English literals **but** an interception hook exists (app-level `OnFrameworkError(err) ErrorView`) that authors can wire to swap messages. No locale negotiation. |
| 5 | Defaults route through a translation key (`@framework.policy_denied`, `@framework.validation_failed`, etc.) but only one locale ships; no fallback graph honored on framework errors. |
| 7 | Translation keys per framework error kind; locale negotiation honored via `i18n.Resolve`; PT-BR + EN ship; doctor warns on missing keys for any supported locale. Probes 1+2+3 anchored. |
| 9 | All of the above **plus** per-command override (`command X { ... when_denied @translation.<key> }`) and per-feature override (`feature.errors.policy_denied @translation.<key>`); LSP completes the override surface. Probe 4 anchored. |
| 10 | All of the above **plus** doctor enforces that every framework error kind enumerated in `docs/error-contract.md` has at least one `@translation.<key>` registered for every locale declared in `app.locale`, with a tracked-cuts row generated when a locale lacks coverage. Round-trip eval: a fixture authored in PT only, with locale `en` requested, emits the EN fallback and never the framework string. |

False-positive carve-outs (Criterion 11 does NOT fire on):

- **5xx internal errors** (status ≥ 500). These represent invariant
  violations or platform bugs; their messages are for operators reading
  logs, not end users. Browser/PWA error boundaries should render
  generic copy regardless of message body. Only `Error.Status < 500`
  paths count toward C11; Probe 1 must be filtered by HTTP status when
  the deduction is computed.
- **CLI / development errors** (`lazuli inspect`, `lazuli doctor`,
  `lazuli generate`). Errors emitted from `crates/lazuli_cli/` binaries
  are out of scope for C11. The criterion is scoped to the **generated
  runtime's wire output**.
- **Diagnostic source-map prose** (`source`, `feature`, `path` fields
  of the error envelope per `docs/error-contract.md`). These are
  intentional introspection aids for tooling. C11 only constrains the
  `message` field.

Triggered by the canonical pilot PWA framework-string leak (2026-05-18). See
proposal `c:/Users/lucas/lazuli-ops/docs/proposals/grader-error-message-criterion.md`
for the full provenance, audit table, and fixture-list expectations.

### Criterion 12 — Test discipline + per-layer coverage (runbook)

Scope: every proposal that introduces, extends, or generalises a
testable construct (`command`, `rule`, `workflow.transition`,
`lifecycle.transition`, `view`, `@fn.*`/`@validator.*`/`@hook.*`
handler). Filed by `tdd-bdd-first-2026-05-23.md` Wave 6.

**Load-bearing invariant.** Coverage is reported and gated **per
layer**, never as a single aggregated percentage that hides which
paradigm is weak. The aggregate is permitted only with explicit
method disclosure (`weighted-by-construct-count`, `weighted-by-LOC`,
`unweighted-mean`), and never as the gate.

**Layer catalog (canonical seven):**

| Layer | Denominator | Numerator | Source |
|---|---|---|---|
| `spec_predicate` | Predicate branches in `requires` / `rule.when` / transition predicates | Branches with `allows when` + `denies when` covering each side | IR walk |
| `spec_actor_matrix` | `(construct, @role.X)` pairs derived from `policy @policy.X` | Pairs touched by `permits`/`forbids` or `allows as`/`denies as` rows | IR walk |
| `spec_transition_state` | `from <state>` slots in workflow + lifecycle transitions | Slots with ≥1 `allows from` (DeniesFrom alone is not sufficient) | IR walk |
| `view_extensibility` | Views with `extensible_by` | Views with ≥1 `allows extension` / `denies extension` | `.lzx` walk |
| `view_e2e_pair` | Declared views | Views with `e2e/<feature>/<view>.spec.ts` present | filesystem |
| `handler_go` | Statements in `app/features/<f>/handlers/*.go` (excluding `_test.go`) | Statements with `count > 0` in `coverage.out` | `go test -coverprofile` parse |
| `spec_polarity` | Constructs with a `tests` block | Constructs with ≥1 `allows*` (positive) AND ≥1 `denies*` (negative) assertion | IR walk |

Layers 1–4 and layer 7 (`spec_polarity`) are pure-IR (zero runtime,
zero flakiness). Layers 5–6 are filesystem / external-tool integrations
that degrade gracefully when their inputs are absent (vacuous pass
with disclosure in `raw_file`).

The seventh layer (`spec_polarity`) was added 2026-05-27 per
`grader-anti-theater-hardening.md` §4.2 to close the gap surfaced by
pilot-A's bug E: the original six layers measure presence and coverage
but are silent on assertion polarity. A test suite that asserts only
failure paths can satisfy `handler_go ≥ block_under` at strict and
production profiles while the happy path is never exercised. The
`spec_polarity` layer is the IR-side complement to Probe R-D
(`_test.go` polarity balance) under Criterion 13. Carve-out: pure
validators (no happy-path return value to assert), pure denials (e.g.,
kill-switch constructs that always error), and pure reads (queries
with no `Creates`/`Updates` effect) are vacuously satisfied.

**Gate matrix (default profile-derived thresholds):**

| profile | block_under | warn_under |
|---|---|---|
| prototype | 0 (no gating) | 0 (no warnings) |
| strict | 0 (warn-only) | per-layer warn target (e.g. `spec_predicate=80`) |
| production | per-layer block target (e.g. `spec_predicate=50`) | per-layer warn target |

Project authors override via `Lazurite.toml [doctor.coverage]`:

```toml
[doctor.coverage]
spec_predicate      = { block_under = 50, warn_under = 80 }
spec_actor_matrix   = { block_under = 70, warn_under = 90 }
aggregate_method    = "weighted-by-construct-count"
```

**Anchors (score scale).**

| Score | Meaning |
|---|---|
| 0 | Proposal introduces a testable construct with no `tests` semantic (or a `tests` shape that is just prose). Auto-BLOCK. |
| 3 | `tests` slot exists on the construct but no calculator measures it. Reviewer cites path:line where the slot is parsed but no coverage layer reads it. |
| 5 | One calculator measures the construct. Per-layer threshold defaults documented for at least one profile. |
| 7 | Three calculators measure constructs touched by the proposal; profile-default thresholds documented for all three; manifest override surface present. |
| 9 | All applicable layers measured; CI gate (`--fail-on coverage:<layer>=<N>`) wired; JSON output has the canonical `coverage` shape per Wave 6.3; LSP completeness gap (if any) filed as adjacent issue. |
| 10 | All of the above **plus** the proposal carries before/after coverage numbers from a real pilot demonstrating that gating produced measurable hardening. |

**Probes (mechanical):**

- **Probe Q-A — Per-layer reporting.** `lazuli doctor --coverage --format json` returns the canonical `coverage.layers.<name>` shape with `covered`/`total`/`pct`/`verdict` populated for every applicable layer. Grader runs:
  ```bash
  lazuli doctor <fixture> --coverage --format json | jq '.coverage.layers | keys'
  ```
  Result MUST include every layer the proposal claims to harden.

- **Probe Q-B — Gate composability.** `--fail-on coverage:<layer>=<N>` exits non-zero when below threshold. Composable with `--fail-on severity` / `--fail-on category:X` (post-Wave 0.5). Grader runs:
  ```bash
  lazuli doctor <fixture> --coverage --fail-on coverage:<layer>=99
  echo "$?"   # MUST be 1 when fixture's <layer> coverage is < 99%
  ```

- **Probe Q-C — No single-percentage gate.** The grader greps the proposal for "≥ 80%" / "coverage ≥ N%" style aggregate gates. ANY use of an aggregate threshold without per-layer breakdown is an automatic Criterion 12 deduction (cap at 5).

- **Probe Q-D — Aggregate disclosure.** If the proposal opts into an aggregate, the `aggregate.method` field MUST be set to one of `weighted-by-construct-count` / `weighted-by-LOC` / `unweighted-mean` (or another method disclosed verbatim). Naked aggregates without method are an auto-BLOCK.

- **Probe Q-E — Assertion polarity balance (spec_polarity layer).**
  Walks every construct with a `tests` block in the IR and verifies
  that at least one positive assertion (`allows when` / `permitted as`)
  AND at least one negative assertion (`denies when` / `forbidden as`)
  exists. Grader runs (against the canonical fixture):
  ```bash
  lazuli doctor <fixture> --coverage --format json \
    | jq '.coverage.layers.spec_polarity | {covered, total, pct, verdict}'
  ```
  Per-layer threshold defaults: prototype 0/0, strict warn_under=80,
  production block_under=50. The layer's verdict carries the same
  block / warn / pass semantics as the other six. A construct that
  satisfies a carve-out (pure validator, pure denial, pure read) is
  removed from the denominator before pct is computed; carve-out
  application is logged under `coverage.layers.spec_polarity.carve_outs`
  for cold-read auditability.

- **Probe Q-F — Stub-state literal sniff (TEST-STUB-ASSERTION-001).**
  Walks every `_test.go` companion in `app/features/<f>/handlers/`
  and flags any assertion (`assert.Contains(t, err.Error(), <lit>)`,
  `require.EqualError(t, err, <lit>)`, equivalent shapes) where
  `<lit>` matches a known stub-state catalog: `"not implemented"`,
  `"not yet implemented"`, `"todo:"`, `"TODO:"`, `"stub"`,
  `"placeholder"`, `"unimplemented"`. The diagnostic
  `TEST-STUB-ASSERTION-001` (paired with the broader
  `TEST-PINS-STUB-VOCAB-001` rule that extends the catalog beyond
  `@TODO authored:` markers) emits at `error` severity at strict AND
  production profiles, `warning` at prototype. The rule is
  independent of `TEST-STUB-001` (which scans `@TODO authored:`
  comments — author-facing); Q-F scans for asserted-on stub-state
  strings — production-facing. Diagnostic must say:
  > `TEST-STUB-ASSERTION-001`: `<path>:<line>` asserts against the
  > literal `<lit>` — this matches a stub-state error message no
  > longer returned by the implementation. Either update the
  > assertion to cover the current contract OR delete the test and
  > re-scaffold via `lazuli generate handler test`.

**Auto-BLOCK escalation (refined 2026-05-27).** The C12 auto-BLOCK
list is extended beyond the original `TEST-FIXTURE-LITERAL-001`
trigger:

- Any layer below `block_under` under the active profile (unchanged).
- Any `TEST-FIXTURE-LITERAL-001` error (Wave 1, unchanged).
- Any `spec_polarity` layer below `block_under` under the active
  profile (new; via Probe Q-E).
- Any `TEST-STUB-ASSERTION-001` error (new; via Probe Q-F — escalated
  from warning to auto-BLOCK because the author cleared the
  `@TODO authored:` marker but forgot to clean the assertion, which
  is materially worse than a never-cleared marker because it implies
  the author *thought* they were done).
- Any `TEST-PINS-STUB-VOCAB-001` error (new; companion diagnostic
  to Q-F covering the broader stub vocabulary catalog).

The escalation reflects the proposal §5.5 cross-validation finding
that "presence is not substance" — a test file that exists, runs,
and asserts is still theater if its assertion is pinned to a stale
implementation state. BLOCK is the correct severity once the
stub-state shape is mechanically detectable.

**False-positive carve-outs (Criterion 12 does NOT fire on):**

- Proposals that touch no testable construct (e.g. pure observability,
  pure manifest hygiene, pure rename/migrate cycles). The criterion
  scope is constructs that ship a `tests` slot or that participate in
  the six coverage layers.
- `handler_go` layer reporting `total = 0` when the project has no
  authored `.go` handlers yet. Vacuous pass; no deduction. The
  `view_e2e_pair` carve-out is symmetric for projects with no `.lzx`
  views.
- Prototype-profile projects. The matrix's `prototype` row sets every
  `block_under` and `warn_under` to 0 by design; reporting is on,
  gating is off. Reviewers MUST verify the proposal does not retroactively
  promote a prototype pilot to strict gating without an authored
  `[doctor.coverage]` opt-in.

**Boundary against runtime invasion (re-affirmed from Wave 3.5):**
Criterion 12 does NOT require Lazuli to run Playwright, Go tests, or
any external runner. The `view_e2e_pair` layer checks file existence
only; `handler_go` parses Go's coverprofile output but never invokes
`go test` itself. CI orchestration runs the test commands; Lazuli
only consumes the resulting artifacts.

**Triggered by:** the framework-side gap surfaced in the audit
(`lazuli-ops/docs/proposals/test-completeness-lints.md` §1) — `tests` slot
present, zero enforcement. Wave 6 closes the loop by making the
coverage observable + gateable + reportable in the same JSON shape
agents and CI already consume. Provenance:
`lazuli-ops/docs/proposals/tdd-bdd-first-2026-05-23.md` Wave 6. The
2026-05-27 refinement (Probes Q-E + Q-F, `spec_polarity` layer,
auto-BLOCK escalation) is provenance:
`lazuli-ops/docs/proposals/grader-anti-theater-hardening.md` §4.2 — closes
the polarity-blind + stub-state-pinned gaps surfaced by the
canonical pilot-A incident (bugs D + E).

### Criterion 13 — Generated-runtime contract honesty (runbook)

Scope: every proposal that introduces, extends, or generalises a
codegen-emitted artifact consumed by hand-authored Go in
`app/features/<f>/handlers/`. Filed by
`grader-anti-theater-hardening.md` (2026-05-27).

**Load-bearing invariant.** For every command/resource pair in the
IR, the codegen contract artifact (struct, generic instantiation,
migration SQL, tag set) MUST be a verifiable predicate over the
hand-authored Go that consumes it. The doctor must be able to make
this check **without running the code**. Codegen contracts cannot
disagree silently with Go authoral.

**Probes (mechanical):**

- **Probe R-A — Handler signature parity.** For every command `C`
  in the IR with effect `Returns @fn.X`, the grader runs:
  ```bash
  # 1. The codegen instantiation. Captures Command[I, O].
  rg -n 'lazuli\.Command\[[^,]+,\s*[^]]+\]' \
     dist/go/<feature>/command.gen.go
  # 2. The handler signature.
  rg -n 'func\s+\w+\(ctx\s+\*lazuli\.Ctx,\s*\w+\s+\w+\)\s+\([^,]+,\s*error\)' \
     app/features/<feature>/handlers/<name>.go
  ```
  Both anchors must resolve. The output type (second slot of
  `Command[I, O]`) of the codegen MUST equal the return type (first
  slot of `(_, error)`) of the handler — `struct{}` is NOT a valid
  match for `string`, `Token`, or any non-void return. The named
  diagnostic is `HANDLER-SIGNATURE-MISMATCH-001`. Until the
  diagnostic ships, Criterion 13 caps ≤ 4 on this probe.

  - 0 disagreements: R-A passes.
  - 1+ disagreements: cap C13 ≤ 4 until `HANDLER-SIGNATURE-MISMATCH-001` ships.
  - Diagnostic ships and fires on the candidate: cap removed AND
    boundary violation fires per §"Quality gate" (auto-BLOCK).

- **Probe R-B — Handler effect surface parity.** For every command
  `C` with effect `Creates @Resource` or `Updates @Resource`, the
  grader runs:
  ```bash
  # 1. Columns the resource struct declares.
  rg -n 'db:"[a-z_]+"' dist/go/<feature>/resource.gen.go
  # 2. Columns the handler's INSERT/UPDATE mentions.
  rg -n 'INSERT INTO|UPDATE\s+\w+\s+SET' \
     app/features/<feature>/handlers/<command>.go
  ```
  The intersection of (NOT NULL columns in IR) and (columns the
  handler ignores in its emitted SQL) MUST be empty. The named
  diagnostic is `HANDLER-SQL-COLUMN-DRIFT-001`. Until the diagnostic
  ships, Criterion 13 caps ≤ 5 on this probe.

  Carve-out: if the resource has `timestamps` enabled and the
  missing column is `created_at` / `updated_at`, no finding — the
  framework injects those automatically at emit time. Probe R-B
  walks the same `timestamps`-aware predicate as
  `UPDATES-MISSING-UPDATED-AT-001`
  (`crates/lazuli_doctor/src/correctness/updates_missing_updated_at.rs`).

- **Probe R-C — Migration ALTER-after-CREATE discipline.** When
  the IR has a column for a resource AND a `CREATE TABLE`
  migration for that resource already exists on disk with a column
  set that differs, codegen MUST emit a new `ALTER TABLE ADD
  COLUMN` (or `DROP COLUMN`) migration file rather than rewriting
  the original `CREATE TABLE`. Grader runs:
  ```bash
  # Detect re-emitted CREATE TABLE for a resource that already has one.
  ls migrations/*<feature>*<resource>*.sql | wc -l
  # If > 1 file matches, at most ONE may contain `CREATE TABLE`; all
  # others MUST be `ALTER TABLE` statements.
  rg -c '^\s*CREATE TABLE' migrations/*<feature>*<resource>*.sql
  ```
  The named diagnostics are `MIGRATION-ALTER-MISSING-001` (codegen
  forgot the ALTER follow-up) paired with
  `MIGRATION-IDEMPOTENT-CREATE-001` (codegen re-emitted a
  `CREATE TABLE IF NOT EXISTS` against a table whose deployed
  schema may have drifted). Both ship together; either alone is
  insufficient evidence.

  - At most one `CREATE TABLE` per resource across all migration
    files: R-C passes.
  - Multiple `CREATE TABLE IF NOT EXISTS` for the same table,
    indicating re-emission instead of incremental ALTER: cap
    C13 ≤ 4 until both diagnostics ship.
  - Codegen also re-emits without bumping the migration number:
    boundary violation per §"Quality gate" — BLOCK regardless of
    score.

- **Probe R-D — `_test.go` polarity balance.** For every
  `handlers/<command>_test.go`, the grader runs:
  ```bash
  # Failure-path assertions.
  rg -c 'require\.Error\(t,|assert\.Error\(t,' \
     app/features/<feature>/handlers/<command>_test.go
  # Happy-path assertions.
  rg -c 'require\.NoError\(t,|assert\.NoError\(t,' \
     app/features/<feature>/handlers/<command>_test.go
  ```
  Both counts must be ≥ 1 unless the construct is a pure validator
  (no happy-path return value to assert) OR the construct is
  `denies`-only by design (e.g., a kill-switch that always errors).
  The named diagnostic is `TEST-FAILURE-ONLY-COVERAGE-001`. Until
  it ships, C13 caps ≤ 5 on this probe; once shipped, the cap is
  removed.

  R-D is the `_test.go`-side complement of C12's `spec_polarity`
  layer (Probe Q-E). The two probes are paired: Q-E walks the IR's
  `tests` block; R-D walks the Go file companion. They are kept
  in separate criteria because R-D belongs with its
  codegen-contract siblings (R-A/R-B/R-C) for coherence — but the
  C12 runbook cross-references R-D explicitly.

**Tier-1 / Tier-2 stratification (per §5.5.1 of source proposal):**

| Probe | Tier | Block behaviour | Cap-lift trigger |
|---|---|---|---|
| R-A — Handler signature parity | **Tier 1** | Single-probe violation auto-BLOCKs (after `HANDLER-SIGNATURE-MISMATCH-001` ships). | `HANDLER-SIGNATURE-MISMATCH-001` lands in `crates/lazuli_doctor/`. |
| R-B — Handler effect surface parity | Tier 2 | Violation contributes to BLOCK *only* in conjunction with ≥1 other Tier-2 violation (R-C OR R-D). | `HANDLER-SQL-COLUMN-DRIFT-001` lands. |
| R-C — Migration ALTER-after-CREATE | Tier 2 | Same — collective Tier-2 BLOCK. | `MIGRATION-ALTER-MISSING-001` + `MIGRATION-IDEMPOTENT-CREATE-001` land as a pair. |
| R-D — `_test.go` polarity balance | Tier 2 | Same. | `TEST-FAILURE-ONLY-COVERAGE-001` + paired `TEST-PINS-STUB-VOCAB-001` / `TEST-STUB-ASSERTION-001` land. |

Tier 1 is ship-first: the runtime registry literally documents the
gap (signature mismatches detected at dispatch, not at registration),
the evidence hierarchy is highest, and the false-positive rate is
lowest (Lazuli's codegen makes string-compare on idents sufficient).
Tier 2 probes (R-B/R-C/R-D) have legitimate edge cases (framework
helpers; greenfield databases; negative-only test suites) that
justify warning-default with collective BLOCK only when multiple
fire — a single Tier-2 violation is a tracked cut, not a ship-stop.

**Scoring anchors:**

| Score | What the grader sees in the candidate |
|---|---|
| 0 | Codegen-emitted artifact silently disagrees with hand-authored consumer and the doctor cannot detect it (e.g., `Command[I, struct{}]` vs. handler returning `string`). Auto-BLOCK per the boundary violation if a migration design is not documented. |
| 3 | One of R-A/R-B/R-C/R-D probes passes with diagnostic-shaped wiring in the doctor; the other three are tracked-cut deferrals. |
| 5 | Two probes pass with diagnostic-shaped wiring; remaining two are tracked-cut deferrals. |
| 7 | Three probes pass with diagnostic-shaped wiring AND the wired diagnostics are emitted at `error` severity under `production` profile, `warning` at `strict`. The fourth probe is documented and tracked. |
| 9 | All four probes pass with diagnostic-shaped wiring at `production` severity; LSP completes the corrective hints. `lazuli inspect` JSON projects the parity status under a `contract_parity` key. |
| 10 | All of the above **plus** the doctor emits the diagnostics on the canonical anti-theater regression fixture (see §"Regression fixture" in the source proposal), which the grader runs as a smoke test before assigning ≥ 9. |

**False-positive carve-outs (Criterion 13 does NOT fire on):**

- Handler files outside the canonical
  `app/features/<f>/handlers/` layout. The doctor's handler-path
  resolution (`crates/lazuli_doctor/src/handler_path.rs`) is the
  authority; paths outside that contract are not graded.
- Validator handlers (`@validator.X` references) where the runtime
  contract is `(ctx, input) error` (no output type to drift).
  R-A is vacuously satisfied; R-B / R-D still apply.
- Pure read-side handlers attached to `query` constructs — they
  have no `Creates`/`Updates` effect to drift against; R-B is
  vacuously satisfied.
- Greenfield projects with no `migrations/` directory yet. R-C is
  vacuously satisfied; the first `lazuli generate go .`
  legitimately emits one `CREATE TABLE`.
- Non-Go targets (once they ship). The principle is target-agnostic
  but Probe R-A's grep target becomes language-conditional
  (`dist/<target>/<feature>/` and
  `app/features/<feature>/handlers/<name>.<target_ext>`). Capture
  in C13 when the second runtime ships.

**Cross-validation against the canonical pilot-A incident (§5.5
of source proposal).** The five staged bug classes from the
incident are reproduced here as the canonical anti-theater set;
each row shows how the refined rubric flips the verdict from
"passes under prior rubric" → "BLOCK under refined rubric":

| Bug | Prior rubric verdict | Refined rubric verdict | Anchor |
|---|---|---|---|
| A — handler signature mismatch (`Command[Input, struct{}]` vs handler `(string, error)`) | PASS — no criterion budges; C9 passes (file-existence), C12 `handler_go` reports 100% statement coverage. | **BLOCK** — C13 Probe R-A fires; `HANDLER-SIGNATURE-MISMATCH-001` is a tier-1 single-probe BLOCK trigger; the new boundary-violation line fires automatically. | C13 R-A + boundary §"Quality gate". |
| B — handler INSERT omits NOT NULL `updated_at` while resource struct declares it | PASS — `UPDATES-MISSING-UPDATED-AT-001` only catches the IR-side gap; handler-side gap is silent; C9 + C12 `handler_go` both pass. | **BLOCK** — C13 Probe R-B fires; `HANDLER-SQL-COLUMN-DRIFT-001` is a tier-2 BLOCK trigger when combined with any other tier-2 violation, OR a boundary-violation single-probe BLOCK trigger per the dispatcher-emission rule. | C13 R-B + boundary §"Quality gate". |
| C.1 — codegen forgot ALTER follow-up after column added; `CREATE TABLE IF NOT EXISTS` re-emits as no-op against drifted prod table | PASS — C8 implicitly excluded migration safety; boundary list silent; `@correctness.migration_out_of_sync` is informational only. | **BLOCK** — C13 Probe R-C fires; `MIGRATION-ALTER-MISSING-001` + `MIGRATION-IDEMPOTENT-CREATE-001` ship as a pair and contribute to tier-2 collective BLOCK; C8 sub-anchor also caps if the proposal doesn't acknowledge the footgun; C8.5 migration-safety sub-anchor caps ≤ 5 if the proposal touches emission without naming the diagnostics. | C13 R-C + C8 sub-anchor + C8.5 sub-anchor. |
| D — `_test.go` pins literal `"not implemented"` after handler implemented | PASS — `TEST-STUB-001` clears (no `@TODO authored:`); `TEST-HANDLER-MISSING-001` clears (file exists); `handler_go` reports 100% statement coverage. | **BLOCK** — C12 Probe Q-F fires `TEST-STUB-ASSERTION-001`; the auto-BLOCK escalation makes this a hard BLOCK (escalated from warning); `TEST-PINS-STUB-VOCAB-001` covers the broader catalog; C13 Probe R-D additionally fires `TEST-FAILURE-ONLY-COVERAGE-001` when the same test asserts only failures. | C12 Q-F auto-BLOCK + C13 R-D. |
| E — failure-only test coverage (no happy-path `require.NoError`) | PASS — `handler_go` reports `covered=N` because failure path runs every statement; no layer measures polarity. | **BLOCK** — C12 Probe Q-E (`spec_polarity` layer) fires when the IR-side `tests` block lacks `allows*` assertion; C13 Probe R-D fires when the `_test.go` companion lacks `require.NoError`; either path is sufficient for auto-BLOCK. | C12 Q-E + C13 R-D. |

The cross-validation is **documentation, not a criterion change**.
It exists to make the rubric's anti-theater intent concrete: a
green doctor under `iron-hand` no longer equals shipping safety;
the refined rubric requires green C12 (with `spec_polarity` +
stub-state) AND green C13 (with all four probes) before the gate
returns PASS. The five rows above are the canonical regression set
the rubric is hardened against.

**Triggered by:** the canonical pilot-A 2026-05-27 incident
(Google sign-in production break under green-doctor iron-hand).
Provenance: `lazuli-ops/docs/proposals/grader-anti-theater-hardening.md` v0.2
(self-graded 8.75 PASS strict). The five bug classes map 1:1 to
the architect-wave proposals
(`HANDLER-SIGNATURE-MISMATCH-001`, `HANDLER-SQL-COLUMN-DRIFT-001`,
`MIGRATION-ALTER-MISSING-001` + `MIGRATION-IDEMPOTENT-CREATE-001`,
`TEST-PINS-STUB-VOCAB-001`, `TEST-FAILURE-ONLY-COVERAGE-001`),
each of which is the diagnostic implementation that lifts the
corresponding C13 probe cap from "score capped at ≤ 4/5" to
"score uncapped, mandatory auto-BLOCK on violation."

### Criterion 4 — Escape hatches (Prisma-trap runbook)

Applies whenever a proposal introduces, extends, or generalises *vocab* —
defined as any new top-level construct, new sub-block keyword, or new
`@<namespace>.<name>` identifier that would *replace* hand-written code
paths.

The grader runs four probes. Failure on any single probe caps Criterion
4 at the indicated score regardless of other evidence on the criterion.

**Probe P-A — Pattern provenance test.** Is the vocab a formalisation of
code that already exists, repeated in ≥3 handlers across ≥2 pilots? The
proposal MUST cite path:line evidence. Grader runs:

```bash
rg --no-heading -F '<pattern token>' \
   <pilot 1 handler tree> <pilot 2 handler tree>
```

- ≥3 hits across ≥2 pilots → P-A passes.
- 1-2 hits OR all hits in single pilot → cap C4 ≤ 5.
- 0 hits AND no `## Preview vocab — no pilot precedent` disclosure →
  cap C4 ≤ 3.

**Probe P-B — Escape hatch parity test.** After landing this vocab, is
`handler @fn.X` still first-class for the same command, query, or job?
Grader runs:

```bash
rg -n -i 'deprecat|legacy|prefer.*vocab|should.*declarative|escape.*hatch.*last' \
   lazuli-ops/docs/proposals/<proposal>.md
rg -n 'handler.*should|fn.*deprecated|imperative.*warning' \
   crates/lazuli_cli/src/doctor/mod.rs crates/lazuli_doctor/src/
```

- Zero hits + proposal explicitly affirms parity → P-B passes.
- Vocab framed as "preferred" / handler as "legacy" → cap C4 ≤ 5.
- Doctor flags handler-based commands as "should be declarative" →
  cap C4 ≤ 3.
- `@deprecated` / `@warning` on `handler @fn.X` → boundary violation,
  BLOCK.

**Probe P-C — Emitted-code legibility test.** Can an author open
`dist/<target>/<feature>/*.gen.<ext>` and read the code for this vocab?

```bash
rg -n 'engine\.|interpreter\.|VocabExecutor|Runner.Run\(.*Definition' \
   <cited codegen target or template>
```

- Per-feature inline emission → P-C passes.
- Runtime engine / interpreter taking vocab as data → cap C4 ≤ 4.
- Non-source artifact (wasm/binary) → boundary violation, BLOCK.

**Probe P-D — Cognitive surface test.** Does the vocab introduce
branching, loops, variable bindings beyond field references, retry
policies declarative, or error-handling primitives?

- Zero rejected constructs → P-D passes.
- Any rejected construct → STOP. Reject the proposal at acceptance
  time per RULE-VOCAB-03. Grading does not proceed.

Triggered by the canonical pilot vocab-saturation analysis (2026-05-20). See
proposal `c:/Users/lucas/lazuli-ops/docs/proposals/grader-vocab-governance.md`
for full probe details, worked examples, and the four Vocab Governance
Rules that gate acceptance.

## Vocab Governance Rules

These rules gate proposal **acceptance** (before grading). A proposal
that fails any rule is returned to the author for revision; it does not
enter the grading DAG.

The four rules are orthogonal to the criterion scoring above:
P-A through P-D produce graded measurements (Criterion 4 caps);
RULE-VOCAB-01 through 04 produce binary gates (proposal accepted or
returned).

### RULE-VOCAB-01 — Pattern Provenance Mandatory

New vocab additions MUST cite ≥3 existing handlers across ≥2 pilots
that the addition formalises. Vocab inventing a new abstraction layer
without precedent in pilot code is REJECTED at proposal acceptance,
not at grade time.

Evidence format: `path:line` citations of the 3+ handlers, with the
boilerplate pattern that the new vocab replaces shown inline. The
grader runs the cited greps to verify pattern existence and density.

Exception: vocab explicitly marked `## Preview vocab — no pilot
precedent` MAY be proposed but is capped at `language-light` in the
capability-layering lifecycle until provenance evidence accumulates.

### RULE-VOCAB-02 — Escape Hatch First-Class Forever

Every command, query, or job that can be expressed in vocab MUST ALSO
accept `handler @fn.X` as the implementation path. The IR cannot emit
`@deprecated`, `@warning`, or `@legacy` annotations on commands using
`handler @fn.X`. Doctor cannot flag handler-based commands as "should
be declarative" or otherwise discourage handler-shaped implementations.

Authoring docs MAY explain when vocab is more concise, but MAY NOT
position handler as a degenerate option. The two paths are siblings,
not parent/child.

### RULE-VOCAB-03 — No Workflow DSL Inside IR

Vocab additions that introduce branching (`if/else`,
`when/otherwise`), loops (`for/each`), variable bindings beyond field
references, retry policies declarative, or error-handling primitives
MUST be REJECTED at proposal acceptance.

Multi-step orchestration belongs in `handler @fn.X`. Multi-tx
side-effect chains belong in `handler @fn.X`. The IR's job is data
shape + policy + lifecycle + single-step effects. The line: a vocab
item that compiles to a single SQL statement, single HTTP call, single
emit, or single function dispatch is in scope. A vocab item that
compiles to a step sequencer is out of scope.

### RULE-VOCAB-04 — Emitted Code Must Be Read-Through

For every new vocab, the codegen emission MUST produce human-readable
source in `dist/<target>/<feature>/*.gen.<ext>` that an author can
read, vendor, or copy out of the framework. No opaque runtime engines,
no interpreters reading vocab as data at request time. The Prisma
engine is the negative pattern to avoid.

Allowed: a runtime helper called *inline* from the per-command .gen
file with explicit arguments. Forbidden: a runtime `VocabRunner.Run(decl
VocabDeclaration)` shape.

## Versioning

The rubric is part of the language contract. Changes to the
weights, criteria, or gate rule are themselves
`/lazuli-grade`-graded changes:

- A weight shift (e.g., Token efficiency 10% → 8%) is a minor
  rubric bump.
- A criterion replacement (e.g., merging Composability into
  Multi-target fit) is a major rubric bump.
- The boundary-violation list is append-only without a major
  bump. Removing a line is major.

History of changes lives in `git log -- docs/grading-rubric.md`.

Notable changes:

- **2026-05-27 — Criterion 13 inserted (Generated-runtime contract
  honesty, 6%); weights redistributed (C1 −1, C3 −1, C8 −1, C10 −1,
  C12 −2).** Sum stays at 100%. AI-first cluster (C2 + C8.5 + C10 +
  C11) moves from 34% to 33% — well inside the 35% ceiling. C12
  extended with `spec_polarity` layer (seventh layer in the
  canonical catalog) + Probes Q-E / Q-F + auto-BLOCK escalation on
  `TEST-STUB-ASSERTION-001` / `TEST-PINS-STUB-VOCAB-001`. C8.5
  runbook extended with migration-safety sub-anchor citing
  `MIGRATION-IDEMPOTENT-CREATE-001`. C8 runbook extended with the
  operational schema evolution sub-anchor. New boundary-violation
  line: codegen-emitted contracts must not silently disagree with
  hand-authored Go; the boundary fires automatically when
  `HANDLER-SIGNATURE-MISMATCH-001` or `HANDLER-SQL-COLUMN-DRIFT-001`
  is emitted. Cross-validation subsection added inside the C13
  runbook mapping each of the five canonical pilot-A bug classes
  (A/B/C.1/D/E) from "PASS under prior rubric" to "BLOCK under
  refined rubric" with anchored evidence. **Forward-only hardening
  — no past PASS retroactively becomes BLOCK.** Triggered by the
  canonical pilot-A 2026-05-27 incident (Google sign-in production
  break under green-doctor iron-hand preset). Source proposal:
  `c:/Users/lucas/lazuli-ops/docs/proposals/grader-anti-theater-hardening.md`
  v0.2 (self-graded 8.75 PASS strict). The five bug classes map
  1:1 to the architect-wave diagnostic proposals shipping under
  cells in `crates/lazuli_doctor/`; the rubric refinement and the
  diagnostic implementations converged independently on the same
  five failure classes, which is the strongest evidence available
  that the failure classes are real and the rubric's anchors are
  the right ones.
- **2026-05-20 — Criterion 4 (Escape hatches) Prisma-trap runbook
  added; Vocab Governance Rules section added; three new
  boundary-violation lines.** No weight redistribution — purely
  additive enforcement layer for vocab proposals. Forward-only: past
  PASS verdicts unaffected; one past APPROVE (`ai-primitives-v0`
  second pass) annotated retroactively as "deferred Cut B would have
  failed RULE-VOCAB-03; the deferral was correct." Triggered by
  the canonical pilot vocab-saturation analysis (72 handlers, ~50% absorbable
  by safe vocab; line drawn at `flow`). Source proposal:
  `c:/Users/lucas/lazuli-ops/docs/proposals/grader-vocab-governance.md`.
- **2026-05-18 — Criterion 11 inserted (Framework error message
  contract, 6%); weights redistributed (C1 −1, C2 −1, C3 −1, C7 −1,
  C10 −2).** Sum stays at 100%. Triggered by the the canonical pilot PWA
  field incident (`runtime/go/lazuli/policy.go:138,146`) where the
  framework's English diagnostic string reached the end user. Source
  proposal:
  `c:/Users/lucas/lazuli-ops/docs/proposals/grader-error-message-criterion.md`
  (self-graded PASS 9.16; architect re-grade pending). Forward-only
  hardening: no past PASS retroactively becomes BLOCK. Past PASS
  verdicts at the boundary (e.g., the 9.02 from Wave R) may degrade
  to "PASS with notes" with C11 logged as a tracked cut — see §5 of
  the source proposal for the estimate. Criterion 2 + 8.5 + 10 + 11
  = 17 + 3 + 9 + 6 = 35%, equal to the AI-first ceiling under the
  open-questions section.
- **2026-05-17 — Criterion 8.5 inserted; AI-first weight 14% → 11%.**
  Architect grade PASS 8.9/10 per
  The 3% redirect from Criterion 10 to 8.5 stays within the AI-first
  axis (Criterion 2 + 8.5 + 10 = 32%, invariant before/after);
  Criterion 10 still owns the subjective AI-first signal while 8.5
  owns the objective grep gate. Self-recursive: this rubric edit
  doesn't introduce diagnostic codes, so Criterion 8.5 is n/a here.

## Open questions

- **Should there be a criterion for "migration / backward
  compat"?** Today the rubric weighs migration through Criterion 5
  (determinism — by ensuring there's one canonical form,
  migrations stay simple). Real product pressure may justify a
  dedicated axis. Defer until ≥ 3 cuts produce migration debt.
- **Should the AI-first weight grow?** Currently 7% (Criterion
  10) plus 17% on semantic density (Criterion 2) plus 3% on
  Criterion 8.5 plus 6% on Criterion 11 = 33% weighted on AI-first
  concerns (was 35% before the 2026-05-27 redistribution to fund
  C13; C10 paid 1pp into C13 but the cluster stayed inside its 35%
  ceiling). The thesis of Lazuli is AI-first, but pushing past 35%
  would flatten the human-cold-read criterion. The 2026-05-18
  insertion of Criterion 11 took the axis exactly to the 35%
  ceiling; the 2026-05-27 redistribution stepped 1pp back down (now
  at 33%, with 2pp of headroom). Future AI-first additions must
  reclaim weight from inside the axis rather than expand past 35%.
  Defer growth; revisit if real LLM-author tests show systemic
  gaps the rubric misses.
- **Should the rubric be per-construct?** Today it grades the
  whole language. Per-construct grading would let proposals
  target specific axes. Plausible after Cut A's IR migration
  delivers per-construct typed shapes.
- **Should Criterion 11 apply to non-Go runtimes once they
  exist?** Yes, symmetrically, but Probe 1's grep target becomes
  language-conditional (`runtime/<language>/lazuli/`). Capture in
  the criterion when the second runtime ships.
- **Should the framework-message namespace be `@translation.*` or
  a sibling `@framework_message.*`?** Recommendation from the
  source proposal: `@translation.framework.<kind>` — same root
  namespace, subnamespace for searchability. Architect to
  ratify when the runtime team lands the migration.

## Reserved

- A separate **runtime grading rubric** for the Go runtime and
  codegen lives outside this document. The language rubric does
  not measure runtime correctness; that is the runtime team's
  discipline.
- An **eval-success rate** metric ("X% of LLM-authored fixtures
  parse and pass doctor on first try") would be the most direct
  AI-first measure. Reserved until evals against the LSP
  diagnostics produce a reliable corpus.

## Manual TODO (post-2026-05-18 rubric bump)

The Criterion 11 insertion is **forward-only hardening**: no past
PASS verdict retroactively flips to BLOCK. The source proposal
(`c:/Users/lucas/lazuli-ops/docs/proposals/grader-error-message-criterion.md`,
§5) lists the following past PASS verdicts that *may* degrade to
"PASS with notes" once C11 is applied retroactively for record-keeping:

- `lazurite-rubric-pass-confirmed-2026-05-17.md` (9.02) — C11 estimate ~3.
  Expected re-graded score ≈ 8.84, **still PASS with notes** (C11=3
  is above the 6-block floor and below the 7-floor → tracked cut).
- `naming-reconciliation-2026-05-17.md` (8.9) — n/a (doesn't touch errors).
  Re-weight alone drops it ≈ 0.1.
- `ai-primitives-v0` (second pass, 8.8) — n/a. Re-weight ≈ unchanged.
- `bucket-i18n-cycle` (graded internally) — C11 estimate ~6. Likely
  PASS with notes.

**TODO** — Lucas / architect: decide whether to (a) re-grade the
listed proposals against the new rubric and update their score
tables in-place, (b) annotate them with a "graded against pre-C11
rubric" stamp, or (c) leave them as historical records and only
apply C11 to net-new grades from 2026-05-18 forward. Option (c) is
the default per the "forward-only hardening" rule above; (a) and (b)
are opt-in audit-trail work.
