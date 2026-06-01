---
id: 0023
title: Plugin scaffolder — `lazuli plugin new <name> --kind <semantic|adapter>`
type: techspec
track: ship/tell (plugin platform)
depends_on: [0021]
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_cli plugin_new && cargo test --workspace"
agent: unassigned
---

# TechSpec — Plugin scaffolder

## Approach

Add a nested `plugin` subcommand to the (today flat) `Commands` enum, with a
`new` verb that **walks one of two embedded template trees** (`semantic/`,
`adapter/`), substitutes a small fixed token set, and writes the files out —
mirroring the existing project scaffolder (`commands::new::scaffold`, which
walks an `include_dir::Dir`, substitutes `{{app_name}}`/`{{module}}`, strips
`.tmpl`). No code generation; static templates derived 1:1 from the two
shipped reference plugins (`lazuli-plugin-scalars-br`,
`lazuli-plugin-mercadopago`). The oracle is `lazuli plugin verify <out>`
(0022) passing on the freshly scaffolded dir with **zero edits**.

## Surface

**Create:**
- `crates/lazuli_cli/src/commands/plugin/mod.rs` — `plugin_command(cmd:
  PluginCommand) -> Result<()>` dispatcher + `plugin_new_command(name:
  &str, kind: PluginKind, namespace: Option<String>, out: Option<&Path>) ->
  Result<()>`. The scaffold logic: resolve `out` (default `./<name>`),
  reject an existing non-empty dir, build the substitution map, walk the
  chosen template tree, write each file.
- `crates/lazuli_cli/src/commands/plugin/templates.rs` — the two embedded
  template trees (either `include_dir!("…/templates/plugin/semantic")` +
  `…/adapter`, OR a literal-string template module mirroring
  `commands::new::scaffold`'s `*_template(...)` fns). Plus
  `substitute(contents, &tokens) -> String` and the token-map builder.
- `crates/lazuli_cli/templates/plugin/semantic/**` and
  `crates/lazuli_cli/templates/plugin/adapter/**` — the `.tmpl` source files
  (if using `include_dir`). File set per "Scaffold file manifest" below.
- `crates/lazuli_cli/tests/plugin_new.rs` — the scaffold→verify-green tests
  (TDD below).

**Modify:**
- `crates/lazuli_cli/src/cli_args/mod.rs` — add `Plugin { #[command(subcommand)]
  command: PluginCommand }` to `Commands`; define `PluginCommand` enum (with
  `New { name, kind, namespace, out }`) and the `PluginKind` value-enum
  (`Semantic` default, `Adapter`). **Coordinate with 0022**: if 0022's
  `Verify` variant already exists, append `New` to the existing
  `PluginCommand`; if 0023 lands first, define `PluginCommand` with `New`
  only and a doc-comment placeholder for 0022's `Verify`.
- `crates/lazuli_cli/src/cli_run.rs` — add the `Commands::Plugin { command }
  => commands::plugin::plugin_command(command)` dispatch arm (mirror the
  `Commands::New { .. } => commands::new::new_command(..)` arm at lines
  86–102).
- `crates/lazuli_cli/src/commands/mod.rs` — add `pub mod plugin;`.
- `docs/plugin-authoring.md` — prepend a **"Quickstart: `lazuli plugin new`"**
  section as the entry path; demote the manual file-layout prose to a
  "Reference: what the scaffold emits" section.

## Contracts

**`lazuli plugin new <name> --kind <semantic|adapter> [--namespace <ns>] [--out <dir>]`**
- `name` (positional, required): kebab-case plugin name. Validated; non-kebab
  input → anchored error (`plugin name must be kebab-case (lowercase
  a–z, 0–9, '-'): got '<name>'`).
- `--kind` (default `semantic`): closed value-enum `{semantic, adapter}`.
  `capability` / `design` are **not** accepted (non-goal) — clap rejects
  unknown values with the closed catalog in `--help`.
- `--namespace` (optional): defaults to `@lazuli/plugin-<name>`.
- `--out` (optional): defaults to `./<name>`. If the target exists and is
  non-empty → bail (`refusing to scaffold into non-empty directory
  <out>`); creating into an empty/absent dir is fine.

**Substitution tokens (frozen):** `{{name}}`, `{{go_package}}` (`<name>`
separators stripped + lowercased), `{{module}}` (`lazuli.dev/plugin/<name>`),
`{{namespace}}`, `{{TypeName}}` (PascalCase of `<name>`'s last segment),
`{{date}}` (scaffold date). Casing via `crate::casing::{pascal_case,
to_kebab_case, to_snake_case}` (already used by `commands::new`).

**Scaffold file manifest — `--kind semantic`** (mirrors `lazuli-plugin-scalars-br`,
minus the TS/npm surface):

| File | Source-of-shape | Notes |
|---|---|---|
| `manifest.toml` | scalars-br `manifest.toml` | `[plugin]` (name/namespace/go_module/ts_package/description) + **one** `[[semantic_types]]` example (`name = "{{TypeName}}Value"`, `alias = "@semantic.{{TypeName}}Value"`, `carrier_type = "String"`, `validator = "Validate{{TypeName}}Value"`, `formatter = "Format{{TypeName}}Value"`, `error_code`, `message_key`) |
| `{{name}}.go` | scalars-br `cpf.go` | `package {{go_package}}`, `Err…` sentinel, `Validate{{TypeName}}Value(s string) error` + `Format{{TypeName}}Value(raw string) (string, error)` — a minimal **non-empty-string** validator (so it compiles + the example test passes) |
| `{{name}}_test.go` | scalars-br `cpf_test.go` | table valid/invalid + format test, asserting `errors.Is(err, Err…)` |
| `go.mod` | scalars-br `go.mod` | `module lazuli.dev/plugin/{{name}}` + `go 1.26.0` (zero deps) |
| `README.md` | scalars-br `README.md` | surface table (1 row) + install + quickstart |
| `CHANGELOG.md` | scalars-br `CHANGELOG.md` | Keep-a-Changelog `[0.1.0] - {{date}}` |
| `LICENSE` | scalars-br `LICENSE` | MIT, year/holder substituted or left generic |
| `.gitignore` | scalars-br `.gitignore` | Go ignores |
| `.github/workflows/go.yml` | scalars-br workflow | `go test ./...` CI (runs in the plugin's own CI, not the Rust test) |

**Scaffold file manifest — `--kind adapter`** (mirrors `lazuli-plugin-mercadopago`):

| File | Source-of-shape | Notes |
|---|---|---|
| `manifest.toml` | mercadopago `manifest.toml` | top-level legacy `version`/`status`/`maintainer`/`implements = ["{{TypeName}}.SomeInterface"]` + `[plugin]` (name/namespace/go_module/description) + `[env]` (`required = []`, `optional = []`) |
| `adapter.go` | mercadopago `adapter.go` | `package {{go_package}}`, `const AdapterRef = "{{namespace}}"`, `ErrUnimplemented`/`ErrUnconfigured` sentinels, `Adapter` struct, **`var _ Interface = (*Adapter)(nil)`** compile-time assertion (against a placeholder `Interface` defined in the same file so it compiles standalone, with a `// TODO: replace Interface with the lazuli.dev/runtime interface you implement` note), `func init() { /* lazuli.RegisterAdapter(AdapterRef, &Adapter{}) */ }`, one method stub returning `ErrUnimplemented` |
| `adapter_test.go` | mercadopago `adapter_test.go` | one test asserting the stub returns `ErrUnimplemented` (no httptest harness — that's vendor-specific) |
| `go.mod` | mercadopago `go.mod` | `module {{module}}` (adapter default `{{module}}` = `lazuli.dev/plugin/{{name}}` unless `--module` given) + `go 1.26.0`, **no vendor deps** (the real mercadopago deps are SDK-specific; the skeleton stays dependency-free so it compiles offline) |
| `README.md` | mercadopago `README.md` | adapter surface + `[env]` + wiring snippet |
| `CHANGELOG.md` | mercadopago `CHANGELOG.md` | Keep-a-Changelog `[0.1.0] - {{date}}` |
| `LICENSE` / `.gitignore` / `.github/workflows/go.yml` | mercadopago | same as semantic |

> The placeholder `Interface` (adapter) keeps the skeleton **self-contained
> and compilable with zero deps**; the author swaps it for the real
> `lazuli.dev/runtime/lazuli/<domain>.<Interface>` and uncomments the
> `RegisterAdapter` line. This is the one deliberate stub-for-compilability
> choice — documented in the emitted file and in plugin-authoring.md.

**Verify-green invariant (the oracle):** for both kinds, immediately after
scaffold, `lazuli plugin verify <out>` (0022) returns OK. The test calls
0022's verify entry point (`commands::plugin::verify_*` or shells the built
binary) — see Decision 3 in the ADR.

## Plan — for the executing agent

1. Read `crates/lazuli_cli/src/commands/new/mod.rs` + `new/scaffold.rs` (the
   template-walk + substitution pattern to mirror), `cli_args/mod.rs`
   (`Commands` enum + `Commands::New` shape), `cli_run.rs` (the `New`
   dispatch arm, 86–102), `commands/mod.rs` (module list). Read 0022's spec
   (or, if 0022 is implemented, its `commands::plugin::verify` entry) to bind
   the oracle and coordinate the `PluginCommand` enum. Read 0021's typed
   manifest schema to ground the emitted `manifest.toml`.
2. Re-read the reference shapes you're mirroring:
   `C:\Users\lucas\dev\lazuli-plugin-scalars-br\{manifest.toml,cpf.go,cpf_test.go,go.mod,README.md,CHANGELOG.md}`
   and `…\lazuli-plugin-mercadopago\{manifest.toml,adapter.go,adapter_test.go,go.mod}`.
3. Add `PluginCommand` / `PluginKind` + the `Commands::Plugin` arm to
   `cli_args/mod.rs`; add the dispatch arm to `cli_run.rs`; `pub mod plugin;`
   in `commands/mod.rs`.
4. Create the two template trees (semantic + adapter) with the file manifests
   above. Keep them dependency-free so the emitted Go compiles offline.
5. Implement `plugin_new_command`: validate name (kebab), resolve `out`,
   reject non-empty dir, build token map, walk template tree, write files
   (reuse `write_scaffold_file`-style helper + `crate::casing`).
6. Write `tests/plugin_new.rs` (TDD — write FIRST), see below. Tune the
   templates until `verify` is green for both kinds.
7. Run the gate (below).
8. TEACH: rewrite the head of `docs/plugin-authoring.md` — "Quickstart:
   `lazuli plugin new`" first; manual layout demoted to reference.
9. PILOT proof: scaffold a fresh plugin into a temp dir and `lazuli plugin
   verify` it green (both kinds). Capture the command + output.
10. Commit framework-only on the loop branch.

## Tests first (TDD)

- [ ] `plugin_new_semantic_verifies_green` — `plugin new foo --kind semantic
      --out <tmp>` → emitted dir passes `lazuli plugin verify` with **zero
      edits**.
- [ ] `plugin_new_adapter_verifies_green` — same for `--kind adapter`;
      additionally assert `adapter.go` contains the literal
      `var _ Interface = (*Adapter)(nil)`.
- [ ] `plugin_new_emits_expected_file_manifest` — for each kind, assert the
      exact file set (table above) exists and each is non-empty
      (regression-locks the scaffold output).
- [ ] `plugin_new_manifest_parses_typed_schema` — the emitted `manifest.toml`
      parses + round-trips against the **0021 typed schema** (no
      unknown/missing required keys); semantic has exactly one
      `[[semantic_types]]`, adapter has `[plugin]` + `[env]` + `implements`.
- [ ] `plugin_new_default_kind_is_semantic` — `plugin new foo` (no `--kind`)
      scaffolds the semantic shape.
- [ ] `plugin_new_namespace_default_and_override` — default namespace is
      `@lazuli/plugin-foo`; `--namespace @x/y` is substituted into
      `manifest.toml`.
- [ ] `plugin_new_rejects_non_kebab_name` — `plugin new Foo_Bar` → anchored
      kebab-case error, nothing written.
- [ ] `plugin_new_refuses_non_empty_out` — `--out` at a non-empty dir →
      bail, no partial write.
- [ ] `plugin_new_go_compiles_when_toolchain_present` — **conditional**: if
      `go` is on `PATH`, run `go vet ./...` (or `go test ./...`) in the
      scaffold dir and assert success; else skip with a logged note (Go is
      not guaranteed in the Rust test env — ADR Decision 4).

## Gate

### Definition of Done (Lazuli Plugin Platform gate)
1. BUILD: implemented; **`cargo test --workspace` green (FULL sweep, not per-crate)**.
2. PILOT: scaffold a fresh plugin (`lazuli plugin new <name> --kind <k>`) into
   a temp dir and `lazuli plugin verify` it **green, zero edits** — for BOTH
   kinds (live proof, command + output captured).
3. TEACH: `docs/plugin-authoring.md` leads with "Quickstart: `lazuli plugin
   new`" as the authoring entry path; manual layout demoted to reference.
4. ENFORCE: the `plugin_new_*_verifies_green` + `plugin_new_emits_expected_file_manifest`
   + `plugin_new_manifest_parses_typed_schema` tests prevent regression and
   lock the scaffold↔verify coupling.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_cli plugin_new` (all TDD above) +
   `cargo test --workspace` 0 failures + `cargo build --workspace` clean. If
   a new diagnostic code is added (it should NOT be — scaffold/name errors are
   anyhow errors, not doctor codes), register it in facets + bridge. Run
   `cargo test -p lazuli_keywords` to confirm no keyword/registry drift
   (none expected — pure CLI surface).
2. **PILOT** — `lazuli plugin new pilot-semantic --kind semantic --out <tmp>`
   then `lazuli plugin verify <tmp>` = green (0 findings); same for
   `--kind adapter`. Report both command transcripts.
3. **TEACH** — plugin-authoring.md Quickstart section landed; manual prose
   demoted.
4. **ENFORCE** — scaffold→verify-green + file-manifest + typed-manifest tests
   green; they couple the scaffolder to 0022's verify and to 0021's schema so
   neither can drift silently.

## Risks & rollback

- **`PluginCommand` enum collision with 0022.** Both 0021-dependent specs
  add variants to the same `plugin` namespace → mitigation: the ADR fixes the
  protocol (first-to-land defines `PluginCommand`, the other appends its
  variant). They are parallel_safe; the merge is a one-line enum-variant add.
  If 0022 is unwritten at execution time, 0023 defines `PluginCommand` with
  `New` only.
- **The emitted Go doesn't compile** (e.g. adapter `var _ Interface =
  (*Adapter)(nil)` references a missing interface) → mitigation: the adapter
  template defines a *placeholder* `Interface` in the same file so the
  skeleton compiles standalone with zero deps; the
  `plugin_new_go_compiles_when_toolchain_present` test catches breakage where
  Go is available; verify-green is the in-process guard everywhere.
- **Verify rejects the scaffold** (template drifts from 0022/0021 rules) →
  mitigation: this is the *point* of binding the test to verify — the
  scaffold→verify-green tests go red, the templates get tuned. The coupling
  is the safety, not a risk.
- **Go toolchain absent in CI makes a Go gate flaky** → mitigation: Go
  compile is conditional (skip-with-note when `go` not on `PATH`); the hard
  in-process gates are file-shape + manifest-typed-validity + verify-green.
- **Scaffold partially writes then errors** (non-empty-dir or name-validation
  late) → mitigation: validate name + reject non-empty `out` **before** any
  write; `plugin_new_rejects_non_kebab_name` / `plugin_new_refuses_non_empty_out`
  assert nothing is written on the error path.

**Rollback:** `git revert` — the change is one new command module + two
template trees + a clap enum variant + a dispatch arm + a doc rewrite. Absent
it, behavior is exactly today's (no `lazuli plugin new`; authoring stays
doc + ops-pipeline). No pilot file is committed; the PILOT proof writes only
to a temp dir.
