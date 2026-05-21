# Lazuli Documentation

This is the framework canon for Lazuli. 30 markdown files, ~10k lines, organized by audience and depth.

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

## Conventions used in this doc set

- **Backticks** for keywords, file paths, identifiers, and code: `feature`, `docs/quickref.md`, `@semantic.Email`.
- **`MUST` / `SHOULD` / `MAY` / `reserved`** follow standard spec meaning. `MUST` is required for canonical v0; `SHOULD` is expected unless a feature has a stated reason to diverge.
- **Closed catalogs** (every `@namespace.*`) — the spec lists every valid member. New members enter only via an architect-graded proposal.
- **Operational archive** is mentioned in some docs when referring to internal proposals / audits / roadmaps. Those documents are not part of the public framework canon.

## What's deliberately not here

- Per-pilot product state, internal roadmaps, sprint checklists, proposal drafts — all in the operational archive.
- Code style for hand-authored Go / TS runtime files — see `runtime/go/lazuli/CLAUDE.md` (if present) and the wire-thin principle in [`CLAUDE.md`](../CLAUDE.md).
- Lazurite distro template internals — see [`lazurite/templates/default/`](../lazurite/templates/default/) directly.
