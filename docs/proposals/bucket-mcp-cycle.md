# Bucket Cycle: MCP (L0→L2)

**Status**: design proposal — L0 surface + L1 IR mirror + L2 codegen
+ runtime wire. Implementation deferred to a separate run with
`mode=implement`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team,
codegen-go owners, downstream product authors who want to expose
their Lazuli app to AI agents.

**Date**: 2026-05-13.

**Pilot bucket**: greenfield infrastructure cycle. This is **not** a
domain-feature bucket — it adds two new authoring kinds
(`mcp_server`, `mcp_client`) and a new runtime contract that any
feature bucket can layer on top of.

**Companion**: `docs/architecture.md` §"founding principle" (wire-thin
discipline), `docs/proposals/bucket-ai-debug-loop-cycle.md` (typed
error envelope MCP errors reuse), `docs/design-principles.md`
(Rule Zero — Vocabulary Over Mechanism).

**First consumer**: Pleiades v2 (Phase C of the strategic pivot
documented at `~/.claude/projects/c--Users-lucas-lazuli/memory/project_strategic_pivot_2026-05-13.md`).
Pleiades v2 will expose `mcp_server` for its knowledge tools (search
slugs, get slug, propose item) and consume `mcp_client` for
ChromaDB similarity search.

---

## Contexto

The AI-first thesis of Lazuli is that every SaaS built with the
framework should be **agent-accessible by default**. In 2026 the
de-facto interop standard for "AI tool calls some external system"
is the **Model Context Protocol** (MCP, originally specified by
Anthropic, now adopted by Cursor, OpenAI's Codex CLI, Continue, and
the broader agent ecosystem). MCP defines three transport-agnostic
surfaces:

| MCP surface | Shape | Use |
|---|---|---|
| **Tools** | function with typed params + structured return | `payments.refund(orderId, reason)` |
| **Resources** | read-only addressable data (URI + MIME) | `db://orders/{id}.json` |
| **Prompts** | templated message lists with parameters | `support/triage_ticket(severity)` |

Today Lazuli emits HTTP routes (REST), OpenAPI specs, and event
emitters. **None of those surfaces are MCP-compatible.** A Lazuli
app cannot be consumed by Claude Desktop, Cursor, or a paperclip
agent without the author hand-rolling MCP server boilerplate
against `github.com/modelcontextprotocol/go-sdk` (or a community
SDK). At the wire-thin LOC ceiling, hand-rolling on every product
port is the wrong answer — it duplicates auth, validation, error
mapping, transport selection, and config plumbing in every
downstream app.

The MCP bucket closes that gap by:

1. **Adding two `.lzi` kinds** (`mcp_server <name>`, `mcp_client
   <name>`) so authors declare MCP surfaces in the same vocabulary
   as `feature` / `kind` / `notification`.
2. **Lowering to IR** so doctor, scope inspectors, and codegen all
   read the MCP contract from a single source.
3. **Codegen → Go** that emits ~30-80 LOC per server/client wiring
   the Lazuli Go runtime's MCP helper against the upstream SDK.
4. **Runtime helper `runtime/go/lazuli/mcp/`** — ~120 LOC wire of
   the upstream Go SDK with Lazuli envelope mapping
   (`*lazuli.Error` → MCP error response, `WithSource` context
   threading).

Beyond the immediate Pleiades v2 dependency, three adjacent risks
motivate this bucket:

5. **Plugin ecosystem leverage**. Any `@plugin/<name>` adapter
   (Stripe, MercadoPago, Datadog, Sentry, ...) can ship a
   prepackaged `mcp_server` block authors enable with one line.
   That converts every plugin into an agent-native integration for
   free.
6. **Cross-product orchestration**. A Lazuli app and a Pleiades v2
   knowledge layer talk via MCP rather than custom HTTP. Two
   Lazuli apps in a multi-product paperclip company share tools
   via MCP. The protocol becomes the language of inter-app
   composition.
7. **Lazuli-the-framework vs Lazuli-the-product separation**. The
   framework should expose itself as MCP too (introspection of
   running IR, doctor results, generated stubs). That's a
   follow-up bucket, but the runtime helper landed here is
   reusable.

The bucket lands **ten cells (M1–M10)** that together yield a
loop in which:

- **Authors write `mcp_server checkout { tool refund { ... } }`** —
  no hand-rolled SDK invocations, no transport selection, no
  registration boilerplate.
- **The IR carries MCPServer / MCPClient as first-class shapes**, so
  doctor enforces name uniqueness, tool-param schema sanity, and
  transport-allowlist policy.
- **Codegen emits a single `.gen.go` per mcp_server** registering
  tools/resources/prompts against the runtime helper.
- **Generated Go imports nothing beyond `errors`, `context`, a
  single MCP SDK package, and existing Lazuli runtime helpers** —
  the wire-thin acceptance test.

The closed-cycle criterion is the §0 8-item checklist (fixture +
check + inspect + doctor lint + generate Go + Lazuli Go runs +
eval/test + LSP hover) applied per cell. Each cell carries its own
smoke in §Cycle.

---

## Baseline (Stages 1-2 inventory)

| Surface | Today | Anchor | L-level |
|---|---|---|---|
| MCP server support in Lazuli runtime | none | `runtime/go/lazuli/` (no `mcp/` package) | **missing** |
| MCP client support in Lazuli runtime | none | n/a | **missing** |
| `mcp_server` block in grammar | not parsed | `crates/lazuli_grammar/` | **missing** |
| `mcp_client` block in grammar | not parsed | `crates/lazuli_grammar/` | **missing** |
| IR `MCPServerSpec` / `MCPClientSpec` | not declared | `crates/lazuli_ir/src/lib.rs` | **missing** |
| Codegen Go `mcp_server.gen.go` emitter | not implemented | `crates/lazuli_codegen_go/src/emitter/` | **missing** |
| Codegen Go `mcp_client.gen.go` emitter | not implemented | `crates/lazuli_codegen_go/src/emitter/` | **missing** |
| Doctor checks `MCP-*` codes | none | `crates/lazuli_cli/src/doctor.rs` | **missing** |
| Example fixture `examples/mcp-smoke/` | does not exist | n/a | **missing** |
| `lazuli mcp list` verb | does not exist | `crates/lazuli_cli/` | **missing** |
| Upstream Go SDK pin | not in any `go.mod` | n/a | **missing** |
| `@plugin/<name>` MCP server preset | not designed | `docs/plugin-authoring.md` | **missing** |

**Cross-cutting fact**: like the AI Debug Loop bucket, every gap is
**additive**. The MCP bucket widens grammar + IR + codegen on the
production side, adds a new runtime package, and grows the doctor
table. No existing surface is removed or has its semantics changed.

---

## Surface design (L0)

### `mcp_server <name>` kind

Top-level kind, declared at app scope or feature scope. Names a
single MCP server endpoint that the deployed app exposes.

```lzi
mcp_server pleiades_knowledge
  transport stdio                      # or http_sse, http_streamable
  scope feature.knowledge              # which feature surface this serves
  auth bearer env.PLEIADES_MCP_TOKEN   # optional; absent = unauthenticated
  metadata
    name "Pleiades Knowledge"
    description "Curated knowledge graph for AI agents"
    version "1.0.0"
  tool search_slugs
    description "Search slugs by query and optional tags"
    params
      query: string required
      tags: list of string optional
    returns SlugSearchResult
    handler @fn.search_slugs
  tool get_slug
    description "Get a single slug with its items"
    params
      key: string required
    returns SlugDetail
    handler @fn.get_slug
  resource slug
    uri_template "slug://{workspace}/{key}"
    mime "application/json"
    handler @fn.read_slug_resource
  prompt summarize_slug
    description "Generate a summary of a slug's items"
    params
      key: string required
      audience: enum [technical, executive, beginner] required
    template "./prompts/summarize_slug.tmpl"
```

**Closed-catalog `transport`:** `stdio` | `http_sse` |
`http_streamable`. Doctor `MCP-TRANSPORT-001` rejects anything
else.

**`scope feature.<name>`** declares which feature surface the
server reads from. Doctor `MCP-SCOPE-001` rejects unknown feature
names; `MCP-SCOPE-002` enforces that `handler` references resolve
within the declared scope.

**`auth bearer env.<NAME>`** is the minimum auth shape. Future
expansions (`auth oauth ...`, `auth mtls ...`) extend the kind
without breaking the bearer form.

**`tool <name>` sub-block** mirrors the MCP tool surface. `params`
borrows the same type vocabulary already used by
`feature.commands`/`queries` (string, int, list of, enum [...]).
`returns <KindRef>` ties to an existing `kind` declaration in the
app — no anonymous response shapes (Rule Zero: vocabulary over
mechanism).

**`resource <name> uri_template "..."`** is intentionally
explicit — the URI shape is authoritative, not derived from a
struct. The `handler` is a domain function returning bytes +
content-type.

**`prompt <name>` sub-block** mirrors the MCP prompt surface.
`template "./prompts/..."` reuses the existing
`notifications.template` file convention (Go `text/template`).

### `mcp_client <name>` kind

Top-level kind, declared at app scope. Names a single external MCP
server the app consumes.

```lzi
mcp_client chroma_search
  transport stdio                      # or http_sse, http_streamable
  endpoint command "chroma mcp serve"  # for stdio
  # or:  endpoint url env.CHROMA_MCP_URL  # for http_*
  auth bearer env.CHROMA_MCP_TOKEN     # optional
  imports
    tool similarity_search(query: string, limit: int) returns SimilaritySearchResult
    tool embed_text(text: string) returns Embedding
    resource collection at "collection://{name}.json" returns CollectionInfo
  on_unavailable degrade           # or fail; closed catalog
```

**`endpoint command "..."`** for stdio spawn; **`endpoint url
env.<NAME>`** for HTTP-based transports.

**`imports`** — explicit allow-list of tools/resources/prompts
the app consumes. Doctor `MCP-CLIENT-IMPORT-001` validates that
imported names exist on the upstream server at boot (warm-up
probe); `MCP-CLIENT-IMPORT-002` enforces type compatibility with
local `kind` declarations.

**`on_unavailable degrade | fail`** — closed-catalog disposition
when the upstream server is not reachable. `degrade` returns a
typed `*lazuli.MCPClientUnavailable` error; `fail` panics through
to `observability.RecoverHTTP` and surfaces as 503.

### `@fn.<name>` resolution

Same convention as existing buckets — `handler @fn.search_slugs`
resolves to `features/<feature>/handlers/search_slugs.go`
exporting `func SearchSlugs(...)`. Doctor `MCP-HANDLER-001`
enforces filename match.

---

## L1 — IR mirroring

### `MCPServerSpec` (in `crates/lazuli_ir/src/lib.rs`)

```rust
pub struct MCPServerSpec {
    pub name: String,
    pub transport: MCPTransport,
    pub scope_feature: String,
    pub auth: Option<MCPAuth>,
    pub metadata: MCPServerMetadata,
    pub tools: Vec<MCPTool>,
    pub resources: Vec<MCPResource>,
    pub prompts: Vec<MCPPrompt>,
    pub span: Option<SpanRef>,
}

pub enum MCPTransport {
    Stdio,
    HttpSse,
    HttpStreamable,
}

pub enum MCPAuth {
    BearerEnvVar { env: String },
    // future: OAuth, mTLS
}

pub struct MCPTool {
    pub name: String,
    pub description: String,
    pub params: Vec<MCPParam>,
    pub returns_kind: KindRef,
    pub handler_fn: String,    // e.g. "@fn.search_slugs"
    pub span: Option<SpanRef>,
}

pub struct MCPResource {
    pub name: String,
    pub uri_template: String,
    pub mime: String,
    pub handler_fn: String,
    pub span: Option<SpanRef>,
}

pub struct MCPPrompt {
    pub name: String,
    pub description: String,
    pub params: Vec<MCPParam>,
    pub template_path: String,
    pub span: Option<SpanRef>,
}

pub struct MCPParam {
    pub name: String,
    pub ty: ParamType,            // reuse existing ParamType enum
    pub required: bool,
}
```

### `MCPClientSpec` (in `crates/lazuli_ir/src/lib.rs`)

```rust
pub struct MCPClientSpec {
    pub name: String,
    pub transport: MCPTransport,
    pub endpoint: MCPClientEndpoint,
    pub auth: Option<MCPAuth>,
    pub imports: Vec<MCPClientImport>,
    pub on_unavailable: MCPClientFailMode,
    pub span: Option<SpanRef>,
}

pub enum MCPClientEndpoint {
    Command(String),
    UrlEnvVar { env: String },
}

pub enum MCPClientImport {
    Tool { name: String, params: Vec<MCPParam>, returns_kind: KindRef },
    Resource { name: String, uri_template: String, returns_kind: KindRef },
    Prompt { name: String, params: Vec<MCPParam> },
}

pub enum MCPClientFailMode {
    Degrade,
    Fail,
}
```

### `App` IR struct extension

```rust
pub struct App {
    // existing fields...
    pub mcp_servers: Vec<MCPServerSpec>,
    pub mcp_clients: Vec<MCPClientSpec>,
}
```

---

## L2 — Runtime helper

### `runtime/go/lazuli/mcp/contract.go` (~50 LOC)

Defines the in-process contract that codegen emits against. Pure
interface + types; no I/O.

```go
package mcp

import (
    "context"
    "errors"
)

type Transport string

const (
    TransportStdio          Transport = "stdio"
    TransportHttpSSE        Transport = "http_sse"
    TransportHttpStreamable Transport = "http_streamable"
)

type ToolHandler func(ctx context.Context, args map[string]any) (any, error)

type ResourceHandler func(ctx context.Context, uri string) (bytes []byte, mime string, err error)

type PromptHandler func(ctx context.Context, args map[string]any) (messages []PromptMessage, err error)

type PromptMessage struct {
    Role    string
    Content string
}

// ServerRegistration is the shape codegen emits per mcp_server.
type ServerRegistration struct {
    Name      string
    Transport Transport
    Auth      *AuthSpec
    Metadata  ServerMetadata
    Tools     []ToolRegistration
    Resources []ResourceRegistration
    Prompts   []PromptRegistration
}

type ToolRegistration struct {
    Name        string
    Description string
    InputSchema map[string]any   // JSON Schema-shaped
    Handler     ToolHandler
}

// ... ResourceRegistration, PromptRegistration similar

// Typed errors.
var (
    ErrMCPInvalidArgs        = errors.New("mcp: invalid tool arguments")
    ErrMCPHandlerFailed      = errors.New("mcp: handler returned error")
    ErrMCPTransportUnsupported = errors.New("mcp: transport not supported")
    ErrMCPClientUnavailable  = errors.New("mcp: client transport unavailable")
)
```

### `runtime/go/lazuli/mcp/server.go` (~80 LOC)

Wire of upstream Go SDK for the server side. Single file. Single
import block beyond stdlib: the SDK package.

```go
package mcp

import (
    "context"
    sdk "github.com/modelcontextprotocol/go-sdk/server"  // exact path verified at impl
)

// Serve binds a ServerRegistration to the upstream SDK and starts
// the chosen transport. Returns when the transport exits or ctx
// is cancelled.
func Serve(ctx context.Context, reg ServerRegistration) error {
    s := sdk.NewServer(sdk.ServerConfig{
        Name:    reg.Metadata.Name,
        Version: reg.Metadata.Version,
    })
    for _, t := range reg.Tools {
        s.RegisterTool(t.Name, t.Description, t.InputSchema, wireTool(t.Handler))
    }
    for _, r := range reg.Resources {
        s.RegisterResource(r.URITemplate, r.MIME, wireResource(r.Handler))
    }
    for _, p := range reg.Prompts {
        s.RegisterPrompt(p.Name, p.Description, p.InputSchema, wirePrompt(p.Handler))
    }
    switch reg.Transport {
    case TransportStdio:
        return s.ServeStdio(ctx)
    case TransportHttpSSE:
        return s.ServeSSE(ctx, reg.Auth)
    case TransportHttpStreamable:
        return s.ServeStreamable(ctx, reg.Auth)
    default:
        return ErrMCPTransportUnsupported
    }
}

// wireTool, wireResource, wirePrompt adapt Lazuli's typed errors
// into the SDK's response envelope. ~20 LOC total.
```

### `runtime/go/lazuli/mcp/client.go` (~80 LOC)

Wire of upstream Go SDK for the client side. Same shape — single
import beyond stdlib, thin adapter for typed errors.

### `runtime/go/lazuli/mcp/test/` (~60 LOC across files)

Smoke tests that spin up an in-process stdio server + client pair,
exchange one tool call, verify envelope mapping.

**Wire-thin acceptance**: `runtime/go/lazuli/mcp/` totals < 300
effective LOC across all `.go` files (including tests) with
exactly one external import (`modelcontextprotocol/go-sdk`).

---

## Cells (M1 – M10)

### M1 — Grammar: `mcp_server` kind

**File**: `crates/lazuli_grammar/src/...` (single grammar file).

**Spec**: parser accepts `mcp_server <name>` block with the
sub-blocks specified in §"Surface design". Reject unknown keys at
parse time (closed grammar invariant per `docs/invariants.md`).

**Tests**: parse acceptance suite under `crates/lazuli_grammar/tests/`.

**Commit message**: `grammar: mcp_server kind`.

### M2 — Grammar: `mcp_client` kind

Same shape as M1, for `mcp_client`. Single grammar file edit.

### M3 — IR mirror: `MCPServerSpec`

**File**: `crates/lazuli_ir/src/lib.rs`. Add the types from
§"L1 — IR mirroring". Add lowering from grammar AST to IR. Add
serde JSON serialization (matches `LZIR_SCHEMA` versioning rules).

**Tests**: roundtrip JSON ↔ IR for the pleiades_knowledge fixture.

**Commit message**: `ir: MCPServerSpec mirror`.

### M4 — IR mirror: `MCPClientSpec`

Same shape as M3, for `MCPClientSpec`.

### M5 — Runtime contract: `runtime/go/lazuli/mcp/contract.go`

**File**: new file, single-file output. Types only — no I/O.

**Tests**: trivial unit tests for default values + typed error
identity.

**Wire-thin gate**: zero external imports.

### M6 — Runtime wire: `runtime/go/lazuli/mcp/server.go`

**File**: new file. Wire of upstream Go SDK.

**Spec**: implement `Serve(ctx, reg)`. Map Lazuli typed errors to
MCP response envelope. Stdio transport must work end-to-end against
the upstream SDK's matching client (per the SDK's own test suite).

**Tests**: `server_test.go` with in-process roundtrip (single tool
call exchange).

**Wire-thin gate**: ≤ 100 effective LOC; one external import.

### M7 — Runtime wire: `runtime/go/lazuli/mcp/client.go`

**File**: new file. Wire of upstream Go SDK client.

**Spec**: implement `Connect(ctx, spec) (*Client, error)`, expose
`Call`, `ReadResource`, `RenderPrompt`. Map upstream errors to
`ErrMCPClientUnavailable` / `ErrMCPInvalidArgs`.

**Wire-thin gate**: ≤ 100 effective LOC; one external import (same
SDK package).

### M8 — Codegen Go: `mcp_server` emitter

**File**: `crates/lazuli_codegen_go/src/emitter/mcp_server.rs`
emits `dist/go/<app>/mcp/<server_name>.gen.go` per `mcp_server`
in the IR.

**Generated shape** (per server, ~40-60 LOC):

```go
// Code generated by lazuli; DO NOT EDIT.
// source: app.lzi:42

package mcp_pleiades_knowledge

import (
    "context"
    "lazuli.dev/runtime/lazuli"
    "lazuli.dev/runtime/lazuli/mcp"
    fns "myapp/features/knowledge/handlers"
)

var Registration = mcp.ServerRegistration{
    Name:      "pleiades_knowledge",
    Transport: mcp.TransportStdio,
    Metadata:  mcp.ServerMetadata{...},
    Tools: []mcp.ToolRegistration{
        {
            Name:        "search_slugs",
            Description: "Search slugs by query and optional tags",
            InputSchema: ..., // generated from params
            Handler:     wrapSearchSlugs,
        },
        // ...
    },
}

func wrapSearchSlugs(ctx context.Context, args map[string]any) (any, error) {
    ctx = lazuli.WithSource(ctx, lazuli.SourceFromGen("app.lzi", 42))
    query, ok := args["query"].(string)
    if !ok { return nil, mcp.ErrMCPInvalidArgs }
    // ... extract other params with type checks
    return fns.SearchSlugs(ctx, query, tags)
}

// Boot binds Registration into the running app's MCP server pool.
func Boot(ctx context.Context) error {
    return mcp.Serve(ctx, Registration)
}
```

**Tests**: snapshot test in `crates/lazuli_codegen_go/tests/`
against the pleiades_knowledge fixture; the resulting `.gen.go`
must compile under `go build`.

**Acceptance**: emitted file is < 100 LOC; imports only
`lazuli.dev/runtime/lazuli`, `lazuli.dev/runtime/lazuli/mcp`, and
the app's own handler package.

### M9 — Codegen Go: `mcp_client` emitter

**File**: `crates/lazuli_codegen_go/src/emitter/mcp_client.rs`
emits a strongly-typed wrapper per `mcp_client`.

**Generated shape** (per client, ~40-60 LOC):

```go
// Code generated by lazuli; DO NOT EDIT.

package mcp_chroma_search

import (
    "context"
    "lazuli.dev/runtime/lazuli/mcp"
)

type Client struct{ inner *mcp.Client }

func New(ctx context.Context) (*Client, error) {
    c, err := mcp.Connect(ctx, mcp.ClientSpec{
        Transport: mcp.TransportStdio,
        Endpoint:  mcp.EndpointCommand("chroma mcp serve"),
        OnUnavailable: mcp.FailModeDegrade,
    })
    if err != nil { return nil, err }
    return &Client{inner: c}, nil
}

func (c *Client) SimilaritySearch(ctx context.Context, query string, limit int) (SimilaritySearchResult, error) {
    out, err := c.inner.Call(ctx, "similarity_search", map[string]any{
        "query": query, "limit": limit,
    })
    if err != nil { return SimilaritySearchResult{}, err }
    return decodeSimilaritySearchResult(out)
}

// embed_text, ReadCollection, ... similar.
```

### M10 — Doctor + fixture: `examples/mcp-smoke/` + `MCP-*` codes

**Files**:
- `examples/mcp-smoke/app.lzi` + `features/echo/echo.lzi` exercising
  both `mcp_server` and `mcp_client` (server exposes one tool;
  client consumes a dummy upstream).
- `examples/mcp-smoke/features/echo/handlers/echo.go` (one domain
  function).
- `crates/lazuli_cli/src/doctor.rs` adds `MCP-NAME-001` (name
  uniqueness across servers + clients), `MCP-TRANSPORT-001`
  (closed catalog), `MCP-SCOPE-001`/`-002` (handler scope checks),
  `MCP-CLIENT-IMPORT-001`/`-002` (import validation), `MCP-HANDLER-001`
  (filename match), `MCP-PROMPT-TEMPLATE-001` (template file exists).

**Acceptance**: `lazuli check examples/mcp-smoke/` is green;
`lazuli generate examples/mcp-smoke/` emits compilable Go; the
generated `mcp/echo_server.gen.go` boots, registers, and exchanges
one tool call with the SDK's reference client (in-process test).

---

## Acceptance (cycle-level)

- `examples/mcp-smoke/` doctor-green and codegen-green.
- `cargo check --all-targets` green.
- `go test ./lazuli/mcp/...` green.
- All other existing fixtures (`examples/full-capsule/`,
  `examples/auth-roundtrip/`, etc.) stay green — MCP bucket lands
  as **additive** widening.
- Runtime `runtime/go/lazuli/mcp/` is < 300 effective LOC with one
  external import (the upstream Go SDK).
- Generated `mcp_*.gen.go` files are < 100 LOC each.
- LSP hover on `mcp_server`/`mcp_client`/`tool`/`resource`/`prompt`
  returns specs lifted from this proposal.

---

## Risks

| Risk | Mitigation |
|---|---|
| Upstream Go SDK does not exist with a stable surface | Verify the canonical SDK at impl time. If no official one is stable, fall back to a maintained community SDK (e.g. `github.com/mark3labs/mcp-go`); the bucket interface stays the same. The implementing cell pins the choice and updates this proposal in-place. |
| MCP protocol spec drifts | Pin a protocol revision (e.g. `2026-03-26`) in `runtime/go/lazuli/mcp/contract.go` constants. Doctor `MCP-PROTOCOL-001` warns on drift. |
| Transport surface explodes | Closed catalog (`stdio`/`http_sse`/`http_streamable`). New transports require a follow-up bucket cell, not ad-hoc growth. |
| Tool-param schema grows beyond `ParamType` vocabulary | Add `JSONSchema` escape-hatch as `params raw "<json>"` only if a real product needs it (Pleiades v2 probably will not). Track as M11 if it surfaces. |
| MCP server multiplexing (multiple servers per app) | Out of scope for L0. The `App` IR carries `Vec<MCPServerSpec>`, but the runtime helper boots one server per call. Multiplex via separate processes / sidecar in v1. |
| Auth surface limited to bearer | Future cells add OAuth + mTLS without changing the L0 shape (`auth oauth ...` / `auth mtls ...`). Closed-catalog growth, not surface revision. |
| Plugin authors abuse `@plugin/<name>` MCP presets to ship vendor-coupled tools into core apps | `docs/plugin-authoring.md` adds the rule: `mcp_server` presets in a plugin MUST be opt-in (not auto-enabled). Doctor `MCP-PLUGIN-OPTIN-001` enforces. |

---

## Out of scope (deferred)

- **MCP server multiplexing** — one `mcp_server` block ↔ one server
  process at runtime. Multi-server-per-process is a follow-up.
- **OAuth + mTLS auth** — bearer-only at L0; extension is purely
  additive.
- **MCP resource subscriptions / live updates** — the protocol
  supports it but Pleiades v2 does not need it; deferred until a
  consumer asks.
- **Two-way agent-as-server** (the app *acts* as the agent calling
  an MCP server hosted by the user) — Lazuli currently models
  the app-side only. Out of scope.
- **`lazuli mcp probe` / `lazuli mcp serve` CLI verbs** — useful for
  authors but additive; track as M11/M12 follow-ups.

---

## Companion docs to update

After this proposal grades-then-fixes through to PASS, the
implementing cells must touch:

- `docs/architecture.md` — add MCP bucket to the runtime
  inventory table.
- `docs/invariants.md` — add the closed-grammar rules for
  `mcp_server` / `mcp_client` sub-blocks; add the
  `MCPTransport` closed enum.
- `docs/design-principles.md` — quote the wire-thin acceptance
  test as an example of the principle applied.
- `docs/plugin-authoring.md` — document the optional
  `mcp_server` preset pattern plugins can ship.
- `runtime/go/lazuli/notifications/contract.go` (or analogous
  file in another bucket): add an example of using `MCPClient`
  from a feature handler, so authors have a real reference.

---

## Grade-then-fix gate

This proposal must reach **≥ 8.5/10 with no individual rubric
dimension below 7** via `lazuli-language-architect`, per the
grade-before-commit discipline (`CLAUDE.md` §"Grade-before-commit
for proposals"). Target ≥ 9.0. Hard blockers:

- **Boundary leak**: any cell that touches `module.rs`, `mod.rs`,
  `types.rs`, or other shared files outside the codegen emitter for
  this bucket. Codex cells in this bucket are single-file output
  only (CLAUDE.md anti-pattern #4).
- **Wire violation**: any file > 100 effective LOC with zero
  external imports in `runtime/go/lazuli/mcp/`. The whole point is
  the SDK wire; reimplementing MCP is the failure mode.
- **Vocabulary drift**: introducing a new `kind` keyword,
  `@-namespace`, or escape-hatch beyond what this proposal
  declares. Closed grammar invariant.
- **Vendor coupling in core**: any reference to a specific paid SaaS
  inside this bucket. MCP is the protocol; vendor adapters
  (`@plugin/<name>`) ship in their own repos.

If any blocker survives v1, the proposal blocks at design time and
cells do not launch.
