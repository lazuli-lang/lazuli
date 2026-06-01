---
id: 0023
title: Plugin scaffolder — `lazuli plugin new` brings authoring in-compiler
type: prd
track: ship/tell (plugin platform)
depends_on: [0021]
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_cli plugin_new && cargo test --workspace"
agent: unassigned
---

# PRD — Plugin scaffolder (`lazuli plugin new`)

## The seam (Seam 5)

There is **no authoring surface in-compiler**. Today, a plugin author who
wants to write a Lazuli plugin has two things and neither is a command:

1. A **583-line prose doc** (`docs/plugin-authoring.md`) that describes, by
   hand, the file layout a plugin must have: `manifest.toml`, the Go
   validator/adapter files, the `var _ Interface = (*Adapter)(nil)`
   compile-time assertion, paired `_test.go`, `go.mod` with the
   `lazuli.dev/plugin/<name>` module path, `README.md`, `CHANGELOG.md`.
2. An **out-of-repo ops pipeline** (`lazuli-lang/ops/.pipely/pipelines/plugin-scaffold/`)
   that the core team runs to stamp out *official* plugins. It is not
   shipped in the compiler, requires network/ops access, and is invisible
   to anyone holding only the `lazuli` binary.

So the path from "I want to write a plugin" to "I have a skeleton that
compiles and passes `lazuli plugin verify`" is: read 583 lines, hand-copy
file shapes from an existing plugin (scalars-br / mercadopago), and hope
you matched the typed-manifest schema (0021) and the verify rules (0022).
The reference shape exists (`lazuli-plugin-scalars-br`,
`lazuli-plugin-mercadopago`) but nothing *emits* it.

**The fix:** `lazuli plugin new <name> --kind <semantic|adapter>` scaffolds
a working plugin skeleton — typed manifest (per 0021) + Go skeleton + paired
`_test.go` + `README`/`CHANGELOG`/`go.mod` — that passes `lazuli plugin
verify` (0022) **out of the box, zero edits**. Authoring becomes a command,
offline, in the binary every author already has.

## Why now / why it matters

- **Hostpoint is the canonical shape.** Hostpoint depends on `scalars-br`
  (a semantic plugin) and `mercadopago` (an adapter plugin). The next pilot
  that needs a locale scalar pack or a payment adapter should *scaffold*
  it, not archaeology-copy it. "Does Hostpoint need this?" — yes: every
  plugin Hostpoint consumes was hand-shaped, and the next one shouldn't be.
- **Agent-first parity.** An agent driving Lazuli cannot run an out-of-repo
  ops pipeline. It can run `lazuli plugin new`. The authoring entry must be
  a plain CLI subcommand with deterministic, inspectable output — no network,
  no ops credentials.
- **The platform is half-built without it.** 0019 unified resolution, 0021
  typed the manifest, 0022 added verify. Those make plugins *loadable*,
  *well-typed*, and *checkable* — but a platform with no front door (no way
  to *create* the thing) is a platform you can only consume, never produce.
  0023 is the front door.

## What ships

A new CLI subcommand:

```
lazuli plugin new <name> --kind <semantic|adapter> [--namespace @lazuli/plugin-<name>] [--out <dir>]
```

- **Default kind = `semantic`** (the common case: locale scalar packs like
  scalars-br).
- Scaffolds a directory (default `./<name>` or `--out <dir>`) containing a
  working plugin skeleton that mirrors the real reference file shapes.
- The scaffolded skeleton passes `lazuli plugin verify <dir>` (0022) with
  **zero edits** — that is the acceptance oracle.

It is **minimal/dumb on purpose**: template files with `<name>` substitution,
not a code generator. No AST, no introspection — string templates, the same
class of machinery `lazuli new` already uses for projects.

## Users & jobs

| User | Job-to-be-done | Today | With 0023 |
|---|---|---|---|
| Pilot author (Hostpoint-class) | "I need a `@semantic.X` scalar for my locale" | Copy scalars-br by hand, match 0021 schema by eye | `lazuli plugin new my-scalars --kind semantic` → edit one validator |
| Integration author | "I need a payment/`X` adapter" | Copy mercadopago, re-derive the `var _ Interface` assertion | `lazuli plugin new my-gw --kind adapter` → fill the adapter methods |
| Agent | "Produce a plugin skeleton non-interactively" | Cannot run ops pipeline | One deterministic CLI call, JSON-clean output |
| Core team | "Stamp an official plugin" | Ops pipeline | Ops pipeline still works; can layer on top of the offline scaffold |

## Scope — the two kinds (Pareto)

Only **two** `--kind` values ship, matching the two real reference plugins:

- **`semantic`** (mirrors `lazuli-plugin-scalars-br`): emits a `manifest.toml`
  with one example `[[semantic_types]]` block (carrier `String`, a
  `Validate<Name>` / `Format<Name>` pair), a Go validator file, a paired
  `_test.go`, `go.mod` (`lazuli.dev/plugin/<name>`), `README.md`,
  `CHANGELOG.md`.
- **`adapter`** (mirrors `lazuli-plugin-mercadopago`): emits a `manifest.toml`
  with `[plugin]` + an `implements` / `[binds]` / `[env]` block, a Go
  `adapter.go` carrying the `var _ Interface = (*Adapter)(nil)` compile-time
  assertion, a paired `_test.go`, `go.mod`, `README.md`, `CHANGELOG.md`.

**`capability` and `design` kinds are deferred** — noted as a non-goal, not
designed here. Two kinds cover both shipped reference plugins and both
Hostpoint dependencies; that is the Pareto cut.

## Success criteria

1. `lazuli plugin new foo --kind semantic` produces a directory that
   `lazuli plugin verify foo` passes — **green, zero edits**.
2. `lazuli plugin new foo --kind adapter` likewise verifies green, zero
   edits, and the emitted `adapter.go` carries the `var _ Interface =
   (*Adapter)(nil)` assertion.
3. The emitted `manifest.toml` is valid against the **0021 typed-manifest
   schema** (parses + round-trips, no unknown/missing required keys).
4. `docs/plugin-authoring.md` opens with a **"Quickstart: `lazuli plugin
   new`"** that is the *entry path* — the manual-layout prose is demoted to
   reference-only ("what the scaffold emits and why").
5. A `lazuli_cli` test (`plugin_new`) asserts scaffold → verify-green for
   both kinds and regression-locks the emitted file manifest.

## Non-goals

- **Publishing / registry upload.** 0023 scaffolds locally; it does not push
  to a registry, tag a release, or touch npm/Go-proxy. (Future spec.)
- **The `capability` and `design` kinds.** Deferred; only `semantic` +
  `adapter` ship.
- **Replacing the ops pipeline for official plugins.** This is the *offline
  authoring entry*. The ops pipeline (CI, signing, catalog publish) can layer
  on top of a scaffolded plugin; 0023 does not delete or supersede it.
- **A real code generator.** No IR introspection, no "scaffold from an
  existing feature's `@semantic.X` usages". Dumb templates only.
- **`go test` / `go build` as a hard CI gate inside the Rust test** — the Go
  toolchain is not guaranteed in the Rust test environment; see ADR for the
  fallback (assert file-shape + manifest validity, run `go test` only when a
  toolchain is present). The *plugin's own* CI (`.github/workflows/go.yml`,
  mirrored from scalars-br) is where `go test` runs for real.
