# Lazuli Documentation

This is the framework canon for Lazuli, organized by audience and depth. The catalog/keyword references are *generated* from the compiler and gated for freshness; prose docs are continuously verified against the code (see [Staying current](#staying-current)) — the canon cannot silently rot.

Operational artifacts (proposals, audits, roadmaps, per-pilot state, swarm tooling) live in a separate private repo and are referenced from here as text-only mentions (`the <name> proposal (operational archive)`) so the link doesn't dangle.

## By audience

### Starting cold (you've never seen Lazuli)

| Doc | Why |
|---|---|
| [`quickstart.md`](quickstart.md) | 5-minute orientation — what Lazuli is, what it compiles to, one example |
| [`architecture.md`](architecture.md) | Why Lazuli looks the way it does — DSL ↔ IR ↔ targets, lineage from prior attempts, the wire-thin runtime principle |
| [`quickref.md`](quickref.md) | Context pack to load before authoring or reviewing `.lzi` / `.lzx`. Designed for LLM agents on a token budget |

### Authoring `.lzi` / `.lzx` (you're writing or reviewing code)

| Doc | What's there |
|---|---|
| [`canonical-semantics.md`](canonical-semantics.md) | The full normative spec — every keyword, every closed namespace, every modifier |
| [`grammar.lzi.md`](grammar.lzi.md) | `feature.lzi` grammar (the main per-feature surface) |
| [`grammar.lzx.md`](grammar.lzx.md) | `feature.lzx` grammar (surfaces / views / cells / drawers) |
| [`grammar.app.md`](grammar.app.md) | `app.lzi` grammar (env, deploy, services, urls) |
| [`grammar.workspace.md`](grammar.workspace.md) | `workspace.lzi` (optional — distributed-system root) |
| [`grammar.contract.md`](grammar.contract.md) | `contract.lzi` (external service contracts) |
| [`grammar.registry.md`](grammar.registry.md) | `registry.lzi` (integrations + plugin bindings) |
| [`style-guide.md`](style-guide.md) | Vocabulary choices, lifecycle conventions, validator placement, plugin-contributed semantics |
| [`project-structure.md`](project-structure.md) | Folder conventions — `features/<x>/`, `handlers/`, `domain/`, `queries/`, `templates/`, `i18n/` |

### Designing or implementing tooling (LSP, doctor, alternative compilers)

| Doc | What's there |
|---|---|
| [`invariants.md`](invariants.md) | The contract every tool must enforce — shorter than canonical-semantics, normative for doctor / LSP / codegen |
| [`ir-abi.md`](ir-abi.md) | Stable IR shape (JSON-serializable) consumed by codegen + MCP + audit-skill |
| [`mcp-abi.md`](mcp-abi.md) | MCP server tools — what LLM agents can read/write |
| [`error-contract.md`](error-contract.md) | Doctor diagnostic codes + severities + how they surface in CLI / LSP |
| [`diagnostics/README.md`](diagnostics/README.md) | Full diagnostics catalog — every `lazuli_doctor` rule (103 across 9 categories) indexed with severity + source anchor |
| [`generation-contract.md`](generation-contract.md) | What code generators promise (idempotent, regen-only, no manual edits) |
| [`extension-points.md`](extension-points.md) | The 5 escape hatches: `@fn` handlers, `handler "./path.go"`, `query.sql`, `extends @anchor / slot`, user `main.go` |
| [`capability-layering.md`](capability-layering.md) | When something belongs in language / compiler / runtime / plugin / adapter |
| [`lazuli-codegen-patterns.md`](lazuli-codegen-patterns.md) | How codegen wires into the Go + TS runtime libraries (wire-thin templates) |
| [`testing-strategy.md`](testing-strategy.md) | What Lazuli auto-generates tests for vs what authors must contract-test |
| [`target-stack.md`](target-stack.md) | Concrete output targets: Go server, React web, Expo mobile, TS SDK |

### Authoring a plugin or contributing

| Doc | What's there |
|---|---|
| [`plugin-authoring.md`](plugin-authoring.md) | How `@plugin/<name>` packages are structured, registered, multi-language (Go server + TS web + TS mobile) |
| [`design-principles.md`](design-principles.md) | Rule Zero ("Vocabulary Over Mechanism") + the core constraints any change must respect |
| [`design-decisions.md`](design-decisions.md) | "Why isn't this dual form an atrito?" — defensive log of intentional pattern collisions and the justifications |
| [`scope-discipline.md`](scope-discipline.md) | The 80/20 boundary — what the framework absorbs vs what stays in app code |
| [`grading-rubric.md`](grading-rubric.md) | The 10-criterion AI-first rubric used to grade design proposals |
| [`release-policy.md`](release-policy.md) | Versioning, schema bumps, migration windows, what counts as breaking |
| [`migrations.md`](migrations.md) | How version-to-version migrations work (recipes live in [`migrations/recipes/`](../migrations/recipes/)) |
| [`auth-guide.md`](auth-guide.md) | The auth model — sessions, tenants, roles, policies, MFA primitives |

## Reading orders

**LLM agent, first contact, asked to edit `.lzi`**
1. `quickref.md` (context pack — small)
2. The specific `grammar.<x>.md` for the file kind being edited
3. `canonical-semantics.md` only if the agent needs to disambiguate

**Human contributor, first contact**
1. `quickstart.md`
2. `architecture.md`
3. `design-principles.md`
4. `scope-discipline.md`
5. Browse `grammar.*.md` for the surfaces that interest you

**Tool author (writing an alternative parser / linter)**
1. `invariants.md`
2. `ir-abi.md`
3. `error-contract.md`
4. `generation-contract.md`
5. `diagnostics/README.md` (per-rule catalog — what each doctor check fires on)

**Plugin author**
1. `plugin-authoring.md`
2. `capability-layering.md`
3. `style-guide.md` (for plugin-contributed semantic types)

## Generated references (never hand-edited)

| Doc | Generated from | Freshness gate |
|---|---|---|
| [`closed-catalogs.md`](closed-catalogs.md) | `lazuli_keywords` — reference namespaces / scalars / semantic scalars / aliases | `catalog_reference_fresh` |
| [`keyword-reference.md`](keyword-reference.md) | `lazuli_keywords::ALL` — the keyword registry | `keyword_reference_fresh` |

Regenerate with `cargo run -p xtask -- gen-catalog-reference` / `gen-keyword-reference`.

## Staying current

Docs are held to the code by three tiers, so the canon cannot drift unnoticed:

1. **Generated** — the references above are rendered from the compiler; a hand-edit or a source change without a regen fails the build.
2. **Verified** — `docs_hygiene` (`cargo test -p lazuli_cli --test docs_hygiene`) asserts that every `path/file.ext[:line]` citation and every inter-doc Markdown link in a maintained doc resolves. A moved source file or a deleted doc fails CI.
3. **Reviewed** — `cargo run -p xtask -- docs-staleness` flags any doc whose cited source files changed *after* the doc was last touched. Self-maintaining: no `last_reviewed` date to remember — git is the source of truth. Run it periodically (a nightly/weekly job), not on every build.

`docs/proposals/*` are archived design snapshots (the live archive moved to the operational repo); they are frozen and exempt from the gates.

## Conventions used in this doc set

- **Backticks** for keywords, file paths, identifiers, and code: `feature`, `docs/quickref.md`, `@semantic.Email`.
- **`MUST` / `SHOULD` / `MAY` / `reserved`** follow standard spec meaning. `MUST` is required for canonical v0; `SHOULD` is expected unless a feature has a stated reason to diverge.
- **Closed catalogs** (every `@namespace.*`) — the spec lists every valid member. New members enter only via an architect-graded proposal.
- **Operational archive** is mentioned in some docs when referring to internal proposals / audits / roadmaps. Those documents are not part of the public framework canon.

## What's deliberately not here

- Per-pilot product state, internal roadmaps, sprint checklists, proposal drafts — all in the operational archive.
- Code style for hand-authored Go / TS runtime files — see `runtime/go/lazuli/CLAUDE.md` (if present) and the wire-thin principle in [`CLAUDE.md`](../CLAUDE.md).
- Lazurite distro template internals — see [`lazurite/templates/default/`](../lazurite/templates/default/) directly.
