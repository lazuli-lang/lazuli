---
id: 0029
title: Comment-discipline — canonical-channel policy + LZI-COMMENT-PROSE-001 + codemod
type: techspec
status: ready
created: 2026-06-02
depends_on: [0028]
parallel_safe: false
test_gate: "cargo test --workspace"
agent: unassigned
---

# TechSpec — Comment-discipline for `.lzi`/`.lzx`

## Approach
A per-comment-line lint, `LZI-COMMENT-PROSE-001`, in the existing `lzi_hygiene/` family (sibling of `lzi_comment_noise.rs`). Pure source-line scan (no IR shape needed for the heuristic) EXCEPT it consults 0028's `Module.doctor_allows` to exempt waiver lines (node + legacy). It flags `#` lines whose body reads as prose, with three narrow carve-outs. Severity defaults WARNING and escalates to ERROR under `DoctorProfile::IronHand` via the existing `resolve_<cat>_severity` infra. LziHygiene never gates, so it nudges without refuse-emit. A conservative `lazuli fix` codemod deletes only the safe mechanical cases (empty `#`, construct-restating duplicate). The channel policy is written into `comment-hygiene.md` and the scaffold guidance. Dumb and functional: word-count + punctuation heuristic, two carve-out predicates, reuse the `lzi_comment_noise` module shape verbatim.

## Surface
**Create:**
- `crates/lazuli_doctor/src/lzi_hygiene/comment_prose_001.rs` — `LZI-COMMENT-PROSE-001`. `scan_lzi_comment_prose(source, allows: &[DoctorAllow]) -> Vec<ProseFinding>`; `is_prose_comment(body) -> bool`; carve-out predicates `is_file_header_line(idx, ...)`, `is_structured_marker(body)`. Mirror `lzi_comment_noise.rs` structure (CODE const, finding struct, module doc with trigger-cue, `# doctor:allow`/node suppression).
- `crates/lazuli_cli/src/commands/fix/comment_prose.rs` — codemod: delete empty `#` lines + comments that exact/near-duplicate the next construct line; leave everything else; idempotent.

**Modify:**
- `crates/lazuli_doctor/src/lzi_hygiene/mod.rs` — `pub mod comment_prose_001;` + re-export.
- `crates/lazuli_doctor/src/lib.rs` — register `LZI-COMMENT-PROSE-001` (LziHygiene); add the diagnostics-registry bridge entry (new doctor code); ensure it is excluded from gate aggregation (LziHygiene is already non-blocking in gate.rs, but the rule must still emit through the standard non-gating path like `LZI-COMMENT-NOISE-001`).
- `crates/lazuli_doctor_config/src/lib.rs` (or `lib_p1`/`lib_p2` — grep `resolve_` + `LziHygiene`/`Lzi`) — wire `LZI-COMMENT-PROSE-001` into the iron-hand escalation map so its WARNING → ERROR under `DoctorProfile::IronHand`. If LziHygiene as a whole already escalates under iron-hand, no map change — assert it via test.
- the rule-dispatch site that calls `scan_lzi_comment_noise` per `.lzi`/`.lzx` file (grep `scan_lzi_comment_noise` callers) — call `scan_lzi_comment_prose` alongside it, passing the module's `doctor_allows` slice for the suppression carve-out.
- `crates/lazuli_cli/src/commands/fix/mod.rs` — wire `comment_prose` under `lazuli fix --rule LZI-COMMENT-PROSE-001`.

**Teach:**
- `docs/lazuli_way/comment-hygiene.md` — add the CHANNEL POLICY decision table (verbatim from ADR) + an `LZI-COMMENT-PROSE-001` row (idiom → before/after `.lzi` excerpt → enforcing rule + carve-outs + the codemod). Keep 0006/0007/0028 sections intact.
- scaffold `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — bullet (byte-identical region per 0001 mirror rule): "Do NOT write prose `#` comments in `.lzi`/`.lzx` (`LZI-COMMENT-PROSE-001`). Rationale → `<feature>.ctx.md`; a construct's intent → its `purpose`/`doc`/`description`; a waiver → `@doctor.allow(CODE, reason: \"…\")`. `#` is only for a file-header line or short `# TODO:`/`# NOTE:` markers."

## Contracts
**`LZI-COMMENT-PROSE-001` (LziHygiene; WARNING default / ERROR iron-hand; NEVER gates):**
- Scope: `.lzi` + `.lzx` only.
- Fires per `#` comment line whose body `is_prose_comment` AND not a carve-out.
- `is_prose_comment(body)`: trim leading `#`/whitespace; TRUE when word_count ≥ 4 OR body ends with `.`/`!`/`?` (and is not a single dotted-identifier like `query.list`). Tunable consts (`PROSE_MIN_WORDS = 4`).
- Carve-outs (no fire): (1) the line is a `@doctor.allow(...)` node OR a legacy `# doctor:allow` whose code appears in `Module.doctor_allows`; (2) a single file-header comment at line 1 (only one, only top); (3) a structured marker — body ≤ `MARKER_MAX_WORDS` (default 3) AND starts with a known marker token (`TODO`/`FIXME`/`NOTE`/`HACK`/`XXX`, optional `:`); (4) a decorative divider (owned by `LZI-COMMENT-NOISE-001` — defer, don't double-flag).
- Message names the channel: e.g. `prose comment — move rationale to <feature>.ctx.md, a construct's intent to its purpose/doc field, or a waiver to @doctor.allow(...)`.
- Suppressible per-file via `@doctor.allow(LZI-COMMENT-PROSE-001, reason: "...")` (0028) and the legacy comment form this window.

**`scan_lzi_comment_prose(source: &str, allows: &[DoctorAllow]) -> Vec<ProseFinding>`** — `ProseFinding { line: usize, message: String }`, mirroring `NoiseFinding`. `allows` is the file's slice of `Module.doctor_allows` for the suppression carve-out (empty slice = no node suppression, legacy `#` scan still applies as before).

**Codemod (`lazuli fix --rule LZI-COMMENT-PROSE-001`):** deletes only (a) whitespace-only `#` lines and (b) a `#` comment line whose body, lowercased and tokenized, is a subset of the immediately-following construct's head tokens (a pure restatement). Never deletes a carve-out line. Idempotent. Reports (does not move) all other prose findings.

## Plan — for the executing agent
1. Read `lzi_comment_noise.rs` (the sibling shape to mirror), its dispatch caller (grep `scan_lzi_comment_noise`), `gate.rs` (confirm LziHygiene non-blocking), `lazuli_doctor_config` `resolve_*`/iron-hand map, 0028's `Module.doctor_allows` + `allow_registry`, the `lazuli fix` dispatcher.
2. Create `comment_prose_001.rs`: CODE const, `ProseFinding`, `is_prose_comment`, the three carve-out predicates, `scan_lzi_comment_prose(source, allows)`. Module doc with the trigger-cue line (module_headers requirement). Honor node + legacy suppression.
3. Unit-test the heuristic + every carve-out (TDD list below).
4. Register the rule in `lazuli_doctor/lib.rs`; add the diagnostics-registry bridge entry; confirm it routes through the non-gating LziHygiene path.
5. Wire WARNING→ERROR-under-iron-hand in `lazuli_doctor_config` (or assert LziHygiene already escalates) + a test (`iron_hand_escalates_to_error`).
6. Call `scan_lzi_comment_prose` at the dispatch site beside `scan_lzi_comment_noise`, threading the file's `doctor_allows` slice.
7. Create the codemod `comment_prose.rs`; wire under `lazuli fix --rule LZI-COMMENT-PROSE-001`; test (delete-empty, delete-restating-duplicate, idempotent, never-deletes-carve-out).
8. Run `cargo test --workspace` (FULL sweep).
9. MIGRATE/verify pilots: on hostpoint + pauta-web (already on 0028's node waivers), run `lazuli doctor` and inspect `LZI-COMMENT-PROSE-001` findings; tune `PROSE_MIN_WORDS`/carve-outs until the rule is signal (not a false-positive storm); run `lazuli fix --rule LZI-COMMENT-PROSE-001` for the mechanical cases; relocate remaining flagged prose into the right channel by hand (proving the policy). Then `lazuli generate go .` (gate passes — WARNING doesn't block) + `go build ./...` clean in both.
10. TEACH: write the channel-policy table + the rule row in `comment-hygiene.md`; add the scaffold bullet to BOTH `.tmpl`s (byte-identical region).

## Tests first (TDD)
- [ ] `comment_prose::fires_on_sentence` — `# This resource stores the customer's billing address.` → finding.
- [ ] `comment_prose::fires_on_four_word_body` — `# stores the billing address` (no terminal punct) → finding.
- [ ] `comment_prose::silent_on_short_marker` — `# TODO: wire auth` → no finding.
- [ ] `comment_prose::silent_on_file_header` — single header comment at line 1 → no finding; a SECOND header-shaped comment lower in the file → finding.
- [ ] `comment_prose::silent_on_doctor_allow_node` — a `@doctor.allow(X, reason: "...")` line (present in `allows`) → no finding.
- [ ] `comment_prose::silent_on_legacy_doctor_allow` — `# doctor:allow X — reason "..."` (code in `allows`) → no finding.
- [ ] `comment_prose::does_not_double_flag_divider` — `# ========` is owned by NOISE-001, PROSE-001 stays silent.
- [ ] `comment_prose::silent_on_dotted_identifier` — `# query.list` (single token, no prose) → no finding.
- [ ] `comment_prose::clean_pilot_shape_quiet` — a representative clean post-0028 `.lzi` → no findings.
- [ ] `comment_prose::iron_hand_escalates_to_error` — severity resolves WARNING under default profile, ERROR under `DoctorProfile::IronHand`.
- [ ] `comment_prose::never_gates` — even at ERROR (iron-hand), LziHygiene stays out of the blocking set (assert via `is_blocking_category(LziHygiene) == false`).
- [ ] `fix::comment_prose::deletes_empty_hash_line` + `deletes_restating_duplicate` + `keeps_real_prose` + `keeps_carveouts` + `idempotent`.

## Gate
`cargo test --workspace` green **and** hostpoint + pauta-web `lazuli generate go .` gate-pass (WARNING `LZI-COMMENT-PROSE-001` does NOT block) + `go build ./...` clean in both, with the rule tuned to fire on genuine prose and stay quiet on the cleaned pilots **and** `docs/lazuli_way/comment-hygiene.md` carries the channel-policy table + rule row **and** both scaffold `.tmpl`s carry the guidance bullet.

### Definition of Done (the repo's governing rule — embedded)
1. BUILD — `cargo test --workspace` green (FULL sweep).
2. MIGRATE — both pilots: `lazuli fix` + hand-relocation clears mechanical/genuine findings; `lazuli generate go .` gate-passes; `go build ./...` clean.
3. TEACH — `comment-hygiene.md` channel policy + rule row; scaffold CLAUDE.md/AGENTS.md bullet.
4. ENFORCE — `LZI-COMMENT-PROSE-001` fires on a prose `#` line and is silent on the carve-outs + the cleaned pilots.
Plus: diagnostics-registry bridge for the new doctor code; module_headers trigger-cue on the new rule module. (No keyword change → no parser↔registry parity / xtask keyword-reference work.)

## Risks & rollback
- False-positive storm on real pilots → mitigation: WARNING default, `@doctor.allow` escape, tune `PROSE_MIN_WORDS`/carve-outs against pilots BEFORE commit; the `clean_pilot_shape_quiet` test pins the cleaned shape.
- Heuristic flags a legitimate `# TODO:` long note → mitigation: marker carve-out is word-budgeted; a long TODO is genuinely prose and should move to `.ctx.md` — acceptable.
- Iron-hand ERROR surprises a pilot CI → mitigation: LziHygiene never gates (gate.rs:110); ERROR is visible, not blocking; the `never_gates` test pins it.
- Codemod deletes a comment that wasn't a pure restatement → mitigation: subset-of-construct-tokens check is strict; `keeps_real_prose` + `keeps_carveouts` tests; pilots diffed after.
- Depends on 0028's `Module.doctor_allows` not yet merged → mitigation: `depends_on: [0028]`, `parallel_safe: false`; do not start until 0028's IR seam is in tree.

**Rollback:** `git revert` the commit — the rule is additive + non-gating + WARNING-default, so reverting removes findings with zero pilot build impact. Revert any pilot `lazuli fix` commits separately if needed.
