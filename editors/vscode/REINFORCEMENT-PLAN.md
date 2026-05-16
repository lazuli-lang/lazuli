# Lazuli Extension — Reinforcement Plan

**Goal:** total reinforcement of (a) syntax highlighting coverage + semantic
consistency, and (b) error/warning feedback in the VS Code editor (matching
what the doctor catches in the CLI).

**Triggered by:** Lucas observing in 2026-05-15 session:
1. `comand move` typo silently passed — fix landed (5e67ed2 + this commit
   bumps to ERROR + adds `emits` to catalog).
2. `emits` block opener was missing from the kind catalog (false positive).
3. Open question: are ALL doctor errors visible in the VS Code editor, or
   are some only surfaced when running `lazuli doctor` from the CLI?
4. Open question: are there other categories of typos (kind keywords inside
   `app`, `registry`, `view`, `surface`, `command body`, `audience`) that
   silently pass?

---

## Phase R1 — Audit (3 parallel agents, no code changes)

### Agent R1.A — Grammar coverage audit (Claude)

**Output:** `editors/vscode/AUDIT-GRAMMAR-COVERAGE.md`

**Task:**
1. Walk every keyword in `crates/lazuli_lsp/src/lib.rs::keyword_description()`.
2. For each, search `editors/vscode/syntaxes/lazuli.tmLanguage.json` to
   verify it's matched by SOME pattern.
3. Walk `examples/full-capsule/full-capsule.lzi` + every Pleiades feature
   token-by-token via VS Code's "Inspect Editor Tokens and Scopes"-equivalent
   logic (read the grammar; predict the scope chain).
4. Classify each finding:
   - **Missing**: keyword has no grammar rule → falls through to plain text
   - **Wrong scope**: matched but assigned a scope that doesn't fit the role
     (e.g., a `keyword` colored as `entity.name`)
   - **Inconsistent**: same role colored differently across contexts
   - **Over-matching**: keyword colored where it shouldn't (typo of Phase 2.1)
5. Report ordered by ROI (most-frequent + most-visible first).

**Constraint:** read-only audit. Don't touch grammar JSON.

### Agent R1.B — LSP-vs-Doctor diagnostic gap audit (Claude)

**Output:** `editors/vscode/AUDIT-LSP-DOCTOR-GAP.md`

**Task:**
1. Walk every doctor diagnostic code defined in `crates/lazuli_cli/src/doctor.rs`
   + sub-modules under `crates/lazuli_cli/src/doctor/`.
2. For each code, classify:
   - **LSP-mirrored**: a corresponding file-local diagnostic exists in
     `crates/lazuli_lsp/src/lib.rs` → user sees squiggle in VS Code
   - **Doctor-only**: only fires during `lazuli doctor` CLI run → invisible
     in editor until manual run
3. For doctor-only codes, classify:
   - **Cross-file inherent**: needs whole-package context (e.g., 
     `cross_feature_type_unresolved`); LSP can't easily replicate
   - **File-local-portable**: could be replicated as an LSP diagnostic with
     reasonable cost
4. Output a prioritized list: "these N codes should be wired into LSP next".

**Constraint:** read-only audit. Don't add LSP diagnostics yet.

### Agent R1.C — Real-world doctor sweep (Codex)

**Output:** `c:/tmp/audit-doctor-sweep.md`

**Task:**
1. Run `lazuli doctor` from the freshly-built release binary on:
   - Every project under `c:/Users/lucas/lazuli/examples/*/`
   - `c:/Users/lucas/dev/pleiades/`
2. Capture every error/warning/hint output.
3. Classify by:
   - **Severity bug**: should be error but is warning, OR vice versa
   - **Typo-detectable class**: similar to `feature-unknown-kind` 
     (unknown kind in app/registry/view/etc.)
   - **Coverage gap**: real semantic issue not currently caught
4. Cross-reference with R1.A and R1.B to identify overlap.

**Constraint:** read-only sweep. Don't fix anything; just catalog.

---

## Phase R2 — Implementation (parallel by file scope, ~5 agents)

After R1 lands, dispatch:

### Agent R2.D — Grammar coverage gaps (Claude)

Based on R1.A findings, add missing patterns + fix scope-name inconsistencies
in `editors/vscode/syntaxes/lazuli.tmLanguage.json`. Constraint: stay within
established scope families documented in `SCOPES.md`.

### Agent R2.E — LSP typo-detection family expansion (Claude)

Replicate the `feature-unknown-kind` pattern for other contexts:

- `app-unknown-kind`: indent-2 inside `app X`, against APP_BODY_KINDS catalog
- `registry-unknown-kind`: indent-2 inside `registry`, against REGISTRY_BODY_KINDS
- `view-unknown-kind`: indent-2 inside `view X <Component>` (L0 #6 bodies)
- `surface-unknown-kind`: indent-2 inside `surface X <platform>`
- `command-statement-unknown`: indent-4 inside `command X` body
- `query-statement-unknown`: indent-4 inside `query.list X`/`query.lookup X`
- `audience-unknown-kind`: indent-4 inside `audience X` (only `view <name> <Component>`)

Each gets:
- A closed catalog (sourced from `keyword_description` + canonical examples)
- Levenshtein suggestion (≤2 edit distance)
- ERROR severity (compile-blocking class) for kinds that drop blocks
- WARNING severity for typos in modifiers (`requied` instead of `required`)
- Filters: skip decorators (`@`), field decls (`:`), assignments (`=`), 
  parameterized calls (`(`)

### Agent R2.F — LSP wiring of doctor-only diagnostics (Claude)

Based on R1.B's "file-local-portable" list, port doctor diagnostics into
LSP file-local checks. Each port:
- Same diagnostic code (so quick-fix tooling can find it)
- Same severity
- File-local subset of the doctor logic (skip cross-file references)

### Agent R2.G — Doctor severity bumps (Claude)

Based on R1.C findings: any doctor warning that's actually a compile-blocker
(parser-silently-drops scenarios, like `feature-unknown-kind` was) gets
bumped to ERROR. Document each bump in commit message with rationale.

### Agent R2.H — Snapshot fixture extension (Codex)

Add `editors/vscode/tests/grammar/typos.lzi` covering every diagnostic class
from R2.E (each typo with expected diagnostic code + suggestion). Snapshot
locks in the new diagnostic catalog. Add to `npm run test:grammar:check` flow.

---

## Phase R3 — Validate + integrate (single agent + me)

1. Repackage `.vsix`
2. Reinstall locally
3. Run `npm run test:grammar:check` — expect all green
4. Visual smoke test on Pleiades + full-capsule
5. Update `editors/vscode/HIGHLIGHTING-CHECKLIST.md` to reflect what landed
6. Commit each wave separately so any regression can be bisected

---

## Constraints (apply to every wave)

- Source of truth: `crates/lazuli_lsp/src/lib.rs` `keyword_description()` + 
  `KEYWORDS` const + per-context `is_canonical_*_block` checks
- Real-world test files: `examples/full-capsule/full-capsule.lzi` + Pleiades
- Established scope families: see `editors/vscode/SCOPES.md`
- All grammar changes must keep existing snapshots passing (or update
  snapshots intentionally with rationale)
- Don't touch `c:/Users/lucas/dev/pleiades` source files
- All agents work uncommitted; orchestrator (me) integrates per wave

---

## Why this order

- R1 first: thorough audit before any change. Avoids fixing the wrong things.
- R2 parallel: each agent owns a distinct file or scope; no merge conflicts.
- R3 last: holistic validation prevents partial-fix regressions.
