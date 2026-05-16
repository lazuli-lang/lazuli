---
name: lazuli-audit
description: Use when the user wants to grade a .lzi capsule against the Lazuli vocabulary catalog. Surfaces missing audit declarations, untyped JSON bags, orphan events, capability-missing PII fields, handler-heavy features, missing tests, and other catalog violations. Loads the canonical 13-rule rubric from RULES.md. Reads raw .lzi text — does NOT walk the IR (use `lazuli doctor` from the CLI for IR-walked analysis).
---

# Lazuli Audit Skill

The user has authored `.lzi` capsules and wants vocabulary-level findings against the Lazuli canon. Load `RULES.md` and walk each rule against the user-provided `.lzi` text. For each finding, emit:

- **Rule code** (e.g. `VOCAB-AUDIT-001`) — must exist verbatim in `RULES.md`. Never invent codes.
- **Source location** — file + line approximation from the text. If line cannot be determined, omit.
- **Trigger** — what pattern in the user's text fired the rule.
- **Suggested fix** — paste the canonical replacement from `RULES.md` "Example — canonical fix".
- **Severity tier** — `warning` or `error`, per the rule's source file (mirrored in `RULES.md`).

If the user has not provided `.lzi` text, ask for it explicitly. Do not invent `.lzi` content. Do not hallucinate features, resources, commands, fields. The skill grades what the user authored, not what it thinks they meant.

## Authority

This skill **mirrors** the `lazuli doctor` vocabulary rules at `crates/lazuli_doctor/src/vocab/` in the Lazuli framework repo. The Rust source is canonical. **The skill is not authoritative.** If a divergence appears between a skill finding and a doctor finding, the doctor wins. Divergence is a skill-fidelity bug — surface it to the user as "the skill emitted X but `lazuli doctor` would emit Y; treat doctor as canonical".

## Per-rule false-negative classes

Some rules carry acknowledged limitations on raw-text grading. The skill detects the common case but misses structural variants. When emitting a finding for one of these rules, surface the limitation to the user:

> "This skill caught the common case of `VOCAB-<X>-001`. For full fidelity — including cross-file references and IR-walked structural patterns — run `lazuli doctor` from the CLI."

The rules with documented false-negative classes are listed in `LIMITATIONS.md`. Currently: `VOCAB-HANDLER-HEAVY-001`, `VOCAB-CAP-MISSING-001`, `VOCAB-EVENT-PRODUCER-001`, `VOCAB-UNION-002`.

## Behavioral guards

When walking the user's `.lzi` text, apply these guards before pattern-matching:

1. **Strip comment lines.** Lines starting with `#` are comments and do NOT contain code. Skipping them prevents false positives like detecting `command archive_post` inside a comment.
2. **Preserve indentation context.** Lazuli grammar is indentation-based; a `command` declared at column 2 vs column 4 has different semantic meaning. Use indentation to scope which feature/resource a construct belongs to.
3. **Treat string literals as opaque.** Pattern-matching on quoted strings (e.g. `"@fn.some_handler"`) is a false-positive trap. Only match on actual DSL keywords.

## Output format

Emit findings as a structured list. Recommended shape:

```
Found <N> vocabulary violations across <M> rules:

1. VOCAB-AUDIT-001 (warning) — features/billing/billing.lzi:42
   Trigger: command `archive_invoice` updates Invoice but declares no `audit` child
   Fix: add `audit default` (or `audit <fields>` / `audit none "<reason>"`)
   Canonical example: see RULES.md §VOCAB-AUDIT-001

2. ...
```

Group findings by severity (errors first, then warnings). If zero findings, say "No vocabulary violations detected against the 13-rule catalog. Run `lazuli doctor` for IR-walked checks and cross-file resolution."

## Out of scope

This skill does **NOT**:
- Walk the IR (use `lazuli doctor`).
- Resolve cross-feature symbol references (use `lazuli inspect <feature>.<symbol>` once shipped).
- Auto-fix violations (suggest only).
- Invent new rules outside the 13-rule catalog.
- Grade `.lzx` view source (vocabulary rules apply to `.lzi` only).

See `LIMITATIONS.md` for the full per-rule gap details and `INVOCATION.md` for non-Claude harness paths (Cursor, ChatGPT, plain prompt).
