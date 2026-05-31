---
id: 0007
title: comment/allow doctor rules — DOCTOR-ALLOW-NO-REASON-001 + LZI-COMMENT-NOISE-001
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: true
track: prove
test_gate: "cargo test -p lazuli_doctor allow_no_reason && cargo test -p lazuli_doctor lzi_comment_noise"
agent: unassigned
---

# TechSpec — comment/allow doctor rules

## Approach
Two advisory rules in `lazuli_doctor`, both pure source-line scans (no IR dependency), both clean on current pilots (preventive). (a) `DOCTOR-ALLOW-NO-REASON-001` adds a reason-tail detector beside the existing opt-out helper and flags bare allows — without changing opt-out semantics. (b) `LZI-COMMENT-NOISE-001` generalizes the `config_noise.rs` heuristic into a new `lzi_hygiene/` module scoped to `.lzi`/`.lzx`, adding a decorative-divider check, advisory and never-gating, honoring `# doctor:allow`.

## Surface
**Create:**
- `crates/lazuli_doctor/src/lzi_hygiene/comment_noise.rs` — `LZI-COMMENT-NOISE-001`. Comment-vs-semantic ratio (reuse the `config_noise.rs` counting shape, extended to `//` line/inline comments since `.lzx` may carry them) + decorative-divider detector. Advisory; never gates.
- (if not already present) `crates/lazuli_doctor/src/lzi_hygiene/mod.rs` — family module re-exporting `file_size_001` (existing sibling) + `comment_noise`. If `file_size_001.rs` currently lives flat in `src/`, MOVE it under `lzi_hygiene/` and update its `mod`/`use` path (keep its rule code `LZI-FILE-SIZE-001` unchanged).

**Modify:**
- `crates/lazuli_doctor/src/allow_comment.rs` — add `pub fn line_allow_has_reason(line: &str) -> bool` (true when the line, after `doctor:allow <CODE>`, contains a `reason` keyword followed by a quoted string, separator `—`/`--`/whitespace). Add `pub fn source_allows_without_reason(source: &str) -> Vec<(usize, String)>` returning `(line_no, code)` for each `# doctor:allow <CODE>` line lacking a reason tail. Do NOT change `source_contains_doctor_allow` behavior (bare allow still opts out).
- `crates/lazuli_doctor/src/<rule_registration>` (the module/list where rules are wired — e.g. `lib.rs` rule registry) — register `DOCTOR-ALLOW-NO-REASON-001` (scans all `.lzi`/`.lzx`) and `LZI-COMMENT-NOISE-001` (scans `.lzi`/`.lzx`). Both advisory severity.
- `crates/lazuli_doctor/src/lib.rs` — `mod lzi_hygiene;` (replacing any flat `mod file_size_001;` if moved).

**Teach:**
- `docs/lazuli_way/comment-hygiene.md` — co-filled with 0006. Append two rows in idiom-doc shape (one per rule). Do NOT overwrite 0006's "doctor:allow is highlighted" note.
- scaffold `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — bullet: "Every `# doctor:allow <CODE>` MUST carry `— reason \"<why>\"` (`DOCTOR-ALLOW-NO-REASON-001`); keep `.lzi`/`.lzx` comment-light, no decorative dividers (`LZI-COMMENT-NOISE-001`)."

## Contracts
**DOCTOR-ALLOW-NO-REASON-001 (advisory):**
- Fires on any line matching `# doctor:allow <CODE>` (case-insensitive on `doctor:allow`, same as `source_contains_doctor_allow`) that does NOT carry a `reason "..."` tail.
- Does NOT fire on `# doctor:allow <CODE> — reason "..."` (em-dash) or `-- reason "..."` (ASCII) or `reason "..."` with only whitespace separator.
- Message names the fix: `add — reason "<why this suppression is justified>"`.
- Severity: advisory (info/hint/warning class). It does NOT void the opt-out — the named rule stays suppressed; this is a parallel nudge about the allow itself.
- Self-suppressible: `# doctor:allow DOCTOR-ALLOW-NO-REASON-001 — reason "..."` (which, being reasoned, doesn't fire on itself).

**LZI-COMMENT-NOISE-001 (advisory, NEVER gates):**
- Scope: `.lzi` + `.lzx` files only.
- Fires when EITHER: (i) `comment_lines > semantic_lines` (the `CONFIG-NOISE-001` dominance rule, extended to count `//` comments for `.lzx`), OR (ii) the file contains ≥1 decorative-divider line (a comment whose body is a run of a single ruler char `- = * # /` of length ≥ a threshold, e.g. 8).
- Does NOT fire on a clean feature file (few comments, no rulers) — the current pilots.
- Honors `# doctor:allow LZI-COMMENT-NOISE-001` via `source_contains_doctor_allow`.
- Severity: advisory; NEVER contributes to gate failure (mirror `config_noise.rs` discipline — its module doc states "never gates").
- Reuses `ConfigNoiseMetrics`-shaped counting (lift the shared counter or factor a common helper; do not duplicate the trailing-`#` logic verbatim — share it).

## Plan — for the executing agent
1. Read `allow_comment.rs` (matcher + tests) and `config_noise.rs` (counter + `fires()` + tests) for the exact shapes to extend/reuse.
2. Add `line_allow_has_reason` + `source_allows_without_reason` to `allow_comment.rs` with unit tests covering: reasoned (no fire), bare (fire), em-dash vs `--`, whitespace-only separator, multiple allows on different lines.
3. Create `lzi_hygiene/` module; move `file_size_001.rs` under it if currently flat (keep rule code). Add `comment_noise.rs` reusing the config-noise counter (extended for `//`) + a `is_decorative_divider(line) -> bool` helper.
4. Register both rules in the doctor rule registry with advisory severity; ensure `LZI-COMMENT-NOISE-001` is excluded from any gate aggregation (never gates).
5. Wire `# doctor:allow` suppression into `LZI-COMMENT-NOISE-001` via `source_contains_doctor_allow`.
6. Run the full `lazuli_doctor` test suite + `lazuli doctor` on both pilots; confirm zero new findings (all 85 allows reasoned; `.lzi` clean).
7. Fill `docs/lazuli_way/comment-hygiene.md` (append, idiom-doc shape, both rule codes named) + add the scaffold bullet to both `.tmpl` files (keep them byte-identical in the edited region, per 0001's mirror rule).

## Tests first (TDD)
- [ ] `allow_no_reason::fires_on_bare_allow` — `# doctor:allow X-1\n` → finding.
- [ ] `allow_no_reason::silent_on_reasoned_allow` — `# doctor:allow X-1 — reason "ok"\n` → no finding.
- [ ] `allow_no_reason::accepts_ascii_dash_and_ws_separator` — `-- reason "x"` and `  reason "x"` both count as reasoned.
- [ ] `allow_no_reason::pilots_clean` — scanning the 85-use corpus shape (reasoned) yields zero findings.
- [ ] `lzi_comment_noise::fires_on_comment_dominant_lzi` — synthetic `.lzi` with `comment_lines > semantic_lines` → finding.
- [ ] `lzi_comment_noise::fires_on_decorative_divider` — a `# --------` ruler line → finding.
- [ ] `lzi_comment_noise::clean_feature_does_not_fire` — minimal clean feature → no finding (matches current pilots).
- [ ] `lzi_comment_noise::doctor_allow_suppresses` — `# doctor:allow LZI-COMMENT-NOISE-001 — reason "generated table"` silences it.
- [ ] `lzi_comment_noise::never_gates` — even when firing, it does not push the gate to fail (assert severity/aggregation).

## Gate
`test_gate` green (`cargo test -p lazuli_doctor allow_no_reason && cargo test -p lazuli_doctor lzi_comment_noise`) **and** `lazuli doctor` on hostpoint + pauta-web shows zero new findings (both rules clean on current pilots).

### Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

**4 gates, concrete for 0007:**
1. BUILD → `cargo test -p lazuli_doctor allow_no_reason && cargo test -p lazuli_doctor lzi_comment_noise` green; both rules registered advisory; `LZI-COMMENT-NOISE-001` excluded from gate aggregation.
2. MIGRATE → `lazuli check && lazuli doctor && go build ./...` clean in hostpoint + pauta-web with ZERO new findings (all 85 allows reasoned; `.lzi` clean) — the rules are preventive, so "migration" is verifying no current pilot trips them.
3. TEACH → `docs/lazuli_way/comment-hygiene.md` filled (co-filled with 0006) with both rule rows in idiom-doc shape; scaffold `CLAUDE.md.tmpl` + `AGENTS.md.tmpl` bullet added.
4. ENFORCE → `DOCTOR-ALLOW-NO-REASON-001` fires on a bare `# doctor:allow CODE`; `LZI-COMMENT-NOISE-001` fires on a comment-dominant / decorative-divider `.lzi`. Both rule codes named in the idiom doc.

## Risks & rollback
- The reason-tail detector diverges from the 0006 grammar regex (one highlights what the other doesn't honor) → mitigation: `comment-hygiene.md` documents both surfaces; align the separator set (`—`/`--`/ws) and the `reason "..."` shape across both specs.
- `LZI-COMMENT-NOISE-001` false-positives on a dense-but-justified feature → mitigation: advisory + never-gates + `# doctor:allow` escape; tune the divider threshold against pilots before commit.
- Moving `file_size_001.rs` under `lzi_hygiene/` breaks its `mod`/`use` paths or its rule registration → mitigation: keep `LZI-FILE-SIZE-001` code unchanged; run the full doctor suite after the move; if the move is risky, leave `file_size_001.rs` flat and have `lzi_hygiene/mod.rs` re-export it.
- Counting `//` for `.lzx` over-counts if `.lzx` doesn't use `//` comments → mitigation: confirm `.lzx` comment syntax before extending the counter; gate the `//` branch on file extension.

**Rollback:** `git revert` the commit — both rules are additive advisory checks that fire on nothing in the current pilots; reverting removes them with no pilot impact. If `file_size_001.rs` was moved, the revert restores its original path.
