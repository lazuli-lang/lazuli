# Proposal: Quickref Split + `lazuli check --stale-previously`

**Status**: Plan only. No file moves; no CLI implementation. Submit for
approval, then enter the implementation queue per
`docs/next-checklist.md`.

**Owner**: TBD. **Target version**: tooling-only — no `LZI_LANG` or
`LZIR_SCHEMA` bump.

## Motivation

`docs/quickref.md` is the project's "load this first" context pack
([line 3](../quickref.md)). It is currently 705 lines, ~5–6k tokens.
That is too much for the *common* case (authoring a single feature
edit), and too little for the *uncommon* case (someone bringing up a
new app with `app.lzi` + `registry.lzi` + `workspace.lzi`). The
"feature-author" reader pays for content they don't need; the "app
operator" reader doesn't have the most relevant material front-loaded.

Separately, `previously migrated|alias` clauses
([docs/canonical-semantics.md §Identity Across Renames](../canonical-semantics.md#L2477))
have no garbage-collection mechanism. Every alias is a temporary
contract that should be removed once "every supported environment has
migrated and the stored IR baseline no longer contains the old name."
There is no tooling to detect when that condition holds, so aliases
accumulate as undecayed lineage.

## Part 1 — Quickref split

### Proposed structure

| File | Audience | Approx. lines | What lives here |
|---|---|---|---|
| `docs/quickref.md` | shared index | ~120 | Status legend, both audience sub-pages linked, the cheat-sheet linked, "do not add in v0", `inspect` context pack invocation. |
| `docs/quickref-write.md` | feature author | ~280 | `feature` body: `defaults`, `domain`, `policies`, `auth`, `command`, `api`, `workflow`, `job`, `webhook`, `agent`, queries, tests, events. Canonical order for `.lzi`. Policy vocabulary. Name resolution. Identity hints. |
| `docs/quickref-runtime.md` | app/runtime author | ~220 | `app.lzi`, `registry.lzi`, `workspace.lzi`, `contract.lzi`, profiles, `.lzx` routes/experiences/surfaces, deploy/runtime/architecture blocks. |
| `docs/quickref-cheatsheet.md` | both, on demand | ~180 | Closed namespaces table (complete), canonical sugar table, generated `provides` shape, security checklist, event kinds, non-goals shapes. |

The index file (`quickref.md`) is intentionally short — it must fit in
the load-before-task budget the project relies on.

### What moves where

The proposal does **not** rewrite content, only relocates it. Each
section in the current file moves to exactly one new file. Mapping:

| Current section (line) | New location |
|---|---|
| Status Legend (10) | `quickref.md` (kept) |
| Minimal Feature — feature body (22–53) | `quickref-write.md` |
| Minimal Feature — `app.lzi` (55–230) | `quickref-runtime.md` |
| Minimal Feature — `contract.lzi` and routes/experiences (232–311) | `quickref-runtime.md` |
| Canonical Order (313) | `quickref-write.md` |
| Policy Vocabulary (352) | `quickref-write.md` |
| Closed Namespaces (379) | `quickref-cheatsheet.md` (full table); `quickref-write.md` short list |
| Binding Namespaces (403) | `quickref-write.md` |
| Name Resolution (419) | `quickref-write.md` |
| Generated Provides (448) | `quickref-cheatsheet.md` |
| Canonical Sugar Table (495) | `quickref-cheatsheet.md` |
| Queries (509) | `quickref-write.md` |
| Tests (555) | `quickref-write.md` |
| Security Checklist (575) | `quickref-cheatsheet.md` |
| Identity Hints (639) | `quickref-write.md` |
| Event Kinds (655) | `quickref-cheatsheet.md` |
| Non-Goals (669) | `quickref-write.md` |
| Inspect Context Pack (685) | `quickref.md` (kept; this is the bootstrap command set) |
| Do Not Add In v0 (699) | `quickref.md` (kept) |

### Cross-reference rules

- The index `quickref.md` links each sub-file by relative path
  (`./quickref-write.md`, etc.). It does not duplicate content.
- Each sub-file links the index at the top: *"Index: `quickref.md`.
  Cheat sheet: `quickref-cheatsheet.md`."*
- The cheat sheet does not link anywhere — it is leaf content.
- One short rule: *prose explanations live in `quickref-write` /
  `quickref-runtime`; tables and dense lookups live in
  `quickref-cheatsheet`.* If the same content fits both, place it in
  the prose page and reference it from the cheat sheet by anchor.

### Agent-load defaults (after split)

Update `docs/canonical-semantics.md §line 5` and
`docs/quickref.md §Inspect Context Pack` to recommend:

- For ordinary feature edits: load `docs/quickref.md` (index) +
  `docs/quickref-write.md`. ~400 lines, ~3k tokens.
- For app/registry/workspace tasks: load `docs/quickref.md` +
  `docs/quickref-runtime.md`. ~340 lines, ~2.5k tokens.
- For ambiguous tasks: load all three.
- For closed-namespace lookup or canonical-sugar verification: pull
  `docs/quickref-cheatsheet.md` on demand.

This is a tooling change, not a language change. `lazuli inspect` does
not care about the split.

### Migration steps

1. Create the three new files with content moved verbatim.
2. Insert cross-reference headers per the rules above.
3. Update `docs/canonical-semantics.md §line 5` to point at the index.
4. Update `docs/quickref.md` to be the new short index.
5. Search the repo for `quickref.md` references (CI, prompts, agent
   profiles, slash commands, README); update each to the new layout.
6. Verify with `Grep` that no orphan references remain.

No content is deleted. No language change. Reversible by `git revert`.

### Risks

- **Sub-files drift** if updates target only one of them. Mitigation:
  add a CI check (`tools/check-quickref-cross-refs.ps1`) that fails if
  a section moved into one sub-file is also present in another.
- **Agents that learned to `Read` `quickref.md` always** lose
  coverage. Mitigation: keep `quickref.md` populated with the
  bootstrap command set and the "do not add in v0" guard rails — the
  parts that are genuinely cross-cutting.

## Part 2 — `lazuli check --stale-previously`

### Goal

Detect `previously migrated <old>` and `previously alias <old>` clauses
that are no longer load-bearing. A clause is *stale* when the IR
baseline no longer contains the old name and no recent semantic-diff
references it.

### CLI shape

```text
$ lazuli check --stale-previously [--remove] [--baseline <path>] [<file>...]

  --stale-previously     Report stale `previously migrated|alias` clauses.
                         Without other args, scans the whole package.
  --remove               Apply the removal patches in place. Implies
                         --stale-previously. Exits non-zero if any clause
                         could not be auto-removed (e.g., comments depend on it).
  --baseline <path>      Override the IR baseline path. Defaults to
                         .lazuli/baseline.lzir.json or the package's
                         configured baseline.
```

### Detection algorithm

A `previously` clause is stale when **all four** conditions hold:

1. **Baseline absence**: the baseline IR (configured per package or via
   `--baseline`) does not contain the old name in the corresponding
   IR location (resource name, field name, command name, transition
   name, etc.).
2. **No semantic-diff reference**: the last `lazuli inspect --diff
   --since <baseline-revision>` projection does not reference the old
   name in any rename edge.
3. **No source reference**: no other `.lzi` or `.lzx` in the package
   references the old name (caught by a literal source scan; rare,
   but a safety belt).
4. **Age threshold met**: the clause has been in source for at least
   one minor `LZI_LANG` cut, measured by git blame against the file's
   commit history. Default: 1 cut. Override with `--min-age <count>`.

A clause failing any of (1)–(3) is *load-bearing*. A clause meeting
(1)–(3) but not (4) is *young*: report as informational, do not
include in `--remove` patches.

### Output format

```text
$ lazuli check --stale-previously
  examples/full-capsule/full-capsule.lzi:46  resource 'Customer'      previously migrated 'Account'   (3 cuts old)
  examples/full-capsule/full-capsule.lzi:50  field    'lifecycle_stage' previously migrated 'status'   (1 cut old)
  examples/customer-import.lzi:88             command  'reassign'      previously alias    'transfer' (5 cuts old)

  3 stale clauses. Run with --remove to apply patches.

$ lazuli check --stale-previously --remove
  Patched 3 clauses across 2 files.
  Re-run `lazuli check` to verify the patches.
```

### Implementation notes

- The IR baseline lookup reuses the existing diff machinery
  (`crates/lazuli_ir/src/diff.rs` if it exists; otherwise via
  `lazuli inspect --format=json --diff --since`).
- `--remove` produces patches as text edits on the source files. It
  does not regenerate IR; the next `lazuli check` run validates the
  result.
- Comments adjacent to a `previously` line are preserved unless they
  literally contain the old name on a single line; in that case, the
  patch leaves the comment in place and the operator must triage.
- The age threshold (4) is a heuristic against premature removal of
  contracts that just landed. The default of 1 cut is liberal; teams
  may set `--min-age 2` in CI for stricter retention.

### Edge cases

- **`previously alias`** (compatibility shim) is removed by the same
  command; the rename is from `<old> -> <current>` and both clauses
  represent transition contracts that decay similarly.
- **Workflow transitions with renamed verbs** (`activate previously
  migrated start: lead -> active`): the clause is on the transition
  body, not inline, per
  `docs/invariants.md §line 127`. Detection is identical.
- **Cross-feature rename** (a feature renames a query that another
  feature consumes): flagged as load-bearing — the consuming feature
  may still emit code referring to the old name through the alias.
  Reported with `previously_load_bearing_cross_feature_diagnostics`,
  not removed.

### Diagnostic ids

| Id | Severity | Meaning |
|---|---|---|
| `previously_stale_diagnostics` | info | Clause meets all four conditions; safe to remove. |
| `previously_young_diagnostics` | info | Clause meets (1)–(3) but not (4); will become removable. |
| `previously_load_bearing_diagnostics` | info | Clause is still referenced; keep. |
| `previously_unknown_warning` | warning | Clause references an old name that was never in the IR baseline (typo or broken history). |
| `previously_load_bearing_cross_feature_diagnostics` | info | Cross-feature reference still active. |

### Layer placement

Tooling, not language. The CLI flag lives in `lazuli check` because
that is where doctor-style diagnostics already aggregate. No IR delta.
No backend codegen change. Adapters do not see this.

### Acceptance criteria

- [ ] `lazuli check --stale-previously` reports correctly across the
  current full-capsule fixture (ground truth: zero stale clauses
  initially).
- [ ] Adding a contrived `previously migrated <old_name>` to a
  fresh field produces a `previously_young_diagnostics` until two
  more `LZI_LANG` minor bumps land.
- [ ] After two minor bumps and removal of the old name from baseline,
  the same clause flips to `previously_stale_diagnostics`.
- [ ] `--remove` patches the clause and removes the now-empty
  indented child block, leaving canonical formatting.
- [ ] No false positives on the kitchen-sink fixture's intentional
  long-running aliases (if any).

## Combined timeline

| Step | Output | Effort |
|---|---|---|
| 1 | Quickref split (file moves, CI cross-ref check) | half-day |
| 2 | `lazuli check --stale-previously` detection (no `--remove`) | 1–2 days |
| 3 | `--remove` patch synthesis | 1 day |
| 4 | LSP exposure (optional code action: "remove stale `previously`") | half-day |

Step 1 is independent and may land first. Steps 2–4 are sequential.

## Non-goals

- A history/audit projection of all renames (`docs/language-backlog.md
  §line 226` defers this until inline `previously` "starts materially
  polluting mature features"). The proposed CLI works on the current
  state; an audit projection would be a separate cut.
- Renaming the sub-files. `quickref-write.md` and `quickref-runtime.md`
  are the chosen names; bikeshedding is non-blocking.
- Detecting stale comments or stale `out_of_scope` entries. Out of
  scope; same lifecycle but different tooling.
