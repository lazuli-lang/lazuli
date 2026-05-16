# Lazuli Audit Skill — Limitations

Normative reference. Lists what this MVP cannot do, organized by global limitations and per-rule false-negative / false-positive classes. Mirrors `docs/proposals/audit-skill-mvp.md` §4.4.

## Global limitations

- **No cross-file resolution.** The skill sees one `.lzi` at a time. Rules that depend on resolving `uses account.Gender` to `account.lzi:N` are out of scope. Use `lazuli doctor` from the CLI for those.
- **No IR walking.** The skill grades by pattern-matching raw text. The doctor walks a parsed IR; the skill walks a string blob. Edge cases where the parser disambiguates better than the LLM (e.g., a `command archive_post` inside a comment) MAY produce false positives — mitigated by the comment-strip guard in `SKILL.md` § Behavioral guards.
- **No auto-fix.** The skill emits suggested fixes as prose; it does not patch the file.
- **No new rules.** The 13-rule catalog is fixed at MVP ship time. New rules enter through the doctor catalog first (`crates/lazuli_doctor/src/vocab/`), then mirror here on the next bundle regen.
- **No multi-file aggregate analysis.** The skill cannot detect "feature X has 70%+ handler-heavy commands" if the commands are spread across multiple `.lzi` files. Single-file capsules work; multi-file capsules may underreport.

## Per-rule false-negative classes

Where the skill catches LESS than the doctor. Each entry: rule code / what skill detects / what it misses / fallback path.

- **`VOCAB-HANDLER-HEAVY-001`**
  - Skill detects: `@fn.<name>` direct invocations in raw text (count `@fn.` substrings vs declarative keywords).
  - Skill misses: (a) commands with `effect == CommandEffect::None` (legacy pure-handler path; no `@fn.` substring); (b) typed `external_calls` invocations using the `calls <slot>.<op>` form (no `@fn.` substring).
  - Fallback: `lazuli doctor` for full-fidelity IR-walked detection.

- **`VOCAB-CAP-MISSING-001`**
  - Skill detects: `@pii.<class>` + missing `@cap.*` on the SAME field declaration in one file.
  - Skill misses: missing cap-tagging when the PII inheritance crosses `uses <feature>` boundaries (PII-tagged field lives in feature A; consumer in feature B may be missing the cap on a derived field).
  - Fallback: `lazuli doctor` with cross-feature IR resolution.

- **`VOCAB-EVENT-PRODUCER-001`**
  - Skill detects: in-feature mutating commands that lack `emits` despite the same feature declaring matching events.
  - Skill misses: cross-feature missing producers — event declared in `account`, expected producer lives in `host` or `payments`, no producer anywhere in the cross-feature graph.
  - Fallback: `lazuli doctor` walks the full module graph.

- **`VOCAB-UNION-002`**
  - Skill detects: polymorphic FK pair (`target: Enum + target_id: ID`) when both fields appear in the same resource block.
  - Skill misses: cross-resource polymorphic patterns — discriminator enum on resource A, FK on resource B that should compose into a typed union.
  - Fallback: `lazuli doctor` for resource-graph-level detection.

## Per-rule false-positive classes

Where the skill catches MORE than the doctor.

- **`VOCAB-TESTS-MISSING-001`**
  - Skill detects: any feature with resources or commands and zero `test ` block opens in raw text.
  - Skill catches more: any legacy untouched feature (the doctor's planned false-positive defense — feature-touched-in-last-N-commits filter — is deferred in BOTH the doctor and the skill; both fire on legacy buckets equally).
  - Mitigation: user reviews the finding and applies `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."` once the opt-out walker ships in a follow-up doctor cell.

## Extension template

New per-rule limitations discovered during validation or production use are appended to this file in the same shape. The 4-field shape is closed:

- **False-negative class**: rule code + what skill detects + what it misses + fallback path.
- **False-positive class**: rule code + what skill detects + what it catches more + mitigation.

Adding a new section type (beyond false-negative / false-positive) requires a proposal amendment to `docs/proposals/audit-skill-mvp.md`.

## Relationship to v2 full bundle

These limitations are upgrades that the v2 full bundle (docs-as-IR-projection per memory `project_docs_as_ir_projection_2026-05-15`) absorbs:

| MVP limitation | v2 resolution |
|---|---|
| No cross-file resolution | IR-walked resolution via `crates/codegen-docs` projector |
| No IR walking | Skill regenerates from IR; same authority as `lazuli doctor` |
| No multi-file aggregate analysis | Module-level walks across the full capsule |
| Per-rule false-negative classes | Closed when the projector consumes the structural detection logic from the Rust source |

v2 ships post-pilot stabilization (≥ 3 pilots calibrate the projector). Until then, this MVP is the cement layer — every limitation here is acknowledged, every fallback path is documented.

## Authority

The Rust source at `crates/lazuli_doctor/src/vocab/` is canonical. This file is the projection. If a limitation listed here turns out to be wrong (e.g., the skill actually does catch a pattern this file says it misses), update both this file AND the relevant rule's source comment in lockstep.
