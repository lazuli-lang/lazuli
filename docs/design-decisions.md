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
3. `docs/next-checklist.md` — what's done, what's tracked.
4. `crates/lazuli_lsp/src/lib.rs` keyword catalog — the closed list of
   keywords the language accepts.
5. `examples/full-capsule/` — the canonical fixture, after the docs.

If a finding contradicts any of (1)–(4), the finding is wrong. Re-read
before reporting.
