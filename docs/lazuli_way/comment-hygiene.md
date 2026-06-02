# Comment hygiene

> Lazuli way idiom. A `# doctor:allow <CODE> — reason "..."` line is a **first-class, highlighted directive**, not throwaway chatter — treat it as load-bearing source.

## Reach for this

When you must suppress a doctor finding you can't (or shouldn't) fix right now, write the canonical directive — `# doctor:allow <CODE> — reason "<text>"` — on the offending line, and **always carry a reason**. It tells `lazuli doctor` (and any human or agent reading the file) *that* a lint was deliberately suppressed and *why*.

Because it has semantic weight, the VSCode grammar colors it distinctly from a plain `# note` (spec 0006). The em dash `—` is canonical; an ASCII `--` is also accepted. A bare `# doctor:allow <CODE>` (no reason tail) still highlights the directive + code but visibly lacks the `reason`/string scopes — that missing color is the at-a-glance cue for an un-reasoned allow (which spec 0007 enforces semantically).

## Before (hand-rolled) / After (idiomatic)

**Before** — the suppression renders in undifferentiated comment gray, indistinguishable from `# TODO` or `# this is a hack`:

```
# doctor:allow VOCAB-TESTS-MISSING-001 — reason "covered by integration suite"
```

**After** (spec 0006) — the same line, now with each token in its own scope so reviewers and agents can spot a newly-added or un-reasoned allow on sight:

| Token | Scope |
| --- | --- |
| `#` | `punctuation.definition.comment.lazuli` |
| `doctor:allow` | `keyword.control.directive.lazuli` |
| `<CODE>` (`VOCAB-TESTS-MISSING-001`) | `entity.name.tag.lazuli` |
| `reason` | `keyword.control.directive.reason.lazuli` |
| `"<text>"` | `string.quoted.double.lazuli` |

A plain comment (`# just a note`) is unchanged — one flat `comment.line.number-sign.lazuli` scope. This mirrors how `scope override` blocks already dignify their `reason` text.

## Enforced by

- **`test:grammar:lazuli:check`** (snapshot gate) — the grammar regression check, fixture `editors/vscode/tests/grammar/comments.lzi`, fires on any regression of the directive scoping (e.g. a future edit reordering the `comments` patterns so the `#.*$` catch-all eats the directive).

<!-- 0007 co-fills the two semantic rule rows below (e.g. DOCTOR-ALLOW-NO-REASON-001) — append, do not clobber the 0006 content above. -->

- **`DOCTOR-ALLOW-NO-REASON-001`** (advisory — `crates/lazuli_doctor/src/allow_no_reason.rs`) — fires when a `# doctor:allow <CODE>` directive lacks a `— reason "..."` tail (separator `—` / `--` / whitespace, then `reason "<text>"`). It does **not** void the opt-out — the named rule stays suppressed; this is a parallel nudge so every suppression carries an audit trail. Meta-suppress with a file-level `# doctor:allow DOCTOR-ALLOW-NO-REASON-001`. Clean on today's pilots (all current allows are reasoned) — preventive against future un-reasoned (often AI-authored) allows.

- **`LZI-COMMENT-NOISE-001`** (advisory, NEVER gates — `crates/lazuli_doctor/src/lzi_hygiene/lzi_comment_noise.rs`) — generalizes `CONFIG-NOISE-001` onto the `.lzi`/`.lzx` surface: fires on decorative-divider rulers (`# ========`, `# --------`, …) and on comment-dominant files (`comment_lines > semantic_lines`). Honors `# doctor:allow LZI-COMMENT-NOISE-001`. Preventive: the pilots' `.lzi` are already clean (zero dividers) — this is a ceiling on future comment drift, not a present-tense fix.

- **`DOCTOR-ALLOW-NO-REASON-001`** (advisory — `crates/lazuli_doctor/src/allow_no_reason.rs`) — fires when a `# doctor:allow <CODE>` directive lacks a `— reason "..."` tail (separator `—` / `--` / whitespace, then `reason "<text>"`). It does **not** void the opt-out — the named rule stays suppressed; this is a parallel nudge so every suppression carries an audit trail. Meta-suppress with a file-level `# doctor:allow DOCTOR-ALLOW-NO-REASON-001`. Clean on today's pilots (all current allows are reasoned) — preventive against future un-reasoned (often AI-authored) allows.

- **`LZI-COMMENT-NOISE-001`** (advisory, NEVER gates — `crates/lazuli_doctor/src/lzi_hygiene/lzi_comment_noise.rs`) — generalizes `CONFIG-NOISE-001` onto the `.lzi`/`.lzx` surface: fires on decorative-divider rulers (`# ========`, `# --------`, …) and on comment-dominant files (`comment_lines > semantic_lines`). Honors `# doctor:allow LZI-COMMENT-NOISE-001`. Preventive: the pilots' `.lzi` are already clean (zero dividers) — this is a ceiling on future comment drift, not a present-tense fix.

- **`DOCTOR-ALLOW-NO-REASON-001`** (advisory — `crates/lazuli_doctor/src/allow_no_reason.rs`) — fires when a `# doctor:allow <CODE>` directive lacks a `— reason "..."` tail (separator `—` / `--` / whitespace, then `reason "<text>"`). It does **not** void the opt-out — the named rule stays suppressed; this is a parallel nudge so every suppression carries an audit trail. Meta-suppress with a file-level `# doctor:allow DOCTOR-ALLOW-NO-REASON-001`. Clean on today's pilots (all current allows are reasoned) — preventive against future un-reasoned (often AI-authored) allows.

- **`LZI-COMMENT-NOISE-001`** (advisory, NEVER gates — `crates/lazuli_doctor/src/lzi_hygiene/lzi_comment_noise.rs`) — generalizes `CONFIG-NOISE-001` onto the `.lzi`/`.lzx` surface: fires on decorative-divider rulers (`# ========`, `# --------`, …) and on comment-dominant files (`comment_lines > semantic_lines`). Honors `# doctor:allow LZI-COMMENT-NOISE-001`. Preventive: the pilots' `.lzi` are already clean (zero dividers) — this is a ceiling on future comment drift, not a present-tense fix.


## Spec 0028 — the first-class `@doctor.allow(...)` waiver node

The canonical waiver is now a parsed annotation, NOT a comment:

```
@doctor.allow(LZI-FILE-SIZE-001, reason: "generated table")
feature billing
```

Written on its own line, it attaches to the construct that FOLLOWS it (or the whole file when no construct follows / it sits at column 0 before any feature). Unlike the `#` comment form, the parser captures it as structured data on `lazuli_ir::Module.doctor_allows` (`{ code, reason, scope, legacy, span }`), so a tool — or an LLM authoring `.lzi` — can tell a waiver apart from prose.

- **Grammar:** `@doctor.allow(<CODE>)` or `@doctor.allow(<CODE>, reason: "<text>")`. The reason is a single-line string literal. A malformed annotation (no parens, unterminated quote) is a hard parse error, never silently dropped.
- **Reason required for error-severity waivers:** a node waiving an error-severity rule without a `reason:` is itself flagged by `DOCTOR-ALLOW-NO-REASON-001` (which now reads the node form too).
- **Both forms are honored this window:** the legacy `# doctor:allow <CODE>` comment still suppresses. The shared string scanner `source_contains_doctor_allow` recognizes BOTH forms, so every doctor rule (advisory AND correctness, e.g. `CREATES-EMPTY-BINDINGS-001`) obeys a node waiver with no code change.
- **Migrate with `lazuli fix`:** `DOCTOR-ALLOW-LEGACY-COMMENT-001` (advisory, never gates) fires on each legacy comment waiver; `lazuli fix --rule DOCTOR-ALLOW-LEGACY-COMMENT-001` rewrites that line `# doctor:allow X — reason "y"` → `@doctor.allow(X, reason: "y")` (point-fix, idempotent, indentation preserved). Banning the comment form outright is a FUTURE spec (0029), not 0028.

- **`DOCTOR-ALLOW-LEGACY-COMMENT-001`** (advisory, NEVER gates — `crates/lazuli_doctor/src/lzi_hygiene/legacy_comment_allow_001.rs`) — nudges each legacy `# doctor:allow` comment toward the `@doctor.allow(...)` node + `lazuli fix`. Silent on the node form. Meta-suppress with `@doctor.allow(DOCTOR-ALLOW-LEGACY-COMMENT-001, reason: "...")`.
