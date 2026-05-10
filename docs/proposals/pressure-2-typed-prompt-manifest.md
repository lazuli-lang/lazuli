# Audit: Pressure 2 — Typed Prompt Manifest

**Status**: Exploratory audit. Not a proposal yet. Surfaces three
design approaches with concrete syntax, IR shape, and tradeoffs.
Recommends one direction with an honest assessment of why and what
the cost is.

**Source**: Pressure 2 from `docs/proposals/ai-first-roadmap.md` —
"`prompt \"./path.md\"` is opaque to Lazuli."

## The problem

Today's `agent` declaration includes:

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
    prompt: Text required
  context customer.query.by_id(id: input.customer_id)
  ...
  prompt "./prompts/summarize_customer.md"
```

The prompt file `./prompts/summarize_customer.md` references
variables that map to the agent's `input.*`, `context.*`, etc.
slots. The template syntax is whatever the runtime/provider
expects (Mustache, jinja2, raw string interpolation).

Lazuli does not parse the file. Three concrete consequences:

1. **Silent breakage on refactor**. Remove `input.prompt` from the
   agent; the prompt file still references `{{ input.prompt }}`;
   doctor passes; runtime fails.
2. **LLM authoring blind spot**. An LLM authoring a prompt for an
   agent cannot consult Lazuli's `inspect` to learn which
   variables are in scope. It guesses, then types
   `{{ customer.foobarbaz }}` and the runtime fails.
3. **PII leakage invisible**. A prompt referencing
   `{{ customer.email }}` is consuming an `@pii.contact` field. If
   the agent declares no `safety` validator, this should warn —
   but Lazuli doesn't see the reference.

Real AI products iterate prompts faster than schema. The cost of
silent breakage compounds over time.

## What "typed prompt manifest" could mean

Three approaches, each with concrete syntax and IR delta.

### Approach A — Sidecar manifest

Each prompt has a companion `.prompt.lzi` file that declares
variables. The template body lives in the existing markdown file
(or wherever). Lazuli parses the sidecar; the body stays opaque.

#### Syntax

```lazuli
# prompts/summarize_customer.prompt.lzi
prompt summarize_customer
  body "./summarize_customer.md"
  vars
    customer_name: Text required
    customer_email: @semantic.Email @pii.contact required
    customer_tier: Text required
    user_instruction: Text required
```

The agent references the prompt by name:

```lazuli
agent summarize_customer
  ...
  prompt @prompt.summarize_customer
```

#### IR shape

```rust
pub struct PromptManifest {
    pub name: String,
    pub body_path: String,
    pub vars: Vec<TypedSlot>,
    pub span_ref: Option<SpanRef>,
}
```

A new namespace `@prompt.*` joins the closed catalog.

#### Doctor checks

- `agent.prompt_ref` resolves to a known prompt.
- Each prompt var has a binding from agent `input.*` /
  `context.*` / `target.*` / `ctx.*`. The binding is *implicit by
  name* — the agent provides `customer_name` from `context.customer.
  name`; doctor walks the lookup. **This is the hardest check** and
  raises sub-questions (see below).
- PII propagation: if a var carries `@pii.<class>`, the agent must
  declare `safety` covering that class (composes with Cut A.5).

#### Cost

- One extra file per prompt.
- A new `@prompt.*` namespace.
- New parser surface (small: `prompt ... body ... vars { ... }`).
- New doctor diagnostic family.

### Approach B — Inline declaration on the agent

The agent's `prompt` clause expands into a block that declares
the body path *and* the expected variables.

#### Syntax

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
    user_instruction: Text required
  context customer.query.by_id(id: input.customer_id)
  ...
  prompt
    body "./prompts/summarize_customer.md"
    vars
      customer_name = context.customer.name
      customer_email = context.customer.email
      customer_tier = context.customer.tier
      user_instruction = input.user_instruction
```

The agent **explicitly** maps each variable to a source expression
in scope.

#### IR shape

Extend `Agent` (Cut A's IR):

```rust
pub struct Agent {
    // existing fields...
    pub prompt: PromptBinding,
}

pub enum PromptBinding {
    Inline(String),                    // legacy: prompt "./path.md"
    Typed(TypedPromptBinding),
}

pub struct TypedPromptBinding {
    pub body_path: String,
    pub vars: Vec<PromptVarBinding>,
}

pub struct PromptVarBinding {
    pub name: String,
    pub source: SourceExpr,  // reuses existing IR expression nodes
    pub span_ref: Option<SpanRef>,
}
```

#### Doctor checks

- Each `vars` source expression resolves (exists in scope).
- The source expression's type matches the var's expected type
  (when types are derivable).
- PII propagation: same as A, composes with Cut A.5.
- No name-resolution magic: the author writes the binding.

#### Cost

- Per-agent verbosity goes up. For prompts with 8 variables, the
  block adds ~10 lines.
- No new file, no new namespace.
- Extends `Agent` IR (additive).

### Approach C — Pack-only

Language ignores prompt typing entirely. A pack (`prompt_pack` or
`rag_pack`) provides:

- A convention (sidecar `.prompt.json` schema).
- Doctor rules contributed by the pack.
- Optional codegen that produces typed binding wrappers.

#### IR shape

Zero language changes. The pack carries its own contract.

#### Cost

- No language complexity added.
- The check fires only for projects that opt into the pack.
- Different packs may compete with different prompt-typing
  conventions; canonical form is lost.

### Approach D — Embedded/fenced prompt body

The prompt body lives **inline** in the agent source, as a fenced
or heredoc block. Variables in the body are part of the same
parsing pass; the body is no longer opaque to Lazuli.

#### Syntax

```lazuli
agent summarize_customer
  ...
  prompt
    body """
      Summarize this customer for the support team.
      Name: {{ context.customer.name }}
      Email: {{ context.customer.email }}
      User instruction: {{ input.user_instruction }}
    """
```

#### Why rejected

Three reasons:

1. **Source bloat**. Prompts grow to dozens of lines with
   examples, instructions, formatting rules. Embedding them in
   `.lzi` source mixes two concerns (configuration + content)
   and burns LLM-context budget every time `lazuli inspect`
   reads the feature.
2. **Editor support**. Prompts often need markdown rendering,
   syntax highlighting for embedded code blocks, and prompt-
   specific tooling (token counting, jinja preview). Embedding
   in `.lzi` source defeats the standalone-file workflow.
3. **Template-syntax decision forced**. Approach D requires
   committing to one template syntax (Mustache, jinja2, etc.) at
   the language level. The standalone-file approaches (A, B, C)
   leave that decision to the runtime/adapter.

Approach D is conceptually clean — it eliminates the body/manifest
gap (S1) entirely. The cost-benefit doesn't justify it. Prompts
are first-class artifacts; treat them like one.

## Sub-questions surfaced by every approach

These cut across A/B/C:

### S1 — Which template syntax does the body use?

Lazuli doesn't have to commit to one. The runtime / adapter
decides whether the body uses Mustache, jinja2, `${var}`,
provider-specific templating, etc. The language only sees the
*declared* variables; substitution stays in the runtime.

Doctor cannot check that the body actually uses every declared
var or only declared vars (without parsing the body). This is a
known gap; the alternative (parsing arbitrary template syntaxes)
explodes the language surface.

### S2 — Implicit name resolution vs explicit binding?

In Approach A, the agent declares no explicit binding; doctor
infers — `prompt vars { customer_name }` resolves to
`context.customer.name` by name match (or fails). This is
**convenient for the author but fragile**: a rename in the source
silently changes which slot binds.

In Approach B, the agent **explicitly** writes
`customer_name = context.customer.name`. Verbose but explicit; no
magic resolution.

The architect's discipline (`docs/design-decisions.md`) favors
explicit. Decision: if A is chosen, *both* the prompt sidecar and
the agent's `prompt @prompt.X` reference must include an explicit
binding block. Otherwise A collapses into an implicit-resolution
trap.

### S3 — PII propagation

All three approaches can compose with Cut A.5's PII coverage
check. When a prompt var carries `@pii.<class>`, that class joins
the agent's tool-resolved PII classes; doctor checks coverage
union vs declared `safety` validators.

This is purely additive: Cut A.5 already walks
`Agent.tools[].resolved_pii_classes`; extending it to walk
`Agent.prompt.vars[].pii_classes` is one extra line in the
propagation function.

### S5 — Tool result schema composition

Cut A.6 (`docs/proposals/ai-primitives-cut-a-6.md`) declares
`@tool.*` result schemas. A prompt body may reasonably reference
`{{ tools.web_search.snippet }}` to inject a tool result. The
typed-prompt approach must let the binding block compose with
tool results:

```lazuli
prompt
  body "./prompts/research_summary.md"
  vars
    headline = tools.web_search.title
    snippet  = tools.web_search.snippet
    customer_name = context.customer.name
```

This requires:

- The agent's tool list (Cut A) to be in scope at binding time.
- The tool's result record (Cut A.6) to provide the field set
  doctor walks for validation.

**Sequencing implication**: prompt typing should land AFTER Cut
A.6, not in parallel. Without `RegistryToolEntry.result_record`,
the bindings above have no field set to check, and the proposal
re-spells the binding shape later when Cut A.6 lands. One-cycle
sequencing wins.

### S4 — Migration cost

Today: ~50 fixtures across the project reference prompts via
`prompt "./path.md"`. Migration:

- **Approach A**: every prompt file gets a sidecar `.prompt.lzi`.
  Every agent changes from `prompt "./path.md"` to `prompt
  @prompt.<name>`. Two file edits per prompt.
- **Approach B**: every agent changes from `prompt "./path.md"`
  to the expanded block form. One file edit per prompt, but per-
  agent verbosity grows.
- **Approach C**: no migration. Pack-only.

Doctor can ship in *warning mode* under both A and B for one cut,
allowing teams to migrate without breaking CI, then escalate to
default error.

## Recommendation

**Approach B (inline typed binding) with the legacy untyped form
preserved indefinitely.**

Three reasons:

1. **No new namespace, no new file convention.** A's `@prompt.*`
   namespace and sidecar files add real complexity without
   proportional value. B reuses what's there.
2. **Explicit binding matches the language's discipline.** The
   author writes `customer_name = context.customer.name`; no
   resolution magic.
3. **Backward compatibility**. Legacy `prompt "./path.md"`
   remains valid. New agents adopt the typed form when they want
   the doctor check. No forced migration.

The cost is per-agent verbosity. For agents with > 8 variables,
the prompt block can be 10+ lines. This is acceptable: the same
agents would have hidden bugs without it. Doctor checks earn
their tokens.

C is rejected: the AI-first thesis says the language should treat
LLMs as first-class consumers. Outsourcing prompt typing to a
pack means the language doesn't help LLMs author prompts — exactly
the bar this project sets itself against.

## What Approach B does NOT solve

- The body's template syntax is still opaque. If the body
  references `{{ customer_nme }}` (typo), doctor can't see it.
  This is fundamental to leaving template parsing out of scope.
- Multi-language prompts (English + Portuguese variants) require
  multiple prompt declarations or a runtime-side abstraction.
  Out of scope for this audit.
- Few-shot example management. Some prompts include in-context
  examples that vary per tenant. Out of scope; that's a separate
  composition problem.

## Promotion path

If Approach B is endorsed, the proposal lifecycle is:

1. **Audit** (this document).
2. **Proposal** (`docs/proposals/ai-primitives-cut-?.md`).
   Approval-likely number: F (after E). The cut sits in the AI-
   primitives sequence after the pilot-gated D and E because it
   touches every agent.
3. **Pilot evidence (evidence-shaped, not pressure-shaped)**: a
   code review surfaces a prompt whose variable list drifted from
   the agent's `input` / `context` and **shipped to production**
   before the runtime failure was caught. The drift-shipped-to-
   prod evidence is the gate, not "any prompt could in principle
   break." This mirrors the Cut A.5 evidence-shape discipline:
   real anti-pattern review activity, not theoretical risk.
4. **Implementation plan**: phased like Cut A.7 (parser → IR →
   doctor → LSP → fixture → docs). Should be small (~6 days for
   one engineer) because the change is additive.

## Coordination

This audit overlaps Cut A.5 (PII coverage) — typed prompts that
declare PII classes extend the same coverage check. The two cuts
should not be mutually blocking but should land in a coordinated
sequence:

- Cut A.5 lands first (closes Cut A's safety-coverage gap).
- Prompt typing (this audit's recommended Approach B) lands when
  pilot evidence justifies it; the PII propagation extends Cut
  A.5's existing pass.

## Open questions for the audit

- **Q-P2-1**: should `vars` accept default values? Today's `input`
  slots accept `= <default>`; should `prompt vars`?
  Recommendation: yes, for parity with `input`.
- **Q-P2-2**: what's the type catalog for `vars`? All scalars and
  semantic types work cleanly. Records and resources need a
  "serialize to template-string" contract per type, which the
  language doesn't have. **Concretely** under a scalars-only v1:
  the canonical fixture's `summarize_customer` agent would be
  *partially* in scope — `customer.name`, `customer.email`,
  `customer.tier` are scalars; `customer` as a whole is not.
  Authors model the projection explicitly:
  `customer_name = context.customer.name` (in-scope) rather than
  `customer = context.customer` (out-of-scope under v1). Real
  fixtures with `Vec<Resource>` context slots (Pressure 5 / Cut
  D) are out-of-scope under v1 entirely; those agents either
  defer typed prompts or accept individual scalar projections.
  Decision: scalars and semantic types in v1; revisit when Cut D
  lands.
- **Q-P2-3**: should `vars` accept expressions beyond simple
  refs? `customer_name = context.customer.first_name + " " +
  context.customer.last_name` introduces an expression language
  that Lazuli doesn't have on the value side. Recommendation:
  scalar refs only in v1; expressions are a follow-on.
- **Q-P2-4**: should the language know about *response* templates
  too? Some AI products have a structured-output template (JSON
  schema for the agent's response). Cut A's discriminated
  `output` already handles enum-shaped responses; record-shaped
  responses are inferred from the record IR. Prompt-side
  templating is the gap, not response-side.

- **Q-P2-5**: tool result composition. A prompt may reference
  `{{ tools.web_search.snippet }}`. The binding block must accept
  `headline = tools.<tool_name>.<field>` where the right-hand-
  side resolves through Cut A.6's `RegistryToolEntry.result_record`.
  Decision: yes; this is part of the typed-prompt contract. The
  proposal that promotes this audit must land after Cut A.6, not
  before.

## Graduation timing

Even with Approach B endorsed, the proposal should **not** land
until after Cut A.6 (tool result schema). Two reasons:

1. **Composition substrate**: typed prompt bindings naturally
   compose with tool result fields (S5 / Q-P2-5). Without Cut
   A.6's `RegistryToolEntry.result_record` in IR, the binding
   shape must re-spell tool result lookup, which forces a
   migration when Cut A.6 lands.
2. **Pilot evidence shape**: the evidence-gate (a prompt's
   variable list drifting *and shipping to prod*) only fires for
   teams already using Cut A's tools surface. That cohort is the
   one whose prompts are most likely to reference tool results;
   if Cut A.6 hasn't landed, those references can't be typed at
   all, and the proposal lands with a known gap.

Sequencing implication: in the master sequence document, Pressure
2 promotion sits after Cut A.6 (and possibly after Cut D if
multi-slot context emerges as a precondition for richer
prompts).

## Final note

This audit deliberately does not produce a proposal. The next
step is for the user to:

1. Decide whether Approach B is the right shape.
2. Decide whether the evidence gate (drift shipped to production)
   is acceptable or whether pre-emptive landing is justified.
3. Promote to proposal **after Cut A.6 lands**, not before.

The audit's value is making the design space visible. The
proposal's value is locking the contract.
