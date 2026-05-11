# Bucket Cycle: OpenAPI Generation (L0→L2)

**Status**: design proposal. Stages 3–9 of the `bucket=openapi` pipeline.
Implementation deferred to a separate run with `mode=implement`.

**Audience**: language team (Lazuli core), runtime team (Drusa).

**Date**: 2026-05-11.

**Side-quest framing**: every other bucket so far (auth / storage / jobs /
observability) lifted *new authored surface* into IR. OpenAPI is the
inverse — it consumes **existing IR** and emits an artifact. That means
the language work is **smaller** (one new decorator, one new CLI verb,
two `--expand` projections) but the blocker is **upstream**: the IR
must already carry every fact OpenAPI needs (input shape, output shape,
policy, response codes, error envelope). Today some of those facts are
typed (`Command.input`, `HttpMethod`, `HttpExposure`, `FileCapability`),
others are still text-pattern (`api` blocks beyond `method`/`path`,
`command.audit` body, error envelope). The proposal makes the gap
explicit and proposes a **typed-IR-only first cut** so the artifact is
not a re-encoding of the text-pattern fallback.

## Contexto

The canonical fixture authors **one** `api` block
(`examples/full-capsule/full-capsule.lzi:303-309` — `api customer_export`)
and **eleven** `command` blocks (`:226, 244, 263, 291, 526, 535, 640,
651, 666, 753`, plus one in `customer_auth`). Each `command` is the
real HTTP-callable surface — the convention is that `command <name>` on
`feature <f>` mounts as `POST /api/<f>/<name>` (or `PUT/DELETE` derived
from the `CommandKind`).

Today the IR carries enough for commands:

| Slot | Status | Anchor |
|---|---|---|
| `Command.name` / `kind` (`Create`/`Update`/`Delete`/`Returns`) | typed | `crates/lazuli_ir/src/lib.rs:501-529` |
| `Command.route: Vec<RouteSlot>` (path params) | typed | `crates/lazuli_ir/src/lib.rs:505, 532` |
| `Command.input: CommandInput` (`Short`/`Typed`/`Empty`) | typed | `crates/lazuli_ir/src/lib.rs:506, 539-547` |
| `Command.policy: PolicyRef` | typed | `crates/lazuli_ir/src/lib.rs:512, 619` |
| `Command.effect: CommandEffect` (`Creates`/`Updates`/`Deletes`/`Returns`) | typed | `crates/lazuli_ir/src/lib.rs:511, 576-584` |
| `Command.emits: Vec<String>` | typed | `crates/lazuli_ir/src/lib.rs:514` |
| `HttpMethod` enum | typed | `crates/lazuli_ir/src/lib.rs:2450-2456` |
| `HttpExposure` (agent `expose http`) | typed | `crates/lazuli_ir/src/lib.rs:2430-2442` |
| `FileCapability` (api output / field) | typed | `crates/lazuli_ir/src/lib.rs:`(post-Tier 2) |

What is **missing** for OpenAPI 3.1 generation:

| Slot | Status | Anchor |
|---|---|---|
| `Command.deprecated` flag | **gap** — no decorator in surface, no field in IR | n/a |
| `Command.audit` body (the typed `audit ...` lines on `:271-272`) | **gap** — text-pattern in `inspect_command`'s feature walker, never lifted | `crates/lazuli_cli/src/main.rs:1791` walker reads `audit ` for inspect but not for IR |
| `Api.input` (when an api receives a body) | **gap** — `api` blocks lift only `method`/`path`/`route_slots`/`audience`/`rate_limit_override` via text-pattern | `crates/lazuli_cli/src/main.rs:1791-1834` |
| `Api.output` (when an api returns a typed shape beyond `@cap.File`) | **gap** — `output` line below `api` is invisible to IR | same anchor |
| `Api.policy` / `Api.handler` | **gap** — text-pattern only | same anchor |
| Default error envelope shape (the `errors` block on `:221-224`) | **gap** — `default hide` / `expose client 4xx` is text-pattern, never reaches IR | `crates/lazuli_cli/src/main.rs` (legacy walker) |

The cross-cutting fact: `api customer_export` is the only `api` block
in the canonical fixture, and even that one loses **6 of 8 slots**
(`output`, `policy`, `rate_limit`, `handler` plus the line span) when
lifted to inspect. Generating OpenAPI from a payload that ignores
output and policy would produce a spec that **fails its own validator**
(missing `responses`, missing `security`).

**Decision** (Stage 0): emit OpenAPI from the **typed slice only**.
Commands are typed enough today; `api` blocks are not. The first cut
covers commands; `api` blocks land after Phase L Tier 4 lifts
`parse_api` alongside `parse_command`/`parse_resource`/`parse_query`/
`parse_record`. Until then `lazuli generate --openapi` emits an
explicit `x-lazuli-text-pattern-skip` annotation for every `api` block
that has not been lifted, with a doctor warning pointing at the gap.

## Side-quest premise

The audit (§13 of `docs/audit/framework-coverage-1400.md:227`) labels
OpenAPI generation as **DL** (deve-ter na linguagem). The roadmap
(`docs/roadmap.md:150`) lists three items together: (a) `openapi`
generation from IR, (b) `deprecated` decorator on api/command,
(c) API changelog from IR diff. The roadmap also lists OpenAPI gen as
part of the **second wave** after the bucket-piloto cycle closes
(`docs/roadmap.md:57, :721`).

Side-quest constraint: codegen can read **existing** IR without
waiting for Tier 4, **as long as** it does not invent new authored
surface that text-pattern walkers would have to match. So the cut is:

1. **Decorator** (`deprecated`) — single boolean, additive on
   `Command` and (post-Tier-4) `Api`. Authored surface change.
2. **CLI verb** (`lazuli generate --openapi`) — read-only IR consumer.
   No authored surface change.
3. **Changelog** (`lazuli changelog --since <git-ref>`) — IR-diff
   consumer that compares two `lazuli inspect --format=json` payloads
   and emits a markdown delta. No authored surface change.

The runtime stub is minimal — Drusa needs **zero new packages** for
the first cut because OpenAPI emission is a Lazuli-side codegen
artifact, not a runtime capability. The runtime team only needs to
*serve* the resulting `openapi.yaml` if the author chooses to expose
it (which is operational, not language).

## Baseline (Stages 1-2 inventory)

| Layer | Status | Anchor |
|---|---|---|
| Surface syntax (`.lzi`) | commands typed (audit `:271`, approval `:273-277`, route `:265`, input `:266-267`, policy `:269`, effect `:278-279`, emits `:280-281`). No `deprecated` decorator anywhere. | `examples/full-capsule/full-capsule.lzi:226-799` |
| IR (`crates/lazuli_ir`) | `Command` typed; `HttpMethod` typed; `HttpExposure` typed; `FileCapability` typed (post-Tier-2). `Api` block is **not** a typed IR node — only the `expose` projection has a placeholder for it. | `crates/lazuli_ir/src/lib.rs:501, 2430, 2450` |
| Parser slice | `parse_command` lives outside the canonical-indent slice (Phase L Tier 4 outstanding — `docs/next-checklist.md:60` row 24). Commands lift into legacy `Feature.commands` via the analyzer; the canonical slice does not see them yet. | `crates/lazuli_syntax/src/parser.rs` (Tier 4 outstanding) |
| LSP | recognises `command`/`api`/`route`/`input`/`output`/`policy` keywords; no `deprecated` hover/completion. | `crates/lazuli_lsp/src/lib.rs` |
| Doctor | command-level rules in place (policy reachability, idempotency, audit). No deprecation-aware rules. | `crates/lazuli_cli/src/doctor.rs` |
| Inspect projection | `--expand=expose` projects `api customer_export` as a partial entry (kind/method/path/rate_limit) but **no input/output/policy/audit/errors** are projected. `commands` are not projected at all in the default output. | `crates/lazuli_cli/src/main.rs:1770-1837` |
| OpenAPI generator | **does not exist** — zero references to `openapi`/`OpenAPI`/`swagger` in `crates/`, `runtime/`, or `dist/`. | confirmed via `Grep` 2026-05-11 |
| Highlighting | no `deprecated` keyword; no `openapi` keyword. | `editors/vscode/syntaxes/lazuli.tmLanguage.json` |

**Cross-cutting fact**: the only authored fixture site that exercises
multiple HTTP-bearing surfaces is `feature customer` (`:226-359`).
It declares 8 commands + 1 api + 2 agents. A useful OpenAPI artifact
must paint all 11 endpoints; today, of the 11, **8 commands** can be
generated from typed IR, **2 agents** can be generated from
`HttpExposure`, and **1 api** falls through to a text-pattern stub.
That's the baseline coverage number: 10/11 = 91% typed.

## Linguagem (Stage 3)

Surface is **almost** canonical — only one additive decorator
(`deprecated`). The CLI verb and the changelog command are not
authored surface.

### Formal grammar (EBNF, draft for `docs/grammar.lzi.md`)

```ebnf
command_block     = "command" identifier NEWLINE INDENT
                    { command_child }
                    DEDENT ;

command_child     = ... existing children ...
                  | command_deprecated ;

command_deprecated = "deprecated" [ deprecated_args ] NEWLINE ;

deprecated_args   = deprecated_arg ( "," deprecated_arg )* ;

deprecated_arg    = "since" ":" string                (* version literal *)
                  | "replacement" ":" qualified_ref   (* @command.*, command name, or url *)
                  | "sunset" ":" date_literal ;       (* ISO-8601 date *)

date_literal      = '"' YYYY "-" MM "-" DD '"' ;
```

### Slot inventory

| Slot | Required | Type | Closed catalog | Fixture anchor |
|---|---|---|---|---|
| `deprecated` (bare) | optional, sigil-only | flag | n/a | not in fixture; Stage 3 adds to `command capture_lead` to exercise the flag without sub-args |
| `deprecated since: "<version>"` | optional | string | no — adapter-parsed (semver-ish) | Stage 3 inline example |
| `deprecated replacement: <ref>` | optional | ref | yes — must resolve to a same-feature command, a cross-feature command, an `@api.*` slot (future), or a URL | Stage 3 inline example |
| `deprecated sunset: "YYYY-MM-DD"` | optional | date literal | yes — must be `YYYY-MM-DD` ISO-8601 | Stage 3 inline example |

### Example expansion in the fixture

Stage 3 adds the bare `deprecated` flag to `command reassign`
(`:263-289`) and a fully-decorated `deprecated` to a *new* shadow
command, to exercise both shapes without breaking existing tests:

```lazuli
  command reassign
    previously migrated assign_owner
    deprecated since: "2026.04", replacement: reassign_v2, sunset: "2026-12-31"
    route id: ID
    input
      owner_id: User.ID required
    ...
```

The decorator is additive — the parser already tolerates unknown
child lines via the legacy lowering path; the new typed slot lands
when Phase L Tier 4 lifts `parse_command`. Pre-Tier-4 the decorator
parses as a text-pattern fact (same shape as today's `previously
migrated`) so it surfaces in OpenAPI output immediately.

### Closed-catalog rationale

- `since` is a free string because semver/calendar/git-sha versioning
  schemes vary; the OpenAPI emitter writes it verbatim into the
  `x-lazuli-deprecated-since` extension field.
- `replacement` must resolve — the same way `safety @validator.<x>`
  must resolve. The doctor rule `deprecated_replacement_unknown`
  catches typos; the rule reuses the symbol table already built for
  `previously migrated` (`:264`).
- `sunset` is ISO-8601 because that's what RFC 8594 (the IETF Sunset
  HTTP header) requires; the runtime emits the same value as the
  `Sunset` HTTP header on every response.

### Why no `api`-level `deprecated`

Until Phase L Tier 4 lifts `parse_api`, the `api` block has no IR
node to attach `deprecated: bool` to. Adding text-pattern lifting
*just* for this decorator would invent two parsers for the same slot
(typed for `command`, text-pattern for `api`); Route B of this
proposal rejects that.

### CLI grammar

Two new verbs, neither is authored Lazuli surface:

```text
lazuli generate --openapi <input.lzi | dir> [--output <path>] [--profile <env>]
                          [--include-text-pattern | --strict] [--version <ver>]

lazuli changelog [--since <git-ref>] [--current <git-ref>]
                 [--against <inspect.json>] [--format markdown|json]
```

Stage 4 specifies what each consumes from IR.

## IR (Stage 4)

Two additive fields. No new struct.

### IR additions

1. **`Command.deprecated: Option<Deprecation>`** on `crates/lazuli_ir/src/lib.rs:501-521`.

   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   pub struct Command {
       pub name: String,
       pub kind: CommandKind,
       // ... existing fields ...
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub deprecated: Option<Deprecation>,
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub span_ref: Option<SpanRef>,
   }

   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   pub struct Deprecation {
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub since: Option<String>,
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub replacement: Option<DeprecationReplacement>,
       /// ISO-8601 `YYYY-MM-DD`. Parsed at lowering; format-checked.
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub sunset: Option<String>,
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub span_ref: Option<SpanRef>,
   }

   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   #[serde(tag = "kind", content = "value")]
   pub enum DeprecationReplacement {
       /// `replacement: <command_name>` resolved within the same feature.
       LocalCommand(String),
       /// `replacement: <feature>.<kind>.<name>` cross-feature.
       Qualified(QualifiedToolRef),
       /// `replacement: "https://..."` — explicit URL escape hatch.
       Url(String),
   }
   ```

2. **`AppManifest.api_version: Option<String>`** — Stage 3 reads from
   `app.lzi` `version <semver>` if present, falls back to git-tag if
   the codegen run is inside git. Empty in inspect when absent.

### CLI command — `lazuli generate --openapi`

The verb is a **pure consumer** of IR. The pipeline is:

```text
[input .lzi or dir]
  → parse + lower (existing analyzer pipeline)
  → ir::Package (existing)
  → openapi::emit(package, options)
  → openapi 3.1.0 yaml (or json with --format=json)
```

`openapi::emit` lives in a new crate `crates/lazuli_openapi` (peer
of `crates/lazuli_codegen_go`). It walks `package.features.commands`
+ `package.features.agents` (for `expose_http`) + the typed `api` IR
once Tier 4 lifts it.

### IR-to-OpenAPI mapping

| IR slot | OpenAPI 3.1 slot | Notes |
|---|---|---|
| `Feature.name` + `Command.name` | `paths./api/<feature>/<command>.post.operationId` | `operationId = "<feature>_<command>"` |
| `CommandKind::Create` | `post` | conventional HTTP method per kind |
| `CommandKind::Update` | `patch` | (or `put` if `route` slots resolve a full identity; default to `patch`) |
| `CommandKind::Delete` | `delete` | |
| `CommandKind::Returns` | `post` | request/response shape, no effect |
| `Command.route: Vec<RouteSlot>` | `paths.<path>.parameters[].in=path` | path param per slot; `RouteSlot.type_ref` → OpenAPI schema |
| `Command.input: CommandInput::Typed(slots)` | `requestBody.content."application/json".schema` | inline object schema; each `TypedSlot` is one property |
| `Command.input: CommandInput::Short(field_names)` | `requestBody.content.…schema.$ref` | reference to the local `creates`/`updates` resource record |
| `Command.input: CommandInput::Empty` | no `requestBody` | |
| `Command.effect::Creates.resource` | `responses.201.content."application/json".schema.$ref` | `#/components/schemas/<Resource>` |
| `Command.effect::Updates.resource` | `responses.200.content."application/json".schema.$ref` | |
| `Command.effect::Deletes` | `responses.204` (no content) | |
| `Command.effect::Returns(ReturnsEffect)` | `responses.200.content."application/json".schema` | |
| `Command.policy: PolicyRef` | `security: [{<scheme>: [<scope_atoms>]}]` | atoms in policy walked transitively (reachability already in doctor) |
| `Command.emits` | `x-lazuli-emits: [...]` extension | not OpenAPI core; readable downstream |
| `Command.deprecated.since` | `deprecated: true`, `x-lazuli-deprecated-since: "..."` | bare `deprecated` is also `deprecated: true` |
| `Command.deprecated.replacement` | `x-lazuli-replacement: "..."` | |
| `Command.deprecated.sunset` | `x-lazuli-sunset: "..."` extension + response `Sunset` header schema | runtime header wiring is Drusa |
| `Resource` declaration | `components.schemas.<Resource>` | walks `Resource.fields`, lowering each `TypeRef` |
| `TypeRef::Builtin(Text/Int/Bool/DateTime/...)` | OpenAPI scalar + `format` | `DateTime` → `format: "date-time"`, etc. |
| `TypeRef::UserDefined(<Resource>)` | `$ref: #/components/schemas/<Resource>` | |
| `TypeRef::EnumRef(<Enum>)` | `$ref: #/components/schemas/<Enum>` + `components.schemas.<Enum>` with `enum: [...]` | |
| `TypeRef::Many(T)` | `type: array, items: <T>` | |
| `TypeRef::Capability(CapabilityRef::File(fc))` | `type: string, format: binary, x-lazuli-max-size, x-lazuli-accept, x-lazuli-visibility, x-lazuli-signed-ttl` | post-Tier 2 typed input/output |
| `@semantic.Email` on a field | `format: "email"` | |
| `@semantic.Phone` | `format: "phone"` (OpenAPI 3.1 doesn't define this; `x-lazuli-format` fallback) | |
| `@semantic.URL` | `format: "uri"` | |
| `@semantic.Date` | `format: "date"` | |
| `@semantic.UUID` | `format: "uuid"` | |
| `@pii.<class>` on a field | `x-pii: "<class>"` extension | downstream consumers (SDK gen) read this for redaction |
| `app.errors` block | `responses.4XX/5XX.content."application/problem+json".schema` (RFC 7807) | typed once errors block lifts to IR (post-Tier 4) |
| `AgentBlock.expose_http` | `paths.<path>.post.…` operation block | `HttpMethod` → method; `route_slots` → path params; agent name → `operationId` |
| `Webhook` (typed via Tier 3) | `paths./webhooks/<feature>/<name>.post.…` | inbound webhook signature surfaces as `securitySchemes` HMAC |

### Inspect JSON shape

Two new `--expand` flags:

1. **`--expand=deprecation`** — projects every `Deprecation`-bearing
   slot (commands today; apis post-Tier 4):

   ```json
   {
     "features": [
       {
         "name": "customer",
         "deprecations": [
           {
             "kind": "command",
             "name": "reassign",
             "since": "2026.04",
             "replacement": {
               "kind": "LocalCommand",
               "value": "reassign_v2"
             },
             "sunset": "2026-12-31",
             "origin": "examples/full-capsule/full-capsule.lzi:265"
           }
         ]
       }
     ]
   }
   ```

2. **`--expand=openapi`** — emits the OpenAPI-mappable summary per
   feature without the full schema chain. Useful for cheap "what would
   the spec look like?" probes:

   ```json
   {
     "features": [
       {
         "name": "customer",
         "openapi_summary": {
           "operations": [
             {
               "method": "POST",
               "path": "/api/customer/capture_lead",
               "operationId": "customer_capture_lead",
               "input_shape": "typed:{name,email}",
               "output_shape": "$ref:Customer",
               "policy_atoms": ["@role.lead_capture_user"],
               "emits": ["customer_created"],
               "deprecated": false,
               "kind": "command"
             }
           ],
           "operations_typed": 8,
           "operations_text_pattern": 1,
           "schemas": ["Customer", "CustomerStatus", "CustomerTier", "..."]
         }
       }
     ]
   }
   ```

Normalisation rules:

- `operations_typed` + `operations_text_pattern` sum to the total
  HTTP-bearing surface count. As Tier 4 lifts more, the text-pattern
  count drops to zero.
- `policy_atoms` is the **transitively-resolved** set; doctor already
  computes this for policy reachability — the OpenAPI projection
  reuses the same walker.
- Without `--expand=openapi` the key is omitted (mirrors the
  agent/tools convention).

### Changelog command shape

`lazuli changelog --since v0.6.0` runs the analyzer pipeline at two
git refs, serialises both as `lazuli inspect --format=json
--expand=openapi`, diffs the two payloads field-by-field, and emits:

```markdown
# API Changelog (v0.6.0 → HEAD)

## Removed
- `POST /api/customer/old_reassign` (commit b3fc39e, `feature customer`)

## Added
- `POST /api/customer/reassign_v2` (commit 3be8611, `feature customer`)

## Deprecated
- `POST /api/customer/reassign` (since 2026.04, replacement
  `reassign_v2`, sunset 2026-12-31)

## Breaking
- `POST /api/customer/create` — input field `name` changed from
  `optional` to `required` (commit ac0241d)

## Non-breaking
- `POST /api/customer/capture_lead` — added optional response field
  `tracking_id` (commit b0304b4)
```

The classification rule (breaking vs non-breaking) is the standard
diff:

- **Breaking**: removed operation; removed input field; required→optional
  output flip; type-narrowed input; widened input enum; widened required;
  removed response code.
- **Non-breaking**: added operation; added optional input; added
  required output; deprecation toggle.

The doctor surface for the changelog is a single diagnostic:
`api_changelog_breaking_change` warning when a `lazuli check` runs
in a CI workflow with `--since main` and the diff contains a
breaking change. Severity is configurable per profile.

## Codegen (Stage 5)

Two new emitter crates. Output is **the artifact**, not generated Go.

### `crates/lazuli_openapi/`

New peer of `crates/lazuli_codegen_go`. Owns:

- `src/lib.rs` — public `emit(package: &ir::Package, opts:
  EmitOptions) -> Result<OpenApi31>`.
- `src/walk.rs` — IR walker that produces a typed `OpenApi31` value.
- `src/schemas.rs` — `TypeRef` → `Schema` lowering, with the
  `@semantic.*` / `@pii.*` rules from Stage 4.
- `src/serialize.rs` — YAML (default) + JSON output via `serde_yaml`
  / `serde_json` (already in workspace deps).

Public crate-shape:

```rust
// crates/lazuli_openapi/src/lib.rs
pub struct EmitOptions {
    /// API version reported in `info.version`. Defaults to
    /// `AppManifest.api_version`, then to git-tag, then to `0.0.0`.
    pub api_version: Option<String>,
    /// When true, skips any operation whose IR is text-pattern only
    /// (i.e. `api` blocks pre-Tier 4). When false, emits a placeholder
    /// with `x-lazuli-text-pattern-skip: true`. Default false; CI
    /// can opt into strict mode via `--strict`.
    pub strict_typed_only: bool,
    /// Profile selector — when set, overlays `profiles/<name>.lzi`
    /// onto the package before emission (URLs, audiences).
    pub profile: Option<String>,
}

pub fn emit(
    package: &ir::Package,
    opts: EmitOptions,
) -> Result<OpenApi31, EmitError>;
```

### `crates/lazuli_changelog/`

New peer crate. Owns:

- `src/lib.rs` — `diff(old: &ir::Package, new: &ir::Package) ->
  ChangelogReport`.
- `src/breaking.rs` — classification rules.
- `src/markdown.rs` — markdown emitter.
- `src/json.rs` — JSON emitter for downstream tooling.

Boundary discipline: **no provider names anywhere**. The crates emit
OpenAPI 3.1 spec text + markdown delta text. They do **not** generate
Go server stubs, Python clients, TypeScript SDKs, or anything else —
those are downstream artifacts produced by **adapters** consuming the
spec, not by Lazuli core.

### Generated artifact shape (example, from `feature customer`)

`dist/openapi/customer-export.yaml` (partial, for the single api):

```yaml
openapi: 3.1.0
info:
  title: customer
  version: 0.6.0
  x-lazuli-feature: customer
paths:
  /api/customer/capture_lead:
    post:
      operationId: customer_capture_lead
      x-lazuli-policy: "@policy.capture_lead"
      x-lazuli-rate-limit: "10 per minute per ip"
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [name, email]
              properties:
                name: { type: string }
                email:
                  type: string
                  format: email
                  x-pii: contact
      responses:
        '201':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Customer'
        '4XX': { $ref: '#/components/responses/Problem' }
        '5XX': { $ref: '#/components/responses/Problem' }
  /api/customer/reassign:
    patch:
      operationId: customer_reassign
      deprecated: true
      x-lazuli-deprecated-since: "2026.04"
      x-lazuli-replacement: "customer.command.reassign_v2"
      x-lazuli-sunset: "2026-12-31"
      x-lazuli-audit: ["actor", "target.id", "input.owner_id"]
      x-lazuli-audit-emit-to: "audit_log"
      x-lazuli-approval:
        required_when: "target.tier = enterprise"
        by: "@role.admin"
        timeout: "24h"
        then: deny
      ...
components:
  schemas:
    Customer:
      type: object
      properties:
        id: { type: string, format: uuid }
        name: { type: string }
        email:
          type: string
          format: email
          x-pii: contact
        lifecycle_stage:
          $ref: '#/components/schemas/CustomerStatus'
        ...
    CustomerStatus:
      type: string
      enum: [lead, active, paused, archived]
  responses:
    Problem:
      content:
        application/problem+json:
          schema:
            type: object
            required: [type, title, status]
            properties:
              type: { type: string, format: uri }
              title: { type: string }
              status: { type: integer }
              detail: { type: string }
              instance: { type: string }
```

### Types reused from `runtime/go/lazuli`

**None.** OpenAPI emission is a Lazuli-core artifact; no Go runtime
code is needed. The optional `lazuli serve --openapi /openapi.yaml`
endpoint (which mounts the generated artifact under the live HTTP
server) is a runtime feature, scoped to a separate row.

## Runtime (Stage 6)

**Minimal stub — read-only.** OpenAPI is an artifact, not a runtime
capability. The only runtime touch is:

### `runtime/go/lazuli/openapi/serve.go` (new, stub)

- **Capability**: optionally mount the generated `openapi.yaml` (or
  `.json`) under a configurable path. The runtime reads the artifact
  from disk at boot.
- **Lifecycle**: stateless. One `http.HandlerFunc` that returns the
  artifact with `Content-Type: application/yaml` (or
  `application/json`).
- **Config**: a single field on `AppManifest.observability` (or a
  new optional `AppManifest.openapi: Option<OpenApiServe>`) that
  controls path + format. Default: not mounted (the artifact is a
  build output, not a runtime endpoint).
- **Dependency**: stdlib `net/http` + `os.ReadFile`. No new
  third-party deps.
- **Typed errors**: none.

This is **the entire runtime surface** for this bucket. Adapters
that want to ingest the OpenAPI artifact for SDK generation
(`@plugin/<publisher>/openapi-typescript`, `@plugin/<publisher>/openapi-python`)
sit downstream of the artifact; Lazuli core doesn't ship them.

Boundary discipline (re-stated): the OpenAPI bucket has **almost no
runtime surface** because OpenAPI is a contract-publication artifact.
Server stub generation, OpenAPI validation middleware, OpenAPI UI
(Swagger UI / Redoc / Stoplight) — these are **DF** (framework /
runtime) per audit `:228` and **stay in Drusa adapters**, not in
Lazuli core. The first cut only generates the spec; downstream tools
read it.

## Evals/Testes (Stage 7)

### Doctor fixture — deprecated replacement unknown

`crates/lazuli_cli/tests/fixtures/openapi/deprecated_replacement_unknown.lzi`:

```lzi
feature x_dep
  domain
    resource Foo
      id: ID required
  command old_op
    deprecated since: "2026.01", replacement: nonexistent_op
    input
      id: ID required
    policy @policy.read
    updates Foo
      id = input.id
```

Asserts that doctor emits **exactly one**
`deprecated_replacement_unknown` diagnostic at the `replacement:`
slot.

### Doctor fixture — sunset date invalid

`crates/lazuli_cli/tests/fixtures/openapi/sunset_date_invalid.lzi`:
authors `deprecated sunset: "2026/12/31"` (slash separator) — asserts
the typed lowering rejects with `deprecated_sunset_date_invalid`.

### Golden artifact — full-capsule OpenAPI

`tests/golden/openapi/full_capsule.openapi.yaml` — frozen output of
`lazuli generate --openapi examples/full-capsule/full-capsule.lzi
--output -`. Used as a regression-style golden: any IR change that
shifts the artifact must update the golden, which forces reviewers
to look at the diff.

### Changelog Go-host test

`crates/lazuli_changelog/tests/diff_test.rs`:

- Parse two synthetic `ir::Package` values (one with `command
  reassign`, one with `command reassign_v2` + the deprecation on
  `reassign`).
- Run `diff(old, new)` and assert the markdown contains exactly:
  one `## Added`, one `## Deprecated`, zero `## Breaking`.
- Add a third case: removed field on `creates Customer` →
  `## Breaking` count = 1.

### LSP test — `deprecated` hover + completion

`crates/lazuli_lsp/tests/openapi.rs`:

- Hover on `deprecated` keyword shows: "Marks the command as
  deprecated. Optional sub-fields: `since:`, `replacement:`,
  `sunset:`. Surfaces as OpenAPI `deprecated: true` + `Sunset`
  header."
- Completion after `deprecated ` offers `since:`, `replacement:`,
  `sunset:`.
- Hover on `deprecated since: "2026.04"` shows the version string +
  the resolved replacement target (if declared).
- Completion at `replacement: ` offers the symbol table of
  same-feature commands.

### Inspect contract test

`crates/lazuli_cli/tests/inspect_openapi.rs`: runs `lazuli inspect
--format=json --expand=openapi,deprecation examples/full-capsule`
and asserts the projection matches the JSON shape in Stage 4 (typed
operations count, `text_pattern` count, deprecation entries).

## Doctor/LSP (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `deprecated_replacement_unknown` | error | "command `<X>`.deprecated.replacement `<Y>` does not resolve: `<reason>`." Reason ∈ { `same-feature command not found`, `cross-feature reference malformed`, `url malformed` } | `Deprecation.replacement::LocalCommand` does not match a same-feature `command`; `Qualified` does not resolve; `Url` fails RFC 3986 shape check | `deprecated_replacement_unknown.lzi` |
| `deprecated_sunset_date_invalid` | error | "command `<X>`.deprecated.sunset `<Y>` is not a valid ISO-8601 date (`YYYY-MM-DD`)." | parser produces a `sunset` string that fails the `YYYY-MM-DD` format check | `sunset_date_invalid.lzi` |
| `deprecated_sunset_in_past` | warning | "command `<X>`.deprecated.sunset `<Y>` is in the past; consumers should expect this endpoint to be removed soon." | `sunset < today` at lowering time (clock from `chrono`) | `sunset_past.lzi` |
| `openapi_text_pattern_api_block` | warning | "api `<X>` is text-pattern; OpenAPI emission falls back to a stub with `x-lazuli-text-pattern-skip: true`. Lift to typed IR via Phase L Tier 4." | `Feature.apis` (text-pattern) is non-empty | the canonical fixture; the diagnostic surfaces 1× for `customer_export` until Tier 4 lands |
| `api_changelog_breaking_change` | warning (CI default) / error (`--strict`) | "API changelog vs `<base>` contains a breaking change: `<diff>`." | invoked only when `lazuli check --since <ref>` runs (e.g. in CI) | synthesised in `crates/lazuli_changelog/tests/` |

All five codes register under `is_security_enforcement_code`
(`crates/lazuli_lsp/src/lib.rs:9527`) for profile severity upgrades.

### Diagnostic anchors (where to add)

- `deprecated_replacement_unknown` — cross-feature pass in
  `crates/lazuli_cli/src/doctor.rs` once IR carries
  `Command.deprecated`. Reuses the symbol table built for `previously
  migrated` (which already resolves command names).
- `deprecated_sunset_date_invalid` — file-local in LSP (parse-time
  rule) and cross-feature in doctor (re-check at lowering).
- `deprecated_sunset_in_past` — doctor only (`chrono` for "today",
  not a hot-path LSP rule).
- `openapi_text_pattern_api_block` — doctor only, runs whenever the
  package contains a text-pattern `api` block. Disappears
  automatically once Tier 4 lifts every `api`.
- `api_changelog_breaking_change` — `lazuli_changelog` crate; called
  by `lazuli check --since`. Not a stand-alone doctor pass on a
  single file.

### LSP hovers (new entries)

Add to `KEYWORD_HOVER` in `crates/lazuli_lsp/src/lib.rs`:

| Keyword | Hover summary |
|---|---|
| `deprecated` | "Marks the command (or api, post-Tier-4) as deprecated. Optional `since:` (version), `replacement:` (command name / qualified ref / URL), `sunset:` (ISO-8601 date). Generates OpenAPI `deprecated: true` + `Sunset` HTTP header." |
| `since` (in `deprecated` context) | "Version string when the deprecation was declared. Free-form (semver, calendar, git-sha); emitted verbatim as `x-lazuli-deprecated-since`." |
| `replacement` | "Replacement reference for a deprecated command. Resolves to a same-feature command, a `<feature>.command.<name>` qualified ref, or a URL." |
| `sunset` | "ISO-8601 date (`YYYY-MM-DD`) when consumers must stop using this endpoint. Emitted as OpenAPI `x-lazuli-sunset` and HTTP `Sunset` header." |

Closed-catalog completions to add:

- After `deprecated `: `since:`, `replacement:`, `sunset:`.
- After `replacement: `: same-feature command names from the symbol
  table (existing helper used for `previously migrated`).

### Namespaces (`is_allowed_reference_namespace`)

No new namespace required. `replacement` refs reuse the existing
qualified-tool resolver (`crates/lazuli_ir/src/lib.rs:2508`) for the
`<feature>.command.<name>` case.

### Highlighting

Add `deprecated | since | replacement | sunset` to the keyword scope
in `editors/vscode/syntaxes/lazuli.tmLanguage.json`. ISO-8601 date
literals hit existing string-content scope.

## Critério de "ciclo fechado"

- [ ] Fixture exercises `deprecated` on `command reassign` (Stage 3
      inline example) — at least one of bare flag form and one fully-
      decorated form.
- [ ] `lazuli check examples/full-capsule` accepts the syntax (text-
      pattern lowering for `deprecated` lands before Tier 4; typed
      lowering lands with Tier 4).
- [ ] `lazuli inspect --format=json --expand=deprecation,openapi
      examples/full-capsule` shows the projections in Stage 4.
- [ ] `lazuli doctor` emits the 5 named diagnostics on the matching
      fixtures.
- [ ] `lazuli generate --openapi examples/full-capsule` produces
      `dist/openapi/full-capsule.yaml` that validates against the
      OpenAPI 3.1 schema (verified via a Go-host test consuming
      `kin-openapi` or `swagger-cli`).
- [ ] Golden artifact `tests/golden/openapi/full_capsule.openapi.yaml`
      stays current via `cargo test`.
- [ ] `lazuli changelog --since v0.6.0` runs against the current
      repo state and emits a valid markdown report (verified via
      a Go-host test fixture).
- [ ] LSP hovers + completion cover the 4 keywords + 1 closed-
      catalog completion (`replacement:`).

## Próximo passo

Human approval of this proposal + a separate `mode=implement` run
that lands:

1. `Command.deprecated: Option<Deprecation>` + `Deprecation` /
   `DeprecationReplacement` types in `crates/lazuli_ir/src/lib.rs:501`.
2. Text-pattern lowering for `deprecated` in `crates/lazuli_cli/src/
   main.rs` walker (until Tier 4 lifts `parse_command`).
3. New crate `crates/lazuli_openapi` with `emit` + IR walker +
   schema lowering.
4. New crate `crates/lazuli_changelog` with `diff` + breaking-change
   classifier.
5. New CLI verbs `lazuli generate --openapi` and `lazuli changelog`
   in `crates/lazuli_cli/src/main.rs`.
6. 5 doctor diagnostics + 4 LSP hovers + 2 `--expand` projections.
7. Runtime stub `runtime/go/lazuli/openapi/serve.go` (optional
   artifact-mount handler, ~20 LOC).
8. Golden artifact + 3 doctor fixtures + 1 LSP test + 1 inspect
   contract test.

The `api`-block lift to typed IR is **explicitly out of scope** —
that's Phase L Tier 4 (`docs/next-checklist.md:60` row 24). This
bucket emits OpenAPI for the **91% typed slice** today, surfaces the
9% gap via `openapi_text_pattern_api_block`, and shrinks the gap to
0% mechanically when Tier 4 lands.

Pointer: `docs/proposals/bucket-openapi-scope.md` carries the
upstream blocker analysis (api-block lift is required to reach 100%
coverage; this proposal accepts the 91% partial as the first cut).

## Rows sugeridas para `docs/next-checklist.md`

Three additions, formatted to match the existing table:

```
| 38 | OpenAPI bucket cycle Route C — `deprecated` decorator + `Deprecation` IR + text-pattern lowering | planned | Additive `Command.deprecated: Option<Deprecation>` in `crates/lazuli_ir/src/lib.rs:501`. Text-pattern lowering in the legacy `inspect_command` walker until Phase L Tier 4 lifts `parse_command`. New `--expand=deprecation` / `--expand=openapi` projections. See `docs/proposals/bucket-openapi-cycle.md` §Linguagem/§IR. |
| 39 | OpenAPI bucket cycle — `lazuli generate --openapi` + `lazuli changelog` | planned | New `crates/lazuli_openapi` (IR walker, schema lowering, YAML/JSON emit) + `crates/lazuli_changelog` (IR diff, breaking-change classifier, markdown/JSON emit). Two new CLI verbs in `crates/lazuli_cli/src/main.rs`. Golden artifact `tests/golden/openapi/full_capsule.openapi.yaml` + breaking-change test. Depends on row 38. See `docs/proposals/bucket-openapi-cycle.md` §Codegen/§Evals. |
| 40 | OpenAPI bucket cycle — 5 doctor diagnostics + LSP coverage | planned | `deprecated_replacement_unknown`, `deprecated_sunset_date_invalid`, `deprecated_sunset_in_past`, `openapi_text_pattern_api_block`, `api_changelog_breaking_change`. 4 LSP hovers (`deprecated`, `since`, `replacement`, `sunset`) + 1 closed-catalog completion for `replacement:`. Depends on row 38. See `docs/proposals/bucket-openapi-cycle.md` §Doctor/LSP. |
```
