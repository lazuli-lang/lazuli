# ADR-0001: Handler home and portability tiers

**Status:** accepted (2026-05-16)
**Supersedes:** the implicit decision in commit [`32fd8be`][32fd8be]
(2026-05-15) that placed user-authored Go handlers inside `dist/go/`.

## Context

When Lazuli emits Go code for a feature `f`, it produces files under
`dist/go/<f>/`. User-authored handlers — the Go functions that
implement `@fn.X`, `@hook.X`, and `@validator.X` references from the
`.lzi` — also need to live somewhere. The question is *where*.

The framework went through two iterations:

### First iteration: `app/features/<f>/handlers/<name>.go`

Original convention had handlers in a sub-package: each feature owned
`app/features/<f>/handlers/` as a separate Go package (`handlers`), and
the generated `dist/go/<f>/command.gen.go` imported it via
`import "myapp/app/features/<f>/handlers"` to wire
`Effect: lazuli.Returns(handlers.Login)`.

This broke when handler signatures referenced types the codegen owns.
A handler implementing `func(*lazuli.Ctx, LoginResultInput) (string, error)`
needs `LoginResultInput`, which lives in `dist/go/<f>/command.gen.go`
under `package <f>`. So `handlers/` had to import the parent package.
But the parent package imported `handlers/` to wire the effect. Circular
import. Go refused to compile.

### Second iteration: `dist/go/<f>/<name>.go` (the pivot)

Commit [`32fd8be`][32fd8be] resolved the cycle by collapsing both sides
into the same Go package. Handlers moved to `dist/go/<f>/<name>.go` with
`package <f>`, alongside the `*.gen.go` files. The cycle disappeared
because there was no longer a second package to import.

The pivot took "Option A" out of four considered (B: lift shared types
to a third sub-package; C: duplicate types; D: move handlers out of
`dist/` entirely). A was chosen for being the simplest change that
shipped `go build → exit 0` on the hostpoint port.

### What the second iteration cost

A regression that didn't show up in the original review: **`dist/`
stopped being disposable.** With user handlers living next to generated
files, `rm -rf dist/go && lazuli generate` would lose user code. The
`.gen.go` filename suffix became the *only* marker separating gen from
user; a glob mistake, a gitignore mistake, or an aggressive cleanup
script would wipe handlers.

The hostpoint port absorbed this trade silently because it was the
only consumer. Reviewing it after the dust settled (~10 days of
production-shape work, 25+ WAR-* gaps closed), the cost became
visible:

- New `dist/go/<f>/` files committed to git mixed gen and user code
  with no folder-level distinction — `git diff` and `git log` had to
  filter by extension.
- The "regen overwrites" rule had to grow exceptions per file pattern.
- IDE tooling couldn't treat `dist/` as a generated tree.
- TypeScript side (where `dist/ts-<frontend>/` is purely gen and user
  code lives in `apps/<frontend>/src/`) demonstrated by contrast that
  the *clean* layout works fine — the Go side was the anomaly.
- New contributors reading the canonical docs (CLAUDE.md,
  project-structure.md) found `handlers/` referenced; reading the
  actual hostpoint dist found handlers in `dist/go/<f>/`. The docs
  hadn't been updated because the change felt local; in practice it
  changed the project-wide invariant.

The decision wasn't wrong in 2026-05-15 — it was the right call to
unblock the port. It was wrong to leave standing once the port
stabilised.

## Decision

**Restore the "dist is disposable" invariant.** Go handlers move back
out of `dist/go/`. The new canonical location is
`app/features/<f>/<name>.go`, with the file living in the same
directory as the feature's `.lzi`.

The decision is anchored in a meta-principle that this ADR also
formalises and that [`project-structure.md`](../project-structure.md)
elaborates:

> Each thing has ONE obvious canonical location, defined by
> `(technological layer × consumer cardinality)`. The `.lzi` is the
> conceptual anchor; physical location follows the geometry of
> consumption.

This produces three durability tiers (Portable / Client-specific /
Disposable). Go handlers are Portable — they're cited by the compiler,
verified by doctor, and conceptually survive a stack swap because the
`.lzi` is the spec a reimplementation would follow. Their home is
`app/features/<f>/` for the same reason `.lzi` lives there.

## Mechanics

The import cycle that drove the first pivot is real and needs an
answer that doesn't sacrifice the invariant.

**Resolution: invert the gen → user direction via runtime registry.**

- `dist/go/<f>/*.gen.go` declares `package <f>gen` (was `package <f>`).
  Code generation contracts (input types, command literals, query
  literals) live here. Disposable.
- `app/features/<f>/<name>.go` declares `package <f>`. User handlers
  live here. Portable.
- Handler signatures import generated types: `func Login(ctx *lazuli.Ctx,
  input <f>gen.LoginResultInput) (string, error)`. The import direction
  is **user → gen**.
- Generated `register.gen.go` does NOT import user handlers directly.
  Instead, user handlers register themselves at `init()`:
  `lazuli.RegisterFn("login", Login)`. The generated `Effect:` slot
  resolves the handler by name at runtime, not import time.
- The runtime registry is the same mechanism already used for
  `lazuli.RegisterBindingFn` (`@fn.X(args)` in declarative effects),
  shipped under WAR-VOCAB-CREATES-FN-CALL-01. Extending it to general
  handler resolution is incremental, not new.

This breaks the cycle by construction: gen never imports user code; user
imports gen for types. No circular dependency is possible.

The trade-off is honest: compile-time "handler implemented" check
becomes a runtime panic if a handler is missing from the registry. This
loss is recovered at the boundary by `lazuli doctor` (the existing
`hook_target_001` rule), which verifies that every `@fn.X` cited in
`.lzi` has a matching file on disk before the build runs. The contract
is enforced statically; the wiring is dynamic.

## Consequences

### Positive

- `dist/` is once again safe to `rm -rf`. The "regen is the only way
  user-edited code enters dist" rule has no exceptions.
- Go layout becomes symmetric with TypeScript: source in `app/` (and
  `apps/<frontend>/` for client-specific UI), generated in `dist/`.
- `git diff` on a feature change shows user code separately from
  generated churn. Hot paths to review are obvious.
- Three durability tiers can be stated as a project-wide rule, not as
  a TypeScript-only fact. Future stacks (Rust runtime, RN/Expo target,
  whatever comes next) inherit the same layout pressure.
- New contributors reading `project-structure.md` get a consistent
  answer for every category. No "Go is special" footnote.

### Negative

- Compile-time check "handler implementation exists" downgrades to a
  doctor rule + runtime panic. Mitigated by the existing doctor
  enforcement, but it's a regression from the strict static check the
  second iteration offered.
- Existing projects (hostpoint, future early adopters) need a one-time
  migration: move `dist/go/<f>/<name>.go` → `app/features/<f>/<name>.go`
  and add `lazuli.RegisterFn` boilerplate at init. The migration is
  mechanical but it's real work — not a behind-the-scenes change.
- The runtime registry grows another responsibility (general handler
  resolution beyond binding-fn callbacks). This is incremental on
  existing infrastructure, not new architecture, but it does mean one
  more thing the runtime owns.

### Neutral

- The maturity tags introduced in `project-structure.md`
  (`[stable]`/`[partial]`/`[planned]`) become load-bearing: the canon
  describes the destination, and contributors check the tag to know
  where the implementation stands. This is overhead but pays back
  every time a contributor reads the doc.
- `Lazurite.toml` gets one new responsibility per project: declaring
  that the project follows the post-pivot layout. (Open question: is a
  schema version bump enough, or do we need an explicit flag? Resolved
  during implementation.)

## Alternatives considered

### A. Keep handlers in `dist/go/<f>/<name>.go` (status quo)

The simplest answer is to leave the pivot in place and document around
it. Rejected because it requires permanently rewriting the
"`dist/` is disposable" invariant — one of the most basic
expectations a Go developer brings to a generated tree. The cost of
that rewrite (every doc, every tooling assumption, every IDE config)
exceeds the cost of restoring the invariant.

### B. Handlers in `app/features/<f>/handlers/<name>.go` (the
   pre-pivot layout, plus a shared-types sub-package)

Restores the sub-folder organization and breaks the import cycle by
lifting shared types to a third package `dist/go/<f>/types/` (package
`<f>types`). Both gen and handlers import types/.

Rejected because three packages per feature is more architecture than
the problem needs. Every emitter has to learn the types/ split.
Cross-feature type references add a layer of indirection. The benefit
(sub-folder organization) doesn't justify the complexity — putting
handlers next to the `.lzi` is just as discoverable and uses one
fewer package.

### C. Handlers in `app/go/<f>/<name>.go`, no `dist/go/`

Drop `dist/go/` entirely. Codegen writes both `*.gen.go` and starter
stubs into `app/go/<f>/`. User and generated files share a folder but
the folder is under `app/` (versioned, not disposable).

Rejected because it creates a third source-of-truth directory (`app/`
+ `app/features/` + `app/go/`) and rejects the "dist is the canonical
output directory" convention that the web ecosystem (Vite, esbuild,
tsc) settled on years ago. Lazuli inherits enough novelty already;
fighting the `dist/` convention has no payoff.

### D. The chosen path: handlers in `app/features/<f>/<name>.go`, gen
   in `dist/go/<f>/` with package `<f>gen`, registry-mediated wiring

Accepted. Lowest conceptual overhead (one folder per tier per feature),
preserves all three invariants ("dist is disposable", "compiler-cited
files are Lazuli territory", "one canonical location per concern"),
and reuses an existing runtime mechanism for the cycle-breaking
inversion.

## Implementation

This ADR sets the direction; materialisation happens in a single PR
(per the "no window between gen emitting one place and doctor checking
another" rule).

The PR touches:

1. **Codegen Go** (`crates/lazuli_codegen_go/src/emitter/`)
   - `handlers.rs::handler_path` emits stubs at
     `app/features/<f>/<name>.go` (was `<f>/<name>.go` relative to
     `dist/go/`).
   - `*.gen.go` files declare `package <f>gen` (was `package <f>`).
   - `command.gen.go` emits `Effect: lazuli.RegistryHandler("<name>")`
     (or equivalent) — resolves the handler at runtime via registry
     lookup instead of direct import.
   - `register.gen.go` no longer imports `app/features/<f>/`.

2. **CLI** (`crates/lazuli_cli/src/cmd_generate_handler.rs`)
   - `lazuli generate handler <feature>.<fn>` writes to
     `app/features/<f>/<fn>.go` (was
     `app/features/<f>/handlers/<fn>.go`).

3. **Doctor** (`crates/lazuli_doctor/src/correctness/hook_target_001.rs`)
   - Handler lookup expects `app/features/<f>/<name>.go` (was
     `app/features/<f>/handlers/<name>.go`).

4. **Runtime** (`runtime/go/lazuli/`)
   - `RegisterFn(name string, handler any)` general handler registry
     (extends the binding-fn pattern from WAR-VOCAB-CREATES-FN-CALL-01).
   - `Effect: lazuli.RegistryHandler("<name>")` resolves via registry
     at command dispatch.

5. **Lazurite scaffold** (`lazurite/templates/default/`)
   - `CLAUDE.md.tmpl` + `AGENTS.md.tmpl` reflect the new location for
     `@fn.X` handler file paths.
   - Scaffold emits `apps/<frontend>/src/features/<f>/` skeleton per
     feature in `.lzi` (elevates the suggested convention to enforced).
   - Mobile scaffold writer (`scaffold_frontend_mobile`) emits to
     `apps/<frontend>/` (was `frontends/<target>/`).

6. **Consumer migration** (hostpoint, then any other early adopters)
   - Move `dist/go/<f>/<name>.go` → `app/features/<f>/<name>.go`.
   - Add `lazuli.RegisterFn("<name>", <Name>)` at init().
   - Delete the now-empty `dist/go/<f>/<name>.go` from version control.

The PR ships items 1–5 together to avoid a window where one tool emits
to a new path while another tool still expects the old one. Item 6 is
the consumer-side cleanup, runs after the PR lands.

## References

- Commit [`32fd8be`][32fd8be] (2026-05-15): the original pivot this
  decision supersedes. The commit message documents the trade
  considered at the time and the four options weighed.
- [`docs/project-structure.md`](../project-structure.md): the
  canonical layout document this ADR formalises and references. Read
  alongside this ADR for the operational rules; this ADR explains the
  *why*, the doc gives the *what*.
- WAR-VOCAB-CREATES-FN-CALL-01 (closed 2026-05-16): the
  binding-fn registry shipped in commit [`e7073ff`][e7073ff] —
  precedent for the registry-mediated handler wiring described in the
  Mechanics section.
- Doctor rule
  [`hook_target_001`](../../crates/lazuli_doctor/src/correctness/hook_target_001.rs):
  the static-check side of the dynamic-wiring trade-off.

[32fd8be]: https://github.com/lazuli-lang/lazuli/commit/32fd8bef221117df073ddf46e0476d3c9e6301f7
[e7073ff]: https://github.com/lazuli-lang/lazuli/commit/e7073ff
