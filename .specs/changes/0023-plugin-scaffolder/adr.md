---
id: 0023
title: Plugin scaffolder — decisions (templates not codegen; nested `plugin` subcommand; verify-green oracle)
type: adr
track: ship/tell (plugin platform)
depends_on: [0021]
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_cli plugin_new && cargo test --workspace"
agent: unassigned
---

# ADR — Plugin scaffolder

## Context

0021 typed the plugin manifest; 0022 added `lazuli plugin verify`. Plugins
are now loadable, typed, and checkable — but there is no in-compiler way to
*create* one. The only authoring artifacts are a 583-line doc and an
out-of-repo ops pipeline. We are adding `lazuli plugin new` so authoring is a
plain, offline, deterministic CLI subcommand. The two real reference plugins
(`lazuli-plugin-scalars-br`, `lazuli-plugin-mercadopago`) define the exact
file shapes the scaffolder must emit.

## Decision 1 — Templates with `<name>` substitution, NOT a code generator

The scaffolder is **static template files plus string substitution**, the
same machinery `lazuli new` (project scaffolder) already uses
(`commands::new::scaffold` walks an `include_dir::Dir`, substitutes
`{{app_name}}` / `{{module}}`, strips `.tmpl`). We mirror that exactly:
embed a `semantic/` and an `adapter/` template tree, walk each, substitute a
small fixed token set, write out.

- **Why:** The seam is "no front door", not "no smart generator". A dumb,
  inspectable, deterministic emitter is the Pareto fix. A code generator
  (IR introspection, "scaffold from existing `@semantic.X` usages") is a
  different, larger spec with its own failure modes. Minimal/dumb wins.
- **Rejected:** generating Go from the IR; templating engine with logic
  (handlebars/tera conditionals). We have exactly two kinds — a `match
  kind { Semantic => .., Adapter => .. }` over two template trees is simpler
  than any engine.

### Substitution token set (frozen, minimal)

| Token | Source | Example (`my-scalars`, semantic) |
|---|---|---|
| `{{name}}` | `<name>` arg, kebab-validated | `my-scalars` |
| `{{go_package}}` | `<name>` with separators stripped, lowercased | `myscalars` |
| `{{module}}` | `lazuli.dev/plugin/<name>` (semantic) or `--module`/derived (adapter) | `lazuli.dev/plugin/my-scalars` |
| `{{namespace}}` | `--namespace` or default `@lazuli/plugin-<name>` | `@lazuli/plugin-my-scalars` |
| `{{TypeName}}` | PascalCase of `<name>`'s last segment, used for the example semantic type / adapter struct context | `MyScalars` |
| `{{date}}` | scaffold date (CHANGELOG `[0.1.0] - <date>`) | `2026-06-01` |

PascalCase / kebab / package-name derivation reuse the existing
`crate::casing` helpers (`pascal_case`, `to_kebab_case`, `to_snake_case`)
already used by `commands::new`.

## Decision 2 — A nested `plugin` subcommand (`Commands::Plugin { subcommand }`)

Today `Commands` (in `cli_args/mod.rs`) is **flat** — every variant is a
top-level verb. `plugin` is a *noun namespace* with verbs under it (`new`
now; `verify` from 0022; future `publish`). We introduce one nested
`#[command(subcommand)]`:

```rust
// cli_args/mod.rs
Plugin {
    #[command(subcommand)]
    command: PluginCommand,
},

#[derive(Debug, clap::Subcommand)]
pub(crate) enum PluginCommand {
    /// Scaffold a new plugin skeleton that passes `lazuli plugin verify`.
    New {
        /// Plugin name (kebab-case), e.g. `scalars-br`.
        name: String,
        /// Plugin kind. Closed catalog: `semantic` (default), `adapter`.
        #[arg(long, value_enum, default_value_t = PluginKind::Semantic)]
        kind: PluginKind,
        /// Namespace override; defaults to `@lazuli/plugin-<name>`.
        #[arg(long)]
        namespace: Option<String>,
        /// Output directory; defaults to `./<name>`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    // `Verify { .. }` is owned by 0022 — 0023 only adds `New`. If 0022
    // landed first, append `New` to the existing `PluginCommand`; if 0023
    // lands first, define `PluginCommand` here and 0022 appends `Verify`.
}
```

- **Why nested:** `lazuli plugin new` and `lazuli plugin verify` (0022) are
  the same noun. A flat `Commands::PluginNew` would split the namespace and
  fight 0022. One `Plugin` arm with a `PluginCommand` enum is the clap-idiomatic
  shape and keeps `plugin *` cohesive.
- **Coordination with 0022:** `PluginCommand` is the shared seam. Whichever of
  {0021-derived 0022, 0023} lands first *defines* `PluginCommand`; the other
  *appends its variant*. Both depend on 0021; they are otherwise
  parallel-safe. (If 0022 is not yet written, 0023 defines `PluginCommand`
  with `New` only and a doc-comment placeholder noting `Verify` is 0022's.)

## Decision 3 — The acceptance oracle is `lazuli plugin verify` green, zero edits

The scaffold is "correct" iff `lazuli plugin verify <out>` (0022) passes on
the freshly emitted directory with no human edits. We do **not** invent a
separate correctness check for the scaffolder — we lean entirely on 0022's
verify as the oracle.

- **Why:** Verify already encodes "what a valid plugin is" (manifest typed
  per 0021, required files present, assertions in place). Re-deriving those
  rules in the scaffolder's test would drift from verify. Binding the
  scaffold test to verify means the scaffold *cannot* emit something verify
  rejects without the test going red.
- **Consequence:** 0023's `plugin_new` test calls the same verify entry
  point 0022 exposes (a `verify_plugin(dir) -> Result<Report>`-shaped
  function, or shells the built binary). The scaffold templates are tuned
  until verify is green — that is the entire tuning loop.

## Decision 4 — Go `go test` / `go build` is NOT a hard gate in the Rust test

The Rust test environment is not guaranteed to have a Go toolchain. So the
`plugin_new` Rust test asserts:

1. **File-shape**: the expected file manifest exists (see below), each
   non-empty.
2. **Manifest validity**: the emitted `manifest.toml` parses against the
   **0021 typed schema** and round-trips (the strongest in-process check).
3. **Verify-green**: `lazuli plugin verify <out>` returns OK (Decision 3).
4. **Conditional Go**: *if* `go` is on `PATH`, additionally run `go vet` /
   `go test ./...` in the scaffold dir and assert success; *else* skip with a
   logged note. The emitted plugin also carries its own
   `.github/workflows/go.yml` (mirrored from scalars-br) so `go test` runs
   for real in the plugin's CI.

- **Why:** Hard-gating `go test` in the workspace test would make
  `cargo test --workspace` flaky/red on machines without Go. The file-shape +
  manifest-validity + verify-green trio is a strong in-process guarantee; the
  Go compile is belt-and-suspenders where the toolchain exists.
- **Rejected:** vendoring a Go toolchain into CI just for this test
  (over-heavy); skipping Go entirely (loses the "skeleton compiles"
  guarantee where it's cheap to check).

## Decision 5 — Mirror the real reference file shapes exactly

The two template trees are **derived from the two shipped reference plugins**,
not invented:

- **semantic** ⇐ `lazuli-plugin-scalars-br`: `manifest.toml` ([plugin] +
  one `[[semantic_types]]`), `<name>.go` (one `Validate<Type>` /
  `Format<Type>` pair + `Err…` sentinel), `<name>_test.go` (table
  valid/invalid + format test, same shape as `cpf_test.go`), `go.mod`
  (`module lazuli.dev/plugin/<name>` + `go 1.26.0`), `README.md`
  (surface table + install + quickstart), `CHANGELOG.md` (Keep-a-Changelog
  `[0.1.0]`), `LICENSE`, `.gitignore`, `.github/workflows/go.yml`.
- **adapter** ⇐ `lazuli-plugin-mercadopago`: `manifest.toml` (legacy
  scalars `version`/`status`/`implements` + `[plugin]` + `[env]`),
  `adapter.go` (`Adapter` struct, `var _ Interface = (*Adapter)(nil)`,
  `init()` → `lazuli.RegisterAdapter`, method stubs returning
  `ErrUnimplemented`), `adapter_test.go`, `go.mod`, `README.md`,
  `CHANGELOG.md`, `LICENSE`, `.gitignore`, `.github/workflows/go.yml`.

We **omit** the TS/npm surface (`package.json`, `tsconfig.json`, `dist/`,
`pnpm-lock.yaml`, `node_modules/`) that scalars-br carries — that is a
publishing concern (non-goal) and not needed for `lazuli plugin verify`. The
scaffold emits the **Go + manifest + docs** core only.

## Alternatives considered

- **Leave authoring as doc + ops pipeline** — rejected: that *is* the seam.
  No offline front door, no agent parity.
- **Ship only the `semantic` kind** — rejected: Hostpoint depends on both a
  semantic plugin (scalars-br) and an adapter (mercadopago). Both reference
  shapes exist; cutting adapter would leave half the canonical surface
  un-scaffoldable.
- **Interactive prompts (`lazuli plugin new` wizard)** — rejected: breaks
  agent-first non-interactive use; everything must be flags.
- **Emit and then auto-run `go mod tidy` / `git init`** — deferred: keep the
  scaffold pure-write + deterministic. (The project scaffolder's
  `run_go_mod_tidy` / `run_git_init` exist and could layer on later behind a
  flag, but are not in 0023's Pareto cut.)

## Consequences

- One new template tree pair embedded via `include_dir` (or a small literal
  template module mirroring `commands::new::scaffold`'s pattern).
- `plugin-authoring.md` is re-anchored: Quickstart command first, manual
  layout demoted to reference.
- The scaffold test is permanently coupled to 0022's verify — a *feature*:
  verify and scaffold cannot drift apart.
- Capability/design kinds remain a clean future extension (add a template
  tree + a `PluginKind` variant).
