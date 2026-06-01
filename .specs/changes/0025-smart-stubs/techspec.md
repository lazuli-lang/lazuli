---
id: 0025
title: SMART-STUBS-001 — codegen emits the delegating runtime body, not `// IMPLEMENT ME`, when a stub's site maps to a known runtime symbol
type: techspec
track: ship/evolve (reinvention defense)
depends_on: []
parallel_safe: false   # shares emit.rs with codegen
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_codegen_go smart_stub && cargo test --workspace"
agent: unassigned
---

# TechSpec — Smart stubs (delegate-to-runtime starter bodies)

PRD + ADR + TechSpec, one document. Sibling of 0024 (`VOCAB-RUNTIME-REINVENTED-001`):
0024 is the **lint backstop** that catches a hand-written reinvention after the
fact; 0025 is the **generation-time wall** that makes the reinvention not happen
in the first place. Same philosophy as 0024's reinvention table: **one
parameterized table, O(1) growth, not per-case branching.**

---

## 1. PRD — why

### Problem (the flagship, mechanism (i))
`/c/tmp/reinvention-damage-report.json` §5 maps **all 31 confirmed reinventions**
to three preventing mechanisms. Mechanism **(i) smart stubs** alone prevents
**18 of 31** at generation time — the single highest-leverage lever in the report.

The flagship case: when a pilot declares

```lazuli
auth password / hash @fn.hash_password, verify @fn.verify_password
# with @cap.Hashed(algorithm:argon2id) on the session/credential column
```

codegen today emits a starter stub at
`app/features/<feature>/handlers/hash_password.go` whose body is:

```go
// IMPLEMENT ME
var zero lazuli.HashedRef
return zero, errors.New("hash_password not yet implemented")
```

An empty `// IMPLEMENT ME` body is an **invitation**: the agent (or human) opens
the file, sees a blank canvas, and writes argon2 by hand —
`golang.org/x/crypto/argon2`, `crypto/rand` salt, hardcoded OWASP constants,
`encodeArgon2id`, PHC parsing. That is **exactly** what happened in pauta
(`account/handlers/hash_password.go:56`, `verify_password.go:16`) and hostpoint.
The runtime **already owns** this mechanism — `auth.HashPassword(ctx, contract,
plaintext)` does argon2id + a concurrency-capped semaphore (SEC-H10) + bcrypt
legacy + PHC encode — but the empty stub never tells the author it exists.

The damage report's own words (§B1, confirmedFindings[0].fix):
> "Body → `auth.HashPassword(pw)`; … Ideally drop `@fn.hash_password` entirely
> and let the `@cap.Hashed(algorithm:argon2id)` declaration wire
> `auth.HashPassword` automatically."

### The fix
When codegen emits a stub whose **binding site** maps to a known runtime symbol,
emit the **delegating runtime call** as the default body instead of
`// IMPLEMENT ME`. A pre-filled `return auth.HashPassword(ctx, <Feature>AuthPassword, pw)`
body is a **wall the author must actively tear down to reinvent** — and there is
no reason to: it compiles, it is the OWASP-correct path, it is the documented
Lazuli way. The empty stub invites argon2-by-hand; the delegating stub
forecloses it.

### Goals
- G1. A `password.hash` / `password.verify` stub emits the delegating
  `auth.HashPassword` / `auth.VerifyPassword` body, wired to the
  `<Feature>AuthPassword` `PasswordContract` the auth emitter already emits.
- G2. **Back-compat is absolute**: a stub whose site has **no** table row emits
  today's `// IMPLEMENT ME` body **byte-for-byte unchanged**. Every non-mapped
  `@fn`/`@hook` is exactly as today.
- G3. The delegating stub stays **user territory** — same `//lazuli:pattern
  extension_stub` marker, same `func init()` + `RegisterFn`, same "Lazuli will
  not overwrite this file" header. Only the **function body** changes. It is a
  starter that compiles + works, not a sealed gen file.
- G4. One parameterized **SITE→DELEGATION** table; seeding a new family
  (session / reset / verify-email) is **one row**, no `emit.rs` control-flow
  edit.

### Non-goals / honest limits (state plainly)
- **NL1. Regenerate-only.** Smart stubs help **only on fresh scaffold or
  regenerate of a not-yet-authored handler**. `emit_handler_stubs` already skips
  any path that exists on disk (`mod.rs` `path_exists`), so an **existing
  hand-written `hash_password.go` is never touched** by this change. The 4 auth
  files already reinvented in pauta/hostpoint are fixed by the **batch work**
  (damage report B1–B4), **not** by this spec. This spec stops the **next**
  pilot from reinventing; it does not retro-fix the current ones.
- **NL2. Not the lint.** 0024 is the backstop that catches reinvention in
  hand-authored handlers regardless of how they were created. 0025 only shapes
  what codegen writes. They are complementary; neither subsumes the other.
- **NL3. Seed is the password pair only.** Session/reset/verify-email sites are
  **candidate rows** (the auth emitter exposes `auth.session.*`,
  `auth.password.reset.*`, `auth.verify.*` per the damage report's mechanism-(ii)
  bucket), but the **proven, clean auto-wire** today is the password hash/verify
  pair (contract var + a 3-arg / 4-arg runtime fn with a stable signature).
  Pareto: ship the flagship row, design the table so the rest are one row each.

---

## 2. ADR — the shape

### Decision
Introduce a `STUB_DELEGATION_TABLE: &[StubDelegation]` const in
`crates/lazuli_codegen_go/src/emitter/handlers/`. Before `emit_stub_contents`
renders the `// IMPLEMENT ME` body, it looks up the stub's `site` against the
table. On a hit, it renders the row's **delegating body** (and that row's import
set); on a miss, it renders today's body verbatim.

### Why a table (mirrors 0024)
The reinvention families are a small, named, slow-growing set. A `match` on site
strings would spread the knowledge across `emit.rs`; a table keeps it in one
place, makes "what auto-wires to the runtime?" a single readable list, and makes
the extensibility test (`table_is_extensible`) a real regression guard. This is
the same const-table philosophy 0024 chose for `REINVENTION_TABLE`.

### Table contract (this IS the deliverable shape)

```rust
/// One auto-wire: a stub whose binding SITE matches `site_suffix` delegates
/// to a known runtime symbol instead of emitting `// IMPLEMENT ME`.
///
/// `site_suffix` matches against `HandlerStub.site` with `.ends_with(...)`.
/// The site is `<feature>.auth.password.hash` (see
/// handlers/collect/feature_walks.rs:254), so the row keys on the stable
/// `.auth.password.hash` tail and is feature-agnostic.
struct StubDelegation {
    /// Suffix of `stub.site` that triggers this row, e.g. ".auth.password.hash".
    site_suffix: &'static str,
    /// Renders the function body (everything between `{` and the closing `}`,
    /// minus the shared observability prologue which emit.rs keeps). Receives
    /// the resolved binding context so it can name the contract var + input.
    render_body: fn(&DelegationCtx) -> String,
    /// Extra imports this body needs beyond the stub's base set
    /// (context/errors + gen import). For the auth rows: the runtime auth pkg.
    extra_imports: &'static [&'static str],
    /// Audit family label (parity with 0024's `family`), for the teach-doc
    /// table and any future diagnostic cross-ref.
    family: &'static str,
}

/// Resolved binding context handed to `render_body`. Built in emit.rs from the
/// `HandlerStub` + casing helpers already in the crate.
struct DelegationCtx<'a> {
    /// `pascal_case(stub.feature)` — matches the auth emitter's `feature_pascal`
    /// (emitter/auth/mod.rs:95), so the contract var name lines up exactly.
    feature_pascal: &'a str,
    /// The gen-package import alias (`<feature>gen`, casing::gen_package_name)
    /// — the PasswordContract var lives in the generated feature package, so the
    /// body references it as `<feature>gen.<Feature>AuthPassword`.
    gen_alias: &'a str,
    /// The stub's single input identifier in scope (the handler param is named
    /// `input`; for `password.hash` the plaintext IS `input` of Go type
    /// `string`).
    input_ident: &'a str,
    /// Output Go type (`qualify_generated_stub_type` result) for the `var zero`
    /// fallback the body may still need.
    output_type: &'a str,
}

const STUB_DELEGATION_TABLE: &[StubDelegation] = &[
    // ---- FLAGSHIP: password hash (proven clean auto-wire) ----
    StubDelegation {
        site_suffix: ".auth.password.hash",
        // Runtime: auth.HashPassword(ctx *lazuli.Ctx, contract auth.PasswordContract,
        //          plaintext string) (string, error)  [runtime/go/lazuli/auth/password.go:165]
        // The auth emitter already emits `var <Feature>AuthPassword =
        // auth.PasswordContract{...}` in the gen package (emitter/auth/contracts.rs:30).
        render_body: render_password_hash_body,
        extra_imports: &["lazuli.dev/runtime/lazuli/auth"],
        family: "auth.password-hash",
    },
    // ---- FLAGSHIP: password verify ----
    StubDelegation {
        site_suffix: ".auth.password.verify",
        // Runtime: auth.VerifyPassword(ctx, contract, plaintext, storedHash string) error
        //          [runtime/go/lazuli/auth/password.go:199]
        render_body: render_password_verify_body,
        extra_imports: &["lazuli.dev/runtime/lazuli/auth"],
        family: "auth.password-verify",
    },
    // ---- CANDIDATE ROWS (NOT seeded — design proof that growth is one row) ----
    // ".auth.session.*"        -> auth.MintSessionToken / auth.HashSessionToken
    //                             (damage report mechanism-(ii) bucket: login,
    //                             login_with_google session-token mint).
    // ".auth.password.reset.*" -> auth.RequestPasswordReset / auth.ConsumePasswordReset.
    // ".auth.verify.*"         -> auth.IssueVerification / auth.ConsumeVerification.
    // Each is ONE StubDelegation row when its runtime symbol + contract var
    // signature is confirmed stable. Do NOT seed them in this spec — the
    // password pair is the Pareto flagship (NL3).
];
```

### Body renderers (the exact Go each flagship row emits)

The output type bridge is real and must be handled honestly. The hash stub's
declared output type is `lazuli.HashedRef` (see tests_p1.rs:132 — the `@cap.Hashed`
column lowers the `@fn` output to `lazuli.HashedRef`), while
`auth.HashPassword` returns `(string, error)`. The delegating body therefore
wraps the runtime string into a `HashedRef`. **The executing agent MUST confirm
the exact `HashedRef` constructor** (likely `lazuli.NewHashedRef(s)` or
`lazuli.HashedRef(s)` — grep the runtime + the gen output) and pick the one that
compiles; the spec fixes the *shape*, the agent fixes the *constructor token*.

`render_password_hash_body` emits (body only; emit.rs keeps the
`if ctx.Context == nil` + `observability.StartOp` prologue exactly as today):

```go
	hashed, err := auth.HashPassword(ctx, {gen_alias}.{Feature}AuthPassword, input)
	if err != nil {
		var zero {output_type}
		return zero, err
	}
	// <runtime returns the PHC string; the @cap.Hashed column type is
	//  lazuli.HashedRef> — wrap per the confirmed constructor:
	return {hashed_ref_wrap(hashed)}, nil
```

`render_password_verify_body` emits (verify's runtime fn returns only `error`;
the stub's output type is whatever the verify `@fn` lowers to — confirm it is the
unit/bool shape and adapt):

```go
	if err := auth.VerifyPassword(ctx, {gen_alias}.{Feature}AuthPassword, input.Plaintext, input.StoredHash); err != nil {
		var zero {output_type}
		return zero, err
	}
	var zero {output_type}
	return zero, nil
```

> NOTE for the executing agent: `verify`'s input is a 2-field struct
> (plaintext + stored hash), not a bare `string`. Read the verify stub's actual
> `input_type` from `collect`/the gen contract before finalizing the field
> accessors (`input.Plaintext` / `input.StoredHash` are placeholders — use the
> generated field names). If the verify input shape is not cleanly resolvable,
> ship **only the hash row** this cycle and leave verify as a candidate row
> (the hash row alone is the proven flagship and satisfies the gate). Be
> honest in the commit which rows shipped.

### emit.rs integration (minimal, surgical)
`emit_stub_contents` (emit.rs:22) currently always renders the
`// IMPLEMENT ME` template. New control flow:

```rust
pub(super) fn emit_stub_contents(stub: &HandlerStub, module_name: &str) -> String {
    // ... existing setup (fn_name, gen_pkg, input/output types, gen_import) ...
    if let Some(rule) = STUB_DELEGATION_TABLE
        .iter()
        .find(|r| stub.site.ends_with(r.site_suffix))
    {
        return emit_delegating_stub(stub, module_name, rule, /* resolved ctx */);
    }
    // ... unchanged: today's `// IMPLEMENT ME` format!(...) block ...
}
```

`emit_delegating_stub` reuses the **same** file scaffold (package header,
doc-comment block minus the `// IMPLEMENT ME` line, `//lazuli:pattern
extension_stub`, `func init()` + `RegisterFn`) and:
- swaps the `// IMPLEMENT ME` marker line for a one-line
  `// Delegates to the Lazuli runtime — edit if you need custom behavior.`
- merges `rule.extra_imports` into the import block (so `auth` is imported, and
  the gen import is forced on because the contract var lives in the gen pkg —
  set `input_uses_gen || output_uses_gen || delegating` true so `gen_import` is
  always present for delegating bodies).
- renders the body from `rule.render_body(&ctx)` in place of the
  `var zero / return zero, errors.New(...)` pair.

Everything else (the init block, the "will not overwrite" header, the
observability prologue) is **byte-identical** to the plain stub. Factor the
shared scaffold so the two paths cannot drift (a single `format!` template with a
`{body}` + `{marker_line}` + `{extra_import_lines}` hole is the cleanest;
the agent may instead keep two `format!`s if it adds a test asserting the init
block + header are identical across both — the table extensibility test already
forces the body to be the only difference).

### Why not auto-drop the `@fn` (the report's "ideally")
The damage report suggests "ideally drop `@fn.hash_password` and let `@cap.Hashed`
auto-wire `auth.HashPassword`" — i.e. emit **no stub at all** and bind the
runtime fn directly. That is a **larger** change (the auth/`@cap.Hashed` binding
emitter would synthesize the `RegisterFn` itself, and pilots that *do* want a
custom hash lose their override point). **0025 deliberately takes the smaller,
fully-back-compatible step**: keep the stub + `@fn` (so the override point
survives), just pre-fill its body with the delegation. Auto-dropping the `@fn` is
a follow-up (note it as future work; do not do it here).

---

## 3. TechSpec — surface, plan, tests

### Surface
**Modify:**
- `crates/lazuli_codegen_go/src/emitter/handlers/emit.rs` — add
  `StubDelegation` + `DelegationCtx` + `STUB_DELEGATION_TABLE` + the two
  `render_*_body` fns + `emit_delegating_stub`; add the `.ends_with` lookup at
  the top of `emit_stub_contents`. The plain-stub `format!` block is untouched
  on the miss path.
- `crates/lazuli_codegen_go/src/emitter/handlers/tests_p1.rs` (or a new
  `tests_smart_stub.rs` sibling wired in `mod.rs` under `#[cfg(test)]`) — the
  golden tests below. Name them so `cargo test -p lazuli_codegen_go smart_stub`
  selects them.

**Create:**
- `docs/lazuli_way/delegate-to-runtime.md` — the TEACH doc (see §TEACH). This is
  the **same file 0024 stubs**; 0025 fills the "stubs delegate to the runtime by
  default" section. If 0024 already created the stub, append; do not clobber its
  header/rule reference (the 0024 diagnostic message links to this file).

**Probably NOT touched (verify):**
- No new keyword / diagnostic code — this is a pure codegen **body** change, so
  `lazuli_keywords` facets + `lazuli_diagnostics_registry` need no row. **If**
  the agent finds it must register anything (it should not), register it and run
  the facet/bridge parity tests. State "none added" in the DoD if so.
- `emitter/patterns.rs` — **reuse** `PATTERN_EXTENSION_STUB`; do **not** mint a
  new pattern id. The delegating stub is still an `extension_stub` (user
  territory, user-editable). The emitter lint already requires the
  `//lazuli:pattern extension_stub` header on both the fn and init — keep both.

### Plan — for the executing agent
1. Read `emit.rs` IN FULL (done in spec) + `handlers/mod.rs` (the `HandlerStub`
   carrier: `feature`, `namespace`, `name`, `site`, `input_type`,
   `output_type`) + `handlers/collect/feature_walks.rs:241-265` (site strings
   `auth.password.hash` / `auth.password.verify`).
2. Read `emitter/auth/contracts.rs:23-63` (`emit_password` → `var
   {feature_pascal}AuthPassword = auth.PasswordContract{...}`) and
   `emitter/auth/mod.rs:95` (`feature_pascal = pascal_case(feature.name)`) to
   confirm the **exact** contract var name the body must reference.
3. Read `runtime/go/lazuli/auth/password.go:165` + `:199` for the **exact**
   `HashPassword` / `VerifyPassword` signatures (done: 3-arg `(ctx, contract,
   plaintext) (string, error)` / 4-arg `(ctx, contract, plaintext, storedHash)
   error`).
4. Resolve the `lazuli.HashedRef` constructor: grep the runtime crate + a sample
   gen output for how a `HashedRef` is built from a `string`. Pick the token
   that compiles. (If the hash `@fn` output is plain `string` in some pilots, the
   wrap is a no-op — handle both: if `output_type == "string"`, return `hashed`
   directly.)
5. Add the table + renderers + lookup to `emit.rs`. Keep the miss path byte-for-
   byte (the existing `password_hash_stub` test in tests_p1.rs will REGRESS if a
   non-mapped stub changes — but `hash_password`'s site DOES match, so that test
   must be UPDATED to expect the delegating body; add a NEW non-mapped test for
   the unchanged path — see tests).
6. Write the golden tests (§Tests first). Run the gate.
7. Write `docs/lazuli_way/delegate-to-runtime.md`.
8. LIVE PROOF: regenerate hostpoint (or the `customer_auth` fixture) and confirm
   the freshly-emitted `hash_password.go` body delegates to `auth.HashPassword`
   (not `// IMPLEMENT ME`). Because regen **skips existing files**, do this
   against a clean out-dir / a feature with **no** existing handler file, or
   temporarily point at an empty existing-set. Report the emitted body.
9. Commit on `loop-serial`.

> WATCH-OUT (existing test will move): `tests_p1.rs` around :125-143 currently
> asserts the `hash_password` stub contains `// IMPLEMENT ME` +
> `return zero, errors.New("hash_password not yet implemented")`. After this
> change that exact stub **delegates**, so those assertions FLIP. Update that
> test to assert the delegating body, and add the separate non-mapped fixture so
> the back-compat path stays covered. Do NOT leave the old assertions — they are
> the proof the change took effect.

### Tests first (TDD) — golden codegen tests
- [ ] `smart_stub_password_hash_delegates` — a feature with
  `auth password / hash @fn.hash_password` + `@cap.Hashed(algorithm:argon2id)`
  output (the tests_p1.rs:100-121 fixture) emits a `hash_password.go` whose body
  contains `auth.HashPassword(ctx, customer_authgen.CustomerAuthAuthPassword, input)`
  and does **NOT** contain `// IMPLEMENT ME` nor `not yet implemented`. Asserts
  the `auth` import + the gen import are present.
- [ ] `smart_stub_password_verify_delegates` — companion: `verify_password.go`
  body calls `auth.VerifyPassword(...)`, no `// IMPLEMENT ME`. (Skip/relax this
  one only if the verify input shape blocks a clean wire — see ADR note; then
  document hash-only in the commit.)
- [ ] `smart_stub_keeps_extension_stub_marker_and_init` — the delegating
  `hash_password.go` STILL contains `//lazuli:pattern extension_stub v1`,
  `func init()`, and `lazuli.RegisterFn("customer_auth.hash_password", HashPassword)`,
  and the "Lazuli will not overwrite this file" header. (Proves G3: still user
  territory.)
- [ ] `non_mapped_fn_still_plain_stub` — a feature with an arbitrary
  `@fn.compute_score` whose site has NO table row emits the **unchanged**
  `// IMPLEMENT ME` + `return zero, errors.New("compute_score not yet implemented")`
  body, no `auth` import. (Proves G2: back-compat, byte-for-byte.)
- [ ] `stub_table_is_extensible` — a unit test that adds a synthetic
  `StubDelegation` row (or asserts via the lookup fn) so a new `site_suffix`
  routes to its renderer without touching `emit_stub_contents` control flow.
  (Mirrors 0024's `table_is_extensible`; proves the O(1)-growth claim.)

### TEACH — `docs/lazuli_way/delegate-to-runtime.md`
Section "**Stubs delegate to the runtime by default**":
- State the rule: when an `@fn` binds a site the runtime already owns, the
  generated stub ships the delegating call — you edit it only for *custom*
  behavior, you do **not** write argon2/sessions/reset by hand.
- Show the before (`// IMPLEMENT ME` empty body → agent writes argon2) vs after
  (`return auth.HashPassword(ctx, <Feature>AuthPassword, pw)`).
- Cross-reference 0024's `VOCAB-RUNTIME-REINVENTED-001`: "if you tear this wall
  down and re-hand-roll the runtime, doctor flags it." The two mechanisms are
  the carrot (pre-filled body) and the stick (lint).
- List the seeded sites (`.auth.password.hash`, `.auth.password.verify`) and the
  candidate sites as "coming as one-row additions" so pilots know the roadmap.

---

## 4. Gate

### Definition of Done (reinvention-defense gate)
1. **BUILD** — implemented; **`cargo test --workspace` green (FULL sweep, 0
   failures REQUIRED)** + `cargo test -p lazuli_codegen_go smart_stub` (all
   golden tests) + the existing `emit.rs`/handler-emission tests
   (`tests_p1`/`tests_p2`) green (the moved `hash_password` assertions updated,
   not deleted). `cargo build --workspace` clean. If a new keyword/diagnostic
   was added (expected: NONE), the facet + bridge parity tests
   (`lazuli_keywords`, `lazuli_diagnostics_registry`) are green — state "none
   added" otherwise.
2. **PROVE (live)** — regenerate hostpoint **or** the `customer_auth` fixture
   into a clean out-dir and confirm the emitted `hash_password.go` body
   delegates to `auth.HashPassword` (not `// IMPLEMENT ME`). Paste the emitted
   body in the report. Confirm a non-mapped `@fn` in the same regen still emits
   the plain stub.
3. **TEACH** — `docs/lazuli_way/delegate-to-runtime.md` exists and carries the
   "stubs delegate by default" section + before/after + the 0024 cross-ref + the
   seeded/candidate site list.
4. **ENFORCE** — the golden tests ARE the enforcement: `*_delegates`,
   `non_mapped_fn_still_plain_stub`, `*_keeps_extension_stub_marker_and_init`,
   and `stub_table_is_extensible` prevent regression of both the auto-wire and
   the back-compat guarantee.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_codegen_go smart_stub && cargo test --workspace` (0 failures) + `cargo build --workspace`.
2. **PROVE** — live regen shows the delegating hash body; non-mapped stub unchanged. Report both bodies.
3. **TEACH** — the lazuli_way doc.
4. **ENFORCE** — extensibility + back-compat tests green.

---

## 5. Risks & rollback
- **Output-type bridge wrong** (`HashedRef` constructor / `string` passthrough):
  the body won't compile. Mitigation: the live PROVE gate runs `go build` on the
  regen output implicitly (or compile the emitted file); the agent confirms the
  constructor against the runtime before finalizing. If `HashedRef`'s
  constructor is non-trivial, fall back to `output_type == "string"` passthrough
  + ship hash-only and file the wrap as a candidate.
- **Verify input shape (2-field struct) doesn't cleanly wire** → ship the hash
  row only this cycle (it is the proven flagship and satisfies the gate); verify
  becomes a candidate row. Be explicit in the commit.
- **Two `format!` paths drift** (delegating vs plain scaffold diverge over time)
  → mitigation: factor the shared scaffold OR add the
  `*_keeps_extension_stub_marker_and_init` test that pins the header + init block
  identical across both paths.
- **A future site collides with a suffix** (e.g. a non-auth `@fn` whose site
  happens to end `.auth.password.hash`) → impossible by construction: the site is
  built from the auth block walk (`feature_walks.rs:254`), so the suffix is
  authoritative. The lookup is `ends_with`, feature-agnostic by design.
- **Regenerate-only limit misread as a full fix** → §NL1 states plainly: this
  does not touch existing hand-written handlers; the batch work (B1–B4) + 0024's
  lint cover those.

**Rollback:** `git revert`. The change is: one lookup + one table + two
renderers + one helper in `emit.rs`, updated tests, one doc. On the miss path the
output is byte-for-byte today's. Absent the table, every stub is the plain
`// IMPLEMENT ME` stub exactly as before. No pilot file is touched (regen skips
existing handlers).
