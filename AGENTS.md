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

Every design proposal (the operational proposal archive) goes through grading against the AI-first rubric in [`docs/grading-rubric.md`](docs/grading-rubric.md) before commit.

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
knowledge/<sector>/       # Curated knowledge vault (committed source-of-truth)
  NNNN-<slug>.md          #   tier/cites/revalidate_by frontmatter; VOCAB-KNOWLEDGE-* checked
Lazurite.toml             # Workspace manifest (distros use distro-named TOML)
```

### Generated (gitignored unless committed deliberately)

```
dist/go/                  # Generated Go (regen-only)
dist/ts-<frontend>/       # Generated TS SDK per frontend (audience-scoped)
.lazuli/                  # Internal cache (graph, source-map, manifest,
                          #   + derived knowledge INDEX regenerated from
                          #   knowledge/ — the index lives here, the source docs do not)
```

**Convention rules:**
- Filenames inside `handlers/`, `domain/`, etc. **must match** the DSL reference. `@fn.verify_password` → `handlers/verify_password.go` with `func VerifyPassword(...)`. Doctor enforces.
- `.tmpl` files in scaffold templates use `{{app_name}}` / `{{module}}` placeholders; codegen uses Go `text/template` `{{.Field}}` syntax for runtime templates.
- `dist/` is never user-edited. Regen overwrites; do not commit edits.

See: [`docs/project-structure.md`](docs/project-structure.md), the `lazurite-scaffold` proposal (operational archive) §3 + §3.3.

---

## Lazuli vs Lazurite vocabulary

- **Lazuli** = the framework. Language (`.lzi`/`.lzx`) + IR + compiler (Rust crates in `crates/`) + Go runtime lib (`runtime/go/lazuli/`) + CLI (`lazuli` binary).
- **Lazurite** = the opinionated distribution on top of Lazuli. Folder conventions + `Lazurite.toml` manifest + `lazuli new` template body. **One distro currently shipped** but the design space supports others.

A future distro (Lazonyx for ERP, Lazpipe for automation, etc.) **cannot add language mechanisms**. New `@-namespace`, new `kind` keyword, new escape-hatch → must enter Lazuli language first, then distros adopt. Same rule that prevents Nuxt modules from extending the Vue compiler.

See: [`docs/architecture.md`](docs/architecture.md) §"Lazuli vs Lazurite", the `lazurite-scaffold` proposal (operational archive) §3.3.

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

8. **A keyword has many faces — change them together.** Adding, renaming, removing, or aliasing any `.lzi`/`.lzx`/manifest keyword, sub-keyword, closed-value catalog, or `@`-sigil is *not done* when the parser accepts it. It is done when the parser, IR lowering, LSP (completion + hover + typo catalogs), VS Code syntax highlighting, docs (grammar + quickref + canonical-semantics), the Lazurite scaffold, and the canonical `examples/` **all agree**. Partial updates drift silently — see the "Language-surface parity" section below for the surface map + definition-of-done.

When you spot a violation: reject in line. Do not merge into a checklist for "later." The boundary is enforced through deletion, not migration.

---

## Language-surface parity — every keyword has many faces

A keyword the *parser* accepts is not "shipped." It is shipped when every surface an author or AI touches agrees on it. The 2026-05 `attach_ctx` incident is the cautionary tale: the parser accepted `attach_ctx`, but it was invisible (no highlight, no LSP completion, absent from `grammar.lzi.md`) while the *dead* feature-level `context "@…"` form was highlighted and documented — and the canonical `examples/full-capsule/` used `context`, which the parser silently dropped (`context_path` never populated). Freshly-scaffolded pilots *looked* wrong while being correct; the blessed example was actually broken. (Both forms were since retired entirely in favour of the co-located `<feature>.ctx.md` convention; the parser now hard-errors `E-ATTACH-CTX-RETIRED` / `E-CONTEXT-RETIRED`.)

**This is now enforced by construction — do NOT hand-sync surfaces.** The single source of truth is the keyword registry:

- **Registry:** `crates/lazuli_keywords` (`ALL`) — one `CapabilitySpec` per keyword/construct. Proven complete against the parser by `crates/lazuli_keywords/tests/proven_complete.rs`. **Any add / rename / retire starts here.**
- **Generated, NEVER hand-edited** (the registry is the only edit point):
  - `editors/vscode/syntaxes/lazuli.tmLanguage.json` ← `cargo run -p xtask -- gen-tmlanguage`
  - `docs/keyword-reference.md` (exhaustive — one row per registry entry) ← `cargo run -p xtask -- gen-keyword-reference`
  - `docs/closed-catalogs.md` (reference namespaces / scalars / semantic scalars / aliases) ← `cargo run -p xtask -- gen-catalog-reference`
- **Gates that must stay green:**
  - `crates/lazuli_lsp/tests/keyword_surface_parity.rs` — iterates the *registry* (not a curated sample) and asserts every keyword is in the LSP catalog + `tmLanguage.json` + `keyword-reference.md`, and that retired forms (`RETIRED_FEATURE_KEYWORDS`) are absent from the feature catalog.
  - `tools/xtask/tests/keyword_reference_fresh.rs` + `catalog_reference_fresh.rs` — the generated reference + catalog docs are in sync with the registry/catalogs.
  - `crates/lazuli_analyzer/tests/catalog_resolver_parity.rs` — the scalar/semantic catalog matches the analyzer's type resolver.

**The faces a generator cannot reach — you still own these by hand:**

| Face | Where | Note |
|---|---|---|
| Recognition (parser) | `crates/lazuli_syntax/src/parser/lzi/…` (feature dispatch in `feature_walker/skeleton.rs`); manifest keys in `crates/lazuli_cli/src/app_manifest/manifest.rs` | registry mirrors this; keep them in lockstep (`proven_complete.rs` checks it) |
| Lowering (IR) | `crates/lazuli_analyzer/src/feature.rs`, `crates/lazuli_ir/` | parses ≠ reaches codegen. Verify with `lazuli inspect` / `generate … --check`, **not** `lazuli parse` (which emits the lossy skeleton AST — analyzer-only blocks like `extensions` show empty there) |
| LSP completion / hover / typo catalogs | `crates/lazuli_lsp/src/keywords.rs`, hover tables, `…/canonical_kinds/sections/{blocks,statements}.rs` | the parity gate fails if a registry keyword is missing here |
| Curated grammar + teaching docs | `docs/grammar.lzi.md`, `grammar.app.md`, `quickref.md`, `canonical-semantics.md`, `invariants.md` | these stay *curated* (EBNF + worked examples for the constructs that matter); exhaustiveness lives in the generated `keyword-reference.md` |
| Scaffold + canon | `lazurite/templates/default/**`, `examples/**` (esp. `full-capsule/`) | every new project & cold-read inherits these — `scaffold_from_template_smoke_tree_matches_expected` now runs `lazuli doctor` on the scaffold to catch drift |
| Migration recipe (rename / retire only) | `lazuli upgrade` recipes in `crates/lazuli_cli` | so existing pilots auto-migrate off the old spelling |

**Definition of done** for a keyword change:
1. Edit the registry (`crates/lazuli_keywords`) **and** the parser; `cargo test -p lazuli_keywords` (proves parser↔registry parity).
2. Regenerate: `cargo run -p xtask -- gen-tmlanguage && cargo run -p xtask -- gen-keyword-reference`. Never hand-edit the generated files.
3. Surface it in the LSP catalog and lower it in the analyzer; add curated `grammar`/`quickref` entries for non-trivial constructs.
4. Green gates: `cargo test -p lazuli_lsp --test keyword_surface_parity` and `cargo test -p xtask`.
5. Regenerate the VS Code grammar snapshots (`cd editors/vscode && npx vscode-tmgrammar-snap -g ./syntaxes/lazuli.tmLanguage.json "./tests/grammar/**/*.lzi" "./tests/grammar/**/*.lzx"`).
6. Round-trip the canonical example (`lazuli inspect examples/full-capsule`) and a fresh scaffold (`lazuli doctor` on `lazuli new`).

If you cannot do it whole, do not change the keyword — file it. The bar is parity, not "the parser accepts it."

### Closed value-catalogs are single-sourced too (not just keywords)

The reference-namespace, scalar, and semantic catalogs — "is `@foo.bar` a valid
namespace?", "is `Email` a known type?" — live ONCE in `lazuli_keywords`
(`REFERENCE_NAMESPACES` / `SCALAR_TYPES` / `SEMANTIC_TYPES` / `SCALAR_ALIASES`).
The LSP (`vocab.rs`), doctor (`refs.rs`), and analyzer (`types.rs`) **derive**
from them — no second hand-maintained copy. This collapsed the historical drift
where the LSP allowed 23 namespaces, the doctor 18, and three docs published
8 / 17 / 23. Add a namespace/scalar/semantic in `lazuli_keywords` only; the
consumers + `docs/closed-catalogs.md` follow. Non-canonical scalar aliases
(`Int`→`Integer`, …) are flagged by `VOCAB-SCALAR-ALIAS-001` (LSP), not silently
resolved.

### Docs cannot silently rot — three tiers

1. **Generated** — `closed-catalogs.md` + `keyword-reference.md` render from the
   source and are freshness-gated.
2. **Verified** — `cargo test -p lazuli_cli --test docs_hygiene` asserts every
   `path/file.ext` citation and inter-doc link in a maintained doc resolves (a
   moved file or deleted doc fails CI). `docs/proposals/*` (archived) and
   `runtime/*` (unbuilt) are exempt.
3. **Reviewed** — `cargo run -p xtask -- docs-staleness` flags any doc whose
   cited source changed after the doc was last touched. Self-maintaining (git is
   the truth); run periodically, not per-build.

When you change code a doc cites, the gates tell you. See `docs/README.md`
"Staying current."

### CLI surface — published vs framework-dev

The published `lazuli` binary carries only app-developer commands. Framework-dev
commands (`parse`, `spike-generate`, `examples`, `doctor --self`) are
`#[command(hide = true)]` and slated to move to a non-published `lazuli-dev`
binary (`specs/cement-r1/SPEC-20.md`). Do not add framework-introspection
commands to the published surface. `lazuli upgrade` IS published — it migrates a
user's project to a new framework version.

---

## Rustdoc conventions — Rails ActiveRecord style

The framework's own Rust source follows Rails-style documentation rather than Spring/.NET template ceremony. This is enforced by `INTERNAL-UNDOC-PUB-001` and `INTERNAL-NO-EXAMPLE-001` rules under the `tdd-iron-hand` preset (workspace-root `Lazurite.toml`).

Three conventions:

1. **PROSE-HEAVY, not template.** Lead with WHY, not WHAT. Sub-headers emerge from content (`## Severity`, `## See also`, `## Custom Types`) — not from a fixed template. Skip `## Arguments` / `## Returns` / `## Errors` unless the signature doesn't carry the info (in Rust it usually does). Restating the signature in prose is bloat.

2. **`## Examples` MANDATORY** for pub items with non-trivial use. Show progressive complexity (simple → realistic → edge). Must compile via `cargo test --doc` — enforced by `INTERNAL-NO-EXAMPLE-001` once a crate reaches W5 sweep completion. Use `# use lazuli_ir::*;` lines to hide setup; show only the meaningful invocation.

3. **Cross-ref liberally** via `` [`Type`] `` / `` [`module::fn`] ``. rust-analyzer renders these as clickable hover. Point to: related fns in the same module, design proposals (`docs/proposals/...`), invariants in `docs/invariants.md`. Cross-references turn each hover into a mini-page.

**Canonical model**: [`crates/lazuli_doctor/src/test_discipline/mod.rs:1-40`](crates/lazuli_doctor/src/test_discipline/mod.rs) already exemplifies the style — prose paragraph, variant bullet list, cross-refs. Treat it as the template when writing module-level docs.

**Anti-patterns** (reject in line):
- Spring-template restating Args/Returns when the signature carries it (`pub fn foo(x: u32) -> bool` followed by `# Arguments\n* x — the input number\n# Returns\n* bool — whether it worked` is pure ceremony; delete)
- Docstrings shorter than 1 sentence on pub items (rule will fire — at minimum a single-line summary in imperative voice)
- Example blocks marked `` ``` `` without a language (won't compile via `cargo test --doc` — must be `` ```rust `` or `` ```no_run `` with annotated reasoning)
- LLM-generated bulk that restates the function name in prose ("validates the validate_x input by validating x")

**Why Rails-style and not Spring**: Rust's type signature carries the bulk of "what the function does" — duplicating it in `# Arguments` blocks creates noise. Rails (Ruby is dynamic) needed the explicit `[+name+]` brackets because the signature was opaque; Rust does not. Lazuli is the AI-native Rails; the docs follow the same philosophy.

---

## Error handling discipline — iron-hand 4th dimension

The framework enforces `error_handling` as a category in `[doctor.error_handling]`, mirroring `coverage` / `test_discipline` / `internal_hygiene`. Rules span three layers Lazuli owns:

**Framework Rust** (`crates/lazuli_*/src/`, fires under `lazuli doctor --self`):
- `INTERNAL-PANIC-UNWRAP-001` — `.unwrap()` / `.expect()` / `panic!` / `todo!` / `unimplemented!` / `unreachable!` in non-test code. Test-context detection via `#[cfg(test)] mod tests` depth tracking. False positives in raw-string fixtures are suppressed with `severity_override` carrying `reason`.
- `INTERNAL-ERROR-NAMING-001` — types deriving `thiserror::Error` should end in `Error` (e.g. `ParseError`, not `ParseFail`).
- `INTERNAL-ERROR-NON-EXHAUSTIVE-001` — pub error enums deriving `thiserror::Error` need `#[non_exhaustive]` for SemVer safety.
- `INTERNAL-ERROR-VARIANT-DOC-001` — each variant of a pub error enum needs `///` doc OR `#[error("...")]` attr (either silences; both is best).

**Go handlers** (user app's `features/<f>/{handlers,domain,jobs,integrations}/`, fires via the standard `lazuli doctor <path>` flow once the W7 aggregator-wiring lands):
- `HANDLER-NO-PANIC-001` — literal `panic(...)` in non-test `.go` files.
- `HANDLER-NO-STRING-ERROR-001` — `errors.New(...)` inside function bodies (sentinel package-level `var Err... = errors.New(...)` is allowed). Also flags `fmt.Errorf` with no `%w`/`%v` (pure string formatting).
- `HANDLER-ERROR-WRAP-001` — `fmt.Errorf("... %v ...", err)` should use `%w` to preserve the error chain.

**`.lzi` contract + `.lzx` UX** — planned (`ERROR-DECLARED-EXHAUSTIVE-001`, `ERROR-MESSAGE-KEY-001`, `ERROR-HTTP-STATUS-MAP-001`, `ERROR-RETRIABLE-CLASS-001`, `ERROR-AUDIT-EMIT-001`, `ERROR-VIEW-ON-ERROR-001`, `ERROR-VIEW-EMPTY-STATE-001`, `ERROR-FIELD-VALIDATION-001`). These need IR-aware walkers; reserved for a follow-up wave. Code prefix `ERROR-*` is already wired into `RuleCategory::ErrorHandling`.

**Posture**: `[doctor.error_handling]` is currently `tdd-strict` (warn-only) at workspace root, not `tdd-iron-hand`. The initial scan surfaces ~1250 panic-prone constructs in the framework that pre-date the rule — most are provably safe (`.unwrap()` after `.is_some()`) or load-bearing diagnostics (`.expect("invariant")`). Promote to `tdd-iron-hand` after a W7 sweep round reduces the count to ~0.

**Out of scope**: emitted `dist/go/` Go (covered by emitter test suites) and frontend `.tsx`/`.ts` in arbitrary slots (surface too narrow for a generic lint rule).

---

## Rails-style source layout — every `.rs` ≤ 500 LOC

The framework's own Rust source follows Rails ActiveRecord layout, not just Rails docstring style. Every refactor between 2026-05-15 and 2026-05-26 enforced this; new work inherits the discipline.

**The ceiling**: every `.rs` file under `crates/` ≤ 500 LOC. Production AND test code. No exceptions. Final audit @ `f61e2704`: 1354 `.rs` files, 0 above 500, max-LOC = 500 (`emitter/handlers/tests.rs`, exactly at the budget boundary).

**The canonical shape**:

```
<concern>.rs                      # one file, ≤ 500 LOC, or →
<concern>/
  mod.rs                          # thin module root: re-exports + shared types + orchestrator only
  <sub_concern_a>.rs              # each sibling ≤ 500 LOC, named for the concern it owns
  <sub_concern_b>.rs
  <sub_concern_c>/                # homonym subdirs are fine when a sibling itself needs splitting
    mod.rs
    ...
  <sub_concern>_tests.rs          # inline test sibling — see "Inline-test rule" below
```

`mod.rs` is a re-exporter, not a kitchen sink. If a file is growing past ~300 LOC and you've already pulled the obvious helpers, that's the signal to split, not to keep packing.

**ABI strictly additive when splitting**: never delete or rename a `pub` / `pub(crate)` / `pub(super)` / `pub(in crate::xxx)` symbol. When you move an item to a sibling file, restore visibility at the parent via `pub use sibling::Item;`. Downstream consumers (the downstream LSP extension, codegen, doctor dispatch tables) must keep resolving every original path. The `cargo public-api --diff` invariant from R4.2 still holds: zero public removal across any refactor commit.

**Inline-test rule**: tests stay co-located with the production code they exercise. When a single `#[cfg(test)] mod tests { ... }` block alone exceeds 500 LOC, split it into sibling `*_tests.rs` files where each sibling is a coherent sub-concern (not a numeric chunk). The canonical pattern, which preserves raw-string indents byte-for-byte:

```rust
#[cfg(test)] mod foo_tests { include!("tests/foo_tests.rs"); }
#[cfg(test)] mod bar_tests { include!("tests/bar_tests.rs"); }
```

The sibling file's content becomes the module body verbatim. **Never dedent or re-indent raw string fixture content** — `r#"..."#` blocks carry structural whitespace that the parser depends on. Use Read + write tools only; do not pipe content through any reformatter. Multiple parser fixture corruptions during R3 traced to dedenting `mod tests {` wrappers.

**Integration test pattern** (`crates/<crate>/tests/*.rs`): when a single integration test file exceeds 500 LOC, convert `tests/foo.rs` → `tests/foo/main.rs` + sibling helper modules declared via `mod helpers;` from `main.rs`. Cargo auto-discovers `tests/foo/main.rs` as a single test binary; the siblings are local modules of that binary (not separately-discovered integration tests). No `Cargo.toml [[test]]` entries needed unless you're overriding a non-default name.

**Shared test fixtures** belong in a `test_support.rs` sibling at `pub(super)` visibility — never duplicate fixture builders across sibling test files.

**`//!` module headers** are required on every `.rs` file under `crates/lazuli_doctor/src/correctness/` and `crates/lazuli_doctor/src/vocab/` (enforced by `tests/module_headers.rs`). The header must describe severity and include one trigger cue (`fixture`, `example`, `fires when`, `warns when`). `*_tests.rs` and `tests.rs` siblings are skipped by the linter; they don't need headers.

---

## Git discipline for refactor work

The rules below emerged from concrete incidents during R3-R10 — every one of them mitigates a specific class of bug observed in this repo. They are non-negotiable when refactoring.

- **`git add <specific files>` ONLY.** Never `git add -A`, never `git add .`. Parallel agents share a `.git/index`; broad-stage commands sweep siblings' staged work into the wrong commit. R9 spent multiple cleanup commits recovering from this exact failure mode.
- **Before every `git commit`** during a refactor, run `git diff --cached --name-only` and verify every staged path is in your scope. If unrelated paths appear, `git reset HEAD <them>` before committing.
- **Never `git reset --hard`** to escape an obstacle. It deletes commits and uncommitted work. Investigate the root cause and fix forward in a new commit.
- **Never `git rebase`** (in any form) on shared branches. **Never `--amend`** to fix a hook failure — create a new commit. Pre-commit hook failures mean the commit did NOT happen, so `--amend` modifies the PREVIOUS commit and silently rewrites history.
- **Never `git stash pop`** during refactor work — re-apply work via a new commit so the diff is reviewable.
- **No `--force` push** ever. No `--no-verify`. No `--no-gpg-sign`. If a hook fails, fix the underlying issue.
- **One commit per extracted concern.** Commit messages: `<crate>/<area>: extract <concern> into <new-file>` (lowercase, present-tense imperative). This makes `git log` itself a readable refactor narrative.
- **Workspace green every commit.** After each commit, run `cargo build --workspace` and the relevant per-crate test suite (`cargo test -p <crate> --lib` + `--tests` when integration tests changed). If anything fails, fix forward in a new commit; never amend.
- **Sequential > parallel agents on a shared worktree.** When two agents both run `git add` + `git commit` near-simultaneously, the shared index causes cross-agent staging contamination — commit messages stop matching content, deletions from one agent's scope leak into another agent's commit. Either dispatch one agent at a time, or have each agent work in its own `git worktree`. If parallelism is unavoidable, brief each agent on the pre-commit `git diff --cached --name-only` discipline and the `git reset HEAD <out-of-scope>` recovery move.
- **Branch per round, `--no-ff` merge into main.** R8 → `rails-style-r8`, R9 → `rails-style-r9`, R10 → `rails-style-r10`. The merge commit preserves topology so `git log --graph main` still tells you what each round did.

---

## When you're unsure

Ask: "could a Lazuli project still function if the Lazuli Go runtime was replaced by a hypothetical second runtime targeting Rust + Yew + Flutter?" If the answer is no because the language is leaking Go-specific or React-specific assumptions, the proposal is at the wrong layer.

---

## When in doubt

- Read [`docs/architecture.md`](docs/architecture.md) end-to-end.
- Read [`docs/invariants.md`](docs/invariants.md) for the closed grammar/IR constraints.
- Read [`docs/design-principles.md`](docs/design-principles.md) — Rule Zero ("Vocabulary Over Mechanism") is the most-cited principle in design decisions.
- Read the relevant the `<x>` proposal (operational archive) if working on the corresponding subsystem.
- Read [`docs/grading-rubric.md`](docs/grading-rubric.md) before proposing any language change.
