# Proposal: Cut A.7 — Agent HTTP exposure

**Status**: Draft proposal. Depends on Cut A
(`docs/proposals/ai-primitives-v0.md`) `Agent` IR shape; lands as a
follow-on after Cut A's parser slice and IR are in.

**Owner**: TBD. **Target version**: `LZI_LANG` minor bump after the
first pilot product authors `expose http` on a real agent.

## Motivation

The canonical fixture has a smell: `examples/full-capsule/full-
capsule.lzi:305-329` declares `api customer_summary_stream` and
`agent summarize_customer` as siblings, with **identical** `policy`,
`rate_limit`, `output stream Text`, and overlapping `input`. The
api's only job is to translate an HTTP request into a call against
the agent. The handler file `./api/stream_customer_summary.go` is
boilerplate.

Today, every product wanting to expose an agent over HTTP writes
this duplicate. The pressure was identified in the AI-first roadmap
audit (`docs/proposals/ai-first-roadmap.md` Pressure 1, Tier 1
Candidate A) and confirmed in the canonical fixture itself.

The fix is small: agents gain an optional `expose http` child that
declares the method, path, and route slots. The runtime auto-mounts
the endpoint with the agent's existing policy / rate_limit / output
contract. The duplicate `api` block becomes unnecessary.

## Scope

- `expose http` child of `agent` with `method`, `path`, optional
  `route` slots, optional `audience`, optional override of
  `rate_limit`.
- `Agent.expose_http: Option<HttpExposure>` IR field.
- Doctor diagnostic
  `agent_expose_path_conflict_diagnostics` for path collisions
  with `api` blocks and other exposed agents.
- Inspect projection in `--expand=summary` and
  `--expand=security`.

## Promotion gate

Cut A.7 lands when **at least one fixture or pilot product has an
`api` block whose only job is to translate an HTTP request into an
agent call**. The full-capsule fixture already qualifies; the
proposal therefore has authoritative pre-existing pressure, unlike
Cut A.5 and A.6 which need new pilot evidence.

The gate is satisfied today. The decision is timing, not
permission: ship Cut A.7 once Cut A's `Agent` IR is in.

## Syntax

Single-line audience, single-line method, indented body for slot
declarations:

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

This replaces:

```lazuli
api customer_summary_stream
  method POST
  path "/api/customers/:id/summary/stream"
  route id: Customer.ID
  input
    prompt: Text required
  output stream Text
  policy @policy.read
  rate_limit "20 per hour per user"
  handler "./api/stream_customer_summary.go"
```

The agent's existing `input` covers the request body (with `customer_id`
binding from the URL slot via the agent's existing
`input` slot resolution). The agent's `policy`, `rate_limit`, and
`output` apply to the exposed endpoint without restating.

## Rules (normative)

- **Block shape**: `expose http` opens an indented body.
  Required children: `method`, `path`. Optional children:
  `route` slots, `audience`, `rate_limit` (overrides agent-level
  rate_limit for HTTP traffic only).
- **Method**: same closed catalog as `api`: `GET`, `POST`, `PUT`,
  `PATCH`, `DELETE`. POST is the default for streaming agents;
  doctor warns if `method GET` is paired with `output stream`.
- **Path**: a quoted URL string. Path slots use `:slot_name`
  placeholders matching `route` declarations.
- **Route slot binding (one canonical form)**: each `:slot_name`
  in `path` must have an explicit `route slot_name: <Type>`
  declaration inside the `expose http` block. Authors who already
  declare the same slot in `input` must move it to `route`; doctor
  rejects the input-only form with
  `agent_expose_slot_must_use_route_diagnostics`. This matches the
  existing `api` precedent (path slots are declared via `route`,
  not `input`) and keeps one canonical form per the determinism
  rule. `lazuli fmt` rewrites legacy `input`-bound slots to `route`
  during migration.
- **Body**: the HTTP request body deserializes into the agent's
  `input` slots not bound to URL. Body shape is implicit; doctor
  reports the resolved shape under `--expand=tools` or a new
  `--expand=expose` projection.
- **Output**: the agent's `output` declaration determines response
  shape. `output stream Text` produces an HTTP streaming response
  (transport per runtime/adapter choice). `output discriminator
  <Enum>` (Cut A) produces a typed JSON response with the
  discriminator field. `output <Record>` is plain JSON.
- **Auth and policy**: the agent's existing `policy
  @policy.<name>` becomes the endpoint's auth gate.
- **Rate limit**: agent-level `rate_limit` applies to all
  invocations; `expose http rate_limit` (if declared) applies
  *only* to HTTP traffic, in addition to the agent-level limit.
  This matches the `api` precedent.
- **Audience**: optional. If declared, the endpoint surfaces only
  to deployments serving that audience (matching `app.lzi
  runtime` `serves` semantics).

## What does NOT replace `api`

- An `api` whose handler does meaningful work beyond agent
  dispatch (validation, multi-step composition, calling several
  agents, format transformation) stays as `api`. Cut A.7 is a
  shortcut for the *trivial agent-dispatch case only*.
- An `api` exposing a `command` or `query` is unchanged. Cut A.7
  does not generalize to non-agent operations; that would be a
  much larger primitive (`expose http` on every operation kind).
  Not in scope.
- A surface-side `submit command.<name>` action is unchanged. The
  `submit` shape is for user-facing forms inside views, not for
  programmatic HTTP exposure.

## Diagnostics

Path-conflict detection splits between LSP (file-local, fast) and
doctor (cross-feature, package-wide):

| Id | Severity | Pipeline | Source |
|---|---|---|---|
| `agent_expose_path_conflict_local_diagnostics` | error | LSP | A7 |
| `agent_expose_path_conflict_cross_feature_diagnostics` | error | doctor | A7 |
| `agent_expose_slot_unbound_diagnostics` | error | LSP | A7 |
| `agent_expose_slot_must_use_route_diagnostics` | error | LSP | A7 |
| `agent_expose_method_streaming_mismatch_warning` | warning | LSP | A7 |
| `agent_expose_audience_unknown_diagnostics` | error | doctor | A7 |

`agent_expose_path_conflict_local_*` runs at the LSP level for
agents/`api` blocks within the same file — instant feedback while
typing. `agent_expose_path_conflict_cross_feature_*` runs in
`lazuli doctor` and walks the whole package set, accounting for
placeholder normalization (`/api/customers/:id` and
`/api/customers/:customer_id` collide when both map to
`Customer.ID`).

`agent_expose_audience_unknown_diagnostics` is cross-file: the
audience must be declared by some `.lzx` surface or route in the
package. It's a doctor-level check because audiences live in
`.lzx`, not `.lzi`.

`agent_expose_method_streaming_mismatch_warning` warns when
`method GET` is paired with `output stream Text`.

## IR delta

Add to `Agent` (Cut A's IR shape):

```rust
pub struct Agent {
    // existing Cut A fields...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expose_http: Option<HttpExposure>,
}

pub struct HttpExposure {
    pub method: HttpMethod,
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_slots: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}
```

**`HttpMethod` enum is new**: today's IR uses `method: Option<String>`
on the contract side (`crates/lazuli_ir/src/lib.rs:93`) and
`method: String` on the api side (`crates/lazuli_ir/src/lib.rs:1610`).
Cut A.7 introduces the typed `HttpMethod` enum and migrates both
existing call sites to use it. The migration is purely structural
(string → enum); no source changes for authors.

`LZIR_SCHEMA`: minor bump for additive `expose_http` field; major
bump if the existing `String` → `HttpMethod` migration breaks
backward IR compatibility for older serialized IRs (likely; the
field name stays but the value shape changes from any-string to
closed-set). Pin the bump decision in the implementation cut.

`LZI_LANG`: minor bump.

## Inspect delta

`--expand=summary` agent entries gain an optional `expose_http` key
when present:

```json
{
  "agent": "summarize_customer",
  "expose_http": {
    "method": "POST",
    "path": "/api/customers/:customer_id/summary",
    "route_slots": [{"name": "customer_id", "type": "Customer.ID"}]
  }
}
```

`--expand=security` includes the resolved auth (agent's `policy`),
the effective rate_limit (agent + override), and the audience.

A new optional `--expand=expose` projection (or extending an
existing `routes`/`apis` expansion) lists the unified set of HTTP
endpoints across `api` blocks and exposed agents. Useful for
generating OpenAPI schemas or checking gateway routing.

## Why language, not pack

Three reasons:

1. The contract is **static**: which method, which path, which
   audience. The runtime needs these at code-generation time to
   mount the endpoint. A pack would have to re-derive the same
   information from a sidecar manifest.
2. Path-conflict detection requires cross-feature, cross-construct
   knowledge — exactly the doctor surface.
3. Removes language-shaped boilerplate (the duplicate `api`
   pattern visible in the canonical fixture). The bar for "this
   is a pack" is "removes runtime work but not source duplication";
   here, source duplication is the primary cost.

The architect's discipline rejects "every useful product feature
becomes language" (capability-layering.md:32-38). This proposal is
an exception for the same reason as Cut A.5: the IR fields are
already in scope, the diagnostic is doctor-shaped, and the
alternative (pack) cannot deliver path-conflict detection across
feature boundaries.

## Why not in Cut A or Cut B

- Cut A's six-primitive bundle was already the wrong size. Adding
  `expose http` would have made the proposal eight primitives.
  The architect would have blocked harder.
- Cut B's deferred items (`flow`, `budget tokens`, `knowledge`,
  `quota cost`) all require pilot evidence to land. Cut A.7's
  evidence is already in the canonical fixture; it doesn't need
  to wait for pilots.

## Acceptance criteria

- Cut A's `Agent` IR shape has shipped.
- `expose http` block parses, lowers, and is honored.
- Path-conflict diagnostic implemented and tested with three
  cases:
  - non-conflicting paths (passes)
  - direct duplicate path (errors)
  - placeholder normalization conflict (errors)
- Slot-unbound diagnostic implemented.
- Streaming-vs-method warning implemented.
- `--expand=summary` reports `expose_http` when present.
- `examples/full-capsule/full-capsule.lzi` updated: `api
  customer_summary_stream` removed; `agent summarize_customer`
  gains `expose http` block.
- The handler file `./api/stream_customer_summary.go` removed
  from the fixture. **Note**: the runtime-side equivalent
  (auto-mounting the agent's HTTP endpoint) is runtime work,
  tracked separately. Cut A.7 ships the language contract;
  the runtime team's parallel work materializes the dispatch.
- `docs/grammar.lzi.md §14 (Agent)` adds `expose http` child.
- `docs/invariants.md` agent invariant lists `expose http` as
  optional.
- `docs/canonical-semantics.md §Working With Agents` documents
  the relationship between `agent expose http` and `api`.
- `docs/design-decisions.md` records: *agent `expose http` is the
  shortcut for trivial agent-dispatch APIs; non-trivial APIs that
  call agents stay as `api` blocks.*

## Migration impact

Existing agents without `expose http`: zero impact. The field is
optional.

Existing `api` blocks delegating to agents: stay valid. Cut A.7
adds a shortcut, not a deprecation. Authors choosing to migrate
remove their `api` block and add `expose http` to the agent. No
hard deadline; teams migrate at their own pace.

If the migration becomes pervasive (every product collapses to
`expose http`), a future cut may *deprecate* trivial
api-as-agent-handler patterns with a `--strict-apis` flag. Out of
scope for this proposal.

## Non-goals

- `expose grpc`, `expose websocket`, `expose graphql`. Transport
  variety is firmly adapter territory. The agent declares its
  shape (text stream, structured, discriminated); the runtime
  picks the wire format.
- `expose http` on `query`, `command`, `webhook`. Those have
  their own exposure paths today (`api`, `service exposes` in
  app.lzi). Cut A.7 narrowly addresses the agent-only case.
- Multi-method agents (one agent exposing both `GET` and `POST`
  on the same path). YAGNI; if needed, declare two agents or
  shape your one agent's `output` to handle both.
- OpenAPI schema generation from `expose http`. The runtime / a
  publication adapter generates schemas; not language work.

## Reserved

- `expose grpc`, `expose websocket`, `expose graphql` (transport
  alternatives).
- Path versioning (`expose http v2 ...`). Out of scope.
- Per-method-overload agent exposure.

## Release timing

Ship **before** A.5 and A.6, immediately after Cut A's `Agent` IR
lands. The architect's grade explicitly endorsed this sequence
because A.7's evidence is already satisfied in the canonical
fixture, while A.5 and A.6 require new pilot evidence.

Recommended sequence:

```
Cut A   (parser slice + Agent IR + tools + discriminator + evals)
  ↓
Cut A.7 (agent expose http)                         [pre-evidenced; ship next]
  ↓
Cut A.5 (safety list + PII coverage check)          [evidence-gated]
  ↓
Cut A.6 (tool result schema)                        [evidence-gated]
  ↓
Cut A.8 (agent_run trace event)                     [coordinate w/ runtime]
```

A.5/A.6/A.7 are parallelizable in IR (each touches different IR
fields), but A.7 is the only one whose fixture changes *immediately*
on merge — providing the fastest feedback signal that Cut A's
architectural foundation is sound.
