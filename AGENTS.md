# Lazuli — Working Rules for AI Agents

Lazuli is an AI-first declarative language that compiles to Go (server) + React (web) + React Native Expo (mobile). The language (`.lzi` / `.lzx`) and IR are designed so an LLM can author + read source cold without external docs. This file is the canonical operating manual for any AI agent (Claude, GPT/Codex, future models) working in this repo.

Mirrored verbatim at `AGENTS.md` for tooling that loads `AGENTS.md` (Codex, Aider, etc.).

---

## Read first: scope discipline

Before doing any design or implementation work, read [`docs/scope-discipline.md`](docs/scope-discipline.md). It defines the **80/20 boundary**: what the framework owns (generics) vs what apps own (specifics, via five escape hatches: `@fn` handlers, `handler "./path.go"` on `api`, `query.sql`, `extends @anchor / slot`, user `main.go`). The framework does NOT absorb per-vendor adapters, per-country scalars, per-product UX flows, or per-client business rules.

**Operational rule**: if a proposal feels like it's making the framework conform to one specific app's specifics, it's a scope violation. Reject or kick to `@plugin/<name>`. The boundary moves only with ≥3-app pilot evidence + an architect-graded proposal (≥ 8.5).

---

## The founding principle (NEVER violate)

**Lazuli is abstraction; the Lazuli Go runtime is *wire*.**

The runtime in `runtime/go/lazuli/<bucket>/` **does not reimplement** primitives that already exist in Go stdlib / extended / mature SDKs. Each adapter / bucket helper is **~10-50 LOC of `import` + `call`**, not 200-800 LOC of homegrown logic.

**Concrete examples of what NOT to do**:

- `http_mtls.go` 587 LOC, zero external imports → should have been ~30 LOC wrapping `crypto/tls.Config` + maybe `caddyserver/certmagic`.
- `http_circuit_breaker.go` 311 LOC, zero external imports → should have been ~20 LOC wrapping `sony/gobreaker`.
- `views/markdown.go` 1066 LOC reimplementing markdown when `gomarkdown/markdown` exists.
- `testkit/coverage.go` 1112 LOC reimplementing what `go test -cover` already does.
- `rpc/grpc.go` 657 LOC reimplementing gRPC when `google.golang.org/grpc` is the de-facto standard.

**Test for your own work before committing:** open the file you just created, count external imports (`github.com/...`, `golang.org/x/...`, `gopkg.in/...`, `cloud.google...`). If LOC > 100 and external imports == 0 and the feature exists in any well-known Go library, **you are violating this principle**. Either rewrite as wire, or delete and use the library directly in the user's code.

See: `docs/architecture.md` lines 26-55 (founding principle).

---

## Namespace policy (CHECK BEFORE EVERY NEW FILE)

Two namespaces, strict separation:

- **`@runtime/<name>`** — OSS commodity infrastructure. Postgres, Redis, S3-protocol signing, SMTP, Kafka, NATS, RabbitMQ, webpush (W3C). Lives in this repo at `runtime/go/lazuli/<bucket>/`. Public.

- **`@plugin/<name>`** — Proprietary or opinionated providers. **Vendor SaaS, paid APIs, or specific named tools/products** (even if open-source). Stripe, MercadoPago, Sendgrid, Mailgun, Twilio, Datadog, Sentry, LaunchDarkly, Algolia, Meilisearch, Discord, Slack, PagerDuty, Expo Push, Google Maps, Mapbox, FCM, MinIO client, Prometheus exporter, OpenFeature SDK, Atlas migrations, etc. Lives in **separate (often private) repos** at `github.com/lazuli-lang/lazuli-plugin-<name>` (or under the user's own org for proprietary providers).

- **NEVER** `@plugin/<consumer-product>/<name>`. The adapter is named after the *provider*, not the consuming product. MercadoPago is `@plugin/mercadopago` (generic), not `@plugin/<app>/mercadopago` (product-scoped).

- **Plugins are multi-language by nature.** Most plugins have a Go server adapter (imported by `dist/go/main.go` via anonymous import + `init()` self-registration) plus optionally TS web (`web/`) and TS mobile (`mobile/`) sides for client-rendered widgets. See [`docs/plugin-authoring.md`](docs/plugin-authoring.md) for the canonical repo shape + adapter patterns + scaffold pipeline.

**Before writing a new adapter file, ask: "is this commodity infrastructure (open spec or de-facto-OSS layer) or is it a specific named product/service?"** If it's a named product, **do not put it in `runtime/go/lazuli/`**. Either it belongs in a separate `@plugin/<name>` repo, OR the user should write it as a regular Go module in their app.

---

## Grade-before-commit for proposals

Every design proposal (`docs/proposals/*.md`) goes through grading against the AI-first rubric in [`docs/grading-rubric.md`](docs/grading-rubric.md) before commit.

Pattern:
1. Write the proposal draft.
2. Grade against the 10-criterion rubric in `docs/grading-rubric.md`. Anchor every score with a `path:line` reference (one for strongest evidence, one for weakest spot).
3. Apply ALL blocker-level fixes; track polish items as future cells.
4. Re-grade. Target ≥ 9.0; gate at ≥ 8.5 with no individual dimension < 7.
5. Then commit + push.

The `skills/audit/` bundle is a portable LLM skill that automates running this rubric against any `.lzi` cold-read — useful for both proposal grading and personal `.lzi` audits.

---

## Folder conventions

### Authored sources (commit these)

```
app.lzi                   # Top-level app declaration (envs, urls, uses)
registry.lzi              # Integrations + plugin bindings
profiles.lzi              # (optional) env-specific overlays
workspace.lzi             # (optional) distributed-system root

features/<feature>/
  <feature>.lzi           # DSL surface — domain/policy/commands/queries/...
  <feature>.lzx           # abstract experience (optional, UI features only)
  <feature>.web.lzx       # web platform projection
  <feature>.mobile.lzx    # mobile platform projection
  handlers/<fn>.go        # @fn.* / @validator.* / @hook.* extension code
  domain/<fn>.go          # domain function extensions
  queries/<name>.sql      # raw SQL files referenced via query.sql @file.<name>
  jobs/<name>.go          # job handler extensions
  integrations/<name>.go  # webhook verifiers, adapter handlers
  templates/<name>.<locale>.tmpl  # email/notif templates
  i18n/<name>.<locale>.json       # feature-local catalogs

contracts/<service>.lzi   # External service contracts
i18n/common.<locale>.json # App-wide translation catalogs
lazurite.toml             # Workspace manifest (distros use distro-named TOML)
```

### Generated (gitignored unless committed deliberately)

```
dist/go/                  # Generated Go (regen-only)
dist/ts-<frontend>/       # Generated TS SDK per frontend (audience-scoped)
.lazuli/                  # Internal cache (graph, source-map, manifest)
```

**Convention rules:**
- Filenames inside `handlers/`, `domain/`, etc. **must match** the DSL reference. `@fn.verify_password` → `handlers/verify_password.go` with `func VerifyPassword(...)`. Doctor enforces.
- `.tmpl` files in scaffold templates use `{{app_name}}` / `{{module}}` placeholders; codegen uses Go `text/template` `{{.Field}}` syntax for runtime templates.
- `dist/` is never user-edited. Regen overwrites; do not commit edits.

See: [`docs/project-structure.md`](docs/project-structure.md), [`docs/proposals/lazurite-scaffold.md`](docs/proposals/lazurite-scaffold.md) §3 + §3.3.

---

## Lazuli vs Lazurite vocabulary

- **Lazuli** = the framework. Language (`.lzi`/`.lzx`) + IR + compiler (Rust crates in `crates/`) + Go runtime lib (`runtime/go/lazuli/`) + CLI (`lazuli` binary).
- **Lazurite** = the opinionated distribution on top of Lazuli. Folder conventions + `lazurite.toml` manifest + `lazuli new` template body. **One distro currently shipped** but the design space supports others.

A future distro (Lazonyx for ERP, Lazpipe for automation, etc.) **cannot add language mechanisms**. New `@-namespace`, new `kind` keyword, new escape-hatch → must enter Lazuli language first, then distros adopt. Same rule that prevents Nuxt modules from extending the Vue compiler.

See: [`docs/architecture.md`](docs/architecture.md) §"Lazuli vs Lazurite", [`docs/proposals/lazurite-scaffold.md`](docs/proposals/lazurite-scaffold.md) §3.3.

---

## Skills bundles (portable contributor tooling)

`skills/` holds portable LLM skill bundles meant to be dropped into any contributor's Claude Code (or compatible LLM authoring tool):

- `skills/audit/` — grade any `.lzi` cold against the canonical rubric.

Skill bundles are framework artifacts: they have zero coupling to any specific operator's setup, no private paths, no opinionated dispatching. Each contributor wires them into their own workflow as needed.

Operational dispatching (orchestration, slash commands, multi-agent coordination, dashboards) lives in each operator's private tooling, not in this repo.

---

## Inviolable language rules

1. **No provider names in core syntax.** No `stripe`, `mercadopago`, `openai`, `aws`, `kubernetes` keywords. Provider references go through registry adapter slots (`@runtime/...`, `@plugin/...`, `@adapter.<local>`).

2. **No DI mechanics in source.** Construction order, lifetimes, logger/db/client instances, test doubles — all Lazuli Go. The language declares `requires integration <slot>: <Capability>` and bindings, not `new()` or `inject()`.

3. **No transport mechanics in contracts.** `contract.lzi` declares schema, operation, event. It doesn't declare HTTP method routing tables, gRPC stub generation flags, broker partition strategies.

4. **No SDK generation as a language concept.** SDK exports for Python/TypeScript clients are an *artifact* of contracts, not a language feature.

5. **`workspace.lzi` is optional.** A single-app project never needs it. Reject any proposal that makes it mandatory.

6. **`container.lzi` does not exist** until registry contracts demonstrably can't express real plugin/runtime pressure. Today, registry can.

7. **Magic discovery requires visibility.** If a filename convention, prefix, or directory rule resolves into language semantics, it must surface in `lazuli inspect`, `lazuli doctor`, and LSP. No silent runtime behavior.

When you spot a violation: reject in line. Do not merge into a checklist for "later." The boundary is enforced through deletion, not migration.

---

## When you're unsure

Ask: "could a Lazuli project still function if the Lazuli Go runtime was replaced by a hypothetical second runtime targeting Rust + Yew + Flutter?" If the answer is no because the language is leaking Go-specific or React-specific assumptions, the proposal is at the wrong layer.

---

## When in doubt

- Read [`docs/architecture.md`](docs/architecture.md) end-to-end.
- Read [`docs/invariants.md`](docs/invariants.md) for the closed grammar/IR constraints.
- Read [`docs/design-principles.md`](docs/design-principles.md) — Rule Zero ("Vocabulary Over Mechanism") is the most-cited principle in design decisions.
- Read the relevant [`docs/proposals/<x>.md`](docs/proposals/) if working on the corresponding subsystem.
- Read [`docs/grading-rubric.md`](docs/grading-rubric.md) before proposing any language change.
