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
| 1 | Legibility (cold human read) | 11% | Can a senior dev read 1000+ lines of fixture top-to-bottom without backtracking or doc-lookup? |
| 2 | Semantic density for LLM | 17% | Are `@policy.*`, `@cap.*`, `@semantic.*`, `@actor.*`, `@pii.*`, `@key.*`, `@llm.*`, `@tool.*` namespaces tight, closed, and unambiguous? |
| 3 | Token efficiency | 8% | Is there gordura recorrente? Count tokens of repeated boilerplate × number of repetitions. |
| 4 | Escape hatches | 8% | Can authors drop to `handler "./..."`, `validates resource "./..."`, custom Go without polluting source? Are the hatches minimal and visible? |
| 5 | Determinism (one way to say each thing) | 10% | If the same intent has two surface forms with no rule for choosing, that's a deduction. |
| 6 | Composability | 8% | Do `extends @anchor.*`, `extensible_by`, `packs`, `has_many`, `event_group` combine cleanly? |
| 7 | Multi-target fit (Go/React/Expo) | 7% | Are surface projections (`.web.lzx` / `.mobile.lzx`) clean? Does any contract leak transport mechanics? |
| 8 | Operational coverage | 6% | Do `runtime`, `deploy`, `profiles`, `services`, `architecture` cover real production needs without becoming Kubernetes config? |
| 8.5 | Diagnostic identifier truthfulness | 3% | For every diagnostic code named in a proposal's acceptance lists: does the code (a) exist in `crates/lazuli_cli/src/doctor.rs` or `crates/lazuli_lsp/src/lib.rs`, or (b) explicitly appear under a `## New diagnostics` heading as net-new? Mechanical grep check. See §"How the rubric is enforced" for the runbook. |
| 9 | Declarative testability | 6% | Are `tests` blocks expressive enough for rules / transitions / anchors / commands without becoming a mock framework? |
| 10 | AI-first readiness | 8% | Does the language treat LLMs as first-class consumers (`agent`, namespaces, inspect contracts, doctor messages)? |
| 11 | Framework error message contract | 6% | Are framework-emitted runtime errors (anything that reaches the HTTP wire without passing through an authored `rule "..." message @translation.<key>` block) keyed by a translation identifier under `@translation.<key>` (or equivalent message-namespace identifier), negotiated against the active locale, and override-able by app or feature surface? Hardcoded English in `Message:` fields of `&Error{...}` constructors in the runtime is an automatic 0. See §"Criterion 11 — Framework error message contract (runbook)" below. |
| 12 | Test discipline (per-layer coverage) | 5% | Does `lazuli doctor --coverage` emit a per-layer report (six layers: `spec_predicate`, `spec_actor_matrix`, `spec_transition_state`, `view_extensibility`, `view_e2e_pair`, `handler_go`) with profile-aware thresholds (prototype reports only; strict warns; production blocks)? Per-layer thresholds are canonical; any aggregate is opt-in only with method disclosure. Auto-BLOCK if any layer is below its `block_under` under the active profile, OR if `TEST-FIXTURE-LITERAL-001` errors are present (Wave 1). See §"Criterion 12 — Test discipline + per-layer coverage (runbook)" below. |

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
| 12 | Test discipline (per-layer coverage) | 7.0 | path:line | path:line |

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

### Criterion 8.5 — Diagnostic identifier truthfulness (runbook)

For every diagnostic identifier (anything matching `*_diagnostics`,
`*-001` / `*-002` / ..., or any heading under a "Doctor diagnostics"
list in an acceptance section) named in a proposal, the grader runs:

```bash
rg --no-heading --no-line-number -F '<code>' \
   crates/lazuli_cli/src/doctor.rs \
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
   crates/lazuli_cli/src/doctor.rs crates/lazuli_doctor/src/

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

Triggered by the the canonical pilot PWA framework-string leak (2026-05-18). See
proposal internal review notes
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

**Layer catalog (canonical six):**

| Layer | Denominator | Numerator | Source |
|---|---|---|---|
| `spec_predicate` | Predicate branches in `requires` / `rule.when` / transition predicates | Branches with `allows when` + `denies when` covering each side | IR walk |
| `spec_actor_matrix` | `(construct, @role.X)` pairs derived from `policy @policy.X` | Pairs touched by `permits`/`forbids` or `allows as`/`denies as` rows | IR walk |
| `spec_transition_state` | `from <state>` slots in workflow + lifecycle transitions | Slots with ≥1 `allows from` (DeniesFrom alone is not sufficient) | IR walk |
| `view_extensibility` | Views with `extensible_by` | Views with ≥1 `accepted by` / `rejected by` | `.lzx` walk |
| `view_e2e_pair` | Declared views | Views with `e2e/<feature>/<view>.spec.ts` present | filesystem |
| `handler_go` | Statements in `app/features/<f>/handlers/*.go` (excluding `_test.go`) | Statements with `count > 0` in `coverage.out` | `go test -coverprofile` parse |

Layers 1–4 are pure-IR (zero runtime, zero flakiness). Layers 5–6 are
filesystem / external-tool integrations that degrade gracefully when
their inputs are absent (vacuous pass with disclosure in `raw_file`).

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
(`docs/proposals/test-completeness-lints.md` §1) — `tests` slot
present, zero enforcement. Wave 6 closes the loop by making the
coverage observable + gateable + reportable in the same JSON shape
agents and CI already consume. Provenance:
`docs/proposals/tdd-bdd-first-2026-05-23.md` Wave 6.

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
   docs/proposals/<proposal>.md
rg -n 'handler.*should|fn.*deprecated|imperative.*warning' \
   crates/lazuli_cli/src/doctor.rs crates/lazuli_doctor/src/
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

Triggered by the the canonical pilot vocab-saturation analysis (2026-05-20). See
proposal internal review notes
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

- **2026-05-20 — Criterion 4 (Escape hatches) Prisma-trap runbook
  added; Vocab Governance Rules section added; three new
  boundary-violation lines.** No weight redistribution — purely
  additive enforcement layer for vocab proposals. Forward-only: past
  PASS verdicts unaffected; one past APPROVE (`ai-primitives-v0`
  second pass) annotated retroactively as "deferred Cut B would have
  failed RULE-VOCAB-03; the deferral was correct." Triggered by
  the canonical pilot vocab-saturation analysis (72 handlers, ~50% absorbable
  by safe vocab; line drawn at `flow`). Source proposal:
- **2026-05-18 — Criterion 11 inserted (Framework error message
  contract, 6%); weights redistributed (C1 −1, C2 −1, C3 −1, C7 −1,
  C10 −2).** Sum stays at 100%. Triggered by the the canonical pilot PWA
  field incident (`runtime/go/lazuli/policy.go:138,146`) where the
  framework's English diagnostic string reached the end user. Source
  proposal: internal review notes
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
- **Should the AI-first weight grow?** Currently 9% (Criterion
  10) plus 17% on semantic density (Criterion 2) plus 3% on
  Criterion 8.5 plus 6% on Criterion 11 = 35% weighted on AI-first
  concerns. The thesis of Lazuli is AI-first, but pushing past 35%
  would flatten the human-cold-read criterion. The 2026-05-18
  insertion of Criterion 11 took the axis exactly to the 35%
  ceiling; future AI-first additions must reclaim weight from
  inside the axis rather than expand it. Defer growth; revisit if
  real LLM-author tests show systemic gaps the rubric misses.
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
(internal review notes,
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
