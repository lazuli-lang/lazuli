# Release Policy

**Status**: draft (2026-05-13). Sets the public stability contract for Lazuli
surfaces so downstream automation (auto-migrator, doctor, `lazuli upgrade`)
has a single rule book to enforce.

**Audience**: Lazuli core (language + codegen + Lazuli Go lib), adapter
authors, and any consumer of the IR JSON ABI.

## Why this exists

Under the AI-100x cost assumption (see
the `bucket-ai-debug-loop-cycle` proposal (operational archive)), each release that quietly
breaks a surface the AI learned to author burns cache that costs real
money to rebuild. Hand-written languages absorb this via community
long-tail. Lazuli does not have that long-tail yet — the only mitigation we
control is a release discipline that:

1. Keeps breakage explicit (semver-tracked).
2. Ships an auto-migration recipe for every break.
3. Refuses to merge breaking changes that lack a migration recipe (CI gate).

LTS dual-channel is **explicitly deferred**. We do not have evidence of
pilot pressure on bleeding-edge churn that justifies the maintenance
overhead of two channels (separate backport policy, two doc trees, two
adapter compatibility windows). Revisit after the second production
pilot. The 80% mitigation is `lazuli_version` pin + auto-migrator; LTS is
the 20% caudal and pre-paying its cost is wasteful before evidence.

## Surfaces covered

| Surface | Covered | Versioning unit |
|---|---|---|
| `.lzi` / `.lzx` language (keywords, blocks, semantics) | yes | `lazuli_version` in `app.lzi` |
| IR JSON ABI (`LZIR_SCHEMA`) | yes | `crates/lazuli_ir/src/lib.rs` constant |
| `lazuli.Error` typed hierarchy (Go) | yes (tier-gated, see below) | semver of `lazuli.dev/runtime` Go module |
| Public CLI verbs (`lazuli generate`, `lazuli inspect`, `lazuli doctor`, `lazuli dev`, `lazuli new`, `lazuli upgrade`, `lazuli debug`, `lazuli examples`, `lazuli profile`, `lazuli migrate`, `lazuli seed`) | yes | semver of the CLI binary |
| Adapter/plugin namespace policy (`@runtime/<name>`, `@plugin/<publisher>/<name>`) | yes | this document + memory `project_plugin_namespace_policy.md` |
| `//line`, `//lazuli:pattern <id> <version>` codegen markers | yes | per-pattern semver tracked in `crates/lazuli_codegen_go` |
| `Lazurite.toml` manifest schema | yes (`[project].schema`) | integer schema in manifest |

Not covered: internal crate APIs across `crates/lazuli_*`, Lazuli Go lib
*internal* helpers (anything not exported), adapter implementations
themselves (each adapter declares its own compatibility window in its
`go.mod`).

## Semver rules

We follow strict semver against the **language surface**, not the
implementation crates:

- **MAJOR**: removes a keyword, changes a keyword's meaning, removes a
  typed error from the public hierarchy, removes an IR field, removes a
  CLI verb. Requires migration recipe.
- **MINOR**: adds a keyword, adds an optional block, adds an IR field
  (additive serde-default), adds a typed error variant, adds a CLI verb.
  **Must not** change meaning of existing surface. May ship a migration
  recipe if non-trivial.
- **PATCH**: bug fix, diagnostic wording, performance, internal refactor.
  No language surface change. No recipe.

Pre-1.0: same rules apply with semantic discipline; we just live in the
`0.x` namespace. Every breaking change still ships a recipe.

## Stability tiers within a release

Each documented surface lives in exactly one tier:

| Tier | Promise | Visible marker |
|---|---|---|
| **Stable** | covered by semver above; breaking change = MAJOR | default; no marker |
| **Experimental** | may break in any MINOR; surface may be removed | `// EXPERIMENTAL: subject to change before 1.0` docstring + LSP hover prefix |
| **Internal** | not part of the surface; LSP/doctor refuse to surface it | not exposed |

**Pre-1.0 default for new surfaces**: experimental for the first minor
that introduces them, promoted to stable on the second minor that ships
them unchanged or with only additive growth. This gives one release
window to find shape mistakes before paying migration cost.

**Typed error hierarchy** (`lazuli.Error` base + `FieldError` /
`PolicyError` / `TenantError` / `AdapterError` / `LibBugError` children)
ships **experimental** in v0 of the AI debug loop bucket. Stable
promotion gated on first pilot consuming `errors.As(err,
&lazuli.FieldError{})` in production code. The reason is asymmetric:
once IA-generated user-code does `errors.As`, every variant added later
must be additive only — no field renames, no enum tightening, no
migrating `Reason` from string to typed.

## Migration recipes

Every MAJOR (and every MINOR with breaking change to an experimental
surface) ships a **migration recipe** living under
`migrations/recipes/<from>-to-<to>/<recipe-name>/`:

```
migrations/recipes/0.11-to-0.12/
  rename-policies-to-rules/
    recipe.toml          # metadata (from-version, to-version, kind, summary)
    input.lzi            # canonical input fixture
    output.lzi           # expected output after `lazuli upgrade`
    README.md            # one-paragraph rationale
```

CI gate: any commit that bumps `LZIR_SCHEMA` or the `app.lzi`/`Lazurite.toml`
schema **must** add at least one recipe directory, and the `lazuli upgrade`
smoke test must pass `input.lzi → output.lzi` for every recipe in the from/to
window. Recipes without compile + run validation rot in silence; the gate
is non-negotiable.

The `lazuli upgrade --from X --to Y <path>` verb composes recipes in
topological order, applies them to the target, and re-runs the test
fixture for each recipe to confirm the upgrade landed.

Recipes for the typed error hierarchy (Go-side breakage) live in the
same tree under `migrations/recipes/<from>-to-<to>/<recipe>/go/` and run
a `go vet` + `go test` against a small fixture module to confirm
rewrites compile.

## Doctor enforcement

- `lazuli doctor` reads `app.lzi`'s `lazuli_version "X.Y"` pin. Mismatch
  with the CLI's `LZIR_SCHEMA` → error with code
  `LAZULI-VERSION-001`, message names the recipe path the user should
  run. When the pin is **missing** in 0.x, the warning message also
  carries the expected pin value `expected_value = <current major>.<minor>`
  so the warning is self-correcting (the IA/user can read the warning
  and write the line directly).
- Missing `lazuli_version` pin → warning in 0.x, error from 1.0.
- `// EXPERIMENTAL` surface authored in `.lzi` → LSP hover prefix warns;
  no doctor flag (authoring experimental is allowed; just visible).

## Release PR review (process gate)

CI-level recipe gates (`MIGRATION-RECIPE-001/002`) catch **absence** of
a recipe, but cannot detect "we shipped a MINOR that quietly changed
the meaning of an existing keyword". This is the politically fragile
half of release discipline.

**Process rule (binding for every release PR pre-1.0)**: each release
PR requires a `lazuli-language-architect` agent review with the
explicit question:

> Does this change the meaning, semantics, or shape of any keyword,
> block, or typed surface that existing pinned versions might rely on?

The agent's verdict is recorded as a PR comment. If the answer is
"yes" and no migration recipe is included, the PR is blocked. The
review is required regardless of patch/minor/major level — even
PATCH-claimed PRs go through it, because the question is "did you
quietly break someone" not "did you intend to".

Post-1.0 we may relax this to MINOR+ only once we have observed
zero false-negative slips in 12+ months of releases. Until then,
universal review is the contract.

## Bucket cycle wave ordering

When a bucket cycle dispatches multi-cell waves to Codex workers, cells
that touch the same shared file should be sequenced within the wave, not
parallelised. Recurring pattern from the AI Debug Loop bucket (2026-05-13):

- Wave 2 of bucket-cycle-pattern had D3, D4, D6, D7, D10 fanned out in
  parallel. D3 and D6 both touched the error-envelope contract
  (`runtime/go/lazuli/error.go` + `observability/panic.go`); ordering
  D3 before D6 within the wave avoided a rebase round.

Codex prompts that touch shared files should call this out explicitly
in the "Constraints" section so the orchestrator knows to land them in
sequence rather than full parallel.

## Stability surfacing in `inspect`

`lazuli inspect --format=json` includes a `"stability"` field on every
construct that has a public stability tier (`stable` | `experimental`).
Downstream tools (LSP, docs generator, IA debug bundle) read this field
to gate behavior — e.g., the AI examples bundle (`lazuli examples --bundle`)
refuses to include an example whose IR carries any `experimental` node.

This makes the tier programmatically observable rather than only a
docstring/hover concept; ungoverned, "experimental" decays into
"forgotten label". The IR field is additive (`Option<StabilityTier>`,
default `Stable`).

## Pattern semver (codegen-internal)

`//lazuli:pattern <id> <version>` annotations on emitted Go functions
(see `bucket-ai-debug-loop-cycle.md` §6.3) carry independent semver
from `LZIR_SCHEMA`:

- A pattern's `v2` supersedes `v1` when its emitted code shape changes
  materially: different allocation pattern, different lock discipline,
  different SQL strategy.
- Pattern version bumps are PATCH-level changes to the language surface
  (no user observable behaviour change), but `lazuli profile`'s
  attribution report groups by `(pattern_id, version)` so perf
  regressions land traceable.

## Out of scope for this document

- **Adapter compatibility windows**: each adapter pack declares its own
  minimum `lazuli.dev/runtime` version in `go.mod`. Adapter authors
  follow this policy at their discretion.
- **`.lzx` / experience surface**: same rules apply by analogy; detailed
  surface inventory not duplicated here. `lazuli_version` pin in
  `app.lzi` covers both.
- **Workspace contract** (`workspace.lzi`): same rules apply. Future
  revision may add a separate `workspace_version` pin if workspace
  surface diverges from app surface.

## Revision history

- 2026-05-13: initial draft. Triggered by
  the `bucket-ai-debug-loop-cycle` proposal (operational archive) proposal needing a
  stable reference for D10 (versioning + auto-migrator).
