# Release Policy

**Status**: draft (2026-05-11). Sets the public stability contract for
Lazuli surfaces so downstream automation (auto-migrator, doctor,
`lazuli upgrade`) has a single rule book to enforce.

**Audience**: Lazuli core (language + codegen + Lazuli Go lib),
adapter authors, and any consumer of the IR JSON ABI.

## Why this exists

Under the AI-100x cost assumption (`docs/proposals/bucket-ai-debug-loop-cycle.md`),
each release that quietly breaks a surface the IA learned to author
burns cache that costs real money to rebuild. Hand-written
languages absorb this via community long-tail. Lazuli does not have
that long-tail yet — the only mitigation we control is a release
discipline that **(a) keeps breakage explicit, (b) ships an
auto-migration for every break, and (c) refuses to merge breaking
changes that lack a migration recipe**.

LTS dual-channel is **explicitly deferred**. We do not have evidence
of pilot pressure on bleeding-edge churn that justifies the
maintenance overhead of two channels. Revisit after the second
production pilot.

## Surfaces covered

| Surface | Covered | Versioning unit |
|---|---|---|
| `.lzi` / `.lzx` language (keywords, blocks, semantics) | yes | `lazuli_version` in `app.lzi` |
| IR JSON ABI (`LZIR_SCHEMA`) | yes | `crates/lazuli_ir/src/lib.rs:42` (today `0.11.0`) |
| `lazuli.Error` typed hierarchy (Go) | yes (tier-gated) | semver of `lazuli.dev/runtime` Go module |
| Public CLI verbs (`lazuli generate`, `lazuli inspect`, `lazuli doctor`, `lazuli upgrade`, `lazuli debug`, `lazuli examples`, `lazuli profile`) | yes | semver of the CLI binary |
| Adapter/plugin namespace policy (`@runtime/<name>`, `@plugin/<name>`) | yes | this document |
| `// lazuli-version`, `//lazuli:pattern <id> <version>` codegen markers | yes | per-pattern semver tracked in `crates/lazuli_codegen_go` |

Not covered: internal crate APIs across `crates/lazuli_*`, Lazuli Go
lib *internal* helpers (anything not exported), adapter
implementations themselves (each adapter declares its own policy).

## Semver rules

We follow strict semver against the **language surface**, not the
implementation crates:

- **MAJOR**: removes a keyword, changes a keyword's meaning, removes
  a typed error from the public hierarchy, removes an IR field,
  removes a CLI verb. Requires migration recipe.
- **MINOR**: adds a keyword, adds an optional block, adds an IR
  field (additive serde-default), adds a typed error, adds a CLI
  verb. **Must not** change meaning of existing surface. May ship a
  migration recipe if non-trivial.
- **PATCH**: bug fix, diagnostic wording, performance, internal
  refactor. No language surface change. No recipe.

Pre-1.0: same rules apply with semantic discipline; we just live in
the `0.x` namespace. Every breaking change still ships a recipe.

### `lazuli_version` pin tolerance

The `lazuli_version` pin in `app.lzi` is written at **MINOR
granularity** (`"0.12"`, not `"0.12.0"` and not `"0.12.3"`):

- `"0.12"` matches any `LZIR_SCHEMA` of shape `0.12.x` for any
  PATCH `x`. PATCH bumps do **not** trigger a recipe gate and do
  **not** fire `LAZULI-VERSION-001`.
- `"0.12"` does **not** match `0.13.0` or any other MINOR.
  Mismatch fires `LAZULI-VERSION-001` with the migration recipe
  path the user should run.
- `"0.12"` does **not** match `1.0.0` (MAJOR mismatch is always
  an error regardless of recipe presence).
- A three-segment pin (`"0.12.0"`) is treated as a syntax error
  and rejected by the loader — authors are not allowed to pin
  patch-level. Rationale: patch changes are by definition
  non-breaking, so binding code to a specific patch would freeze
  bug fixes the author actually wants. Doctor surfaces this as
  `LAZULI-VERSION-003` (proposal `bucket-ai-debug-loop-cycle.md`
  §5 — added in revision).
- Floating `"latest"` or unpinned absence is allowed in 0.x with
  a warning; required pin (error on absence) lands at 1.0.

## Stability tiers within a release

Each documented surface lives in exactly one tier:

| Tier | Promise | Visible marker |
|---|---|---|
| **Stable** | covered by semver above; breaking change = MAJOR | default; no marker |
| **Experimental** | may break in any MINOR; surface may be removed | `// EXPERIMENTAL: subject to change before 1.0` docstring + LSP hover prefix |
| **Internal** | not part of the surface; LSP/doctor refuse to surface it | not exposed |

**Pre-1.0 default for new surfaces**: experimental for the first
minor that introduces them, promoted to stable on second minor that
ships them unchanged or with only additive growth. This gives one
release window to find shape mistakes before paying migration cost.

**Typed error hierarchy** (`lazuli.Error` + child types) ships
**experimental** in v0 of the AI debug loop bucket. Stable promotion
gated on first pilot consuming `errors.As(err, &lazuli.FieldError{})`
in production code. The reason is asymmetric: once IA-generated
user-code does `errors.As`, every variant added later must be
additive only — no field renames, no enum tightening, no migrating
`Reason` from string to typed.

## Migration recipes

Every MAJOR (and every MINOR with breaking change to an experimental
surface) ships a **migration recipe** living under
`migrations/recipes/<from>-to-<to>/<recipe-name>/`:

```
migrations/recipes/0.11-to-0.12/
  rename-policies-to-rules/
    recipe.toml          # metadata (from-version, to-version, kind)
    input.lzi            # canonical input fixture
    output.lzi           # expected output after `lazuli upgrade`
    README.md            # one-paragraph rationale
```

CI gate: any commit that bumps `LZIR_SCHEMA` or the `app.lzi`
grammar **must** add at least one recipe directory, and the
`lazuli upgrade` smoke test must pass `input.lzi → output.lzi` for
every recipe in the from/to window. Recipes without compile + run
validation rot in silence; the gate is non-negotiable.

The `lazuli upgrade --from X --to Y <path>` verb composes recipes
in topological order, applies them to the target, and re-runs the
test fixture for each recipe to confirm the upgrade landed.

Recipes for the typed error hierarchy (Go-side breakage) live in
the same tree under `migrations/recipes/<from>-to-<to>/<recipe>/go/`
and run a `go vet` + `go test` against a small fixture module to
confirm rewrites compile.

## Doctor enforcement

- `lazuli doctor` reads `app.lzi`'s `lazuli_version "X.Y"` pin.
  Mismatch with the CLI's `LZIR_SCHEMA` → error with
  `code: LAZULI-VERSION-001`, message names the recipe path the
  user should run.
- Missing `lazuli_version` pin → warning today, error on 1.0.
- `// EXPERIMENTAL` surface authored in `.lzi` → LSP hover prefix
  warns; no doctor flag (authoring experimental is allowed; just
  visible).

## Out of scope for this document

- **Adapter compatibility windows**: each adapter pack declares its
  own minimum `lazuli.dev/runtime` version in `go.mod`. Adapter
  authors follow this policy at their discretion.
- **`.lzx` / experience surface**: same rules apply by analogy;
  detailed surface inventory not duplicated here. `lazuli_version`
  pin in `app.lzi` covers both.
- **Workspace contract** (`workspace.lzi`): same rules apply.
  Future revision may add a separate `workspace_version` pin if
  workspace surface diverges from app surface.

## Revision history

- 2026-05-11: initial draft. Triggered by
  `bucket-ai-debug-loop-cycle.md` proposal needing a stable
  reference for D10 (versioning + auto-migrator).
