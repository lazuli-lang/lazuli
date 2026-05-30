# Cement R1 — Reconciliation Charter (owner decisions)

This file is **authoritative** over the individual `SPEC-*.md` drafts where they conflict.
It folds in the 7 discovery specs and resolves every blocker/major the adversarial review found.
Branch: `cement/language-coherence-r1`.

## Owner decisions

**D1 — Add SPEC-00 (migration engine), Wave 0.** `apply_recipe` only does `additive`
(upgrade.rs:137). Build `rename` + `rewrite` kinds first; every breaking spec ships data, not code.

**D2 — Merge SPEC-03 into SPEC-01.** Both rewrite the identical alias arms at
`types.rs:160-178`. The catalog foundation owns the alias decision. **Alias policy:** *reject +
autocorrect* — a diagnostic that names the canonical spelling, and `lazuli fmt` normalizes
`Int→Integer` etc. (Never silently tolerate — that is the exact bug being killed, and it
contradicts the "reject+autocorrect, never tolerate" doctrine.) Single rule: `VOCAB-SCALAR-ALIAS-001`.

**D3 — Sigil doctrine is codified in SPEC-01, not inline prose.** `CatalogKind` *is* the doctrine,
machine-readable. The normative line:
> `@` = a named reference to a declared-or-closed entity (policy, role, scope, actor, pii-class,
> key, fn, hook, validator, anchor, client, command, feature, translation, file, audience, trace,
> llm, tool, adapter, query_modifier). **Bare PascalCase** = a type from the closed type/semantic
> catalog (Text, Integer, Email, Money, Encrypted, …). `@` NEVER appears in type position.

This resolves the coherence-critic blocker: `@pii`/`@key` **stay `@`** (they are classifications/
references, not types); only `@semantic`/`@cap` **types** go bare (SPEC-04 retires those two
decorator rows). The doctrine ships as a generated normative doc section + a doctor rule.

**D4 — SPEC-05 does NOT depend on SPEC-04.** `==` and `@`-off-types are semantically independent
(dependency-critic major). They still **serialize on the shared `full-capsule.lzi`**, but order is
free.

**D5 — Merge SPEC-09 into SPEC-14.** `field: many Type` (`many_decl`) is a grammar-doc ghost the
parser never implemented (discovery). So the "cut" is a grammar-doc deletion + the unused `many`
type-ctor registry row + a one-line fix to `examples/linear-issue.lzi:42`. Trivial.

**D6 — SPEC-01 splits `registry.rs` first.** It is 3601 LOC (over the 500 ceiling) and SPEC-01 adds
rows. Rails-layout split (`registry/{keywords,decorators,types,design,...}.rs` + `mod.rs`
re-exporter) is the first commit of SPEC-01. The broader ~40-file LOC debt → **SPEC-19** (independent,
background-eligible); CLAUDE.md's false "0 above 500" claim is corrected in SPEC-11.

**D7 — Money is `SemanticScalar`,** not `Scalar` (types.rs:172-174 lowers it via the semantic path).
Fix in SPEC-01's CatalogKind.

**D8 — SPEC-02 corrections.** (a) Drop the false claim that `LZX-REPEATABLE-SUM-001` /
`LZX-BOARD-LANES-001` are unimplemented — they exist. (b) Retiring §7a's `inline_table on_change
@command.X` removes the *only* `@command` user, so `@command` is **retired from the catalog** too —
that flows through SPEC-01's single source (and refs.rs/vocab.rs derive it, so no hand-edit).

**D9 — SPEC-11 depends on ALL specs** (docs keystone), not just SPEC-01. SPEC-10b likewise.

**D10 — SPEC-04 and SPEC-07 both touch decorator rows** in `registry.rs` (retire `@semantic`/`@cap`;
reclassify `@role`/`@scope`/`@actor`). After SPEC-01 adds `CatalogKind` to every row, these become
small per-row edits → serialize SPEC-04 → SPEC-07. SPEC-08's `requires`-at-three-scopes
(registry.rs:1725+) is handled in-spec; SPEC-08 serializes on the shared fixture with 05/07.

## Corrected spec set (18 nodes)

| Spec | Title | Breaking | Note |
|---|---|---|---|
| SPEC-00 | Migration engine (rename+rewrite) | no | NEW (D1) |
| SPEC-01 | Single-source catalogs + alias policy + sigil doctrine + registry split | yes | absorbs SPEC-03 (D2), +D3/D6/D7 |
| SPEC-02 | Retire §7a brace dialect | yes | +D8 |
| ~~SPEC-03~~ | ~~scalar aliases~~ | — | merged → SPEC-01 (D2) |
| SPEC-04 | `@` off types | yes | dep SPEC-01 |
| SPEC-05 | `==` for equality | yes | dep none (D4); serialize on fixture |
| SPEC-06 | Compound-join rule | yes | needs SPEC-00 rename |
| SPEC-07 | Policy coherence | yes | dep SPEC-04 (doctrine) |
| SPEC-08 | Test dialects 4→2 | yes | serialize on fixture |
| SPEC-09+14 | Retire `many` collection ghost | yes | merged (D5) |
| SPEC-10a | Delete dirty examples + re-point scaffold | no | early |
| SPEC-10b | Author prod-ready curated CI fixtures | n/a | LAST (dep ALL) |
| SPEC-11 | Docs reorg + de-dictatorialize + CLAUDE.md | n/a | LAST (dep ALL) (D9) |
| SPEC-12 | app.lzi grammar: locale/cors/logging/tracing/encryption | no (doc) | discovery HIGH |
| SPEC-13 | enum int/string storage grammar; retire `value` ghost | yes(doc+parser) | discovery |
| SPEC-15 | app.lzi grammar: cookie/proxy/limits/headers | no (doc) | discovery HIGH |
| SPEC-16 | runtime `locale_negotiate` grammar | no (doc) | discovery |
| SPEC-17 | enum metadata colon-form grammar | no (doc) | dep SPEC-13 |
| SPEC-18 | app.lzi grammar: lazuli_version/subscription | no (doc) | discovery |
| SPEC-19 | LOC debt paydown (~40 files >500) | no | NEW (D6); background |

## Corrected execution waves

- **Wave 0 [sequential]** — SPEC-00 (migration engine). Unblocks all recipes.
- **Wave 1 [sequential]** — SPEC-01 (registry split → CatalogKind → scalars/semantic rows → derive
  fns → rewire vocab/refs/types → alias policy → sigil doctrine → gen-catalog-reference → drift
  gates). **Root.**
- **Wave 2 [parallel, own worktrees — disjoint files]** — SPEC-10a (examples/ + scaffold), SPEC-12,
  SPEC-15, SPEC-16, SPEC-18 (app.lzi grammar DOC only), SPEC-13 (enum grammar+enums.rs), SPEC-09+14
  (grammar ghost + linear-issue), SPEC-19 (LOC debt; pure splits). SPEC-06 (compound-join) parallel
  iff it lands after SPEC-00. SPEC-02 (§7a, .lzx-surface) parallel **but coordinates @command
  retirement through SPEC-01** → run after Wave 1.
- **Wave 3 [sequential]** — SPEC-04 (`@` off types; retires @semantic/@cap rows; rewrites fixture
  type sites).
- **Wave 4 [sequential, shared full-capsule + registry rows]** — SPEC-05 → SPEC-07 → SPEC-08.
- **Wave 5 [sequential]** — SPEC-17 (enum metadata doc; dep SPEC-13).
- **Wave 6 [sequential, bookend — observe final surface]** — SPEC-10b, SPEC-11.

**Critical path:** SPEC-00 → SPEC-01 → SPEC-04 → SPEC-07 → SPEC-11.

## Standing risks (carry into execution)
1. Fixture-churn collision on `full-capsule.lzi` (SPEC-04/05/07/08) — strictly serialize Wave 4; each
   commit re-runs `lazuli inspect full-capsule` + `generate --check`.
2. Shared registry + generated artifacts (tmLanguage, keyword-reference) — only SPEC-01 owns the
   generator; later specs edit registry rows then **regenerate**, never hand-edit generated files.
3. Every breaking spec ships its `recipe.toml` in the SAME wave as the parser change (SPEC-00 makes
   this possible).
4. Workspace green every commit: `cargo build --workspace` + per-crate tests + `proven_complete` +
   `keyword_surface_parity` + `*_fresh`. Doctor iron-hand only promoted after a spec's count reaches ~0.
