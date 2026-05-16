# Proposal — Audit-Skill MVP (Portable Lint Bundle for User LLMs)

**Status:** L0 v0.3 PASS 8.93/10 (2026-05-16) via `lazuli-language-architect`. See §11 Revision history for the v0.1 → v0.2 → v0.3 trail.
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Driver:** Cement the existing doctor vocab catalog as a portable artifact users can run inside their own LLM harness against their own `.lzi` capsules. Hostpoint capsule (`c:/Users/lucas/hostpoint/app/features/`) is the calibration corpus.
**Depends on:** `crates/lazuli_doctor/src/vocab/` (existing 13-rule catalog as of `mod.rs` commit 8229ec0), `docs/canonical-semantics.md`, `docs/invariants.md`, `docs/design-principles.md`.
**Defers to:** the future docs-as-IR-projection L0 (captured in memory `project_docs_as_ir_projection_2026-05-15`; no proposal file committed yet — the path `docs/proposals/projection-targets-v0.1.md` is reserved). This MVP intentionally ships **before** that bundle lands.
**Honors:** `docs/design-principles.md` Rule Zero (Vocabulary Over Mechanism); `docs/scope-discipline.md` (80/20 boundary); `feedback_cement_over_ship_until_users_2026-05-15` (cement first, ship-loud later); `feedback_normative_not_narrative_2026-05-15` (the skill body is prescriptive, not narrative).

---

## §1. Status & motivation

A **Lazuli audit skill** is a portable LLM bundle (`.skill` for Claude Code, system-prompt template for ChatGPT / Cursor, plain-prompt fallback) that loads a user's `.lzi` capsule plus a hardcoded rubric and emits findings: vocabulary violations, missing audit declarations, untyped JSON bags, orphan events, capability-missing PII fields, etc. The full vision is captured in memory `project_audit_skill_idea_2026-05-15`. The **L0 plan** for that vision depended on `projection-targets-v0.1` (docs + diagrams projected from the IR — memory `project_docs_as_ir_projection_2026-05-15`): in the long-run, the audit skill consumes the same projected rule corpus the LSP serves on hover, plus a cookbook with promotion-to-lint mechanics.

That full bundle is not yet ready (memory `project_docs_as_ir_projection_2026-05-15` enumerates 11 pre-conditions, most still open; bundle approval gates on 3-pilot corpus stabilization).

This proposal locks an **MVP path** that ships before the bundle:

| Aspect | Original L0 (full bundle) | MVP (this proposal) |
|---|---|---|
| Source of rules | Auto-projected from IR via `crates/codegen-docs` (not built) | Hardcoded markdown bundle, one section per existing doctor rule |
| Rule corpus | All doctor rules + cookbook + invariants + error catalog | 13 rules — full `crates/lazuli_doctor/src/vocab/` catalog as published in `mod.rs` (orphan file `vocab_lifecycle_001.rs` not in scope; it's not wired into the catalog yet) |
| Cross-file resolution | IR-walked symbol resolution (`uses account` → `account.lzi:N`) | None — raw `.lzi` text grading per-file |
| Validation corpus | 3 pilots (Hostpoint + Pleiades + Atelier) | 1 pilot — Hostpoint capsules in `c:/Users/lucas/hostpoint/app/features/` |
| Distribution shape | Claude skill + Cursor rules + GPT custom GPT + plain prompt | One markdown bundle + one `SKILL.md` for Claude Code |
| Versioning rhythm | `.lazuli/canonical/v<X>/` projected per release | Single versioned file in repo: `skills/audit/SKILL.md` |
| Cookbook promotion mechanic | `pattern_count` frontmatter + `doctor scan` + auto-proposal | None — promotion happens via this proposal's manual review loop |
| Doc-quality-as-test | CI advisory grading per kind | None — narrative quality reviewed at proposal grade time |
| Ships | Post-pilot stabilization (≥ 3 pilots calibrate the threshold) | This wave (2026-05-16…2026-05-23) |

**Why ship MVP first:**

1. **Cement the catalog, not the projector.** The 13 vocabulary rules at [crates/lazuli_doctor/src/vocab/mod.rs:13-26](crates/lazuli_doctor/src/vocab/mod.rs#L13-L26) are the load-bearing artifact. They have been validated through the Pleiades / Erudito / Atelier handler audit (memory `project_handler_audit_lints_2026-05-14`, `project_product_vocab_audits_2026-05-14`) and through the Hostpoint cruel re-review (memory `project_vocab_audit_findings_2026-05-14`). The two newest rules (`VOCAB-HANDLER-HEAVY-001`, `VOCAB-TESTS-MISSING-001`) shipped in commits `cf82472`, `e0c190b`, `8229ec0`. The catalog is **stable enough** to expose as a portable artifact.
2. **Cement-over-ship posture** per memory `feedback_cement_over_ship_until_users_2026-05-15` — the MVP is not a marketing push. It is a forcing function: every rule that exists in `crates/lazuli_doctor/src/vocab/` MUST also exist in the skill bundle, with the same code, same trigger, same fix. Divergence is a bug, not a feature. This is the cement layer.
3. **Hostpoint calibration corpus is available.** `c:/Users/lucas/hostpoint/app/features/host/host.lzi` + `c:/Users/lucas/hostpoint/app/features/account/account.lzi` are the **active pilot** per memory `project_strategic_pivot_2026-05-15`. The handler-heavy refactor at memory `project_handler_audit_lints_2026-05-14` was driven by `host.lzi`'s 5/5 handler-heavy ratio. The skill MVP is graded against the **already-known-correct** doctor output for these two capsules; any divergence is a skill-fidelity bug.
4. **The full bundle takes months.** The projection-targets v0.1 L0 needs 11 pre-conditions resolved + 3 pilots stabilized + cookbook corpus calibrated. The skill MVP is a 1-week effort. Decoupling the timelines preserves both: the MVP cements while the full bundle matures.

**Boundary statement:**

> The audit skill is a **mirror** of the doctor catalog into a portable LLM-readable artifact. The skill body has zero authority over rule definitions; the doctor source is canonical. The skill MVP grades **raw `.lzi` text** by pattern-matching, not by walking the IR. Any rule that requires cross-file symbol resolution (e.g., `uses account` import-graph walks) is **deferred** to v2 (the full bundle, IR-walked). Some rules in the MVP catalog have **acknowledged false-negative classes** on raw text — these are enumerated in `LIMITATIONS.md` (§4.4) per rule, so consumers know the skill's coverage gap vs the doctor.

---

## §2. Scope

### In scope

1. **A portable LLM skill bundle** (`skills/audit/`) checked into the repo, containing:
   - `SKILL.md` — top-level Claude Code skill manifest with `name`, `description`, invocation triggers.
   - `RULES.md` — the hardcoded rule catalog, one section per rule (13 rules, mirroring all declared rules in `crates/lazuli_doctor/src/vocab/mod.rs`).
   - `INVOCATION.md` — how to load the skill in non-Claude harnesses (Cursor rule pack, ChatGPT system prompt template, plain-prompt fallback).
   - `LIMITATIONS.md` — explicit list of what the MVP cannot do.
2. **Rule catalog** (§5) mirroring all 13 doctor rules declared in [crates/lazuli_doctor/src/vocab/mod.rs:14-26](crates/lazuli_doctor/src/vocab/mod.rs#L14-L26). Includes both lints shipped this wave (`VOCAB-HANDLER-HEAVY-001` `e0c190b`, `VOCAB-TESTS-MISSING-001` `cf82472`); each carries acknowledged false-negative classes in `LIMITATIONS.md`. The orphan file `vocab_lifecycle_001.rs` is NOT a shipped rule (not in mod.rs) — out of MVP scope.
3. **Hostpoint validation procedure** (§6) — concrete recipe to run the skill against `c:/Users/lucas/hostpoint/app/features/host/host.lzi` and `account/account.lzi`, diff against the doctor's known-good output, treat divergence as a skill-fidelity bug to fix.
4. **Versioning convention** — the skill bundle pins to the Lazuli release tag (`lazuli@0.X` → `skills/audit/v0.X/`). One bundle per release. No live network fetch.
5. **Explicit difference table from v2** (§7) so future contributors don't conflate the MVP with the eventual full audit skill.

### Non-goals

1. **No docs-as-IR-projection.** The projector crate `crates/codegen-docs` is not built. The skill MVP ingests hand-curated markdown. Deferred to v2; see memory `project_docs_as_ir_projection_2026-05-15`.
2. **No cross-file symbol resolution.** Rules that require walking `uses <feature>` import graphs (e.g., "is `Gender` resolvable from `account` in `host`?") are excluded. The doctor itself handles those server-side; the skill MVP graded raw text only.
3. **No IR walking.** The skill receives a string blob (`.lzi` text) plus the rule rubric. It does not parse the IR. This is the MVP shortcut.
4. **No auto-fix mode.** The skill emits findings + suggested fixes as prose; it does not patch the file. A future `audit-skill-autofix` L0 may revisit this once findings are stabilized.
5. **No new rules invented by the skill.** Every rule in `RULES.md` mirrors an existing doctor rule. Inventing a heuristic that is not in `crates/lazuli_doctor/src/vocab/` is a scope violation. New rules enter through the doctor catalog first (Rule Zero compliance).
6. **No SaaS audit dashboard, no API, no hosted service.** The skill is a text bundle. Distribution is by checking out the repo or copying the `skills/audit/` directory. Same posture as memory `project_audit_skill_idea_2026-05-15` "Do NOT build hosted SaaS — distribute as a portable bundle."
7. **No runtime impact.** Zero Rust crate touches. Zero Go runtime touches. Zero codegen emitter touches. Pure markdown + a small validation script.
8. **No code in `crates/`.** The MVP is a text artifact, not a compiler change. Future v2 changes the projector (`crates/codegen-docs`); MVP touches `skills/`, `docs/`, and at most a CI workflow.
9. **No promotion-to-lint mechanic.** The full bundle has `pattern_count` frontmatter + `doctor scan` + auto-proposal. The MVP has none of that — the rule list is fixed at MVP ship time.
10. **No multi-language bundle.** Skill prose is English. Internationalization deferred to v2+ (the canonical user-facing audit channel is the user's own LLM, which handles localization).

---

## §3. Why MVP before bundle

Two structural reasons + one operational reason.

### §3.1 Structural — the catalog is the cement layer

Rule Zero from `docs/design-principles.md:8-27` says Lazuli grows by adding **shared vocabulary**, not user-defined mechanism. The 13 vocabulary rules declared at `crates/lazuli_doctor/src/vocab/mod.rs:14-26` ARE the vocabulary lattice: `VOCAB-AUDIT-001` cements "every mutating command needs an `audit` child"; `VOCAB-UNION-001` cements "enum + correlated-optional-fields → use `union`"; etc. Each rule encodes a **catalog-fixed semantic**, not a heuristic.

Distributing the catalog as a portable artifact does **not** add new vocabulary. It exposes the existing vocabulary in a second surface (markdown for LLMs) on top of the canonical surface (Rust source + `DoctorDiagnostic` adapter). This is the same shape as `docs/canonical-semantics.md` exposing the IR's closed-namespace catalog to humans — a parallel projection, not a new mechanism.

If the projection itself becomes a feature with vocabulary (e.g., the skill invents rules the doctor doesn't have), Rule Zero is violated. The MVP's "no new rules invented by the skill" non-goal (§2 item 5) is the structural guard.

### §3.2 Structural — the projector is not load-bearing for MVP value

Memory `project_docs_as_ir_projection_2026-05-15` lays out an 11-pre-condition path to project docs from the IR. Most of those pre-conditions (e.g., `narrative` field on every kind, `diagram_hints` per kind, plugin doc authoring contract, doc-quality-as-test in CI) are **valuable** but not the **load-bearing** part of the audit-skill value. The load-bearing part is: a user opens their LLM, points it at their `.lzi`, gets findings.

Hand-curating a 13-rule markdown bundle replicates ~80% of MVP value with ~5% of the engineering cost. The 80% number is calibrated empirically: the existing doctor rules are what catch the vast majority of vocabulary violations in the Pleiades + Atelier + Erudito audits (memory `project_handler_audit_lints_2026-05-14`, `project_product_vocab_audits_2026-05-14`). The remaining 20% — cross-file references, IR-walk-driven heuristics, projected error explanations — is what the full bundle adds.

Shipping MVP first proves the value loop (user → skill → finding → fix) **before** investing in the projector infrastructure. If the value loop is broken (e.g., users don't run the skill, findings are too noisy, the markdown rubric is too sparse), we discover it cheaply.

### §3.3 Operational — Hostpoint is the active pilot

Memory `project_strategic_pivot_2026-05-15` documents the pivot to Hostpoint as the **active** Lazuli pilot. The capsule lives at `c:/Users/lucas/hostpoint/app/features/` with at minimum `host/host.lzi` and `account/account.lzi`. The handler audit at memory `project_handler_audit_lints_2026-05-14` was driven by `host.lzi`'s 5/5 handler-heavy ratio.

The MVP **validates** against this capsule. The skill is correct when its findings match the doctor's findings byte-for-byte on `host.lzi` + `account.lzi`. Any divergence is a skill-fidelity bug. This gives the MVP a **mechanical** ship gate: not "does the prose sound right?" but "does the LLM running this rubric reproduce the doctor's output?"

The Hostpoint corpus is private (memory `project_public_vs_private_repo`). The repo's validation script must NOT commit the Hostpoint capsules into the public Lazuli repo. The validation is run locally by the lead before each skill version ships.

---

## §4. Skill shape

The skill bundle is a directory at `skills/audit/` with the following layout. Every file is markdown. No code.

```
skills/audit/
├── SKILL.md          # Claude Code skill manifest (name, description, triggers)
├── RULES.md          # The rule catalog (§5)
├── INVOCATION.md     # How to load in Claude Code / Cursor / ChatGPT / plain-prompt
├── LIMITATIONS.md    # What the MVP cannot do (per §2 non-goals)
└── EXAMPLES/         # Optional — copy-able .lzi snippets that trigger each rule
    ├── audit-001.lzi
    ├── union-001.lzi
    └── …
```

### §4.1 `SKILL.md` shape

The Claude Code skill manifest follows the format expected by the harness's skill loader. Approximate body:

```markdown
---
name: lazuli-audit
description: Use when the user wants to grade a .lzi capsule against the Lazuli vocabulary catalog. Surfaces missing audit declarations, untyped JSON bags, orphan events, capability-missing PII fields, handler-heavy features, missing tests, and other catalog violations. Loads the canonical 13-rule rubric from RULES.md. Reads raw .lzi text — does NOT walk the IR (use `lazuli doctor` from the CLI for IR-walked analysis).
---

# Lazuli Audit Skill

The user has authored .lzi capsules and wants vocabulary-level findings. Load `RULES.md` and walk each rule against the user-provided .lzi text. For each finding, emit:

- Rule code (e.g., `VOCAB-AUDIT-001`).
- Source location (file + line approximation from the text).
- Trigger (what pattern fired the rule).
- Suggested fix (the canonical replacement from the catalog).
- Severity tier (`warning` / `error`).

If the user has not provided .lzi text, ask for it explicitly. Do not invent .lzi content.

This skill mirrors the `lazuli doctor` vocabulary rules at `crates/lazuli_doctor/src/vocab/` in the Lazuli framework repo. The skill is not authoritative — the doctor is. If a divergence appears between skill finding and doctor finding, file the divergence as a skill-fidelity bug.

**Some rules have per-rule false-negative classes** documented in LIMITATIONS.md (e.g., `VOCAB-HANDLER-HEAVY-001` only detects `@fn.` direct invocations on raw text; misses typed `external_calls` and legacy `CommandEffect::None` paths). When emitting findings for those rules, surface the limitation to the user: "this skill caught the common case; for full fidelity run `lazuli doctor`".

See LIMITATIONS.md for what this MVP cannot do (cross-file resolution, IR walking, auto-fix) and the per-rule gap details.
```

### §4.2 `RULES.md` rule entry shape

Each rule in `RULES.md` follows a strict template, mirroring the existing doctor source comments at the top of files like [crates/lazuli_doctor/src/vocab/vocab_audit_001.rs:1-11](crates/lazuli_doctor/src/vocab/vocab_audit_001.rs#L1-L11):

```markdown
## VOCAB-AUDIT-001 — mutating command without an explicit `audit` child

**Severity:** `warning` (strict-profile), `error` (production-profile).
**Source:** crates/lazuli_doctor/src/vocab/vocab_audit_001.rs
**Reference:** docs/invariants.md:93-97

### Trigger

A `command.*` block carries a write effect (`creates`, `updates`, `deletes`)
or emits events but does NOT declare any of the three audit forms:

- `audit default` — log all default fields
- `audit <field>, <field>` — log specific fields
- `audit none` — explicit opt-out (with a reason in comments)

### Example (5-line snippet that triggers the rule)

```lzi
feature publishing
  resource Publication
    title: Text required

  command archive_post
    deletes Publication                  # ← Doctor: VOCAB-AUDIT-001
    # missing: audit default | audit <fields> | audit none
```

### Fix

Add one of the three audit forms:

```lzi
  command archive_post
    deletes Publication
    audit default                         # ← canonical
```
```

The five-line example is canonical. It must (a) parse as valid `.lzi`, (b) fire the rule, (c) be self-contained (no `uses <feature>` cross-file references — the MVP is single-file).

### §4.3 `INVOCATION.md` — how to load

Three invocation paths, all hand-written markdown:

1. **Claude Code skill** — drop `skills/audit/` into the user's `~/.claude/skills/` (or per-project `.claude/skills/`). Invoke via `/lazuli-audit` slash command or by referencing the skill in a prompt.
2. **Cursor rule pack** — copy `RULES.md` into the project's `.cursor/rules/lazuli-audit.mdc`. Cursor loads it as a global rule that fires on `.lzi` file context.
3. **ChatGPT / plain-prompt fallback** — paste the full `RULES.md` as a system prompt or user prompt prefix, then paste the `.lzi` content as a follow-up. Same rubric, no harness-specific features.

The MVP commits to **all three paths** because the rule body is plain markdown — the only delta is the wrapper file. `INVOCATION.md` documents each path with a copy-pasteable example.

### §4.4 `LIMITATIONS.md` — what the MVP cannot do

A normative file (per `feedback_normative_not_narrative_2026-05-15`) listing the MVP's hard constraints. Approximate body:

**Global limitations:**

- **No cross-file resolution.** The skill sees one `.lzi` at a time. Rules that depend on resolving `uses account.Gender` to `account.lzi:N` are out of scope. Use `lazuli doctor` from the CLI for those.
- **No IR walking.** The skill grades by pattern-matching raw text. The doctor walks a parsed IR; the skill walks a string blob. Edge cases where the parser disambiguates better than the LLM (e.g., a `command archive_post` inside a comment) MAY produce false positives.
- **No auto-fix.** The skill emits suggested fixes as prose; it does not patch the file.
- **No new rules.** The 13-rule catalog is fixed at MVP ship time. New rules enter through the doctor catalog first.
- **No multi-file aggregate analysis.** The skill cannot detect "feature X has 70%+ handler-heavy commands" if the commands are spread across multiple `.lzi` files (Hostpoint's host.lzi is single-file, so this works for the calibration corpus; other capsules may not).

**Per-rule false-negative classes** (where the skill catches LESS than the doctor). Each entry follows the 4-field closed shape below.

- **`VOCAB-HANDLER-HEAVY-001`**
  - Skill detects: `@fn.<name>` direct invocations in raw text (count `@fn.` substrings vs declarative keywords).
  - Skill misses: (a) commands with `effect == CommandEffect::None` (legacy pure-handler path; no `@fn.` substring); (b) typed `external_calls` invocations using the `calls <slot>.<op>` form (no `@fn.` substring).
  - Fallback: `lazuli doctor` for full-fidelity IR-walked detection.

- **`VOCAB-CAP-MISSING-001`**
  - Skill detects: `@pii.<class>` + missing `@cap.*` on the SAME field declaration in one file.
  - Skill misses: missing cap-tagging when the PII inheritance crosses `uses <feature>` boundaries (the PII-tagged field lives in feature A; the consumer in feature B may be missing the cap on a derived field).
  - Fallback: `lazuli doctor` with cross-feature IR resolution.

- **`VOCAB-EVENT-PRODUCER-001`**
  - Skill detects: in-feature mutating commands that lack `emits` despite the same feature declaring matching events.
  - Skill misses: cross-feature missing producers — event declared in `account`, expected producer lives in `host` or `payments`, no producer anywhere in the cross-feature graph.
  - Fallback: `lazuli doctor` walks the full module graph.

- **`VOCAB-UNION-002`**
  - Skill detects: polymorphic FK pair (`target: Enum + target_id: ID`) when both fields appear in the same resource block.
  - Skill misses: cross-resource polymorphic patterns — discriminator enum on resource A, FK on resource B that should compose into a typed union.
  - Fallback: `lazuli doctor` for resource-graph-level detection.

**Per-rule false-positive classes** (where the skill catches MORE than the doctor):

- **`VOCAB-TESTS-MISSING-001`**
  - Skill detects: any feature with resources or commands and zero `test ` block opens in raw text.
  - Skill catches more: any legacy untouched feature (the doctor's planned false-positive defense — feature-touched-in-last-N-commits filter — is deferred in BOTH the doctor and the skill; both fire on legacy buckets equally).
  - Mitigation: the user reviews the finding and applies `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."` once the opt-out walker ships in a follow-up cell.

These limitations are upgrades that the v2 full bundle would absorb. Recording them explicitly per-rule prevents the MVP from being mistaken for the full thing and tells consumers exactly when to fall back to `lazuli doctor`.

**Extension template:** new per-rule limitations discovered during D.3 validation (or in production use) are appended to this file in the same shape — one bullet per rule under either "Per-rule false-negative classes" (rule code + what skill detects + what it misses + fallback path) or "Per-rule false-positive classes" (rule code + what skill detects + what it catches more + mitigation). The 4-field shape is closed. Adding a new section type requires a proposal amendment.

---

## §5. Rule catalog (MVP set)

Thirteen rules. All mirror the existing doctor catalog at [crates/lazuli_doctor/src/vocab/mod.rs:14-26](crates/lazuli_doctor/src/vocab/mod.rs#L14-L26). Includes both lints shipped this wave (`VOCAB-HANDLER-HEAVY-001`, `VOCAB-TESTS-MISSING-001`). The orphan file `vocab_lifecycle_001.rs` is not declared in `mod.rs` and is NOT included in the MVP — it's a staged file pending wire-up (see Note below).

| # | Code | Source file | Trigger (1-line) | Severity |
|---|---|---|---|---|
| 1 | `VOCAB-AUDIT-001` | [vocab_audit_001.rs](crates/lazuli_doctor/src/vocab/vocab_audit_001.rs) | Mutating command (`creates`/`updates`/`deletes` or `emits`) without an explicit `audit` child. | warning/error |
| 2 | `VOCAB-AUDIT-002` | [vocab_audit_002.rs](crates/lazuli_doctor/src/vocab/vocab_audit_002.rs) | Handler-only command invalidates a resource carrying sensitive `@cap.*` fields without an `audit` child. | warning/error |
| 3 | `VOCAB-CAP-MISSING-001` | [vocab_cap_missing_001.rs](crates/lazuli_doctor/src/vocab/vocab_cap_missing_001.rs) | `@pii.<class>` field without `@cap.Hashed`/`@cap.Encrypted`/`@cap.E2ee`/`@cap.Token`. | error |
| 4 | `VOCAB-DERIVED-READ-001` | [vocab_derived_read_001.rs](crates/lazuli_doctor/src/vocab/vocab_derived_read_001.rs) | Optional field never written by any command/job — likely `derived from <expr>`. | warning |
| 5 | `VOCAB-EVENT-ORPHAN-001` | [vocab_event_orphan_001.rs](crates/lazuli_doctor/src/vocab/vocab_event_orphan_001.rs) | Event declared but no command/job in the feature emits it. | warning |
| 6 | `VOCAB-EVENT-PAYLOAD-001` | [vocab_event_payload_001.rs](crates/lazuli_doctor/src/vocab/vocab_event_payload_001.rs) | `emits <event>` references an event without `payload <Type>` or `payload none`. | warning |
| 7 | `VOCAB-EVENT-PRODUCER-001` | [vocab_event_producer_001.rs](crates/lazuli_doctor/src/vocab/vocab_event_producer_001.rs) | Mutating command lacks `emits` even though the feature declares matching events. | warning |
| 8 | `VOCAB-GRAMMAR-FORM-001` | [vocab_grammar_form_001.rs](crates/lazuli_doctor/src/vocab/vocab_grammar_form_001.rs) | Deprecated grammar forms (`validates resource @validator.X`, inline `previously`, legacy `validate "./path.go"`). | warning/error |
| 9 | `VOCAB-JSON-TYPED-001` | [vocab_json_typed_001.rs](crates/lazuli_doctor/src/vocab/vocab_json_typed_001.rs) | Resource has `JSON` field + sibling closed-catalog enum that is not referenced anywhere. | warning |
| 10 | `VOCAB-UNION-001` | [vocab_union_001.rs](crates/lazuli_doctor/src/vocab/vocab_union_001.rs) | Resource has enum + correlated-optional-fields (variant-name prefix heuristic) — should be a `union`. | warning |
| 11 | `VOCAB-UNION-002` | [vocab_union_002.rs](crates/lazuli_doctor/src/vocab/vocab_union_002.rs) | Polymorphic FK pair (`target: Enum + target_id: ID`) — should be a discriminated union with typed FKs per variant. | warning/error |
| 12 | `VOCAB-HANDLER-HEAVY-001` | [vocab_handler_heavy_001.rs](crates/lazuli_doctor/src/vocab/vocab_handler_heavy_001.rs) | Feature with ≥3 commands and ≥70% routing through `@fn`-style handlers (vs declarative `creates`/`updates`/`deletes`/`returns`). Raw-text MVP has acknowledged false-negative class — see Note below + `LIMITATIONS.md`. | warning |
| 13 | `VOCAB-TESTS-MISSING-001` | [vocab_tests_missing_001.rs](crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs) | Feature declares resources or commands but zero `test` blocks anywhere in the feature. Shipped v0 is the simple form (no opt-out parsing yet; no git-touched filter). Raw-text grep for `test ` block opens is feasible. | warning |

**Note on `VOCAB-LIFECYCLE-001`:** the orphan file at [vocab_lifecycle_001.rs](crates/lazuli_doctor/src/vocab/vocab_lifecycle_001.rs) exists in the crate but is NOT declared in `mod.rs` (`crates/lazuli_lsp/src/lib.rs:15449-15452` documents this as "pending extraction wave"). Therefore it's not a shipped rule today, and out of MVP scope by definition. When the wire-up cell lands, this rule joins the MVP catalog only if its detection is feasible on raw text (uncertain — the rule needs a structural pass over multiple commands).

**Note on `VOCAB-HANDLER-HEAVY-001` false-negative class:** the shipped Rust rule fires on `Command.effect == CommandEffect::None` (legacy pure-handler path) and on `Command.external_calls` non-empty (`calls <slot>.<op>` form), neither of which has a `@fn.` substring. The skill MVP's raw-text heuristic — count `@fn.` substrings vs declarative keywords — will systematically miss those cases. `LIMITATIONS.md` (§4.4) records this gap explicitly. The skill catches the common case (`@fn.<name>` direct invocation) and an audit-skill consumer who needs full fidelity for handler-heavy detection must fall back to `lazuli doctor` CLI.

That gives **13 rules** in v1.0 — the full declared catalog. The two lints shipped this wave (`VOCAB-HANDLER-HEAVY-001` in `e0c190b`, `VOCAB-TESTS-MISSING-001` in `cf82472`) are both included; the catalog state at [mod.rs:14-26](crates/lazuli_doctor/src/vocab/mod.rs#L14-L26) is the source of truth.

### §5.1 Rule entry coverage requirements

Every rule in `RULES.md` MUST:

1. Cite the source file in `crates/lazuli_doctor/src/vocab/` with `path` (no line number — file-level cite is sufficient since the source is the canonical authority).
2. Mirror the rule's `Finding::message` and `Finding::CODE` constants verbatim where they appear in the rule body.
3. Include a 5-line `.lzi` snippet that triggers the rule (per §4.2).
4. Include a 5-line `.lzi` snippet that resolves the rule (the canonical fix).
5. Declare the severity tier from the source file (`warning` / `error` / both per profile).

Cell D.2 (§8) is the mechanical extraction of these fields from the Rust source. The skill's authority is the Rust source; the markdown is the projection.

### §5.2 Why these 13 specifically (and what's NOT included)

The 13 mirror-rules cover every rule declared at [crates/lazuli_doctor/src/vocab/mod.rs:14-26](crates/lazuli_doctor/src/vocab/mod.rs#L14-L26). `vocab_lifecycle_001.rs` exists as an orphan file but is not in mod.rs and not in any catalog enumeration consumer (LSP, CLI doctor walker), so it's not a "shipped rule" — see Note in §5.

`VOCAB-HANDLER-HEAVY-001` is included because (a) the most common form — `@fn.<name>` direct invocation — IS detectable from raw text (count `@fn.` substrings vs `creates`/`updates`/`deletes`/`returns` keywords in a feature block), and (b) it is the textbook trigger from the Hostpoint pilot (memory `project_handler_audit_lints_2026-05-14` documents `host.lzi`'s 5/5 handler-heavy ratio as the proposal driver). The shipped Rust rule additionally walks `CommandEffect::None` legacy commands and typed `external_calls` — these forms have no `@fn.` substring and the skill's raw-text path will not detect them. This false-negative class is acknowledged in `LIMITATIONS.md` per rule rather than excluding the rule entirely; skipping `VOCAB-HANDLER-HEAVY-001` would leave the MVP unable to surface its own driving example for the common case.

`VOCAB-TESTS-MISSING-001` is included because the shipped doctor rule (commit `cf82472`) is the simple form: "feature has resources or commands but zero `test` blocks declared anywhere." The full version (opt-out comment parsing + git-touched-in-last-N-commits filter) is explicitly deferred in the rule's own source comment (`vocab_tests_missing_001.rs:10-17`). The skill mirrors what the doctor actually ships today — detecting `test ` block opens in raw text is feasible — and inherits the same false-positive risk on legacy untouched buckets that the doctor inherits. Both downstream concerns belong to v1.1, not v1.0 deferral.

No other doctor rules from outside `crates/lazuli_doctor/src/vocab/` are included. The skill grades **vocabulary** rules only. Diagnostic categories like `lzx-*` (view-level rules) and `cell-*` (cell-binding rules) are out of scope — those are surface-projection lints, not vocabulary lints, and they require the `.lzx` projection to be loaded too. v2 (full bundle) can absorb them.

---

## §6. Hostpoint validation procedure

The MVP's mechanical ship gate. Concrete recipe to run against the Hostpoint capsule (per memory `project_strategic_pivot_2026-05-15`; access path `c:/Users/lucas/hostpoint/app/features/`).

### §6.1 Step-by-step

**Pre-conditions:**
- The Hostpoint capsule exists at `c:/Users/lucas/hostpoint/app/features/host/host.lzi` and `c:/Users/lucas/hostpoint/app/features/account/account.lzi`. Both files have been audited and have known-correct doctor output (memory `project_handler_audit_lints_2026-05-14`).
- `lazuli doctor` is built and reproduces the known-correct findings from those files.
- The skill bundle `skills/audit/` has shipped through cells D.1–D.3 (see §8).

**Validation steps:**

1. **Capture doctor's findings.** Run `lazuli doctor` against `host.lzi` and `account.lzi`. Persist the output in a private fixture (NOT committed to the public repo per memory `project_public_vs_private_repo`) — call it `host.lzi.doctor-findings.txt` and `account.lzi.doctor-findings.txt`.
2. **Run the skill.** Open a Claude Code session in the Hostpoint directory. Invoke the `/lazuli-audit` skill. Paste the contents of `host.lzi`. Capture the skill's findings into `host.lzi.skill-findings.txt`. Repeat for `account.lzi`.
3. **Diff.** Compare `host.lzi.doctor-findings.txt` to `host.lzi.skill-findings.txt`. Expected: the set of rule codes is identical. The exact wording of trigger explanations may differ (skill is prose; doctor is structured), but the **rule code** + **the resource/field/command name** in each finding must match.
4. **Categorize divergences.**
   - **Skill missed a finding the doctor caught** → skill-fidelity bug. The rule entry in `RULES.md` is incomplete or the LLM is misreading the trigger. Fix the rule entry; re-run; converge.
   - **Skill caught a finding the doctor missed** → could be a real finding (doctor has a gap) OR a skill false-positive. Manual triage. False positives downgrade the rule's prose; real findings file a doctor bug.
   - **Skill and doctor disagree on the location** → acceptable for v1 (the skill approximates line numbers; the doctor knows). Not a ship blocker.

**Ship gate:** v1.0 of the bundle ships when, for both `host.lzi` and `account.lzi`:
- Skill's set of distinct rule-codes is a superset of ≥ 90% of doctor's distinct rule-codes (i.e. of the unique rule-codes the doctor surfaces, ≥ 90% also appear at least once in the skill output). Finding-count multiset comparison is not the metric — the LLM may surface one finding per rule while the doctor surfaces multiple per rule, and that's acceptable.
- Zero false-positive rules — the skill must NOT invent findings for rule codes that don't apply, where "rule code" means a code that the doctor catalog declares.

### §6.2 What divergence means (cross-file references)

`host.lzi` references `Gender` from `account` via `uses account` (per memory entry on cross-feature symbol resolution review in `docs/next-checklist.md:27`). The skill MVP **does not** resolve `Gender → account.lzi`. If a rule depends on knowing the type of `Gender`, the skill MAY produce a false negative; this is documented in `LIMITATIONS.md` (§4.4). The doctor handles cross-file resolution server-side. v2 closes this gap.

### §6.3 Frequency

Validation runs **once per skill version bump**. The lead runs the procedure locally before tagging `skills/audit/v0.X` and posting to the public release. No CI gating until the projector exists (v2).

---

## §7. Difference from v2 audit-skill (full bundle)

The MVP and the full bundle share a name and an audience. They diverge in everything else. This table is the canonical reference for future contributors confused about which one they're working on.

| Aspect | MVP (v1, this proposal) | Full (v2, deferred — see memory `project_docs_as_ir_projection_2026-05-15`) |
|---|---|---|
| Rule list source | Hardcoded markdown (`skills/audit/RULES.md`), 13 entries | Auto-generated from IR projector (`crates/codegen-docs`) — every doctor rule + every kind + every error + every invariant |
| Cross-file resolution | None — raw text per file | IR-walked — `uses account.Gender` resolves to `account.lzi:N` |
| Calibration corpus | 1 pilot — Hostpoint (`host.lzi`, `account.lzi`) | 3 pilots — Hostpoint + Pleiades + Atelier per memory `project_three_products_lazuli_dogfood` |
| Distribution shape | Single markdown bundle in repo (`skills/audit/`) | Cookbook + IR-projected docs in `.lazuli/canonical/v<X>/` + agent-facing skill bundle |
| Versioning rhythm | Pinned to Lazuli release tag (one bundle per release) | Two rhythms (canonical pinned per release; project regenerated per `lazuli build`) per memory `project_docs_as_ir_projection_2026-05-15` correction 2 |
| Promotion-to-lint mechanic | None — fixed list at ship time | `pattern_count` frontmatter + `doctor scan` + auto-proposal at threshold per memory §"Refinement 1" |
| Doc-quality-as-test | None — quality reviewed at proposal-grade time | Advisory CI grading per kind per memory §"Refinement 1" |
| Security opt-out | None needed — no IR data flows out | Default-deny for `@cap.Encrypted`/`@pii.credential`/`@cap.Token`/`@cap.Hashed` per memory §"Refinement 2" |
| LSP integration | None — skill body is the only surface | LSP `textDocument/hover` serves projected doc inline per memory §"The architecture" |
| Ships | This wave (≈ 1-week effort) | Post-pilot stabilization + 11 pre-conditions resolved (multi-month effort) |

The MVP **does not block** the full bundle and **does not preempt** any of its design decisions. When v2 ships, the projector regenerates `RULES.md` from the IR; `SKILL.md`, `INVOCATION.md`, and `LIMITATIONS.md` move from hand-curated to projector-emitted. **`EXAMPLES/*.lzi` stays as the snapshot-test corpus** — those 13 snippets are user-authored fixtures the snapshot test (`crates/lazuli_doctor/tests/examples_snapshot.rs`) drives against `lazuli doctor` to assert per-rule fidelity. They are NOT IR-projectable (they're inputs, not outputs of the projector), so v2's projector regenerates `RULES.md` only; `EXAMPLES/` is untouched.

---

## §8. Implementation cells

Four cells. The full skill bundle is mostly mechanical text extraction.

| Cell | Owner | Scope | Risk |
|---|---|---|---|
| **D.1** | Claude (orchestrator) | Author `skills/audit/SKILL.md`, `INVOCATION.md`, `LIMITATIONS.md`. These are the static framing files; no per-rule content. Includes the cite to `crates/lazuli_doctor/src/vocab/mod.rs` as the source of truth. | Low (judgement work — Claude only) |
| **D.2** | Codex (serial single-file) | Author `skills/audit/RULES.md` by extracting the rule sections from the 13 declared `crates/lazuli_doctor/src/vocab/vocab_*.rs` files (all rules in `mod.rs:14-26`). For each rule: copy the `Finding::message` and `Finding::CODE` constants; copy the source comment header (`//! …`) verbatim into the trigger description; author a minimal `.lzi` example per §4.2 by adapting tests in the source `#[cfg(test)]` blocks. **One Codex invocation produces one file containing all 13 rule sections sequentially.** No parallel-internal extraction — the single-output-file constraint from `CLAUDE.md` §"Codex parallel-dispatch reference" rule 4 is honored. | Medium (per-rule extraction; 13 rule sections in one file; ~700-1100 lines of markdown) |
| **D.3** | Claude | Validate against Hostpoint capsules per §6. Run `lazuli doctor` and the skill locally. Diff outputs. File skill-fidelity bugs as TODOs in `RULES.md`. Iterate. **The validation outputs (`*.doctor-findings.txt`, `*.skill-findings.txt`) are NOT committed** — Hostpoint capsules are private per memory `project_public_vs_private_repo`. | Medium (judgement work + capsule access) |
| **D.4** | Claude | Author `skills/audit/EXAMPLES/*.lzi` — 13 minimal `.lzi` snippets, one per rule, that trigger the rule. Each snippet must doctor-flag the correct rule when run through `lazuli doctor`. **CI gating decision: commit to it.** Ship `crates/lazuli_doctor/tests/examples_snapshot.rs` as part of D.4; the test takes each snippet, runs `lazuli doctor`, asserts the expected rule code is in the findings. If the cost of CI tooling overhead is too high, the polish item to remove the test is tracked in `docs/next-checklist.md` — not a runtime decision in this cell. | Medium (test scaffolding) |

### §8.1 Wave layout

One wave. No parallel Codex agents on shared files. Sequencing:

- **D.1 → D.2 → D.3 → D.4** is the strict order. D.2 depends on D.1's framing decisions; D.3 depends on D.2's rule content; D.4 follows D.3's validation findings to ensure the example snippets actually trigger the rules.

D.2 is a **single serial Codex invocation** emitting the whole `RULES.md`. No parallel-internal split — `CLAUDE.md` §"Codex parallel-dispatch reference" rule 4 explicitly prohibits Codex agents writing to the same file in parallel, and that rule applies whether the parallelism is across-cells or internal to one cell's emit pass. Codex owns `RULES.md` exclusively during D.2; Claude does not edit it during D.2.

### §8.2 Out of scope for the cells

- No CI gating for the validation procedure (§6). v1.0 is a manual gate.
- No translation of the bundle into other languages.
- No publishing to a skill marketplace / registry.

---

## §9. References

### §9.1 Source files

- [crates/lazuli_doctor/src/vocab/mod.rs](crates/lazuli_doctor/src/vocab/mod.rs) — module declaration for the 13 declared vocab rules (all 13 included in MVP).
- [crates/lazuli_doctor/src/vocab/vocab_audit_001.rs](crates/lazuli_doctor/src/vocab/vocab_audit_001.rs) — `VOCAB-AUDIT-001`.
- [crates/lazuli_doctor/src/vocab/vocab_audit_002.rs](crates/lazuli_doctor/src/vocab/vocab_audit_002.rs) — `VOCAB-AUDIT-002`.
- [crates/lazuli_doctor/src/vocab/vocab_cap_missing_001.rs](crates/lazuli_doctor/src/vocab/vocab_cap_missing_001.rs) — `VOCAB-CAP-MISSING-001`.
- [crates/lazuli_doctor/src/vocab/vocab_derived_read_001.rs](crates/lazuli_doctor/src/vocab/vocab_derived_read_001.rs) — `VOCAB-DERIVED-READ-001`.
- [crates/lazuli_doctor/src/vocab/vocab_event_orphan_001.rs](crates/lazuli_doctor/src/vocab/vocab_event_orphan_001.rs) — `VOCAB-EVENT-ORPHAN-001`.
- [crates/lazuli_doctor/src/vocab/vocab_event_payload_001.rs](crates/lazuli_doctor/src/vocab/vocab_event_payload_001.rs) — `VOCAB-EVENT-PAYLOAD-001`.
- [crates/lazuli_doctor/src/vocab/vocab_event_producer_001.rs](crates/lazuli_doctor/src/vocab/vocab_event_producer_001.rs) — `VOCAB-EVENT-PRODUCER-001`.
- [crates/lazuli_doctor/src/vocab/vocab_grammar_form_001.rs](crates/lazuli_doctor/src/vocab/vocab_grammar_form_001.rs) — `VOCAB-GRAMMAR-FORM-001`.
- [crates/lazuli_doctor/src/vocab/vocab_handler_heavy_001.rs](crates/lazuli_doctor/src/vocab/vocab_handler_heavy_001.rs) — `VOCAB-HANDLER-HEAVY-001` (commit `e0c190b`).
- [crates/lazuli_doctor/src/vocab/vocab_json_typed_001.rs](crates/lazuli_doctor/src/vocab/vocab_json_typed_001.rs) — `VOCAB-JSON-TYPED-001`.
- [crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs](crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs) — `VOCAB-TESTS-MISSING-001` (commit `cf82472`).
- [crates/lazuli_doctor/src/vocab/vocab_union_001.rs](crates/lazuli_doctor/src/vocab/vocab_union_001.rs) — `VOCAB-UNION-001`.
- [crates/lazuli_doctor/src/vocab/vocab_union_002.rs](crates/lazuli_doctor/src/vocab/vocab_union_002.rs) — `VOCAB-UNION-002`.
- [crates/lazuli_doctor/src/vocab/vocab_lifecycle_001.rs](crates/lazuli_doctor/src/vocab/vocab_lifecycle_001.rs) — orphan file, **not** a shipped rule; not declared in `mod.rs` (per `crates/lazuli_lsp/src/lib.rs:15449-15452` "pending extraction wave"). Out of MVP scope by definition.

### §9.2 Docs

- [docs/design-principles.md:8-27](docs/design-principles.md) — Rule Zero (Vocabulary Over Mechanism).
- [docs/scope-discipline.md](docs/scope-discipline.md) — 80/20 boundary; the audit-skill is a portable projection of the existing catalog, not a new boundary.
- [docs/canonical-semantics.md](docs/canonical-semantics.md) — the closed-namespace catalog the rule body references.
- [docs/invariants.md:93-97](docs/invariants.md) — referenced by `VOCAB-AUDIT-001`'s message.
- [docs/next-checklist.md:19](docs/next-checklist.md) — the bullet "audit-skill MVP scope is narrower than original L0 plan" that this proposal closes.
- [docs/next-checklist.md:11](docs/next-checklist.md) — `VOCAB-TESTS-MISSING-001` (deferred to MVP v1.1).
- [docs/next-checklist.md:17](docs/next-checklist.md) — `VOCAB-HANDLER-HEAVY-001` (included in MVP v1.0).
- [docs/grading-rubric.md](docs/grading-rubric.md) — the proposal will be graded against this rubric by `lazuli-language-architect`.

### §9.3 Memory references (`~/.claude/projects/c--Users-lucas-lazuli/memory/`)

- `project_audit_skill_idea_2026-05-15.md` — the original product idea.
- `project_docs_as_ir_projection_2026-05-15.md` — the deferred v2 design; supersedes the audit-skill's full-bundle dependency.
- `project_strategic_pivot_2026-05-15.md` — Hostpoint as the active pilot (calibration corpus).
- `project_three_products_lazuli_dogfood.md` — the 3-pilot stabilization gate for v2 (Hostpoint + Pleiades + Atelier).
- `project_handler_audit_lints_2026-05-14.md` — Hostpoint `host.lzi`'s 5/5 handler-heavy ratio (textbook trigger for `VOCAB-HANDLER-HEAVY-001`).
- `project_product_vocab_audits_2026-05-14.md` — the audit-driven empirical case that the existing rule catalog is the right MVP set.
- `project_vocab_audit_findings_2026-05-14.md` — Hostpoint cruel re-review evidence.
- `project_public_vs_private_repo.md` — why Hostpoint validation outputs are NOT committed to the public repo.
- `project_comments_are_vocabulary_smell.md` — companion principle (prose-staging-for-lints).
- `feedback_cement_over_ship_until_users_2026-05-15.md` — cement-first posture justifying MVP before full bundle.
- `feedback_normative_not_narrative_2026-05-15.md` — `LIMITATIONS.md` and skill prose are prescriptive.
- `feedback_grade_before_commit.md` — this proposal goes through `lazuli-language-architect` grade-then-fix loop.

---

## §10. Acceptance criteria

L0 PASS condition: this proposal answers, deterministically, the following 10 questions from the proposal text. Every "Yes" or "No" must be anchored in a `path:line` or `§N` reference inside this document.

1. **Does the MVP introduce any new vocabulary, IR types, or `@-namespace` entries?** → **No.** §2 In-scope item 5; §2 Non-goals items 1, 5; §5.2.
2. **Does the MVP depend on the docs-as-IR-projection L0 (memory `project_docs_as_ir_projection_2026-05-15`)?** → **No.** §1 gap table row 1; §3.2; §7 row 1. The MVP intentionally ships **before** that bundle.
3. **How many rules ship in v1.0 of `RULES.md`?** → **13.** §5 table. All 13 rules declared in [crates/lazuli_doctor/src/vocab/mod.rs:14-26](crates/lazuli_doctor/src/vocab/mod.rs#L14-L26).
4. **Which existing doctor rules are excluded from the MVP, and why?** → **None of the shipped rules.** The orphan file `vocab_lifecycle_001.rs` exists but is not in `mod.rs` (per `crates/lazuli_lsp/src/lib.rs:15449-15452`), so it's not a shipped rule to exclude. All 13 declared rules are included; rules with raw-text limitations (notably `VOCAB-HANDLER-HEAVY-001`) carry per-rule false-negative class documentation in `LIMITATIONS.md` (§4.4).
5. **What does the skill bundle physically contain?** → §4. Five top-level files (`SKILL.md`, `RULES.md`, `INVOCATION.md`, `LIMITATIONS.md`, `EXAMPLES/` directory with 13 `.lzi` snippets — one per rule).
6. **How does the MVP validate correctness?** → §6. Run `lazuli doctor` against Hostpoint `host.lzi` + `account.lzi`; run the skill against the same files; diff the rule-code sets. Ship gate metric (§6.1): the skill's distinct rule-code set covers ≥ 90% of the doctor's distinct rule-code set, with zero invented rule codes. Set-cardinality, not finding multiset cardinality. Validation runs locally, NOT in CI, NOT committed to the public repo (per memory `project_public_vs_private_repo`). A public smoke against `examples/full-capsule/` or `examples/marketplace-mini/` MAY run in CI as a follow-up (tracked in `docs/next-checklist.md`).
7. **What runtime impact does the MVP have?** → **None.** §2 Non-goals item 7; §2 Non-goals item 8. Pure markdown artifact in `skills/audit/`.
8. **Is the MVP's rule corpus authoritative over the doctor?** → **No.** §1 Boundary statement; §4.1 SKILL.md body ("the skill is not authoritative — the doctor is"); §5.1 item 1. The skill is a projection; the Rust source is canonical.
9. **What's the cell decomposition for implementation?** → §8. Four cells: D.1 (framing files — Claude), D.2 (RULES.md extraction — Codex), D.3 (Hostpoint validation — Claude), D.4 (example snippets — Claude). One wave, strict sequencing.
10. **What's the difference from the v2 (full) bundle?** → §7. Hand-curated vs IR-projected; raw text vs IR-walked; 1 pilot vs 3 pilots; one rhythm vs two rhythms; no security opt-out vs default-deny for sensitive types; no LSP integration vs hover via projector; ships this wave vs post-pilot-stabilization multi-month effort.

If all 10 answers are mechanical from the proposal text, L0 passes.

### §10.1 Boundary check (per `docs/grading-rubric.md:97-114`)

This proposal must NOT introduce boundary violations. Quick check:

- **Provider-specific names in core syntax?** No. The skill body has no `@plugin/<name>` references; the rule catalog is provider-neutral.
- **`container.lzi` introduced?** No.
- **`workspace.lzi` mandatory for single-app?** No.
- **Magic discovery without `lazuli inspect`/`doctor`/LSP visibility?** No — the skill MIRRORS the doctor's visibility. The doctor is the authoritative surface.
- **Lazuli runtime mechanics pushed into the language layer?** No — the MVP touches `skills/` and `docs/` only; zero Rust crate changes; zero runtime changes.

Boundary clean. Ready for `lazuli-language-architect` grade.

---

## §11. Revision history

- **v0.1 (2026-05-16)** — BLOCK 7.83/10. Four blockers: catalog state mis-anchored (11 rules quoted; reality 13); handler-heavy raw-text infeasibility hand-waved; tests-missing deferral rationale dead (the shipped rule was simpler than assumed); D.2 cell sequencing contradiction.
- **v0.2 (2026-05-16)** — BLOCK 8.34/10. B2/B3/B4 PASS; B1 residual text-drift in 5 spots (§3.1, §7, §8 D.4, §9.1 head, §9.1 file list still on the 11/12 counts).
- **v0.3 (2026-05-16)** — **PASS 8.93/10**. All 5 residual drift spots swept; P1 LIMITATIONS extension template added; P2 D.4 CI gating decision committed; P3 SKILL.md surfaces per-rule false-negative classes explicitly.
