# Plugin Authoring

How to write a Lazuli `@plugin/<name>` adapter — the canonical repo
shape, the multi-language reality, and the wire-thin discipline.

**Audience**: anyone scaffolding a new plugin repo (humans or
agents via `plugin-scaffold` pipeline).

## What a plugin is

`@plugin/<name>` is a Lazuli-language reference to an adapter that
implements one or more bucket contracts (e.g.
`maps.Geocoder`, `payments.PaymentGateway`,
`notifications.ChannelDispatcher`). The DSL declares
`adapter @plugin/<name>` in `registry.lzi`; the runtime resolves it
via a static mapping in `Lazurite.toml [plugins]` to a concrete Go
module + (when applicable) TS packages.

Plugins live in **separate (often private) repos** under
`github.com/lazuli-lang/lazuli-plugin-<name>` (or the user's own org
for proprietary adapters). They are never bundled into
`lazuli-lang/lazuli` itself.

## The multi-language reality

Most real-world adapters have **multiple faces** depending on which
side of the provider they speak to:

| Face | Language | What it does |
|---|---|---|
| **Server adapter** | Go | Lazuli runtime ↔ provider's server API (auth, data fetches, webhooks). Loaded into `dist/go/main.go` via anonymous import. |
| **Web client** | TS | Browser ↔ provider's client SDK (rendering widgets, capturing user input). Consumed by `dist/ts-web/`. |
| **Mobile client** | TS | React Native/Expo ↔ provider's mobile SDK. Consumed by `dist/ts-mobile/`. |

Not every plugin needs all three faces. Examples:

| Plugin | Go server | TS web | TS mobile |
|---|---|---|---|
| `@plugin/google-maps` | Geocoding API | Maps JS API widget | `react-native-maps` |
| `@plugin/mercadopago` | Preferences + webhooks | Checkout Pro widget | MercadoPago RN SDK |
| `@plugin/expo-push` | Push HTTP API | — (push is mobile-only) | `expo-notifications` register |
| `@plugin/sendgrid` | Email API | — | — |
| `@plugin/sentry` | Server tracing | Browser SDK | RN SDK |

## Canonical repo layout

Single repo, subdirs by target language. Stripe / Mapbox / MercadoPago
all do this:

```
lazuli-plugin-<name>/
├── README.md                # explain provider, status of each face
├── LICENSE                  # MIT default
├── .gitignore
│
├── go.mod                   # Go server adapter (root of the repo)
├── adapter.go               # main entry: init() + struct + interface impl
├── adapter_test.go          # smoke + interface assertion
│
├── web/                     # TS web client (only when needed)
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       └── index.ts
│
└── mobile/                  # TS mobile client (only when needed)
    ├── package.json
    ├── tsconfig.json
    └── src/
        └── index.ts
```

Why same repo across languages: one source of truth for the contract;
versions bump together; codegen resolves all three faces from a
single `[plugins]` entry in `Lazurite.toml`.

## Go server adapter pattern

The Go side is the minimum every plugin must ship — runtime imports
it via anonymous import in `dist/go/main.go` and the adapter
self-registers in `init()`.

Canonical shape (~30-80 LOC depending on bucket contract surface):

```go
// Package <name> is the Lazuli @plugin/<name> adapter.
// Wraps <upstream-go-lib> for <provider's feature>.
package <name>

import (
    "context"
    
    "<upstream/go-lib>"          // the real provider SDK
    
    "lazuli.dev/runtime/lazuli"
    "lazuli.dev/runtime/lazuli/<bucket>"   // bucket contract (maps, payments, ...)
)

const AdapterRef = "@plugin/<name>"

type Adapter struct {
    client *upstream.Client
    err    error // set if config invalid at init
}

// Compile-time assertion that Adapter implements the bucket interface.
var _ <bucket>.<Interface> = (*Adapter)(nil)

func init() {
    lazuli.RegisterAdapter(AdapterRef, newAdapter())
}

func newAdapter() *Adapter {
    // Read config from env, construct upstream client, return Adapter.
}

// Implement each method of the bucket interface — wire to upstream SDK.
func (a *Adapter) Method(ctx context.Context, req <bucket>.Request) (<bucket>.Response, error) {
    if a.err != nil { return <bucket>.Response{}, a.err }
    out, err := a.client.UpstreamCall(ctx, translateRequest(req))
    if err != nil { return <bucket>.Response{}, err }
    return translateResponse(out), nil
}
```

### Wire-thin discipline

Every Go adapter file MUST be ~30-80 LOC of `import + call`. If you
find yourself writing > 100 LOC, you're reimplementing functionality
the upstream library already provides — STOP and ask:

- Is there a better upstream library?
- Is my bucket contract too granular (multiple interface methods that
  should be one)?
- Am I sneaking in business logic that belongs in the user's code?

The 2026-05-12 incident shipped 60+ adapter files with 200-1000 LOC
of stdlib-only reimplementations. All reverted. **Do not repeat**;
this is the founding principle (see `CLAUDE.md §The founding
principle`).

### Error surfaces

Adapter errors propagate up the call stack. The Lazuli runtime wraps
them at the codegen-emitted handler boundary into typed
`*lazuli.AdapterError` envelopes (per D6 in the AI Debug Loop bucket).
Plugin code returns bare errors; codegen does the wrap.

Sentinel errors for known failure modes are encouraged — e.g.
`var ErrUnconfigured = errors.New("...")`. Surface them with
`errors.Is`-compatible chains. Don't construct typed lazuli errors
yourself (codegen wraps; see `CODEGEN-WRAP-001` doctor check).

## TS client patterns (web + mobile)

When the provider exposes a browser or mobile SDK that consumers
need, add `web/` or `mobile/` subdirs. Each is an independently-versioned
npm package, published privately to your org's package registry
(GitHub Packages, npm scoped, or local file path during dev).

### Web client (`web/`)

```
web/
├── package.json          # name: @lazuli-lang/lazuli-plugin-<name>-web
├── tsconfig.json
└── src/
    ├── index.ts          # exports the public API
    └── components/       # React components (if widget-based)
```

`package.json` peer-depends on `react` + (when relevant) `@tanstack/react-query`.
Mirror the Lazuli runtime's pattern: wire-thin wrappers around the
upstream JS SDK. NEVER reimplement provider logic.

### Mobile client (`mobile/`)

```
mobile/
├── package.json          # name: @lazuli-lang/lazuli-plugin-<name>-mobile
├── tsconfig.json
└── src/
    ├── index.ts
    └── hooks/            # React Native hooks (e.g. useMapView, useNotificationToken)
```

`package.json` peer-depends on `expo` (or specific Expo modules like
`expo-notifications`). Wire-thin wrappers around Expo SDK / React
Native packages.

### When TS faces ship

Don't pre-create empty `web/` or `mobile/` dirs. Each face ships when
a downstream product needs it:

- Phase 1 (Auth) port: no TS plugins. Server-only.
- Phase 2 (Data + Maps): add `mobile/` to `@plugin/google-maps` for
  `react-native-maps` integration.
- Phase 3 (Chat + Push): add `mobile/` to `@plugin/expo-push` for
  `expo-notifications` token registration.
- Phase 4 (Payment): add `web/` or `mobile/` to `@plugin/<payment>`
  for checkout widget (or skip if server-redirect flow).

## Namespace policy

Strict rules (also in `CLAUDE.md §Namespace policy` + memory
`project_plugin_namespace_policy`):

- Plugin name = **provider name**, not consumer-product name.
  ✅ `@plugin/mercadopago` (provider). ❌ `@plugin/myapp/mercadopago`.
- `@runtime/<name>` is reserved for OSS commodity infra (postgres,
  redis, S3-protocol, smtp, kafka, etc.) that lives in Lazuli core.
  Plugins NEVER use `@runtime/` prefix.
- Repo path: `github.com/lazuli-lang/lazuli-plugin-<name>` for plugins
  under the canonical org, or `github.com/<your-org>/lazuli-plugin-<name>`
  for proprietary plugins in a different org.

## Adapter binding flow (how the runtime finds it)

1. Lazuli DSL declares `adapter @plugin/<name>` in `registry.lzi`
   under an `integrations.<slot>: <BucketInterface>` entry.
2. App's `Lazurite.toml` `[plugins]` block maps the DSL ref to a
   Go module path: `"@plugin/<name>" = { module = "github.com/lazuli-lang/lazuli-plugin-<name>", version = "v0.1.0" }`.
3. Codegen emits `dist/go/main.go` with `_ "github.com/lazuli-lang/lazuli-plugin-<name>"`
   anonymous-import + a `requires integration <slot>: <BucketInterface>`
   wire in the consuming feature.
4. At process boot, the anonymous import triggers the plugin's
   `init()`, which calls `lazuli.RegisterAdapter("@plugin/<name>", &Adapter{})`.
5. The runtime's `lazuli.LookupAdapter("@plugin/<name>")` returns
   the registered instance when generated code needs it.

For TS faces (web/mobile), the consumer app `package.json` adds the
plugin's `web/` or `mobile/` package as a direct dep; generated
`dist/ts-<frontend>/` imports from it as needed.

## Plugin registry resolution order

Codified 2026-05-17 per architect re-grade observation #1 across W1.3
(`datasource-plugin-contract`), W2.A (`auth-flow-blocks-cycle`), and
W2.B (`payments-plugin-contract`). All three proposals assume — but
none state — that the `@plugin/<kind>` registry is populated before
any compile-time validator or runtime resolver consults it. This
section makes the assumption explicit.

The rule:

1. **Plugin registration completes during workspace boot, before any
   compile-time validator runs.** The anonymous-import chain in
   `dist/go/main.go` triggers every plugin's `init()` (which calls
   `lazuli.RegisterAdapter`) before the runtime hands control to
   `lazuli.Mux()`. Concretely: a plugin's `init()` MUST be self-
   contained — no network calls, no DB reads, no env-var fallbacks
   that block. Registration is a pure-Go assignment into a map.

2. **Compile-time validators see the same registry as runtime
   resolvers.** `lazuli doctor` and `lazuli build` walk the same
   `@plugin/<kind>` set the running process will consult. Codegen
   diagnostics like `DATASOURCE-KIND-UNKNOWN` /
   `PAYMENTS-KIND-UNKNOWN` (per W1.3 + W2.B respectively, both
   proposal-pending) fire from this shared view. A plugin missing
   from `Lazurite.toml [plugins]` is invisible to both.

3. **Circular plugin dependencies are static rejects.** A plugin
   declaring `extends = "@plugin/other-plugin"` (per W1.3 §3.4 single-
   parent `extends`) is walked at registration time; cycles
   (`A extends B extends A`) are rejected by the manifest validator
   BEFORE Go's `init()` chain runs. The runtime never sees an
   unresolved chain.

4. **Registration order within a single boot is unspecified BUT
   deterministic across boots.** Go's `init()` ordering follows
   import-dependency topology — Lazuli does not impose a separate
   order. Plugins MUST NOT depend on another plugin being registered
   before their own `init()` returns. If a plugin needs to consult
   another at runtime, it does so lazily (first use), not at boot.

5. **The consuming app's view is monotonic.** Once `lazuli.Mux()`
   returns, the `@plugin/*` registry is frozen for the process
   lifetime. There is no `RegisterAdapter` after-boot path. A plugin
   that needs to swap implementations at runtime (e.g., circuit-break
   to a fallback) does so internally; the framework-visible registry
   slot stays bound to one adapter instance.

These five points are the contract between the framework, plugin
authors, and consuming apps. Any proposal that requires a different
boot-time guarantee (e.g., "register-after-DB-ready") needs to amend
this section first.

## Resolution guarantee (semantic-type plugins)

Codified 2026-06-01 (change 0019). A plugin's `@semantic.<Name>`
contributions are wired into the compiler's IR by exactly **one
resolution stage** — `resolve_module_plugins` in
`crates/lazuli_cli/src/module_loader/plugin_resolution.rs`. Every
module-load path funnels through it. The historical bug this section
prevents: the resolver lived inline in only one of two near-duplicate
loaders, so `lazuli generate go` (the source-map path) silently never
resolved plugin semantics and a correctly-declared, correctly-built
plugin surfaced as an opaque downstream codegen error
(`CODEGEN-GO-SEMANTIC-004: ... outside the closed Go semantic table`).

The guarantee, in three rules:

1. **One pipeline, every load path.** `build_module_from_path` and
   `build_module_with_source_from_path` both call
   `resolve_module_plugins(&mut module, input)` just before returning
   their module. There is no second copy of the resolution logic to
   drift. A `both_loaders_resolve_plugin_semantics` regression test
   asserts both loaders, on the same plugin-using input, resolve every
   `@semantic.*` ref to a typed `SemanticPluginType` (zero residual
   `UserDefined`). **A third loader, if ever added, MUST call this
   stage** — it is the single public resolution entry point.

2. **Project root is found by walking UP.** Resolution ascends from the
   input directory to the nearest `Lazurite.toml`. So
   `lazuli generate go app` (features under `app/`, manifest at the repo
   root via `app_dir = "app"`) resolves plugins declared in the
   repo-root `[plugins]` block — it does not mistake `app/` for the
   project root and silently no-op on an empty alias map.

3. **Declared-but-unwired = a loud, anchored error at the boundary.**
   When a non-empty `[plugins]` block is declared, any failure to wire
   it is a hard error at the resolution boundary, naming the cause and
   the fix — never a silent no-op that resurfaces 200 lines later as a
   codegen symptom on a field you didn't write:
   - manifest read/parse error, namespace mismatch, unsupported carrier
     (`carrier_type` outside the closed `String` catalog), and
     alias conflicts propagate from `build_alias_map` loudly (they were
     previously swallowed by an `if let Ok`);
   - a referenced `@semantic.<X>` that no declared plugin provides bails
     with `plugin semantic '@semantic.<X>' is referenced but no declared
     plugin provides it — declare the contributing plugin in
     Lazurite.toml [plugins], or check its manifest.toml
     [[semantic_types]] (declared plugins: <list>)`.

   The single legitimate **silent** case is preserved: single-file
   `lazuli check <one.lzi>` with no project root above it. There,
   `find_project_root` returns `None`, the stage is a no-op, and
   `@semantic.*` stays unresolved for the doctor to anchor
   `SEMANTIC-PLUGIN-001` at the field site. Apps with zero plugins, or
   all-resolving plugins, see no new errors.

## Doctor and codegen agree

Codified 2026-06-01 (change 0020). **If `lazuli doctor` is green on your
plugin's semantic types, `lazuli generate go` resolves those same types.**
Passing doctor is a true precondition for generate — you will never fix
every doctor finding and then hit a `generate`-time
`CODEGEN-GO-SEMANTIC-004` for a `@semantic.<X>` doctor said was fine.

The historical bug this prevents: doctor built its OWN plugin alias map.
It called the same `build_alias_map` as codegen, but fed it a **different
project root**. Run from a features subdir (`lazuli doctor app`, manifest
one level up via `app_dir = "app"`), doctor's pass-through root pointed at
`app/`, found no `Lazurite.toml` there, and either went silent on plugins
or false-flagged them — while `lazuli generate go app` walked UP, found
the repo-root manifest, and resolved. The two surfaces could resolve the
same project differently: **doctor-green ≠ codegen-green.**

The fix is structural — there is now **one resolver root, shared**:

1. **One upward project-root walk, re-homed.** `find_project_root` (the
   ascend-to-nearest-`Lazurite.toml` walk) lives in `lazuli_manifest`
   (`lazuli_manifest::lazurite_manifest::find_project_root`), the crate
   both the CLI codegen path and the doctor engine depend on. 0019's
   `lazuli_cli` `find_project_root` is a re-export of it. Doctor's
   plugin-semantic checks call the **identical** function. One walk, two
   surfaces — they cannot drift on the root.

2. **One alias map, same inputs.** Doctor resolves through
   `authoritative_alias_map` (in
   `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/plugin_resolution_view.rs`):
   it walks UP to the manifest root, loads the manifest from **that**
   root, and calls the SAME `build_alias_map(manifest, &root)` codegen
   feeds. Same function + same root + same manifest ⇒ byte-identical map.
   Both the `SEMANTIC-PLUGIN-001` check and the legacy
   `semantic_type_unknown` suppression consume this one map, so a
   plugin-provided alias (e.g. `@semantic.BrazilianCEP`) is resolved on
   **every** doctor surface, not just one.

3. **The doctor finding mirrors the generate bail.**
   `SEMANTIC-PLUGIN-001` now fires exactly when the same `@semantic.<X>`
   would leave a residual on the generate path — `[plugins]` present and
   no resolving alias on the authoritative root. So doctor flags an
   unresolved alias at the cheapest gate (the field site), and resolves
   one generate would resolve.

A `plugin_semantic_doctor_and_generate_agree` drift-guard test asserts the
invariant mechanically: on the same plugin-using fixture run from the same
subdir, the doctor's unresolved-`@semantic.*` set **equals** the generate
path's residual set. They are green together or flag the identical alias —
never one without the other. Any future plugin kind (adapter, capability)
inherits this guarantee by resolving through the same shared walk.

## Manifest is typed per kind

Codified 2026-06-01 (change 0021). A plugin's `manifest.toml` is the
contract between an external adapter and the compiler. The schema is
**kind-discriminated** — each plugin is one of four `kind`s, and the
compiler reads the typed contract for that kind instead of dropping it as
opaque prose. The schema lives in
`crates/lazuli_manifest/src/plugin_manifest/types.rs` (`PluginManifest` +
`PluginKind`).

### The four kinds

| `kind`       | What it contributes                              | Modeled in v1 |
|--------------|--------------------------------------------------|---------------|
| `semantic`   | `[[semantic_types]]` scalars/validators (0019)   | **Fully** (unchanged) |
| `adapter`    | `implements` + `[env]` + `[binds]` Go-interface contract | **Fully** |
| `capability` | reserved future kind                             | Thin stub (full schema deferred) |
| `design`     | reserved future kind                             | Thin stub (full schema deferred) |

### `kind` is inferred, not required

No existing manifest declares a top-level `kind` key, so it is **never
mandatory**. The compiler derives it via `PluginManifest::resolved_kind()`
using this precedence ladder:

1. An explicit top-level `kind = "..."` wins (the override).
2. Else if `[[semantic_types]]` is non-empty → `semantic`. *(This preserves
   the 0019 path: any manifest the semantic resolver cares about keeps
   classifying as semantic, even if it also carries adapter sections.)*
3. Else if any adapter section is present (`implements` non-empty, or
   `[env]`, or `[binds]`) → `adapter`.
4. Else → `semantic` (the historical default for an identity-only manifest;
   harmless — it contributes no aliases).

`capability`/`design` are **never inferred** in v1 — they only arise from an
explicit `kind`. The adapter-contract fields are parsed and readable
regardless of inferred kind; `kind` only selects which verify/scaffold path
(0022/0023) a manifest takes.

> NOTE: `smtp` carries `kind = "notifications/email-sender"` **inside
> `[plugin]`**. That is a free-form *catalog* string on `PluginIdentity`,
> distinct from the `PluginKind` enum, and it does **not** feed inference.

### The blessed adapter schema

An adapter declares its framework contract with three keys (grounded in the
real mercadopago / smtp / object-store manifests):

```toml
# top-level: the framework contract bucket(s) this adapter satisfies.
# 0022 verifies each against a real Go interface.
implements = ["payments.PaymentGateway"]

[plugin]
name = "mercadopago"
namespace = "@lazuli/plugin-mercadopago"
go_module = "github.com/lazuli-lang/lazuli-plugin-mercadopago"

# the environment-variable contract; doctor surfaces it, 0023 seeds it.
[env]
required = ["MERCADOPAGO_ACCESS_TOKEN", "MERCADOPAGO_WEBHOOK_SECRET"]
optional = []
# smtp-style conditional tier (required only when auth is configured):
required_for_auth = ["SMTP_USERNAME", "SMTP_PASSWORD"]

# the Go interface this adapter binds against + its exported methods.
# 0022 resolves `interface` to a real Go interface and checks `methods`.
[binds]
interface = "github.com/lazuli-lang/lazuli-plugin-smtp.EmailSender"
methods = ["SendEmail", "SendEmailBatch"]
```

Tolerated legacy spellings (not the blessed shape, but parsed without
error): `[plugin].module` is read as a fallback for `go_module` (via
`effective_go_module()`); per-variable `[env.<VAR>]` detail sub-tables
(`description`/`allowed`/`default`) are catalog metadata and are dropped on
parse, not modeled. The legacy `[contract].methods` / `[provides].go_interface`
spellings (on social-apple / social-google) are **not** auto-mapped to
`[binds]` — those plugins migrate to `[binds]` when the scaffolder (0023)
lands.

### Capability / design — deferred

`[capability]` and `[design]` are reserved so the discriminant is exhaustive
and round-trips, but v1 models only a thin marker (`provides` / `emits`
free-form name lists). The full per-kind contract is **DEFERRED** to a later
spec.

### Back-compat guarantee

Every field added by 0021 is `#[serde(default)]`, so all 24 real plugin
manifests keep deserialising unchanged and the `[[semantic_types]]` struct
is byte-for-byte the same. A vendored-fixture regression test
(`crates/lazuli_manifest/tests/plugin_manifest_typed.rs`) deserialises every
real manifest and asserts the inferred kind, the round-tripped adapter
fields, and structural rejection of a malformed adapter.

## Scaffolding (automated)

The `plugin-scaffold` pipeline in `lazuli-lang/ops/.pipely/pipelines/plugin-scaffold/`
automates this — it:

1. Validates the proposed name follows the namespace policy.
2. Creates the GitHub repo (private by default).
3. Generates the canonical Go server adapter skeleton.
4. Dispatches a Codex worker to fill the wire against a named
   upstream Go SDK (~30-80 LOC).
5. Pushes the initial commit.

TS web/mobile faces are added later via separate pipeline runs when
the product port reaches that phase.

## See also

- `docs/architecture.md` §Lazuli vs Lazurite + §Three internal layers
- `docs/release-policy.md` §Stability tiers (plugins start EXPERIMENTAL)
- `CLAUDE.md` / `AGENTS.md` §Namespace policy + §The founding principle
- `lazuli-lang/ops/.pipely/pipelines/plugin-scaffold/pipeline.toml`
- Memory `project_plugin_namespace_policy.md`

## Design emitter plugins (`@plugin/design-<target>`)

Design emitter plugins extend the `design.lzi` token pipeline introduced
by L0 #2 (Design Tokens, commit `20d8413`) beyond the universal emitters
described in the `design-tokens` proposal (operational archive) §4. Lazuli core ships
five emitters because every product needs at least one of them: web
`tokens.ts`, `tokens.css`, `tailwind.gen.ts`, `tailwind.theme.css`, and
mobile `tokens.ts`. Anything beyond that — Figma sync, Style Dictionary,
Panda, vanilla-extract, Tamagui, Restyle — is opinionated tool
integration, so it belongs in plugin space per the namespace policy.

### Plugin contract

Design emitter plugins follow the same `@plugin/<name>` discipline as
provider adapters, with the target encoded after `design-`:

- **Plugin repo name**: `github.com/lazuli-lang/lazuli-plugin-design-<target>`
  for canonical plugins, or `github.com/<your-org>/lazuli-plugin-design-<target>`
  for private plugins.
- **Plugin namespace**: `@plugin/design-<target>`, where `<target>` is
  kebab-case. Examples: `figma`, `style-dictionary`, `panda`,
  `vanilla-extract`, `tamagui`, `restyle`.
- **Activation**: declared in `Lazurite.toml [plugins]` with the module
  path and version.
- **Input**: the lowered `Design` IR slice, represented in the
  compiler as the typed Rust struct from `lazuli_ir::Design`. Treat
  this as opaque for now: Lazuli passes the parsed token catalog to the
  plugin. The precise schema and ABI land in the L2 implementation
  cell.
- **Output**: one or more generated files under
  `dist/ts-web/design/<plugin-target>/` for web output and/or
  `dist/ts-mobile/design/<plugin-target>/` for mobile output. The
  plugin chooses filenames and content inside that directory.

`<plugin-target>` is the namespace suffix without `@plugin/design-`.
Examples:

| Namespace | Plugin target | Web output root | Mobile output root |
|---|---|---|---|
| `@plugin/design-figma` | `figma` | `dist/ts-web/design/figma/` | — |
| `@plugin/design-style-dictionary` | `style-dictionary` | `dist/ts-web/design/style-dictionary/` | — |
| `@plugin/design-panda` | `panda` | `dist/ts-web/design/panda/` | — |
| `@plugin/design-vanilla-extract` | `vanilla-extract` | `dist/ts-web/design/vanilla-extract/` | — |
| `@plugin/design-tamagui` | `tamagui` | — | `dist/ts-mobile/design/tamagui/` |
| `@plugin/design-restyle` | `restyle` | — | `dist/ts-mobile/design/restyle/` |

The output root is intentionally nested one level deeper than core
emission. Core owns:

```
dist/ts-web/design/
├── tokens.ts
├── tokens.css
├── tailwind.gen.ts
├── tailwind.theme.css
└── allowlist.json

dist/ts-mobile/design/
└── tokens.ts
```

Plugins own only:

```
dist/ts-web/design/<plugin-target>/
dist/ts-mobile/design/<plugin-target>/
```

Design emitter plugins are sandboxed by convention and by the future
L2 contract. A plugin MUST NOT touch any file outside its own
`dist/ts-web/design/<plugin-target>/` or
`dist/ts-mobile/design/<plugin-target>/` subtree. A plugin MUST NOT
modify core-emitted files such as `tokens.ts`, `tokens.css`,
`tailwind.gen.ts`, or `tailwind.theme.css`.

The plugin may emit any file format required by the downstream tool:
`.json`, `.ts`, `.css.ts`, `.css`, or supporting manifest files. Those
files are generated output. Product code may import them, but product
authors should not hand-edit them.

The plugin input is already lowered. Plugin authors do not parse
`design.lzi`, do not re-run validation, and do not infer token meaning
from source text. Core parsing/lowering owns:

- grammar validity;
- token group closure;
- color state closure;
- unit normalization where required;
- dark-mode value pairing;
- diagnostics for invalid token shapes.

Plugins translate from Lazuli's canonical token IR to an ecosystem
format. If a target format cannot represent a Lazuli token exactly, the
plugin should emit the closest faithful representation and report a
diagnostic. It should not mutate the source catalog to make the target
tool easier to satisfy.

Activation order is not a dependency mechanism. Each design plugin is
given the same lowered `Design` IR slice and writes its own output
subtree. A plugin MUST NOT read another design plugin's output as its
input. If two plugins need a shared helper representation, that helper
belongs either in Lazuli core IR or in a normal library dependency used
inside both plugin repos.

### Reserved canonical plugins

L0 #2 §4.7 reserves the following plugin names. Third-party
implementations for the same target MUST use the reserved name, not a
variant such as `@plugin/panda-design` or `@plugin/my-panda`. This
keeps discovery predictable for humans and LLM agents, and prevents two
competing plugin names from claiming the same ecosystem target.

| Plugin | Output target | Reserved name |
|---|---|---|
| Figma Tokens Studio | W3C Design Tokens JSON, Figma round-trip | `@plugin/design-figma` |
| Style Dictionary (Amazon) | Style Dictionary source-format JSON | `@plugin/design-style-dictionary` |
| Panda CSS | Panda config tokens slice (`panda.config.ts`) | `@plugin/design-panda` |
| vanilla-extract | `themeContract.css.ts` | `@plugin/design-vanilla-extract` |
| Tamagui (RN) | Tamagui tokens config | `@plugin/design-tamagui` |
| Shopify Restyle (RN) | Restyle theme | `@plugin/design-restyle` |

Reserved does not mean implemented in Lazuli core. It means the name is
held for the ecosystem target. The implementation still lives in its
own plugin repo and is activated only when the product lists it in
`Lazurite.toml [plugins]`.

Reserved also does not make a provider/vendor part of the runtime. The
namespace stays `@plugin/design-<target>` because these targets are
specific tools or frameworks, not commodity Lazuli runtime buckets.

If a private product needs a fork of one of the reserved targets, keep
the Lazuli namespace stable and point it at the private module:

```toml
[plugins]
"@plugin/design-panda" = { module = "github.com/acme/lazuli-plugin-design-panda", version = "v0.1.0-acme.1" }
```

This keeps authored Lazuli source portable. Moving from a private fork
back to the canonical plugin should be a `Lazurite.toml` change, not a
DSL rewrite.

### Activation example

```toml
[plugins]
"@plugin/design-figma" = { module = "github.com/lazuli-lang/lazuli-plugin-design-figma", version = "v0.1.0" }
"@plugin/design-panda" = { module = "github.com/lazuli-lang/lazuli-plugin-design-panda", version = "v0.1.0" }
```

A product can declare multiple design plugins. Each runs independently.
Plugin output is opt-in: removing an entry from `[plugins]` skips that
plugin without breaking the build, as long as product code does not
import files that only the plugin emits.

Activation does not replace core emitters. If a product enables
`@plugin/design-panda`, Lazuli still emits the core `tokens.ts` and
`tokens.css` files. The Panda plugin adds its target-specific output
under `dist/ts-web/design/panda/`.

Activation also does not imply frontend presence. A web-only product
may activate Figma and Panda. A mobile-only product may activate
Tamagui or Restyle. A multi-frontend product may activate both web and
mobile design plugins from the same `[plugins]` block.

If a plugin targets a frontend that is not enabled, codegen should skip
that face or report a clear diagnostic, depending on the L2 contract.
The user-facing rule is simple: plugins may emit only into the dist
roots that exist for the product.

Expected generated tree for the example above:

```
dist/ts-web/design/
├── tokens.ts
├── tokens.css
├── tailwind.theme.css
├── figma/
│   └── tokens.json
└── panda/
    └── panda.config.ts
```

Exact filenames inside `figma/` and `panda/` are plugin-owned. The
subdirectory boundary is not.

### Round-trip: Figma

`@plugin/design-figma` is bidirectional because Figma Tokens Studio can
act as an external token catalog. The plugin must support these flows:

| Command | Plugin path | Output |
|---|---|---|
| `lazuli design export --target figma` | export | W3C Design Tokens JSON consumable by Figma Tokens Studio |
| `lazuli design import --from <figma.json>` | import | lifted `design.lzi` source |
| `lazuli design diff --against <figma.json>` | diff | drift report between `design.lzi` and the external catalog |

Other design plugins are export-only unless a future proposal says
otherwise. Panda, Tamagui, Restyle, vanilla-extract, and similar
consumer targets emit files for downstream tools, but they do not
import an external source of truth because the source of truth is
`design.lzi`.

Figma is the exception because teams may edit token values in Figma
Tokens Studio and need an explicit reconciliation path. The path is
still not a silent sync:

1. Export writes Figma-consumable JSON from `design.lzi`.
2. Import lifts a Figma JSON catalog into Lazuli source.
3. Diff reports drift before a user accepts the source change.

Round-trip support is target-specific. The Figma plugin may have both
export and import entrypoints. The Panda plugin should not invent an
import path from `panda.config.ts` back into `design.lzi`, because
Panda config is generated consumer output, not the product's design
source.

For import, the plugin output is source text, not an in-place mutation.
The CLI may write a new `design.lzi`, update the existing one after
confirmation, or print a patch. In all cases, the user sees the diff.

For diff, the plugin should compare semantic token paths and values,
not generated file formatting. Reordered JSON keys, whitespace, and
target-specific metadata should not count as drift unless they change a
token that Lazuli understands.

### Non-goals

Design plugins are emitters, not language extensions:

- Plugins MUST NOT add new token groups. The eight-group catalog
  (`color`, `typography`, `space`, `radius`, `shadow`, `motion`,
  `breakpoint`, `z`) is closed. Adding `gradient` or `animation` is a
  Lazuli core proposal, not a plugin choice.
- Plugins MUST NOT add new sub-groups within a group. Closed sub-groups
  such as typography `family` / `scale` / `weight` / `tracking` and
  motion `duration` / `easing` stay closed.
- Plugins MUST NOT extend the color state catalog. `base`, `hover`,
  `active`, and `foreground` are the only color states in v0.
- Plugins MUST NOT silently mutate `design.lzi`. Import flow may write
  a new `design.lzi` or update the existing one, but the user must see
  the diff before accepting the change.

Plugins also do not own Doctor policy. Doctor may learn to inspect
plugin outputs later, but the plugin contract itself does not get to
define new lint rules, new severity levels, or new accepted token
syntax. Those belong in Lazuli core.

Plugins do not own frontend folder topology. They cannot move design
output to `app/design/`, `.lazuli/`, `generated/`, or a tool-preferred
root. Generated output stays under `dist/ts-web/design/<plugin-target>/`
or `dist/ts-mobile/design/<plugin-target>/`.

Plugins do not own package-manager wiring by side effect. If a target
needs npm dependencies, the L2 contract may allow the plugin to declare
dependencies for generated package manifests, but the plugin should not
edit arbitrary product files to install them.

Plugins do not bypass the namespace policy. A named design tool or
framework still lives under `@plugin/design-<target>`, never under
`@runtime/`, and never under a product-scoped namespace such as
`@plugin/<consumer-product>/figma`.

Plugins do not make generated output authoritative. The authoritative
catalog is `design.lzi`; generated files can always be deleted and
recreated.

### Forward reference

The precise L2 plugin contract — Rust trait, FFI shape, and plugin
discovery mechanism — is defined in the L0 #2 Cell E implementation.
This section establishes the stable user-facing surface.
