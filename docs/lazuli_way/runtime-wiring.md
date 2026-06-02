# Runtime wiring — portable, no absolute paths

The Lazuli Go runtime ships as a single module, `lazuli.dev/runtime`. It is
**never published to a Go module proxy**: developers edit it in-tree against the
same lazuli checkout that built the CLI. A generated pilot therefore resolves the
runtime through a LOCAL `replace`/`use` directive that points at that checkout.

The one rule: **that path must be relative, never absolute.** A committed
absolute path like `C:/Users/lucas/lazuli/runtime/go` exists only on the author's
machine — a second developer or CI clone has no such directory, and `go build`
fails on the first compile. (This was pauta gap BT-01.)

## The contract

1. Clone `lazuli` near your pilot. The canonical layout is **lazuli as a sibling
   of the pilot**:

   ```
   ~/lazuli            ← the framework + runtime checkout
   ~/my-pilot          ← your project
   ```

2. Declare the runtime location with a **relative** `[lazuli] path` in
   `Lazurite.toml`, pointing at the lazuli source ROOT (not `runtime/go` — codegen
   appends that):

   ```toml
   [lazuli]
   runtime = "0.1.0"
   path = "../lazuli"      # sibling layout
   ```

   **The depth is relative to YOUR project root — compute it, do not copy
   `../lazuli` blindly.** If your pilot sits one level deeper than lazuli's parent
   (e.g. `~/dev/my-pilot` while lazuli is at `~/lazuli`), the correct value is
   `../../lazuli`:

   ```toml
   [lazuli]
   runtime = "0.1.0"
   path = "../../lazuli"   # nested layout (project two levels under the shared ancestor)
   ```

3. Run `lazuli generate go .`. Codegen computes the project-root-relative path with
   a real relativizer and emits:

   - `dist/go/go.mod` → `replace lazuli.dev/runtime => <relative>/runtime/go`
     (relative to `dist/go/`, so a sibling layout yields `../../../lazuli/runtime/go`).
   - `go.work` → `use <relative>/runtime/go` (relative to the project root, e.g.
     `../lazuli/runtime/go`).

   `dist/go/go.mod` is fully codegen-owned and overwritten on every run. The
   root `go.work` is preserve-merged (your hand-added `use` entries survive), and
   any stale standalone `replace lazuli.dev/runtime => ...` line — including a
   hand-pasted absolute one — is **stripped and re-derived** on every regen, so a
   regenerated file is always portable with no hand editing.

4. A fresh clone of the pilot + a sibling lazuli checkout now builds with
   `cd dist/go && go build ./...` and zero path editing.

## CI / non-standard layouts — `LAZULI_RUNTIME_PATH`

When `[lazuli] path` is absent (or for a layout that has no relative bridge, e.g.
a different Windows drive), export `LAZULI_RUNTIME_PATH` pointing at the
`runtime/go` directory. `lazuli generate go .` consults it as the build-time
fallback and **relativizes** it at emit time — it never bakes the absolute env
value into the committed artifact. If the env-resolved path can only be expressed
absolutely, codegen emits **nothing** and prints a fix hint rather than committing
a non-portable path.

## The guard — `RUNTIME-WIRING-ABSOLUTE-PATH-001`

`lazuli doctor` scans the project's committed `go.mod` and `go.work` for an
absolute `replace lazuli.dev/runtime => <abs>` (single-line or block form) or an
absolute go.work `use <abs>/runtime/go`. It is a **Correctness** finding (error
severity) and therefore **blocks the generate gate** — a committed absolute path
is a concrete build break, not a style nit.

To fix a firing: set a relative `[lazuli] path` (or `LAZULI_RUNTIME_PATH`) and
`lazuli generate go .` to re-emit. If you genuinely must keep an absolute path
(no known legitimate case), suppress it per-site with
`@doctor.allow(RUNTIME-WIRING-ABSOLUTE-PATH-001, reason: "...")` — a reason is
required because the rule blocks.

## Non-goals

- Publishing/vendoring the runtime as a versioned proxy module (would freeze the
  runtime and break the dogfooding loop).
- Frontend/TS runtime wiring — out of scope here; the `[lazuli] path` field is
  also consumed by the TS/vite alias emission, but the portability rule above
  governs the Go `go.mod`/`go.work` artifacts.
