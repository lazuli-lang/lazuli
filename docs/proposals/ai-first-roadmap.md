# AI-First Frontier Audit

**Status**: Exploratory roadmap. Surfaces candidate AI-first
primitives beyond Cut A and Cut B of the proposal in
`docs/proposals/ai-primitives-v0.md`. Ranks by leverage × architect-
approval-likelihood. Each candidate gets a short proposal sketch and
a recommendation (proposal-ready / pack candidate / pilot-gated /
red herring).

**Method**: cold-read of `examples/full-capsule/`,
`docs/language-backlog.md`, `docs/capability-layering.md`, and
`docs/canonical-semantics.md`. Looked for:

1. Places where AI-shaped behavior is forced into existing constructs
   (boilerplate signal).
2. Open backlog items with AI implications.
3. Capability classifications that defer AI items (pack vs
   language-light).
4. Real production AI-product needs the project hasn't named yet.

## Pressure points found in the codebase

### Pressure 1 — duplicated `api` next to `agent`

`examples/full-capsule/full-capsule.lzi:305-329` declares
`api customer_summary_stream` and `agent summarize_customer` as
siblings, with matching `policy`, `rate_limit`, `output stream Text`,
and `input { prompt: Text required }`. The api delegates to
`"./api/stream_customer_summary.go"`, which presumably calls the
agent. **The api is boilerplate**: a handler that exists only because
agents do not auto-expose over HTTP.

### Pressure 2 — `prompt "./path.md"` is opaque to Lazuli

The agent's prompt file uses template variables (`{{ customer.name }}`)
that map to `input.*` and `context.*` slots, but Lazuli does not
parse the file. There is no static guarantee that
`./prompts/summarize_customer.md` references variables that the agent
declares, nor that it does *not* reference variables the agent omits.
Real AI products iterate prompts faster than schema; the language
should know what the prompt expects.

### Pressure 3 — agent observability is unstructured

`event.trace <name>` exists for observability events. Agent runs are
the most observed surface in any AI product (tokens, duration, model,
cost, tool calls per turn). Today every product invents the same
event_group and rebuilds the same dashboards. There is no canonical
agent-run trace event shape.

### Pressure 4 — `@tool.*` registry has no result-schema declaration

Cut A's `RegistryToolEntry` carries `effect` and optional
`pii_classes`. It does not carry the *shape* of what the tool
returns. For first-party tools, the returning resource/record
encodes the shape implicitly. For adapter tools (`@tool.web_search`,
`@tool.calendar.create_event`), the shape is unknown to Lazuli.
Doctor cannot check that an agent's `prompt` references fields that
exist on tool results.

### Pressure 5 — `context` accepts only one expression

Today: `context customer.query.by_id(id: input.customer_id)`. A
single target expression. If the agent needs multiple contexts
(customer + recent invoices + tickets), the author writes a single
SQL query that joins them, or does the join in the prompt template,
or in a custom function. None of those are checkable.

### Pressure 6 — agent ↔ contract binding is implicit

`examples/full-capsule/contracts/ai.lzi` declares `record
CustomerSummaryRequest`, `record CustomerSummaryResult`, and
`operation summarize_customer` over HTTP. `agent summarize_customer`
in the feature file declares the same shape independently. The two
are not linked. A team using a remote AI service has to re-declare
its contract twice.

### Pressure 7 — async / background agent

Agents today are dispatched synchronously from a `command`, an `api`
handler, or a manual call. Many real AI workflows are background:
"summarize all customers nightly", "classify all incoming tickets in
a queue". There is no syntax for `job` + `agent` composition; you
write a `job` whose handler calls into the agent.

### Pressure 8 — agent caching

`query.list` and `query.lookup` accept a `cache key STRING ttl
DURATION` block. Agent calls are far more cache-worthy (LLM
dispatch is expensive). There is no equivalent on `agent`.

### Pressure 9 — multi-modal output

`output stream Text`, `output <Record>`, `output discriminator <Enum>`
cover text and structured outputs. Image / audio / structured-plus-
text cases require manual modeling. Most products today need at most
text + structured; multi-modal is genuinely on the horizon for
generative-product use cases.

### Pressure 10 — prompt-version rollout / canary

Real AI products iterate prompts faster than they iterate code. A
new prompt version is a real production change but has no
safety net: no canary, no blue/green, no rollback signal. There is
no syntax for "use prompt v12 for 20% of requests, v11 for 80%."

## Candidate primitives — ranked

Ranking weights:
- **Leverage** (1–10): how many products would benefit / how much
  boilerplate it removes / how strong the static-check story is.
- **Approval likelihood** (1–10): probability the architect would
  say "ship it" given existing principles in `design-principles.md`,
  `capability-layering.md`, and the second-pass grade behavior.
- **Dependency**: what must land first.

### Tier 1 — proposal-ready (ship after Cut A)

#### A — Agent HTTP exposure (auto-mount)

| Field | Value |
|---|---|
| Pressure | 1 |
| Leverage | 9 |
| Approval | 9 |
| Layer | language |
| Dependency | Cut A IR |

**Solves**: removes `api customer_summary_stream`-style boilerplate
when an agent is exposed over HTTP. The api block becomes implicit.

**Sketch**:

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
    prompt: Text required
  context customer.query.by_id(id: input.customer_id)
  policy @policy.read
  rate_limit "20 per hour per user"
  output stream Text
  model @llm.default
  prompt "./prompts/summarize_customer.md"
  safety @validator.pii_scrub
  expose http
    method POST
    path "/api/customers/:customer_id/summary"
```

**Doctor**: `agent_expose_path_conflict_diagnostics` if the path
collides with another agent or `api`.

**IR**: optional `Agent.expose_http: Option<HttpExposure>` field.
Single field; HttpExposure mirrors `api` shape minus the handler.

**Why it would land**: closes a real boilerplate gap (verified in the
fixture). Architect's bar for layer placement: this is *static*
information the runtime needs to mount the endpoint, and it cannot
be expressed as a pack without re-creating the api/agent split.

#### B — `agent_run` trace event (auto-emitted)

| Field | Value |
|---|---|
| Pressure | 3 |
| Leverage | 8 |
| Approval | 9 |
| Layer | language-light + runtime |
| Dependency | Cut A IR |

**Solves**: every product gets the same canonical observability event
without re-modeling.

**Sketch**: a built-in trace event emitted per agent dispatch:

```text
# implicit, no source declaration; surfaces in inspect --expand=events
event.trace agent_run
  payload
    agent: Text required          # "<feature>.<agent_name>"
    model: Text required          # @llm.* resolved
    tokens_input: Integer required
    tokens_output: Integer required
    cost_usd: @semantic.Money optional
    duration_ms: Integer required
    tool_calls: Vec<ToolCall>     # name + duration + status
    finish_reason: Text required
```

**Layer**: language-light contract (the event shape and the fact it
fires); runtime owns the actual instrumentation; adapters export to
OpenTelemetry / Datadog / etc.

**Why it would land**: matches the existing `event.trace` surface and
the `tracing` row in `capability-layering.md:246` (language-light +
runtime + adapter). Every product needs this; modeling it as
language-light prevents 50 reinventions.

#### C — `@tool.*` registry result-schema declaration

| Field | Value |
|---|---|
| Pressure | 4 |
| Leverage | 7 |
| Approval | 9 |
| Layer | language |
| Dependency | Cut A registry-side IR |

**Solves**: doctor can check that prompt references to `tools.<x>.<field>`
resolve.

**Sketch**:

```lazuli
# in registry.lzi
tool web_search
  effect read
  pii_class behavioral
  result Record
    title: Text required
    url: Text required
    snippet: Text required

tool calendar.create_event
  effect write
  result Record
    event_id: ID required
    confirmation_link: Text required
```

**Doctor**: extend `agent_tool_pii_unsafetied_warning` to also
propagate result-schema PII classes.

**IR delta**: extend `RegistryToolEntry` (already in Cut A Phase 2)
with `result_record: Option<RecordRef>`. Trivial additive change.

**Why it would land**: the architect already approved
`RegistryToolEntry.pii_classes`; result schema is the next mile of
the same surface. Costs almost nothing once Cut A's registry-side IR
is in.

### Tier 2 — pack candidate or pilot-gated

#### D — Multi-slot `context` block

| Field | Value |
|---|---|
| Pressure | 5 |
| Leverage | 6 |
| Approval | 7 |
| Layer | language |
| Dependency | Cut A IR |

**Sketch**:

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
  context
    customer = customer.query.by_id(id: input.customer_id)
    recent_invoices = billing.query.invoices_by_customer(customer_id: input.customer_id)
    tickets = support.query.recent_tickets(customer_id: input.customer_id)
```

**Approval risk**: the prompt template needs to reference
`context.customer.name` etc., which adds a path-resolution
requirement to whatever consumes the prompt. Today's single-context
form is `context.<resource_field>` directly; multi-slot would shift to
`context.<slot>.<field>`. Migration risk if existing prompts must
update.

**Recommendation**: defer until a pilot product hits the boilerplate
of single-context. Easy to land later as additive.

#### E — Async agent (`agent` triggered by `job`)

| Field | Value |
|---|---|
| Pressure | 7 |
| Leverage | 6 |
| Approval | 7 |
| Layer | language-light |
| Dependency | Cut A IR |

**Sketch**:

```lazuli
job nightly_summarize_customers
  trigger schedule "0 2 * * *"
  fanout tenants org
  target query.list(status: active)
  dispatches agent.summarize_customer(customer_id: target.id)
  retry 3 backoff exponential
```

**Approval risk**: introduces a new keyword (`dispatches`) for what
is effectively `calls` over an agent target. Could reuse `calls`,
but `calls` today goes to integrations.

**Recommendation**: reuse `calls` with a `agent.<name>` target shape
instead of inventing `dispatches`. Pilot evidence required before
landing — most async-AI cases today fit fine in a manual job
handler.

#### F — Agent caching

| Field | Value |
|---|---|
| Pressure | 8 |
| Leverage | 5 |
| Approval | 6 |
| Layer | language |
| Dependency | Cut A IR |

**Sketch**:

```lazuli
agent summarize_customer
  ...
  cache
    key "summary:{{ input.customer_id }}:{{ input.prompt | hash }}"
    ttl 1h
```

**Approval risk**: cache-key templating is its own can. The string-
interpolation in `key` would be a new language layer; today caching
on queries uses literal strings. Real AI caching also needs the
ability to cache by *semantic similarity* (cosine over embeddings),
which is firmly pack territory.

**Recommendation**: defer to a `cache_pack` until pilot evidence
shows the simple string-key form is enough. Likely never lands as
language.

### Tier 3 — pack territory or already deferred

#### G — Streaming protocol differentiation

`output stream Text via sse|websocket|grpc` would be language-light.
But protocol choice is firmly the runtime / adapter's job. The
architect would push back on adding to language: the streaming
*shape* is already declared; the *transport* is implementation.

**Recommendation**: pack. Adapter exposes the stream over whatever
the deployment target wants.

#### H — Multi-modal output

`output stream image` or `output stream audio` is plausible but
niche. Almost all current AI products need at most text +
structured.

**Recommendation**: defer. Revisit when ≥3 pilot products need it.
When it lands, `output_kind` enum extends additively.

#### I — Prompt rollout / canary

`agent ... rollout v12 weight 20% v11 weight 80%` introduces a feature-
flag-shaped surface that the project has explicitly deferred (`docs/
language-backlog.md:147`: "Feature flags: deferred until repeated
source pressure"). Prompt rollout is a special case of feature
flags; it should ride the feature-flag primitive when that lands, not
get its own surface.

**Recommendation**: pack candidate; absorbed by feature-flag work
later.

### Red herring

#### J — Conversation/thread

Already classified as `chat` pack in `docs/capability-layering.md:239`
and explicitly deferred in Cut B of the AI primitives proposal.
Re-raising would duplicate.

**Recommendation**: stay deferred. Cut B's `flow` covers the most
acute orchestration need without thread state.

## Tier 1 summary — recommended next proposals

Ordered by approval likelihood and immediate value:

1. **A — Agent HTTP exposure**: highest confidence, smallest delta,
   removes verifiable boilerplate. Write the proposal next; should
   pass first-grade.

2. **C — Tool result schema**: piggybacks on Cut A's registry-side
   IR landing in Phase 2. Add as Cut A.6 once Cut A.5 (`safety`
   list) is in. Architect gives it for free.

3. **B — `agent_run` trace event**: language-light + runtime; needs
   coordination with the runtime team currently spiking. Worth
   writing the language-side proposal so the runtime can implement
   without re-deriving the contract.

## Tier 2 summary — pilot-gated

4. **D — Multi-slot context**: write only after one pilot capsule
   shows real boilerplate.
5. **E — Async agent via job**: reuse `calls` with agent target;
   defer until pilot evidence.

## Tier 3 summary — pack/deferred

6. **F — Agent caching**: pack.
7. **G — Streaming protocol**: pack.
8. **H — Multi-modal output**: deferred until ≥3 pilots.
9. **I — Prompt rollout**: rides feature flags.

## Pressure-to-candidate mapping (audit closure)

| Pressure | Candidate | Verdict |
|---|---|---|
| 1. Duplicated api/agent | A | proposal-ready |
| 2. Opaque prompt files | (not in this audit) | open question |
| 3. Unstructured agent observability | B | proposal-ready (with runtime coord) |
| 4. No tool result schema | C | proposal-ready |
| 5. Single-context | D | pilot-gated |
| 6. Agent ↔ contract not bound | (not in this audit) | open question |
| 7. Async / background agent | E | pilot-gated |
| 8. Agent caching | F | pack |
| 9. Multi-modal output | H | deferred |
| 10. Prompt rollout | I | pack via feature flags |

## Open questions surfaced (not resolved by this audit)

- **Pressure 2 — typed prompt manifest**. Should Lazuli parse prompt
  files for variable references and check them against `input` /
  `context` slots? Three sub-questions: which template syntax (mustache,
  jinja, `${}`), where the manifest lives (sidecar JSON, in-source
  declarations), and whether the language owns this at all (could
  be a pack with doctor rules). Worth its own audit / proposal.
- **Pressure 6 — agent input from contract record**. Should
  `agent foo input from contract.<name>.<record>` be allowed? Bind
  agent shape to a remote contract's record. Saves duplication when
  the agent dispatches to a remote AI service. Probably yes, low
  risk, but needs its own scope.
- **Tool dispatch protocol**. Cut A's `tools` block declares the
  binding; the runtime maps to the LLM provider's tool-calling API
  (OpenAI, Anthropic, MCP). Different providers expose different
  tool-calling shapes (function-calling JSON, MCP tools/resources,
  raw text-with-XML). Should the language carry a hint? Probably
  no — adapter territory — but worth recording the deferral.

## Recommendation for the next proposal cut

After Cut A ships and Cut A.5 (safety list) lands:

**Cut A.6 = Candidate C (tool result schema)**. Smallest delta,
piggybacks on registry-side IR.

**Cut B' (replaces the deferred Cut B in part) = Candidate A (agent
HTTP exposure)**. Higher leverage than Cut B's `flow`, lower
implementation cost, no pilot evidence required because the
boilerplate is verifiable in the existing fixture.

**Cut C = Candidate B (`agent_run` trace event)**. Coordinated with
the runtime team's observability work. Would benefit from writing
the language-side now so the runtime has a target to implement
against.

This re-orders the previously deferred Cut B from `flow` →
`budget tokens` → `knowledge` to a new sequence: HTTP exposure →
result schema → agent_run trace → flow / budget / knowledge (those
last three still gated on pilot evidence).

The architect would re-grade this re-ordering before committing.
