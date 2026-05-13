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
