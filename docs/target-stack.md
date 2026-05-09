# Lazuli Target Stack

These choices are product invariants for Lazuli v0, not interchangeable
implementation details. Lazuli is AI-first: generated code is part of the
developer contract, and the best stack is the one that lets agents generate
boring, legible, idiomatic output with the least ceremony.

## Invariants

| Slot | Choice | Status | Runner-up |
| --- | --- | --- | --- |
| Backend target | Go | Keep | Elixir/Phoenix |
| Web frontend target | React + Vite | Keep | Solid + Vite |
| Web data/routing layer | TanStack Query + TanStack Router | Adopt as default layer | Framework-owned routing/data APIs |
| Mobile target | Expo / React Native | Keep | Flutter |

## Backend: Go

Go is the default backend target because it fits Lazuli's code generation
contract better than the alternatives.

- Codegen should emit readable `.go` files directly. Go's simple syntax,
  stable AST, `go generate` culture, and small feature set make generated code
  look like code a human would write.
- Lazuli's typed IR maps directly to Go structs and interfaces without Rust's
  lifetime/generic complexity or Java/Kotlin boilerplate.
- Goroutines, channels, and `context.Context` map cleanly to Lazuli jobs,
  webhooks, cancellation, and request lifecycles.
- The standard library covers most generated backend needs: `net/http`,
  `database/sql`, `encoding/json`, `crypto/*`, `context`, and `log/slog`.
- LLMs are consistently strong at Go because idiomatic Go has low variation.
  That matters more than theoretical expressiveness for an AI-first framework.
- Single-binary builds and simple cross-compilation keep deployment adapters
  straightforward.

Rust/Axum is not the backend target for v0. Rust would be appropriate for
systems-level or performance-critical software, but Lazuli's primary workload
is CRUD, workflows, jobs, webhooks, policies, and surfaces. The extra codegen
complexity is not worth the performance headroom.

Elixir/Phoenix remains the honest second choice because BEAM is excellent for
async workloads and supervision, but the smaller LLM training corpus and
ecosystem make it a weaker fit for Lazuli's AI-first premise.

## Web Frontend: React + Vite

React + Vite is the default web target.

- React has the largest LLM training corpus for frontend component generation.
- The modern component and interaction ecosystem assumes React: Radix,
  shadcn/ui, Tailwind, and TanStack all compose naturally here.
- Most product engineers already know React, so adopting Lazuli means learning
  the DSL rather than a new frontend runtime.
- Vite is a build tool, not a competing metaframework. It stays out of
  Lazuli's way. Next and Remix own routing, data loading, and server/runtime
  boundaries too strongly for Lazuli's generated architecture.

React's implicit render model is not a perfect semantic match for Lazuli's
declarative views. Lazuli should bridge that mismatch with generated
TanStack Query and TanStack Router code. A generated `query.list` or
`query.lookup` should map naturally to `useQuery` with stable keys, typed
query functions, loader integration where appropriate, and predictable cache
invalidation from generated commands.

Solid or Svelte would map more elegantly to Lazuli's declarative nature, but
the smaller ecosystem and weaker LLM corpus lose against React for v0.

## Mobile: Expo

Expo / React Native is the default mobile target.

- EAS Build and Update remove most native build and release friction.
- OTA updates fit Lazuli's regenerate-and-deliver loop better than store-only
  releases.
- React Native shares the React mental model with the web target, so generated
  views, typed clients, and interaction patterns can share shape even when
  components differ.
- The ecosystem and iteration speed are stronger for Lazuli than Flutter's
  Dart-based stack.

Mobile and web surfaces must stay separate in the DSL. A `surface web admin`
and a `surface mobile sales` can share product intent, but they must not be
forced into identical UI abstractions. A web `Table` may become a mobile
`FlatList`; a web `SidePanel` may become a navigation stack. The DSL declares
intent, and each surface adapter decides the native interaction shape.

Flutter is the second choice because it has consistent cross-platform UI and a
codegen-friendly language, but it loses the shared React model, the React web
ecosystem, and the current LLM advantage.

## Workflow Runtime

Durable workflow systems such as Temporal, Inngest, or Restate are adapter
decisions, not language decisions. Lazuli should be able to add one later for
retries, durability, and observability without changing the DSL or replacing
Go as the backend target.

## Guardrail

Do not reopen these choices for v0 based only on elegance. Lazuli optimizes for
generated code that developers can read, debug, and safely modify around. In an
AI-first framework, LLM corpus strength, boring idioms, and ecosystem gravity
are part of the architecture.
