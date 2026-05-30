# Lazuli Design Decisions

This document is the canonical answer to *"why isn't this dual form an atrito?"*
or *"why is X intentionally distinct from Y?"*.

**Audit tools, reviewers, and the `/lazuli-grade` and `/lazuli-improve`
pipelines must load this document during their `orient` stage**, before
classifying any vocabulary or shape as friction. If a pattern appears here,
it is an architectural choice with a stated justification — not a
deduction-worthy duplication.

When the language genuinely changes its mind on one of these decisions
(deprecating a form, merging two concepts), update both the entry here and
the underlying contract. Don't leave stale entries.

## Format

Each decision has three parts:

1. **The pattern that looks like friction.** Two or more constructs that
   share vocabulary, prefix, or call site.
2. **Why it isn't.** The architectural justification — what each one
   actually models, and what would break if they were merged.
3. **Where the contract lives.** The invariants line, doctor diagnostic, or
   LSP rule that enforces it. If you can't enforce it, document the rule.

If you find yourself thinking "but a reader will be confused" — yes,
that's the point of this document. The reader (human or LLM) consults it
to learn the distinction. Friction lives in surface vocabulary; the
distinction is in the typed graph behind it.

## Decisions

### 1. `event_group`, `event`, and `event.trace` are three distinct concepts

**Looks like friction**: three constructs with the prefix `event` and
overlapping payload syntax.

**Why it isn't**:

- `event_group <pattern> on <Resource>` is a *payload template*. It
  declares shared payload fields for events whose names match the pattern
  (e.g., `customer_*`). It is not itself an event — concrete events under
  the group inherit the template's fields.
- `event <name>` is a *concrete domain event* that participates in the
  feature-to-feature reaction graph. Other features may consume it,
  emit-driven jobs may trigger from it, and the graph edges are checked by
  doctor.
- `event.trace <name>` is *outside the reaction graph*. It models
  observability/audit trace points that producers publish for telemetry.
  Consumers cannot trigger jobs from `event.trace` — promoting a trace
  event to a domain event requires changing the keyword, which is the
  intended diff.

**Where**: `docs/invariants.md` "Events" section enforces these
distinctions. `crates/lazuli_lsp/src/lib.rs` `event_trace_trigger_diagnostics`
rejects `trigger event <trace>`. `inspect --expand=events` exposes the
distinction in the IR.

### 2. `extensible_by` and `extends @anchor` are complementary, not redundant

**Looks like friction**: two constructs that both express "feature X
extends feature Y".

**Why it isn't**: this is a *bilateral contract*.

- `extensible_by <feature>, <feature>` is the *host opt-in*. A resource
  owner declares which other features may extend its anchors. Without
  this, no feature can extend the host.
- `extends @anchor.<name>` is the *guest opt-in*. The extending feature
  declares which anchor it targets. Without this, the host's
  `extensible_by` is dormant.

Both must agree for the extension to compile. Removing either would
either break host control (anyone can extend) or break composition (no
one can extend explicitly). The dual nature mirrors capability-based
security: subject + object both consent.

**Where**: `docs/invariants.md` "Authored Shape" section. Doctor
cross-checks that every `extends @anchor.<name>` has a matching
`extensible_by` entry on the host feature.

### 3. `app.lzi services` redeclares the feature graph by design

**Looks like friction**: `services` block lists `owns <feature>` and
`exposes command/query/api/...`, duplicating what features already
declare.

**Why it isn't**: services are *deployment topology*, not domain
ownership. The feature graph defines what a feature does; the services
graph defines which features run together as a logical service boundary.
the Lazuli runtime uses the services graph to decide:

- Whether to deploy as monolith, modular monolith, or split services.
- Which exposures cross service boundaries (and need typed transport).
- Which event consumption is in-process versus cross-service.

The same feature graph can be deployed under three different services
graphs without changing a line of feature code. Inferring services from
features would couple the two and remove the indirection that lets a
single codebase ship as three deployment topologies.

**Where**: `docs/invariants.md` lines 77-79 ("logical ownership
contracts"). Doctor cross-checks that every `services.X.owns <feature>`
references a feature that exists in `app.uses`.

### 4. `escape_route` is intentionally outside the `.lzx` route graph

**Looks like friction**: `escape_route "/admin/foo"` and `route admin_foo
path "/admin/foo" ...` both declare HTTP paths.

**Why it isn't**:

- `.lzx route <name>` declares a *product route*. It enters the experience
  graph, the sitemap, generated navigation, and the route builder. It
  participates in audience reachability checks.
- `escape_route "/admin/foo"` declares a *non-product route*. It bypasses
  the experience graph. There is no `.lzx view` or audience binding —
  the Lazuli runtime mounts the handler directly. Generated nav doesn't link to it.
  Doctor doesn't check audience reachability against it.

Escape routes are the explicit way to ship admin/debug pages without
polluting the product graph or the experience model. Folding them into
`.lzx route` would force every admin page to declare an experience and
audience, which inverts the intent.

**Where**: `docs/invariants.md` "Authored Shape" — `escape_route` is
explicit and declares its route, policy, tenant boundary, and source
path. LSP `escape_route_security_diagnostics` enforces the explicitness.

### 5. Namespaces (`@*`) split by axis, not by random taxonomy

**Looks like friction**: many `@*` namespaces (`@semantic`, `@cap`,
`@pii`, `@policy`, `@actor`, `@scope`, `@role`, `@anchor`, `@adapter`,
`@key`, `@llm`, `@tool`, `@fn`, `@hook`, `@validator`, `@client`,
`@query_modifier`) without a visible "this one is auth, that one is data
class" tag at the call site.

**Why it isn't**: each namespace is one *axis* of meaning. The axis is
encoded in the namespace name itself, and the closed catalog is enforced
by `is_allowed_reference_namespace` in
`crates/lazuli_lsp/src/lib.rs`. The axes are:

| Axis | Namespaces |
|---|---|
| Identity / authorization | `@actor`, `@role`, `@scope`, `@policy` |
| Data classification | `@pii`, `@cap`, `@key`, `@semantic` |
| Extension surface | `@fn`, `@hook`, `@validator`, `@adapter`, `@client`, `@query_modifier`, `@anchor` |
| AI capabilities | `@llm`, `@tool` |

**Settled — kind split on the identity axis (SPEC-07 B, consistent with
SPEC-04's `@`-doctrine).** The four identity namespaces share one axis but are
two *kinds* of reference, now named distinctly in the keyword registry:
`@policy.<category>` is a **feature-local named reference** (resolves to a
`policies` block in the same feature — registered as a `decorator`), whereas
`@role`/`@scope`/`@actor` are **app-level catalog atoms** (resolve against the
registry identity catalog — registered with the `catalog_atom` builder, scope
leaf `entity.name.tag.catalog-atom.lazuli`). The axis is preserved; only the
named-ref-vs-catalog-atom kind is made explicit, surfacing in hover and the
generated `docs/keyword-reference.md` scope column. Enforced by
`identity_catalog_atoms_are_a_distinct_kind` in `lazuli_keywords`.

**Settled — no CRUD-named policy categories (SPEC-07 C).** A `policies`
category must not shadow a command effect verb (`create`/`read`/`update`/
`delete` or the plural `creates`/`updates`/`deletes`/`reads`): at a `policy
@policy.create` site the name reads as a write *effect*, not an authorization
category. Canon uses semantic authorization names (`author`/`view`/`edit`/
`remove`/`manage`); `POLICY-CATEGORY-SHADOWS-EFFECT-001` enforces it (warning
under strict, error under iron-hand + production). This makes the `@policy.`
prefix single-purpose — the SPEC-04 named-reference marker — rather than a
disambiguation hack papering over the self-inflicted near-collision.

Merging axes into fewer namespaces would lose the ability to enforce
axis-specific rules (e.g., `@cap.Encrypted(key:@key.tenant)` requires
`@key.*` exactly because key scope is its own axis). The cost of "many
namespaces" is the catalog being closed and enforced; the benefit is
that LLMs and humans never invent `@auth.*` or `@db.*` because the
parser rejects unknown namespaces immediately.

**Where**: `docs/invariants.md` "Namespaces" section.
`crates/lazuli_lsp/src/lib.rs` `namespace_reference_diagnostics` rejects
unknown namespaces. The closed catalog is the contract.

### 6. `query.list`, `query.lookup`, `query.sql` are not "three forms of query"

**Looks like friction**: three query keywords with shared prefix.

**Why it isn't**: each kind has different children, different defaults,
and different code generation. They share the prefix because they share
the noun ("query"); they diverge in shape because they describe
different operations:

- `query.list <name>` — collection. Defaults: `order created_at desc`,
  `paginate <n>`. Children: `params`, `filters`, `search`, `cache`,
  `modifier`.
- `query.lookup <name> by <field>: <Type>` — single-record by key.
  Children: `params` (rare), `cache`. The `by` clause is mandatory in
  the header.
- `query.sql <name>` — opaque SQL backed by `sql "./path.sql"`.
  Children: `params`, `returns <Resource|Record>`. Doctor refuses to
  infer the result shape.

Cold-readers see the kind in the header and know which children to
expect. The bare `query <name>` short form was tried and reverted in
commit `7e263cd` because moving the kind to body shape hurt cold-read
clarity.

**Where**: `docs/invariants.md` "Queries And Relations" section. LSP
`query_mode_diagnostics` enforces the three-kind closed set and rejects
bare `query <name>`.

### 7. `route` is intentionally one keyword across three locator contexts

**Looks like friction**: `route` appears in three places with different
semantics — top-level `.lzx route admin_customer_detail` declares a URL,
`route id: ID` inside a command declares a path/context locator, and
`route id: Customer.ID` inside a view declares a path/context locator
for the experience layer.

**Why it isn't**: all three are *route locators* — the slot space the
generated runtime fills from the request URL or a parent context.
They're the same concept at three layers:

- `.lzx route` — *outer* route, owns the URL/mobile path.
- view `route id: ...` — *experience layer* route slot, derived from
  the surrounding `.lzx route`'s `path` segments.
- command `route id: ...` — *backend* route slot, derived from the
  caller's URL/context.

References use the same form (`route.id`) at all three layers; the only
difference is which scope owns the declaration. Renaming any of them
(`url`, `param`, `slot`) would lose the through-line that lets a single
named locator flow URL → view → command without per-layer translation.

The polysemy is *coherent across layers*, not at-a-point. Merging or
renaming would force authors to remap names at every layer boundary.

**Where**: `docs/invariants.md` "Targets And Bindings" — lists `route.*`
explicitly as a locator space. LSP `lzx_app_route_diagnostics` enforces
the URL form; LSP `command_route_binding_diagnostics` enforces the
backend form; the experience-layer form is checked by the route-slot
binding diagnostics in `crates/lazuli_lsp/src/lib.rs`.

### 8. Mobile `[id]` and web `:id` path syntaxes are intentional

**Looks like friction**: `.lzx route` blocks declare mobile paths as
`"customers/[id]"` (Expo bracket convention) and web paths as
`"/admin/customers/:id"` (Express colon convention).

**Why it isn't**: each platform has an established route-syntax
contract that downstream consumers depend on. Forcing one canonical
form would either:

- Generate Expo routes from `:id` paths and silently break Expo's
  file-system routing convention, or
- Generate Express paths from `[id]` and break the typed router
  expectations that Express/Next.js Pages tooling relies on.

The platform is selected by `surface ... web|mobile` in the route
header; the path syntax follows from that platform's idiom. LSP/doctor
do not normalize across platforms — they accept the platform-native
form on each side.

**Where**: `docs/invariants.md` line 84 — "Dynamic path segments such
as `:id` or `[id]` declare typed `route <name>: <Type>` slots".
`crates/lazuli_lsp/src/lib.rs` `lzx_declared_path_params` extracts both
forms.

### 9. Profile `bindings` reaffirm app `bindings` only when overriding

**Looks like friction**: `profile local.bindings.customer_import.crm =
integrations.crm` repeats the same binding already declared in
`app.lzi`'s `bindings` block.

**Why it isn't (with caveat)**: profile bindings are *override slots*.
When a profile redeclares a binding with the same value as the app
default, it's a documentation-of-intent — the author saying "I
considered this binding for `local` and confirmed the app default is
correct." When the binding differs (e.g., a different
`integrations.<name>` per environment), the profile redeclaration is
load-bearing.

The fixture's `profile local` redeclaration is the documented case
where the profile reaffirms the app default. It's harmless. The
production profile, by contrast, omits the binding entirely (relying
on the app default), which is also fine.

If a future cleanup wants to remove redundant reaffirmations as a
linting rule, that's a separate cut. Until then, both shapes are
canonical: redeclaration-with-same-value is intent documentation,
omission is implicit-default. Doctor does not flag either.

**Where**: `docs/invariants.md` — `profile <environment>` section.
Doctor `profile_contract_diagnostics` validates that bindings reference
declared integrations; it does not require nor forbid redundant
reaffirmation.

### 10. Validators reference scope via type, not call-site keyword

**Looks like friction**: previously, `validates field <name>
@validator.<name>` and `validates resource @validator.<name>` were two
forms of "apply a validator".

**Why it isn't (anymore)**: cut 14 unified them. The canonical form is
`validates @validator.<name>`. Scope is encoded in the validator's
extension type:

- `validator tier_check: Validator[Customer.tier]` — field validator
  (scope `Customer.tier`).
- `validator row_check: Validator[CustomerImportRow]` — whole-resource
  validator (scope `CustomerImportRow`).

The legacy forms still parse but warn with the suggestion to drop the
scope keyword. Documenting this here so audits don't keep proposing
"unify validates field/resource" as if it were still open — it was
shipped in commit `93ac166`.

**Where**: `docs/invariants.md` validators line. LSP
`validation_syntax_diagnostics` warns the legacy forms.

### 11. Test vocabulary is two dialects (generated vs authored), not four

**Looks like friction**: the test surface once spoke four allow/deny-polarity
verb pairs — `allows`/`denies` (predicate/edge), `permits`/`forbids`
(generated actor matrix), `accepted by`/`rejected by` (`.lzx` view
extensibility), and `requires`/`forbids` (agent `evals` cases) — and `forbids`
meant two different things while `requires` meant three.

**Why it isn't (anymore)**: SPEC-08 collapsed the four dialects to **two**,
split on the one genuinely load-bearing axis — generated vs authored:

- **KEPT — generated `permits`/`forbids`.** Command actor-matrix rows are
  machine-derived from `policy @policy.*`. The distinct verb pair is a 1-bit
  at-a-glance signal "this row is generated, do not hand-edit" — information
  the typed subject cannot carry. This is the one test-vocabulary distinction
  worth keeping; collapsing it would force authors to hand-write rows they
  should never touch. Do not propose merging it into `allows`/`denies`.
- **RETIRED — `accepted by`/`rejected by`.** Folded into
  `allows extension <feature>` / `denies extension <feature>`. An allowlist
  membership test is just an authored allow/deny over a typed `extension`
  subject; the distinction lived in the SUBJECT, not the verb. Same move as
  §10 (validator scope moved into the type, not a call-site keyword).
- **RETIRED — eval `requires`/`forbids`.** Folded into `allows`/`denies` over
  the agent-output predicate. Eval polarity is exactly authored allow/deny;
  the eval-only predicate extensions (`<ref> contains …`, `tools.calls
  includes|excludes …`) are unchanged. This also resolves the hard `forbids`
  collision (eval negative-assertion vs generated negative-authorization row)
  and removes one of the three `requires` overloads. The feature-header
  `requires integration <slot>: <Cap>` dependency line and the command
  `requires @policy.x` precondition are DIFFERENT constructs and are
  UNCHANGED.

The canonical answer to "how do I write a test?" is now: authored → always
`allows`/`denies`, with the typed subject (`when`/`from`/`as`/`extension`/bare
eval predicate) naming the dimension; generated → always `permits`/`forbids`.
Two verbs, one canonical form per intent — the same no-information-in-the-verb
entropy win as the reverted bare `query` form (§6) and the unified
`validates field/resource` → `validates` (§10).

The retired spellings hard-error in the parser: `E-TEST-ACCEPTED-BY-RETIRED`,
`E-TEST-REJECTED-BY-RETIRED`, `E-EVAL-REQUIRES-RETIRED`,
`E-EVAL-FORBIDS-RETIRED`. **Do not re-propose re-adding `accepted by`/
`rejected by` or eval `requires`/`forbids`.**

**Where**: `docs/invariants.md` test-vocabulary section + "View tests are
extensibility, NOT policy". Doctor rules `TEST-EVAL-VERB-RETIRED-001`,
`TEST-VIEW-EXTENSION-VERB-RETIRED-001`, and `TEST-MATRIX-VERB-MISPLACED-001`
enforce the folded forms and the generated-vs-authored boundary. Parser
`test_blocks.rs` flags hand-authored `permits`/`forbids` inside command tests
as a generated-only smell.

### 12. `=` and `==` split by role, not friction

**Looks like friction**: the token `=` and the token `==` both appear in
`.lzi`/`.lzx` source. A reader (or an audit pipeline) may see two spellings
and assume one should fold into the other — "why isn't equality just `=`?"

**Why it isn't**: SPEC-05 split the two tokens along the one axis that
matters — *comparison* vs *binding* — so each token carries a single,
self-describing meaning instead of a context-dependent polysemy.

- **`==` is THE equality operator** in the closed predicate language, and the
  *only* one. Every comparison context uses it: rule `deny … when <pred>`,
  `tests allows/denies when <pred>`, query `filters` (`field == value`),
  `unique … when <pred>`, `invariant when <pred>`, conditional policy atoms
  (`… when <pred>`), the `.lzx` route-guard
  (`requires <feature>.lookup_my.<field> == <literal>`), and webhook
  `emits … when <pred>`. This matches the equality reflex an LLM is trained on
  from every mainstream language (Go, Python, JS, TS, Rust, Java, C), so a
  first-attempt predicate parses instead of costing a doctor/compile
  round-trip. `!=` and `has` are unchanged; the ordered comparisons
  `<`, `<=`, `>`, `>=` are unchanged. The IR was already ahead of the surface
  here: `CompareOp::Eq` is documented as `==` and `CompareOp::Ne` as `!=`
  (`crates/lazuli_ir/src/nodes/query.rs`) — the surface now matches the IR.
- **`=` (single) keeps exactly its three NON-comparison roles**, none of which
  is equality: (1) **assignment / payload binding** — `name = input.name`,
  `owner = nil` inside `creates`/`updates` blocks; (2) **field default** —
  `tier: CustomerTier = free`, `health: @semantic.Percentage = 0`; (3) **enum
  storage** — `lead = 10` (the variant's stored value). A bare `=` is never a
  boolean comparison.
- **Lifecycle state bindings stay `=` and are deliberately OUT OF SCOPE.**
  `requires_lifecycle <Resource> = <state>` and
  `only_when lifecycle <Resource> = <state>` bind a state; they do not compare
  one. They lower to a state binding, never to `CompareOp::Eq`, so they are
  not predicate equality and must NOT be migrated to `==`.

**Do not re-propose merging `=` and `==`, and do not flag lifecycle `=`
bindings as needing `==`.** The two tokens model different operations; merging
them would re-introduce the at-a-point collision between equality and
assignment/default/storage that SPEC-05 removed. A bare `=` used as an
equality comparison is a hard error, not an accepted alternate spelling.

**Where**: `docs/grammar.lzi.md` (`comparison`, `filter_op`, `scope_predicate`
productions read `"==" | "!=" | "has"`). Doctor rule
`PREDICATE-EQ-OPERATOR-001` (iron-hand) rejects a single `=` used as an
equality comparison in any closed-predicate context with a fix-it to `==`, and
deliberately EXCLUDES assignment, field default, enum storage, and the two
lifecycle state-binding forms. The retired predicate `=` hard-errors as
`E-PREDICATE-EQ-RETIRED`; a `lazuli upgrade` recipe rewrites predicate-context
`=`→`==` span-precisely while leaving the four `=` roles untouched.

### Tool effect is derived, not declared at the binding site (Cut A)

`agent ... tools` lists references only — no per-tool `effect: read |
write`. The underlying capability (`query.*` → read; `command` → write;
`api` → its declared `method`; `@tool.*` → its registry-side `effect`)
is the source of truth. Re-declaring at the binding would create a
contradiction surface that doctor must reconcile (extra rule, no extra
invariant).

This mirrors the existing surface convention (`submit
command.<name>` inherits effect from the command). Authors signal
intent by *which* tools they list; doctor cross-checks policy
compatibility and write-tool guarding against `safety`.

**Where**: proposal the `ai-primitives-v0` proposal (operational archive) §A1. IR:
`Agent.tools[].resolved_effect: Option<ToolEffect>` populated only by
the inspect expand pass.

### `evals` is separate from `tests` (Cut A)

`tests` are pure-IR predicates evaluated by `lazuli check` and `lazuli
test` against the IR — they never dispatch. `evals` runs a real LLM
call under `lazuli test --evals` and must be gated by an explicit
determinism pin (`temperature 0` AND `seed <int>`) for the case to gate
CI; otherwise `eval_nondeterministic_warning` fires and cases produce
informational results only.

Conflating the two would make `lazuli check` non-deterministic for
some constructs and not others. Keeping the determinism boundary
explicit at the call site preserves `tests` as a pure-pipeline tool
while giving evals a typed home.

The predicate language extends only inside `evals`: `<ref> contains
"<literal>"`, `<ref> contains @semantic.<Type>`, and `tools.calls
includes|excludes <tool-ref>`. Outside evals the closed predicate
language is unchanged.

**Where**: proposal §A3. Doctor diagnostics
`eval_nondeterministic_warning` and
`eval_ordered_op_invalid_diagnostics` enforce the boundary.

### CORS lives in `app.lzi`, not in `expose http` (Cut A.11)

CORS is declared at the `app.lzi` level (language-light tier)
alongside `urls`, not as a child of `expose http` or `api`. Three
reasons:

1. **Boundary test:** CORS doesn't change static analysis of the
   capsule contract — it shapes how the runtime configures HTTP
   transport. By the existing `capability-layering.md` rules, that
   makes it runtime/adapter territory. But it crosses into
   language-light because *observability matters*: an LLM editing
   the capsule to add `expose http path "/api/v2/foo"` needs to see
   the allowlist; doctor needs to cross-check origins against
   declared URLs; the source-of-truth invariant says configuration
   that affects the generated API surface lives in source.
2. **Shared shape with `urls`:** CORS is "which web app can call
   which API per environment" — the same shape `urls per environment
   per target` already declares. Putting CORS elsewhere duplicates
   the environment + target dimension.
3. **Per-endpoint defer:** the 80% case is a global allowlist
   matching declared URLs. Per-endpoint overrides (`expose http cors
   origins ...`) wait for pilot evidence — a real product where the
   global allowlist fails because one endpoint needs wildcard or
   different `allow_credentials`.

**Methods aren't declared** in the `cors` block. The runtime
catalogues whatever `expose http method` / `api method` declare and
serves those on the matching path. Declaring methods in CORS would
create a contradiction surface that doctor must reconcile.

**Where**: proposal the `ai-primitives-cut-a-11` proposal (operational archive). IR:
`AppManifest.cors: Option<AppCors>`, `AppCors`, `AppCorsOriginRule`.
Doctor diagnostics `cors_unknown_environment_diagnostics`,
`cors_credentials_wildcard_conflict_diagnostics`,
`cors_origin_undocumented_diagnostics`. LSP file-local
`cors_contract_diagnostics`.

### `approval` is the third write-tool guard (Cut A.9)

Commands gain an optional `approval` block that gates dispatch on
conditional human sign-off. The write-tool guard
(`agent_tool_write_unguarded_diagnostics`) is extended so that an
agent's write-effect tool is considered guarded when **any** of three
shapes holds:

1. The agent declares `safety @validator.<name>` (Cut A baseline).
2. The target command declares an `approval` block (Cut A.9).
3. The target command declares `idempotency by ...` (Cut B; reserved).

The three are **not** subsets of each other and each addresses a
different threat shape:

- `safety` is a **pre-flight input scrub** — the validator inspects
  the input + tool result before the model can act on it.
- `approval` is a **runtime gating step** — execution pauses until a
  human in the declared role(s) signs off, with a timeout fallback.
- `idempotency by` (Cut B) is **replay-safety** — the command's
  effect deduplicates across retries by the declared key.

A multi-write-tool agent may use a mix: some tools guarded by
`approval` on the target command, others by the agent's own `safety`
when the command's nature makes per-dispatch approval inappropriate.
Doctor reports which guard satisfied each binding.

**Where**: proposal the `ai-primitives-cut-a-9` proposal (operational archive). Doctor
diagnostics `approval_role_unresolved_diagnostics`,
`approval_timeout_invalid_diagnostics`, `approval_contract_diagnostics`,
plus the `agent_tool_write_unguarded_diagnostics` extension. LSP
file-local `approval_contract_diagnostics`. Captured via text-pattern
facts (`CommandApprovalFact`) until the canonical-indent slice
covers commands.

### Built-in trace events are IR-registered, not authored (Cut A.8)

`agent_run` is the foundational built-in trace event: the runtime
auto-emits it per agent dispatch with a canonical payload schema
(`agent`, `model`, `tokens_input/output/total`, `cost_usd`,
`duration_ms`, `finish_reason`, `tools[]`, etc.). The language reserves
the name in the IR; authored `event.trace agent_run` declarations are
rejected with `event_trace_reserved_name_diagnostics`. Subscriber jobs
that reference fields outside the canonical schema fail at
`lazuli doctor` time via `agent_run_subscriber_payload_drift_diagnostics`
rather than at runtime — schema drift is caught before ship.

Three reasons the language owns the contract (rather than the runtime
emitting any shape it likes):
1. **Schema drift** — without language registration, every consumer
   hardcodes the schema; changes become silent breakages.
2. **Inspect contract** — built-in events must appear in the typed
   read model (`magic discovery requires visibility`). They surface
   via `lazuli inspect --expand=events`'s `built_in_trace_events[]`
   slot alongside authored events.
3. **Cross-runtime portability** — the wire format can change; the
   contract stays put.

`agent_run.cost_usd` is `Decimal`, **not** `@semantic.Money`. Trace
events are denominated in a single canonical currency because
multi-currency conversion at observation time would force every
adapter to carry exchange-rate state. Adapters that bill in non-USD
convert at observation. This is a scoped exception to the project-wide
`@semantic.Money` discipline; do not generalize.

**Where**: proposal the `ai-primitives-cut-a-8` proposal (operational archive). IR:
`built_in_trace_events()`, `BuiltInTraceEvent`, `BuiltInTraceRecord`,
`TraceFiresPer`, `is_reserved_trace_event_name`. Doctor diagnostics
`event_trace_reserved_name_diagnostics` (also fires file-local in LSP)
and `agent_run_subscriber_payload_drift_diagnostics`. Inspect
projection extends `--expand=events` with `built_in_trace_events[]`.

### `expose http` is the shortcut for trivial agent-dispatch APIs (Cut A.7)

`agent <name>` gains an optional `expose http` block that auto-mounts
the agent as an HTTP endpoint. The agent's existing `policy`,
`rate_limit`, and `output` apply to the exposed endpoint without
restating. `api <name>` blocks remain for handlers whose work goes
beyond translating an HTTP request into an agent dispatch (multi-step
orchestration, format transformation, calling several agents,
validation that the agent's `input` shape cannot express).

The boundary test: *does the handler do work beyond translating HTTP
to agent dispatch?* If yes, keep `api`. If no, collapse into
`expose http`. Cut A.7's evidence was the canonical fixture's own
duplicated `api customer_summary_stream` next to `agent
summarize_customer` — the api block did no work, and the maintenance
cost was real.

Doctor enforces `(method, normalised_path)` uniqueness across
features (both `api` and `expose http` count); LSP catches the same
within a single file plus slot-binding mistakes (path placeholder
without a matching `route`, or declared as `input` instead of
`route`).

**Where**: proposal the `ai-primitives-cut-a-7` proposal (operational archive); plan
the `ai-primitives-cut-a-7-implementation` proposal (operational archive). IR:
`Agent.expose_http: Option<HttpExposure>`. Doctor diagnostics
`agent_expose_path_conflict_cross_feature_diagnostics`,
`agent_expose_audience_unknown_diagnostics`. LSP diagnostics
`agent_expose_path_conflict_local_diagnostics`,
`agent_expose_slot_unbound_diagnostics`,
`agent_expose_slot_must_use_route_diagnostics`,
`agent_expose_method_streaming_mismatch_warning`. New inspect
projection `--expand=expose`.

### Discriminated `output` lands before `flow` (Cut A)

Discriminated output (`output discriminator <Enum>` or `output
<Record>` with a `discriminator` field marker) ships in Cut A even
though flow (the obvious consumer) is deferred to Cut B. The reason:
flow's `step on <prev>.<branch>` needs a typed branch token from day
one. Landing the discriminator first means flow is a pure routing-graph
addition on top of an already-shipped output type system, and the
branch-checking diagnostic is implementable when flow eventually ships
without retrofitting the IR.

**Where**: proposal §A2; IR `AgentOutputKind` + `DiscriminatorRef`.
Doctor diagnostics `agent_discriminator_target_invalid_diagnostics` and
`agent_discriminator_field_invalid_diagnostics`.

## Already-shipped primitives the audit pipeline keeps hallucinating as missing

The `audit-primitives` and `identify-missing` stages have repeatedly
proposed primitives that already exist in the language. Before listing
any of these as missing, **grep the fixture and the LSP keyword catalog
in `crates/lazuli_lsp/src/lib.rs`**. If found, the construct exists.

| Primitive | Status | Fixture use sites |
|---|---|---|
| `audit` | shipped (cut 3) | commands/queries/jobs/webhooks |
| `derived from` | shipped (cut 2) | resource fields |
| `has_many` | shipped (cut 4) | resources |
| `agent` | shipped (cut 6) | `customer.agent summarize_customer` |
| `notification` | shipped (cut 11) | `customer_outreach.notification welcome_email`, `archive_survey` |
| `validates @validator.<name>` | shipped (cut 7, unified in cut 14) | resources |
| `previously` as block child | shipped (cuts 8 and 15) | uniformly across resource/command/transition/field |
| `invalidates` wildcard / short form | shipped (cut 9) | command invalidations |
| `agent` config siblings | shipped (cut 10) | `temperature`, `max_tokens`, `top_p`, `seed` |
| `query <kind>` short form | reverted (commit `7e263cd`) | use `query.list`/`.lookup`/`.sql` |
| `emits <event> from <effect>` | shipped (cut 12) | command emits |
| Contract operation AI-first dimensions | shipped (cut 13) | `contracts/ai.lzi:operation summarize_customer` |
| `event.trace <name>` | preexisting | observability events |
| `escape_route "<path>"` | preexisting | admin/debug pages |
| `event_group` | preexisting | shared payload templates |
| `extensible_by` / `extends @anchor` | preexisting | anchor extensions |
| `policy_for <kinds>: <policy>` | preexisting | feature `defaults` block (covered by LSP, hover, inspect) |
| `defaults` block (`tenancy`, `timestamps`, `policy_for`, `policy`) | preexisting | feature defaults |
| `previously migrated\|alias <old>` (as block child) | shipped (cuts 8 + 15) | uniform across fields/resources/commands/transitions |

If a primitive is on this list, it is **not** an audit finding. Suggest
new primitives only after greping the fixture and confirming the
construct keyword does not appear.

## Reading order for audit pipelines

1. `docs/invariants.md` — the rules.
2. `docs/design-decisions.md` *(this file)* — why apparent dual forms are
   not friction.
3. the operational next-checklist — what's done, what's tracked.
4. `crates/lazuli_lsp/src/lib.rs` keyword catalog — the closed list of
   keywords the language accepts.
5. `examples/full-capsule/` — the canonical fixture, after the docs.

If a finding contradicts any of (1)–(4), the finding is wrong. Re-read
before reporting.
