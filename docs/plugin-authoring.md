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
via a static mapping in `lazurite.toml [plugins]` to a concrete Go
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
single `[plugins]` entry in `lazurite.toml`.

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
  ✅ `@plugin/mercadopago` (provider). ❌ `@plugin/hostpoint/mercadopago`.
- `@runtime/<name>` is reserved for OSS commodity infra (postgres,
  redis, S3-protocol, smtp, kafka, etc.) that lives in Lazuli core.
  Plugins NEVER use `@runtime/` prefix.
- Repo path: `github.com/lazuli-lang/lazuli-plugin-<name>` for plugins
  under the canonical org, or `github.com/<your-org>/lazuli-plugin-<name>`
  for proprietary plugins in a different org.

## Adapter binding flow (how the runtime finds it)

1. Lazuli DSL declares `adapter @plugin/<name>` in `registry.lzi`
   under an `integrations.<slot>: <BucketInterface>` entry.
2. App's `lazurite.toml` `[plugins]` block maps the DSL ref to a
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
described in `docs/proposals/design-tokens.md` §4. Lazuli core ships
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
- **Activation**: declared in `lazurite.toml [plugins]` with the module
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
`lazurite.toml [plugins]`.

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
back to the canonical plugin should be a `lazurite.toml` change, not a
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
