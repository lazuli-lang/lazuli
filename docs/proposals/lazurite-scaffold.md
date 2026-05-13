# Proposal — Lazurite: the opinionated Lazuli distribution

**Status:** Draft v0.3 — 2026-05-13 (Nuxt-analogy rename + frontend topology + §3.3 Lazuli→Lazurite primitive map; v0.2 was 8.68/10 PASS, v0.1 was 7.6/10 BLOCK; v0.3 graded 9.19/10 PASS pre-§3.3, §3.3 added post-grade as boundary clarification per user query)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Supersedes / amends:** `docs/project-structure.md:101` (older `lazuli.toml` hand-wave deprecated — manifest is now `lazurite.toml`), `docs/architecture.md` (§Lazuli vs Lazurite)
**Honors:** `docs/invariants.md:14-15` (app.lzi owns environments+urls; NOT the manifest)

---

## §1. Status & motivation

Lazuli is the framework (language + IR + compiler + Go runtime lib + CLI). Lazurite is **the opinionated distribution on top of Lazuli** — folder conventions, project manifest schema, scaffold templates, default plugins. Same relationship as Vue→Nuxt or React→Next.

Today the gap is concrete:

1. `lazuli new <project>` scaffolds **5 files** (app.lzi, registry.lzi, README, .gitignore, features/.gitkeep). A real app needs more shape than that to be productive.
2. `docs/project-structure.md` describes a rich layout (features/{ui,hooks,domain,queries,jobs,integrations,pages}, `.lazuli/generated/`, contracts/, profiles.lzi, workspace.lzi) but **no code enforces it** — fixtures (`auth-roundtrip`, `smoke-hello`) follow simpler shapes, and the canonical fixture `full-capsule` is flat. The convention is hypothetical.
3. There is **no project manifest in code**: no place to declare framework version pins, plugin module resolution, codegen output paths, frontend topology, distro template lineage. This proposal names the manifest **`lazurite.toml`** (distro-named per the Nuxt→Vue analogy this proposal itself draws — Nuxt has `nuxt.config.ts`, not `vue.config.ts`). `docs/project-structure.md:101` references an older `lazuli.toml` hand-wave; L8 updates that doc to the new name. The manifest **does NOT duplicate** what `app.lzi` already owns (environments, URLs, deploy gates per `docs/invariants.md:14-15`).
4. Plugins (`@plugin/<name>`) have a registration API in Go (`plugin_registry.go`) but **no declarative source-of-truth** for which plugins an app uses. Today this lives implicitly in `go.mod` deps.
5. **Generation output path is inconsistent**: `docs/project-structure.md` says `.lazuli/generated/`; current codegen + tests + `lazuli generate go --out` default to `dist/go/`. Drift.

Lazurite resolves all five gaps by being the **rails** Lazuli was missing — opinionated shapes that turn the framework primitives into a productive starter, with the project manifest (`lazurite.toml`) carrying *only* what `.lzi` doesn't already own.

**Why now:** Hostpoint port can't start until the rails exist. Phase Prep §1.1/§1.2/§1.3 closed; the bottleneck is no longer "can Lazuli codegen?" but "what shape does a Lazuli app take?". Lazurite is that answer.

**Boundary discipline (the §5 determinism guard):** Per `docs/invariants.md:14-15`, `app.lzi` owns environments, URLs, deploy gates. Per this proposal, `lazurite.toml` owns the framework version pin, plugin module resolution, codegen settings, migration runner policy, distro template lineage. **There is no overlap.** Doctor enforces. See §13.6 for the explicit rejection of `[env.*]` in the manifest.

---

## §2. Scope split — what's Lazuli, what's Lazurite

The split mirrors Vue→Nuxt: framework owns primitives; distro owns conventions, project shape, and "the way you actually build apps".

| Concern | Owner | Why |
|---|---|---|
| `.lzi` / `.lzx` grammar, IR, parser, analyzer | **Lazuli** | Closed language; never specialized by distro |
| `lazuli` CLI verbs (parse, doctor, generate, inspect, plan, dev, new) | **Lazuli** | Tooling on top of language; distro can't replace |
| Go runtime (`lazuli.dev/runtime/lazuli/...`) | **Lazuli** | All distros share runtime; no specialization point |
| Adapter contracts (`@runtime/<name>`, `@plugin/<name>`) | **Lazuli** | Closed catalog of namespaces |
| Project folder conventions | **Lazurite** | Opinionated shape; Vue/React don't enforce a folder layout, Nuxt/Next do |
| `lazurite.toml` workspace manifest | **Lazurite** | Distro-specific contract |
| `lazuli new` template content | **Lazurite** | Lazuli ships the **verb**; Lazurite ships the **template body** |
| Default plugins selected for a new app | **Lazurite** | Opinionated choice; another distro may pick differently |
| Migration runner policy (where files live, naming convention) | **Lazurite** | Convention on top of Lazuli `migrations` primitives |
| Email / notification template directory layout | **Lazurite** | Convention; runtime accepts a directory, distro picks the directory |
| CI/CD templates | **Lazurite** | Opinionated; future cut |
| Deploy targets / cloud config | **Lazurite (or @plugin)** | Out of Lazuli framework scope (memory: `runtime/lazuli/deploy/` was removed) |

**Boundary rule** (locks the split):

> Lazuli ships **primitives that can be combined any way**. Lazurite ships **one canonical combination**, and the **verbs to instantiate it** (`lazuli new`, `lazuli dev`). Other distros (future) ship different canonical combinations against the same Lazuli framework.

---

## §3. The Lazurite App Shape

The canonical layout a `lazuli new myapp` produces. **Source files are committed; `dist/` and `.lazuli/` are gitignored.**

```
myapp/
├── lazurite.toml                    # workspace manifest (see §4)
├── app.lzi                          # canonical app declaration
├── registry.lzi                     # external integrations + plugin bindings
├── profiles.lzi                     # optional: env-specific overlays (dev/staging/prod)
│
├── features/                        # one dir per feature
│   ├── account/
│   │   ├── account.lzi              # feature DSL surface
│   │   ├── handlers/                # Go extension code (validators/fns/hooks)
│   │   │   └── verify_password.go
│   │   ├── migrations/              # additional manual SQL (rare; codegen emits to dist/)
│   │   │   └── 20260513_001_add_index.sql
│   │   ├── templates/               # email/notification templates per locale
│   │   │   ├── password_reset.en-US.tmpl
│   │   │   └── password_reset.pt-BR.tmpl
│   │   └── queries/                 # custom SQL files (referenced by query.sql)
│   │       └── active_users.sql
│   └── property/
│       └── property.lzi
│
├── contracts/                       # external service contracts (optional)
│   └── stripe.v1.lzi
│
├── i18n/                            # global translation catalogs (per-feature in features/<x>/i18n/)
│   ├── common.en-US.json
│   └── common.pt-BR.json
│
├── plugins/                         # local plugin sources (rare; usually deps in go.mod)
│
├── scripts/                         # custom scripts (CI, deploy, db reset, etc.)
│   └── seed.sh
│
├── .lazuli/                         # internal cache + manifests (gitignored)
│   ├── graph.json                   # IR graph snapshot for incremental
│   ├── source-map.json              # IR position → generated line map
│   └── manifest.json                # extension file registry
│
├── dist/                            # generated output (gitignored)
│   ├── go/                          # `lazuli generate go --out dist/go`
│   │   ├── main.go
│   │   ├── account/
│   │   │   ├── resource.gen.go
│   │   │   ├── command.gen.go
│   │   │   ├── query.gen.go
│   │   │   ├── api.gen.go
│   │   │   ├── auth.gen.go
│   │   │   └── migrations/
│   │   │       └── 20260513_account.up.sql
│   │   └── go.mod
│   └── web/                         # `lazuli generate ts --out dist/web` (future)
│
├── go.mod                           # `module myapp` + require lazuli.dev/runtime/lazuli
├── go.sum
├── .gitignore
└── README.md
```

### §3.1 Feature dir conventions

Each `features/<feature>/` contains:

- `<feature>.lzi` — the DSL surface (REQUIRED)
- `<feature>.lzx` — optional view-layer declaration (REQUIRED if feature has UI)
- `<feature>.{web,mobile}.lzx` — optional platform projections
- `<feature>.ctx.md` — optional LLM context pack
- Extension code subdirs (only the ones the feature needs):
  - `handlers/` — Go extension points referenced via `@fn.*`, `@validator.*`, `@hook.*`, `@auth.*` (verify/hash functions, custom validators, lifecycle hooks)
  - `migrations/` — manual SQL additions (codegen emits its own to `dist/go/<feature>/migrations/`; this is for hand-written ones)
  - `templates/` — email/notification body templates per locale (`<name>.<locale>.tmpl`)
  - `queries/` — `*.sql` files referenced by `query.sql @file.<name>`
  - `i18n/` — feature-local translation catalogs
  - `hooks/` — workflow hooks (separate from `handlers/` because workflow lifecycle hooks have a different signature)

**Convention rule:** filenames inside subdirs match the DSL reference name. `@fn.verify_password` → `handlers/verify_password.go` with `func VerifyPassword(...)`. The codegen + doctor enforce this via the existing `@fn.*` resolution rules.

### §3.2 What's NOT in features/

- `app.lzi`, `registry.lzi`, `profiles.lzi`, `workspace.lzi` (optional) — top-level only.
- `contracts/` — external API/event contracts (Stripe, OpenAI, internal services). One file per contract version.
- Global i18n in top-level `i18n/` (for app-wide labels, not feature-specific).

### §3.3 Lazuli primitives → Lazurite folder map

**Boundary discipline (Rule Zero from `docs/design-principles.md`):** Lazurite **does not add new extension mechanisms, escape hatches, or architectural points to Lazuli**. Everything in this section is a **pre-existing Lazuli primitive** with IR shape, doctor invariants, and codegen contract. Lazurite's contribution is **giving each one a canonical filesystem location** so `lazuli new` scaffolds it and doctor enforces the placement.

This is the load-bearing claim that prevents distro creep: a future distro (Lazonyx, Lazpipe) may pick different folder names or different default plugins, but it **cannot invent new extension mechanisms** — those would have to enter the Lazuli language first.

#### Extensions (user-supplied code that the runtime calls)

| Lazuli primitive | DSL ref form | Lazurite location | Owner |
|---|---|---|---|
| Custom function | `@fn.<name>` | `features/<f>/handlers/<name>.go` | Lazuli (resolution rule), Lazurite (location) |
| Custom validator | `@validator.<name>` | `features/<f>/domain/validate_<name>.go` (or `handlers/`) | Lazuli, Lazurite |
| Lifecycle hook (workflow/resource) | `@hook.<name>` | `features/<f>/hooks/<name>.go` | Lazuli, Lazurite |
| Job handler | `job handler @fn.<name>` | `features/<f>/jobs/<name>.go` | Lazuli, Lazurite |
| Webhook verifier | `webhook verify @validator.<name>` | `features/<f>/integrations/<name>.go` | Lazuli, Lazurite |
| Domain function | `@fn.<name>` (domain-tier) | `features/<f>/domain/<name>.go` | Lazuli, Lazurite |
| Query SQL | `query.sql @file.<name>` | `features/<f>/queries/<name>.sql` | Lazuli, Lazurite |
| Adapter binding | `@adapter.<name>` | declared in `registry.lzi` + plugin Go module | Lazuli (catalog), `[plugins]` in `lazurite.toml` (resolution) |

#### Escape hatches (deviating from the canonical convention)

| Lazuli primitive | What it does | How Lazurite treats it |
|---|---|---|
| `at "path"` keyword | Override default file location for an extension | Doctor permits but emits info: "non-canonical path; consider conventional location". Used when extension code legitimately lives outside `features/<f>/` (e.g., shared util across many features). |
| Whole-block redeclaration in `.lzx` audience/tenant | Override view for specific audience/tenant cross-product | Native to `.lzx`; Lazurite has no special handling. |
| Raw SQL via `query.sql @file.<name>` | When Lazuli's typed query layer can't express the query | Lazuli-owned; Lazurite locates the file at `features/<f>/queries/<name>.sql`. |
| Custom `extensions` block in `registry.lzi` | Declare a new extension point catalog entry | Lazuli-owned; lives in `registry.lzi`. Lazurite scaffolds an empty `registry.lzi` and never overwrites it. |
| Direct Go in `features/<f>/handlers/` with no DSL ref | (Not supported) — every file under `handlers/` must back a `@fn`/`@validator`/`@hook` ref | Doctor errors: `ORPHAN-HANDLER-001`. Force user to either delete the file or add the DSL declaration. |

#### Architectural points (project-level declarations)

| Lazuli primitive | Where it lives | What Lazurite does |
|---|---|---|
| App entrypoint | `app.lzi` (top-level) | Scaffolded by `lazuli new`; doctor enforces presence |
| External contracts | `contracts/<name>.lzi` | Scaffolded as empty dir; doctor permits absence |
| Distributed-system topology | `workspace.lzi` (top-level, optional) | Not scaffolded by default; advanced opt-in (`lazuli new --workspace`) |
| Environment overlays | `profiles.lzi` (top-level, optional) | Not scaffolded by default; opt-in when app has multi-env complexity beyond `app.lzi environments` |
| Registry of integrations/packs/capabilities | `registry.lzi` (top-level) | Scaffolded with a header comment + placeholder; populated as user declares adapters/integrations |
| Deploy gates | `app.lzi deploy { ... }` block | Owned by `app.lzi`; **NOT** in `lazurite.toml` (§13.3 rejection) |
| Environments + URLs + CORS | `app.lzi environments`/`urls`/`cors` blocks | Owned by `app.lzi`; **NOT** in `lazurite.toml` (§13.6 rejection) |
| Audience/tenant declarations | `.lzx audience <name>` (per-feature) | Lazurite reads audiences via `[frontends.*].audiences` to project per-frontend SDKs (§4.4); does NOT re-declare audiences |
| Plugin modules | `lazurite.toml [plugins]` + `go.mod` | **Resolution** is in Lazurite manifest; **DSL reference** (`@plugin/<name>`) stays in `.lzi` |

#### Sanity check: what Lazurite is *not* allowed to do

To prevent distro creep, the following are explicitly forbidden in Lazurite v0 and any future distro that calls itself a "Lazuli distro":

- Adding a new `@-namespace` (e.g. `@distro/<name>`) — the namespace catalog is closed and lives in `crates/lazuli_lsp/src/lib.rs::is_allowed_reference_namespace`.
- Adding new `kind` keywords (e.g. `widget`, `pipeline`) — `.lzi`/`.lzx` grammar is closed.
- Shadowing a Lazuli primitive with a distro version (e.g. a Lazurite-specific `@fn` resolution path).
- Carrying business logic in templates or boilerplate scaffolded by `lazuli new`.

If a distro genuinely needs a new primitive, the primitive **must enter Lazuli first** (with grammar update + doctor invariant + codegen contract), then the distro adopts it. This is the same boundary rule that prevents Nuxt modules from extending the Vue compiler.

---

## §4. `lazurite.toml` — workspace manifest

A single TOML file at the project root that holds **build/run environment glue** the DSL doesn't own — version pins, codegen settings, plugin module resolution, distro template lineage. TOML over `.lzi` because: (a) CI/Dependabot/IDE-plugin-friendly without the Lazuli parser, (b) the DSL is for *declarations* and *contracts*; the manifest is *environment glue*.

**Strict boundary (this is the §5 determinism fix):** `lazurite.toml` does NOT duplicate anything `.lzi` already owns. Per `docs/invariants.md:14-15`, `app.lzi` owns **environments, URLs, generated targets, runtime units, provider-neutral deploy gates, and logical service boundaries**. The manifest never re-declares those. Doctor enforces the boundary (see §4.8 + §7.1).

**Name rationale**: `lazurite.toml` (distro-named, **Nuxt→Vue analogy**). Three reasons: (i) Nuxt has `nuxt.config.ts`, not `vue.config.ts` — the distro names the project manifest, not the framework; (ii) Lazuli alone needs no manifest (`examples/full-capsule/` proves this — pure Lazuli, no Lazurite, no manifest); (iii) future distros (Lazonyx ERP, Lazpipe automation) each ship their own `<distro>.toml` with their own conventions, cleanly separated. Sections inside the file CAN carry framework-level concerns (`[lazuli]`, `[plugins]`, `[generate]`) — same as `nuxt.config.ts` carries Vue-level keys — but the file itself is distro-owned.

### §4.1 Required minimum

```toml
[project]
# Project name (cosmetic; defaults to derived from go.mod module path).
name = "myapp"

# Go module path (echoes go.mod; doctor cross-checks).
module = "github.com/myorg/myapp"

# Manifest schema version. Doctor enforces compatibility.
schema = 1

[lazuli]
# Pin the Lazuli framework version. Codegen reads this; if go.mod's
# `lazuli.dev/runtime/lazuli` version doesn't match, doctor errors.
runtime = "0.1.0"

[lazurite]
# Distro template that scaffolded this app (informational + upgrade target).
# Absent or "bare" = no Lazurite conventions assumed; doctor treats lazurite.toml
# as advisory rather than enforcing the Lazurite app shape (§14.5).
template = "lazurite-default"
template_version = "0.1.0"
```

### §4.2 Plugin registry

Replaces the implicit "plugins live in go.mod" pattern with an explicit declarative source. Codegen reads this and emits the import + `RegisterAdapter` calls; runtime is wired at boot.

```toml
[plugins]
# Each plugin maps a DSL @-ref to its Go module + version.
# Adapter contracts live in @plugin/<name>; codegen emits an anonymous
# `_ "<module>"` import in main.go to trigger the plugin's init().

"@plugin/mercadopago" = { module = "github.com/lazurite/lazuli-plugin-mercadopago", version = "v0.2.0" }
"@plugin/expo_push"   = { module = "github.com/lazurite/lazuli-plugin-expo-push", version = "v0.1.0" }
"@plugin/google_maps" = { module = "github.com/myorg/lazuli-plugin-google-maps", version = "v0.0.1" }

# Local development override (rare; doctor warns if used in env=prod):
# "@plugin/mercadopago" = { path = "../lazuli-plugin-mercadopago" }

# @runtime/<name> commodities (postgres/redis/s3/...) are NOT listed here.
# They live in the Lazuli core runtime; `runtime/lazuli/<bucket>` provides
# the wire and codegen emits the wire automatically.
```

Doctor diagnostics (full list in §7.1):
- `PLUGIN-NOT-DECLARED-001` — `.lzi` references `@plugin/<X>` not in `[plugins]`.
- `PLUGIN-NAMESPACE-MISMATCH-001` — wrong namespace (e.g. `@runtime/mercadopago` should be `@plugin/mercadopago`); cross-check via `lazuli_lsp::is_allowed_reference_namespace`.

### §4.3 Generation settings

```toml
[generate.go]
out = "dist/go"
gofmt = true               # default; disable for debug
strict = true              # error on doctor warnings
emit_main = true           # emit dist/go/main.go entrypoint
submodule = true           # emit dist/go/go.mod as a sub-module (see §6.1)
```

TypeScript codegen is **not** a single `[generate.ts]` block — it's driven by `[frontends.*]` (§4.4 below) because real Lazuli apps frequently have multiple TS surfaces (mobile + admin web + customer web), each scoped to a different audience.

### §4.4 Frontend topology — `[frontends.*]`

A Lazuli app can have **multiple frontends sharing one backend**, with different audiences and platforms. Hostpoint is the canonical example: Expo mobile (traveler + host audiences) + web view for host management + admin web for internal staff. Three frontends, one Go backend.

`.lzx` already supports this via audience declarations (`audience host`) and platform projections (`*.web.lzx`, `*.mobile.lzx`). The manifest **declares which frontends exist** and codegen emits **one SDK per frontend, audience-scoped**.

```toml
[frontends.mobile]
target = "expo"                       # codegen target: lazuli generate expo
out = "dist/ts-mobile"
audiences = ["traveler", "host"]      # mobile shows both roles

[frontends.web-host]
target = "tanstack-vite"
out = "dist/ts-web-host"
audiences = ["host"]                  # web limited to host audience

[frontends.admin]
target = "tanstack-vite"
out = "dist/ts-admin"
audiences = ["admin"]                 # admin staff frontend
```

**Per-frontend SDK scoping**: codegen reads `audiences = [...]` for each frontend and emits **only** the commands/queries/types the listed audiences are allowed to call (per `policy` declarations in `.lzi` and `audience` blocks in `.lzx`). The host-web SDK literally does not contain admin-only endpoints — they're a compile-time non-thing.

Doctor diagnostics:
- `FRONTEND-AUDIENCE-UNKNOWN-001` — `[frontends.<x>].audiences` lists an audience not declared in any `.lzx`.
- `FRONTEND-TARGET-UNKNOWN-001` — `target = "<X>"` not in the closed enum (see below). Parser rejects at TOML parse time via serde `#[serde(rename_all = "kebab-case")]`.
- `FRONTEND-TARGET-MISSING-001` — `target` is in the catalog but codegen for it isn't shipped yet (v0 ships `tanstack-vite`; `expo` is a follow-up). Warning, not error.
- `FRONTEND-OUT-COLLISION-001` — two `[frontends.*]` blocks declare the same `out` path.
- `AUDIENCE-NO-FRONTEND-001` — `.lzx audience <X>` declared but no `[frontends.*]` lists `<X>` in `audiences`. Warning: orphan audience = dead view code; either ship a frontend that consumes it or remove the `.lzx audience` block.

**Closed enum for `target`** (closed set, doctor + parser enforce):

| Value | Codegen | Status |
|---|---|---|
| `tanstack-vite` | TS+React+Vite+TanStack Router/Query/Form | v0 candidate (post-Lazurite-bootstrap) |
| `expo` | TS+React Native via Expo Router | v0 candidate (post-Lazurite-bootstrap) |
| `next` | TS+Next.js (SSR-first) | reserved; not shipped |
| `tauri` | TS+Tauri (desktop) | reserved; not shipped |
| `cli` | Go CLI binary with command surface | reserved; for apps that ship CLI tools |

A future distro can extend this catalog only via Lazuli core PR (the catalog lives in `crates/lazuli_cli/src/lazurite_manifest.rs::FrontendTarget` enum + closed `serde` rename).

**Projection-to-target mapping rule:** `.lzx` platform projections compose with `[frontends.<name>].target` as follows:
- `target = "tanstack-vite"` consumes `<feature>.web.lzx` (preferred) or `<feature>.lzx` (fallback if no platform-specific projection exists).
- `target = "expo"` consumes `<feature>.mobile.lzx` (preferred) or `<feature>.lzx` (fallback).
- `target = "next"` consumes `<feature>.web.lzx` (same as tanstack-vite).
- `target = "tauri"` consumes `<feature>.web.lzx` (same; renders in webview).
- `target = "cli"` consumes only `<feature>.lzi` (no `.lzx`).

Doctor diagnostic `PROJECTION-MISSING-001` warns when a frontend's audience views can't be projected (e.g., `target = "expo"` but no `<feature>.mobile.lzx` and no fallback `<feature>.lzx` exists).

**v0 scope**: codegen TS is **not in v0**; `[frontends.*]` parsing + doctor diagnostics ship in v0, actual TS/Expo codegen is a follow-up. The schema is forward-compatible.

**Single-frontend apps**: simplest case (one Expo app, one audience) collapses to one `[frontends.mobile]` block. `lazuli new` scaffolds `[frontends.web]` by default; `lazuli new --template=mobile` swaps to Expo default.

### §4.5 Migration policy

```toml
[migrations]
# Where Lazurite expects generated migration SQL.
generated = "dist/go/migrations"
# Where developers add manual migrations (e.g. data backfills).
manual = "migrations"
# Strategy: how migrations are applied at boot.
# auto = run pending on `lazuli dev`; manual = require `lazuli migrate up`;
# check-only = doctor verifies but never applies.
strategy = "auto"
```

**Boundary note**: `app.lzi` declares `deploy { migrations before_deploy }` style gates (see `examples/full-capsule/app.lzi`); `lazurite.toml [migrations].strategy` is the *runtime* policy for whether migrations apply at boot. They compose: DSL says "migrations must precede deploy"; manifest says "apply them automatically at boot". Doctor cross-checks (`MIGRATION-STRATEGY-CONFLICT-001` if `strategy = "manual"` but `deploy_gates.migrations.before_deploy = true`).

### §4.6 Seeds

```toml
[seeds]
dir = "seeds"
# Seeds run after migrations on `lazuli dev` and on explicit `lazuli seed`.
# Never auto-runs in env=production regardless of this flag.
auto = false
```

### §4.7 Local development overrides (advanced)

```toml
[dev]
# Override module resolution for local plugin development.
# Doctor warns when this section is non-empty and env=prod is targeted.
plugin_paths = { "@plugin/mercadopago" = "../lazuli-plugin-mercadopago" }
```

### §4.8 Reserved table

| Section | Purpose | Status |
|---|---|---|
| `[project]` | Project name + Go module + manifest schema version | required |
| `[lazuli]` | Framework version pin | required |
| `[lazurite]` | Distro template lineage | optional (absent = bare; §14.5) |
| `[plugins]` | DSL `@plugin/*` → Go module resolution | optional |
| `[generate.<target>]` | Codegen settings per target (Go) | optional |
| `[frontends.<name>]` | Frontend topology (per-frontend audience-scoped SDK) | optional (required if app has >1 frontend) |
| `[migrations]` | Migration runner policy | optional |
| `[seeds]` | Seed policy | optional |
| `[dev]` | Local dev overrides (plugin paths, etc.) | optional |
| `[runtime]` | (future) Lazuli Go runtime version pin if it diverges from `[lazuli].runtime` | reserved |
| `[targets]` | (future) Enable/disable codegen targets per project | reserved |
| `[scripts]` | (future) Custom verbs | reserved |
| `[deploy]` | **NOT** in v0 — `app.lzi deploy { ... }` is canonical (see §13.3) | rejected |
| `[env.*]` | **NOT** in v0 — `app.lzi environments`/`urls` is canonical (see §13.6) | rejected |

### §4.9 `inspect` integration — manifest is LLM-visible

The proposal must close the AI-first gap: declarations outside the IR are invisible to LLMs reading `lazuli inspect` output.

Resolution: `lazuli inspect --include=manifest` (and the default `lazuli inspect --format=json` when `lazurite.toml` exists) surfaces the manifest as a derived JSON node alongside the IR graph. Shape:

```json
{
  "ir": { ... canonical IR ... },
  "manifest": {
    "origin": "lazurite.toml",
    "project": { "name": "myapp", "module": "github.com/myorg/myapp", "schema": 1 },
    "lazuli": { "runtime": "0.1.0" },
    "lazurite": { "template": "lazurite-default", "template_version": "0.1.0" },
    "plugins": [
      { "ref": "@plugin/mercadopago", "module": "github.com/lazurite/lazuli-plugin-mercadopago", "version": "v0.2.0", "source": "remote" }
    ],
    "generate": { "go": { "out": "dist/go", "submodule": true, "emit_main": true } },
    "migrations": { "strategy": "auto", "generated": "dist/go/migrations", "manual": "migrations" }
  }
}
```

LLMs reading the inspect pack now see the framework version, plugin set, migration strategy without having to find/parse the TOML themselves. Doctor + LSP also reuse the same parsed structure (single source of truth — `crates/lazuli_cli/src/lazurite_manifest.rs` per cell L1).

---

## §5. `lazuli new` evolved

Current behavior (post-reset): writes `app.lzi`, `registry.lzi`, README, `.gitignore`, `features/.gitkeep`.

Proposed behavior:

```bash
lazuli new myapp                       # default Lazurite template
lazuli new myapp --template=bare       # minimal — just app.lzi (current behavior)
lazuli new myapp --template=hostpoint  # opinionated Hostpoint-style starter (future)
lazuli new myapp --no-git              # skip git init
lazuli new myapp --module=github.com/foo/myapp  # explicit module path
```

### §5.1 Default template — what gets scaffolded

For `lazuli new myapp` (default Lazurite template):

```
myapp/
├── lazurite.toml                # populated with module path + lazuli v pin
├── app.lzi                      # `app Myapp\n  urls\n    dev: "http://localhost:3000"`
├── registry.lzi                 # placeholder declarations
├── features/
│   └── account/                 # one example feature — auth, the universal need
│       ├── account.lzi          # minimal auth surface (identity/password/sessions)
│       ├── handlers/
│       │   ├── hash_password.go   # @fn.hash_password skeleton (argon2id wire ~10 LOC)
│       │   └── verify_password.go # @fn.verify_password skeleton
│       └── templates/
│           ├── welcome.en-US.tmpl
│           └── welcome.pt-BR.tmpl
├── i18n/
│   └── common.en-US.json        # placeholder catalog
├── scripts/
│   └── seed.sh                  # shell script template
├── go.mod                       # `module <derived_or_flag>` + require lazuli.dev/runtime/lazuli
├── go.sum                       # generated by go mod tidy at end of `lazuli new`
├── .gitignore                   # ignores dist/ .lazuli/ secrets
├── README.md                    # tailored: lazuli dev, doctor, generate go, run
└── .git/                        # initialized unless --no-git
```

After scaffolding, `lazuli new` automatically runs:
1. `go mod tidy` (if Go toolchain present) — populates `go.sum`
2. `lazuli doctor myapp/` — sanity check the generated shape
3. `git init && git add -A && git commit -m "initial: lazuli new"` (unless --no-git)

### §5.2 Template registry

Templates live in **`crates/lazuli_cli/templates/<name>/`** at build time and are **embedded** into the CLI binary via `include_dir!`. Three reasons:
- Network-free `lazuli new` (no clone latency, no offline failure).
- Templates version-locked with the CLI version that ships them.
- Easy to override at runtime via `--template-dir=<path>` for development of new templates.

Future cut: external template registry (`lazurite-templates/<name>` repos) loaded on-demand.

---

## §6. Generated artifacts — `dist/` is canonical

**Resolves the `dist/` vs `.lazuli/generated/` drift** in current docs.

Decision: **`dist/`** for all generated output. Rationale:
- Web ecosystem convention (Vite, esbuild, tsc, Webpack, Aerocoding).
- `dist/` clearly says "build output" to any developer.
- `.lazuli/` reserved for **internal cache + manifests** (graph.json, source-map.json, manifest.json), not generated user-facing code.

Layout (paths reflect `[generate.<target>]` + `[frontends.<name>].out`):

```
dist/
├── go/                          # [generate.go].out — backend
│   ├── main.go                  # entrypoint (emit_main = true)
│   ├── go.mod                   # `module <app>-generated` (sub-module)
│   ├── <feature>/*.gen.go       # per-kind emissions
│   └── migrations/              # *.up.sql / *.down.sql
├── ts-mobile/                   # [frontends.mobile].out — Expo SDK, audience-scoped
│   └── <feature>/*.gen.ts
├── ts-web-host/                 # [frontends.web-host].out — TanStack SDK, host audience only
│   └── <feature>/*.gen.ts
└── ts-admin/                    # [frontends.admin].out — TanStack SDK, admin audience only
    └── <feature>/*.gen.ts

.lazuli/
├── graph.json                   # IR snapshot (for incremental codegen)
├── source-map.json              # IR position ↔ generated line ↔ source line
└── manifest.json                # extension file registry (handlers/templates/queries)
```

Single-frontend apps collapse to `dist/go/` + one TS folder (e.g. `dist/ts-web/`). The `[frontends.*]` topology is opt-in via the manifest; absent means "single default frontend".

`project-structure.md` will be amended in this proposal: `.lazuli/generated/**` → `dist/<target>/**`.

### §6.1 `dist/go/go.mod` as sub-module — pros, cons, decision

Proposal at §3 shows `dist/go/go.mod` as a Go sub-module (separate from the project's root `go.mod`). This is a load-bearing call; flagging explicitly per architect note.

**Pros:**
- Clean separation: the project's `go.mod` lists user-facing deps (Lazuli runtime + plugins + any custom imports from `features/<f>/handlers/`); `dist/go/go.mod` lists only what the generated code needs (Lazuli runtime).
- `go build ./dist/go/...` builds the generated binary in isolation; no leakage of dev/test deps.
- Regen of `dist/` doesn't touch the root `go.mod`.

**Cons:**
- `go mod tidy` runs in two places (root + dist/go/). CI complexity.
- `replace` directives for local plugin development (path-based plugins from `[dev].plugin_paths`) must be mirrored in `dist/go/go.mod` or `replace` resolution breaks. Codegen handles this — but it's logic that needs to exist.
- Workspace mode (`go.work`) is the cleanest fix for both files coexisting. Default scaffold writes `go.work` referencing both modules.

**Decision:** **sub-module = default** (`[generate.go].submodule = true`). Codegen emits `dist/go/go.mod` + a top-level `go.work` listing both modules. `lazuli new` scaffolds `go.work` automatically. Doctor checks both `go.mod`s for consistency (`SUBMODULE-DRIFT-001` if `dist/go/go.mod` references a Lazuli runtime version different from the root).

Set `submodule = false` for advanced apps that prefer one module (e.g. monorepos using `go.work` for other reasons). Doctor stays silent in that case but warns if the root `go.mod` doesn't list Lazuli runtime (it must, since generated code is now part of the root module).

---

## §7. Plugin resolution model

Today: plugins register at runtime via `lazuli.RegisterAdapter("@plugin/<name>", impl)` from inside the plugin's `init()`. Implicit: the app's `go.mod` must include the plugin module, and somewhere a Go import must trigger the init.

Lazurite tightens this:

### §7.1 Declarative wiring

`lazurite.toml` `[plugins]` lists every `@plugin/<name>` the app uses. Codegen reads this and emits:

```go
// dist/go/main.go (excerpt)
package main

import (
    _ "github.com/lazurite/lazuli-plugin-mercadopago"   // init() registers @plugin/mercadopago
    _ "github.com/lazurite/lazuli-plugin-expo-push"     // init() registers @plugin/expo_push
    "lazuli.dev/runtime/lazuli"
    // ... feature imports
)
```

Doctor diagnostics:
- `PLUGIN-NOT-DECLARED-001` — `.lzi` references `@plugin/<X>`, `lazurite.toml` doesn't declare it (error).
- `PLUGIN-UNUSED-001` — `lazurite.toml` declares `@plugin/<X>`, no `.lzi` references it (warning).
- `PLUGIN-NAMESPACE-MISMATCH-001` — `.lzi` or `lazurite.toml` uses wrong namespace for a known adapter (e.g. `@runtime/mercadopago` should be `@plugin/mercadopago` per `project_plugin_namespace_policy`). Cross-checked against `crates/lazuli_lsp::is_allowed_reference_namespace` catalog (error).
- `SUBMODULE-DRIFT-001` — `dist/go/go.mod` Lazuli runtime version differs from root `go.mod` (error; applies only when `[generate.go].submodule = true`).
- `MIGRATION-STRATEGY-CONFLICT-001` — `[migrations].strategy = "manual"` but `app.lzi deploy { migrations before_deploy }` (warning; signals operator intent mismatch).

**Removed from v0.1 list**: `PLUGIN-MODULE-MISSING-001` (manifest declares plugin but `go.mod` lacks require). Reason: this is a Go build error already — duplicating it as a doctor diagnostic adds noise without value. Doctor can run `go list -m` if a fast-fail surface is needed, but doesn't error on the absence itself.

### §7.2 Plugin authoring contract

A `@plugin/<name>` repo (e.g. `github.com/lazurite/lazuli-plugin-mercadopago`):
- Go module path matches `lazurite.toml` `[plugins].<ref>.module`.
- `init()` calls `lazuli.RegisterAdapter("@plugin/<name>", &MyAdapter{})`.
- Adapter type implements the bucket-specific interface (e.g. `payments.PaymentGateway`).
- Repo carries a `plugin.lazurite.toml` declaring which buckets + DSL refs it satisfies (for the doctor's cross-check).

Reserved namespace policy (from memory `project_plugin_namespace_policy`):
- `@runtime/<name>` = commodity OSS infra (postgres/redis/s3). Lives in Lazuli core. Not in `[plugins]`.
- `@plugin/<name>` = proprietary/opinionated adapter. Lives in separate (often private) repo. Listed in `[plugins]`.

---

## §8. CLI surface — single `lazuli` binary

Decision: **one binary `lazuli`**, all verbs unified. No separate `lazurite` CLI. Reasoning:
- Nuxt has its own CLI because Nuxt is loadable as a node module; the CLI is just a node script. Different runtime story.
- Lazuli's CLI is a Rust binary; splitting adds packaging + version-skew complexity.
- The user-facing distinction between "framework verb" and "distro verb" is fuzzy and changes over time; better to evolve under one binary.

Verbs (post-Lazurite design):

| Verb | Owner | Status | Notes |
|---|---|---|---|
| `lazuli check` / `lazuli parse` | Lazuli | shipped | DSL parsing |
| `lazuli doctor` | Lazuli | shipped | Cross-package invariants; **extended** to read `lazurite.toml` |
| `lazuli inspect` | Lazuli | shipped | IR → JSON |
| `lazuli plan` | Lazuli | shipped | Schema/migration diff plan |
| `lazuli generate <target>` | Lazuli | shipped | Codegen entry |
| `lazuli dev` | Lazuli | shipped (PP3) | Watch + regen + run |
| `lazuli new <project>` | Lazuli verb / **Lazurite template** | shipped (minimal) | Verb is Lazuli; template content is Lazurite |
| `lazuli init <path>` | Lazuli | shipped | Writes a single `app.lzi` (for in-place adoption) |
| `lazuli migrate <up\|down\|status>` | **Lazurite** | proposed | Reads `lazurite.toml` migration policy + applies via runtime |
| `lazuli seed` | **Lazurite** | proposed | Runs `seeds/` scripts after migrations |
| `lazuli plugins <add\|list>` | **Lazurite** | future | Edits `lazurite.toml` + `go.mod` |

"Owner: Lazurite" means the verb's *behavior* depends on `lazurite.toml`; if a project doesn't have one, the verb prints a hint to run `lazuli init --lazurite` to upgrade.

---

## §9. Migrations layout

Builds on the existing `runtime/go/lazuli/migrations/` primitives.

```
myapp/
├── lazurite.toml             # [migrations] policy
├── features/account/
│   └── migrations/           # MANUAL migrations (rare)
│       └── 20260513_001_backfill_users.sql
└── dist/go/
    └── migrations/           # GENERATED — codegen emits one per resource
        ├── 20260513_001_account_user.up.sql
        ├── 20260513_001_account_user.down.sql
        └── 20260513_002_account_session.up.sql
```

Rules:
1. **Generated migrations** live in `dist/go/migrations/`. Filename: `<timestamp>_<seq>_<feature>_<resource>.{up,down}.sql`. Codegen owns these — regenerated on every `lazuli generate go`.
2. **Manual migrations** live in `features/<feature>/migrations/` (or top-level `migrations/` for app-wide ones). Filename: `<timestamp>_<seq>_<description>.sql`. Hand-written; never overwritten by codegen.
3. Migration **runner** (in `runtime/go/lazuli/migrations/`) merges both lists chronologically by filename timestamp before applying.
4. Default `[migrations].strategy = "auto"` runs all pending migrations on `lazuli dev` startup. Prod sets `"manual"` and runs `lazuli migrate up` explicitly.

---

## §10. i18n + templates layout

Two scopes:

```
myapp/
├── i18n/                                  # APP-WIDE catalogs
│   ├── common.en-US.json                  # shared keys (button labels, etc.)
│   └── common.pt-BR.json
└── features/<feature>/
    ├── i18n/                              # FEATURE-LOCAL catalogs
    │   ├── account.en-US.json
    │   └── account.pt-BR.json
    └── templates/                         # FEATURE-LOCAL email/notif bodies
        ├── welcome.en-US.tmpl
        └── welcome.pt-BR.tmpl
```

Rules:
1. App-wide catalogs (`i18n/common.<locale>.json`) load first.
2. Feature catalogs (`features/<f>/i18n/<f>.<locale>.json`) override on the same key.
3. Templates: filename = DSL reference. `template "./templates/welcome.{locale}.tmpl"` resolves to `features/<f>/templates/welcome.<negotiated-locale>.tmpl`.
4. Locale negotiation is runtime (`runtime/go/lazuli/i18n/`).

Codegen embeds template files into the binary via `embed.FS` at generate-time (so `go run` doesn't need filesystem access for templates at runtime). Generated code: `//go:embed templates/welcome.*.tmpl`.

---

## §11. Bootstrap plan — where Lazurite lives

Architecture.md says Lazurite "lives outside the Lazuli repo and may evolve independently." But for **v0 bootstrap**, splitting is premature — the Lazurite definition is still flexible enough that co-iteration is faster.

Phased plan:

### §11.1 Phase L0 — Lazurite inside Lazuli repo (now)

Location: `lazurite/` directory at repo root. Contents:
- `lazurite/templates/default/` — files included by `lazuli new` via `include_dir!`. The starter template.
- `lazurite/SPEC.md` — this proposal becomes the canonical spec doc.
- `lazurite/schemas/lazurite.toml.schema.json` — JSON schema for IDE autocomplete on lazurite.toml.

CLI integration: `crates/lazuli_cli/src/main.rs` reads templates from `../../lazurite/templates/` via `include_dir!` at build time.

This phase ships first. All learning happens here.

### §11.2 Phase L1 — Lazurite split out (after Hostpoint port stabilizes)

Once Lazurite is exercised against 2+ real apps (Hostpoint port + at least one other), split:
- New repo: `github.com/lazurite/lazurite`.
- Templates published as a Go module dependency of `lazuli_cli`.
- Or: templates fetched at `lazuli new` time from a tagged release (offline cache).

Indicators that L0→L1 is ready:
- Two real apps shipped on Lazurite without unforeseen folder restructuring.
- `lazurite.toml` schema hasn't changed in 60 days.
- Plugin ecosystem has 3+ external `@plugin/*` repos using the standard plugin authoring contract.

### §11.3 Phase L2 — Multiple distros (out of v0 scope)

Lazurite may inspire competing distros (`lazonyx`? `lazerite-mobile`?). At that point the `lazuli new --template=<distro>` registry becomes a real thing. Not v0.

---

## §12. Implementation cells (Phase Lazurite — ~8-11 cells)

Sequenced for parallel-friendly execution. Each ≤ ~200 LOC. Sizing/sequencing updated post-grade per architect notes.

| Cell | Scope | Files | Type | Depends on |
|---|---|---|---|---|
| **L0** | Doctor treats `lazurite.toml` as **optional** (not required) — current fixtures (`full-capsule`, `auth-roundtrip`, `smoke-hello`) must continue passing without one. | `crates/lazuli_cli/src/doctor.rs` (guard around manifest-aware diagnostics) | Rust, edit | — |
| **L1** | `lazurite.toml` parser (serde + validation) + `inspect --include=manifest` integration | `crates/lazuli_cli/src/lazuli_manifest.rs` (new), `crates/lazuli_cli/src/inspect.rs` (edit) | Rust, new+edit | L0 |
| **L2** | Doctor diagnostics for `lazurite.toml` (PLUGIN-NOT-DECLARED, PLUGIN-NAMESPACE-MISMATCH, SUBMODULE-DRIFT, MIGRATION-STRATEGY-CONFLICT). All gated by manifest-present check from L0. | `crates/lazuli_cli/src/doctor.rs` (additive) | Rust, edit | **L1** (needs parser output) |
| **L3** | `lazurite/templates/default/` directory tree + `include_dir!` wiring. Template content uses `{{app_name}}`/`{{module}}` placeholders. | `lazurite/templates/default/**`, `crates/lazuli_cli/Cargo.toml` | Mixed | — (independent) |
| **L4** | `lazuli new` evolved — read embedded template, substitute placeholders, write files. Includes `--template=bare` path that skips Lazurite assumptions (no `lazurite.toml`). | `crates/lazuli_cli/src/main.rs` (replaces `new_command`) | Rust, edit | L1, L3 |
| **L5** | `lazuli migrate <up\|down\|status>` verb (reads `[migrations]` policy, runs runtime migrator) | `crates/lazuli_cli/src/migrate.rs`, runtime wire | Rust + Go | L1 |
| **L6** | `lazuli seed` verb (executes `seeds/` scripts) | `crates/lazuli_cli/src/seed.rs` | Rust | L1 |
| **L7** | Codegen Go reads `[plugins]` → emits anonymous `_ "<module>"` imports in `main.go`. Codegen reads `[generate.go].submodule` → emits `dist/go/go.mod` + root `go.work`. | `crates/lazuli_codegen_go/src/emitter/root.rs`, `module.rs` | Rust | L1 |
| **L7b** | `[frontends.*]` parsing + doctor diagnostics (`FRONTEND-AUDIENCE-UNKNOWN`, `FRONTEND-TARGET-MISSING`, `FRONTEND-OUT-COLLISION`). **TS codegen itself is post-v0** — this cell ships the manifest surface + validation only. | `crates/lazuli_cli/src/lazurite_manifest.rs` (extend L1), `crates/lazuli_cli/src/doctor.rs` | Rust | L1 |
| **L8** | Update `docs/project-structure.md` to reflect `dist/` decision + Lazurite shape + manifest name (`lazurite.toml`). **Must wait for L1's final schema** to avoid lying. | `docs/project-structure.md`, `docs/invariants.md` (add Lazurite/manifest invariants) | Doc | **L1, L7** (needs final schema decisions) |
| **L9** | Migrate `examples/hostpoint-mini/` to Lazurite shape; verify Hostpoint port works against new shape. Acts as gate — if migration reveals schema gap, **rerun L1-L8**. | `examples/hostpoint-mini/**`, add `lazurite.toml`, restructure to `features/<f>/` | Fixture | All prior |
| **L10** | (post-L9 if needed) Schema refinement based on L9 findings. May be no-op if Lazurite shape survives migration. | TBD | — | L9 |

**Sequencing rules** (post-grade fixes):
- **L0 must land first**: doctor must treat `lazurite.toml` as optional before any L1+ work merges, so existing fixtures keep passing.
- **L1 is the schema author**: L2, L5, L6, L7, L8 all consume the schema L1 ships. They cannot run truly in parallel — they can be **kicked off in parallel** once L1 lands, but L1 is gating.
- **L3 is parallel to L1**: template files don't depend on the manifest schema (placeholders are independent).
- **L4 depends on both L1 and L3**: needs parser to substitute, template to read.
- **L8 sequentially after L1+L7**: docs must reflect the *final* schema (avoids the lying-doc anti-pattern flagged in v0.1 grade).
- **L9 is a gate**, not a closing cell. If `hostpoint-mini` migration uncovers a missing slot, the proposal goes to v0.3 and L1 reships. The cell is designed to be a feedback loop.

**Parallel kickoff (3 Codex agents):** L0, L1, L3 simultaneously. L2/L5/L6/L7 in second wave once L1 merges. L4 in third wave. L8 just before L9. L9 as gate (validation by Claude orchestrator, not Codex).

---

## §13. Cuts considered & rejected

### §13.1 `lazurite.lzi` instead of `lazurite.toml` (rejected)

Considered: making the manifest a `.lzi` file for consistency with the DSL.

Rejected because:
- Manifest is **environment glue** (URLs, secrets refs, version pins) — not declarations or contracts. DSL is for the latter.
- TOML is parseable by every CI tool, Dependabot, IDE plugin, language-agnostic script. `.lzi` requires the Lazuli parser; that's friction outside the framework.
- Vue → `vue.config.js`/`package.json`, Nuxt → `nuxt.config.ts`, Rails → `config/application.rb`. None of these are in the framework's DSL.

### §13.2 Separate `lazurite` binary (rejected)

See §8.

### §13.3 `[deploy]` block in v0 (rejected)

Deploy was part of Lazuli core in the GPT batch (memory `runtime-pre-vendor-audit-2026-05-13`); we deleted it. Deploy is opinionated per-host, per-cloud, per-team — out of scope for the framework AND the distro initially. Future cut, possibly as `@plugin/deploy-<target>` plugins.

### §13.4 Auto-generate plugin repos via `lazurite plugin new` (deferred)

The plugin authoring contract is well-defined enough that a scaffold verb makes sense. Deferred to Phase L1 (post-bootstrap) — we don't have enough plugin repos yet to know the right shape.

### §13.5 `workspace.lzi` as workspace manifest (rejected as primary, kept as advanced feature)

`workspace.lzi` exists per `project-structure.md` for monorepo/polyrepo roots. Kept for that purpose — declares multiple apps + their wiring. But it's not the *project* manifest; that's `lazurite.toml`. Multi-app workspaces have both: each app has `lazurite.toml`, the workspace root has `workspace.lzi`.

### §13.6 `[env.<name>]` blocks in `lazurite.toml` (rejected — boundary leak)

v0.1 of this proposal included `[env.dev]`, `[env.staging]`, `[env.prod]` blocks in the manifest carrying per-env URLs and database connection strings. Rejected by `lazuli-language-architect` grade at 5.5/10 on determinism.

Reasons:
- `app.lzi environments` + `urls` + `cors` already own this (`docs/invariants.md:14-15`, `examples/full-capsule/app.lzi:31-46`). Duplicating in TOML creates two sources of truth — directly violates Rule Zero (Vocabulary Over Mechanism: don't add a manifest-level mechanism to redo what the language already declares).
- DB connection strings with embedded credentials in source-controlled TOML is the wrong shape; envs resolve to `DATABASE_URL`-style env vars at runtime via `runtime/go/lazuli/config/`.
- `lazuli inspect` consumes the IR. `app.lzi environments` is in the IR; `[env.*]` in TOML would be a parallel surface invisible to inspect (and to all downstream LLM tooling).

Operational env-specific values (secrets, db urls) come from env vars at runtime, not from TOML. The manifest is for static facts only.

### §13.7 `lazuli new` clones from a GitHub template (rejected for v0)

Considered: skip `include_dir!`, instead `git clone github.com/lazurite/template-default` at scaffold time.

Rejected:
- Offline failure mode.
- Network latency on every `lazuli new`.
- Version-skew: the cloned template may use Lazuli features the local CLI doesn't have.

Reconsider for Phase L1 when Lazurite ships as separate repo (then template versions track release tags).

---

## §14. Open questions

### §14.1 Should the `lazurite.toml` `[plugins]` block also accept `path = "..."` for local development?

Yes, but limited to dev. Doctor should warn if a deployed env (`[env.prod]`) references a path-based plugin. Spec'd in v0.2.

### §14.2 How does `lazuli dev` resolve plugin paths during file watching?

Today `lazuli dev` (PP3) re-runs `lazuli generate go --out dist/go/` on change. After Lazurite, it should also re-read `lazurite.toml` and reload plugin imports. Probably: invalidate generated `main.go` whenever `lazurite.toml` mtime changes. Easy.

### §14.3 Profile overlays (`profiles.lzi`) vs `[env.*]` in `lazurite.toml`

Both exist:
- `profiles.lzi` — DSL-level overlays (e.g. enable certain features in staging).
- `[env.*]` — environment-specific URLs/db/static facts.

Boundary: profiles change *which features* run; envs change *where they run*. Doctor enforces.

### §14.4 What's the upgrade path for an existing app with no `lazurite.toml`?

`lazuli init --lazurite` (proposed verb) reads the existing `app.lzi` + scans for `@plugin/*` refs + scans `go.mod` + writes a `lazurite.toml` skeleton.

### §14.5 When is `lazurite.toml` required vs optional?

Resolution (drives cell L0):

| Scenario | `lazurite.toml` required? | Doctor behavior |
|---|---|---|
| Fixture for codegen testing (`examples/full-capsule/`, `examples/smoke-hello/`) | **No** | Doctor passes with no manifest; manifest-aware diagnostics (`PLUGIN-NAMESPACE-MISMATCH`, etc.) skipped. |
| App with no `@plugin/*` refs in `.lzi` | **No** (recommended) | Doctor emits info: "consider `lazuli init --lazurite` to formalize project metadata". Build still works. |
| App with `@plugin/*` refs | **Yes** | Doctor errors `MANIFEST-REQUIRED-001` when manifest missing but `.lzi` uses `@plugin/`. |
| App scaffolded via `lazuli new` (default, non-bare) | **Yes** (written automatically) | Standard diagnostics apply. |
| App scaffolded via `lazuli new --template=bare` | **No** | No manifest; only the canonical Lazuli framework verbs work; Lazurite-specific verbs (`lazuli migrate`, `lazuli seed`, `lazuli plugins`) print "this verb requires `lazurite.toml` (try `lazuli init --lazurite`)". |

**Auto-detection rule**: Doctor checks for manifest presence at `<project_root>/lazurite.toml`. If present, manifest-aware mode. If absent + no `@plugin/*` refs in IR, advisory mode. If absent + `@plugin/*` refs present, error.

### §14.6 How does `lazuli dev` (PP3) handle `lazurite.toml` changes?

PP3's file watcher (`crates/lazuli_cli/src/dev.rs`) watches `*.lzi`/`*.lzx` today. After Lazurite: also watch `lazurite.toml`. On change → invalidate generated `main.go` (re-run codegen). Easy add to existing watcher.

---

## §14.7 L9 gate results (2026-05-13)

The end-to-end validation against `examples/hostpoint-mini/` ran post Wave 1 + Wave 2 cherry-picks. Findings:

**What worked:**
- `lazuli generate go examples/hostpoint-mini --out examples/hostpoint-mini/dist/go` emits 20 files including `main.go`, `go.mod`, `go.work`, per-feature `*.gen.go`, and migrations. Codegen reads `lazurite.toml` (L7 wiring) and `[generate.go].submodule` defaults to sub-module emission.
- `doctor` passes hostpoint-mini once `lazurite.toml` is added (resolves `MANIFEST-REQUIRED-001` from L2).
- Workspace resolution: `go work sync` + `use ../../runtime/go` makes `lazuli.dev/runtime` (and all bucket sub-packages) resolvable locally without a published v0.1.0 tag. Transitive deps (`pgx`, `river`, etc.) auto-populate from runtime's `go.mod`.
- `[frontends.*]` topology validated independently via the `lazurite-multifrontend` fixture (3 frontends, 4 audiences).

**Gaps surfaced (follow-up cells, not blockers):**

| # | Issue | Surface |
|---|---|---|
| L10 | Codegen does NOT auto-emit `replace lazuli.dev/runtime => <relative-path>` for in-repo fixture builds. Test suite (`smoke_go_build.rs` and `auth_*_flow.rs`) appends it manually; `lazuli new` should append it when scaffolding inside the Lazuli monorepo or when `[generate.go].dev_replace = true`. | `crates/lazuli_codegen_go/src/emitter/module.rs` |
| L11 | Codegen emits `require <plugin-module>` in `dist/go/go.mod` for every `[plugins]` entry, but the placeholder modules in hostpoint-mini's manifest don't exist on github. Plugins listed with `path = "..."` should emit a corresponding `replace` directive; plugins with `module = "..."` should add a require with the correct version, AND doctor should warn if the plugin repo isn't accessible during `go work sync`. | same |
| L12 | Generated code that uses `@semantic.GeoPoint` imports `github.com/cridenour/go-postgis` but codegen does NOT add it to `dist/go/go.mod` `require` block. `go work sync` cannot resolve it. Codegen should track which Go packages it emits imports for and add corresponding `require` entries. | same |

**Gate verdict:** PASS with 3 documented follow-ups. The Lazurite shape (folder + manifest + frontend topology) is structurally sound; the gaps are codegen polish, not architectural redesigns. Hostpoint Phase 1 (Auth port) can proceed in parallel with L10-L12.

## §15. Decision gate

Approve to proceed with Cell L0 → L9 implementation. Suggested order (post-grade fixes):

1. **L0 + L1 + L3 in parallel** (3 Codex cells; doctor optionality guard, manifest parser, template tree).
2. **L2 + L5 + L6 + L7 in parallel** after L1 lands (4 cells; doctor diagnostics, migrate verb, seed verb, codegen plugin imports).
3. **L4** (scaffold verb) — needs L1 + L3.
4. **L8** (docs) — sequential after L1+L7 to avoid lying-doc.
5. **L9** (hostpoint-mini migration as gate) — Claude orchestrator validates e2e; if it surfaces a schema gap, the proposal goes to v0.3 and L1 reships.

Estimated total: **~8-10 cells, 2-3 days at 4 parallel Codex agents** assuming this proposal grades ≥ 9.0 with no major redesigns.

After L9 lands clean, Lazurite v0 is **shipped** and we kick **Hostpoint Phase 1 (Auth port)**.

---

## §16. Risks

| Risk | P | Impact | Mitigation |
|---|---|---|---|
| `lazurite.toml` shape needs a v2 before Hostpoint port lands | 30% | Re-template all existing fixtures | Keep schema additive; only add sections, never rename in v1 |
| Plugin registry causes confusion vs `go.mod` deps | 40% | Doctor friction | Doctor cross-checks: plugin in toml but not go.mod = error; vice versa = warn |
| `dist/` regen on every `lazuli dev` change feels slow (full feature regen) | 50% | DX hit | Incremental codegen via `.lazuli/graph.json` snapshot — future cut, not v0 |
| Bootstrap-in-repo (L0) makes Lazurite/Lazuli boundary fuzzy | 25% | Drift | Hard rule in CLAUDE.md: nothing in `lazurite/` may import Lazuli internals |
| Hostpoint port reveals missing convention (e.g. how multi-tenant orgs scaffold) | 35% | Re-template default | Iterate; L0 is meant to absorb this |

---

## §17. Out of scope for v0 of Lazurite

- CI/CD templates (GitHub Actions YAML, GitLab CI, etc.) — Phase L1+.
- Deploy targets (Docker, k8s manifests, cloud-specific) — `@plugin/deploy-<target>` later.
- Admin UI scaffold (Lazuli's admin DSL is gated; once shipped, Lazurite gets an admin starter).
- Test scaffolding beyond a placeholder.
- Pre-built design system / token starter for `.lzx` — wait until `.lzx` runtime exists.

---

## Appendix A — Comparison vs Nuxt / Next.js / Rails

| Concern | Nuxt | Next | Rails | Lazurite |
|---|---|---|---|---|
| Project manifest | `nuxt.config.ts` | `next.config.js` | `config/application.rb` | `lazurite.toml` |
| Pages/features dir | `pages/` | `app/` or `pages/` | `app/controllers/` + `app/views/` | `features/<name>/` |
| Convention enforcement | Strong (file = route) | Strong (file = route) | Strong (RESTful resources) | Strong (one feature = one DSL surface) |
| Code style | Editable by user | Editable by user | Editable by user | **Generated code is non-editable** (regen-only) |
| Plugin model | `modules: []` in config | npm deps + `next.config.js` plugins | gems + `Gemfile` | `[plugins]` in toml + go.mod |
| Default UI | Nuxt UI / Tailwind | Tailwind | ERB views + asset pipeline | (Future via `.lzx`) |

Lazurite occupies the same slot as the rightmost convention layer in each row, with one key difference: **generated code is not user-editable**. The convention isn't "where you write code" — it's "where the framework reads your declarations and where you write extension points".

---

## Appendix B — Migration from `app.lzi`-only apps to Lazurite shape

For existing fixtures (full-capsule, auth-roundtrip) to become Lazurite-shaped:

1. Add `lazurite.toml` at root (minimal: `[project] name="..." module="..." schema=1`; `[lazuli] runtime="0.1.0"`). Per §13.6, `[env.*]` blocks are NOT in the manifest — environments stay in `app.lzi environments`/`urls`.
2. Move feature `.lzi` files into `features/<name>/<name>.lzi` if currently flat.
3. Move handler files into `features/<name>/handlers/`.
4. Update doctor invocation to use the project root.

Migration helper: `lazuli init --lazurite` (proposed in §14.4).

`examples/full-capsule/` is deliberately kept flat as a **canonical codegen fixture**, not an app shape — exempt from migration.
