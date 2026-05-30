# Customer v2 — Side-by-Side Comparison

This document compares the current `feature customer`
(`examples/full-capsule/full-capsule.lzi`) against four ergonomic
experiments raised in earlier critique. Each block carries a verdict:

- **PROMOTE** — the change improves the language without breaking the
  architect-graded coherence axis.
- **REJECT** — the change diverges from the established style and
  loses something measurable.
- **DEFER** — the idea has merit but needs evidence from real usage
  pressure before promotion (`docs/capability-layering.md` lifecycle).

Verdicts reflect the second-pass grade by `lazuli-language-architect`
on the AI-primitives proposal. Where this conflicts with my first
critique, the architect's discipline wins.

## Experiment 1 — `delegates` flat shape

### v1 (current)

```lazuli
non_goals
  delegated_to
    user: "staff authentication"
    customer_auth: "customer login and MFA"
    customer_tags: "tag management"
    customer_import: "CSV import and external CRM ingestion"
    billing: "invoicing"
```

### v2 (proposed in first critique)

```lazuli
delegates
  auth -> customer_auth
  tags -> customer_tags
  imports -> customer_import
  invoicing -> billing
```

### Verdict: REJECT

Three reasons make the v1 shape better:

1. **Two distinct semantics.** `non_goals > delegated_to` lives next to
   `non_goals > out_of_scope`. They serve different roles: `delegated_to`
   says "another feature owns this concern"; `out_of_scope` says "no one
   in this product owns this." Flattening to `delegates` collapses one
   axis and forces `out_of_scope` into a sibling block, doubling the
   keyword surface.
2. **Free-text justification is load-bearing.** `customer_import: "CSV
   import and external CRM ingestion"` is a *contract for the agent
   reader*. The string reduces hallucination ("oh, customer_import is
   the CSV thing"). A bare `imports -> customer_import` drops that
   anchor.
3. **The arrow `->` is not in the language.** Every other reference
   today resolves through `:` or feature-qualified dotted paths. Adding
   `->` for one block introduces a parser shape used nowhere else.

The original critique under-weighted (1) and (2) and over-weighted
visual brevity. Keep v1 as canonical.

## Experiment 2 — Audience inline on view

### v1 (current, in `.lzx`)

```lazuli
surface customer web
  uses experience customer
  audience admin
    view list Table
      columns name, email, lifecycle_stage, tier, score, owner, created_at
```

### v2 (proposed in first critique)

```lazuli
view customer.list Table @admin @web
  columns name, email, lifecycle_stage, tier, score, owner, created_at
```

### Verdict: DEFER

The concern is real: five levels of indent (surface > audience > view >
columns > each-column) is dense. But the proposed flattening has two
costs:

1. **Loses the bilateral platform/audience composition.** `surface
   customer web` + `audience admin` isolate platform-specific decisions
   from audience-specific ones. The flat form merges both into a single
   line of `@` annotations, which makes diffs harder when only one
   axis changes.
2. **`@admin` would compete with the closed namespace catalog.**
   `@actor.admin`, `@role.admin`, and `@scope.admin` already exist.
   Adding `@admin` as an audience marker creates a fourth meaning for
   the same name with no namespace prefix.

The v2 idea would work if reframed as a `view` declaration that
*explicitly* references the surface (and audience) by name —
`view customer.list at surface.web.admin Table` — but that adds
keywords without removing them.

**Defer**. Revisit only if a real product capsule shows the deep
indent producing concrete authoring pain across 10+ surfaces, and the
new form preserves the platform/audience split.

## Experiment 3 — Optional `end` terminator

### v1 (current)

```lazuli
command create
  input
    name: Text required
    email: @semantic.Email required
  policy @policy.author
  rate_limit "10 per hour per ip"
  creates Customer
    name = input.name
    email = input.email
  emits customer_created from creates
```

### v2 (proposed in first critique)

```lazuli
command create
  input
    name: Text required
    email: @semantic.Email required
  end input

  policy @policy.author
  rate_limit "10 per hour per ip"

  creates Customer
    name = input.name
    email = input.email
  end creates

  emits customer_created from creates
end command
```

### Verdict: REJECT

`lazuli fmt` already eliminates the underlying concern. Indentation
errors in agent-generated source are a *formatter* problem, not a
*language* problem. Adding `end` blocks pollutes the canonical surface
with optional decoration that authors will inevitably use
inconsistently.

The first critique cited "LLM gets confused in 800-line files with
6-level indent". This was correct for *parsers*, not for the present
language: the parser already tracks INDENT/DEDENT virtually
(`docs/grammar.lzi.md §1.2`), and `lazuli fmt --check` rejects ill-
formed indentation before the file lands. Where `end` would help is
*human cold-reading* of large files — but the canonical-semantics
guidance is to *split such features* (`feature customer_auth`,
`feature customer_tags`, etc.), not to terminate-mark them.

The right tool for the human reading concern is `lazuli inspect
--expand=summary`, not `end`. **Reject.**

## Experiment 4 — Compact `previously migrated` to `was`

### v1 (current)

```lazuli
resource Customer
  previously migrated Account
  ...
  lifecycle_stage: CustomerStatus = lead
    previously migrated status
```

### v2 (proposed in first critique)

```lazuli
resource Customer was Account
  ...
  lifecycle_stage was status: CustomerStatus = lead
```

### Verdict: REJECT

Three problems:

1. **`previously migrated` and `previously alias` are distinct.**
   `migrated` says "the IR baseline knows the old name"; `alias` says
   "the source can be referenced by either name during a transition
   window." Compacting both to `was` drops the distinction.
2. **`was` collides with English semantics.** `name was first_name`
   reads as "the field was previously called first_name", which is
   correct, but `lifecycle_stage was status: CustomerStatus = lead`
   reads as "lifecycle_stage was status (a CustomerStatus)" — the
   reader's eye treats `was` as a copula, not a rename marker.
3. **Inline `previously` was already considered and rejected.**
   `docs/invariants.md:127-131` records the decision: *"`previously
   migrated|alias <old>` is a child of the block it migrates, not
   inline on the header line… keep one concept per line."* The first
   critique missed this decision.

**Reject.** Token cost of `previously migrated` (3 words on a
dedicated line) is bearable for a temporary contract that will be
removed once the IR baseline ages.

## Experiment 5 — Apply Cut A AI primitives to `summarize_customer`

This experiment IS approved. Showing it for completeness — this is
the meaningful "v2" that should ship with Cut A.

### v1 (current)

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
    prompt: Text required
  context customer.query.by_id(id: input.customer_id)
  policy @policy.view
  rate_limit "20 per hour per user"
  output stream Text
  model @llm.default
  temperature 0.2
  max_tokens 1024
  top_p 0.9
  prompt "./prompts/summarize_customer.md"
  safety @validator.pii_scrub
```

### v2 (Cut A)

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
    prompt: Text required
  context customer.query.by_id(id: input.customer_id)
  policy @policy.view
  rate_limit "20 per hour per user"
  output stream Text
  model @llm.default
  temperature 0
  seed 1
  max_tokens 1024
  top_p 0.9
  prompt "./prompts/summarize_customer.md"
  safety @validator.pii_scrub
  tools
    customer.query.by_id
    customer.query.list
  evals
    case short_for_active
      requires customer.lifecycle_stage = active
      requires output.length < 800
      requires output contains "active"
    case redacts_email
      requires customer.email = "ada@example.com"
      forbids output contains @semantic.Email
```

### Verdict: PROMOTE (with Cut A)

This is what the proposal at `docs/proposals/ai-primitives-v0.md`
ships. Three additions:

1. **`tools`** declares the dispatch graph that doctor cross-checks
   against the agent's policy. Closes the existing invariant promise.
2. **`temperature 0` + `seed 1`** make the agent's evals
   gating-eligible per Section A3 of the proposal. The 0.2 in v1
   silently defeats CI gating.
3. **`evals` with two cases** demonstrates the predicate-language
   extension (`forbids output contains @semantic.Email`) in real
   context. `lazuli test --evals` runs them; `lazuli check` validates
   the predicates and emits `eval_nondeterministic_warning` if the
   determinism gate were not met.

This is the only experiment with a clear "PROMOTE" — the others teach
that several first-pass intuitions about ergonomics did not survive
contact with the project's existing decisions.

## Summary

| # | Experiment | Verdict |
|---|---|---|
| 1 | `delegates` flat shape | REJECT |
| 2 | Audience inline on view | DEFER |
| 3 | Optional `end` terminator | REJECT |
| 4 | `was` for `previously migrated` | REJECT |
| 5 | Cut A AI primitives on `summarize_customer` | PROMOTE |

**Net**: of five ergonomic experiments raised in first critique, four
were rejected on review against the project's prior decisions, one
deferred, and one promoted. The lesson is the architect's discipline
in `docs/design-decisions.md`: most apparent friction is an
architectural choice with stated justification, not a deduction-worthy
duplication. Audit the decisions doc before proposing surface-syntax
changes.

The genuine wins from the broader review (closing `tools`, adding
discriminated `output`, `evals` with the determinism gate) live in
`docs/proposals/ai-primitives-v0.md`, not in this comparison.
