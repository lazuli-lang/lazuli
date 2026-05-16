# Lazuli Architecture

This document records the high-level architectural decisions for Lazuli's
implementation phase. The language layer (DSL syntax, IR, doctor, LSP,
inspect) reached a stable plateau at audit score 8.87 across three
consecutive panel runs. This doc captures the thinking that shapes the
runtime layer we now begin to build.

It is the prerequisite reading before any work in `runtime/go/`,
`runtime/ts/`, `crates/lazuli_codegen_go/`, or `crates/lazuli_codegen_ts/`.

## What we learned from a previous attempt

Lazuli's lead has shipped this kind of system before, under two names:

- **Aerocoding** — a UML-driven full-stack code generator. Users drew
  domain diagrams (entities, attributes, relations, actions, validations);
  the system emitted complete code for multiple language targets
  (TypeScript, Rust, Dart, .NET, SQL).
- **Orion Studio** — a port of Aerocoding into the Orion monorepo, with
  the engine extracted into `packages/schema-engine` and per-language
  generators under `src/languages/{typescript,rust,dotnet,dart,sql}`.

Both projects hit the same structural wall:

1. **Templates per language × per construct = combinatorial cost.** Each
   new domain feature (e.g., soft-delete, multi-tenancy, audit, retention)
   required updating templates in every target. Each new target
   multiplied template debt by an order of magnitude.

2. **Cross-cutting concerns lived in N places.** "How soft-delete works
   in SQL" and "how soft-delete works in the TypeScript ORM" and "how
   soft-delete works in the delete handler" had to be implemented in
   three different templates. A bug in the concept required three
   coordinated fixes.

3. **Library upgrades cascaded.** Bumping the ORM version meant rewriting
   templates that consumed it. Each target's ecosystem moved on its own
   schedule; staying current across all of them was full-time work.

4. **Generated code was the system's only output.** There was no
   "framework runtime" that the generated code consumed. The templates
   carried all the behavior. So bug fixes meant editing templates and
   regenerating, with diff churn proportional to the scope of the fix.

5. **Maintenance cost grew over time.** As the number of features
   crossed ~30, the number of generators stayed at 5, and the matrix of
   template combinations became unmaintainable for a small team.

Aerocoding/Orion Studio was source-of-truth-from-diagram and full code
generation. Lazuli inverts both. We start from a textual DSL (which an
LLM can author cold) and we generate **thin wiring**, not full
implementations. The implementations live in a runtime library that we
ship as part of the framework.

## What Lazuli is

Lazuli is a **full-stack framework on top of Go + React/TanStack + Expo**,
with a separate authoring DSL designed for LLM productivity. It is
single-target by design (the v0 target stack is fixed in
`docs/target-stack.md`) and ruthlessly opinionated, in the spirit of
Rails — but with more native batteries that map to modern needs (LLM
agents, multi-channel notifications, multi-tenancy, audit trails,
retention contracts, typed event reaction graphs).

The closest analogs and how Lazuli differs from each:

| Framework | Pitch | What Lazuli does differently |
|---|---|---|
| **Rails** | Convention over configuration in Ruby; batteries included; productivity first. | Same opinion philosophy, but Go+React stack (better LLM corpus and type safety end-to-end), DSL-driven authoring, and `dist/` is non-editable (you only edit `.lzi`/`.lzx`). |
| **Encore.dev** | Go with `//encore:api` decorators; runtime parses source and generates infra. | Lazuli has the same fat-runtime / thin-generated split, but with an external DSL instead of inline Go decorators. The DSL is denser, has a closed grammar, and doesn't require Go knowledge to author. |
| **Wasp** | DSL `.wasp` declarations + JS/TS code for actions/queries; generates React + Express scaffolding. | Lazuli is fully declarative — you do not write JS/TS in addition to the DSL. Generated code is non-editable. The runtime executes the contract; user-supplied code only enters through typed extension points (`@validator.*`, `@fn.*`, `@hook.*`, etc.). |
| **Aerocoding / Orion Studio** | UML diagrams → multi-target code generation. | Lazuli inverts the data flow: DSL is source, IR is the typed semantic graph, and diagrams (if needed) become a derivative output of the IR. No template-per-target maintenance — runtime is the implementation. |
| **Hasura / Strapi / JHipster** | Declarative schema → auto-API or scaffolding. | Lazuli covers more than data: workflows, agents, notifications, jobs, webhooks are all first-class. And the runtime is meant to be deployed, not just used as scaffolding. |

The category is "opinionated full-stack framework with declarative
authoring layer". The unique slot Lazuli occupies in that category is
**LLM-first, fully-declarative, non-editable-generated-code, modern stack
(Go + React/TanStack)**. None of the listed analogs occupy that slot.

## Lazuli vs Lazurite

Two names, two scopes:

- **Lazuli** — the framework itself. The single product. Includes the language (`.lzi`/`.lzx`), IR, compiler, the Go runtime library, the scaffold conventions, and the CLI. When in doubt, "Lazuli" refers to the whole framework.

- **Lazurite** — a distribution / ecosystem **on top of Lazuli**. A starter project with conventions already applied, default features wired, ready to run. Conceptually one of several possible distros; currently the only one shipped. Authored separately from the framework itself.

The relationship mirrors **Vue → Nuxt** or **React → Next.js**: Vue is the framework; Nuxt is the opinionated distribution that uses Vue and adds project structure, conventions, and defaults. Same shape here.

Operationally, `lazuli new myapp` will instantiate from a distro template (default: Lazurite). Lazurite lives outside the Lazuli repo and may evolve independently.

> Historical note: "Drusa" was an internal name used during early design discussions to refer to the runtime/framework portion of Lazuli. It is **no longer in use** — everything is just "Lazuli" (the framework) and "Lazurite" (the distro). Proposals and commits before 2026-05-11 may still use "Drusa"; treat it as historical vocabulary.

## Three internal layers

Lazuli is **one framework with three internal layers**. The names are
internal architectural labels, not separate brands. Mixing them is the
most common failure mode in DSL design and the source of most of the
boundary rules in `.ai/rules/lazuli-language-boundaries.md`.

| Layer | Owns | Embodied in |
|---|---|---|
| **Language** | Verifiable contracts: `.lzi`/`.lzx` source, IR, doctor, inspect, LSP, syntax highlighting. The grammar is closed; the namespace catalog is closed; the keyword set is closed. | `crates/` (Rust): `lazuli_syntax`, `lazuli_analyzer`, `lazuli_ir`, `lazuli_lsp`, `lazuli_cli`, `lazuli_codegen_*`. |
| **Runtime** | The substantial Go and TypeScript libraries that the generated code imports and calls. HTTP server, command dispatcher, query engine, event bus, workflow runtime, audit, RBAC, validators, multi-tenant scoping, db pool, transactions, rate limiter, cache, etc. | `runtime/go/lazuli/` (Go module) and `runtime/ts/lazuli/` (TS package). To be built. |
| **Adapters** | Concrete provider implementations: HTTP, gRPC, Kafka, NATS, Postgres, Redis, OpenAI, Anthropic, AWS, Stripe, MercadoPago, etc. Available under `@runtime/<adapter>` (first-party) or `@plugin/<publisher>/<adapter>` (third-party). | `runtime/go/adapters/` (or separate Go modules) and `runtime/ts/adapters/`. To be built incrementally. |

## Pipeline

```
.lzi / .lzx (source)
   │
   │ lazuli_syntax         ← parses to AST (positions, raw tokens)
   ▼
AST
   │
   │ lazuli_analyzer       ← resolves cross-file references, types
   ▼
IR (lazuli_ir)             ← typed canonical semantic graph
   │
   ├── lazuli_codegen_go  ──→ dist/go/<feature>/*.gen.go
   │                            ↓ imports
   │                       runtime/go/lazuli  ← runtime library executes
   │
   ├── lazuli_codegen_ts  ──→ dist/web/<feature>/*.gen.ts
   │                            ↓ imports
   │                       runtime/ts/lazuli ← runtime library executes
   │
   ├── lazuli doctor        ← cross-package invariants
   ├── lazuli inspect       ← LLM context pack (JSON)
   └── future sinks         ← ER diagrams, sequence diagrams, OpenAPI
                              export, autodoc — all derive from IR
```

Two properties matter here:

1. **IR is multi-consumer.** Codegen Go, codegen TS, doctor, inspect,
   and any future sinks (diagrams, autodoc, exports) all read the same
   IR. Resolving references and computing types happens once, in
   `lazuli_analyzer`. Each consumer reads the resolved semantic graph.
   This is why the IR survives even though we lock to a single target
   stack — the IR's value is amortized across five+ consumers, not
   single-purpose.

2. **Generated code is thin.** It declares (`var X = lazuli.Command[...]{...}`)
   and registers (`func init() { lazuli.Register(...) }`). It does not
   contain business logic, validation logic, or transport logic. Those
   live in the runtime library, written once.

## Where bugs get fixed

The blast radius of a fix depends on the layer it belongs to.

| Bug type | Where to fix | Blast radius |
|---|---|---|
| Runtime behavior is wrong (e.g., soft-delete doesn't filter from queries; rate limiter has off-by-one) | `runtime/go/lazuli/<concern>.go` | **One line, no regen.** All `dist/` callers inherit immediately. |
| Generated code shape is wrong (e.g., struct field name mismatched runtime expectation) | `crates/lazuli_codegen_go/` + regenerate | Affects the relevant `dist/*.gen.go` files only. Codegen test catches regressions. |
| DSL accepts something it shouldn't (e.g., `derived from` with `default`) | `crates/lazuli_lsp/` (diagnostic), optionally `lazuli_analyzer` for IR-level rejection | Detected at edit time. Codegen never sees invalid input. |
| Cross-feature contract broken (e.g., `extensible_by` doesn't match `extends @anchor`) | `crates/lazuli_cli/src/doctor.rs` | Caught by `lazuli doctor`. Runtime never reaches an inconsistent state. |

The vast majority of production bugs land in the first row: runtime
behavior. That's the metaframework win — most fixes are one line in
one file and require no regeneration. This is the structural reason
Aerocoding/Orion Studio became unmaintainable: there was no runtime
layer, so every behavioral fix was a template fix × N targets +
regeneration.

## Technology picks for v0

Picks lock to "stdlib first, focused libs second, custom code when the
concern is central to the DSL".

### Backend (Go)

| Layer | Pick | Rationale |
|---|---|---|
| HTTP routing | `net/http` (stdlib, Go 1.22+ enhanced ServeMux) | Routing matured in 1.22; zero deps; large LLM corpus. Chi is a strong runner-up if middleware groups become complex. |
| DB driver | `pgx/v5` | De-facto Postgres driver in Go; rich types; performance. |
| Query layer | Custom in `runtime/go/lazuli/query.go` | DSL emits `Customer.List(args)`-style calls; runtime translates to pgx queries internally. Avoids stacking another codegen tool (sqlc) or a heavy ORM (GORM). |
| Migrations | `atlas` | Declarative — Lazuli emits the desired schema; atlas computes the diff. Philosophy aligns. |
| Background jobs | `river` | Postgres-backed, transactional, observability built in. Same DB as the app for v0; a Redis-backed adapter can come later. |
| Event bus | In-process for v0 (Go channels + sync handlers); river for durable event-triggered jobs | Avoids running Redis/NATS as a hard dependency for v0. NATS/Redis arrive as adapters when cross-service pressure demands it. |
| Auth | Custom in `runtime/go/lazuli/auth.go` | DSL already declares `auth password algorithm: argon2id`, sessions, TOTP, OAuth. Runtime implements with `golang-jwt/jwt`, `pquerna/otp`, `golang.org/x/crypto/argon2`. |
| Validation | Custom in `runtime/go/lazuli/validator.go` | DSL models validators (`@validator.X`) with explicit ordering (`let`/`requires`). Runtime executes the pipeline. Avoids `go-playground/validator` struct tags, which would conflict with the DSL. |
| Observability | `slog` (stdlib) + OpenTelemetry | Stdlib first; OTEL is industry standard. |
| Email/SMS/push | Adapters under `@runtime/<provider>` (Sendgrid, Twilio, APNs, FCM) | DSL `notification` declares; runtime dispatches by channel; adapter handles transport. |
| LLM transport | Adapters under `@runtime/<provider>` (OpenAI, Anthropic) | DSL `agent model @llm.<name>`; adapter implements the provider call. |

### Frontend (Web — React + Vite)

| Layer | Pick | Rationale |
|---|---|---|
| Build tool | Vite | Industry standard; large LLM corpus. |
| Routing | TanStack Router | Type-safe route definitions; integrates with TanStack Query naturally. |
| Server state | TanStack Query | DSL emits `useCommand`/`useQuery` wrappers. Cache and invalidation contracts derive directly from DSL declarations. |
| Forms | TanStack Form | Type-safe; integrates with mutations. |
| Tables | TanStack Table | For `view list Table` projections. |
| Components | shadcn/ui (copied components, not a dependency) | Copy-and-customize model; no version skew; large LLM corpus. |
| Styling | Tailwind CSS | Industry standard; large LLM corpus; design tokens map well to Lazuli's design system. |
| Client state | Zustand if needed; otherwise React state | Server state is TanStack Query. |

### Mobile (Expo / React Native)

| Layer | Pick | Rationale |
|---|---|---|
| Routing | Expo Router | Mirrors TanStack Router conceptually. File-based. |
| Server state | TanStack Query | Shared with web via the universal `@lazuli/runtime/react` entrypoints. |
| Runtime resolution | Single `@lazuli/runtime` package; `react-native` exports condition selects `react.native.ts` on Metro, `react.web.ts` everywhere else. | Emitter stays target-agnostic — `package.json` does the work, not codegen. |
| Persistence hook | `useLocalSetting` with `AsyncStorage` body (web body uses `localStorage` + `useSyncExternalStore`). | First-render contract differs across platforms; documented in the hook's JSDoc and pinned by an exports-parity test. |
| Components | React Native primitives in scaffolded view files (Tamagui / shadcn-rn / etc. authored by the user). | Lazuli does not opt into a single RN component library — see `docs/proposals/mobile-target.md` §5.3. |
| Reference fixture | `examples/marketplace-mini-mobile/` | Hostpoint-shaped buyer audience + catalog/booking features. |

### What we are explicitly **not** bringing into v0

- **Redis as a hard dependency.** Postgres + river covers queues, jobs,
  and locks for v0.
- **Kafka / SQS / Pub/Sub as core dependencies.** Adapters when needed.
- **GORM, Ent, or similar ORMs.** Custom typed query layer is more
  predictable for LLM-emitted code.
- **GraphQL as the API layer.** Generated TS hooks talk REST/RPC to the
  Go runtime. GraphQL export can be a future derivative sink from IR.
- **Scaffolding-then-edit workflow.** `dist/` is regen-only; you do not
  edit it.
- **Multi-target codegen.** Locked to Go + React/TanStack/Expo for v0.
  The IR is target-agnostic so a future second target is structurally
  possible, but every additional target carries the maintenance cost
  Aerocoding learned the hard way.

## What we are explicitly building

A framework where the user's loop is:

1. Author `.lzi`/`.lzx` files (a few dozen lines per feature).
2. Run `lazuli check` (LSP catches violations) and `lazuli doctor`
   (cross-package invariants).
3. Run `lazuli generate` to produce thin wiring in `dist/`.
4. Run `go run` and `pnpm dev` to bring up the app.

When something breaks in production:

- An LLM reads the relevant `.lzi` (15-30 lines for a typical command).
- The DSL keyword maps to one file in `runtime/go/lazuli/<concern>.go`.
- A one-line fix in that runtime file ships to every consumer.

The DSL is auto-documenting: it declares what should happen in a closed
grammar. The runtime is where what actually happens lives. Both are
small-blast-radius by design.

## What we explicitly do not build

- A Lazuli-specific language compiler that emits everything from
  scratch. The runtime libraries do the work; the compiler routes.
- A template engine. Codegen emits Go and TS strings directly, then runs
  through `gofmt`/`biome` for formatting.
- A multi-target framework. Single target locked for v0. The IR can
  target other runtimes later if it is worth the maintenance, but that
  is a separate project, not a v0 commitment.
- A scaffolding tool that you edit. Generated code is regen-only.
- A "framework on top of frameworks" with infinite optionality. Lazuli
  is opinionated. There is one canonical way to do each thing in the
  DSL.

## Next step: runtime API spike

Implementation begins with a spike of the **runtime API**, not codegen:

1. Create `runtime/go/lazuli/` with a `go.mod` and the core type
   signatures: `Resource[T]`, `Command[I, O]`, `Query[A, R]`, `Event`,
   `Tenant`, `Policy`, `Validator`, `Audit`, `EventBus`. Signatures
   only at first.
2. Hand-write `dist/go/customer/customer.gen.go` consuming those
   signatures, using `customer.create` and `customer.query.list` from
   the full-capsule fixture as the test cases. Iterate the runtime API
   until the generated code reads cleanly.
3. Implement `Command.Handle()`: policy enforce → validators → tx →
   effect → emits → audit → invalidate caches → return. One file in
   the runtime, used by all generated commands.
4. Bring up Postgres locally; run a minimal `main.go` that registers
   the customer command and serves over HTTP. Verify the end-to-end
   path.
5. **Then** return to `crates/lazuli_codegen_go/` and replace the
   skeletal codegen with one that emits the proven shape from the IR.

Order matters: API first, codegen second. Reversing means regenerating
`dist/` repeatedly while the runtime API still flutters, which produces
churn the metaframework architecture exists to prevent.

The same loop applies to the web side later: `runtime/ts/lazuli/`
shape decided first by hand against `dist/web/customer.gen.ts`, then
codegen automation.

## Glossary

| Term | Meaning |
|---|---|
| **Lazuli** | The framework itself (lang + IR + compiler + Go runtime lib + scaffold + CLI). Single product. |
| **Lazurite** | A distribution/ecosystem on top of Lazuli — starter project with conventions and defaults wired. Analogous to Nuxt for Vue. Currently the only distro. |
| `.lzi` / `.lzx` | The Lazuli source language. `.lzi` is the domain/operational layer; `.lzx` is the experience/view layer. |
| IR | Internal representation. The typed canonical semantic graph produced by `lazuli_analyzer` and consumed by codegen, doctor, inspect, and future tools. |
| Runtime | The Go and TypeScript libraries (`runtime/go/lazuli/`, `runtime/ts/lazuli/`) that the generated code imports. The runtime executes the contract declared in the DSL. Hand-written and maintained, not generated. |
| Lazuli Go | Specific qualifier for the Go runtime library when disambiguation is needed. Canonical Go module path: `lazuli.dev/runtime/lazuli` with per-bucket subpackages `lazuli.dev/runtime/lazuli/<bucket>` (e.g. `auth`, `storage`, `jobs`). |
| Lazuli compiler | The Rust toolchain in `crates/` (lazuli_syntax, lazuli_analyzer, lazuli_ir, lazuli_codegen_go, lazuli_lsp, lazuli_cli, …). |
| `dist/` | Generated code, regen-only, not user-editable. Imports the runtime library; contains no business logic. |
| Adapter | A concrete provider implementation (HTTP transport, Postgres, OpenAI, etc.) under `@runtime/<adapter>` or `@plugin/<publisher>/<adapter>`. |
| Doctor | `lazuli doctor`: cross-package invariant checker. |
| Inspect | `lazuli inspect`: structured JSON output of IR for LLM context packs. |
| ~~Drusa~~ | Historical name (pre-2026-05-11) for the runtime/framework portion. **No longer in use**; replaced by "Lazuli" (framework) + "Lazurite" (distro). Old commits/proposals may still reference it. |
